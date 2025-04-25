use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use bytes::Bytes;
use dotenv::dotenv;
use image::io::Reader as ImageReader;
use image::{ImageOutputFormat, RgbaImage};
use imageproc::drawing::draw_text_mut;
use lazy_static::lazy_static;
use log::{error, info, warn};
use minio::s3::args::GetObjectArgs;
use minio::s3::client::Client as MinioClient;
use minio::s3::creds::StaticProvider;
use rusttype::{Font, Scale};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use url::Url;

mod config;
use config::CONFIG;

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

struct AppState {
    minio_client: MinioClient,
    font: Arc<RwLock<Option<Font<'static>>>>,
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

fn load_font() -> Result<Font<'static>, String> {
    let font_path = &CONFIG.font_path;

    info!("Attempting to load font from: {}", font_path);

    let font_data = match std::fs::read(font_path) {
        Ok(data) => {
            info!("Successfully loaded font from {}", font_path);
            data
        }
        Err(e1) => {
            warn!(
                "Failed to load font from '{}': {}. Trying alternative path.",
                font_path, e1
            );
            let alt_path = format!("./{}", font_path);
            match std::fs::read(&alt_path) {
                Ok(data) => {
                    info!(
                        "Successfully loaded font from alternative path {}",
                        alt_path
                    );
                    data
                }
                Err(e2) => {
                    error!(
                        "Failed to load font from path: {}, error: {}",
                        font_path, e1
                    );
                    error!(
                        "Failed to load font from alternative path: {}, error: {}",
                        alt_path, e2
                    );

                    #[cfg(feature = "embedded_font")]
                    {
                        info!("Using embedded font as fallback");
                        include_bytes!("../assets/DejaVuSans.ttf").to_vec()
                    }

                    #[cfg(not(feature = "embedded_font"))]
                    {
                        error!("Embedded font feature not enabled. Cannot load font.");
                        return Err(format!(
                            "Failed to load font file: {} (also tried {}). Embedded font not available.",
                            e1, e2
                        ));
                    }
                }
            }
        }
    };

    let static_font_data: &'static [u8] = Box::leak(font_data.into_boxed_slice());
    Font::try_from_bytes(static_font_data).ok_or_else(|| "Failed to parse font data".to_string())
}

