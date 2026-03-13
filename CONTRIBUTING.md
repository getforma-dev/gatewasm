# Contributing to GateWASM

## Prerequisites

- Rust (stable toolchain)
- Node.js 22+
- PostgreSQL 16 (via Docker)
- Docker & Docker Compose

## Setup

```bash
# Clone the repo
git clone https://github.com/getforma-dev/gatewasm.git
cd gatewasm

# Start databases
docker-compose up -d db db-test

# Build frontend assets
cd admin
npm install
npx tsx build.ts --ssr
cd ..

# Copy env and fill in secrets
cp .env.example .env

# Run the server
cargo run -p gatewasm-server

# Run tests
cargo test -p gatewasm-server
cd admin && npx playwright test
```

## Project Structure

```
gatewasm/
├── crates/server/    Rust server (axum + forma-server + forma-ir)
├── admin/            TypeScript frontend (FormaJS islands)
├── bridge/           Client SDK (gate-bridge.js)
├── infra/            AWS CDK infrastructure
├── scripts/          Build & deployment utilities
└── specs/            Compliance specifications
```
