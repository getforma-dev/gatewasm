# WS Email Verification, Resend Fix, and Tab Persistence

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three console UX issues — real-time email verification via WebSocket, broken resend button, and URL-hash-based tab persistence on refresh.

**Architecture:** Add `account:updated` WS topic to backend, emit it on email verification, subscribe in frontend to refresh user state. Fix double-prefixed API path on resend. Encode active tab/view in URL hash and restore on load/popstate.

**Tech Stack:** Rust/Axum (backend WS), TypeScript/FormaJS (frontend signals + DOM)

---

### Task 1: Add `account:updated` WS topic and emit helper

**Files:**
- Modify: `crates/server/src/ws/handler.rs:21` (VALID_TOPICS)
- Modify: `crates/server/src/ws/events.rs` (add emit_account_updated)

- [ ] **Step 1: Add `account:updated` to VALID_TOPICS**

In `crates/server/src/ws/handler.rs`, line 21, change:

```rust
const VALID_TOPICS: &[&str] = &["team:changed", "tenant:activity", "metrics:updated"];
```

to:

```rust
const VALID_TOPICS: &[&str] = &["team:changed", "tenant:activity", "metrics:updated", "account:updated"];
```

- [ ] **Step 2: Add emit helper in events.rs**

In `crates/server/src/ws/events.rs`, add after the `emit_team_changed` function (after line 30):

```rust
/// Emits `account:updated`. Called when operator account state changes
/// (e.g. email verified).
pub fn emit_account_updated(bus: &EventBus, tenant_id: &str, hint: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    bus.send(WsEvent {
        tenant_id: tenant_id.to_string(),
        topic: "account:updated".into(),
        hint: Some(hint.into()),
        timestamp: now,
    });
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check -p gatewasm-server 2>&1 | tail -5`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/ws/handler.rs crates/server/src/ws/events.rs
git commit -m "feat(ws): add account:updated topic and emit helper"
```

---

### Task 2: Emit `account:updated` WS event on email verification

**Files:**
- Modify: `crates/server/src/handlers/auth.rs:1853-1870` (verify_email handler)

- [ ] **Step 1: Add WS event emission after webhook dispatch**

In `crates/server/src/handlers/auth.rs`, the `verify_email` function already queries `tenant_id` for webhook dispatch (lines 1856-1861). After the webhook dispatch block (after line 1870), add:

```rust
    // Notify connected WS clients so the verification banner disappears
    if let Some(ref tid) = tenant_id {
        crate::ws::events::emit_account_updated(&state.event_bus, tid, "email_verified");
    }
```

The full section from line 1853 to the new code should read:

```rust
    tracing::info!(user_id = %user_id, "Email verified");

    // Dispatch webhook — look up tenant_id for this user
    let tenant_id: Option<String> = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    if let Some(ref tid) = tenant_id {
        super::webhook_engine::dispatch_event(
            &state.db,
            tid,
            "user.email_verified",
            serde_json::json!({ "user_id": user_id }),
        )
        .await;
    }

    // Notify connected WS clients so the verification banner disappears
    if let Some(ref tid) = tenant_id {
        crate::ws::events::emit_account_updated(&state.event_bus, tid, "email_verified");
    }
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check -p gatewasm-server 2>&1 | tail -5`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/handlers/auth.rs
git commit -m "feat(auth): emit WS account:updated on email verification"
```

---

### Task 3: Subscribe to `account:updated` in frontend

**Files:**
- Modify: `admin/src/platform/console/store.ts:1431-1454` (WS subscriptions section)

- [ ] **Step 1: Add subscription**

In `admin/src/platform/console/store.ts`, after the existing `tenant:activity` subscription block (after line 1454), add:

```ts
socket.subscribe('account:updated', () => {
  loadCurrentUser();
});
```

- [ ] **Step 2: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npx vite build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat(console): subscribe to account:updated WS topic for live verification"
```

---

### Task 4: Fix double-prefixed resend verification API path

**Files:**
- Modify: `admin/src/platform/console/store.ts:1593` (resendVerification function)

- [ ] **Step 1: Fix the API path**

The bug: `api.post` prepends `auth.baseUrl` which is `serverUrl + '/t/platform'`. The current call uses path `/t/platform/auth/resend-verification`, producing the double-prefixed URL `serverUrl/t/platform/t/platform/auth/resend-verification`.

In `admin/src/platform/console/store.ts`, line 1593, change:

```ts
    await api.post('/t/platform/auth/resend-verification', {});