async fn generate(
    payload: web::Json<GenerateRequest>,
    app_state: web::Data<AppState>,
) -> impl Responder {
    let start_time = Instant::now();
    info!(
        "Received watermarking request for: {}",
        payload.get_object_context.input_s3_url
    );

    let url_params = extract_url_params(&payload.user_request.url);
    let watermark_text = url_params
        .get("usercode")
        .cloned()
        .unwrap_or_else(|| "WATERMARK".to_string());

    if watermark_text.is_empty() {
        warn!("Received request with empty watermark text parameter.");
    }

    let input_s3_url = &payload.get_object_context.input_s3_url;
    let (bucket_name, object_name) = match parse_s3_url(input_s3_url) {
        Ok((bucket, object)) => (bucket, object),
        Err(e) => {
            error!("Failed to parse S3 URL: {}", e);
            return HttpResponse::BadRequest().json(GenerateResponse {
                status: "error".to_string(),
                message: format!("Invalid input S3 URL format: {}", e),
            });
        }
    };

    let image_bytes =
        match download_image(&app_state.minio_client, &bucket_name, &object_name).await {
            Ok(bytes) => bytes,
            Err(e) => {
                error!("Failed to download image from MinIO: {}", e);
                return HttpResponse::InternalServerError().json(GenerateResponse {
                    status: "error".to_string(),
                    message: format!("Failed to download image from MinIO: {}", e),
                });
            }
        };
    let download_duration = start_time.elapsed();

    match add_watermark(image_bytes, &watermark_text, &app_state.font).await {
        Ok(watermarked_image) => {
            let process_duration = start_time.elapsed() - download_duration;
            info!(
                "Successfully processed image with watermark '{}'. Download: {:?}, Process: {:?}",
                watermark_text, download_duration, process_duration
            );

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

fn parse_s3_url(s3_url: &str) -> Result<(String, String), String> {
    if s3_url.starts_with("s3://") {
        let parsed_url = Url::parse(s3_url).map_err(|_| "Failed to parse S3 URL".to_string())?;
        if parsed_url.scheme() != "s3" {
            return Err("URL scheme is not 's3'".to_string());
        }
        if let Some(host) = parsed_url.host_str() {
            let path_segments: Vec<&str> = parsed_url
                .path_segments()
                .map(|c| c.collect())
                .unwrap_or_default();
            if !host.is_empty() && !path_segments.is_empty() {
                Ok((host.to_string(), path_segments.join("/")))
            } else {
                Err("Invalid S3 URL format: missing bucket or object key".to_string())
            }
        } else {
            Err("Invalid S3 URL format: missing bucket".to_string())
        }
    } else {
        // Tentativa de parsing para URLs HTTP que podem conter bucket e objeto no path
        // Exemplo: http://minio.example.com/mybucket/myimage.jpg?param=value
        if let Ok(parsed_url) = Url::parse(s3_url) {
            let segments: Vec<&str> = parsed_url
                .path_segments()
                .map(|c| c.collect())
                .unwrap_or_default();
            if segments.len() >= 2 {
                let bucket = segments[0].to_string();
                let object = segments[1..].join("/");
                if !bucket.is_empty() && !object.is_empty() {
                    return Ok((bucket, object));
                }
            }
        }
        Err("S3 URL does not start with 's3://' and could not be parsed as HTTP URL with bucket/object".to_string())
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

async fn download_image(
    client: &MinioClient,
    bucket_name: &str,
    object_name: &str,
) -> Result<Bytes, String> {
    info!(
        "Downloading object '{}' from bucket '{}' in MinIO",
        object_name, bucket_name
    );

    let args_result = GetObjectArgs::new(bucket_name, object_name);

    let args = match args_result {
        Ok(args) => args,
        Err(e) => return Err(format!("Failed to create GetObjectArgs: {}", e)),
    };

    let response = client
        .get_object(&args)
        .await
        .map_err(|e| format!("Failed to get object from MinIO: {}", e))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read object bytes from MinIO: {}", e))?;

    Ok(bytes)
}

async fn add_watermark(
    image_bytes: Bytes,
    watermark_text: &str,
    watermark_font_ref: &Arc<RwLock<Option<Font<'static>>>>,
) -> Result<Vec<u8>, String> {
    let start_time = Instant::now();

    if watermark_text.is_empty() {
        warn!("Watermark text is empty, returning original image bytes.");
        return Ok(image_bytes.to_vec());
    }

    let img = ImageReader::new(Cursor::new(image_bytes))
        .with_guessed_format()
        .map_err(|e| format!("Could not guess image format: {}", e))?
        .decode()
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let width = img.width();
    let height = img.height();
    info!("Image decoded: {}x{} pixels", width, height);

    let font = {
        let maybe_font_guard = watermark_font_ref
            .read()
            .map_err(|_| "Failed to acquire read lock on font".to_string())?;
        maybe_font_guard
            .as_ref()
            .ok_or("Font not available (failed to load?)")?
            .clone()
    };

    let font_height = (height as f32 * CONFIG.font_height_ratio).max(CONFIG.font_height_min);
    let scale = Scale {
        x: font_height * CONFIG.font_width_ratio,
        y: font_height,
    };

    let watermark_color = CONFIG.watermark_color;
    let shadow_color = CONFIG.shadow_color;
    let shadow_offset_ratio = CONFIG.shadow_offset_ratio;
    let shadow_offset_x = (scale.x * shadow_offset_ratio).round() as i32;
    let shadow_offset_y = (scale.y * shadow_offset_ratio).round() as i32;

    let chars: Vec<char> = watermark_text.chars().collect();
    let char_spacing_x = scale.x * CONFIG.char_spacing_x_ratio;
    let char_spacing_y = scale.y * CONFIG.char_spacing_y_ratio;
    let chars_per_row = ((width as f32 / char_spacing_x).ceil() as usize).max(1);
    let rows = ((height as f32 / char_spacing_y).ceil() as usize).max(1) + 1;
    let global_offset_x = char_spacing_x * CONFIG.global_offset_x_ratio;
    let global_offset_y = char_spacing_y * CONFIG.global_offset_y_ratio;

    // Create a transparent layer for the watermark text and shadow
    let mut watermark_layer = RgbaImage::new(width, height);

    for row in 0..rows {
        let x_stagger = if row % 2 == 0 {
            0.0
        } else {
            char_spacing_x / 2.0
        };
        let y_pos = (row as f32 * char_spacing_y + global_offset_y).round() as i32;

        for col in 0..chars_per_row {
            let x_pos = (col as f32 * char_spacing_x + x_stagger + global_offset_x).round() as i32;
            let char_idx = (row + col) % chars.len();

            // Draw shadow on the watermark layer
            draw_text_mut(
                &mut watermark_layer,
                shadow_color,
                x_pos + shadow_offset_x,
                y_pos + shadow_offset_y,
                scale,
                &font,
                &chars[char_idx].to_string(),
            );

            // Draw watermark text on the watermark layer
            draw_text_mut(
                &mut watermark_layer,
                watermark_color,
                x_pos,
                y_pos,
                scale,
                &font,
                &chars[char_idx].to_string(),
            );
        }
    }

    // Convert the original image to RGBA if it's not already
    let mut base_image = img.into_rgba8();

    // Merge the watermark layer onto the base image using alpha blending
    for y in 0..height {
        for x in 0..width {
            let watermark_pixel = watermark_layer.get_pixel(x, y);
            let base_pixel = base_image.get_pixel_mut(x, y);

            let watermark_alpha = watermark_pixel[3] as f32 / 255.0;

            for i in 0..3 {
                base_pixel[i] = (watermark_pixel[i] as f32 * watermark_alpha
                    + base_pixel[i] as f32 * (1.0 - watermark_alpha))
                    .round() as u8;
            }
        }
    }

    let mut output_buffer = Cursor::new(Vec::new());
    base_image
        .write_to(
            &mut output_buffer,
            ImageOutputFormat::Jpeg(CONFIG.jpeg_quality),
        )
        .map_err(|e| format!("Failed to encode image to JPEG: {}", e))?;

    let encoding_duration = start_time.elapsed();
    info!(
        "Watermark added and image encoded in {:?}",
        encoding_duration
    );

    Ok(output_buffer.into_inner())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load variables from .env file
    dotenv().ok();

    env_logger::init_from_env(env_logger::Env::new().default_filter_or(CONFIG.log_level.as_str()));

    let host = &CONFIG.host;
    let port = CONFIG.port;

    let minio_endpoint = CONFIG.minio_endpoint.clone();
    let minio_access_key = CONFIG.minio_access_key.clone();
    let minio_secret_key = CONFIG.minio_secret_key.clone();
    let minio_secure = CONFIG.minio_secure;

    let credentials = StaticProvider::new(&minio_access_key, &minio_secret_key, None);
    let endpoint = minio_endpoint.parse().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to parse MinIO endpoint: {}", e),
        )
    })?;
    let provider: Option<Box<dyn minio::s3::creds::Provider + Send + Sync + 'static>> =
        Some(Box::new(credentials));
    let ssl_cert_file: Option<&std::path::Path> = None;
    let ignore_cert_check: Option<bool> = Some(!minio_secure);

    info!("Creating MinIO client...");
    let minio_client =
        minio::s3::client::Client::new(endpoint, provider, ssl_cert_file, ignore_cert_check)
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create MinIO client: {}", e),
                )
            })?;

    info!("Preloading font...");
    let font_ref_clone = Arc::clone(&WATERMARK_FONT);
    {
        let font_guard = WATERMARK_FONT.read().expect("Font RwLock poisoned");
        match *font_guard {
            Some(_) => info!("Font loaded successfully at startup."),
            None => error!("Font is None after attempted loading. Watermarking will fail!"),
        }
    }

    info!("Starting server on {}:{}...", host, port);

    let workers = if CONFIG.workers == 0 {
        num_cpus::get()
    } else {
        CONFIG.workers
    };
    info!("Using {} worker threads", workers);

    let app_state = web::Data::new(AppState {
        minio_client,
        font: font_ref_clone,
    });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/", web::post().to(generate))
            .route(
                "/",
                web::get().to(|| async { HttpResponse::Ok().body("OK") }),
            )
            .route(
                "/health/",
                web::get().to(|| async { HttpResponse::Ok().body("OK") }),
            )
    })
    .workers(workers)
    .bind((host.as_str(), port))?
    .run()
    .await
}
