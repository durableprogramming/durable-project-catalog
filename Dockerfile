# Multi-stage Docker build for Durable Project Catalog
# Produces a minimal Alpine-based image with the dpc binary

# Build stage
FROM rust:1.75-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    sqlite-dev \
    openssl-dev \
    pkgconfig

WORKDIR /build

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY lib ./lib

# Build the release binary
RUN cargo build --release -p dprojc-cli

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache \
    sqlite-libs \
    ca-certificates \
    && adduser -D -u 1000 dpc

# Copy binary from builder
COPY --from=builder /build/target/release/dpc /usr/local/bin/dpc

# Create data directory
RUN mkdir -p /home/dpc/.local/durable && \
    chown -R dpc:dpc /home/dpc/.local

USER dpc
WORKDIR /home/dpc

# Set default database location
ENV DPC_DATABASE=/home/dpc/.local/durable/durable-project-catalog

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD dpc stats || exit 1

ENTRYPOINT ["dpc"]
CMD ["--help"]
