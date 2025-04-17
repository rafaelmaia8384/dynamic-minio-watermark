use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use bytes::Bytes;
use image::{GenericImageView, ImageOutputFormat};
use imageproc::drawing::draw_text_mut;
use lazy_static::lazy_static;
use log::{error, info};
use reqwest::Client;
use rusttype::{Font, Scale};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::RwLock;

lazy_static! {
    static ref WATERMARK_FONT: Arc<RwLock<Option<Font<'static>>>> = {
        let font_result = load_font();
        match font_result {
            Ok(font) => Arc::new(RwLock::new(Some(font))),
            Err(e) => {
                error!("Failed to load font at startup: {}", e);
                Arc::new(RwLock::new(None))
            }
        }
    };
}

// Estrutura compartilhada em toda aplicação
struct AppState {
    client: Client,
}

#[derive(Debug, Deserialize)]
struct ObjectContext {
    #[serde(rename = "inputS3Url")]
    input_s3_url: String,
    #[serde(rename = "outputRoute")]
    output_route: String,
    #[serde(rename = "outputToken")]
    output_token: String,
}

#[derive(Debug, Deserialize)]
struct UserIdentity {
    #[serde(rename = "accessKeyId")]
    access_key_id: String,
    #[serde(rename = "principalId")]
    principal_id: String,
    #[serde(rename = "type")]
    identity_type: String,
}

#[derive(Debug, Deserialize)]
struct UserRequestHeaders {
    #[serde(default, rename = "Accept")]
    accept: Vec<String>,
    #[serde(default, rename = "Accept-Encoding")]
    accept_encoding: Vec<String>,
    #[serde(default, rename = "Connection")]
    connection: Vec<String>,
    #[serde(default, rename = "User-Agent")]
    user_agent: Vec<String>,
    #[serde(default, rename = "X-Amz-Signature-Age")]
    x_amz_signature_age: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UserRequest {
    headers: UserRequestHeaders,
    url: String,
}

#[derive(Debug, Deserialize)]
struct GenerateRequest {
    #[serde(rename = "getObjectContext")]
    get_object_context: ObjectContext,
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    #[serde(rename = "userIdentity")]
    user_identity: UserIdentity,
    #[serde(rename = "userRequest")]
    user_request: UserRequest,
}

#[derive(Debug, Serialize)]
struct GenerateResponse {
    status: String,
    message: String,
}

// Load font from file or embedded resource
fn load_font() -> Result<Font<'static>, String> {
    // Try to get font path from environment variable
    let font_path =
        std::env::var("FONT_PATH").unwrap_or_else(|_| "assets/DejaVuSans.ttf".to_string());

    info!("Loading font from: {}", font_path);

    // Prepare font for watermark - try to load from specified or default location
    let font_data = match std::fs::read(&font_path) {
        Ok(data) => data,
        Err(e1) => {
            // If that fails, try the relative path from a different location
            let alt_path = format!("./{}", font_path);
            info!("Trying alternative font path: {}", alt_path);
            match std::fs::read(&alt_path) {
                Ok(data) => data,
                Err(e2) => {
                    // If that also fails, try embedded font as last resort
                    error!(
                        "Failed to load font from path: {}, error: {}",
                        font_path, e1
                    );
                    error!(
                        "Failed to load font from alternative path: {}, error: {}",
                        alt_path, e2
                    );

                    // Try to use the embedded font
                    #[cfg(feature = "embedded_font")]
                    {
                        info!("Using embedded font as fallback");
                        include_bytes!("../assets/DejaVuSans.ttf").to_vec()
                    }

                    #[cfg(not(feature = "embedded_font"))]
                    return Err(format!(
                        "Failed to load font file: {} (also tried {})",
                        e1, e2
                    ));
                }
            }
        }
    };

    // Convert the loaded font data to a static lifetime
    let static_font_data: &'static [u8] = Box::leak(font_data.into_boxed_slice());

    Font::try_from_bytes(static_font_data).ok_or_else(|| "Failed to parse font data".to_string())
}

async fn generate(
    payload: web::Json<GenerateRequest>,
    app_state: web::Data<AppState>,
) -> impl Responder {
    info!(
        "Received watermarking request: {:?}",
        payload.get_object_context.input_s3_url
    );

    // Extract watermark parameter from URL if present
    let url_params = extract_url_params(&payload.user_request.url);
    let watermark_text = url_params
        .get("usercode")
        .cloned()
        .unwrap_or_else(|| "WATERMARK".to_string());

    // Step 1: Download the image from input_s3_url
    match download_image(&app_state.client, &payload.get_object_context.input_s3_url).await {
        Ok(image_bytes) => {
            // Step 2: Add watermark to the image
            match add_watermark(image_bytes, &watermark_text).await {
                Ok(watermarked_image) => {
                    info!("Successfully processed image with watermark");

                    // Return the watermarked image directly in the response with the required headers
                    HttpResponse::Ok()
                        .content_type("image/jpeg")
                        .append_header((
                            "x-amz-request-route",
                            payload.get_object_context.output_route.clone(),
                        ))
                        .append_header((
                            "x-amz-request-token",
                            payload.get_object_context.output_token.clone(),
                        ))
                        .body(watermarked_image)
                }
                Err(e) => {
                    error!("Failed to add watermark: {}", e);
                    HttpResponse::InternalServerError().json(GenerateResponse {
                        status: "error".to_string(),
                        message: format!("Failed to add watermark: {}", e),
                    })
                }
            }
        }
        Err(e) => {
            error!("Failed to download image: {}", e);
            HttpResponse::InternalServerError().json(GenerateResponse {
                status: "error".to_string(),
                message: format!("Failed to download image: {}", e),
            })
        }
    }
}

