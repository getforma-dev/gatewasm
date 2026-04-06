# Account/Workspace Mode Split Implementation Plan (Sub-plan A.5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separate tenant selection from the plane system by introducing two distinct shell modes: Account (no tenant, simple tabs for tenant list/settings) and Workspace (tenant selected, full PlaneNav with planes).

**Architecture:** Top-level render switches on a `mode` signal (`'account' | 'workspace'`). Account mode renders a minimal header + horizontal tab bar (Tenants, API Keys, Settings). Workspace mode renders PlaneNav (with Tenants/Settings removed from planes) + a tenant switcher in the header. The `ConsoleView` type splits into `AccountTab` and workspace `ConsoleView`. No existing view components are touched.

**Tech Stack:** TypeScript/FormaJS (frontend only — no backend changes)

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `admin/src/platform/console/store.ts` | **Modify** | Add `mode`/`accountTab` signals, remove `tenants`/`settings` from PLANES, update `backToTenants`/`selectTenant` |
| `admin/src/platform/console/ConsoleApp.ts` | **Rewrite** | Top-level mode switch, account shell component, tenant switcher in workspace |
| `admin/src/platform/console/views/PlaneNav.ts` | **Modify** | Add tenant switcher dropdown to header bar, remove Tenants from plane buttons |
| `admin/src/core/theme/components.css` | **Modify** | Add account shell CSS, tenant switcher CSS |

---

### Task 1: Update store — mode signal, PLANES cleanup, navigation functions

**Files:**
- Modify: `admin/src/platform/console/store.ts`

- [ ] **Step 1: Add mode and accountTab types and signals**

After the existing `Plane` type (line 124), add:

```typescript
export type AppMode = 'account' | 'workspace';
export type AccountTab = 'tenants' | 'api-keys' | 'account-settings';
```

After the `plane` signal (line 142), add:

```typescript
export const [mode, setMode] = createSignal<AppMode>('account');
export const [accountTab, setAccountTab] = createSignal<AccountTab>('tenants');
```

- [ ] **Step 2: Remove 'tenants' and 'settings' from ConsoleView and PLANES**

Update `ConsoleView` type — remove `'tenants'` and `'settings'`:

```typescript
export type ConsoleView =
  | 'overview'
  | 'integrate'
  | 'members'
  | 'hosted-page'
  | 'roles'
  | 'audit-log'
  | 'sessions'
  | 'passkeys'
  | 'tokens'
  | 'activity';
```

Update `TenantWorkspaceView` — it's now the same as `ConsoleView` (no exclusion needed):
```typescript
export type TenantWorkspaceView = ConsoleView;
```

Update `PLANES` — remove `tenants` from Plane 1:
```typescript
export const PLANES: { key: Plane; label: string; sector: string; num: number; tabs: PlaneTab[] }[] = [
  {
    key: 'operator', label: 'Operator', sector: 'SEC-01', num: 1,
    tabs: [
      { id: 'overview', label: 'Overview', shortcut: '\u2325\u0031' },
      { id: 'integrate', label: 'Integrate', shortcut: '\u2325\u0032' },
    ],
  },
  {
    key: 'organization', label: 'Organization', sector: 'SEC-02', num: 2,
    tabs: [
      { id: 'members', label: 'Members', shortcut: '\u2325\u0031' },
      { id: 'hosted-page', label: 'Hosted Page', shortcut: '\u2325\u0032' },
      { id: 'roles', label: 'Roles', shortcut: '\u2325\u0033' },
      { id: 'audit-log', label: 'Audit Log', shortcut: '\u2325\u0034' },
    ],
  },
  {
    key: 'enduser', label: 'End User', sector: 'SEC-03', num: 3,
    tabs: [
      { id: 'sessions', label: 'Sessions', shortcut: '\u2325\u0031' },
      { id: 'passkeys', label: 'Passkeys', shortcut: '\u2325\u0032' },
      { id: 'tokens', label: 'Tokens', shortcut: '\u2325\u0033' },
      { id: 'activity', label: 'Activity', shortcut: '\u2325\u0034' },
    ],
  },
];
```

- [ ] **Step 3: Update `selectTenant` to set mode to 'workspace'**

```typescript
export function selectTenant(tenant: Tenant): void {
  const nextView = preferredTenantView(tenant);

  batch(() => {
    setSelectedTenant(tenant);
    resetTenantScopedState();
    setView(nextView);
    setPlaneSignal(planeForView(nextView));
    setMode('workspace');
  });

  persistWorkspaceState();
  void hydrateTenantView(nextView, tenant);
}
```

- [ ] **Step 4: Update `backToTenants` to set mode to 'account'**

