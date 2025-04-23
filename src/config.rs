use image::Rgba;
use lazy_static::lazy_static;
use log::warn;
use std::env;
use std::fmt::Debug;

lazy_static! {
    pub static ref CONFIG: Config = Config::from_env();
}

pub struct Config {
    // Server settings
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub log_level: String,

    // Font settings
    pub font_path: String,
    pub font_height_ratio: f32,
    pub font_height_min: f32,
    pub font_width_ratio: f32,

    // Color settings
    pub watermark_color: Rgba<u8>,
    pub shadow_color: Rgba<u8>,

    // Layout settings
    pub shadow_offset_ratio: f32,
    pub char_spacing_x_ratio: f32,
    pub char_spacing_y_ratio: f32,
    pub global_offset_x_ratio: f32,
    pub global_offset_y_ratio: f32,

    // Image quality settings
    pub jpeg_quality: u8,

    // Minio settings
    pub minio_endpoint: String,
    pub minio_access_key: String,
    pub minio_secret_key: String,
    pub minio_secure: bool,
}

impl Config {
    pub fn from_env() -> Self {
        // Helper function to get numeric settings with default values
        fn get_numeric<T: std::str::FromStr + Debug>(key: &str, default: T) -> T {
            match env::var(key) {
                Ok(val) => match val.parse::<T>() {
                    Ok(parsed) => parsed,
                    Err(_) => {
                        warn!("Invalid value for {}, using default: {:?}", key, default);
                        default
                    }
                },
                Err(_) => default,
            }
        }

        // Reading server settings
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = get_numeric("PORT", 3333);
        let workers = get_numeric("WORKERS", 0);
        let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "error".to_string());

        // Reading font settings
        let font_path =
            env::var("FONT_PATH").unwrap_or_else(|_| "assets/DejaVuSans.ttf".to_string());
        let font_height_ratio = get_numeric("FONT_HEIGHT_RATIO", 0.10);
        let font_height_min = get_numeric("FONT_HEIGHT_MIN", 10.0);
        let font_width_ratio = get_numeric("FONT_WIDTH_RATIO", 0.6);

        // Reading color settings
        let watermark_color = Rgba([
            get_numeric("WATERMARK_COLOR_R", 255),
            get_numeric("WATERMARK_COLOR_G", 255),
            get_numeric("WATERMARK_COLOR_B", 255),
            get_numeric("WATERMARK_COLOR_A", 46),
        ]);

        let shadow_color = Rgba([
            get_numeric("SHADOW_COLOR_R", 0),
            get_numeric("SHADOW_COLOR_G", 0),
            get_numeric("SHADOW_COLOR_B", 0),
            get_numeric("SHADOW_COLOR_A", 46),
        ]);

        // Reading layout settings
        let shadow_offset_ratio = get_numeric("SHADOW_OFFSET_RATIO", 0.065);
        let char_spacing_x_ratio = get_numeric("CHAR_SPACING_X_RATIO", 1.1);
        let char_spacing_y_ratio = get_numeric("CHAR_SPACING_Y_RATIO", 0.4);
        let global_offset_x_ratio = get_numeric("GLOBAL_OFFSET_X_RATIO", -0.5);
        let global_offset_y_ratio = get_numeric("GLOBAL_OFFSET_Y_RATIO", -1.2);

        // Reading image quality settings
        let jpeg_quality = get_numeric("JPEG_QUALITY", 90);

        // Reading Minio settings
        let minio_endpoint = env::var("MINIO_ENDPOINT").expect("MINIO_ENDPOINT must be set");
        let minio_access_key = env::var("MINIO_ACCESS_KEY").expect("MINIO_ACCESS_KEY must be set");
        let minio_secret_key = env::var("MINIO_SECRET_KEY").expect("MINIO_SECRET_KEY must be set");
        let minio_secure = env::var("MINIO_SECURE").expect("MINIO_SECURE must be set");
        let minio_secure = minio_secure.parse::<bool>().unwrap_or(false);
        Self {
            host,
            port,
            workers,
            log_level,
            font_path,
            font_height_ratio,
            font_height_min,
            font_width_ratio,
            watermark_color,
            shadow_color,
            shadow_offset_ratio,
            char_spacing_x_ratio,
            char_spacing_y_ratio,
            global_offset_x_ratio,
            global_offset_y_ratio,
            jpeg_quality,
            minio_endpoint,
            minio_access_key,
            minio_secret_key,
            minio_secure,
        }
    }
}
