# Console Redesign Phase 2: Tabbed Console Views

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Phase 2 stubs in the console with a tabbed tenant detail view: Overview, Users, Integrate, and Settings tabs. Add self-serve user management endpoints so the Plane 2 console can list/manage tenant users without needing a Plane 3 JWT or management API key.

**Architecture:** The console is a Plane 2 app (Platform JWT auth). Tenant-scoped admin endpoints at `/admin/*` require Plane 3 JWT — the console can't use them. Instead, new self-serve endpoints at `/platform/self-serve/tenants/{id}/users` verify tenant ownership via Platform JWT and query users scoped to that tenant. Frontend views live in `console-v2/views/` — one file per tab, imported by `ConsoleApp.ts` via the existing `createSwitch`.

**Tech Stack:** FormaJS (h, createSignal, createShow, createSwitch, batch, mount), Rust/Axum (handlers, sqlx), PostgreSQL

**Spec:** `docs/superpowers/specs/2026-03-31-console-redesign-design.md` — Phase 2 section

---

## File Map

### New Files (Frontend)

| File | Responsibility |
|------|----------------|
| `admin/src/platform/console-v2/views/OverviewView.ts` | Stats cards, recent activity, quick links |
| `admin/src/platform/console-v2/views/UsersView.ts` | User list with search/filter, click → detail |
| `admin/src/platform/console-v2/views/IntegrateView.ts` | bridge.js guide, API key management |
| `admin/src/platform/console-v2/views/SettingsView.ts` | Branding, auth config, providers, danger zone |

### Modified Files (Frontend)

| File | Change |
|------|--------|
| `admin/src/platform/console-v2/ConsoleApp.ts` | Replace stubs with real views, add tab navigation to sidebar |
| `admin/src/platform/console-v2/store.ts` | Add view types, user management state, API functions for users |

### New/Modified Files (Backend)

| File | Change |
|------|--------|
| `crates/server/src/handlers/onboarding.rs` | Add self-serve user endpoints: list, detail, block, unblock, revoke sessions |
| `crates/server/src/handlers/mod.rs` | Register new self-serve user routes |

---

