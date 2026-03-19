# GateWASM — Local Development Guide

How to run, test, and debug the auth server on your machine.

---

## Prerequisites

```bash
# Rust toolchain
rustup update stable
rustc --version   # 1.80+

# Docker (for Postgres)
docker --version  # 24+

# Optional: psql CLI
brew install libpq && brew link --force libpq
```

---

## Docker Architecture

This project uses Docker for **two separate purposes**:

### 1. Local Databases (Docker Compose)

Two PostgreSQL containers run locally for development and testing. These stay on your machine — they are **never pushed anywhere**.

| Container | Port | Database | Purpose |
|-----------|------|----------|---------|
| `db` | 5434 | `gatewasm` | Local dev server data |
| `db-test` | 5433 | `gatewasm_test` | Integration tests (isolated schemas) |

The Rust server runs natively on your host (`cargo run`), connecting to these containers.

### 2. Server Image (Docker Build → ECR → App Runner)

When deploying to AWS, the Rust server itself is packaged into a Docker image:

```
cargo build → Docker image → push to ECR → App Runner auto-deploys
```

| What | Where | Image |
|------|-------|-------|
| Postgres containers | Local only | `postgres:16-alpine` |
| Server image | Pushed to AWS ECR | `gatewasm-server:latest` |

In AWS, the server connects to RDS PostgreSQL (not these Docker containers).

### Quick reference

```bash
# Start local databases
docker compose up -d db db-test

# Build server image for AWS deployment
docker build --platform linux/amd64 -t gatewasm-server -f crates/gatewasm-server/Dockerfile ..

# Push to ECR
aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin 877573132739.dkr.ecr.us-east-1.amazonaws.com
docker tag gatewasm-server:latest 877573132739.dkr.ecr.us-east-1.amazonaws.com/gatewasm-server:latest
docker push 877573132739.dkr.ecr.us-east-1.amazonaws.com/gatewasm-server:latest
```

---

## 1. Start Local Databases

```bash
# Start both databases
npm run db:test

# Verify they're healthy
docker compose ps
```

Both should show `(healthy)`. The dev DB runs on port **5434** (not 5432, to avoid conflicts with any native Postgres installation).

---

## 2. Database GUI (pgweb)

A browser-based Postgres viewer is included in docker-compose. No install needed — browse tables, run queries, export data.

```bash
npm run db:studio       # Dev DB → http://localhost:8081
npm run db:studio:test  # Test DB → http://localhost:8082
```

| GUI | URL | Database |
|-----|-----|----------|
| Dev DB | http://localhost:8081 | `gatewasm` (port 5434) |
| Test DB | http://localhost:8082 | `gatewasm_test` (port 5433) |

Open the URL in your browser — you can browse tables, run queries, and export data.

### Alternative: CLI (psql)

```bash
# Connect to dev database
psql "postgres://gatewasm:gatewasm@localhost:5434/gatewasm"

# Connect to test database
psql "postgres://gatewasm:gatewasm@localhost:5433/gatewasm_test"

# Useful commands:
# \dt              — list tables
# \d users         — describe users table
# SELECT * FROM users LIMIT 10;
```

### Alternative: Desktop GUI Apps

If you prefer a native app over pgweb:

- **TablePlus** (macOS, free tier) — `brew install --cask tableplus`
- **DBeaver** (cross-platform, free) — `brew install --cask dbeaver-community`
- **Postico 2** (macOS, free tier) — `brew install --cask postico`

Connection settings for any GUI app:

```
Host:     localhost
Port:     5434 (dev) or 5433 (test)
Database: gatewasm (dev) or gatewasm_test (test)
User:     gatewasm
Password: gatewasm
SSL:      off
```

### Resetting data

Two levels of reset — pick the right one:

| Command | What it does | When to use |
|---------|-------------|-------------|
| `npm run db:clear` | Deletes all user data (users, sessions, tokens, audit logs, webhooks, etc.) but **keeps tables, tenants, roles, and config** | Want a clean slate without waiting for migrations |
| `npm run db:reset` | Drops everything — tables, indexes, constraints. Server re-runs all 26 migrations on next start | Schema changes, broken migrations, total reset |
| `npm run db:reset:test` | Same as reset but for the test database | Before re-running integration tests |

