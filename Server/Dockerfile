# ─── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.89-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependencies first
COPY Cargo.toml Cargo.lock* ./
RUN mkdir -p src/bin && \
    echo "fn main(){}" > src/main.rs && \
    echo "fn main(){}" > src/bin/uploader.rs && \
    cargo build --release && \
    rm -rf src

# Build actual application
COPY src ./src
COPY templates ./templates
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release

# ─── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd -r appuser && useradd -r -g appuser appuser

WORKDIR /app

COPY --from=builder /build/target/release/obsidian-publisher /app/obsidian-publisher
COPY static ./static

RUN mkdir -p /vault /config && chown -R appuser:appuser /app /vault /config

USER appuser

EXPOSE 3000

ENV CONFIG_PATH=/app/config.yml \
    RUST_LOG=info

CMD ["/app/obsidian-publisher"]
