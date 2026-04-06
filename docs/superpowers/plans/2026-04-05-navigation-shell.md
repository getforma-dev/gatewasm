# Navigation Shell Implementation Plan (Sub-plan A)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the sidebar navigation with a two-tier horizontal tab system organized around three planes (Operator, Organization, End User), wiring all 8 existing views into their new locations.

**Architecture:** The shell changes from `[sidebar | stage]` grid to a vertical stack: `[header | tier1 | tier2 | content]`. A new `PlaneNav` component replaces the `Sidebar`. The store's `ConsoleView` type expands to encode plane+tab. Three new fonts (Rajdhani, Orbitron, DM Mono already present) and per-plane accent colors drive the theming. Existing view components mount unchanged — only the navigation wrapper changes.

**Tech Stack:** TypeScript/FormaJS (frontend), CSS custom properties, self-hosted WOFF2 fonts

**Sequencing note:** This is Sub-plan A. Sub-plan B (new views for Roles, Audit Log, Sessions, Passkeys, Tokens, Activity) builds on this shell and will be planned separately after A ships.

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `admin/src/platform/console/ConsoleApp.ts` | **Rewrite** | Replace Sidebar with PlaneNav, rewire view switching |
| `admin/src/platform/console/store.ts` | **Modify** | New `Plane` type, updated `ConsoleView`, `setPlane()`, `setTab()` |
| `admin/src/platform/console/views/PlaneNav.ts` | **Create** | Header bar + Tier 1 (plane switcher) + Tier 2 (page tabs) + collapsed mini-bar |
| `admin/src/core/theme/tokens.css` | **Modify** | Add Rajdhani + Orbitron fonts, plane accent CSS vars |
| `admin/src/core/theme/components.css` | **Modify** | Replace sidebar styles with plane-nav styles, update shell grid |
| `admin/src/core/theme/pages.css` | **Modify** | Update `.console-view` max-width (full width now), remove sidebar refs |

---

### Task 1: Add new fonts and plane color tokens

**Files:**
- Modify: `admin/src/core/theme/tokens.css`

Add Rajdhani and Orbitron font-face declarations (self-hosted WOFF2 files need to be added to the assets directory) and CSS custom properties for the three-plane accent system.

- [ ] **Step 1: Download and add font files**

Download WOFF2 files for Rajdhani (400, 500, 600, 700) and Orbitron (500, 700) from Google Fonts. Place them in the project's static assets directory alongside the existing DM Mono and Instrument Serif files. Find where assets are served from by checking the existing font URLs in `tokens.css` (they use `/_assets/` prefix). The font files should be placed wherever the dev asset server or production build serves `/_assets/` from.

Run: `ls crates/server/assets/` or `grep -r "dm-mono" crates/` to find the font file location.

- [ ] **Step 2: Add font-face declarations to tokens.css**

After the existing Instrument Serif font-face block (around line 40), add:

```css
@font-face {
  font-family: 'Rajdhani';
  font-style: normal;
  font-weight: 400;
  font-display: swap;
  src: url('/_assets/rajdhani-400.woff2') format('woff2');
}

@font-face {
  font-family: 'Rajdhani';
  font-style: normal;
  font-weight: 500;
  font-display: swap;
  src: url('/_assets/rajdhani-500.woff2') format('woff2');
}

@font-face {
  font-family: 'Rajdhani';
  font-style: normal;
  font-weight: 600;
  font-display: swap;
  src: url('/_assets/rajdhani-600.woff2') format('woff2');
}

@font-face {
  font-family: 'Rajdhani';
  font-style: normal;
  font-weight: 700;
  font-display: swap;
  src: url('/_assets/rajdhani-700.woff2') format('woff2');
}

@font-face {
  font-family: 'Orbitron';
  font-style: normal;
  font-weight: 500;
  font-display: swap;
  src: url('/_assets/orbitron-500.woff2') format('woff2');
}

@font-face {
  font-family: 'Orbitron';
  font-style: normal;
  font-weight: 700;
  font-display: swap;
  src: url('/_assets/orbitron-700.woff2') format('woff2');
}
```