`db:clear` is what you want 90% of the time — it's fast (no migration re-run) and keeps your tenant setup intact.

---

## 3. Run the Server Locally

The `.env` file in the project root configures the server. After starting databases:

```bash
npm run dev
```

This starts both the Rust server and frontend watch mode (via `concurrently`) in one terminal. The server reads `.env` automatically via `dotenvy`.

You should see:

```
[server]  Starting GateWASM server host=0.0.0.0 port=3001
[server]  Database connected
[server]  Phase 2 SSR enabled for 8 route(s)
[server]  GateWASM server listening addr=0.0.0.0:3001
[frontend] ⚡ Done in 28ms
```

Migrations (1-26) run automatically on startup. When you edit a `.ts` file in `admin/src/`, the frontend rebuilds instantly — refresh the browser to see changes.

### Verify

```bash
curl -s http://localhost:3001/health | jq
# {"status":"ok","db_status":"ok"}
```

---

## 4. Create a Local Tenant

The server is multi-tenant. You need at least one tenant for `/t/{slug}/...` routes to work.

```bash
BASE=http://localhost:3001

# Create the "forma" tenant
curl -s -X POST $BASE/platform/tenants \
  -H "Authorization: Bearer plat_local_dev_key" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Forma Local",
    "slug": "forma",
    "owner_email": "admin@local.dev",
    "allowed_providers": ["email", "google", "github"]
  }' | jq
```

Save the returned `api_key` (starts with `mgmt_...`) — you need it for management endpoints.

### Configure the tenant

```bash
curl -s -X PATCH $BASE/manage/config \
  -H "Authorization: Bearer <PASTE_MGMT_KEY_HERE>" \
  -H "Content-Type: application/json" \
  -d '{
    "login_redirect_url": "http://localhost:3001/t/forma/dashboard",
    "allowed_providers": ["email"]
  }' | jq
```

---

## 5. Test in Browser

Open: **http://localhost:3001/t/forma/login**

1. Register with any email/password
2. You land on the dashboard at `/t/forma/dashboard`
3. Test profile, login history, sign out

### Token flow (httpOnly cookies)

- **Refresh token** is stored as an httpOnly cookie (invisible to JS)
- **Access token** lives only in a JS variable (never persisted)
- On page reload, the dashboard does a silent refresh via the cookie
- Check `DevTools → Application → Cookies` to see the `refresh_token` cookie
- Check `DevTools → Application → Session Storage` — should be empty (no tokens)

---

## 6. Run Tests

```bash
# Unit tests (no DB needed)
npm run test

# Integration tests (needs PostgreSQL)
npm run db:test
npm run test:integration

# Playwright E2E (needs running server)
npm run dev          # in one terminal
npm run test:e2e     # in another

# Run a specific Rust test
cargo test -p gatewasm-server --test integration refresh_token_from_cookie

# Run with output visible
cargo test -p gatewasm-server -- --nocapture
```

Tests use the `db-test` database on port 5433. Each test creates an isolated Postgres schema, so tests never interfere with each other.

### Test database URL

The default is `postgres://gatewasm:gatewasm@localhost:5433/gatewasm_test`. Override with:

```bash
TEST_DATABASE_URL=postgres://user:pass@host:port/db cargo test -p gatewasm-server
```

---

## 7. API Quick Reference

All endpoints require `X-Tenant-Slug: forma` header (or use `/t/forma/...` URL paths).

### Public endpoints