fn extract_url_params(url: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();

    if let Some(query_str) = url.split('?').nth(1) {
        for pair in query_str.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                params.insert(key.to_string(), value.to_string());
            }
        }
    }

    params
}

async fn download_image(client: &Client, url: &str) -> Result<Bytes, String> {
    client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read bytes: {}", e))
}

async fn add_watermark(image_bytes: Bytes, watermark_text: &str) -> Result<Vec<u8>, String> {
    // Load image from bytes
    let img = image::load_from_memory(&image_bytes)
        .map_err(|e| format!("Failed to load image: {}", e))?;

    // Get image dimensions
    let (width, height) = img.dimensions();

    // Create a mutable copy of the image
    let mut watermarked = img.to_rgba8();

    // Get the font from the lazy static
    let font = {
        let font_lock = WATERMARK_FONT
            .read()
            .map_err(|_| "Failed to acquire read lock on font".to_string())?;
        match &*font_lock {
            Some(font) => font.clone(),
            None => {
                // If font failed to load at startup, try to load it again
                drop(font_lock); // Release the read lock before acquiring write lock
                let mut font_lock = WATERMARK_FONT
                    .write()
                    .map_err(|_| "Failed to acquire write lock on font".to_string())?;
                if let None = *font_lock {
                    // Try to load again
                    *font_lock = Some(load_font()?);
                }
                font_lock.as_ref().unwrap().clone()
            }
        }
    };

    // Calculate font size to be 10% of image height (increased from 5%)
    let font_height = (height as f32) * 0.10;
    let scale = Scale {
        x: font_height * 0.6, // Make width slightly smaller for a denser pattern
        y: font_height,
    };

    // Semi-transparent gray for better visibility on various backgrounds
    let text_color = image::Rgba([128, 128, 128, 80]); // Semi-transparent gray

    // Create a continuous pattern across the entire image

    // First, convert the watermark text to a repeating character array
    let chars: Vec<char> = watermark_text.chars().collect();
    if chars.is_empty() {
        return Err("Watermark text cannot be empty".to_string());
    }

    // Calculate the horizontal and vertical spacing between characters
    let char_spacing_x = scale.x * 0.9; // Slight overlap between characters (adjusted for larger font)
    let char_spacing_y = scale.y * 1.1; // Slight spacing between rows (reduced for larger font)

    // Calculate how many characters we can fit horizontally and vertically
    let chars_per_row = (width as f32 / char_spacing_x).ceil() as usize;
    let rows = (height as f32 / char_spacing_y).ceil() as usize;

    // Create two diagonal patterns (one from left-top to right-bottom
    // and another from right-top to left-bottom)
    for pattern in 0..2 {
        // Starting offset for the second pattern (right-to-left)
        let y_offset = if pattern == 0 {
            0.0
        } else {
            char_spacing_y / 2.0
        };

        // Draw pattern of single characters
        for row in 0..rows {
            let y_pos = row as f32 * char_spacing_y + y_offset;

            for col in 0..chars_per_row {
                // For the second pattern, reverse the direction
                let x_pos = if pattern == 0 {
                    col as f32 * char_spacing_x
                } else {
                    width as f32 - col as f32 * char_spacing_x
                };

                // Get character (cycling through the watermark text)
                let char_idx = (row + col) % chars.len();
                let c = chars[char_idx];

                // Draw this character
                if x_pos >= 0.0 && y_pos >= 0.0 && x_pos < width as f32 && y_pos < height as f32 {
                    let x = x_pos as i32;
                    let y = y_pos as i32;

                    draw_character_mut(&mut watermarked, text_color, x, y, scale, &font, c);
                }
            }
        }
    }

    // Convert back to the original format - use a more efficient buffer
    let mut buffer = Vec::with_capacity((width * height * 3) as usize); // Preallocate buffer
    let mut cursor = Cursor::new(&mut buffer);
    watermarked
        .write_to(&mut cursor, ImageOutputFormat::Jpeg(90))
        .map_err(|e| format!("Failed to encode image: {}", e))?;

    Ok(buffer)
}

// Helper function to draw a single character
fn draw_character_mut(
    image: &mut image::RgbaImage,
    color: image::Rgba<u8>,
    x: i32,
    y: i32,
    scale: Scale,
    font: &Font,
    character: char,
) {
    // We'll use the built-in draw_text_mut function but with only one character
    let char_str = character.to_string();
    draw_text_mut(image, color, x, y, scale, font, &char_str);
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Load environment variables (optional)
    if let Err(e) = dotenv::dotenv() {
        println!("Warning: Error loading .env file: {}", e);
    }

    // Get host and port from environment variables or use defaults
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3333);

    // Initialize shared HTTP client
    let client = Client::builder()
        .pool_max_idle_per_host(10) // Reuse connections
        .build()
        .expect("Failed to create HTTP client");

    // Preload font at startup
    info!("Preloading font...");
    match &*WATERMARK_FONT.read().unwrap() {
        Some(_) => info!("Font loaded successfully at startup"),
        None => error!("Failed to load font at startup, will try again on first request"),
    }

    info!("Starting server on {}:{}...", host, port);

    // Create app state with shared HTTP client
    let app_state = web::Data::new(AppState { client });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/generate/", web::post().to(generate))
    })
    .workers(num_cpus::get()) // Use optimal number of workers based on CPU cores
    .bind(format!("{}:{}", host, port))?
    .run()
    .await
}
