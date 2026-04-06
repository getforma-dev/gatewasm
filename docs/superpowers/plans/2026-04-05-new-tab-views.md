# New Tab Views Implementation Plan (Sub-plan B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the 6 dashboard views for tabs that currently show "Coming Soon" — Audit Log, Activity, Sessions, Passkeys, Tokens, Roles — each with backend self-serve proxy endpoints where needed.

**Architecture:** Each tab follows the same pattern: (1) add a self-serve proxy endpoint in `onboarding.rs` using the ownership-verification pattern, (2) add store types/signals/load function, (3) create the view component, (4) wire it into `hydrateTenantView` and `ConsoleApp.ts`. Sequenced for maximum reuse: Audit Log first (unlocks Activity), then Sessions, Passkeys, Tokens, Roles.

**Tech Stack:** Rust/axum/sqlx (backend), TypeScript/FormaJS (frontend)

**Fix included:** Task 1 also fixes the plane-switch reactivity issue (color highlight not updating until mouse move).

---

## Sequencing Rationale

1. **Audit Log (2.4)** → builds the self-serve audit-log proxy endpoint
2. **Activity (3.4)** → shares the same endpoint with different default filters (nearly free)
3. **Sessions (3.1)** → new aggregate sessions endpoint
4. **Passkeys (3.2)** → new admin passkey listing endpoint
5. **Tokens (3.3)** → JWKS viewer + key management (APIs exist, frontend only)
6. **Roles (2.3)** → self-serve proxy for role CRUD + assignment UI

---

### Task 1: Fix plane-switch reactivity + add self-serve audit-log endpoint

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add `list_tenant_audit_log`)
- Modify: `crates/server/src/handlers/mod.rs` (register route)
- Modify: `admin/src/platform/console/views/PlaneNav.ts` (fix reactivity)

The plane-switch color highlight not updating until mouse move is caused by cached DOM references. The expanded nav (tier1 + tier2) is created once and reused. When the plane signal changes, the reactive `class` functions on buttons DO update, but the browser may not repaint clipped elements without a layout trigger. Fix by ensuring the tier2 content is reactive (wrapped in a function that re-evaluates when plane changes).

- [ ] **Step 1: Add the audit log endpoint in onboarding.rs**

Add after `list_tenant_user_sessions` (before the `// ── Hosted Page Management` section). Follow the exact ownership-verification pattern:

```rust
/// GET /platform/self-serve/tenants/{id}/audit-log
pub async fn list_tenant_audit_log(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Query(params): Query<PaginationQuery>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let rows = sqlx::query(
        "SELECT la.id, la.email, la.user_id, la.success, la.ip_address, la.user_agent, \
         la.failure_reason, la.created_at \
         FROM login_attempts la \
         WHERE la.tenant_id = $1 \
         ORDER BY la.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(&tenant_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let entries: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "email": r.get::<String, _>("email"),
                "user_id": r.get::<Option<String>, _>("user_id"),
                "success": r.get::<bool, _>("success"),
                "ip_address": r.get::<Option<String>, _>("ip_address"),
                "user_agent": r.get::<Option<String>, _>("user_agent"),
                "failure_reason": r.get::<Option<String>, _>("failure_reason"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "audit_log": entries,
        "limit": limit,
        "offset": offset,
    })))
}
```

Note: The `login_attempts` table has a `tenant_id` column (indexed via `idx_login_attempts_tenant`), so we can filter directly by `tenant_id = $1` instead of the subquery `user_id IN (SELECT id FROM users WHERE tenant_id = $1)` used in the management handler. This is more efficient.

Also note: `PaginationQuery` is already defined in `admin.rs` — you may need to import it or define it locally. Check if `onboarding.rs` already imports it. If not, add:
```rust
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
```

- [ ] **Step 2: Register the route in mod.rs**

Add before the hosted-page route:
```rust
        .route(
            "/platform/self-serve/tenants/{id}/audit-log",
            get(onboarding::list_tenant_audit_log),
        )
```

