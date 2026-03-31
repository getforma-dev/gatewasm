# Console Redesign Phase 1: Onboarding Wizard + Sandbox Page

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a 3-step onboarding wizard for new tenants and a developer sandbox page that shows decoded auth data, all running in parallel to the existing console.

**Architecture:** The wizard lives inside `console-v2/` as a view mode of a new console app. The sandbox is a standalone tenant-scoped page at `/t/{slug}/sandbox`. Both are new entry points — the existing console is untouched. A redirect query param is added to the tenant login store so the sandbox (and later hosted page) can redirect back after auth.

**Tech Stack:** FormaJS (h, createSignal, createShow, mount), Rust/Axum (handlers, middleware, sqlx), PostgreSQL (migrations), esbuild (build config)

**Spec:** `docs/superpowers/specs/2026-03-31-console-redesign-design.md`

---

## File Map

### New Files (Frontend)

| File | Responsibility |
|------|----------------|
| `admin/src/platform/console-v2/app.ts` | Entry point: init auth → mount ConsoleApp |
| `admin/src/platform/console-v2/store.ts` | Console state: tenants, selected tenant, wizard state, auth |
| `admin/src/platform/console-v2/ConsoleApp.ts` | App shell: sidebar + wizard/console view router |
| `admin/src/platform/console-v2/wizard/WizardFlow.ts` | Wizard orchestrator: step state machine, renders current step |
| `admin/src/platform/console-v2/wizard/BrandingStep.ts` | Step 1: app name + color picker + live preview |
| `admin/src/platform/console-v2/wizard/TestAuthStep.ts` | Step 2: waiting state, polling, auto-open, success state |
| `admin/src/platform/console-v2/wizard/ChoosePathStep.ts` | Step 3: two cards, tab preference, go to console |
| `admin/src/tenant/sandbox/app.ts` | Entry point: auth check → mount SandboxPage |
| `admin/src/tenant/sandbox/store.ts` | Introspect API call, token decode, session data |
| `admin/src/tenant/sandbox/SandboxPage.ts` | Two-column layout: callback preview, profile, session, JWT |
| `admin/src/core/hooks/createPoller.ts` | Reusable polling hook with backoff |
| `admin/src/core/components/JwtViewer.ts` | Color-coded JWT decoder with raw toggle |
| `admin/src/core/components/CodeBlockTabs.ts` | Code block with language tabs (TS/JS/curl) |

### New Files (Rust)

| File | Responsibility |
|------|----------------|
| `crates/server/src/handlers/sandbox.rs` | GET /t/{slug}/sandbox page handler + cross-plane auth |
| `crates/server/src/handlers/introspect.rs` | GET /t/{slug}/auth/introspect API endpoint |
| `crates/server/src/db/pg_migrations/002_onboarding_fields.sql` | Add onboarding_completed, onboarding_step, preferred_tab to tenants |

### Modified Files

| File | Change |
|------|--------|
| `admin/build.ts` | Add console-v2 and sandbox entry points + routes |
| `admin/src/tenant/login/store.ts` | Redirect query param support after auth |
| `crates/server/src/handlers/mod.rs` | Register new routes: sandbox, introspect, console-v2 |
| `crates/server/src/handlers/onboarding.rs` | Enrich tenant_stats with latest_user, add onboarding fields to tenant CRUD |
| `crates/server/src/handlers/platform_dashboard.rs` | Add console-v2 page handler |

---

## Task 1: DB Migration — Onboarding Fields

**Files:**
- Create: `crates/server/src/db/pg_migrations/002_onboarding_fields.sql`
- Modify: `crates/server/src/db/mod.rs` (if migration loading is manual — check first)

- [ ] **Step 1: Write the migration SQL**

```sql
-- 002: Add onboarding fields to tenants
ALTER TABLE tenants
  ADD COLUMN IF NOT EXISTS onboarding_completed BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN IF NOT EXISTS onboarding_step SMALLINT NOT NULL DEFAULT 1,
  ADD COLUMN IF NOT EXISTS preferred_tab TEXT;
```

Save to `crates/server/src/db/pg_migrations/002_onboarding_fields.sql`.

- [ ] **Step 2: Verify migration runs on server start**

The server auto-runs migrations from the `pg_migrations/` directory on startup (see `001_initial.sql` pattern). Restart the dev server:

```bash
npm run dev
```

Check server log for migration application. Connect to the dev DB and verify:

```bash
docker compose exec db psql -U gatewasm -d gatewasm -c "\d tenants" | grep onboarding
```

Expected: `onboarding_completed`, `onboarding_step`, `preferred_tab` columns visible.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/db/pg_migrations/002_onboarding_fields.sql
git commit -m "feat: add onboarding fields to tenants table (wizard state persistence)"
```

---

## Task 2: Redirect Query Param in Tenant Login

**Files:**
- Modify: `admin/src/tenant/login/store.ts`

This is a Phase 1 prerequisite. The sandbox needs `/t/{slug}/login?redirect=/t/{slug}/sandbox` to work.

- [ ] **Step 1: Read the current post-auth redirect logic**

Read `admin/src/tenant/login/store.ts`. Find all places where `window.location.href = loginRedirectUrl` is used after successful auth. There are at least two: the OAuth callback handler (~line 148) and the email login success handler. Also check the silent refresh path.

- [ ] **Step 2: Add redirect query param support**

At the top of the store (near line 33 where `urlParams` is already defined), add:

```typescript
const redirectParam = urlParams.get('redirect') || '';
// Security: only allow same-tenant redirects (starts with /t/{slug}/)
const safeRedirect = redirectParam.startsWith('/t/' + slug + '/') ? redirectParam : '';
```

Then in every post-auth success path where `window.location.href = loginRedirectUrl` appears, change to:

```typescript
window.location.href = safeRedirect || loginRedirectUrl;
```

This applies to:
- OAuth callback success (~line 148)
- Email login success
- Silent refresh success that calls `routeAfterAuth()` or similar

The key constraint: the redirect MUST start with `/t/{slug}/` — no open redirects. If the redirect param points elsewhere, ignore it.

- [ ] **Step 3: Test manually**

With the dev server running, visit:
```
http://localhost:3001/t/forma/login?redirect=/t/forma/dashboard
```

Log in with `admin@local.dev` / `Test1234!`. After auth, verify you land on `/t/forma/dashboard` (not the default redirect). Then test without the param — should go to the default redirect.

- [ ] **Step 4: Commit**

```bash
git add admin/src/tenant/login/store.ts
git commit -m "feat: tenant login respects redirect query param (same-tenant paths only)"
```

---

## Task 3: Enrich Stats Endpoint with latest_user

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (the `tenant_stats` handler)

- [ ] **Step 1: Read the current tenant_stats handler**

Read `crates/server/src/handlers/onboarding.rs` and find the `tenant_stats` function. It currently returns counts (total_users, verified_users, etc.). We need to add a `latest_user` object when `total_users > 0`.

- [ ] **Step 2: Add latest_user query**

After the existing stat queries in `tenant_stats`, add:

```rust
// Latest user (for wizard success state)
let latest_user = if total_users > 0 {
    sqlx::query_as::<_, (String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT email, \
         COALESCE((SELECT lp.provider FROM login_attempts lp WHERE lp.user_id = u.id AND lp.success = TRUE ORDER BY lp.created_at DESC LIMIT 1), 'email') as provider, \
         u.created_at \
         FROM users u WHERE u.tenant_id = $1 ORDER BY u.created_at DESC LIMIT 1"
    )
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .map(|(email, provider, created_at)| serde_json::json!({
        "email": email,
        "provider": provider,
        "created_at": created_at,
    }))
} else {
    None
};
```

Then add `"latest_user": latest_user` to the JSON response body.

- [ ] **Step 3: Test with curl**

```bash
# Get a tenant ID
TENANT_ID=$(docker compose exec db psql -U gatewasm -d gatewasm -t -c "SELECT id FROM tenants WHERE slug='forma'" | tr -d ' ')