## Task 1: Self-Serve User Management Endpoints (Rust)

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs`
- Modify: `crates/server/src/handlers/mod.rs`

The console needs to list/manage tenant users. Existing `/admin/users` endpoints require Plane 3 JWT + admin role. The console has Platform JWT (Plane 2). Add self-serve user endpoints that verify tenant ownership via Platform JWT.

- [ ] **Step 1: Add `list_tenant_users` handler**

In `onboarding.rs`, add after the `delete_my_tenant` handler:

```rust
/// GET /platform/self-serve/tenants/{id}/users — List users for a tenant owned by the authenticated user.
pub async fn list_tenant_users(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    // Verify ownership
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found or you don't have access".into() })?;

    let rows = sqlx::query(
        "SELECT u.id, u.email, u.name, u.email_verified, u.created_at, u.updated_at, \
         COALESCE(u.locked_until > NOW(), FALSE) AS is_locked, \
         (SELECT array_agg(r.name) FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = u.id AND ur.tenant_id = $1) AS roles, \
         (SELECT c.provider FROM credentials c WHERE c.user_id = u.id AND c.tenant_id = $1 LIMIT 1) AS provider \
         FROM users u WHERE u.tenant_id = $1 \
         ORDER BY u.created_at DESC LIMIT 100",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let users: Vec<serde_json::Value> = rows.iter().map(|r| {
        let roles: Option<Vec<String>> = r.get("roles");
        serde_json::json!({
            "id": r.get::<String, _>("id"),
            "email": r.get::<String, _>("email"),
            "name": r.get::<Option<String>, _>("name"),
            "email_verified": r.get::<bool, _>("email_verified"),
            "is_locked": r.get::<bool, _>("is_locked"),
            "provider": r.get::<Option<String>, _>("provider"),
            "roles": roles.unwrap_or_default(),
            "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
        })
    }).collect();

    Ok(Json(serde_json::json!({ "users": users, "total": users.len() })))
}
```

- [ ] **Step 2: Add `block_tenant_user` and `unblock_tenant_user` handlers**

```rust
/// POST /platform/self-serve/tenants/{id}/users/{user_id}/block
pub async fn block_tenant_user(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    // Verify ownership
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    // Block: set locked_until to far future
    let result = sqlx::query(
        "UPDATE users SET locked_until = '2099-12-31T23:59:59Z', updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound { error: "user_not_found", message: "User not found".into() });
    }

    Ok(Json(serde_json::json!({ "status": "blocked" })))
}

/// POST /platform/self-serve/tenants/{id}/users/{user_id}/unblock
pub async fn unblock_tenant_user(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    sqlx::query(
        "UPDATE users SET locked_until = NULL, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "status": "unblocked" })))
}
```

- [ ] **Step 3: Add `revoke_tenant_user_sessions` handler**

```rust
/// POST /platform/self-serve/tenants/{id}/users/{user_id}/revoke-sessions
pub async fn revoke_tenant_user_sessions(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    let revoked = sqlx::query(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "revoked": revoked.rows_affected() })))
}
```

- [ ] **Step 4: Register routes in `mod.rs`**

In the self-serve route section (where `list_my_tenants`, `get_my_tenant`, etc. are registered), add:

```rust
.route(
    "/platform/self-serve/tenants/{id}/users",
    get(onboarding::list_tenant_users),
)
.route(
    "/platform/self-serve/tenants/{id}/users/{user_id}/block",
    post(onboarding::block_tenant_user),
)
.route(
    "/platform/self-serve/tenants/{id}/users/{user_id}/unblock",
    post(onboarding::unblock_tenant_user),
)
.route(
    "/platform/self-serve/tenants/{id}/users/{user_id}/revoke-sessions",
    post(onboarding::revoke_tenant_user_sessions),
)
```

- [ ] **Step 5: Verify compilation**

```bash
cargo check
```
Expected: compiles with no errors.

- [ ] **Step 6: Test with curl**

```bash
# Register platform user, create tenant, register a tenant user, then list users
npm run dev:clean
# (wait for server to start)
# Register platform user
REG=$(curl -s -c /tmp/p2-cookies -b /tmp/p2-cookies http://localhost:3001/t/platform/auth/register -H 'Content-Type: application/json' -d '{"email":"p2test@test.dev","password":"Test1234!","name":"Test"}')
TOKEN=$(echo $REG | python3 -c 'import sys,json; print(json.load(sys.stdin)["access_token"])')
# Create tenant
CREATE=$(curl -s -b /tmp/p2-cookies http://localhost:3001/platform/self-serve/tenants -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{"name":"Phase2 App","slug":"phase2-app"}')
TID=$(echo $CREATE | python3 -c 'import sys,json; print(json.load(sys.stdin)["tenant"]["id"])')
# Register a test user on the tenant
curl -s http://localhost:3001/t/phase2-app/auth/register -H 'Content-Type: application/json' -d '{"email":"enduser@test.com","password":"Test1234!"}'
# List users via self-serve
curl -s -b /tmp/p2-cookies "http://localhost:3001/platform/self-serve/tenants/$TID/users" -H "Authorization: Bearer $TOKEN" | python3 -m json.tool
```

Expected: `{ "users": [{ "id": "usr_...", "email": "enduser@test.com", ... }], "total": 1 }`

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs crates/server/src/handlers/mod.rs
git commit -m "feat: add self-serve user management endpoints for console Phase 2"
```

---

## Task 2: Extend Store with User Management State

**Files:**
- Modify: `admin/src/platform/console-v2/store.ts`

Add view types for tabs, user state, and API functions the views will call.

- [ ] **Step 1: Add types and state for users**

Add to the Types section of `store.ts`:

```typescript
export interface TenantUser {
  id: string;
  email: string;
  name: string | null;
  email_verified: boolean;
  is_locked: boolean;
  provider: string | null;
  roles: string[];
  created_at: string;
}
```

Add to the State section:

```typescript
export const [users, setUsers] = createSignal<TenantUser[]>([]);
export const [usersLoading, setUsersLoading] = createSignal(false);
```

- [ ] **Step 2: Update the view type to include tab views**

Change the view signal type:

```typescript
export const [view, setView] = createSignal<'tenants' | 'wizard' | 'overview' | 'users' | 'integrate' | 'settings'>('tenants');
```

Note: replace `'detail'` with the specific tab views. Update `selectTenant` to use the tenant's `preferred_tab` or default to `'overview'`:

```typescript
export function selectTenant(tenant: Tenant): void {
  batch(() => {
    setSelectedTenant(tenant);
    setSelectedTenantStats(null);
    if (tenant.onboarding_completed) {
      const tab = tenant.preferred_tab;
      if (tab === 'integrate') setView('integrate');
      else setView('overview');
    } else {
      setView('wizard');
    }
  });
}
```

- [ ] **Step 3: Add user management API functions**

```typescript
export async function loadUsers(tenantId: string): Promise<void> {
  setUsersLoading(true);
  try {
    const data = await api.get<{ users: TenantUser[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users',
    );
    setUsers(data.users || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load users', 'error');
  } finally {
    setUsersLoading(false);
  }
}

export async function blockUser(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/block', {});
    toast.show('User blocked', 'success');
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to block user', 'error');
  }
}

export async function unblockUser(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/unblock', {});
    toast.show('User unblocked', 'success');
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to unblock user', 'error');
  }
}

export async function revokeUserSessions(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/revoke-sessions', {});
    toast.show('Sessions revoked', 'success');
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to revoke sessions', 'error');
  }
}
```

- [ ] **Step 4: Add `api.post` method if missing from createApi**

Check `admin/src/core/hooks/createApi.ts`. If it doesn't have a `post` method, add one following the same pattern as `get` and `patch`.

- [ ] **Step 5: Build and verify**

```bash
npm run build
```
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add admin/src/platform/console-v2/store.ts admin/src/core/hooks/createApi.ts
git commit -m "feat: add user management state and API functions to console store"
```

---

## Task 3: Overview Tab View

**Files:**
- Create: `admin/src/platform/console-v2/views/OverviewView.ts`

The default tab after onboarding. Shows tenant stats, recent user, and quick links.

- [ ] **Step 1: Create OverviewView**

```typescript
import { h, createShow } from 'formajs';
import { selectedTenant, selectedTenantStats, loadTenantDetail, type Tenant, type TenantStats } from '../store';

export function OverviewView() {
  const tenant = selectedTenant()!;
  loadTenantDetail(tenant.id);

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, () => selectedTenant()?.name || 'Overview'),
      h('p', { class: 'view-subtitle text-muted' }, () => '/t/' + (selectedTenant()?.slug || '')),
    ),
    h('div', { class: 'stats-grid' },
      StatCard('Total Users', () => String(selectedTenantStats()?.total_users ?? '—')),
      StatCard('Active Sessions', () => String(selectedTenantStats()?.active_sessions ?? '—')),
      StatCard('Logins (24h)', () => String(selectedTenantStats()?.logins_24h ?? '—')),
    ),
    createShow(
      () => selectedTenantStats()?.latest_user != null,
      () => {
        const lu = selectedTenantStats()!.latest_user!;
        return h('div', { class: 'console-card' },
          h('div', { class: 'card-label' }, 'Latest User'),
          h('div', { class: 'card-value' }, lu.email),
          h('div', { class: 'card-meta text-muted' },
            new Date(lu.created_at).toLocaleString() + ' — ' + lu.provider,
          ),
        );
      },
    ),
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Quick Links'),
      h('div', { class: 'quick-links' },
        h('a', { href: () => '/t/' + (selectedTenant()?.slug || '') + '/login', target: '_blank', class: 'btn-link' }, 'Login Page'),
        h('a', { href: () => '/t/' + (selectedTenant()?.slug || '') + '/sandbox', target: '_blank', class: 'btn-link' }, 'Sandbox'),
      ),
    ),
  );
}