- [ ] **Step 3: Add plane color CSS custom properties**

In the `:root[data-theme='dark']` block, after the existing accent vars (around line 72), add:

```css
  /* ── Plane Accents ─────────────────────────────────────────────── */
  --plane1: #F05A3A;
  --plane1-glow: rgba(240, 90, 58, 0.25);
  --plane1-muted: rgba(240, 90, 58, 0.08);
  --plane1-border: rgba(240, 90, 58, 0.25);
  --plane2: #2E9BFF;
  --plane2-glow: rgba(46, 155, 255, 0.25);
  --plane2-muted: rgba(46, 155, 255, 0.08);
  --plane2-border: rgba(46, 155, 255, 0.25);
  --plane3: #1EDC82;
  --plane3-glow: rgba(30, 220, 130, 0.25);
  --plane3-muted: rgba(30, 220, 130, 0.08);
  --plane3-border: rgba(30, 220, 130, 0.25);

  /* Active plane (set dynamically via JS on the shell element) */
  --plane: var(--plane1);
  --plane-glow: var(--plane1-glow);
  --plane-muted: var(--plane1-muted);
  --plane-border: var(--plane1-border);
```

Also add the same vars (with appropriate light-mode values) in the light theme block.

- [ ] **Step 4: Add nav layout tokens**

In the spacing/layout section of `:root`, update the shell layout vars:

```css
  --header-height: 56px;
  --tier1-height: 52px;
  --tier2-height: 42px;
  --collapsed-nav-height: 42px;
```

- [ ] **Step 5: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 6: Commit**

```bash
git add admin/src/core/theme/tokens.css
git commit -m "feat: add Rajdhani + Orbitron fonts and three-plane color tokens"
```

---

### Task 2: Update store with plane/tab navigation model

**Files:**
- Modify: `admin/src/platform/console/store.ts`

Replace the flat `ConsoleView` type with a plane+tab model. Keep backward compatibility for view hydration.

- [ ] **Step 1: Add plane types and constants**

After the existing type definitions (around line 116), add:

```typescript
export type Plane = 'operator' | 'organization' | 'enduser';

export interface PlaneTab {
  id: string;
  label: string;
  shortcut: string;
}

export const PLANES: { key: Plane; label: string; sector: string; num: number; tabs: PlaneTab[] }[] = [
  {
    key: 'operator', label: 'Operator', sector: 'SEC-01', num: 1,
    tabs: [
      { id: 'overview', label: 'Overview', shortcut: '\u23251' },
      { id: 'tenants', label: 'Tenants', shortcut: '\u23252' },
      { id: 'integrate', label: 'Integrate', shortcut: '\u23253' },
      { id: 'settings', label: 'Settings', shortcut: '\u23254' },
    ],
  },
  {
    key: 'organization', label: 'Organization', sector: 'SEC-02', num: 2,
    tabs: [
      { id: 'members', label: 'Members', shortcut: '\u23251' },
      { id: 'hosted-page', label: 'Hosted Page', shortcut: '\u23252' },
      { id: 'roles', label: 'Roles', shortcut: '\u23253' },
      { id: 'audit-log', label: 'Audit Log', shortcut: '\u23254' },
    ],
  },
  {
    key: 'enduser', label: 'End User', sector: 'SEC-03', num: 3,
    tabs: [
      { id: 'sessions', label: 'Sessions', shortcut: '\u23251' },
      { id: 'passkeys', label: 'Passkeys', shortcut: '\u23252' },
      { id: 'tokens', label: 'Tokens', shortcut: '\u23253' },
      { id: 'activity', label: 'Activity', shortcut: '\u23254' },
    ],
  },
];
```

