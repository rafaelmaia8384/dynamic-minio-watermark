# Minio Watermarker

Service for adding watermarks to images stored in MinIO.

To configure Minio to use lambda functions, see: (https://min.io/docs/minio/linux/developers/transforms-with-object-lambda.html)

## Configuration

The project uses a `.env` file for configuration. You can copy the `.env.example` file (if it exists) and customize it according to your needs.

### Available Environment Variables

#### Server Settings
- `HOST` - Address to bind the server (default: "0.0.0.0")
- `PORT` - Server port (default: 3333)
- `WORKERS` - Number of workers (threads). Use 0 to use the number of available CPUs (default: 0)

#### Font Settings
- `FONT_PATH` - Path to the TTF font (default: "assets/DejaVuSans.ttf")
- `FONT_HEIGHT_RATIO` - Font height as a fraction of image height (default: 0.10)
- `FONT_HEIGHT_MIN` - Minimum font height in pixels (default: 10.0)
- `FONT_WIDTH_RATIO` - Font width to height ratio (default: 0.6)

#### Color Settings (values from 0-255)
- `WATERMARK_COLOR_R` - R component of watermark color (default: 255)
- `WATERMARK_COLOR_G` - G component of watermark color (default: 255)
- `WATERMARK_COLOR_B` - B component of watermark color (default: 255)
- `WATERMARK_COLOR_A` - Alpha component of watermark color (default: 46, ~18% opacity)

- `SHADOW_COLOR_R` - R component of shadow color (default: 0)
- `SHADOW_COLOR_G` - G component of shadow color (default: 0)
- `SHADOW_COLOR_B` - B component of shadow color (default: 0)
- `SHADOW_COLOR_A` - Alpha component of shadow color (default: 46, ~18% opacity)

#### Layout Settings
- `SHADOW_OFFSET_RATIO` - Shadow offset as a fraction of font size (default: 0.065)
- `CHAR_SPACING_X_RATIO` - Horizontal spacing as a fraction of font width (default: 1.1)
- `CHAR_SPACING_Y_RATIO` - Vertical spacing as a fraction of font height (default: 0.4)
- `GLOBAL_OFFSET_X_RATIO` - Global horizontal offset as a fraction of spacing (default: -0.5)
- `GLOBAL_OFFSET_Y_RATIO` - Global vertical offset as a fraction of spacing (default: -1.0)

#### HTTP Settings
- `HTTP_POOL_MAX_IDLE` - Maximum number of idle connections per host (default: 10)
- `HTTP_CONNECT_TIMEOUT` - Connection timeout in seconds (default: 10)
- `HTTP_REQUEST_TIMEOUT` - Overall request timeout in seconds (default: 60)

#### Image Quality Settings
- `JPEG_QUALITY` - Output JPEG image quality (0-100) (default: 90)

## Compiling with Embedded Font

To compile the project with an embedded font (useful for containers or environments without filesystem access):

```bash
cargo build --release --features embedded_font
```

## Usage

Start the server:

```bash
cargo run --release
```

Or configure and run using the .env file:

```bash
# Create or modify the .env file with your settings
echo "PORT=8080" >> .env

# Run the application
cargo run --release
```

## Docker Deployment

The project includes Docker and Docker Compose configuration for easy deployment.

### Using Docker Compose

1. Configure your environment variables (optional):

```bash
# Copy example configuration
cp .env.example .env

# Edit the configuration file
nano .env
```

2. Build and start the service:

```bash
docker-compose up -d
```

3. Check the logs:

```bash
docker-compose logs -f
```

4. Stop the service:

```bash
docker-compose down
```

### Custom Configuration

The docker-compose.yml file is set up to use environment variables with sensible defaults. You can:

1. Modify the .env file with your settings (recommended)
2. Override specific variables in the command line:

```bash
PORT=8080 WORKERS=4 WATERMARK_COLOR_A=128 docker-compose up -d
```

### Production Considerations

- The Dockerfile builds the application with the `embedded_font` feature enabled for reliability
- SSL/TLS termination should be handled by a reverse proxy like Nginx or Traefik
- For high availability, consider deploying multiple instances behind a load balancer

The service will be available at:
- Main endpoint: `/generate/`
- Health check: `/health/` 