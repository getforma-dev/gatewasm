# GateWASM

Multi-tenant authentication platform built on [Forma](https://github.com/getforma-dev/forma). Rust backend (Axum) + FormaJS frontend + PostgreSQL + AWS (App Runner, RDS, CDK).

## Features

- Email/password + OAuth (Google, GitHub) authentication
- Multi-tenant with tenant branding and custom login pages
- MFA (TOTP) with setup, verification, and recovery
- JWT tokens with Ed25519 signing and refresh token rotation
- WebAuthn / FIDO2 passkey support
- Management API with API key authentication
- Self-serve tenant onboarding with trial enforcement
- HIPAA compliance engine with startup validation
- Island architecture SSR with activateIslands()
- CSP-safe hardened runtime — no eval, no Function

## Project Structure

```
gatewasm/
├── admin/          # Frontend — FormaJS admin UI, Tailwind CSS, Playwright tests
├── infra/          # Infrastructure — AWS CDK stacks (App Runner, RDS, networking)
├── crates/server/  # Backend — Rust server (Axum), auth logic, API handlers
├── migrations/     # SQL migrations (run automatically on server start)
├── scripts/        # Deployment and ops scripts (build, push, deploy, rollback)
├── bridge/         # gate-bridge.js client SDK
└── specs/          # Compliance specifications (HIPAA, planned: OWASP, SOC 2)
```

## Prerequisites

- **Rust** — `rustup` with stable toolchain (`rustc 1.80+`)
- **Node.js** — v18+ or v20+
- **Docker** — for local PostgreSQL (`docker 24+`)
- **npm** — comes with Node.js

For deployment only (not needed for local dev):
- **AWS CLI v2** — `brew install awscli`
- **AWS CDK** — `npm install -g aws-cdk`

## Quick Start

```bash
# 1. Install dependencies
npm install && cd admin && npm install && cd ..

# 2. Start the database
npm run db

# 3. Start everything (server + frontend watcher)
npm run dev

# 4. Seed test data (in another terminal, first time only)
npm run db:seed
```

The watcher generates a dev manifest automatically, and the server auto-runs migrations on startup. No manual build step needed. The seed script creates test tenants and users — run it once after a fresh DB or after `db:reset`.

For detailed local dev setup (creating tenants, API usage, env vars, troubleshooting), see [LOCAL-DEV.md](LOCAL-DEV.md).

---

## Scripts Reference

Three `package.json` files exist in this repo. The root orchestrates everything — you rarely need to `cd` into `admin/` or `infra/` directly.

### Root (`package.json`) — Orchestrator

#### Development

| Script | What it does | When to use |
|--------|-------------|-------------|
| `npm run dev` | Starts Rust server + frontend watcher (watcher auto-generates dev manifest) | Day-to-day development |
| `npm run dev:server` | Starts only the Rust backend | When you only need the API, no UI changes |
| `npm run dev:frontend` | Starts only the frontend file watcher | When you only need UI changes, server already running |

#### Database

| Script | What it does | When to use |
|--------|-------------|-------------|
| `npm run db` | Starts dev Postgres container (port 5434) | Before running the server |
| `npm run db:test` | Starts dev + test Postgres (ports 5434, 5433) | Before running integration tests |
| `npm run db:stop` | Stops all Docker containers | Done for the day |
| `npm run db:studio` | Dev DB + pgweb browser at http://localhost:8081 | Inspect dev data visually |
| `npm run db:studio:test` | Test DB + pgweb browser at http://localhost:8082 | Inspect test data visually |
| `npm run db:reset` | Wipes dev DB entirely (DROP SCHEMA) — restart server to re-migrate | Schema is broken, want a total fresh start |
| `npm run db:reset:test` | Wipes test DB entirely | Test DB is in a bad state |
| `npm run db:clear` | Truncates all user data but keeps schema and tenants | Want clean data without re-migrating (use this 90% of the time) |
| `npm run db:seed` | Creates test tenants (forma, acme), test users, assigns admin role | After a fresh DB or `db:reset` — sets up a usable local state |

#### Testing

| Script | What it does | When to use |
|--------|-------------|-------------|
| `npm run test` | Rust unit tests (`cargo test --lib`) — no DB needed | After changing Rust code |
| `npm run test:integration` | Rust integration tests — hits real test DB | After changing API handlers or DB logic |
| `npm run test:e2e` | Playwright browser tests (84 tests) | After changing UI or auth flows |
| `npm run typecheck` | TypeScript type check (`tsc --noEmit`) | After changing TypeScript code |

#### Build

| Script | What it does | When to use |
|--------|-------------|-------------|
| `npm run build` | Production frontend build (minified, content-hashed) | Before committing frontend changes |
| `npm run build:ssr` | Production build + SSR IR bytecode generation | After changing SSR page structure |

#### Cleanup

| Script | What it does | When to use |
|--------|-------------|-------------|
| `npm run clean` | Removes root + admin `node_modules` | Dependency issues |
| `npm run clean:build` | Removes `admin/dist` + `target` build artifacts | Stale build output |
| `npm run clean:all` | Removes everything — deps, artifacts, lock files, caches | Something is deeply broken |
| `npm run fresh` | `clean:all` then reinstall and rebuild SSR | Fresh clone or total rebuild |

---

### Admin (`admin/package.json`) — Frontend

Run from `admin/`. Most are also available from the root via wrapper scripts above.

| Script | What it does | When to use |
|--------|-------------|-------------|
| `npm run build` | Production JS/CSS bundles via esbuild (minified, content-hashed, compressed) | Before committing |
| `npm run build:ssr` | Same as build + generates SSR IR bytecode files | After changing SSR page structure |
| `npm run dev` | Watch mode — auto-rebuilds JS + CSS on every file save | During UI development |
| `npm run test` | Runs all Playwright E2E tests | After UI or auth flow changes |
| `npm run typecheck` | TypeScript type check without emitting | After changing `.ts` files |

Watch mode does NOT rebuild the Rust server or FormaJS itself. If you change `dist/` hashes or Rust handlers, restart the server. If you change FormaJS source, rebuild it first.

For frontend development patterns (components, pages, theming, build pipeline), see [admin/DEVELOPER.md](admin/DEVELOPER.md).

---

### Infra (`infra/package.json`) — AWS CDK

Run from `infra/`. Only needed for deployment — not for local development.

| Script | What it does | When to use |
|--------|-------------|-------------|
| `npm run build` | Compile CDK TypeScript stacks to JavaScript | Before running CDK commands |
| `npm run cdk` | Run any CDK command (e.g., `npm run cdk -- diff --all`) | Deploying or previewing infrastructure changes |

Common CDK workflows:

```bash
cd infra
TAG=$(../scripts/image_tag.sh)

# Preview what will change
npm run cdk -- diff --all -c gatewasm:imageTag=$TAG

# Deploy all stacks
npm run cdk -- deploy --all -c gatewasm:imageTag=$TAG
```

For full deployment procedures, see [RUNBOOK.md](RUNBOOK.md).

---

## Testing Guide

```bash
# Unit tests (fast, no DB needed)
npm run test

# Integration tests (needs test DB running)
npm run db:test
npm run test:integration

# E2E browser tests (needs server + DB running)
npm run db
npm run dev              # in one terminal
npm run test:e2e         # in another terminal

# Specific E2E tests
cd admin
npx playwright test e2e/smoke.spec.ts       # smoke tests only
npx playwright test -g 'tenant login'       # grep by test name
npx playwright test --headed                # see the browser
npx playwright test --debug                 # step-through debugger
```

## Architecture

Built on the Forma framework:
- **@getforma/core** — fine-grained reactive UI library (real DOM, no VDOM)
- **@getforma/build** — production build pipeline (esbuild, content hashing, Brotli)
- **@getforma/compiler** — TSX-to-FMIR compiler for SSR IR emission
- **forma-ir + forma-server** — compiled SSR with FMIR v2 binary IR
- **Island architecture** — SSR HTML with selective client hydration via `activateIslands()`

## Documentation

| Document | Purpose |
|----------|---------|
| [LOCAL-DEV.md](LOCAL-DEV.md) | Full local dev setup — tenants, API reference, env vars, troubleshooting |
| [admin/DEVELOPER.md](admin/DEVELOPER.md) | Frontend development — components, pages, theming, build pipeline |
| [RUNBOOK.md](RUNBOOK.md) | AWS deployment and operations — deploy, monitor, rollback, troubleshoot |
| [CUSTOMER-ONBOARDING-RUNBOOK.md](CUSTOMER-ONBOARDING-RUNBOOK.md) | Business operations — personas, customer onboarding flows |
| [ROADMAP.md](ROADMAP.md) | Future implementation plan — organizations, enterprise SSO |
| [specs/compliance/](specs/compliance/) | Compliance specifications — HIPAA, planned OWASP and SOC 2 |

## License

Business Source License 1.1 — see [LICENSE](LICENSE)
