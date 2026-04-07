# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install build dependencies (for ssh2, ergo-lib optional)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Build actual binary
COPY src ./src
COPY contracts ./contracts
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/xergon-agent /usr/local/bin/

# Copy default config (can be overridden via volume mount)
COPY config.toml /etc/xergon/config.toml.example

RUN useradd -r -s /bin/false xergon && \
    mkdir -p /app/data && \
    chown xergon:xergon /app/data

USER xergon

WORKDIR /app

EXPOSE 9099

HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:9099/api/health || exit 1

ENTRYPOINT ["xergon-agent"]
