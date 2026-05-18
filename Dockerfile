FROM rust:1.82-slim AS builder

WORKDIR /app

# Install build dependencies.
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libudev-dev libdbus-1-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . .

RUN cargo build --release -p veyn-core

# ── Runtime image ──────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libdbus-1-3 curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/veyn-core /usr/local/bin/veyn-core
COPY --from=builder /app/rules.toml /app/rules.toml

EXPOSE 7700 7701

ENV RUST_LOG=info

ENTRYPOINT ["veyn-core", "--no-auth"]