- [ ] **Step 2: Update ConsoleView type**

Replace the existing `ConsoleView` type (line 106) with:

```typescript
export type ConsoleView =
  | 'tenants'
  | 'overview'
  | 'integrate'
  | 'settings'
  | 'members'
  | 'hosted-page'
  | 'roles'
  | 'audit-log'
  | 'sessions'
  | 'passkeys'
  | 'tokens'
  | 'activity';
```

Update `TenantWorkspaceView` to exclude `'tenants'` (same as before):
```typescript
export type TenantWorkspaceView = Exclude<ConsoleView, 'tenants'>;
```

- [ ] **Step 3: Add plane signal and navigation functions**

After the existing signals (around line 141), add:

```typescript
export const [plane, setPlaneSignal] = createSignal<Plane>('operator');
```

Add a `setPlane` function that resets to the first tab of the target plane:

```typescript
export function setPlane(nextPlane: Plane): void {
  if (plane() === nextPlane) return;
  setPlaneSignal(nextPlane);
  const planeDef = PLANES.find((p) => p.key === nextPlane);
  if (planeDef) {
    const firstTab = planeDef.tabs[0].id as ConsoleView;
    navigateTenantView(firstTab);
  }
}
```

Add a helper to derive plane from current view:

```typescript
export function planeForView(v: ConsoleView): Plane {
  for (const p of PLANES) {
    if (p.tabs.some((t) => t.id === v)) return p.key;
  }
  return 'operator';
}
```

- [ ] **Step 4: Update navigateTenantView to also set plane**

Find `navigateTenantView` and add plane sync:

```typescript
export function navigateTenantView(nextView: TenantWorkspaceView): void {
  setView(nextView);
  setPlaneSignal(planeForView(nextView));
  persistWorkspaceState();
  const tenant = selectedTenant();
  if (tenant) {
    void hydrateTenantView(nextView, tenant);
  }
}
```

- [ ] **Step 5: Map 'users' to 'members' for backward compat**

The old view value `'users'` is now `'members'`. In `restoreWorkspaceState`, add a migration:

Find the workspace restoration code and add after loading the persisted state:
```typescript
// Migrate old 'users' view to 'members'
if (persisted.view === 'users') persisted.view = 'members';
```

Also update `hydrateTenantView` to handle 'members' the same as 'users' (load users data).

- [ ] **Step 6: Update PersistedConsoleWorkspace to include plane**

```typescript
interface PersistedConsoleWorkspace {
  tenantId: string | null;
  view: ConsoleView;
  plane: Plane;
  integrateSection: IntegrateSection;
}
```

Update `persistWorkspaceState` and `restoreWorkspaceState` to include `plane`.