# Get platform token (login as platform user first, or use plat key)
curl -s "http://localhost:3001/platform/self-serve/tenants/$TENANT_ID/stats" \
  -H "Authorization: Bearer <platform_token>" | jq '.latest_user'
```

Expected: `{ "email": "admin@local.dev", "provider": "email", "created_at": "..." }` or `null` if no users.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs
git commit -m "feat: enrich tenant stats with latest_user for wizard success state"
```

---

## Task 4: Expose Onboarding Fields in Tenant CRUD

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (tenant detail + update handlers)

- [ ] **Step 1: Add onboarding fields to tenant detail response**

Find the handler that returns tenant detail (likely `tenant_detail` or `get_tenant`). Add `onboarding_completed`, `onboarding_step`, and `preferred_tab` to the JSON response.

- [ ] **Step 2: Accept onboarding fields in tenant update**

Find the PATCH handler for tenants (likely `configure_tenant` or `update_tenant`). Add `onboarding_completed`, `onboarding_step`, and `preferred_tab` to the accepted fields. These should be optional — only update if present in the request body.

- [ ] **Step 3: Test with curl**

```bash
# Update onboarding_step
curl -s -X PATCH "http://localhost:3001/platform/self-serve/tenants/$TENANT_ID" \
  -H "Authorization: Bearer <platform_token>" \
  -H "Content-Type: application/json" \
  -d '{"onboarding_step": 2}' | jq '{onboarding_completed, onboarding_step}'
```

Expected: `{ "onboarding_completed": false, "onboarding_step": 2 }`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs
git commit -m "feat: expose onboarding fields in tenant detail and update endpoints"
```

---

## Task 5: Auth Introspect Endpoint

**Files:**
- Create: `crates/server/src/handlers/introspect.rs`
- Modify: `crates/server/src/handlers/mod.rs` (register route)

- [ ] **Step 1: Create the introspect handler**

Create `crates/server/src/handlers/introspect.rs`:

```rust
//! GET /auth/introspect — Returns decoded JWT + session + user profile
//! for the current Plane 3 session. Used by the sandbox page.

use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use crate::{AppState, error::AppError};
use crate::middleware::auth::AuthUser;