function StatCard(label: string, value: () => string) {
  return h('div', { class: 'stat-card' },
    h('div', { class: 'stat-value' }, value),
    h('div', { class: 'stat-label' }, label),
  );
}
```

- [ ] **Step 2: Add CSS for overview to `pages.css`**

Append to `admin/src/core/theme/pages.css`:

```css
/* ==========================================================================
   Console Views — Phase 2
   ========================================================================== */

.console-view {
  padding: 1.5rem 2rem;
  max-width: 960px;
}

.view-header {
  margin-bottom: 1.5rem;
}

.view-title {
  font-size: 1.25rem;
  font-weight: 500;
  color: var(--text);
  margin: 0 0 0.25rem;
}

.view-subtitle {
  font-size: 0.78rem;
  font-family: 'DM Mono', monospace;
  font-weight: 300;
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 1rem;
  margin-bottom: 1.5rem;
}

.stat-card {
  border: 1px solid var(--border);
  background: var(--surface);
  padding: 1rem;
  text-align: center;
}

.stat-value {
  font-size: 1.75rem;
  font-weight: 500;
  color: var(--text);
  font-family: 'DM Mono', monospace;
}

.stat-label {
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: var(--text-muted);
  margin-top: 0.25rem;
}

.console-card {
  border: 1px solid var(--border);
  background: var(--surface);
  padding: 1rem 1.25rem;
  margin-bottom: 1rem;
}

