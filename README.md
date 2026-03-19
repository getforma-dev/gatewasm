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

## Development

```bash
npm run dev             # Server + frontend watch (one terminal)
npm run build:ssr       # Production build with SSR IR
npm run test            # Rust unit tests
npm run test:e2e        # Playwright E2E (needs running server)
npm run typecheck       # TypeScript type check
npm run db:reset        # Wipe dev database
```

See [LOCAL-DEV.md](LOCAL-DEV.md) for full setup instructions.
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
