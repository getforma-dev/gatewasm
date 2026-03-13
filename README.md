# GateWASM

Multi-tenant authentication platform built on [Forma](https://github.com/getforma-dev/forma).

## Features

- Email/password + OAuth (Google, GitHub) authentication
- Multi-tenant with tenant branding and custom login pages
- MFA (TOTP) with setup, verification, and recovery
- JWT tokens with Ed25519 signing and refresh token rotation
- Management API with API key authentication
- Self-serve tenant onboarding with trial enforcement
- HIPAA compliance engine with startup validation
- Island architecture SSR with activateIslands()
- 84 end-to-end Playwright tests

## Quick Start

```bash
# Start PostgreSQL
docker-compose up -d db db-test

# Build the frontend
cd admin && npm install && npx tsx build.ts --ssr && cd ..

# Run the server
cargo run -p gatewasm-server
```

Open http://127.0.0.1:3001

## Architecture

Built on the Forma framework:
- **@getforma/core** — fine-grained reactive UI library (real DOM, no VDOM)
- **forma-ir + forma-server** — compiled SSR with FMIR v2 binary IR
- **Island architecture** — SSR HTML with selective client hydration via `activateIslands()`

## Development

See [LOCAL-DEV.md](LOCAL-DEV.md) for full setup instructions.
See [RUNBOOK.md](RUNBOOK.md) for deployment procedures.

## Testing

```bash
# Rust tests (143 integration tests — requires PostgreSQL)
cargo test -p gatewasm-server

# Playwright tests (84 E2E tests — requires running server)
cd admin && npx playwright test
```

## License

Business Source License 1.1 — see [LICENSE](LICENSE)