/// Returns the current user's decoded token, session info, and profile.
/// Requires a valid tenant-scoped JWT (Plane 3 auth).
pub async fn introspect(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let user_id = &auth_user.user_id;
    let tenant_id = &auth_user.tenant_id;

    // User profile
    let profile = sqlx::query_as::<_, (String, String, Option<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT email, id, name, created_at FROM users WHERE id = $1 AND tenant_id = $2"
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "user_not_found",
        message: "User not found".into(),
    })?;

    // User roles
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT role_id FROM user_roles WHERE user_id = $1 AND tenant_id = $2"
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // Active session info
    let session = sqlx::query_as::<_, (String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, created_at, expires_at FROM refresh_tokens \
         WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE AND expires_at > NOW() \
         ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?;

    // Tenant slug (for display)
    let tenant_slug: String = sqlx::query_scalar(
        "SELECT slug FROM tenants WHERE id = $1"
    )
    .bind(tenant_id)
    .fetch_one(&state.db)
    .await?;

    // Build JWT claims from the auth token (already validated by middleware)
    let claims = &auth_user.claims;

    Ok(Json(serde_json::json!({
        "user": {
            "id": profile.1,
            "email": profile.0,
            "name": profile.2,
            "roles": roles,
            "created_at": profile.3,
        },
        "session": session.map(|(id, created, expires)| serde_json::json!({
            "id": id,
            "created_at": created,
            "expires_at": expires,
        })),
        "tenant": {
            "id": tenant_id,
            "slug": tenant_slug,
        },
        "token": {
            "issuer": claims.get("iss"),
            "audience": claims.get("aud"),
            "issued_at": claims.get("iat"),
            "expires_at": claims.get("exp"),
            "algorithm": "EdDSA",
        },
    })))
}
```

Note: The exact field names on `AuthUser` and `claims` depend on the existing auth middleware implementation. Read `crates/server/src/middleware/auth.rs` to get the exact struct fields before implementing. The handler above shows the pattern — adjust field access to match the actual types.

- [ ] **Step 2: Register the route**

In `crates/server/src/handlers/mod.rs`, add `pub mod introspect;` to the module declarations at the top. Then add the route to the `tenant_auth` router (since it needs tenant middleware + auth middleware):

```rust
.route("/auth/introspect", get(introspect::introspect))
```

Add it alongside the existing `/auth/me` route in the `protected` router section (it requires the same auth level).

- [ ] **Step 3: Test with curl**

```bash
# Login to get a token
TOKEN=$(curl -s -X POST http://localhost:3001/auth/login \
  -H "X-Tenant-Slug: forma" \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@local.dev","password":"Test1234!"}' | jq -r '.access_token')

# Introspect
curl -s http://localhost:3001/auth/introspect \
  -H "X-Tenant-Slug: forma" \
  -H "Authorization: Bearer $TOKEN" | jq
```

Expected: JSON with `user`, `session`, `tenant`, `token` objects.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/introspect.rs crates/server/src/handlers/mod.rs
git commit -m "feat: add /auth/introspect endpoint for sandbox page"
```

---

## Task 6: Sandbox Page — Rust Route + Cross-Plane Auth

**Files:**
- Create: `crates/server/src/handlers/sandbox.rs`
- Modify: `crates/server/src/handlers/mod.rs` (register route)

This is architecturally novel — the first route that checks both Plane 3 (tenant session) AND Plane 2 (platform membership).

- [ ] **Step 1: Create the sandbox page handler**

Create `crates/server/src/handlers/sandbox.rs`:

```rust
//! GET /sandbox — Developer debug page showing decoded auth data.
//!
//! Access control (cross-plane):
//! 1. Must be authenticated as tenant end-user (Plane 3 session cookie)
//! 2. Must also be a platform member of this tenant (Plane 2 check)
//! If only Plane 3: redirect to /t/{slug}/site (regular end-user)
//! If not authenticated: redirect to /t/{slug}/login?redirect=/t/{slug}/sandbox

use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
};
use crate::AppState;
use crate::middleware::auth::AuthUser;
use crate::middleware::tenant::ResolvedTenant;
use crate::html_template::render_page;
use forma_server::types::{PageConfig, RenderMode};

pub async fn sandbox_page(
    State(state): State<AppState>,
    tenant: ResolvedTenant,
    auth_user: Option<AuthUser>,
) -> impl IntoResponse {
    let slug = &tenant.slug;

    // Check Plane 3 auth
    let user = match auth_user {
        Some(u) => u,
        None => {
            // Not authenticated — redirect to login with redirect back
            return Redirect::temporary(
                &format!("/t/{}/login?redirect=/t/{}/sandbox", slug, slug)
            ).into_response();
        }
    };

    // Check Plane 2 membership — is this user a platform member of this tenant?
    // Query: does a platform_accounts row exist for this user's email,
    // AND does that platform account own this tenant?
    let is_platform_member = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM tenants t
            JOIN platform_accounts pa ON pa.id = t.owner_user_id
            JOIN users u ON u.email = pa.email
            WHERE t.id = $1 AND u.id = $2
        )"
    )
    .bind(&tenant.id)
    .bind(&user.user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if !is_platform_member {
        // Regular end-user — redirect to site (or dashboard)
        return Redirect::temporary(
            &format!("/t/{}/dashboard", slug)
        ).into_response();
    }

    // Render sandbox page
    let config_script = format!(
        "window.__TENANT__={};window.__FORMA_SERVER_URL__=\"{}\";",
        serde_json::json!({
            "slug": slug,
            "name": tenant.name,
        }),
        state.config.server_url,
    );

    let route_pattern = "/t/:slug/sandbox";
    let page = render_page(&PageConfig {
        title: &format!("{} — Sandbox", tenant.name),
        route_pattern,
        manifest: &state.asset_manifest,
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;}"),
        body_prefix: None,
        render_mode: RenderMode::Phase1ClientMount,
        ir_module: None,
        slots: None,
    });

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (axum::http::header::HeaderName::from_static("content-security-policy"), page.csp),
        ],
        axum::response::Html(page.html),
    ).into_response()
}
```

Note: The exact types for `ResolvedTenant` and `AuthUser` and the Plane 2 ownership query depend on the existing schema. Read `middleware/tenant.rs` and `middleware/auth.rs` to get the exact extractor types and field names. The cross-plane query above shows the pattern — the exact SQL may need adjustment based on how `platform_accounts` relates to `tenants` and `users`. The key constraint: the user must be both a Plane 3 end-user AND a Plane 2 owner/member of this tenant.

- [ ] **Step 2: Register the sandbox route**

In `crates/server/src/handlers/mod.rs`, add `pub mod sandbox;` to module declarations. Add the route in the `tenant_auth` section (it needs tenant middleware for slug resolution):

```rust
.route("/sandbox", get(sandbox::sandbox_page))
```

This goes alongside `/login`, `/dashboard`, `/admin` in the tenant-scoped routes.

- [ ] **Step 3: Verify the route responds**

```bash
# Should redirect to login (not authenticated)
curl -s -o /dev/null -w "%{http_code} %{redirect_url}" http://localhost:3001/t/forma/sandbox
```

Expected: `307 http://localhost:3001/t/forma/login?redirect=/t/forma/sandbox`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/sandbox.rs crates/server/src/handlers/mod.rs
git commit -m "feat: add /t/{slug}/sandbox route with cross-plane auth gate"
```

---

## Task 7: Console-v2 Page Route (Rust)

**Files:**
- Modify: `crates/server/src/handlers/platform_dashboard.rs`
- Modify: `crates/server/src/handlers/mod.rs`

- [ ] **Step 1: Add console-v2 page handler**

In `crates/server/src/handlers/platform_dashboard.rs`, add a new handler that follows the same pattern as `dashboard_page` but uses the console-v2 route pattern and JS bundle:

```rust
/// GET /platform/console-v2 — New console (development only, swaps to /platform/console later)
pub async fn dashboard_page_v2(State(state): State<AppState>) -> impl IntoResponse {
    let config_script = format!(
        "window.__FORMA_SERVER_URL__=\"{}\";",
        state.config.server_url
    );

    let page = render_page(&PageConfig {
        title: "Platform Console — Forma Auth",
        route_pattern: "/platform/console-v2",
        manifest: &state.asset_manifest,
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;}"),
        body_prefix: None,
        render_mode: RenderMode::Phase1ClientMount,
        ir_module: None,
        slots: None,
    });

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (axum::http::header::HeaderName::from_static("content-security-policy"), page.csp),
        ],
        Html(page.html),
    ).into_response()
}
```

- [ ] **Step 2: Register the route**

In `mod.rs`, add alongside the existing `/platform/console` route:

```rust
.route("/platform/console-v2", get(platform_dashboard::dashboard_page_v2))
```

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/handlers/platform_dashboard.rs crates/server/src/handlers/mod.rs
git commit -m "feat: add /platform/console-v2 dev route"
```

---

## Task 8: Build Config — New Entry Points

**Files:**
- Modify: `admin/build.ts`

- [ ] **Step 1: Add console-v2 and sandbox entry points**

In `admin/build.ts`, add to the `entryPoints` array:

```typescript
{ entry: 'src/platform/console-v2/app.ts', outfile: 'platform-console-v2.js' },
{ entry: 'src/tenant/sandbox/app.ts', outfile: 'tenant-sandbox.js' },
```

Add to the `routes` object:

```typescript
'/platform/console-v2': { js: ['platform-console-v2'], css: ['forma-platform'] },
'/t/:slug/sandbox':     { js: ['tenant-sandbox'],      css: ['forma-platform'] },
```

- [ ] **Step 2: Commit**

```bash
git add admin/build.ts
git commit -m "feat: add console-v2 and sandbox to build config"
```

Note: The build will fail until we create the actual app.ts files in the next tasks. That's expected.

---

## Task 9: createPoller Hook

**Files:**
- Create: `admin/src/core/hooks/createPoller.ts`

A reusable polling hook with backoff, used by the wizard's TestAuthStep.

- [ ] **Step 1: Create the hook**

```typescript
import { createSignal, onUnmount } from 'formajs';

export interface PollerOptions {
  /** Initial interval in ms */
  initialMs: number;
  /** Interval after backoff kicks in */
  backoffMs: number;
  /** Time in ms before switching to backoff interval */
  backoffAfterMs: number;
  /** Polling function — return true to stop polling */
  fn: () => Promise<boolean>;
}

export function createPoller(opts: PollerOptions) {
  const [active, setActive] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let startTime = 0;

  function currentInterval(): number {
    const elapsed = Date.now() - startTime;
    return elapsed > opts.backoffAfterMs ? opts.backoffMs : opts.initialMs;
  }

  async function tick() {
    if (!active()) return;
    try {
      const done = await opts.fn();
      if (done) {
        stop();
        return;
      }
    } catch {
      // Swallow errors — keep polling
    }
    if (active()) {
      timer = setTimeout(tick, currentInterval());
    }
  }

  function start() {
    if (active()) return;
    startTime = Date.now();
    setActive(true);
    tick();
  }

  function stop() {
    setActive(false);
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  onUnmount(stop);

  return { start, stop, active };
}
```

- [ ] **Step 2: Commit**

```bash
git add admin/src/core/hooks/createPoller.ts
git commit -m "feat: add createPoller hook with backoff for wizard polling"
```

---

## Task 10: Console-v2 Store

**Files:**
- Create: `admin/src/platform/console-v2/store.ts`

- [ ] **Step 1: Create the store**

The store manages: auth, tenant list, selected tenant, wizard state. It imports from shared hooks.

```typescript
import { createSignal } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createToast } from '../../core/hooks/createToast';

// ── Config ────────────────────────────────────────────────────────────
const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;

// ── Auth ──────────────────────────────────────────────────────────────
const auth = createAuth({ serverUrl, tenant: 'platform' });
export const toast = createToast();

// ── State ─────────────────────────────────────────────────────────────
export interface Tenant {
  id: string;
  name: string;
  slug: string;
  plan: string;
  status: string;
  onboarding_completed: boolean;
  onboarding_step: number;
  preferred_tab: string | null;
  primary_color: string | null;
  login_redirect_url: string | null;
  management_api_key?: string;
  created_at: string;
}

export interface TenantStats {
  total_users: number;
  active_sessions: number;
  logins_24h: number;
  latest_user: { email: string; provider: string; created_at: string } | null;
}

export const [tenants, setTenants] = createSignal<Tenant[]>([]);
export const [selectedTenant, setSelectedTenant] = createSignal<Tenant | null>(null);
export const [selectedTenantStats, setSelectedTenantStats] = createSignal<TenantStats | null>(null);
export const [loading, setLoading] = createSignal(false);
export const [view, setView] = createSignal<'tenants' | 'wizard' | 'detail'>('tenants');

// ── API helpers ───────────────────────────────────────────────────────
async function api(method: string, path: string, body?: any): Promise<any> {
  const res = await fetch(serverUrl + path, {
    method,
    headers: {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer ' + auth.accessToken(),
    },
    credentials: 'include',
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.message || 'API error');
  }
  return res.json();
}

// ── Data loading ──────────────────────────────────────────────────────
export async function loadTenants(): Promise<void> {
  setLoading(true);
  try {
    const data = await api('GET', '/platform/self-serve/tenants');
    setTenants(data.tenants || data);
  } catch (e: any) {
    toast.error('Failed to load tenants: ' + e.message);
  } finally {
    setLoading(false);
  }
}

export async function loadTenantDetail(id: string): Promise<void> {
  setLoading(true);
  try {
    const [detail, stats] = await Promise.all([
      api('GET', '/platform/self-serve/tenants/' + id),
      api('GET', '/platform/self-serve/tenants/' + id + '/stats').catch(() => null),
    ]);
    setSelectedTenant(detail);
    setSelectedTenantStats(stats);
  } catch (e: any) {
    toast.error('Failed to load tenant: ' + e.message);
  } finally {
    setLoading(false);
  }
}

export async function updateTenant(id: string, fields: Record<string, any>): Promise<void> {
  try {
    const updated = await api('PATCH', '/platform/self-serve/tenants/' + id, fields);
    setSelectedTenant(updated);
  } catch (e: any) {
    toast.error('Failed to update tenant: ' + e.message);
  }
}

export async function pollTenantStats(id: string): Promise<TenantStats> {
  return api('GET', '/platform/self-serve/tenants/' + id + '/stats');
}

// ── Navigation ────────────────────────────────────────────────────────
export function selectTenant(tenant: Tenant): void {
  setSelectedTenant(tenant);
  setSelectedTenantStats(null);
  if (!tenant.onboarding_completed) {
    setView('wizard');
  } else {
    setView('detail');
    loadTenantDetail(tenant.id);
  }
}

export function backToTenants(): void {
  setSelectedTenant(null);
  setSelectedTenantStats(null);
  setView('tenants');
}

// ── Init ──────────────────────────────────────────────────────────────
export async function init(): Promise<void> {
  await auth.silentRefresh();
  auth.startAutoRefresh();
  await loadTenants();

  // If there's exactly one tenant and it hasn't completed onboarding, auto-select it
  const t = tenants();
  if (t.length === 1 && !t[0].onboarding_completed) {
    selectTenant(t[0]);
  }
}

export function signout(): void {
  auth.logout();
  window.location.href = '/platform/login';
}
```

- [ ] **Step 2: Commit**

```bash
git add admin/src/platform/console-v2/store.ts
git commit -m "feat: console-v2 store with auth, tenant CRUD, wizard state"
```