```bash
BASE=http://localhost:3001

# Register
curl -X POST $BASE/auth/register \
  -H "X-Tenant-Slug: forma" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@test.com","password":"Str0ngP@ss","name":"Test"}'

# Login
curl -X POST $BASE/auth/login \
  -H "X-Tenant-Slug: forma" \
  -H "Content-Type: application/json" \
  -d '{"email":"test@test.com","password":"Str0ngP@ss"}'

# Refresh token (via cookie — use -b to send cookie)
curl -X POST $BASE/auth/token \
  -H "X-Tenant-Slug: forma" \
  -H "Content-Type: application/json" \
  -b "refresh_token=<TOKEN>" \
  -d '{"grant_type":"refresh_token"}'

# JWKS (public keys for JWT verification)
curl $BASE/auth/jwks | jq
```

### Authenticated endpoints (need Bearer token)

```bash
TOKEN=<access_token_from_login>

# Get current user profile (includes permissions)
curl $BASE/auth/me \
  -H "X-Tenant-Slug: forma" \
  -H "Authorization: Bearer $TOKEN"

# Login history
curl $BASE/auth/login-history \
  -H "X-Tenant-Slug: forma" \
  -H "Authorization: Bearer $TOKEN"

# Logout
curl -X POST $BASE/auth/logout \
  -H "X-Tenant-Slug: forma" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{}'
```

### Admin endpoints (need admin role)

```bash
# Stats
curl "$BASE/admin/stats" -H "Authorization: Bearer $TOKEN" -H "X-Tenant-Slug: forma"

# User list
curl "$BASE/admin/users?page=1&per_page=20" -H "Authorization: Bearer $TOKEN" -H "X-Tenant-Slug: forma"

# Audit log
curl "$BASE/admin/audit-log?page=1&per_page=20" -H "Authorization: Bearer $TOKEN" -H "X-Tenant-Slug: forma"
```

### Management endpoints (need mgmt_ API key)

```bash
MGMT_KEY=<mgmt_key_from_tenant_creation>

# --- Users ---
curl "$BASE/manage/users?page=1&per_page=20" -H "Authorization: Bearer $MGMT_KEY"
curl -X POST $BASE/manage/users -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" \
  -d '{"email":"new@test.com","password":"Str0ngP@ss","name":"New User","role":"editor"}'

# --- Custom Roles ---
# List roles (system + custom)
curl $BASE/manage/roles -H "Authorization: Bearer $MGMT_KEY"

# Create custom role
curl -X POST $BASE/manage/roles -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name":"editor","description":"Can edit content","permissions":["user:read","user:write"]}'

# Update custom role
curl -X PATCH $BASE/manage/roles/<ROLE_ID> -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" \
  -d '{"description":"Updated","permissions":["user:read","user:write","audit:read"]}'

# Delete custom role (system roles "user"/"admin" can't be deleted)
curl -X DELETE $BASE/manage/roles/<ROLE_ID> -H "Authorization: Bearer $MGMT_KEY"

# Assign role to user
curl -X POST $BASE/manage/users/<USER_ID>/roles -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" -d '{"role_id":"<ROLE_ID>"}'

# Remove role from user
curl -X DELETE $BASE/manage/users/<USER_ID>/roles/<ROLE_ID> -H "Authorization: Bearer $MGMT_KEY"

# --- Webhooks ---
# List webhooks
curl $BASE/manage/webhooks -H "Authorization: Bearer $MGMT_KEY"

# Create webhook (secret returned once — save it!)
curl -X POST $BASE/manage/webhooks -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://your-backend.com/webhook",
    "events": ["user.registered", "user.logged_in", "user.role_changed"],
    "description": "Notify backend on auth events"
  }'

# Get webhook detail (secret masked)
curl $BASE/manage/webhooks/<WEBHOOK_ID> -H "Authorization: Bearer $MGMT_KEY"

# Update webhook
curl -X PATCH $BASE/manage/webhooks/<WEBHOOK_ID> -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" \
  -d '{"active": false}'

# Delete webhook
curl -X DELETE $BASE/manage/webhooks/<WEBHOOK_ID> -H "Authorization: Bearer $MGMT_KEY"

# View delivery history
curl "$BASE/manage/webhooks/<WEBHOOK_ID>/deliveries?page=1&per_page=20" \
  -H "Authorization: Bearer $MGMT_KEY"

# Send test event
curl -X POST $BASE/manage/webhooks/<WEBHOOK_ID>/test -H "Authorization: Bearer $MGMT_KEY"
```