```typescript
export function backToTenants(): void {
  clearOverviewRefresh();

  batch(() => {
    tenantDetailRequestSeq += 1;
    usersRequestSeq += 1;
    hostedPageRequestSeq += 1;
    introspectRequestSeq += 1;
    setLoading(false);
    setSelectedTenant(null);
    resetTenantScopedState();
    setMode('account');
    setAccountTab('tenants');
  });
  persistWorkspaceState();
}
```

- [ ] **Step 5: Update `PersistedConsoleWorkspace` to include mode and accountTab**

```typescript
interface PersistedConsoleWorkspace {
  tenantId: string | null;
  view: ConsoleView;
  integrateSection: IntegrateSection;
  plane: Plane;
  mode: AppMode;
  accountTab: AccountTab;
}
```

Update `persistWorkspaceState` to save `mode()` and `accountTab()`.
Update `restoreWorkspaceState` to restore them with fallbacks:
- If `tenantId` is null → mode = 'account'
- If `tenantId` is set → mode = 'workspace'

- [ ] **Step 6: Update `readWorkspaceState` validViews**

Remove `'tenants'` and `'settings'` from the `validViews` array. Add `'account-settings'` and `'api-keys'` to a separate `validAccountTabs` check. Handle backward compat: if persisted view is `'tenants'` or `'settings'`, set mode to `'account'` and accountTab appropriately.

- [ ] **Step 7: Update `preferredTenantView`**

Find `preferredTenantView`. It currently returns a `TenantWorkspaceView`. Make sure it doesn't return `'tenants'` or `'settings'` — those are no longer valid. Default to `'overview'`.

- [ ] **Step 8: Export new signals and functions**

Make sure `mode`, `setMode`, `accountTab`, `setAccountTab`, `AppMode`, `AccountTab` are all exported.

- [ ] **Step 9: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 10: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat: add account/workspace mode split to store — separate tenant selection from planes"
```

---

### Task 2: Add tenant switcher to PlaneNav header

**Files:**
- Modify: `admin/src/platform/console/views/PlaneNav.ts`

Add a tenant switcher dropdown between the logo and the center status area. It shows the current tenant name + chevron, and clicking it opens a dropdown with all tenants + a "All Tenants" option to go back to account mode.

- [ ] **Step 1: Update PlaneNavProps**

Add new props:

```typescript
export interface PlaneNavProps {
  plane: () => Plane;
  view: () => ConsoleView;
  onPlaneSwitch: (plane: Plane) => void;
  onTabSwitch: (view: ConsoleView) => void;
  onSettings: () => void;
  onSignOut: () => void;
  user: () => PlatformUser | null;
  tenant: () => Tenant | null;
  tenants: () => Tenant[];
  onTenantSwitch: (tenant: Tenant) => void;
  onBackToAccount: () => void;
}
```

- [ ] **Step 2: Build the tenant switcher**

In the header bar, between the logo group and the center status, add a tenant switcher. It's a button showing the tenant name + chevron that opens a dropdown:

```typescript
const [tenantMenuOpen, setTenantMenuOpen] = createSignal(false);

const tenantSwitcher = h('div', { class: 'plane-tenant-switcher' },
  h('button', {
    type: 'button',
    class: 'plane-tenant-trigger hv',
    onClick: () => setTenantMenuOpen(!tenantMenuOpen()),
  },
    h('span', { class: 'plane-tenant-name' }, () => props.tenant()?.name || ''),
    h('span', { class: 'plane-tenant-chevron' }, '\u25BE'),
  ),
  createShow(tenantMenuOpen, () =>
    h('div', { class: 'plane-tenant-menu' },
      h('button', {
        type: 'button',
        class: 'plane-tenant-menu-item plane-tenant-menu-back hv',
        onClick: () => { setTenantMenuOpen(false); props.onBackToAccount(); },
      }, '\u2190 All Tenants'),
      h('div', { class: 'plane-avatar-menu-divider' }),
      () => props.tenants().map((t) =>
        h('button', {
          type: 'button',
          class: () => 'plane-tenant-menu-item hv' + (props.tenant()?.id === t.id ? ' active' : ''),
          onClick: () => { setTenantMenuOpen(false); props.onTenantSwitch(t); },
        },
          h('span', null, t.name),
          h('span', { class: 'plane-tenant-menu-slug' }, '/t/' + t.slug),
        ),
      ),
    ),
  ),
);
```

Insert `tenantSwitcher` in the header left cluster, after the domain text.

- [ ] **Step 3: Remove old tenant indicator**

Remove the old `.plane-tenant-indicator` element (the plain text display). The tenant switcher replaces it — it shows the name AND is interactive.

- [ ] **Step 4: Close tenant menu on outside click**

Add outside-click handling for the tenant menu (same pattern as the avatar menu).

- [ ] **Step 5: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 6: Commit**

```bash
git add admin/src/platform/console/views/PlaneNav.ts
git commit -m "feat: add tenant switcher dropdown to PlaneNav header"
```

---

### Task 3: Add CSS for account shell and tenant switcher

**Files:**
- Modify: `admin/src/core/theme/components.css`

- [ ] **Step 1: Add account shell CSS**

Add a new section for the account shell:

```css
/* ==========================================================================
   Account Shell (no tenant selected)
   ========================================================================== */