- [ ] **Step 3: Fix PlaneNav reactivity**

In `PlaneNav.ts`, the tier1 plane buttons use reactive classes but the DOM elements are created once. When the plane changes, the class updates but the browser may not repaint clip-path elements. Force a layout recalc by making the tier1 buttons render inside a reactive wrapper.

Find the `tier1` block and change the plane buttons from spread `...PLANES.map(...)` to a reactive function wrapper:
```typescript
const tier1 = h('div', { class: 'plane-tier1' },
  () => PLANES.map((pd) =>
    h('button', { /* same as current */ }),
  ),
  // collapse toggle stays as direct child
);
```

The key change: `...PLANES.map(...)` becomes `() => PLANES.map(...)`. This makes the button list reactive — when `plane()` changes, FormaJS re-evaluates the function and replaces the DOM nodes, ensuring the new active state renders immediately.

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs crates/server/src/handlers/mod.rs admin/src/platform/console/views/PlaneNav.ts
git commit -m "feat: add self-serve audit-log endpoint and fix plane-switch reactivity"
```

---

### Task 2: Build AuditLogView + store

**Files:**
- Modify: `admin/src/platform/console/store.ts` (add types, signals, load function)
- Create: `admin/src/platform/console/views/AuditLogView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts` (replace ComingSoonView)

- [ ] **Step 1: Add store types and signals**

In `store.ts`, add after the `UserSession` interface:

```typescript
export interface AuditLogEntry {
  id: string;
  email: string;
  user_id: string | null;
  success: boolean;
  ip_address: string | null;
  user_agent: string | null;
  failure_reason: string | null;
  created_at: string;
}
```

Add signals after `userSessionsLoading`:

```typescript
export const [auditLog, setAuditLog] = createSignal<AuditLogEntry[]>([]);
export const [auditLogLoading, setAuditLogLoading] = createSignal(false);
```

Add load function after `loadUserSessions`:

```typescript
export async function loadAuditLog(tenantId: string): Promise<void> {
  setAuditLogLoading(true);
  try {
    const data = await api.get<{ audit_log: AuditLogEntry[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/audit-log?limit=100',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setAuditLog(data.audit_log || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load audit log', 'error');
  } finally {
    setAuditLogLoading(false);
  }
}
```

Add `'audit-log'` to `hydrateTenantView`:
```typescript
  } else if (nextView === 'audit-log' || nextView === 'activity') {
    tasks.push(loadAuditLog(tenant.id));
  }
```

- [ ] **Step 2: Create AuditLogView component**

Create `admin/src/platform/console/views/AuditLogView.ts`:

Build a filterable table showing login attempts. The view should have:
- Header: "Audit Log" title with record count
- Filter chips: All, Successful, Failed
- Table/list of entries showing: email, success/fail badge, IP address, timestamp, failure reason (if failed)
- Loading skeleton state
- Empty state

Follow the same patterns as `UsersView.ts` — use `createSignal` for filters, `createShow` for conditional rendering, the existing CSS classes (`.console-view`, `.console-card`, `.list-shell`, `.card-label`, etc.).

Each entry row should show:
- Email (left)
- Success badge (green "Success") or fail badge (red "Failed" + failure reason)
- IP address (mono font)
- Relative timestamp

- [ ] **Step 3: Wire into ConsoleApp**

In `ConsoleApp.ts`, import `AuditLogView` and replace the `ComingSoonView('Audit Log')` match:
```typescript
{ match: 'audit-log', render: TenantWorkspaceGuard(() => AuditLogView()) },
```

- [ ] **Step 4: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/store.ts admin/src/platform/console/views/AuditLogView.ts admin/src/platform/console/ConsoleApp.ts
git commit -m "feat: build AuditLogView with filterable login attempt history"
```

---

### Task 3: Build ActivityView (shares audit-log endpoint)

**Files:**
- Create: `admin/src/platform/console/views/ActivityView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts` (replace ComingSoonView)

