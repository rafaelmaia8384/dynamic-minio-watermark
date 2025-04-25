# Dynamic MinIO Watermark

![Result watermark](assets/example.jpg)

## Service for Dynamically Adding Watermarks to Images Stored in MinIO

## This service is designed to dynamically apply watermarks to images stored in a MinIO object storage system. It leverages MinIO's native Lambda functions, which allow server-side transformations to be triggered on-the-fly, without the need to modify the original image or store multiple versions of it.

Whenever an image is requested through a specially crafted URL or via a predefined Lambda trigger, this service intercepts the request and applies a watermark in real-time, based on customizable rules or predefined templates.

This approach ensures:

- Storage efficiency, as only the original image is stored.

- Flexibility, allowing different watermarks per use case (e.g., user-based, time-based, or branding-specific).

- Performance, since the transformation happens at the edge, close to the storage layer.


Ideal for systems that require secure image distribution, brand protection, or user-specific watermarking, such as media platforms, digital asset management systems, or document repositories.

To configure Minio to use lambda functions, see: (https://min.io/docs/minio/linux/developers/transforms-with-object-lambda.html)

## Configuration

The project uses a `.env` file for configuration. You can copy the `.env.example` file (if it exists) and customize it according to your needs.

### Available Environment Variables

#### Server Settings
- `HOST` - Address to bind the server (default: "0.0.0.0")
- `PORT` - Server port (default: 3333)
- `WORKERS` - Number of workers (threads). Use 0 to use the number of available CPUs (default: 0)
- `LOG_LEVEL` - {debug,info,error}

#### Minio Settings
-   `MINIO_ENDPOINT`: The full address of your MinIO server, including the port. **Example:** `http://localhost:9000` or `https://s3.example.com`. Ensure that the scheme in `MINIO_ENDPOINT` matches the `MINIO_SECURE` setting (`http://` for `false`, `https://` for `true`).
-   `MINIO_ACCESS_KEY`: The access key (username) to authenticate with your MinIO server.
-   `MINIO_SECRET_KEY`: The secret key (password) corresponding to your MinIO access key.
-   `MINIO_SECURE`: A boolean value (`true` or `false`) indicating whether the connection to MinIO should use HTTPS (`true`) or HTTP (`false`). Ensure the scheme in `MINIO_ENDPOINT` aligns with this setting.


#### Font Settings
- `FONT_PATH` - Path to the TTF font (default: "assets/DejaVuSans.ttf")
- `FONT_HEIGHT_RATIO` - Font height as a fraction of image height (default: 0.10)
- `FONT_HEIGHT_MIN` - Minimum font height in pixels (default: 40.0)
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
- `GLOBAL_OFFSET_Y_RATIO` - Global vertical offset as a fraction of spacing (default: -1.2)

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

### MinIO Lambda Request

When configuring a MinIO Object Lambda function, you need to point it to this service's endpoint (`http://<your-service-host>:<port>/`). MinIO will send a `POST` request with a JSON payload containing details about the original object request.

The service expects the following JSON structure in the request body:

```json
{
  "getObjectContext": {
    "inputS3Url": "PRESIGNED_URL_FOR_ORIGINAL_IMAGE",
    "outputRoute": "MINIO_OUTPUT_ROUTE",
    "outputToken": "MINIO_OUTPUT_TOKEN"
  },
  "userRequest": {
    "url": "ORIGINAL_USER_REQUEST_URL_WITH_QUERY_PARAMS"
  }
}
```

- **`getObjectContext`**: Information provided by MinIO.
  - `inputS3Url`: A presigned URL generated by MinIO, allowing the service to download the original image.
  - `outputRoute` & `outputToken`: Used by the service to return the processed image back to MinIO.
- **`userRequest`**: Information about the original client request.
  - `url`: The full URL the end-user requested. The service uses query parameters from this URL to customize the watermark. For example, adding `?usercode=YourWatermarkText` to the original image URL will use "YourWatermarkText" as the watermark.

Refer to the [MinIO Object Lambda documentation](https://min.io/docs/minio/linux/developers/transforms-with-object-lambda.html) for details on setting up the Lambda function.

### Example Python Script for Generating Presigned URL

Here's an example using the `minio-py` library to generate a presigned URL that triggers the watermark lambda function:

```python
from minio import Minio
from minio.error import S3Error
from datetime import timedelta

# MinIO Configuration
endpoint = 'YOUR_MINIO_ENDPOINT'      # e.g., 'localhost:9000'
access_key = 'YOUR_ACCESS_KEY'
secret_key = 'YOUR_SECRET_KEY'
bucket_name = 'your-bucket-name'
object_key = 'image.jpg'
lambda_arn = 'arn:minio:s3-object-lambda::dynamicminiowatermark:webhook' # Replace dynamicminiowatermark with your Lambda ARN

# Desired watermark text
watermark_text = "example"

# Create MinIO client
client = Minio(
    endpoint,
    access_key=access_key,
    secret_key=secret_key,
    secure=secure # Set based on endpoint prefix
)

# Define extra parameters for Lambda override and watermark
extra_params = {
    "lambdaArn": f"{lambda_arn}",
    "usercode": watermark_text
}

# Generate presigned URL
try:
    presigned_url = client.presigned_get_object(
        bucket_name=bucket_name,
        object_name=object_key,
        expires=timedelta(hours=1), # URL expires in 1 hour
        extra_query_params=extra_params
    )
    print(f"Presigned URL with watermark '{watermark_text}':")
    print(presigned_url)

except S3Error as e:
    print(f"Error generating presigned URL: {e}")
```

## Usage

1. Start the server:

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
- Main endpoint: `[POST] /`
- Health check: `[GET] /health/` 