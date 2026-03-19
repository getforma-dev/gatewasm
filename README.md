# GateWASM

Multi-tenant authentication platform built on [Forma](https://github.com/getforma-dev/forma).

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
- 76 unit tests + 84 Playwright E2E tests

## Quick Start

```bash
# Install dependencies
npm install              # root (concurrently)
cd admin && npm install  # frontend

# Start PostgreSQL
npm run db

# Build frontend with SSR
npm run build:ssr

# Start everything (server + frontend watch)
npm run dev
```

Open http://localhost:3001

## Commands

### Development

| Command | What it does |
|---------|-------------|
| `npm run dev` | Start server + frontend watch in one terminal |
| `npm run dev:server` | Start Rust server only |
| `npm run dev:frontend` | Start frontend watch only (esbuild, rebuilds on save) |
| `npm run build` | Production frontend build (no SSR) |
| `npm run build:ssr` | Production frontend build with SSR IR emission |

### Database

| Command | What it does |
|---------|-------------|
| `npm run db` | Start dev PostgreSQL container |
| `npm run db:test` | Start dev + test PostgreSQL containers |
| `npm run db:studio` | Open pgweb DB viewer at http://localhost:8081 |
| `npm run db:studio:test` | Open pgweb for test DB at http://localhost:8082 |
| `npm run db:clear` | Delete all user data (keep tables, tenants, roles) |
| `npm run db:reset` | Wipe entire database (server re-runs migrations on restart) |
| `npm run db:reset:test` | Wipe test database |
| `npm run db:stop` | Stop all Docker containers |

### Testing

| Command | What it does |
|---------|-------------|
| `npm run test` | Rust unit tests (no DB needed) |
| `npm run test:integration` | Rust integration tests (needs PostgreSQL) |
| `npm run test:e2e` | Playwright E2E tests (needs running server) |
| `npm run typecheck` | TypeScript type check |

### Cleanup

| Command | What it deletes | When to use |
|---------|----------------|-------------|
| `npm run clean` | node_modules (root + admin) | Dependency issues |
| `npm run clean:build` | admin/dist + target | Stale build artifacts |
| `npm run clean:all` | Everything + lockfiles + npm cache | Nuclear option |
| `npm run fresh` | clean:all → reinstall → rebuild SSR | Start completely over |

See [LOCAL-DEV.md](LOCAL-DEV.md) for full setup guide.
See [RUNBOOK.md](RUNBOOK.md) for deployment procedures.

## Architecture

Built on the Forma framework:
- **@getforma/core** — fine-grained reactive UI library (real DOM, no VDOM)
- **@getforma/build** — production build pipeline (esbuild, content hashing, Brotli)
- **@getforma/compiler** — TSX-to-FMIR compiler for SSR IR emission
- **forma-ir + forma-server** — compiled SSR with FMIR v2 binary IR
- **Island architecture** — SSR HTML with selective client hydration via `activateIslands()`

## License

Business Source License 1.1 — see [LICENSE](LICENSE)