Activity shares the same `auditLog` signal and `loadAuditLog` function. The view is a timeline/feed focused on end-user events, with different visual treatment than the table-based Audit Log.

- [ ] **Step 1: Create ActivityView component**

Create `admin/src/platform/console/views/ActivityView.ts`:

Build a timeline feed. The view should:
- Header: "Activity" title, "End-user authentication events"
- Show entries as a vertical timeline (not a table)
- Each entry: timestamp on left, event description on right
- Success entries: green dot + "user@email.com signed in via email from 1.2.3.4"
- Failed entries: red dot + "user@email.com failed to sign in — invalid password"
- Loading/empty states

Import `auditLog`, `auditLogLoading` from store (same data as Audit Log tab).

- [ ] **Step 2: Wire into ConsoleApp**

```typescript
{ match: 'activity', render: TenantWorkspaceGuard(() => ActivityView()) },
```

- [ ] **Step 3: Build and commit**

```bash
git add admin/src/platform/console/views/ActivityView.ts admin/src/platform/console/ConsoleApp.ts
git commit -m "feat: build ActivityView timeline (shares audit-log data)"
```

---

### Task 4: Add aggregate sessions endpoint + SessionsView

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add `list_tenant_sessions`)
- Modify: `crates/server/src/handlers/mod.rs` (register route)
- Modify: `admin/src/platform/console/store.ts` (add signals, load function)
- Create: `admin/src/platform/console/views/SessionsView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts` (replace ComingSoonView)

- [ ] **Step 1: Add aggregate sessions endpoint**

In `onboarding.rs`, add after `list_tenant_audit_log`:

```rust
/// GET /platform/self-serve/tenants/{id}/sessions
pub async fn list_tenant_sessions(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let rows = sqlx::query(
        "SELECT rt.id, rt.user_id, u.email, rt.created_at, rt.expires_at, \
         rt.auth_method, rt.auth_provider, rt.last_activity_at, rt.login_ip, rt.mfa_verified \
         FROM refresh_tokens rt \
         JOIN users u ON u.id = rt.user_id \
         WHERE rt.tenant_id = $1 AND rt.revoked = FALSE AND rt.expires_at > NOW() \
         ORDER BY rt.created_at DESC LIMIT 100",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let sessions: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "user_id": r.get::<String, _>("user_id"),
                "email": r.get::<String, _>("email"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
                "expires_at": r.get::<chrono::DateTime<Utc>, _>("expires_at").to_rfc3339(),
                "auth_method": r.get::<String, _>("auth_method"),
                "auth_provider": r.get::<Option<String>, _>("auth_provider"),
                "last_activity_at": r.get::<Option<chrono::DateTime<Utc>>, _>("last_activity_at").map(|d| d.to_rfc3339()),
                "login_ip": r.get::<Option<String>, _>("login_ip"),
                "mfa_verified": r.get::<bool, _>("mfa_verified"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "sessions": sessions })))
}
```

- [ ] **Step 2: Register route**

```rust
        .route(
            "/platform/self-serve/tenants/{id}/sessions",
            get(onboarding::list_tenant_sessions),
        )
```

- [ ] **Step 3: Add store types and signals**

In `store.ts`, add type:
```typescript
export interface TenantSession {
  id: string;
  user_id: string;
  email: string;
  created_at: string;
  expires_at: string;
  auth_method: string;
  auth_provider: string | null;
  last_activity_at: string | null;
  login_ip: string | null;
  mfa_verified: boolean;
}
```

Add signals:
```typescript
export const [allSessions, setAllSessions] = createSignal<TenantSession[]>([]);
export const [allSessionsLoading, setAllSessionsLoading] = createSignal(false);
```

Add load function:
```typescript
export async function loadAllSessions(tenantId: string): Promise<void> {
  setAllSessionsLoading(true);
  try {
    const data = await api.get<{ sessions: TenantSession[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/sessions',
    );
    if (selectedTenant()?.id !== tenantId) return;
    setAllSessions(data.sessions || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load sessions', 'error');
  } finally {
    setAllSessionsLoading(false);
  }
}
```