**Valid webhook events**: `user.registered`, `user.logged_in`, `user.logged_out`, `user.deleted`, `user.email_verified`, `user.password_reset`, `user.role_changed`

**Valid permission keys**: `user:read`, `user:write`, `user:delete`, `role:read`, `role:write`, `audit:read`, `config:read`, `config:write`, `webhook:read`, `webhook:write`, plus any `custom:*` prefix.

### Platform endpoints (need platform key)

```bash
# List tenants
curl $BASE/platform/tenants \
  -H "Authorization: Bearer plat_local_dev_key"

# Create tenant
curl -X POST $BASE/platform/tenants \
  -H "Authorization: Bearer plat_local_dev_key" \
  -H "Content-Type: application/json" \
  -d '{"name":"New Tenant","slug":"new","owner_email":"owner@test.com","allowed_providers":["email"]}'
```

---

## 8. Project Structure

```
gatewasm/
├── package.json                  # Root dev scripts (npm run dev, test, etc.)
├── crates/server/                # Axum server
│   ├── src/
│   │   ├── main.rs               # Entry point + background tasks
│   │   ├── lib.rs                # AppState, module exports
│   │   ├── config.rs             # AppConfig from env vars
│   │   ├── error.rs              # AppError enum → JSON responses
│   │   ├── responses.rs          # Typed response structs
│   │   ├── types.rs              # Shared request/response types
│   │   ├── db/                   # Database pool, migrations (001-026)
│   │   ├── handlers/
│   │   │   ├── mod.rs            # Router setup
│   │   │   ├── auth.rs           # Register, login, logout, me, verify-email
│   │   │   ├── oauth.rs          # Google/GitHub OAuth PKCE flow
│   │   │   ├── token.rs          # Token refresh, grant types
│   │   │   ├── mfa.rs            # TOTP setup, verify, recovery
│   │   │   ├── webauthn.rs       # WebAuthn / FIDO2 passkeys
│   │   │   ├── management.rs     # Tenant management API (mgmt_ key)
│   │   │   ├── owner/            # Owner dashboard (split into submodules)
│   │   │   │   ├── customers.rs  # Customer CRUD + lifecycle
│   │   │   │   ├── analytics.rs  # Stats, analytics, timeline
│   │   │   │   ├── billing.rs    # Payments
│   │   │   │   ├── invites.rs    # Owner invitations
│   │   │   │   └── dashboard.rs  # Page serving
│   │   │   ├── onboarding.rs     # Self-serve tenant onboarding
│   │   │   ├── webhooks.rs       # Webhook CRUD + SSRF filter
│   │   │   ├── ssf.rs            # Security Event Framework streams
│   │   │   └── ...
│   │   ├── middleware/
│   │   │   ├── tenant.rs         # Tenant resolution + cache
│   │   │   ├── auth.rs           # JWT extraction
│   │   │   ├── cors.rs           # Dynamic per-tenant CORS
│   │   │   ├── csp.rs            # CSP headers (via forma-server)
│   │   │   ├── rate_limit.rs     # IP-based rate limiting
│   │   │   ├── entitlements.rs   # Plan enforcement
│   │   │   ├── fingerprint.rs    # Client fingerprinting
│   │   │   ├── dpop.rs           # DPoP proof validation
│   │   │   └── ...
│   │   ├── jwt/                  # Ed25519 key management, token signing
│   │   ├── crypto/               # Argon2 password hashing
│   │   ├── compliance/           # HIPAA compliance engine
│   │   ├── html_template.rs      # SSR rendering (delegates to forma-server)
│   │   └── ir/                   # FMIR binary IR loading (via forma-server)
│   ├── tests/integration.rs      # Integration tests (needs PostgreSQL)
│   └── Dockerfile                # Multi-stage build for AWS
├── admin/                        # TypeScript frontend (FormaJS + esbuild)
│   ├── build.ts                  # Build config (delegates to @getforma/build)
│   ├── src/core/                 # Shared components, hooks, types
│   ├── src/platform/             # Platform pages (login, onboarding, console)
│   ├── src/tenant/               # Tenant pages (login, dashboard, admin)
│   ├── src/admin/                # Owner dashboard
│   └── e2e/                      # Playwright tests (84 tests)
├── bridge/                       # gate-bridge.js client SDK
├── infra/                        # AWS CDK infrastructure
├── docker-compose.yml            # Local Postgres + pgweb
├── .env                          # Local config (gitignored)
├── RUNBOOK.md                    # Deployment procedures
└── LOCAL-DEV.md                  # This file
```

