# ─────────────────────────────────────────────────────────────────────────────
# Stage 1: Builder
# ─────────────────────────────────────────────────────────────────────────────
FROM rust:1.86-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first (for dependency caching)
COPY Cargo.toml ./
# Generate lockfile if not present (supports gitignored Cargo.lock)
RUN if [ ! -f Cargo.lock ]; then cargo generate-lockfile; fi

# Placeholder src to warm up cargo cache
RUN mkdir -p src && echo "fn main() {}\" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source and rebuild
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ─────────────────────────────────────────────────────────────────────────────
# Stage 2: Runtime
# ─────────────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user and data directory
RUN useradd -r -s /bin/false xergon && \
    mkdir -p /app/data /etc/xergon && \
    chown -R xergon:xergon /app

COPY --from=builder /app/target/release/xergon-relay /usr/local/bin/
COPY config.toml.example /etc/xergon/config.toml.example

# Set default env (can be overridden)
ENV CONFIG_PATH=/etc/xergon/config.toml
ENV SETTLEMENT_DB_PATH=/app/data/settlement.db
ENV RUST_BACKTRACE=1
ENV RUST_LOG=info

USER xergon
WORKDIR /app

EXPOSE 9090

HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:9090/health || exit 1

ENTRYPOINT ["xergon-relay"]