Add to `hydrateTenantView`:
```typescript
  } else if (nextView === 'sessions') {
    tasks.push(loadAllSessions(tenant.id));
  }
```

- [ ] **Step 4: Create SessionsView**

Create `admin/src/platform/console/views/SessionsView.ts`:

Table of all active sessions across the tenant. Each row shows:
- User email
- Auth method (with MFA badge if verified)
- Login IP
- Created timestamp
- Expiry with live countdown
- Revoke button (calls `revokeUserSessions` from store)

Follow `UsersView` patterns for layout/CSS.

- [ ] **Step 5: Wire into ConsoleApp**

Replace `ComingSoonView('Sessions')` with `SessionsView()`.

- [ ] **Step 6: Build both backend and frontend, commit**

```bash
git add crates/server/src/handlers/onboarding.rs crates/server/src/handlers/mod.rs admin/src/platform/console/store.ts admin/src/platform/console/views/SessionsView.ts admin/src/platform/console/ConsoleApp.ts
git commit -m "feat: build SessionsView with aggregate tenant sessions endpoint"
```

---

### Task 5: Add passkey listing endpoint + PasskeysView

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add `list_tenant_passkeys`)
- Modify: `crates/server/src/handlers/mod.rs` (register route)
- Modify: `admin/src/platform/console/store.ts` (add types, signals)
- Create: `admin/src/platform/console/views/PasskeysView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts`

- [ ] **Step 1: Add passkey listing endpoint**

In `onboarding.rs`:

```rust
/// GET /platform/self-serve/tenants/{id}/passkeys
pub async fn list_tenant_passkeys(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let rows = sqlx::query(
        "SELECT wc.id, wc.name, wc.is_synced, wc.user_id, u.email, \
         wc.created_at, wc.last_used_at, wc.aaguid \
         FROM webauthn_credentials wc \
         JOIN users u ON u.id = wc.user_id \
         WHERE wc.tenant_id = $1 \
         ORDER BY wc.created_at DESC LIMIT 100",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let passkeys: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "name": r.get::<String, _>("name"),
                "is_synced": r.get::<bool, _>("is_synced"),
                "user_id": r.get::<String, _>("user_id"),
                "email": r.get::<String, _>("email"),
                "aaguid": r.get::<Option<String>, _>("aaguid"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
                "last_used_at": r.get::<Option<chrono::DateTime<Utc>>, _>("last_used_at").map(|d| d.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "passkeys": passkeys })))
}
```

- [ ] **Step 2: Register route, add store types/signals/load, create PasskeysView, wire into ConsoleApp**

Same pattern as Task 4. Store type `Passkey`, signals `passkeys`/`passkeysLoading`, load function `loadPasskeys`. View shows table: user email, passkey name, synced badge, AAGUID, created date, last used date. Add `'passkeys'` case to `hydrateTenantView`.

- [ ] **Step 3: Build and commit**

```bash
git commit -m "feat: build PasskeysView with tenant passkey listing endpoint"
```

---

### Task 6: Build TokensView (APIs already exist)

**Files:**
- Modify: `admin/src/platform/console/store.ts` (add JWKS signal, load function)
- Create: `admin/src/platform/console/views/TokensView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts`

No backend changes needed — the JWKS endpoint (`GET /auth/jwks`) is public, and API key info is already in the `Tenant` type.

- [ ] **Step 1: Add store signals**

```typescript
export const [jwks, setJwks] = createSignal<any>(null);

export async function loadJwks(slug: string): Promise<void> {
  try {
    const res = await fetch('/t/' + encodeURIComponent(slug) + '/auth/jwks');
    if (res.ok) setJwks(await res.json());
  } catch { /* ignore */ }
}
```

Add `'tokens'` case to `hydrateTenantView`:
```typescript
  } else if (nextView === 'tokens') {
    tasks.push(loadJwks(tenant.slug));
  }
```

- [ ] **Step 2: Create TokensView**