---

## 9. Database Migrations

Migrations run automatically on server startup. Current migrations (1-26):

| # | Label | What it does |
|---|-------|--------------|
| 1 | initial schema | users, credentials, roles, user_roles, refresh_tokens, login_attempts |
| 2 | hmac + email verification | email_verified, verification tokens, HMAC columns |
| 3 | hardening | rate limiting, account lockout, token family tracking |
| 4 | multi-tenancy | tenants table, tenant_id on all tables, scoped constraints |
| 5 | compliance engine | audit_log, compliance_config, session_timeout, mfa columns |
| 6 | hosted login branding | tenant branding (logo, colors, CSS), login_redirect_url |
| 7 | self-serve onboarding | trial_expires_at, owner_user_id, platform users |
| 8 | custom roles | tenant-scoped roles, permissions_jsonb, is_system flag |
| 9 | webhooks | webhooks table, webhook_deliveries table |
| 10-16 | login redirects → billing | redirect defaults, entitlements, orgs, owner plane, invites, billing |
| 17-19 | MFA + indexes | MFA lockout + secret encryption, timeline events, query indexes |
| 20-22 | security hardening | signup invites, platform redirect fix, security config table |
| 23 | webauthn + fedcm | WebAuthn/FIDO2 credentials, FedCM approved clients |
| 24 | enterprise security | PAR requests, mTLS certificates, DPoP jti cache |
| 25 | SSF | Shared Signals Framework streams + outbox + geo columns |
| 26 | tenant FK constraints | Foreign key constraints on tenant_id columns |

---

## 10. Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3001` | Server port |
| `HOST` | `0.0.0.0` | Bind address |
| `SERVER_URL` | `http://localhost:3001` | Public URL (cookies, OAuth redirects) |
| `DATABASE_URL` | `postgres://...@localhost:5434/gatewasm` | Postgres connection string |
| `JWT_ISSUER` | `gatewasm` | JWT `iss` claim |
| `JWT_AUDIENCE` | `gatewasm-app` | JWT `aud` claim |
| `JWT_ACCESS_TOKEN_EXPIRY` | `900` | Access token TTL in seconds (15 min) |
| `JWT_REFRESH_TOKEN_EXPIRY` | `2592000` | Refresh token TTL in seconds (30 days) |
| `ALLOWED_ORIGINS` | `http://localhost:3000,...` | CORS origins (comma-separated) |
| `PLATFORM_DOMAIN` | — | Domain for slug-based routing |
| `PLATFORM_ADMIN_KEY` | — | Platform management API key |
| `COMPLIANCE_MODE` | `standard` | `standard`, `hipaa`, `owasp`, `soc2` |
| `RUST_LOG` | — | Log level (`info`, `debug`, `trace`) |
| `GOOGLE_CLIENT_ID` | — | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | — | Google OAuth client secret |
| `GITHUB_CLIENT_ID` | — | GitHub OAuth client ID |
| `GITHUB_CLIENT_SECRET` | — | GitHub OAuth client secret |
| `ED25519_PRIVATE_KEY` | — | PEM key for JWT signing (auto-generated if missing) |
| `CSRF_SECRET` | auto-generated | HMAC secret for CSRF tokens |

---

## 11. Common Tasks

### Reset the local database

```bash
# Stop everything
docker compose down

# Remove volumes (deletes all data)
docker compose down -v

# Start fresh
docker compose up -d db db-test
```