---

## Task 11: Console-v2 App Shell

**Files:**
- Create: `admin/src/platform/console-v2/app.ts`
- Create: `admin/src/platform/console-v2/ConsoleApp.ts`

- [ ] **Step 1: Create app.ts**

```typescript
import { mount } from 'formajs';
import { ConsoleApp } from './ConsoleApp';
import { init } from './store';

init()
  .then(() => mount(() => ConsoleApp(), '#app'))
  .catch(() => {});
```

- [ ] **Step 2: Create ConsoleApp.ts**

```typescript
import { h, createSwitch } from 'formajs';
import { Sidebar } from '../../core/components/Sidebar';
import { LoadingBar } from '../../core/components/LoadingBar';
import { ToastView } from '../../core/components/ToastView';
import { view, setView, loading, toast, backToTenants, signout, loadTenants } from './store';
import { WizardFlow } from './wizard/WizardFlow';

// Placeholder views for Phase 2 — just the wizard and a stub for now
function TenantsStub() {
  return h('div', null, h('p', null, 'Tenants list — Phase 2'));
}

function DetailStub() {
  return h('div', null, h('p', null, 'Tenant detail — Phase 2'));
}

export function ConsoleApp() {
  const sidebarItems = [
    { id: 'tenants', label: 'Tenants' },
  ];
  const sidebarFooter = [
    { id: 'refresh', label: 'Refresh' },
    { id: 'signout', label: 'Sign Out' },
  ];

  function onNav(id: string) {
    if (id === 'refresh') { loadTenants(); return; }
    if (id === 'signout') { signout(); return; }
    if (id === 'tenants') { backToTenants(); return; }
  }

  return h('div', { class: 'shell' },
    Sidebar({
      brand: 'Forma Auth',
      title: 'PLATFORM CONSOLE',
      items: sidebarItems,
      footer: sidebarFooter,
      activeId: () => view() === 'tenants' ? 'tenants' : '',
      onNavigate: onNav,
    }),
    h('main', { class: 'main' },
      LoadingBar({ active: loading }),
      createSwitch(
        () => view(),
        {
          tenants: TenantsStub,
          wizard: WizardFlow,
          detail: DetailStub,
        },
      ),
      ToastView({ toast }),
    ),
  );
}
```

Note: The `Sidebar`, `LoadingBar`, `ToastView` imports depend on the exact export names from `core/components/`. Check the actual files and adjust import paths. The `createSwitch` call matches the pattern used in the existing `ConsoleApp.ts`.

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console-v2/app.ts admin/src/platform/console-v2/ConsoleApp.ts
git commit -m "feat: console-v2 app shell with sidebar and view router"
```

---

## Task 12: WizardFlow Orchestrator

**Files:**
- Create: `admin/src/platform/console-v2/wizard/WizardFlow.ts`

- [ ] **Step 1: Create the wizard flow**

```typescript
import { h, createSignal, createSwitch } from 'formajs';
import { selectedTenant } from '../store';
import { BrandingStep } from './BrandingStep';
import { TestAuthStep } from './TestAuthStep';
import { ChoosePathStep } from './ChoosePathStep';