Create `admin/src/platform/console/views/TokensView.ts`:

Three sections:
1. **JWKS Public Keys** — fetch and display the keys from `/auth/jwks`. Show key ID (kid), algorithm, key type, use. Display the raw JSON in a collapsible code block.
2. **Management API Key** — show the key (if available from `selectedTenant().management_api_key`), with reveal/copy/rotate actions. Move the existing `ManagementKeyCard` logic from IntegrateView.
3. **Token Configuration** — read-only display of algorithm (EdDSA), issuer (gatewasm), audience (gatewasm-app). Same as the Auth Reference section but contextualized for the Tokens tab.

- [ ] **Step 3: Wire into ConsoleApp, build, commit**

```bash
git commit -m "feat: build TokensView with JWKS viewer and API key management"
```

---

### Task 7: Add self-serve role endpoints + RolesView

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add role proxy endpoints)
- Modify: `crates/server/src/handlers/mod.rs` (register routes)
- Modify: `admin/src/platform/console/store.ts` (add types, signals, CRUD functions)
- Create: `admin/src/platform/console/views/RolesView.ts`
- Modify: `admin/src/platform/console/ConsoleApp.ts`

- [ ] **Step 1: Add self-serve role endpoints**

In `onboarding.rs`, add four functions following the ownership-verification pattern:

1. `list_tenant_roles` — `GET /platform/self-serve/tenants/{id}/roles`
   ```sql
   SELECT id, name, description, is_system, permissions_jsonb, tenant_id, created_at
   FROM roles WHERE tenant_id IS NULL OR tenant_id = $1
   ORDER BY is_system DESC, name
   ```

2. `create_tenant_role` — `POST /platform/self-serve/tenants/{id}/roles`
   - Accept JSON body `{ name, description, permissions }` 
   - Insert into roles with generated id, tenant_id

3. `update_tenant_role` — `PATCH /platform/self-serve/tenants/{id}/roles/{role_id}`
   - Accept JSON body with optional `name`, `description`, `permissions`
   - Only update custom roles (not system roles)

4. `delete_tenant_role` — `DELETE /platform/self-serve/tenants/{id}/roles/{role_id}`
   - Delete from user_roles first, then roles
   - Only delete custom roles

- [ ] **Step 2: Register routes**

```rust
        .route(
            "/platform/self-serve/tenants/{id}/roles",
            get(onboarding::list_tenant_roles).post(onboarding::create_tenant_role),
        )
        .route(
            "/platform/self-serve/tenants/{id}/roles/{role_id}",
            patch(onboarding::update_tenant_role).delete(onboarding::delete_tenant_role),
        )
```

- [ ] **Step 3: Add store types, signals, and CRUD functions**

```typescript
export interface Role {
  id: string;
  name: string;
  description: string | null;
  is_system: boolean;
  permissions: string[];
  tenant_id: string | null;
  created_at: string;
}
```

Signals: `roles`, `rolesLoading`. Functions: `loadRoles`, `createRole`, `updateRole`, `deleteRole`.

- [ ] **Step 4: Create RolesView**

Table of roles with:
- Name, description, system badge, permission count, created date
- "Create Role" button (inline form or modal)
- Edit/delete actions on custom roles (system roles are read-only)
- User assignment count per role (optional, can be a follow-up)

- [ ] **Step 5: Wire into ConsoleApp, build, commit**

```bash
git commit -m "feat: build RolesView with self-serve role CRUD endpoints"
```

---

### Task 8: Clean up — remove all remaining ComingSoonView references

**Files:**
- Modify: `admin/src/platform/console/ConsoleApp.ts`

- [ ] **Step 1: Verify all ComingSoonView references are replaced**

After Tasks 2-7, all 6 "Coming Soon" views should be replaced with real components. Run a grep to confirm no remaining `ComingSoonView` calls exist. If the `ComingSoonView` function itself is now unused, remove it.

- [ ] **Step 2: Build and commit**

```bash
git commit -m "chore: remove ComingSoonView — all tabs have real views"
```