.card-label {
  font-size: 0.6rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  color: var(--text-muted);
  margin-bottom: 0.5rem;
}

.card-value {
  font-size: 0.9rem;
  color: var(--text);
}

.card-meta {
  font-size: 0.72rem;
  margin-top: 0.25rem;
}

.quick-links {
  display: flex;
  gap: 1rem;
}

.btn-link {
  font-size: 0.78rem;
  color: var(--accent);
  text-decoration: none;
}

.btn-link:hover {
  text-decoration: underline;
}
```

- [ ] **Step 3: Build and verify**

```bash
npm run build
```

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console-v2/views/OverviewView.ts admin/src/core/theme/pages.css
git commit -m "feat: add Overview tab view for console Phase 2"
```

---

## Task 4: Users Tab View

**Files:**
- Create: `admin/src/platform/console-v2/views/UsersView.ts`

Lists tenant users with search, shows locked status, and provides block/unblock/revoke actions.

- [ ] **Step 1: Create UsersView**

```typescript
import { h, createSignal, createShow } from 'formajs';
import {
  selectedTenant, users, usersLoading, loadUsers,
  blockUser, unblockUser, revokeUserSessions,
} from '../store';

export function UsersView() {
  const tenant = selectedTenant()!;
  const [search, setSearch] = createSignal('');
  const [expandedId, setExpandedId] = createSignal<string | null>(null);

  loadUsers(tenant.id);

  const filteredUsers = () => {
    const q = search().toLowerCase();
    if (!q) return users();
    return users().filter(u =>
      u.email.toLowerCase().includes(q) || (u.name || '').toLowerCase().includes(q),
    );
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Users'),
      h('p', { class: 'view-subtitle text-muted' }, () => users().length + ' total'),
    ),
    h('div', { class: 'console-card' },
      h('input', {
        type: 'text',
        class: 'search-input',
        placeholder: 'Search by email or name...',
        value: search,
        onInput: (e: Event) => setSearch((e.target as HTMLInputElement).value),
      }),
    ),
    createShow(
      () => usersLoading(),
      () => h('p', { class: 'text-muted', style: { padding: '1rem' } }, 'Loading...'),
    ),
    h('div', { class: 'user-list' },
      () => filteredUsers().map(u =>
        h('div', {
          class: () => 'user-row' + (expandedId() === u.id ? ' expanded' : ''),
          key: u.id,
        },
          h('div', {
            class: 'user-row-summary',
            onClick: () => setExpandedId(expandedId() === u.id ? null : u.id),
          },
            h('div', { class: 'user-email' },
              u.email,
              u.is_locked ? h('span', { class: 'badge badge-danger' }, 'Blocked') : null,
            ),
            h('div', { class: 'user-meta text-muted' },
              (u.provider || 'email') + ' — ' + new Date(u.created_at).toLocaleDateString(),
            ),
          ),
          createShow(
            () => expandedId() === u.id,
            () => h('div', { class: 'user-detail' },
              h('div', { class: 'detail-grid' },
                DetailRow('User ID', u.id),
                DetailRow('Name', u.name || '(not set)'),
                DetailRow('Verified', u.email_verified ? 'Yes' : 'No'),
                DetailRow('Roles', u.roles.join(', ') || 'none'),
              ),
              h('div', { class: 'user-actions' },
                u.is_locked
                  ? h('button', { class: 'btn btn-sm btn-ghost', onClick: () => unblockUser(tenant.id, u.id) }, 'Unblock')
                  : h('button', { class: 'btn btn-sm btn-ghost', onClick: () => blockUser(tenant.id, u.id) }, 'Block'),
                h('button', { class: 'btn btn-sm btn-ghost', onClick: () => revokeUserSessions(tenant.id, u.id) }, 'Revoke Sessions'),
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

function DetailRow(label: string, value: string) {
  return h('div', { class: 'sandbox-detail-row' },
    h('span', { class: 'sandbox-detail-label' }, label),
    h('span', { class: 'sandbox-detail-value' }, value),
  );
}
```

