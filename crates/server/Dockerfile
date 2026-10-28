# Build context is a committed source archive (scripts/build_local_image.sh).
# Frontend bundles are built inside this image; host admin/dist is never an input.
FROM --platform=$BUILDPLATFORM node:22-bookworm-slim AS frontend
WORKDIR /build/admin
COPY admin/package.json admin/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY admin/build.ts admin/tsconfig.json ./
COPY admin/src/ ./src/
COPY admin/scripts/write-build-identity.mjs ./scripts/write-build-identity.mjs
RUN npm run build:ssr
ARG GATEWASM_SOURCE_REVISION
RUN GATEWASM_SOURCE_REVISION=${GATEWASM_SOURCE_REVISION} node scripts/write-build-identity.mjs

FROM rust:1.93.1-bookworm AS builder
ARG CARGO_BUILD_JOBS=3
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}
RUN apt-get update && apt-get install -y --no-install-recommends libpq-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY .cargo/config.toml ./.cargo/config.toml
COPY crates/server/Cargo.toml ./crates/server/Cargo.toml
RUN mkdir -p crates/server/src \
    && echo "fn main(){}" > crates/server/src/main.rs \
    && echo "pub fn stub(){}" > crates/server/src/lib.rs
RUN cargo build --locked --release -p gatewasm-server
COPY crates/ ./crates/
COPY bridge/ ./bridge/
COPY specs/ ./specs/
COPY --from=frontend /build/admin/dist/ ./admin/dist/
RUN touch crates/server/src/main.rs crates/server/src/lib.rs
ARG GATEWASM_SOURCE_REVISION
ENV GATEWASM_SOURCE_REVISION=${GATEWASM_SOURCE_REVISION}
RUN cargo build --locked --release -p gatewasm-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libpq5 curl \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --shell /bin/bash app
USER app
WORKDIR /home/app
COPY --from=builder /build/target/release/gatewasm-server ./gatewasm-server
COPY --from=frontend /build/admin/dist/build-identity.json ./build-identity.json
ARG GATEWASM_SOURCE_REVISION
LABEL org.opencontainers.image.revision=${GATEWASM_SOURCE_REVISION}
ENV PORT=3001
ENV HOST=0.0.0.0
EXPOSE 3001
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3001/health || exit 1
CMD ["./gatewasm-server"]
