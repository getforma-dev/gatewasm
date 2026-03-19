# Contributing to GateWASM

## Prerequisites

- Rust (stable toolchain)
- Node.js 22+
- Docker (for PostgreSQL)

## Setup

```bash
git clone https://github.com/getforma-dev/gatewasm.git
cd gatewasm

# Install dependencies
npm install
cd admin && npm install && cd ..

# Copy env template
cp .env.example .env

# Start databases + build + run
npm run db
npm run build:ssr
npm run dev
```

Open http://localhost:3001

## Development

```bash
npm run dev           # Server + frontend watch (one terminal)
npm run test          # Rust unit tests
npm run test:e2e      # Playwright E2E (needs running server)
npm run typecheck     # TypeScript type check
```

## Project Structure

```
gatewasm/
├── crates/server/    Rust server (Axum + forma-server + forma-ir)
│   └── src/handlers/owner/   Split into submodules (customers, analytics, billing, invites)
├── admin/            TypeScript frontend (FormaJS islands + @getforma/build)
├── bridge/           Client SDK (gate-bridge.js)
├── infra/            AWS CDK infrastructure
├── scripts/          Build & deployment utilities
└── specs/            Compliance specifications (HIPAA)
```
