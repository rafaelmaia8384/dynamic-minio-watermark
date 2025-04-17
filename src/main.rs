use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use bytes::Bytes;
use image::ImageOutputFormat;
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
struct UserRequest {
    url: String,
}

#[derive(Debug, Deserialize)]
struct GenerateRequest {
    #[serde(rename = "getObjectContext")]
    get_object_context: ObjectContext,
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

    // Convert to RGBA8 if not already
    let mut watermarked = img.to_rgba8();

    // Get the font
    let font = {
        let font_lock = WATERMARK_FONT
            .read()
            .map_err(|_| "Failed to acquire read lock on font".to_string())?;
        font_lock.as_ref().ok_or("Font not loaded")?.clone()
    };

    // Calculate font size (10% of image height)
    let font_height = (watermarked.height() as f32) * 0.10;
    let scale = Scale {
        x: font_height * 0.6,
        y: font_height,
    };

    // Watermark and shadow settings
    let watermark_color = [255, 255, 255];
    let shadow_color = [0, 0, 0];
    let watermark_opacity = 0.18;
    let shadow_opacity = 0.18;
    let shadow_offset_ratio = 0.065;

    // Calculate shadow offset (5% of character size)
    let shadow_offset_x = (scale.x * shadow_offset_ratio) as i32;
    let shadow_offset_y = (scale.y * shadow_offset_ratio) as i32;

    // Prepare watermark pattern
    let chars: Vec<char> = watermark_text.chars().collect();
    if chars.is_empty() {
        return Err("Watermark text cannot be empty".to_string());
    }

    // Calculate spacing
    let char_spacing_x = scale.x * 1.1;
    let char_spacing_y = scale.y * 0.4;
    let chars_per_row = (watermarked.width() as f32 / char_spacing_x).ceil() as usize;
    let rows = (watermarked.height() as f32 / char_spacing_y).ceil() as usize;

    // Apply global offset
    let global_offset_x = -char_spacing_x * 0.5;
    let global_offset_y = -char_spacing_y;

    // Create a temporary image for character drawing
    let char_width = (scale.x * 1.5) as u32;
    let char_height = (scale.y * 1.5) as u32;
    let mut char_buffer = image::RgbaImage::new(char_width, char_height);

    // Draw watermark
    for row in 0..rows {
        let x_offset = if row % 2 == 0 {
            0.0
        } else {
            char_spacing_x / 2.0
        };
        let y_pos = row as f32 * char_spacing_y + global_offset_y;

        for col in 0..chars_per_row {
            let x_pos = col as f32 * char_spacing_x + x_offset + global_offset_x;
            let char_idx = (row + col) % chars.len();

            if x_pos >= 0.0
                && y_pos >= 0.0
                && x_pos < watermarked.width() as f32
                && y_pos < watermarked.height() as f32
            {
                // Clear the character buffer
                for pixel in char_buffer.pixels_mut() {
                    *pixel = image::Rgba([0, 0, 0, 0]);
                }

                // Draw the shadow first (offset position)
                draw_text_mut(
                    &mut char_buffer,
                    image::Rgba([255, 255, 255, 255]), // White with full opacity
                    shadow_offset_x,
                    shadow_offset_y,
                    scale,
                    &font,
                    &chars[char_idx].to_string(),
                );

                // Blend the shadow onto the main image
                for (bx, by, pixel) in char_buffer.enumerate_pixels() {
                    let wx = x_pos as u32 + bx;
                    let wy = y_pos as u32 + by;

                    if wx < watermarked.width() && wy < watermarked.height() && pixel[3] > 0 {
                        let bg_pixel = watermarked.get_pixel(wx, wy);

                        // Blend shadow color with specified opacity
                        let r = (bg_pixel[0] as f32 * (1.0 - shadow_opacity)
                            + shadow_color[0] as f32 * shadow_opacity)
                            as u8;
                        let g = (bg_pixel[1] as f32 * (1.0 - shadow_opacity)
                            + shadow_color[1] as f32 * shadow_opacity)
                            as u8;
                        let b = (bg_pixel[2] as f32 * (1.0 - shadow_opacity)
                            + shadow_color[2] as f32 * shadow_opacity)
                            as u8;

                        watermarked.put_pixel(wx, wy, image::Rgba([r, g, b, 255]));
                    }
                }

                // Clear the character buffer again
                for pixel in char_buffer.pixels_mut() {
                    *pixel = image::Rgba([0, 0, 0, 0]);
                }

                // Draw the main character (original position)
                draw_text_mut(
                    &mut char_buffer,
                    image::Rgba([255, 255, 255, 255]), // White with full opacity
                    0,
                    0,
                    scale,
                    &font,
                    &chars[char_idx].to_string(),
                );

                // Blend the main character onto the main image
                for (bx, by, pixel) in char_buffer.enumerate_pixels() {
                    let wx = x_pos as u32 + bx;
                    let wy = y_pos as u32 + by;

                    if wx < watermarked.width() && wy < watermarked.height() && pixel[3] > 0 {
                        let bg_pixel = watermarked.get_pixel(wx, wy);

                        // Blend watermark color with specified opacity
                        let r = (bg_pixel[0] as f32 * (1.0 - watermark_opacity)
                            + watermark_color[0] as f32 * watermark_opacity)
                            as u8;
                        let g = (bg_pixel[1] as f32 * (1.0 - watermark_opacity)
                            + watermark_color[1] as f32 * watermark_opacity)
                            as u8;
                        let b = (bg_pixel[2] as f32 * (1.0 - watermark_opacity)
                            + watermark_color[2] as f32 * watermark_opacity)
                            as u8;

                        watermarked.put_pixel(wx, wy, image::Rgba([r, g, b, 255]));
                    }
                }
            }
        }
    }

    // Save as JPEG with quality 90
    let mut buffer = Vec::new();
    watermarked
        .write_to(&mut Cursor::new(&mut buffer), ImageOutputFormat::Jpeg(90))
        .map_err(|e| format!("Failed to encode image: {}", e))?;

    Ok(buffer)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Get host and port from environment variables or use defaults
    let host = "0.0.0.0".to_string();
    let port = 3333;

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