export function WizardFlow() {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  // Resume from persisted step
  const [step, setStep] = createSignal<1 | 2 | 3>(
    (tenant.onboarding_step as 1 | 2 | 3) || 1
  );

  function advance() {
    const next = (step() + 1) as 1 | 2 | 3;
    if (next <= 3) setStep(next);
  }

  return h('div', { class: 'wizard' },
    createSwitch(
      () => String(step()),
      {
        '1': () => BrandingStep({ tenant, onNext: advance }),
        '2': () => TestAuthStep({ tenant, onNext: advance }),
        '3': () => ChoosePathStep({ tenant }),
      },
    ),
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add admin/src/platform/console-v2/wizard/WizardFlow.ts
git commit -m "feat: wizard flow orchestrator with step state machine"
```

---

## Task 13: BrandingStep (Step 1)

**Files:**
- Create: `admin/src/platform/console-v2/wizard/BrandingStep.ts`

- [ ] **Step 1: Create step 1 component**

```typescript
import { h, createSignal } from 'formajs';
import { updateTenant, type Tenant } from '../store';

interface BrandingStepProps {
  tenant: Tenant;
  onNext: () => void;
}

export function BrandingStep({ tenant, onNext }: BrandingStepProps) {
  const [appName, setAppName] = createSignal(tenant.name || '');
  const [color, setColor] = createSignal(tenant.primary_color || '#6366f1');
  const [saving, setSaving] = createSignal(false);

  async function handleNext() {
    setSaving(true);
    await updateTenant(tenant.id, {
      name: appName(),
      primary_color: color(),
      onboarding_step: 2,
    });
    setSaving(false);
    onNext();
  }

  return h('div', { class: 'wizard-step' },
    h('div', { class: 'wizard-step-header' },
      h('div', { class: 'wizard-step-counter' }, 'Step 1 of 3'),
      h('h2', { class: 'wizard-step-title' }, 'Make it yours'),
      h('p', { class: 'wizard-step-subtitle' }, 'Give your login page a name and color. Takes five seconds.'),
    ),
    h('div', { class: 'wizard-step-body' },
      h('div', { class: 'field' },
        h('label', null, 'App Name'),
        h('input', {
          type: 'text',
          value: appName,
          onInput: (e: Event) => setAppName((e.target as HTMLInputElement).value),
        }),
      ),
      h('div', { class: 'field' },
        h('label', null, 'Primary Color'),
        h('div', { class: 'flex items-center gap-sm' },
          h('input', {
            type: 'color',
            value: color,
            onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
            style: 'width:36px;height:36px;border:none;padding:0;cursor:pointer;',
          }),
          h('input', {
            type: 'text',
            value: color,
            onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
            style: 'flex:1;',
          }),
        ),
      ),
      // Live preview
      h('div', { class: 'wizard-preview' },
        h('div', {
          class: 'wizard-preview-card',
          style: () => `border-top: 3px solid ${color()};`,
        },
          h('div', {
            class: 'wizard-preview-name',
            style: () => `color: ${color()};`,
          }, () => appName() || 'Your App'),
          h('div', { class: 'wizard-preview-subtitle' }, 'Sign in to your account'),
          h('div', { class: 'wizard-preview-field' }),
          h('div', { class: 'wizard-preview-field' }),
          h('div', {
            class: 'wizard-preview-btn',
            style: () => `background: ${color()};`,
          }, 'Sign in'),
        ),
      ),
    ),
    h('div', { class: 'wizard-step-footer' },
      h('button', {
        class: 'btn btn-primary',
        disabled: () => saving() || !appName().trim(),
        onClick: handleNext,
      }, () => saving() ? 'Saving...' : 'Next →'),
    ),
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add admin/src/platform/console-v2/wizard/BrandingStep.ts
git commit -m "feat: wizard step 1 — branding (app name, color, live preview)"
```

---

## Task 14: TestAuthStep (Step 2)

**Files:**
- Create: `admin/src/platform/console-v2/wizard/TestAuthStep.ts`

- [ ] **Step 1: Create step 2 component**

```typescript
import { h, createSignal, createShow } from 'formajs';
import { createPoller } from '../../../core/hooks/createPoller';
import { pollTenantStats, updateTenant, type Tenant, type TenantStats } from '../store';

interface TestAuthStepProps {
  tenant: Tenant;
  onNext: () => void;
}

export function TestAuthStep({ tenant, onNext }: TestAuthStepProps) {
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const loginUrl = `${serverUrl}/t/${tenant.slug}/login`;
  const sandboxUrl = `${serverUrl}/t/${tenant.slug}/sandbox`;
  const [success, setSuccess] = createSignal(false);
  const [latestUser, setLatestUser] = createSignal<TenantStats['latest_user']>(null);

  // Auto-open login page on mount
  const opened = window.open(loginUrl, '_blank');
  if (!opened) {
    // Popup blocked — user will use the manual button
  }

  // Poll for first auth event
  const poller = createPoller({
    initialMs: 2000,
    backoffMs: 5000,
    backoffAfterMs: 30000,
    fn: async () => {
      const stats = await pollTenantStats(tenant.id);
      if (stats.total_users > 0 && stats.latest_user) {
        setLatestUser(stats.latest_user);
        setSuccess(true);
        return true; // stop polling
      }
      return false;
    },
  });

  poller.start();

  async function handleNext() {
    await updateTenant(tenant.id, { onboarding_step: 3 });
    onNext();
  }

  return h('div', { class: 'wizard-step' },
    h('div', { class: 'wizard-step-header' },
      h('div', { class: 'wizard-step-counter' }, 'Step 2 of 3'),
      h('h2', { class: 'wizard-step-title' }, () => success() ? 'It works' : 'See it work'),
      createShow(
        () => !success(),
        () => h('p', { class: 'wizard-step-subtitle' },
          'Open your login page in a new tab, register as a test user, and watch the magic.',
        ),
      ),
    ),
    h('div', { class: 'wizard-step-body' },
      // Waiting state
      createShow(
        () => !success(),
        () => h('div', { class: 'wizard-waiting' },
          h('div', { class: 'wizard-url-bar' },
            h('code', null, loginUrl),
            h('button', {
              class: 'btn btn-ghost btn-sm',
              onClick: () => navigator.clipboard.writeText(loginUrl),
            }, 'Copy'),
            h('a', {
              class: 'btn btn-primary btn-sm',
              href: loginUrl,
              target: '_blank',
              rel: 'noopener',
            }, 'Open login page ↗'),
          ),
          h('div', { class: 'wizard-pulse' },
            h('div', { class: 'wizard-pulse-dot' }),
            h('p', { class: 'text-muted' }, 'Waiting for your first login...'),
            h('p', { class: 'text-muted-dim text-xs' },
              'Register on your login page. This console will advance automatically.',
            ),
          ),
        ),
      ),
      // Success state
      createShow(
        () => success(),
        () => h('div', { class: 'wizard-success' },
          h('div', { class: 'wizard-success-card' },
            h('div', { class: 'wizard-success-label' }, 'First user authenticated'),
            h('div', { class: 'wizard-success-email' }, () => latestUser()?.email || ''),
            h('div', { class: 'wizard-success-meta' },
              () => {
                const u = latestUser();
                if (!u) return '';
                const date = new Date(u.created_at).toLocaleString();
                return `${date} — ${u.provider} provider`;
              },
            ),
          ),
          h('a', {
            class: 'btn-link text-sm',
            href: sandboxUrl,
            target: '_blank',
            rel: 'noopener',
          }, 'See what your app receives →'),
        ),
      ),
    ),
    // Footer (only in success state)
    createShow(
      () => success(),
      () => h('div', { class: 'wizard-step-footer' },
        h('button', {
          class: 'btn btn-primary',
          onClick: handleNext,
        }, 'Next →'),
      ),
    ),
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add admin/src/platform/console-v2/wizard/TestAuthStep.ts
git commit -m "feat: wizard step 2 — test auth with polling, auto-open, success state"
```

---

## Task 15: ChoosePathStep (Step 3)

**Files:**
- Create: `admin/src/platform/console-v2/wizard/ChoosePathStep.ts`

- [ ] **Step 1: Create step 3 component**

```typescript
import { h, createSignal } from 'formajs';
import { updateTenant, setView, type Tenant } from '../store';

interface ChoosePathStepProps {
  tenant: Tenant;
}

export function ChoosePathStep({ tenant }: ChoosePathStepProps) {
  const [selected, setSelected] = createSignal<'hosted-page' | 'integrate' | null>(null);
  const [saving, setSaving] = createSignal(false);

  async function handleFinish() {
    const choice = selected();
    if (!choice) return;
    setSaving(true);
    await updateTenant(tenant.id, {
      onboarding_completed: true,
      preferred_tab: choice,
    });
    setSaving(false);
    // Transition to full console view
    setView('detail');
  }

  function cardClass(option: string): string {
    return 'wizard-path-card' + (selected() === option ? ' selected' : '');
  }

  return h('div', { class: 'wizard-step' },
    h('div', { class: 'wizard-step-header' },
      h('div', { class: 'wizard-step-counter' }, 'Step 3 of 3'),
      h('h2', { class: 'wizard-step-title' }, 'What do you want to do next?'),
      h('p', { class: 'wizard-step-subtitle' }, 'Both options are always available in your console. Pick where to start.'),
    ),
    h('div', { class: 'wizard-step-body' },
      h('div', { class: 'wizard-paths' },
        h('div', {
          class: () => cardClass('hosted-page'),
          onClick: () => setSelected('hosted-page'),
        },
          h('h3', null, 'Host it here'),
          h('p', null, "Don't have an app yet? We'll host an auth-gated page for you. Free forever."),
        ),
        h('div', {
          class: () => cardClass('integrate'),
          onClick: () => setSelected('integrate'),
        },
          h('h3', null, 'Add it to your app'),
          h('p', null, 'Already have an app? Integrate auth in three steps with bridge.js.'),
        ),
      ),
      h('p', { class: 'text-muted-dim text-xs text-center' },
        'You can switch anytime from your console.',
      ),
    ),
    h('div', { class: 'wizard-step-footer' },
      h('button', {
        class: 'btn btn-primary',
        disabled: () => !selected() || saving(),
        onClick: handleFinish,
      }, () => saving() ? 'Saving...' : 'Go to Console →'),
    ),
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add admin/src/platform/console-v2/wizard/ChoosePathStep.ts
git commit -m "feat: wizard step 3 — choose path (first-run tab preference)"
```

---

## Task 16: Sandbox Store

**Files:**
- Create: `admin/src/tenant/sandbox/store.ts`

- [ ] **Step 1: Create the sandbox store**

```typescript
import { createSignal } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';

const config = (window as any).__TENANT__ || {};
const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const slug: string = config.slug || '';

const auth = createAuth({ serverUrl, tenant: slug });

export interface IntrospectData {
  user: { id: string; email: string; name: string | null; roles: string[]; created_at: string };
  session: { id: string; created_at: string; expires_at: string } | null;
  tenant: { id: string; slug: string };
  token: { issuer: string; audience: string; issued_at: number; expires_at: number; algorithm: string };
}

export const [data, setData] = createSignal<IntrospectData | null>(null);
export const [error, setError] = createSignal<string | null>(null);
export const [rawToken, setRawToken] = createSignal<string | null>(null);
export const tenantName: string = config.name || slug;
export { slug, serverUrl };

export async function init(): Promise<void> {
  try {
    const token = await auth.silentRefresh();
    setRawToken(token);

    const res = await fetch(serverUrl + '/t/' + slug + '/auth/introspect', {
      headers: { 'Authorization': 'Bearer ' + token },
      credentials: 'include',
    });
    if (!res.ok) throw new Error('introspect_failed');
    const introspect = await res.json();
    setData(introspect);
  } catch (e: any) {
    setError(e.message || 'Failed to load auth data');
  }
}

export function logout(): void {
  auth.logout();
  window.location.href = '/t/' + slug + '/login?redirect=/t/' + slug + '/sandbox';
}
```

- [ ] **Step 2: Commit**

```bash
git add admin/src/tenant/sandbox/store.ts
git commit -m "feat: sandbox store with introspect API and token management"
```

---

## Task 17: Sandbox Page Component

**Files:**
- Create: `admin/src/tenant/sandbox/app.ts`
- Create: `admin/src/tenant/sandbox/SandboxPage.ts`

- [ ] **Step 1: Create app.ts**

```typescript
import { mount } from 'formajs';
import { SandboxPage } from './SandboxPage';
import { init } from './store';

init()
  .then(() => mount(() => SandboxPage(), '#app'))
  .catch(() => {});
```

- [ ] **Step 2: Create SandboxPage.ts**

This is the two-column layout. Lead with "Your app receives this" (callback preview), then profile, session, JWT.

```typescript
import { h, createSignal, createShow } from 'formajs';
import { data, error, rawToken, tenantName, slug, serverUrl, logout } from './store';

function formatExpiry(expiresAt: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = expiresAt - now;
  if (diff <= 0) return 'Expired';
  const mins = Math.floor(diff / 60);
  return `Expires in ${mins} minute${mins !== 1 ? 's' : ''}`;
}

function CallbackPreview() {
  const [lang, setLang] = createSignal<'js' | 'ts' | 'curl'>('js');
  const d = data();
  if (!d) return h('div', null);

  function jsCode(): string {
    return `bridge.onStateChange(({ state, user }) => {\n  // state: "authenticated"\n  // user:\n  ${JSON.stringify({
      id: d.user.id,
      email: d.user.email,
      name: d.user.name,
      roles: d.user.roles,
      tenant: d.tenant.slug,
    }, null, 2).split('\n').join('\n  ')}\n});`;
  }

  function tsCode(): string {
    return `bridge.onStateChange(({ state, user }: AuthEvent) => {\n  // state: "authenticated"\n  // user: AuthUser\n  ${JSON.stringify({
      id: d.user.id,
      email: d.user.email,
      name: d.user.name,
      roles: d.user.roles,
      tenant: d.tenant.slug,
    }, null, 2).split('\n').join('\n  ')}\n});`;
  }

  function curlCode(): string {
    return `curl ${serverUrl}/t/${slug}/auth/me \\\n  -H "Authorization: Bearer <access_token>"`;
  }

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-header' },
      h('span', { class: 'sandbox-section-label' }, 'Your app receives this'),
      h('div', { class: 'sandbox-lang-tabs' },
        h('button', { class: () => 'btn-tab' + (lang() === 'js' ? ' active' : ''), onClick: () => setLang('js') }, 'JavaScript'),
        h('button', { class: () => 'btn-tab' + (lang() === 'ts' ? ' active' : ''), onClick: () => setLang('ts') }, 'TypeScript'),
        h('button', { class: () => 'btn-tab' + (lang() === 'curl' ? ' active' : ''), onClick: () => setLang('curl') }, 'curl'),
      ),
      h('button', {
        class: 'btn btn-ghost btn-sm',
        onClick: () => {
          const code = lang() === 'ts' ? tsCode() : lang() === 'curl' ? curlCode() : jsCode();
          navigator.clipboard.writeText(code);
        },
      }, 'Copy'),
    ),
    h('pre', { class: 'sandbox-code' },
      () => lang() === 'ts' ? tsCode() : lang() === 'curl' ? curlCode() : jsCode(),
    ),
  );
}

function UserProfile() {
  const d = data();
  if (!d) return h('div', null);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-label' }, 'User Profile'),
    h('div', { class: 'sandbox-grid' },
      h('span', { class: 'sandbox-key' }, 'Email'), h('span', null, d.user.email),
      h('span', { class: 'sandbox-key' }, 'User ID'), h('code', null, d.user.id),
      h('span', { class: 'sandbox-key' }, 'Name'), h('span', null, d.user.name || '—'),
      h('span', { class: 'sandbox-key' }, 'Roles'), h('span', null, d.user.roles.join(', ') || 'user'),
      h('span', { class: 'sandbox-key' }, 'Created'), h('span', null, new Date(d.user.created_at).toLocaleString()),
    ),
  );
}

function SessionInfo() {
  const d = data();
  if (!d || !d.session) return h('div', null);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-label' }, 'Session'),
    h('div', { class: 'sandbox-grid' },
      h('span', { class: 'sandbox-key' }, 'Session ID'), h('code', null, d.session.id),
      h('span', { class: 'sandbox-key' }, 'Created'), h('span', null, new Date(d.session.created_at).toLocaleString()),
      h('span', { class: 'sandbox-key' }, 'Expires'), h('span', null, formatExpiry(d.token.expires_at)),
    ),
  );
}

function JwtViewer() {
  const d = data();
  const [showRaw, setShowRaw] = createSignal(false);
  if (!d) return h('div', null);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-header' },
      h('span', { class: 'sandbox-section-label' }, 'Access Token (JWT)'),
      h('div', { class: 'flex gap-xs' },
        h('button', {
          class: 'btn btn-ghost btn-sm',
          onClick: () => navigator.clipboard.writeText(rawToken() || ''),
        }, 'Copy'),
        h('button', {
          class: 'btn btn-ghost btn-sm',
          onClick: () => setShowRaw(!showRaw()),
        }, () => showRaw() ? 'Show decoded' : 'Show raw'),
      ),
    ),
    createShow(
      () => showRaw(),
      () => h('pre', { class: 'sandbox-code sandbox-raw-token' }, rawToken() || ''),
    ),
    createShow(
      () => !showRaw(),
      () => h('div', null,
        h('div', { class: 'sandbox-jwt-section sandbox-jwt-header' },
          h('div', { class: 'sandbox-jwt-label' }, 'Header'),
          h('pre', null, JSON.stringify({ alg: d.token.algorithm, typ: 'JWT' }, null, 2)),
        ),
        h('div', { class: 'sandbox-jwt-section sandbox-jwt-payload' },
          h('div', { class: 'sandbox-jwt-label' }, 'Payload'),
          h('pre', null, JSON.stringify({
            sub: d.user.id,
            email: d.user.email,
            name: d.user.name,
            tenant_id: d.tenant.id,
            tenant_slug: d.tenant.slug,
            roles: d.user.roles,
            iss: d.token.issuer,
            aud: d.token.audience,
            iat: d.token.issued_at,
            exp: d.token.expires_at,
          }, null, 2)),
        ),
        h('div', { class: 'sandbox-jwt-section sandbox-jwt-signature' },
          h('div', { class: 'sandbox-jwt-label' }, 'Signature'),
          h('div', { class: 'sandbox-jwt-verified' }, `${d.token.algorithm} — verified ✓`),
        ),
      ),
    ),
    // Collapsed refresh token section
    h('details', { class: 'sandbox-refresh-token' },
      h('summary', null, 'Refresh Token'),
      h('p', { class: 'text-muted-dim text-xs' },
        'Managed by bridge.js — your app never touches this directly. Token refresh happens automatically in the background.',
      ),
    ),
  );
}

export function SandboxPage() {
  return h('div', { class: 'sandbox-page' },
    // Banner
    h('div', { class: 'sandbox-banner' },
      h('span', null,
        'You\'re viewing this as an end user of ',
        h('strong', null, tenantName),
        ' — this is what your customers\' auth data looks like.',
      ),
      h('a', {
        class: 'btn btn-ghost btn-sm',
        href: '/platform/console-v2',
      }, 'Back to Console ↗'),
    ),
    // Error state
    createShow(
      () => !!error(),
      () => h('div', { class: 'sandbox-error' }, () => error() || ''),
    ),
    // Main content
    createShow(
      () => !!data(),
      () => h('div', { class: 'sandbox-columns' },
        h('div', { class: 'sandbox-left' },
          CallbackPreview(),
          UserProfile(),
          SessionInfo(),
        ),
        h('div', { class: 'sandbox-right' },
          JwtViewer(),
        ),
      ),
    ),
    // Footer
    h('div', { class: 'sandbox-footer' },
      h('span', { class: 'text-muted-dim text-xs' },
        'Sandbox — developer debug view. Your end users never see this page.',
      ),
      h('button', {
        class: 'btn btn-ghost btn-sm',
        onClick: logout,
      }, 'Log out and try again'),
    ),
  );
}
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/tenant/sandbox/app.ts admin/src/tenant/sandbox/SandboxPage.ts
git commit -m "feat: sandbox page with JWT viewer, callback preview, session info"
```

---

## Task 18: Verify End-to-End Flow

- [ ] **Step 1: Restart the dev server**

```bash
npm run dev
```

Verify both new routes load:
- `http://localhost:3001/platform/console-v2` — should show the console-v2 shell
- `http://localhost:3001/t/forma/sandbox` — should redirect to login (then show sandbox after auth)

- [ ] **Step 2: Create a fresh test tenant**

Use the existing onboarding page to create a new tenant. After creation, navigate to `http://localhost:3001/platform/console-v2`. The new tenant should have `onboarding_completed = false` and should trigger the wizard.

- [ ] **Step 3: Walk through the wizard**

1. Step 1: Enter app name, pick a color, verify live preview updates, click Next
2. Step 2: Login page should auto-open in a new tab. Register a test user. Console should detect the auth event and advance to success state.
3. Step 3: Pick a path, click "Go to Console"

- [ ] **Step 4: Test the sandbox**

Visit `http://localhost:3001/t/{new-slug}/sandbox` while logged in as the test user you just created. Verify: callback preview, user profile, session info, JWT viewer all populate with real data.

- [ ] **Step 5: Test redirect query param**

Visit `http://localhost:3001/t/forma/login?redirect=/t/forma/sandbox`. Log in. Verify you land on the sandbox page, not the default dashboard.

- [ ] **Step 6: Commit any fixes**

If any adjustments were needed during testing, commit them:

```bash
git add -A
git commit -m "fix: adjustments from end-to-end wizard + sandbox testing"
```

---

## CSS Note

This plan creates components with class names like `.wizard-step`, `.wizard-preview`, `.sandbox-page`, `.sandbox-columns`, etc. These CSS classes need to be added to the theme. The exact styles should follow the existing Forma design system (tokens.css variables, dark palette, DM Mono for code, Instrument Serif for headings). Add them to `src/core/theme/pages.css` (for page layouts) or `src/core/theme/components.css` (for reusable component styles).

The CSS is intentionally not specified in this plan because the user wants to provide a style guide separately. The component structure and class names are set — the styles will be applied during or after implementation.
