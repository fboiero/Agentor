# Stage 1: Build the WASM echo-skill
FROM rust:1.83-slim-bookworm AS wasm-builder
RUN rustup target add wasm32-wasip1
WORKDIR /build
COPY skills/echo-skill/ skills/echo-skill/
RUN cargo build --target wasm32-wasip1 --release --manifest-path skills/echo-skill/Cargo.toml

# Stage 2: Build the main binary
FROM rust:1.83-slim-bookworm AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /build
# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY wit/ wit/
RUN cargo build --release --bin agentor

# Stage 3: Minimal runtime image
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
RUN useradd -m -s /bin/bash agentor

WORKDIR /app

# Copy binary
COPY --from=builder /build/target/release/agentor /app/agentor

# Copy WASM skills
COPY --from=wasm-builder /build/skills/echo-skill/target/wasm32-wasip1/release/echo-skill.wasm /app/skills/echo-skill.wasm

# Copy default config
COPY agentor.toml /app/agentor.toml

# Create data directories
RUN mkdir -p /app/data/audit /app/data/sessions && chown -R agentor:agentor /app

USER agentor

EXPOSE 3000

ENTRYPOINT ["/app/agentor"]
CMD ["serve"]