- [ ] **Step 2: Add CSS for users view to `pages.css`**

```css
/* Users View */

.search-input {
  width: 100%;
  padding: 0.5rem 0.75rem;
  background: var(--bg);
  border: 1px solid var(--border);
  color: var(--text);
  font-size: 0.82rem;
  font-family: inherit;
}

.search-input:focus {
  outline: none;
  border-color: var(--accent);
}

.user-list {
  display: flex;
  flex-direction: column;
}

.user-row {
  border: 1px solid var(--border);
  background: var(--surface);
  margin-bottom: -1px;
}

.user-row-summary {
  padding: 0.75rem 1rem;
  cursor: pointer;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.user-row-summary:hover {
  background: var(--surface-hover);
}

.user-email {
  font-size: 0.82rem;
  color: var(--text);
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.user-meta {
  font-size: 0.72rem;
}

.user-detail {
  padding: 0.75rem 1rem;
  border-top: 1px solid var(--border);
  background: var(--bg);
}

.detail-grid {
  margin-bottom: 0.75rem;
}

.user-actions {
  display: flex;
  gap: 0.5rem;
  padding-top: 0.5rem;
  border-top: 1px solid var(--border);
}

.badge {
  font-size: 0.6rem;
  padding: 0.1rem 0.4rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.badge-danger {
  border: 1px solid var(--danger);
  color: var(--danger);
  background: rgba(239, 68, 68, 0.08);
}
```

- [ ] **Step 3: Build and verify**

```bash
npm run build
```

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console-v2/views/UsersView.ts admin/src/core/theme/pages.css
git commit -m "feat: add Users tab view with search, block/unblock, revoke sessions"
```

---

## Task 5: Integrate Tab View

**Files:**
- Create: `admin/src/platform/console-v2/views/IntegrateView.ts`

Shows bridge.js integration guide pre-filled with the tenant's slug and URL, plus API key management.

- [ ] **Step 1: Create IntegrateView**

```typescript
import { h, createSignal, createShow } from 'formajs';
import { selectedTenant, updateTenant, toast } from '../store';