### Make a user an admin

```bash
# Via psql
psql "postgres://gatewasm:gatewasm@localhost:5434/gatewasm" -c "
  INSERT INTO user_roles (user_id, role_id, tenant_id)
  SELECT u.id, 'role_admin', u.tenant_id
  FROM users u WHERE u.email = 'you@example.com'
  ON CONFLICT DO NOTHING;
"

# Or via Management API
curl -X POST http://localhost:3001/manage/users/<USER_ID>/roles \
  -H "Authorization: Bearer <MGMT_KEY>" \
  -H "Content-Type: application/json" \
  -d '{"role_id": "role_admin"}'
```

### Create a custom role and assign it

```bash
MGMT_KEY=<your_mgmt_key>

# Create "editor" role with specific permissions
curl -X POST http://localhost:3001/manage/roles \
  -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name":"editor","description":"Content editor","permissions":["user:read","user:write"]}'
# → returns { "id": "role_xxx", ... }

# Assign it to a user
curl -X POST http://localhost:3001/manage/users/<USER_ID>/roles \
  -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" \
  -d '{"role_id":"role_xxx"}'
```

### Set up a webhook

```bash
curl -X POST http://localhost:3001/manage/webhooks \
  -H "Authorization: Bearer $MGMT_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://webhook.site/your-unique-id",
    "events": ["user.registered", "user.logged_in"],
    "description": "Test webhook"
  }'
# → returns { "id": "wh_xxx", "secret": "whsec_xxx" } — save the secret!
```

Webhook payloads include an `X-GateWASM-Signature` header (HMAC-SHA256) for verification.

### View all tables

```bash
psql "postgres://gatewasm:gatewasm@localhost:5434/gatewasm" -c "\dt"
```

Or open http://localhost:8081 (pgweb).

### Rebuild after code changes

The server recompiles on `cargo run`. For faster iteration:

```bash
# Install cargo-watch for auto-reload
cargo install cargo-watch

# Auto-restart on file changes
cd auth-module-poc
cargo watch -x 'run -p gatewasm-server' \
  -i 'tests/*' -i 'bridge/*' -i 'infra/*'
```

### Deploy to AWS

```bash
cd auth-module-poc

# 1. Build image for linux/amd64
docker build --platform linux/amd64 -t gatewasm-server -f crates/gatewasm-server/Dockerfile ..

# 2. Tag for ECR
docker tag gatewasm-server:latest 877573132739.dkr.ecr.us-east-1.amazonaws.com/gatewasm-server:latest

# 3. Login to ECR
aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin 877573132739.dkr.ecr.us-east-1.amazonaws.com

# 4. Push (triggers App Runner auto-deploy)
docker push 877573132739.dkr.ecr.us-east-1.amazonaws.com/gatewasm-server:latest

# 5. Verify health
curl https://auth.getforma.dev/health
```

---

## 12. Troubleshooting

### "Failed to create database pool"

Database isn't running. Start it:

```bash
docker compose up -d db
```

### "address already in use"

Another process is using the port. Change `PORT`:

```bash
PORT=3101 cargo run -p gatewasm-server
```

### Port 5432 conflict

If you have a native Postgres installation, port 5432 will conflict. The docker-compose dev DB uses port **5434** to avoid this. If you still have issues:

```bash
# Check what's on port 5434
lsof -i :5434

# Or change the port in docker-compose.yml
```

### Tests fail with "connection refused"

Test database isn't running:

```bash
docker compose up -d db-test
```

### "relation does not exist"

Migrations haven't run. They run automatically on server start, or you can:

```bash
# Start the server briefly to run migrations, then Ctrl+C
cargo run -p gatewasm-server
```

### Cookie not being set in browser

- Check `DevTools → Network → Response Headers` for `Set-Cookie`
- Local dev uses `SameSite=Lax` (not `None`) because there's no HTTPS
- Cookie `Path=/` so it's sent on all requests
- `HttpOnly` means you can't see it in `document.cookie` (that's intentional)