```

to:

```ts
    await api.post('/auth/resend-verification', {});
```

- [ ] **Step 2: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npx vite build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "fix(console): correct double-prefixed resend-verification API path"
```

---

### Task 5: URL hash persistence for account tabs and workspace views

**Files:**
- Modify: `admin/src/platform/console/store.ts` — multiple locations:
  - Lines 382 (accountTab signal)
  - Lines 1284-1292 (navigateTenantView)
  - Lines 1370-1381 (switchToWorkspaceInternal)
  - Lines 1385-1400 (switchToAccountInternal)
  - Lines 1402-1410 (selectTenant / backToTenants)
  - Lines 1412-1429 (initRouting)
  - Lines 1458-1530 (init)

- [ ] **Step 1: Add hash read/write helpers**

In `admin/src/platform/console/store.ts`, after the `AccountTab` type definition (after line 269), add:

```ts
// ── URL hash helpers ───────────────────────────────────────────────────

const VALID_ACCOUNT_TABS: readonly string[] = ['tenants', 'team', 'api-keys', 'account-settings'];
const VALID_CONSOLE_VIEWS: readonly string[] = [
  'overview', 'integrate', 'security', 'members', 'hosted-page',
  'organizations', 'roles', 'audit-log', 'sessions', 'passkeys', 'tokens', 'activity',
];

function readHash(): string {
  return window.location.hash.replace(/^#/, '');
}

function writeHash(value: string): void {
  const url = window.location.pathname + window.location.search + (value ? '#' + value : '');
  history.replaceState(history.state, '', url);
}
```

- [ ] **Step 2: Update `setAccountTab` calls to write hash**

In `admin/src/platform/console/store.ts`, line 382, the signal is currently:

```ts
export const [accountTab, setAccountTab] = createSignal<AccountTab>('tenants');
```

Change to a private signal with a public wrapper:

```ts
const [accountTab, _setAccountTab] = createSignal<AccountTab>('tenants');
export { accountTab };
export function setAccountTab(tab: AccountTab): void {
  _setAccountTab(tab);
  if (mode() === 'account') writeHash(tab === 'tenants' ? '' : tab);
}
```

- [ ] **Step 3: Update `navigateTenantView` to write hash**

In `admin/src/platform/console/store.ts`, the `navigateTenantView` function (line 1284) currently is:

```ts
export function navigateTenantView(nextView: TenantWorkspaceView): void {
  const tenant = selectedTenant();
  if (!tenant) return;

  setView(nextView);
  setPlaneSignal(planeForView(nextView));

  void hydrateTenantView(nextView, tenant);
}
```

Add hash write after `setView`:

```ts
export function navigateTenantView(nextView: TenantWorkspaceView): void {
  const tenant = selectedTenant();
  if (!tenant) return;

  setView(nextView);
  setPlaneSignal(planeForView(nextView));
  writeHash(nextView === 'overview' ? '' : nextView);

  void hydrateTenantView(nextView, tenant);
}
```

- [ ] **Step 4: Clear hash on mode transitions**

In `switchToAccountInternal` (line 1385), add hash restore at the end of the function, after line 1400 (after the closing `});`):

```ts
  const h = readHash();
  if (VALID_ACCOUNT_TABS.includes(h)) {
    _setAccountTab(h as AccountTab);
  } else {
    _setAccountTab('tenants');
  }
  writeHash(accountTab() === 'tenants' ? '' : accountTab());
```

In `switchToWorkspaceInternal` (line 1370), after `setView(nextView);` on line 1375, the hash is already set by `navigateTenantView` for user-driven navigation. But for popstate/init restoration, add after line 1381:

```ts
  const h = readHash();
  if (VALID_CONSOLE_VIEWS.includes(h)) {
    setView(h as ConsoleView);
    setPlaneSignal(planeForView(h as ConsoleView));
    void hydrateTenantView(h as ConsoleView, tenant);
  }
  writeHash(view() === 'overview' ? '' : view());
```