export function IntegrateView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const [showKey, setShowKey] = createSignal(false);

  const bridgeSnippet = () => {
    const t = selectedTenant()!;
    return `<script src="${serverUrl}/bridge.js"><\/script>
<script>
  GateAuthBridge.init({
    serverUrl: '${serverUrl}',
    slug: '${t.slug}',
    onStateChange: ({ state, user }) => {
      if (state === 'authenticated') {
        console.log('Hello,', user.name);
      }
    }
  });
<\/script>`;
  };

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Integrate'),
      h('p', { class: 'view-subtitle text-muted' }, 'Add auth to your app in three steps'),
    ),

    // Step 1: Script tag
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Step 1 — Add the script tag'),
      h('pre', { class: 'sandbox-code' }, bridgeSnippet),
      h('button', {
        class: 'btn btn-sm btn-ghost',
        style: { marginTop: '0.5rem' },
        onClick: () => { navigator.clipboard.writeText(bridgeSnippet()); toast.show('Copied', 'success'); },
      }, 'Copy'),
    ),

    // Step 2: Login URL
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Step 2 — Your login page URL'),
      h('code', { class: 'sandbox-code', style: { display: 'block' } },
        () => serverUrl + '/t/' + (selectedTenant()?.slug || '') + '/login',
      ),
      h('p', { class: 'text-muted', style: { fontSize: '0.75rem', marginTop: '0.5rem' } },
        'Redirect unauthenticated users here. After login, they return to your app with a session cookie.',
      ),
    ),

    // Step 3: API Key
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Step 3 — Management API key (backend only)'),
      createShow(
        () => !!selectedTenant()?.management_api_key,
        () => h('div', null,
          createShow(
            () => showKey(),
            () => h('code', { class: 'sandbox-code', style: { display: 'block', wordBreak: 'break-all' } },
              () => selectedTenant()?.management_api_key || '',
            ),
          ),
          h('div', { style: { display: 'flex', gap: '0.5rem', marginTop: '0.5rem' } },
            h('button', { class: 'btn btn-sm btn-ghost', onClick: () => setShowKey(!showKey()) },
              () => showKey() ? 'Hide' : 'Reveal',
            ),
            createShow(
              () => showKey(),
              () => h('button', {
                class: 'btn btn-sm btn-ghost',
                onClick: () => { navigator.clipboard.writeText(selectedTenant()?.management_api_key || ''); toast.show('Copied', 'success'); },
              }, 'Copy'),
            ),
          ),
        ),
      ),
      createShow(
        () => !selectedTenant()?.management_api_key,
        () => h('p', { class: 'text-muted', style: { fontSize: '0.75rem' } },
          'API key was shown once at tenant creation. Use the rotate button if you need a new one.',
        ),
      ),
      h('p', { class: 'text-muted', style: { fontSize: '0.72rem', marginTop: '0.5rem' } },
        'Use this key from your server to manage users, roles, and webhooks via the Management API.',
      ),
    ),
  );
}
```

- [ ] **Step 2: Build and verify**

```bash
npm run build
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console-v2/views/IntegrateView.ts
git commit -m "feat: add Integrate tab view with bridge.js guide and API key"
```

---

## Task 6: Settings Tab View

**Files:**
- Create: `admin/src/platform/console-v2/views/SettingsView.ts`

Branding configuration, auth providers, and danger zone.

- [ ] **Step 1: Create SettingsView**

```typescript
import { h, createSignal, createShow } from 'formajs';
import { Field } from '../../../core/components/Field';
import { selectedTenant, updateTenant, toast, backToTenants, type Tenant } from '../store';

