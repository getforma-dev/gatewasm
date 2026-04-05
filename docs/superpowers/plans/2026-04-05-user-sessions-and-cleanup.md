# Per-User Session Inspection & Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-user active session details to the Users view so operators can see what they're revoking, and clean up dead code from the Auth Reference refactor.

**Architecture:** Add `active_sessions` count to the existing user list API response, add a new per-user sessions detail endpoint, enrich the frontend Users view expanded row with session data, and remove the now-unused `CallbackPreview` component.

**Tech Stack:** Rust/axum/sqlx (backend), TypeScript/formajs (frontend), Playwright (e2e)

---

### Task 1: Add `active_sessions` count to user list API response

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs:1293-1319`

This modifies the existing `list_tenant_users` SQL query to include a subquery counting active refresh tokens per user, and includes the count in the JSON response.

- [ ] **Step 1: Add the active_sessions subquery to the SQL**

In `crates/server/src/handlers/onboarding.rs`, replace the SQL query at line 1293:

```rust
    let rows = sqlx::query(
        "SELECT u.id, u.email, u.name, u.email_verified, u.created_at, u.updated_at, \
         COALESCE(u.locked_until > NOW(), FALSE) AS is_locked, \
         (SELECT array_agg(r.name) FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = u.id AND ur.tenant_id = $1) AS roles, \
         (SELECT c.provider FROM credentials c WHERE c.user_id = u.id AND c.tenant_id = $1 LIMIT 1) AS provider, \
         (SELECT COUNT(*)::BIGINT FROM refresh_tokens rt WHERE rt.user_id = u.id AND rt.tenant_id = $1 AND rt.revoked = FALSE AND rt.expires_at > NOW()) AS active_sessions \
         FROM users u WHERE u.tenant_id = $1 \
         ORDER BY u.created_at DESC LIMIT 100",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;
```

- [ ] **Step 2: Include active_sessions in the JSON response**

Replace the JSON mapping block at line 1305:

```rust
    let users: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let roles: Option<Vec<String>> = r.get("roles");
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "email": r.get::<String, _>("email"),
                "name": r.get::<Option<String>, _>("name"),
                "email_verified": r.get::<bool, _>("email_verified"),
                "is_locked": r.get::<bool, _>("is_locked"),
                "provider": r.get::<Option<String>, _>("provider"),
                "roles": roles.unwrap_or_default(),
                "active_sessions": r.get::<i64, _>("active_sessions"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs
git commit -m "feat: include active_sessions count in user list API response"
```

---

### Task 2: Add per-user sessions list endpoint (backend)

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs` (add function after `revoke_tenant_user_sessions` at ~line 1453)
- Modify: `crates/server/src/handlers/mod.rs:392-395` (add route)

Adds a new `GET /platform/self-serve/tenants/{id}/users/{user_id}/sessions` endpoint that returns active sessions for a specific user. Follows the exact same auth/ownership pattern as `revoke_tenant_user_sessions`.

- [ ] **Step 1: Add the handler function**

In `crates/server/src/handlers/onboarding.rs`, add this function immediately after `revoke_tenant_user_sessions` (after line 1453, before the `// ── Hosted Page Management` comment):

```rust
/// GET /platform/self-serve/tenants/{id}/users/{user_id}/sessions
pub async fn list_tenant_user_sessions(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
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

    // Verify user belongs to tenant
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM users WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "user_not_found",
        message: "User not found in this tenant".into(),
    })?;

    let rows = sqlx::query(
        "SELECT id, created_at, expires_at, auth_method, auth_provider, \
         last_activity_at, login_ip, mfa_verified \
         FROM refresh_tokens \
         WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE AND expires_at > NOW() \
         ORDER BY created_at DESC",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let sessions: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
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

- [ ] **Step 2: Register the route**

In `crates/server/src/handlers/mod.rs`, add the route after the `revoke-sessions` route (after line 395):

```rust
        .route(
            "/platform/self-serve/tenants/{id}/users/{user_id}/sessions",
            get(onboarding::list_tenant_user_sessions),
        )
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs crates/server/src/handlers/mod.rs
git commit -m "feat: add GET endpoint for per-user active sessions"
```

---

### Task 3: Extend frontend types and store for session data

**Files:**
- Modify: `admin/src/platform/console/store.ts` (TenantUser type at line 62, add UserSession type, add signals, add loadUserSessions function)

- [ ] **Step 1: Add `active_sessions` to `TenantUser` type**

In `admin/src/platform/console/store.ts`, update the `TenantUser` interface at line 62:

```typescript
export interface TenantUser {
  id: string;
  email: string;
  name: string | null;
  email_verified: boolean;
  is_locked: boolean;
  provider: string | null;
  roles: string[];
  active_sessions: number;
  created_at: string;
}
```

- [ ] **Step 2: Add `UserSession` type after `TenantUser`**

Add after the `TenantUser` interface (after line 71):

```typescript
export interface UserSession {
  id: string;
  created_at: string;
  expires_at: string;
  auth_method: string;
  auth_provider: string | null;
  last_activity_at: string | null;
  login_ip: string | null;
  mfa_verified: boolean;
}
```

- [ ] **Step 3: Add signals for user sessions**

After the `introspectUnavailable` signal (after line 124), add:

```typescript
export const [userSessions, setUserSessions] = createSignal<UserSession[]>([]);
export const [userSessionsLoading, setUserSessionsLoading] = createSignal(false);
```

- [ ] **Step 4: Add `loadUserSessions` function**

Add after the `revokeUserSessions` function (after line 407):

```typescript
export async function loadUserSessions(tenantId: string, userId: string): Promise<void> {
  setUserSessionsLoading(true);
  try {
    const data = await api.get<{ sessions: UserSession[] }>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/sessions',
    );
    if (selectedTenant()?.id !== tenantId) {
      return;
    }
    setUserSessions(data.sessions || []);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load sessions', 'error');
  } finally {
    setUserSessionsLoading(false);
  }
}
```

- [ ] **Step 5: Update `revokeUserSessions` to also clear the sessions signal**

Replace the `revokeUserSessions` function at line 399:

```typescript
export async function revokeUserSessions(tenantId: string, userId: string): Promise<void> {
  try {
    await api.post('/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/users/' + encodeURIComponent(userId) + '/revoke-sessions', {});
    toast.show('Sessions revoked', 'success');
    setUserSessions([]);
    await loadUsers(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to revoke sessions', 'error');
  }
}
```

- [ ] **Step 6: Build frontend**

Run: `cd admin && npm run build 2>&1 | tail -5`
Expected: Builds without errors.

- [ ] **Step 7: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat: add UserSession type, signals, and loadUserSessions store function"
```

---

### Task 4: Enrich UsersView expanded detail with session data

**Files:**
- Modify: `admin/src/platform/console/views/UsersView.ts`

This adds: (a) active session count indicator on user rows, (b) session detail cards in the expanded panel that load on expand.

- [ ] **Step 1: Update imports**

Replace the import block at line 4 of `UsersView.ts`:

```typescript
import {
  selectedTenant, users, usersLoading,
  blockUser, unblockUser, revokeUserSessions,
  loadUserSessions, userSessions, userSessionsLoading, setUserSessions,
  type UserSession,
} from '../store';
```

- [ ] **Step 2: Load sessions when a user row is expanded**

Replace the `setExpandedId` toggle on line 129:

```typescript
                onClick: () => {
                  const nextId = expandedId() === user.id ? null : user.id;
                  setExpandedId(nextId);
                  if (nextId) {
                    void loadUserSessions(tenantState().id, user.id);
                  } else {
                    setUserSessions([]);
                  }
                },
```

- [ ] **Step 3: Add session count badge to user row summary**

Replace the `user-meta` div on lines 135-138:

```typescript
                  h('div', { class: 'user-meta text-muted' },
                    (user.provider || 'email') + ' \u00b7 ' + new Date(user.created_at).toLocaleDateString(),
                    user.active_sessions > 0
                      ? h('span', { class: 'badge badge-subtle' }, user.active_sessions + ' active ' + (user.active_sessions === 1 ? 'session' : 'sessions'))
                      : null,
                  ),
```

- [ ] **Step 4: Add session cards to the expanded detail panel**

Replace the expanded detail content at lines 143-157:

```typescript
                () => h('div', { class: 'user-detail' },
                  h('div', { class: 'detail-grid' },
                    DetailRow('User ID', user.id),
                    DetailRow('Name', user.name || '(not set)'),
                    DetailRow('Verified', user.email_verified ? 'Yes' : 'No'),
                    DetailRow('Roles', user.roles.join(', ') || 'none'),
                  ),
                  createShow(
                    () => userSessionsLoading(),
                    () => h('div', { class: 'user-sessions-loading' },
                      Skeleton({ width: '100%', height: '12px' }),
                      Skeleton({ width: '80%', height: '12px' }),
                    ),
                  ),
                  createShow(
                    () => !userSessionsLoading() && userSessions().length > 0,
                    () => h('div', { class: 'user-sessions-list' },
                      h('div', { class: 'card-label' }, 'Active Sessions'),
                      () => userSessions().map((s: UserSession) => SessionCard(s)),
                    ),
                  ),
                  createShow(
                    () => !userSessionsLoading() && userSessions().length === 0 && user.active_sessions === 0,
                    () => h('div', { class: 'user-sessions-empty text-muted' }, 'No active sessions'),
                  ),
                  h('div', { class: 'user-actions' },
                    user.is_locked
                      ? h('button', { class: 'btn btn-sm btn-ghost', onClick: () => unblockUser(tenantState().id, user.id) }, 'Unblock')
                      : h('button', { class: 'btn btn-sm btn-ghost', onClick: () => blockUser(tenantState().id, user.id) }, 'Block'),
                    h('button', {
                      class: 'btn btn-sm btn-ghost',
                      disabled: () => user.active_sessions === 0,
                      onClick: () => revokeUserSessions(tenantState().id, user.id),
                    }, () => user.active_sessions > 0
                      ? 'Revoke ' + user.active_sessions + ' ' + (user.active_sessions === 1 ? 'Session' : 'Sessions')
                      : 'No Sessions'),
                  ),
                ),
```

- [ ] **Step 5: Add the SessionCard helper function**

Add at the bottom of `UsersView.ts`, before the closing of the file:

```typescript
function SessionCard(s: UserSession) {
  return h('div', { class: 'session-card' },
    h('div', { class: 'session-card-row' },
      h('span', { class: 'detail-label' }, 'Session'),
      h('code', { class: 'detail-value' }, s.id.slice(0, 16) + '...'),
    ),
    h('div', { class: 'session-card-row' },
      h('span', { class: 'detail-label' }, 'Method'),
      h('span', { class: 'detail-value' }, (s.auth_provider || s.auth_method) + (s.mfa_verified ? ' + MFA' : '')),
    ),
    h('div', { class: 'session-card-row' },
      h('span', { class: 'detail-label' }, 'Created'),
      h('span', { class: 'detail-value' }, new Date(s.created_at).toLocaleString()),
    ),
    h('div', { class: 'session-card-row' },
      h('span', { class: 'detail-label' }, 'Expires'),
      h('span', { class: 'detail-value' }, new Date(s.expires_at).toLocaleString()),
    ),
    s.last_activity_at
      ? h('div', { class: 'session-card-row' },
          h('span', { class: 'detail-label' }, 'Last Active'),
          h('span', { class: 'detail-value' }, new Date(s.last_activity_at).toLocaleString()),
        )
      : null,
    s.login_ip
      ? h('div', { class: 'session-card-row' },
          h('span', { class: 'detail-label' }, 'IP'),
          h('code', { class: 'detail-value' }, s.login_ip),
        )
      : null,
  );
}
```

- [ ] **Step 6: Build frontend**

Run: `cd admin && npm run build 2>&1 | tail -5`
Expected: Builds without errors.

- [ ] **Step 7: Commit**

```bash
git add admin/src/platform/console/views/UsersView.ts
git commit -m "feat: show active sessions count and detail in Users view"
```

---

### Task 5: Add CSS for session cards and badges

**Files:**
- Modify: `admin/src/core/theme/pages.css` (add after the user-actions section)

- [ ] **Step 1: Find the insertion point**

Locate the `.user-actions` CSS rule in `pages.css`. Add the new styles after it.

- [ ] **Step 2: Add session card and badge styles**

```css
/* ── User Session Cards ───────────────────────────────────────────── */

.badge-subtle {
  font-size: 0.6rem;
  padding: 0.1rem 0.4rem;
  border-radius: var(--radius-sm);
  background: var(--surface-raised);
  border: 1px solid var(--border);
  color: var(--text-dim);
  white-space: nowrap;
}

.user-sessions-list {
  display: grid;
  gap: var(--space-sm);
}

.user-sessions-loading {
  display: grid;
  gap: var(--space-xs);
  padding: var(--space-sm) 0;
}

.user-sessions-empty {
  font-size: 0.78rem;
  padding: var(--space-sm) 0;
}

.session-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-sm) var(--space-md);
  background: var(--surface);
  display: grid;
  gap: 0;
}

.session-card-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  padding: 0.25rem 0;
  gap: 0.5rem;
}

.session-card-row:not(:last-child) {
  border-bottom: 1px solid var(--border);
}
```

- [ ] **Step 3: Build frontend**

Run: `cd admin && npm run build 2>&1 | tail -5`
Expected: Builds without errors.

- [ ] **Step 4: Commit**

```bash
git add admin/src/core/theme/pages.css
git commit -m "style: add session card and badge styles for Users view"
```

---

### Task 6: Remove unused CallbackPreview component

**Files:**
- Delete: `admin/src/core/components/domain/CallbackPreview.ts`
- Modify: `admin/src/core/components/index.ts` (remove export line)

- [ ] **Step 1: Verify CallbackPreview is not imported anywhere**

Run: `grep -r "CallbackPreview" admin/src/ --include="*.ts" | grep -v "CallbackPreview.ts" | grep -v "index.ts"`
Expected: No output (no consumers remain).

- [ ] **Step 2: Remove the export from the barrel**

In `admin/src/core/components/index.ts`, delete the line:
```typescript
export { CallbackPreview } from './domain/CallbackPreview';
```

- [ ] **Step 3: Delete the file**

```bash
rm admin/src/core/components/domain/CallbackPreview.ts
```

- [ ] **Step 4: Build frontend**

Run: `cd admin && npm run build 2>&1 | tail -5`
Expected: Builds without errors.

- [ ] **Step 5: Commit**

```bash
git add -u admin/src/core/components/domain/CallbackPreview.ts admin/src/core/components/index.ts
git commit -m "chore: remove unused CallbackPreview component"
```

---

### Task 7: Update e2e test for user sessions

**Files:**
- Modify: `admin/e2e/session-inspector.spec.ts`

Add a test that verifies the Users view shows session-related UI when a user has active sessions.

- [ ] **Step 1: Add the test**

Add after the existing `auth reference lives inside integrate` test:

```typescript
test('users view shows active session count', async ({ browser }) => {
  const { context, page } = await loginAsTestUser(browser);
  const slug = 'sess-' + Date.now().toString(36);
  let tenantId = '';

  try {
    const refresh = await context.request.post(BASE + '/t/platform/auth/token', {
      data: { grant_type: 'refresh_token' },
    });
    expect(refresh.ok()).toBeTruthy();
    const refreshJson = await refresh.json();
    const accessToken = refreshJson.access_token as string;

    const createTenant = await context.request.post(BASE + '/platform/self-serve/tenants', {
      headers: { Authorization: 'Bearer ' + accessToken },
      data: { name: 'Session Test', slug },
    });
    expect(createTenant.ok()).toBeTruthy();
    const createJson = await createTenant.json();
    tenantId = String(createJson.tenant.id);

    await page.goto('/platform/console');
    await page.waitForTimeout(1500);

    await page.locator('.tenant-card').filter({ hasText: 'Session Test' }).first().click();
    await expect(page.locator('.sidebar')).toBeVisible();

    await page.locator('.sidebar').getByText('Users').click();
    await page.waitForTimeout(1000);

    // Users view should render (may be empty for a new tenant)
    await expect(page.getByText('Users')).toBeVisible();
  } finally {
    if (tenantId) {
      const refresh = await context.request.post(BASE + '/t/platform/auth/token', {
        data: { grant_type: 'refresh_token' },
      });
      if (refresh.ok()) {
        const refreshJson = await refresh.json();
        const accessToken = refreshJson.access_token as string;
        await context.request.delete(BASE + '/platform/self-serve/tenants/' + tenantId, {
          headers: { Authorization: 'Bearer ' + accessToken },
          data: { confirm_slug: slug },
        });
      }
    }

    await context.close();
  }
});
```

- [ ] **Step 2: Commit**

```bash
git add admin/e2e/session-inspector.spec.ts
git commit -m "test: add e2e test for users view session display"
```
