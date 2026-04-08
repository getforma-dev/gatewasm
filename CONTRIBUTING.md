# Contributing to GateWASM

## Prerequisites

- Rust (stable toolchain)
- Node.js 22+
- Docker (for PostgreSQL)
- Mailpit (`brew install mailpit`) — local email capture

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

Open http://localhost:3001 (app) and http://localhost:8025 (email inbox)

## Development Commands

All commands run from the project root.

### Daily Dev

| Command | What it does |
|---------|-------------|
| `npm run dev` | Starts Rust server + frontend watcher + Mailpit (all three) |
| `npm run dev:clean` | Clears all user data, then starts dev (keeps tenants/roles) |
| `npm run dev:reset` | Wipes DB schema completely, then starts dev (server re-runs migrations) |
| `npm run cache:clear` | Opens `/dev/clear-cache` to purge browser localStorage/cookies |

### Building

| Command | What it does |
|---------|-------------|
| `npm run build` | Frontend production build |
| `npm run build:ssr` | Frontend SSR build (required before `cargo run`) |
| `npm run typecheck` | TypeScript type-check |

### Testing

| Command | What it does |
|---------|-------------|
| `npm test` | Rust unit tests |
| `npm run test:integration` | Rust integration tests (needs test DB: `npm run db:test`) |
| `npm run test:e2e` | Full Playwright e2e suite (149 tests, starts its own server on :3102) |
| `npm run test:e2e:smoke` | Smoke subset only (44 critical-path tests) |

From `admin/` directory, additional test filtering:

| Command | What it does |
|---------|-------------|
| `npm run test:smoke` | `@smoke` tagged tests |
| `npm run test:regression` | `@regression` — all tests |
| `npm run test:rbac` | `@rbac` — team/role/permission tests |
| `npm run test:ws` | `@ws` — WebSocket tests |
| `npm run test:visual` | `@visual` — screenshot regression |
| `npm run test:a11y` | `@a11y` — accessibility (axe-core) |
| `npm run test:headed` | Run tests in visible browser |
| `npm run test:debug` | Playwright inspector (step through tests) |

### Database

| Command | What it does |
|---------|-------------|
| `npm run db` | Start dev PostgreSQL (Docker) |
| `npm run db:test` | Start dev + test PostgreSQL containers |
| `npm run db:stop` | Stop all Docker containers |
| `npm run db:reset` | Drop + recreate dev DB schema (nuclear reset) |
| `npm run db:reset:test` | Drop + recreate test DB schema |
| `npm run db:clear` | Truncate all user data but keep tenants/roles |
| `npm run db:seed` | Seed test data (forma + acme tenants, test users) |
| `npm run db:studio` | Open pgweb UI for dev DB at http://localhost:8081 |
| `npm run db:studio:test` | Open pgweb UI for test DB at http://localhost:8082 |

### Cleanup

| Command | When to use |
|---------|-------------|
| `npm run clean` | Remove node_modules (both root + admin) |
| `npm run clean:build` | Remove build artifacts (admin/dist + target) |
| `npm run clean:all` | Nuclear: remove everything (node_modules, dist, target, lockfiles) |
| `npm run fresh` | `clean:all` → reinstall → rebuild SSR |
| `npm run fresh:dev` | `fresh` → reset DB → start dev. **Complete fresh start.** |

### When to use which reset

```
Something looks stale         →  npm run cache:clear  (browser caches)
User data is messy             →  npm run dev:clean    (truncate users, keep schema)
Migrations changed             →  npm run dev:reset    (drop schema, re-run migrations)
Dependencies or build broken   →  npm run fresh        (nuke + reinstall + rebuild)
Everything broken, start over  →  npm run fresh:dev    (nuke + reinstall + rebuild + reset DB + dev)
```

## Email in Dev

`npm run dev` automatically starts [Mailpit](https://github.com/axllent/mailpit) alongside the server. Every email the dev server sends (verification, invites, password reset) is captured locally.

- **Inbox UI:** http://localhost:8025
- **No real emails are sent** — Resend is disabled in `.env` by default
- To test real delivery via Resend: uncomment `RESEND_API_KEY` and comment out `SMTP_*` in `.env`

## Project Structure

```
gatewasm/
├── crates/server/    Rust server (Axum + forma-server + forma-ir)
├── admin/            TypeScript frontend (FormaJS islands + @getforma/build)
│   └── e2e/          Playwright e2e tests
├── bridge/           Client SDK (gate-bridge.js)
├── infra/            AWS CDK infrastructure
├── scripts/          Build & deployment utilities
└── specs/            Compliance specifications (HIPAA)
```
