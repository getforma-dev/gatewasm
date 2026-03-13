# --- Build stage ---
# NOTE: Build context must be the monorepo root (forma-code/)
# because include_str! in hosted_login.rs references npm-modules/ outside auth-module-poc/
FROM rust:1.93-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends libpq-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace manifests first for layer caching
COPY auth-module-poc/Cargo.toml auth-module-poc/Cargo.lock ./auth-module-poc/
COPY auth-module-poc/crates/gatewasm-core/Cargo.toml ./auth-module-poc/crates/gatewasm-core/Cargo.toml
COPY auth-module-poc/crates/gatewasm-server/Cargo.toml ./auth-module-poc/crates/gatewasm-server/Cargo.toml

# Create dummy src files to pre-build dependencies
RUN mkdir -p auth-module-poc/crates/gatewasm-core/src \
    && echo "pub fn stub(){}" > auth-module-poc/crates/gatewasm-core/src/lib.rs \
    && mkdir -p auth-module-poc/crates/gatewasm-server/src \
    && echo "fn main(){}" > auth-module-poc/crates/gatewasm-server/src/main.rs \
    && echo "pub fn stub(){}" > auth-module-poc/crates/gatewasm-server/src/lib.rs

RUN cd auth-module-poc && cargo build --release -p gatewasm-server 2>/dev/null || true

# Copy actual source and assets referenced by include_str!
COPY auth-module-poc/crates/ ./auth-module-poc/crates/
COPY auth-module-poc/bridge/ ./auth-module-poc/bridge/
COPY auth-module-poc/specs/ ./auth-module-poc/specs/
COPY auth-module-poc/admin/dist/ ./auth-module-poc/admin/dist/
COPY npm-modules/formajs/dist/formajs.global.js ./npm-modules/formajs/dist/formajs.global.js

# Touch files to invalidate the stub build cache
RUN touch auth-module-poc/crates/gatewasm-core/src/lib.rs \
    && touch auth-module-poc/crates/gatewasm-server/src/main.rs \
    && touch auth-module-poc/crates/gatewasm-server/src/lib.rs

RUN cd auth-module-poc && cargo build --release -p gatewasm-server

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

COPY --from=builder /build/auth-module-poc/target/release/gatewasm-server ./gatewasm-server

ENV PORT=3001
ENV HOST=0.0.0.0

EXPOSE 3001

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3001/health || exit 1

CMD ["./gatewasm-server"]