- [ ] **Step 7: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 8: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat: add plane/tab navigation model to store"
```

---

### Task 3: Build the PlaneNav component

**Files:**
- Create: `admin/src/platform/console/views/PlaneNav.ts`

This is the core navigation component — header bar + tier 1 (plane switcher) + tier 2 (page tabs) + collapsed mini-bar. It replaces the Sidebar entirely.

- [ ] **Step 1: Create the PlaneNav component**

Create `admin/src/platform/console/views/PlaneNav.ts` with the full component. This is a large component — read the blueprint sections 3.1-3.4 for exact specifications.

The component needs:
- A `collapsed` signal (local state)
- The header bar (logo, status dot, uptime ticker, version badge, avatar)
- Tier 1: Three plane buttons with number badges (parallelogram clip-path), labels, sector tags. Active plane gets accent color + bottom border + glow.
- Tier 2: Per-plane tab buttons with shortcut hints. Active tab gets accent bottom border.
- Collapsed mini-bar: Shows current plane/tab readout + three plane pips + expand toggle.
- Keyboard handler: `1`/`2`/`3` for planes, `Alt+1-4` for tabs, backtick for collapse toggle.

The component should accept these props:
```typescript
interface PlaneNavProps {
  plane: () => Plane;
  view: () => ConsoleView;
  onPlaneSwitch: (plane: Plane) => void;
  onTabSwitch: (view: ConsoleView) => void;
  onCollapse?: () => void;
  user: () => PlatformUser | null;
  tenant: () => Tenant | null;
}
```

It reads `PLANES` from the store to render buttons. It applies `data-plane="operator|organization|enduser"` on the root element for CSS theming.

The plane switch should trigger a brief flash overlay (a div with the plane's accent color at 7% opacity, fading in over 100ms then out).

- [ ] **Step 2: Add the keyboard shortcut handler**

Inside PlaneNav, attach a global `keydown` listener:

```typescript
function handleKeyDown(e: KeyboardEvent) {
  const tag = (e.target as HTMLElement).tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

  // Plane switching: 1, 2, 3 (no modifiers)
  if (!e.altKey && !e.ctrlKey && !e.metaKey) {
    if (e.key === '1') { props.onPlaneSwitch('operator'); e.preventDefault(); return; }
    if (e.key === '2') { props.onPlaneSwitch('organization'); e.preventDefault(); return; }
    if (e.key === '3') { props.onPlaneSwitch('enduser'); e.preventDefault(); return; }
    if (e.key === '`' || e.key === 'Escape') { setCollapsed(!collapsed()); e.preventDefault(); return; }
  }

  // Tab switching: Alt+1 through Alt+4
  if (e.altKey && !e.ctrlKey && !e.metaKey) {
    const num = parseInt(e.key);
    if (num >= 1 && num <= 4) {
      const planeDef = PLANES.find((p) => p.key === props.plane());
      if (planeDef && planeDef.tabs[num - 1]) {
        props.onTabSwitch(planeDef.tabs[num - 1].id as ConsoleView);
      }
      e.preventDefault();
    }
  }
}

document.addEventListener('keydown', handleKeyDown);
```

- [ ] **Step 3: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console/views/PlaneNav.ts
git commit -m "feat: create PlaneNav component with header, tier 1/2, collapsed state, keyboard shortcuts"
```

---

### Task 4: Add CSS for the plane navigation system

**Files:**
- Modify: `admin/src/core/theme/components.css`

Add styles for the PlaneNav: header bar, tier 1, tier 2, collapsed mini-bar, plane-specific theming, clip-path badges, transitions. This replaces the sidebar as the primary navigation styling.

- [ ] **Step 1: Add the plane-nav styles**

Add a new section after the existing sidebar styles (or replace them — keep the old sidebar CSS commented out for reference during transition). The key CSS classes needed:

- `.plane-nav` — sticky top container
- `.plane-header` — 56px fixed header
- `.plane-tier1` — plane switcher container
- `.plane-btn` — individual plane button
- `.plane-btn.active` — active plane with accent color
- `.plane-num` — parallelogram number badge (clip-path)
- `.plane-label` — plane name + sector tag stack
- `.plane-tier2` — page tab container
- `.plane-tab` — individual tab button
- `.plane-tab.active` — active tab
- `.plane-collapsed` — 42px mini-bar
- `.plane-pip` — small plane pip in collapsed bar
- `.clip-badge` — shared parallelogram shape component
- `.flash-overlay` — plane switch flash effect
- `.grid-overlay` — subtle background grid

Follow the blueprint's exact values for heights, colors, clip-paths, transitions.

- [ ] **Step 2: Update shell layout**

Change `.shell` from sidebar+stage grid to single-column:

```css
.shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  min-height: 100vh;
  width: 100%;
  overflow: hidden;
  position: relative;
}

.shell-stage {
  min-width: 0;
  min-height: 0;
  flex: 1 1 auto;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
```

- [ ] **Step 3: Add plane-specific theming via data attribute**