.account-shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

.account-header {
  height: var(--header-height);
  background: #101018;
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  padding: 0 28px;
  flex-shrink: 0;
}

.account-header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.account-header-right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 8px;
}

.account-tabs {
  display: flex;
  gap: 0;
  padding: 0 28px;
  background: #0E0E16;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.account-tab {
  padding: 10px 20px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-muted);
  font-family: inherit;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 150ms ease;
}

.account-tab:hover {
  color: var(--text-dim);
}

.account-tab.active {
  color: var(--text);
  border-bottom-color: var(--accent);
  font-weight: 600;
}

.account-stage {
  flex: 1 1 auto;
  overflow-y: auto;
}
```

- [ ] **Step 2: Add tenant switcher CSS**

```css
/* ── Tenant Switcher ──────────────────────────────────────────────── */

.plane-tenant-switcher {
  position: relative;
  margin-left: 8px;
  padding-left: 12px;
  border-left: 1px solid var(--border);
}

.plane-tenant-trigger {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: transparent;
  cursor: pointer;
  font-family: inherit;
  color: inherit;
}

.plane-tenant-name {
  font-family: Rajdhani, sans-serif;
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
  max-width: 180px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.plane-tenant-chevron {
  font-size: 10px;
  color: var(--text-muted);
}

.plane-tenant-menu {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  min-width: 240px;
  max-height: 320px;
  overflow-y: auto;
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 6px 0;
  z-index: 100;
  box-shadow: var(--shadow-lg);
}

.plane-tenant-menu-item {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 14px;
  border: none;
  background: transparent;
  cursor: pointer;
  font-family: inherit;
  font-size: 13px;
  color: var(--text-dim);
  text-align: left;
}

.plane-tenant-menu-item.active {
  color: var(--plane);
}

.plane-tenant-menu-back {
  color: var(--text-muted);
  font-size: 12px;
}

.plane-tenant-menu-slug {
  font-family: 'DM Mono', monospace;
  font-size: 11px;
  color: var(--text-muted);
}
```

- [ ] **Step 3: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add admin/src/core/theme/components.css
git commit -m "style: add account shell and tenant switcher CSS"
```

---

### Task 4: Rewrite ConsoleApp with mode switch

**Files:**
- Modify: `admin/src/platform/console/ConsoleApp.ts`

This is the integration task. The top-level render now switches on `mode()`:
- `'account'` → renders AccountShell (minimal header + tab bar + content)
- `'workspace'` → renders the existing PlaneNav + workspace views

- [ ] **Step 1: Update imports**

Add new store imports:
```typescript
import {
  view, loading, toast, backToTenants, signout,
  selectedTenant, tenants, selectTenant, createTenant, currentUser,
  navigateTenantView, refreshCurrentView, openIntegrateGuide, openSessionInspector,
  plane, setPlane, mode, accountTab, setAccountTab,
  type Plane, type ConsoleView, type TenantWorkspaceView, type AccountTab,
} from './store';
```

- [ ] **Step 2: Build AccountShell component**

Create an inline `AccountShell` function that renders:
- A minimal header: logo mark (using default accent `--accent`) + "GateWASM" product name + avatar button (with signout only — no settings since Settings IS a tab in account mode)
- Three tab buttons: Tenants (default), API Keys, Settings
- Content area switching on `accountTab()`:
  - `'tenants'` → existing `TenantsList()` component
  - `'api-keys'` → `ComingSoonView('API Keys')`
  - `'account-settings'` → `ComingSoonView('Account Settings')`

```typescript
function AccountShell() {
  const user = currentUser();
  return h('div', { class: 'account-shell' },
    h('header', { class: 'account-header' },
      h('div', { class: 'account-header-left' },
        h('div', { class: 'plane-logo-mark' }),
        h('span', { class: 'plane-product-name' }, 'GateWASM'),
      ),
      h('div', { class: 'account-header-right' },
        h('button', {
          type: 'button',
          class: 'plane-avatar hv',
          onClick: () => void signout(),
        }, user ? userInitials(user.name, user.email) : '--'),
      ),
    ),
    h('div', { class: 'account-tabs' },
      AccountTabBtn('tenants', 'Tenants'),
      AccountTabBtn('api-keys', 'API Keys'),
      AccountTabBtn('account-settings', 'Settings'),
    ),
    h('div', { class: 'account-stage' },
      h('main', { class: 'main page-enter' },
        LoadingBar(() => loading()),
        createSwitch(
          accountTab,
          [
            { match: 'tenants', render: () => TenantsList() },
            { match: 'api-keys', render: () => ComingSoonView('API Keys') },
            { match: 'account-settings', render: () => ComingSoonView('Account Settings') },
          ],
        ),
      ),
    ),
  );
}

function AccountTabBtn(id: AccountTab, label: string) {
  return h('button', {
    type: 'button',
    class: () => 'account-tab' + (accountTab() === id ? ' active' : ''),
    onClick: () => setAccountTab(id),
  }, label);
}
```

- [ ] **Step 3: Update the top-level render**

Replace the current `return h('div', { class: 'shell', ... }, PlaneNav(...), ...)` with a mode switch:

```typescript
return h('div', null,
  createSwitch(
    mode,
    [
      { match: 'account', render: () => AccountShell() },
      {
        match: 'workspace',
        render: () => h('div', {
          class: 'shell',
          'data-plane': () => plane(),
        },
          PlaneNav({
            plane: () => plane(),
            view: () => view(),
            onPlaneSwitch: (p: Plane) => setPlane(p),
            onTabSwitch: (v: ConsoleView) => navigateTenantView(v),
            onSettings: () => backToTenants(),  // Settings is now in account mode
            onSignOut: () => void signout(),
            user: () => currentUser(),
            tenant: () => selectedTenant(),
            tenants: () => tenants(),
            onTenantSwitch: (t) => selectTenant(t),
            onBackToAccount: () => backToTenants(),
          }),
          h('div', { class: 'shell-stage' },
            h('main', { class: 'main page-enter' },
              LoadingBar(() => loading()),
              createSwitch(
                () => view(),
                [
                  { match: 'overview', render: () => OverviewView() },
                  { match: 'members', render: () => UsersView() },
                  { match: 'integrate', render: () => IntegrateView() },
                  { match: 'hosted-page', render: () => HostedPageView() },
                  { match: 'roles', render: () => ComingSoonView('Roles') },
                  { match: 'audit-log', render: () => ComingSoonView('Audit Log') },
                  { match: 'sessions', render: () => ComingSoonView('Sessions') },
                  { match: 'passkeys', render: () => ComingSoonView('Passkeys') },
                  { match: 'tokens', render: () => ComingSoonView('Tokens') },
                  { match: 'activity', render: () => ComingSoonView('Activity') },
                ],
              ),
            ),
          ),
          CommandPalette({
            open: paletteOpen,
            onClose: closeCommandPalette,
            shortcutLabel,
            sections: paletteSections,
          }),
          ToastView(toast),
        ),
      },
    ],
  ),
);
```

Note: In workspace mode, no more `TenantWorkspaceGuard` — if mode is 'workspace', a tenant IS selected. The guard is no longer needed. Also `SettingsView` is removed from workspace routes — Settings is accessed via account mode.

Also note: `CommandPalette` and `ToastView` should be inside the workspace shell. For account mode, the command palette isn't needed (or you can add it later).

- [ ] **Step 4: Update command palette**

Remove the 'tenants' and 'settings' palette entries from workspace mode. The "Your Tenants" entry becomes "← All Tenants" and calls `backToTenants()` (switches to account mode).

- [ ] **Step 5: Keep helper functions**

Keep `userInitials` and `userDisplayName` (used by avatar). Keep `ComingSoonView`. Remove `TenantWorkspaceGuard` (no longer needed).

- [ ] **Step 6: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 7: Commit**

```bash
git add admin/src/platform/console/ConsoleApp.ts
git commit -m "feat: split console into account/workspace modes — tenant selection separated from planes"
```

---

### Task 5: Update e2e tests

**Files:**
- Modify: `admin/e2e/session-inspector.spec.ts`

The navigation flow is now: land on account shell → click a tenant card → enter workspace mode. Update test selectors.

- [ ] **Step 1: Update navigation flow in tests**

The old flow was: go to console → click tenant card → click sidebar/plane tab. The new flow is: go to console → in account mode, click tenant card → enters workspace mode with PlaneNav → click plane/tab.

Update selectors:
- Clicking a tenant card now triggers mode switch to 'workspace'
- Plane tabs work the same in workspace mode
- "Users" is now in Organization plane as "Members"

- [ ] **Step 2: Build and commit**

```bash
git add admin/e2e/session-inspector.spec.ts
git commit -m "test: update e2e tests for account/workspace mode split"
```
