use actix_web::{http::header, web, App, HttpResponse, HttpServer, Responder};
use bytes::Bytes;
use image::{GenericImageView, ImageOutputFormat};
use imageproc::drawing::draw_text_mut;
use log::{error, info};
use reqwest::Client;
use rusttype::{Font, Scale};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;

#[derive(Debug, Deserialize)]
struct ObjectContext {
    inputS3Url: String,
    outputRoute: String,
    outputToken: String,
}

#[derive(Debug, Deserialize)]
struct UserIdentity {
    accessKeyId: String,
    principalId: String,
    #[serde(rename = "type")]
    identity_type: String,
}

#[derive(Debug, Deserialize)]
struct UserRequestHeaders {
    #[serde(default)]
    Accept: Vec<String>,
    #[serde(default, rename = "Accept-Encoding")]
    Accept_Encoding: Vec<String>,
    #[serde(default)]
    Connection: Vec<String>,
    #[serde(default, rename = "User-Agent")]
    User_Agent: Vec<String>,
    #[serde(default, rename = "X-Amz-Signature-Age")]
    X_Amz_Signature_Age: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UserRequest {
    headers: UserRequestHeaders,
    url: String,
}

#[derive(Debug, Deserialize)]
struct GenerateRequest {
    getObjectContext: ObjectContext,
    protocolVersion: String,
    userIdentity: UserIdentity,
    userRequest: UserRequest,
}

#[derive(Debug, Serialize)]
struct GenerateResponse {
    status: String,
    message: String,
}

async fn generate(payload: web::Json<GenerateRequest>) -> impl Responder {
    info!(
        "Received watermarking request: {:?}",
        payload.getObjectContext.inputS3Url
    );

    let client = Client::new();

    // Extract watermark parameter from URL if present
    let url_params = extract_url_params(&payload.userRequest.url);
    let watermark_text = url_params
        .get("usercode")
        .cloned()
        .unwrap_or_else(|| "WATERMARK".to_string());

    // Step 1: Download the image from inputS3Url
    match download_image(&client, &payload.getObjectContext.inputS3Url).await {
        Ok(image_bytes) => {
            // Step 2: Add watermark to the image
            match add_watermark(image_bytes, &watermark_text).await {
                Ok(watermarked_image) => {
                    // Step 3: Send the watermarked image back to the output URL
                    match send_processed_image(
                        &client,
                        &payload.getObjectContext.outputRoute,
                        &payload.getObjectContext.outputToken,
                        watermarked_image,
                    )
                    .await
                    {
                        Ok(_) => {
                            info!("Successfully processed and returned watermarked image");
                            HttpResponse::Ok().json(GenerateResponse {
                                status: "success".to_string(),
                                message: "Image successfully watermarked".to_string(),
                            })
                        }
                        Err(e) => {
                            error!("Failed to send processed image: {}", e);
                            HttpResponse::InternalServerError().json(GenerateResponse {
                                status: "error".to_string(),
                                message: format!("Failed to send processed image: {}", e),
                            })
                        }
                    }
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

    // Prepare font for watermark
    let font_data = include_bytes!("../assets/DejaVuSans.ttf");
    let font = Font::try_from_bytes(font_data as &[u8])
        .ok_or_else(|| "Failed to load font".to_string())?;

    // Set text scale (size)
    let scale = Scale {
        x: (width as f32) / 10.0,
        y: (height as f32) / 20.0,
    };

    // Draw text at 45-degree angle across the image
    let text_color = image::Rgba([255, 0, 0, 128]); // Semi-transparent red

    // Position text in the center
    let text_width_approx = (watermark_text.len() as f32 * scale.x) / 4.0;
    let x = ((width as f32) / 2.0 - text_width_approx) as i32;
    let y = ((height as f32) / 2.0) as i32;

    draw_text_mut(
        &mut watermarked,
        text_color,
        x,
        y,
        scale,
        &font,
        watermark_text,
    );

    // Convert back to the original format
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    watermarked
        .write_to(&mut cursor, ImageOutputFormat::Jpeg(90))
        .map_err(|e| format!("Failed to encode image: {}", e))?;

    Ok(buffer)
}

async fn send_processed_image(
    client: &Client,
    output_route: &str,
    output_token: &str,
    image_data: Vec<u8>,
) -> Result<(), String> {
    // Create the WriteGetObjectResponse request
    let response = client
        .post(format!("http://localhost:9000/{}", output_route))
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header("x-amz-request-route", output_route)
        .header("x-amz-request-token", output_token)
        .body(image_data)
        .send()
        .await
        .map_err(|e| format!("Failed to send response: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Error response from server: {}", response.status()));
    }

    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Load environment variables (optional)
    if let Err(e) = dotenv::dotenv() {
        println!("Warning: Error loading .env file: {}", e);
    }

    let host = "127.0.0.1";
    let port = 8080;

    info!("Starting server on port {}...", port);

    HttpServer::new(|| App::new().route("/generate/", web::post().to(generate)))
        .bind(format!("{}:{}", host, port))?
        .run()
        .await
}