```css
.shell[data-plane='operator'] {
  --plane: var(--plane1);
  --plane-glow: var(--plane1-glow);
  --plane-muted: var(--plane1-muted);
  --plane-border: var(--plane1-border);
}
.shell[data-plane='organization'] {
  --plane: var(--plane2);
  --plane-glow: var(--plane2-glow);
  --plane-muted: var(--plane2-muted);
  --plane-border: var(--plane2-border);
}
.shell[data-plane='enduser'] {
  --plane: var(--plane3);
  --plane-glow: var(--plane3-glow);
  --plane-muted: var(--plane3-muted);
  --plane-border: var(--plane3-border);
}
```

- [ ] **Step 4: Add responsive breakpoint**

```css
@media (max-width: 768px) {
  .plane-btn .plane-label { display: none; }
  .plane-tier2 { overflow-x: auto; }
}
```

- [ ] **Step 5: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 6: Commit**

```bash
git add admin/src/core/theme/components.css
git commit -m "style: add plane navigation CSS with three-plane theming and responsive layout"
```

---

### Task 5: Rewire ConsoleApp to use PlaneNav

**Files:**
- Modify: `admin/src/platform/console/ConsoleApp.ts`

This is the integration task: replace the Sidebar with PlaneNav, update the createSwitch to handle all 12 view values, set the `data-plane` attribute on the shell element for CSS theming.

- [ ] **Step 1: Update imports**

Remove the `Sidebar` import. Add `PlaneNav` import:
```typescript
import { PlaneNav } from './views/PlaneNav';
```

Add new store imports:
```typescript
import { plane, setPlane, planeForView, PLANES, type Plane } from '../store';
```

- [ ] **Step 2: Replace Sidebar with PlaneNav in the shell**

Replace the entire `return h('div', { class: 'shell' }, Sidebar(...), ...)` block. The new structure:

```typescript
return h('div', {
  class: 'shell',
  'data-plane': () => plane(),
},
  PlaneNav({
    plane: () => plane(),
    view: () => view(),
    onPlaneSwitch: (p: Plane) => setPlane(p),
    onTabSwitch: (v: ConsoleView) => {
      if (v === 'tenants') { backToTenants(); return; }
      navigateTenantView(v as TenantWorkspaceView);
    },
    user: () => currentUser(),
    tenant: () => selectedTenant(),
  }),
  h('div', { class: 'shell-stage' },
    h('main', { class: 'main page-enter' },
      LoadingBar(() => loading()),
      createSwitch(
        () => view(),
        [
          { match: 'tenants', render: () => TenantsList() },
          { match: 'overview', render: TenantWorkspaceGuard(() => OverviewView()) },
          { match: 'members', render: TenantWorkspaceGuard(() => UsersView()) },
          { match: 'integrate', render: TenantWorkspaceGuard(() => IntegrateView()) },
          { match: 'hosted-page', render: TenantWorkspaceGuard(() => HostedPageView()) },
          { match: 'settings', render: TenantWorkspaceGuard(() => SettingsView()) },
          { match: 'roles', render: TenantWorkspaceGuard(() => ComingSoonView('Roles')) },
          { match: 'audit-log', render: TenantWorkspaceGuard(() => ComingSoonView('Audit Log')) },
          { match: 'sessions', render: TenantWorkspaceGuard(() => ComingSoonView('Sessions')) },
          { match: 'passkeys', render: TenantWorkspaceGuard(() => ComingSoonView('Passkeys')) },
          { match: 'tokens', render: TenantWorkspaceGuard(() => ComingSoonView('Tokens')) },
          { match: 'activity', render: TenantWorkspaceGuard(() => ComingSoonView('Activity')) },
        ],
      ),
    ),
  ),
  CommandPalette({ ... }),
  ToastView(toast),
);
```

