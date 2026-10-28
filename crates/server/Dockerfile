# --- Build stage ---
# Build context: repo root (gatewasm/)
FROM rust:1.93-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends libpq-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates/server/Cargo.toml ./crates/server/Cargo.toml

# Create dummy src files to pre-build dependencies
RUN mkdir -p crates/server/src \
    && echo "fn main(){}" > crates/server/src/main.rs \
    && echo "pub fn stub(){}" > crates/server/src/lib.rs

RUN cargo build --release -p gatewasm-server 2>/dev/null || true

# Copy actual source and assets referenced by include_str! / rust-embed
COPY crates/ ./crates/
COPY bridge/ ./bridge/
COPY specs/ ./specs/
COPY admin/dist/ ./admin/dist/

# Touch files to invalidate the stub build cache
RUN touch crates/server/src/main.rs crates/server/src/lib.rs

ARG GATEWASM_SOURCE_REVISION=unrecorded
ENV GATEWASM_SOURCE_REVISION=${GATEWASM_SOURCE_REVISION}
RUN cargo build --locked --release -p gatewasm-server

# --- Runtime stage ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash app
USER app
WORKDIR /home/app

COPY --from=builder /build/target/release/gatewasm-server ./gatewasm-server

ENV PORT=3001
ENV HOST=0.0.0.0

EXPOSE 3001

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3001/health || exit 1

CMD ["./gatewasm-server"]