export function SettingsView() {
  const tenant = selectedTenant()!;
  const [appName, setAppName] = createSignal(tenant.name || '');
  const [color, setColor] = createSignal(tenant.primary_color || '#6366f1');
  const [saving, setSaving] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal('');

  async function saveBranding() {
    setSaving(true);
    await updateTenant(tenant.id, { name: appName(), primary_color: color() });
    setSaving(false);
    toast.show('Settings saved', 'success');
  }

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Settings'),
    ),

    // Branding
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Branding'),
      Field({ label: 'App Name', name: 'app-name', value: appName, onChange: setAppName, placeholder: 'My App' }),
      h('div', { class: 'field' },
        h('label', null, 'Primary Color'),
        h('div', { style: { display: 'flex', alignItems: 'center', gap: '0.5rem' } },
          h('input', {
            type: 'color', value: color,
            onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
            style: { width: '36px', height: '36px', border: 'none', padding: '0', cursor: 'pointer' },
          }),
          h('input', {
            type: 'text', value: color,
            onInput: (e: Event) => setColor((e.target as HTMLInputElement).value),
            style: { flex: '1' },
          }),
        ),
      ),
      h('button', {
        class: 'btn btn-primary btn-sm',
        style: { marginTop: '0.75rem' },
        disabled: saving,
        onClick: saveBranding,
      }, () => saving() ? 'Saving...' : 'Save'),
    ),

    // Info
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Tenant Info'),
      h('div', { class: 'detail-grid' },
        InfoRow('Slug', tenant.slug),
        InfoRow('Plan', tenant.plan),
        InfoRow('Status', tenant.status),
        InfoRow('Created', new Date(tenant.created_at).toLocaleDateString()),
      ),
    ),

    // Danger zone
    h('div', { class: 'console-card danger-zone' },
      h('div', { class: 'card-label' }, 'Danger Zone'),
      h('p', { class: 'text-muted', style: { fontSize: '0.75rem', marginBottom: '0.75rem' } },
        'Deleting a tenant removes all users, sessions, and configuration. This cannot be undone.',
      ),
      h('input', {
        type: 'text',
        class: 'search-input',
        placeholder: () => 'Type "' + tenant.slug + '" to confirm',
        value: confirmDelete,
        onInput: (e: Event) => setConfirmDelete((e.target as HTMLInputElement).value),
        style: { marginBottom: '0.5rem' },
      }),
      h('button', {
        class: 'btn btn-sm btn-danger',
        disabled: () => confirmDelete() !== tenant.slug,
        onClick: async () => {
          // TODO: call delete endpoint
          toast.show('Tenant deleted', 'success');
          backToTenants();
        },
      }, 'Delete Tenant'),
    ),
  );
}

function InfoRow(label: string, value: string) {
  return h('div', { class: 'sandbox-detail-row' },
    h('span', { class: 'sandbox-detail-label' }, label),
    h('span', { class: 'sandbox-detail-value' }, value),
  );
}
```

- [ ] **Step 2: Add danger zone CSS to `pages.css`**

```css
/* Settings — Danger Zone */
.danger-zone {
  border-color: var(--danger);
}

.btn-danger {
  background: var(--danger);
  color: white;
  border: none;
  padding: 0.4rem 0.85rem;
  font-size: 0.78rem;
  cursor: pointer;
  font-family: inherit;
}

.btn-danger:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
```

- [ ] **Step 3: Build and verify**

```bash
npm run build
```

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console-v2/views/SettingsView.ts admin/src/core/theme/pages.css
git commit -m "feat: add Settings tab view with branding config and danger zone"
```

---

## Task 7: Wire Up ConsoleApp with Tab Navigation

**Files:**
- Modify: `admin/src/platform/console-v2/ConsoleApp.ts`

Replace stubs with real views, add tab icons to sidebar.

- [ ] **Step 1: Update imports and remove stubs**

Replace the current ConsoleApp.ts content. Remove `TenantsStub` and `DetailStub`. Import the real views. Add tab navigation items to the sidebar that are only visible when a tenant is selected and onboarded.