Note: `ComingSoonView` is a tiny function for tabs that will get real views in Sub-plan B:
```typescript
function ComingSoonView(label: string) {
  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('div', { class: 'view-eyebrow' }, 'Coming Soon'),
      h('h2', { class: 'view-title' }, label),
      h('p', { class: 'view-subtitle text-muted' }, 'This view is being built. API endpoints are ready — dashboard view coming next.'),
    ),
  );
}
```

- [ ] **Step 3: Remove old Topbar from shell**

The PlaneNav replaces both the Sidebar AND the Topbar. Remove the Topbar rendering from the shell. The session status chip and tenant info can move into the PlaneNav header bar.

- [ ] **Step 4: Update the command palette sections**

Update the palette entries to reference the new view IDs and labels. Replace 'users' with 'members'. Add entries for new tabs (roles, audit-log, sessions, passkeys, tokens, activity).

- [ ] **Step 5: Remove old sidebarGroups, sidebarContext, sidebarAccount functions**

These are no longer needed. The PlaneNav handles all navigation state internally.

- [ ] **Step 6: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 7: Commit**

```bash
git add admin/src/platform/console/ConsoleApp.ts
git commit -m "feat: replace sidebar with PlaneNav — three-plane horizontal tab navigation"
```

---

### Task 6: Update page-level CSS for full-width content

**Files:**
- Modify: `admin/src/core/theme/pages.css`

With the sidebar gone, content area gets the full viewport width. Update max-widths and padding.

- [ ] **Step 1: Update console-view max-width**

Find `.console-view` (around line 1259) and update:
```css
.console-view {
  max-width: 1240px;
  margin: 0 auto;
  padding: var(--space-xl) var(--space-xl) var(--space-xl);
  width: 100%;
}
```

- [ ] **Step 2: Update the main content scroll area**

Update `.main` to account for the new nav height:
```css
.main {
  flex: 1 1 auto;
  overflow-y: auto;
  padding-bottom: var(--space-xl);
}
```

- [ ] **Step 3: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 4: Commit**

```bash
git add admin/src/core/theme/pages.css
git commit -m "style: update content area for full-width layout (sidebar removed)"
```

---

### Task 7: Update e2e tests for new navigation

**Files:**
- Modify: `admin/e2e/session-inspector.spec.ts`

The existing e2e tests reference sidebar navigation (`.sidebar`, clicking sidebar items). Update them to use the new plane tab navigation.

- [ ] **Step 1: Update navigation selectors**

Replace references to `.sidebar` with plane-nav selectors. For example, clicking "Integrate" now means clicking the Operator plane, then the Integrate tab. Update test assertions for the new tab labels ("Auth Reference" etc.).

The exact selectors will depend on Task 3's implementation (PlaneNav), so read the rendered HTML to find the correct selectors (role=tab, text content, etc.).

- [ ] **Step 2: Build and verify**

Run: `cd admin && npm run build 2>&1 | tail -5`

- [ ] **Step 3: Commit**

```bash
git add admin/e2e/session-inspector.spec.ts
git commit -m "test: update e2e tests for plane-based navigation"
```

---

### Task 8: Download and serve font files

**Files:**
- Add: Font WOFF2 files to the static assets directory
- Modify: `crates/server/src/handlers/mod.rs` or asset serving config (if font files need explicit registration)

- [ ] **Step 1: Find where font files are stored**

Run: `find . -name "dm-mono-400.woff2"` to locate the existing font directory.

- [ ] **Step 2: Download Rajdhani and Orbitron WOFF2 files**

Download from Google Fonts API or use `google-webfonts-helper`:
- Rajdhani: weights 400, 500, 600, 700
- Orbitron: weights 500, 700

Place in the same directory as existing fonts.

- [ ] **Step 3: Verify fonts load**

Start the dev server and check that the fonts load in browser dev tools (Network tab, filter by woff2).

- [ ] **Step 4: Commit**

```bash
git add <font-directory>
git commit -m "assets: add Rajdhani and Orbitron WOFF2 font files"
```
