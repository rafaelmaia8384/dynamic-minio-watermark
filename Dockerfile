FROM rust:1.77-slim as builder

WORKDIR /usr/src/app
COPY . .

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Build with release profile and embedded font feature
RUN cargo build --release --features embedded_font

# Create a smaller production image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy the binary from builder
COPY --from=builder /usr/src/app/target/release/dynamic-minio-watermark /app/dynamic-minio-watermark

# Copy assets for optional non-embedded usage
COPY assets /app/assets

# Expose the port
ARG PORT=3333
ENV PORT=$PORT
EXPOSE $PORT

# Run as non-root user for security
RUN groupadd -r appuser && useradd -r -g appuser appuser
RUN chown -R appuser:appuser /app
USER appuser

# Command to run
CMD ["/app/dynamic-minio-watermark"]