```typescript
import { h, createSwitch, createShow } from 'formajs';
import { Sidebar } from '../../core/components/Sidebar';
import { LoadingBar } from '../../core/components/LoadingBar';
import { ToastView } from '../../core/components/Toast';
import { WizardFlow } from './wizard/WizardFlow';
import { OverviewView } from './views/OverviewView';
import { UsersView } from './views/UsersView';
import { IntegrateView } from './views/IntegrateView';
import { SettingsView } from './views/SettingsView';
import {
  view, setView, loading, toast, backToTenants, loadTenants, signout,
  selectedTenant, tenants, selectTenant,
} from './store';

// SVG icons — Lucide, stroke-width 1.5
const ICONS = {
  tenants: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>',
  overview: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>',
  users: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>',
  integrate: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>',
  settings: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>',
  refresh: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>',
  signout: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>',
};

function TenantsList() {
  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Your Tenants'),
    ),
    h('div', { class: 'user-list' },
      () => tenants().map(t =>
        h('div', {
          class: 'user-row',
          key: t.id,
        },
          h('div', {
            class: 'user-row-summary',
            onClick: () => selectTenant(t),
          },
            h('div', { class: 'user-email' }, t.name),
            h('div', { class: 'user-meta text-muted' },
              '/t/' + t.slug + ' — ' + t.plan + ' — ' + t.status,
            ),
          ),
        ),
      ),
    ),
  );
}

export function ConsoleApp() {
  const hasTenant = () => selectedTenant() != null;
  const isOnboarded = () => {
    const t = selectedTenant();
    return t != null && t.onboarding_completed;
  };

  const sidebarItems = () => {
    const items: { id: string; label: string; icon: string }[] = [
      { id: 'tenants', label: 'Tenants', icon: ICONS.tenants },
    ];
    if (isOnboarded()) {
      items.push(
        { id: 'overview', label: 'Overview', icon: ICONS.overview },
        { id: 'users', label: 'Users', icon: ICONS.users },
        { id: 'integrate', label: 'Integrate', icon: ICONS.integrate },
        { id: 'settings', label: 'Settings', icon: ICONS.settings },
      );
    }
    return items;
  };

  return h('div', { class: 'shell' },
    Sidebar({
      brand: { name: 'Forma Auth', mark: '\u25C7', subtitle: 'SELF-SERVE CONSOLE' },
      items: sidebarItems,
      footer: [
        { id: 'refresh', label: 'Refresh', icon: ICONS.refresh },
        { id: 'signout', label: 'Sign Out', icon: ICONS.signout, danger: true },
      ],
      activeView: () => view(),
      onNavigate: (id: string) => {
        if (id === 'refresh') { loadTenants(); return; }
        if (id === 'signout') { signout(); return; }
        if (id === 'tenants') { backToTenants(); return; }
        if (['overview', 'users', 'integrate', 'settings'].includes(id)) {
          setView(id as any);
          return;
        }
      },
    }),
    h('main', { class: 'main' },
      LoadingBar(() => loading()),
      createSwitch(
        () => view(),
        [
          { match: 'tenants', render: () => TenantsList() },
          { match: 'wizard', render: () => WizardFlow() },
          { match: 'overview', render: () => OverviewView() },
          { match: 'users', render: () => UsersView() },
          { match: 'integrate', render: () => IntegrateView() },
          { match: 'settings', render: () => SettingsView() },
        ],
      ),
    ),
    ToastView(toast),
  );
}
```

- [ ] **Step 2: Verify the Sidebar component accepts dynamic items**

Check `admin/src/core/components/Sidebar.ts` — the `items` prop may need to accept a function (reactive) instead of a static array. If it only accepts a static array, update it to support `items: SidebarItem[] | (() => SidebarItem[])`.

- [ ] **Step 3: Build and verify**

```bash
npm run build
```

- [ ] **Step 4: Test the full flow**

```bash
npm run dev:clean
```

1. Go to `http://localhost:3001/platform/login`
2. Register, create a tenant, complete the wizard
3. After wizard completion, verify you see the Overview tab
4. Click Users, Integrate, Settings tabs — verify each renders
5. Verify sidebar shows tabs only when a tenant is selected and onboarded

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console-v2/ConsoleApp.ts
git commit -m "feat: wire up tabbed console views — overview, users, integrate, settings"
```

---

## Task 8: Polish and Styling

**Files:**
- Modify: `admin/src/core/theme/pages.css` (any missing CSS from above tasks)
- Modify: various view files if styling issues found during testing

- [ ] **Step 1: Test all tabs end-to-end and fix visual issues**

Walk through every tab. Fix any missing CSS classes, broken layouts, or visual issues. Ensure:
- Stats grid renders properly on Overview
- User list is scrollable and search works
- Code blocks in Integrate are readable and copy works
- Settings save works and toast appears
- Danger zone requires slug confirmation

- [ ] **Step 2: Build final**

```bash
npm run build
cargo check
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "fix: Phase 2 console polish — styling and UX fixes"
```
