use image::Rgba;
use lazy_static::lazy_static;
use log::warn;
use std::env;
use std::fmt::Debug;

lazy_static! {
    pub static ref CONFIG: Config = Config::from_env();
}

pub struct Config {
    // Configurações do servidor
    pub host: String,
    pub port: u16,
    pub workers: usize,

    // Configurações de fonte
    pub font_path: String,
    pub font_height_ratio: f32,
    pub font_height_min: f32,
    pub font_width_ratio: f32,

    // Configurações de cores
    pub watermark_color: Rgba<u8>,
    pub shadow_color: Rgba<u8>,

    // Configurações de layout
    pub shadow_offset_ratio: f32,
    pub char_spacing_x_ratio: f32,
    pub char_spacing_y_ratio: f32,
    pub global_offset_x_ratio: f32,
    pub global_offset_y_ratio: f32,

    // Configurações HTTP
    pub http_pool_max_idle: usize,
    pub http_connect_timeout: u64,
    pub http_request_timeout: u64,

    // Configuração de qualidade da imagem
    pub jpeg_quality: u8,
}

impl Config {
    pub fn from_env() -> Self {
        // Função auxiliar para obter configurações numéricas com valores padrão
        fn get_numeric<T: std::str::FromStr + Debug>(key: &str, default: T) -> T {
            match env::var(key) {
                Ok(val) => match val.parse::<T>() {
                    Ok(parsed) => parsed,
                    Err(_) => {
                        warn!("Valor inválido para {}, usando padrão: {:?}", key, default);
                        default
                    }
                },
                Err(_) => default,
            }
        }

        // Leitura das configurações do servidor
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = get_numeric("PORT", 3333);
        let workers = get_numeric("WORKERS", 0);

        // Leitura das configurações de fonte
        let font_path =
            env::var("FONT_PATH").unwrap_or_else(|_| "assets/DejaVuSans.ttf".to_string());
        let font_height_ratio = get_numeric("FONT_HEIGHT_RATIO", 0.10);
        let font_height_min = get_numeric("FONT_HEIGHT_MIN", 10.0);
        let font_width_ratio = get_numeric("FONT_WIDTH_RATIO", 0.6);

        // Leitura das configurações de cores
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

        // Leitura das configurações de layout
        let shadow_offset_ratio = get_numeric("SHADOW_OFFSET_RATIO", 0.065);
        let char_spacing_x_ratio = get_numeric("CHAR_SPACING_X_RATIO", 1.1);
        let char_spacing_y_ratio = get_numeric("CHAR_SPACING_Y_RATIO", 0.4);
        let global_offset_x_ratio = get_numeric("GLOBAL_OFFSET_X_RATIO", -0.5);
        let global_offset_y_ratio = get_numeric("GLOBAL_OFFSET_Y_RATIO", -1.0);

        // Leitura das configurações HTTP
        let http_pool_max_idle = get_numeric("HTTP_POOL_MAX_IDLE", 10);
        let http_connect_timeout = get_numeric("HTTP_CONNECT_TIMEOUT", 10);
        let http_request_timeout = get_numeric("HTTP_REQUEST_TIMEOUT", 60);

        // Leitura da configuração de qualidade da imagem
        let jpeg_quality = get_numeric("JPEG_QUALITY", 90);

        Self {
            host,
            port,
            workers,
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
            http_pool_max_idle,
            http_connect_timeout,
            http_request_timeout,
            jpeg_quality,
        }
    }
}