Wait — `switchToWorkspaceInternal` already sets the view via `preferredTenantView`. The hash should override it. Restructure:

Replace `switchToWorkspaceInternal` (lines 1370-1382) entirely:

```ts
function switchToWorkspaceInternal(tenant: Tenant): void {
  const h = readHash();
  const nextView = VALID_CONSOLE_VIEWS.includes(h)
    ? (h as ConsoleView)
    : preferredTenantView(tenant);
  batch(() => {
    setSelectedTenant(tenant);
    resetTenantScopedState();
    setView(nextView);
    setPlaneSignal(planeForView(nextView));
    setMode('workspace');
  });
  writeHash(nextView === 'overview' ? '' : nextView);
  void hydrateTenantView(nextView, tenant);
}
```

And replace `switchToAccountInternal` (lines 1385-1400) — preserve all existing seq bumps and calls:

```ts
function switchToAccountInternal(): void {
  clearOverviewRefresh();
  const h = readHash();
  const nextTab = VALID_ACCOUNT_TABS.includes(h)
    ? (h as AccountTab)
    : 'tenants';
  batch(() => {
    tenantDetailRequestSeq += 1;
    usersRequestSeq += 1;
    hostedPageRequestSeq += 1;
    introspectRequestSeq += 1;
    sessionRequestSeq += 1;
    setLoading(false);
    setSelectedTenant(null);
    resetTenantScopedState();
    setMode('account');
    _setAccountTab(nextTab);
  });
  writeHash(nextTab === 'tenants' ? '' : nextTab);
}
```

- [ ] **Step 5: Add `hashchange` listener in `initRouting`**

In `initRouting` (line 1412), add a `hashchange` listener after the existing `popstate` listener:

```ts
export function initRouting(): void {
  window.addEventListener('popstate', () => {
    const path = window.location.pathname;
    const match = path.match(/^\/platform\/console\/([^/]+)/);
    if (match) {
      const slug = match[1];
      const tenant = tenants().find((t) => t.slug === slug);
      if (tenant) {
        switchToWorkspaceInternal(tenant);
      } else {
        switchToAccountInternal();
        history.replaceState(null, '', '/platform/console');
      }
    } else {
      switchToAccountInternal();
    }
  });

  window.addEventListener('hashchange', () => {
    const h = readHash();
    if (mode() === 'account' && VALID_ACCOUNT_TABS.includes(h)) {
      _setAccountTab(h as AccountTab);
    } else if (mode() === 'workspace' && VALID_CONSOLE_VIEWS.includes(h)) {
      const tenant = selectedTenant();
      if (tenant) {
        setView(h as ConsoleView);
        setPlaneSignal(planeForView(h as ConsoleView));
        void hydrateTenantView(h as ConsoleView, tenant);
      }
    }
  });
}
```

- [ ] **Step 6: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npx vite build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 7: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat(console): URL hash persistence for account tabs and workspace views"
```

---

### Task 6: Manual verification

- [ ] **Step 1: Start dev environment**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run dev`

- [ ] **Step 2: Test tab persistence**

1. Navigate to `http://localhost:3001/platform/console`
2. Click "Members" tab → URL should show `#team`
3. Click "API Keys" → URL should show `#api-keys`
4. Refresh page → should stay on API Keys tab, URL still has `#api-keys`
5. Click "Tenants" → hash should clear (just `/platform/console`)
6. Enter a workspace → click "Sessions" → URL should show `#sessions`
7. Refresh → should stay on Sessions view

- [ ] **Step 3: Test resend button**

1. Sign up with a new test account (unverified email)
2. Verification banner should appear
3. Click "Resend" → toast should show "Verification email sent"
4. Check Mailpit at `http://localhost:8025` → verification email should be there

- [ ] **Step 4: Test real-time verification via WS**

1. With unverified account, console open at `http://localhost:3001/platform/console`
2. Open Mailpit, find verification email, click verify link
3. Banner in the console tab should disappear WITHOUT refreshing

- [ ] **Step 5: Commit any fixes if needed**
