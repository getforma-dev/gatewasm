# Console Polish & Overview Redesign

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the console's Overview tab useful by surfacing integration reference data (JWT viewer, callback preview, session info, user profile) inline, and apply a CSS polish pass across all console tabs.

**Architecture:** Extract sandbox components (JwtViewer, CallbackPreview, UserProfile, SessionInfo) from `SandboxPage.ts` into shared `core/components/` files. Refactor SandboxPage to import from shared. Redesign OverviewView as a two-zone scrollable view: Quick Check (Plane 2 stats) + Integration Reference (Plane 3 introspect data with auth-prompt fallback). Add spacing tokens and console-specific CSS classes for consistent layout/typography.

**Tech Stack:** FormaJS (h, createSignal, createShow, batch, mount), TypeScript, CSS custom properties

**Spec:** `docs/superpowers/specs/2026-04-01-console-polish-design.md`

**Note:** The console directory is `admin/src/platform/console-v2/` on disk (old console was deleted, directory was not renamed). All paths reference the real filesystem.

---

## File Map

### New Files

| File | Responsibility |
|------|----------------|
| `admin/src/core/components/JwtViewer.ts` | Decoded JWT viewer with raw toggle + refresh token section |
| `admin/src/core/components/CallbackPreview.ts` | Code snippet tabs (JS/TS/curl) showing what the app receives |
| `admin/src/core/components/AuthDataCards.ts` | UserProfile + SessionInfo components with helpers |

### Modified Files

| File | Change |
|------|--------|
| `admin/src/core/types.ts` | Add `IntrospectData` type |
| `admin/src/tenant/sandbox/SandboxPage.ts` | Import from shared components, remove inlined code |
| `admin/src/platform/console-v2/store.ts` | Add introspect state + `loadIntrospect()` + visibility-change listener |
| `admin/src/platform/console-v2/views/OverviewView.ts` | Two-zone layout with Quick Check + Integration Reference |
| `admin/src/platform/console-v2/views/UsersView.ts` | CSS class updates + truncation indicator |
| `admin/src/platform/console-v2/views/IntegrateView.ts` | CSS class updates |
| `admin/src/platform/console-v2/views/HostedPageView.ts` | CSS class updates |
| `admin/src/platform/console-v2/views/SettingsView.ts` | CSS class updates |
| `admin/src/core/theme/tokens.css` | Add spacing + radius tokens |
| `admin/src/core/theme/pages.css` | Add console-specific styles (zone, stat-card, auth-prompt, code-block) |

---

## Task 1: Add IntrospectData Type to Shared Types

**Files:**
- Modify: `admin/src/core/types.ts`

- [ ] **Step 1: Add IntrospectData interface**

At the bottom of `admin/src/core/types.ts`, before the `CsvColumn` interface, add:

```typescript
// ── Introspect (Plane 3 auth data) ────────────────────────────────────────

export interface IntrospectData {
  user: { id: string; email: string; name: string | null; roles: string[]; created_at: string };
  session: { id: string; created_at: string; expires_at: string } | null;
  tenant: { id: string; slug: string };
  token: { issuer: string; audience: string; issued_at: number; expires_at: number; algorithm: string };
}
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

Expected: builds with no errors.

- [ ] **Step 3: Commit**

```bash
git add admin/src/core/types.ts
git commit -m "feat: add IntrospectData type to shared types"
```

---

## Task 2: Extract JwtViewer into Shared Component

**Files:**
- Create: `admin/src/core/components/JwtViewer.ts`

- [ ] **Step 1: Create JwtViewer.ts**

```typescript
// ---------------------------------------------------------------------------
// JwtViewer — Decoded JWT viewer with raw toggle and refresh token section.
// Store-agnostic: takes token string and slug as props.
// ---------------------------------------------------------------------------

import { h, createSignal, createShow } from 'formajs';

/** Decode a base64url string to a UTF-8 string. */
function b64decode(str: string): string {
  const padded = str.replace(/-/g, '+').replace(/_/g, '/');
  return decodeURIComponent(
    atob(padded)
      .split('')
      .map((c) => '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2))
      .join(''),
  );
}

export function decodeJwt(token: string): { header: string; payload: string; signature: string } | null {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  try {
    return {
      header: JSON.stringify(JSON.parse(b64decode(parts[0])), null, 2),
      payload: JSON.stringify(JSON.parse(b64decode(parts[1])), null, 2),
      signature: parts[2],
    };
  } catch {
    return null;
  }
}

interface JwtViewerProps {
  token: string;
  slug: string;
  serverUrl: string;
}

export function JwtViewer({ token, slug, serverUrl }: JwtViewerProps) {
  const [showRaw, setShowRaw] = createSignal(false);
  const [showRefresh, setShowRefresh] = createSignal(false);
  const decoded = decodeJwt(token);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-toggle-row' },
      h('div', { class: 'sandbox-section-title', style: 'margin-bottom:0' }, 'JWT Access Token'),
      h('button', {
        class: 'sandbox-toggle-btn',
        onClick: () => setShowRaw(!showRaw()),
      }, () => showRaw() ? 'Decoded' : 'Raw'),
    ),
    createShow(
      () => !showRaw() && decoded != null,
      () => h('div', null,
        h('div', { class: 'sandbox-jwt-block' },
          h('div', { class: 'sandbox-jwt-label sandbox-jwt-header' }, 'Header'),
          h('pre', { class: 'sandbox-code sandbox-jwt-header' }, decoded!.header),
        ),
        h('div', { class: 'sandbox-jwt-block' },
          h('div', { class: 'sandbox-jwt-label sandbox-jwt-payload' }, 'Payload'),
          h('pre', { class: 'sandbox-code sandbox-jwt-payload' }, decoded!.payload),
        ),
        h('div', { class: 'sandbox-jwt-block' },
          h('div', { class: 'sandbox-jwt-label sandbox-jwt-signature' }, 'Signature'),
          h('pre', { class: 'sandbox-code sandbox-jwt-signature' }, decoded!.signature),
        ),
      ),
    ),
    createShow(
      () => showRaw(),
      () => h('div', { class: 'sandbox-jwt-raw' }, token),
    ),
    h('div', { class: 'sandbox-refresh-section' },
      h('button', {
        class: 'sandbox-refresh-toggle',
        onClick: () => setShowRefresh(!showRefresh()),
      }, () => (showRefresh() ? '- ' : '+ ') + 'Refresh token'),
      createShow(
        () => showRefresh(),
        () => h('div', { class: 'mt-sm' },
          h('p', { class: 'text-xs', style: 'color: var(--text-muted); margin-bottom: 0.5rem' },
            'Refresh tokens are httpOnly cookies. They are not visible to JavaScript. Your server exchanges them silently via POST /auth/token.',
          ),
          h('pre', { class: 'sandbox-code' },
            'POST ' + serverUrl + '/t/' + slug + '/auth/token\n'
            + 'Content-Type: application/json\n'
            + '{ "grant_type": "refresh_token" }\n'
            + '\n'
            + '// Cookie is sent automatically.\n'
            + '// Response: { "access_token": "eyJ..." }',
          ),
        ),
      ),
    ),
  );
}
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

Expected: builds with no errors.

- [ ] **Step 3: Commit**

```bash
git add admin/src/core/components/JwtViewer.ts
git commit -m "feat: extract JwtViewer into shared component"
```

---

## Task 3: Extract CallbackPreview into Shared Component

**Files:**
- Create: `admin/src/core/components/CallbackPreview.ts`

- [ ] **Step 1: Create CallbackPreview.ts**

```typescript
// ---------------------------------------------------------------------------
// CallbackPreview — Code snippet tabs (JS/TS/curl) showing what the app
// receives when a user authenticates. Store-agnostic: takes props.
// ---------------------------------------------------------------------------

import { h, createSignal } from 'formajs';
import type { IntrospectData } from '../types';

interface CallbackPreviewProps {
  data: IntrospectData;
  slug: string;
  serverUrl: string;
}

export function CallbackPreview({ data: d, slug, serverUrl }: CallbackPreviewProps) {
  const [lang, setLang] = createSignal<'js' | 'ts' | 'curl'>('js');

  const userName = d.user.name || d.user.email;
  const userId = d.user.id;

  const jsCode =
`bridge.onStateChange(({ state, user }) => {
  if (state === 'authenticated') {
    console.log('Hello,', user.name);
    // user.id    = "${userId}"
    // user.email = "${d.user.email}"
    // user.name  = "${userName}"
  }
});`;

  const tsCode =
`interface AuthState {
  state: 'authenticated' | 'unauthenticated';
  user: { id: string; email: string; name: string | null };
}

bridge.onStateChange(({ state, user }: AuthState) => {
  if (state === 'authenticated') {
    console.log('Hello,', user.name);
    // user.id    = "${userId}"
    // user.email = "${d.user.email}"
    // user.name  = "${userName}"
  }
});`;

  const curlCode =
`curl ${serverUrl}/t/${slug}/auth/me \\
  -H "Authorization: Bearer <token>"

# Response:
# {
#   "id": "${userId}",
#   "email": "${d.user.email}",
#   "name": "${userName}"
# }`;

  function codeForLang(): string {
    const l = lang();
    if (l === 'ts') return tsCode;
    if (l === 'curl') return curlCode;
    return jsCode;
  }

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'Your app receives this'),
    h('div', { class: 'sandbox-tabs' },
      h('button', {
        class: () => 'sandbox-tab' + (lang() === 'js' ? ' active' : ''),
        onClick: () => setLang('js'),
      }, 'JavaScript'),
      h('button', {
        class: () => 'sandbox-tab' + (lang() === 'ts' ? ' active' : ''),
        onClick: () => setLang('ts'),
      }, 'TypeScript'),
      h('button', {
        class: () => 'sandbox-tab' + (lang() === 'curl' ? ' active' : ''),
        onClick: () => setLang('curl'),
      }, 'curl'),
    ),
    h('pre', { class: 'sandbox-code' }, () => codeForLang()),
  );
}
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/core/components/CallbackPreview.ts
git commit -m "feat: extract CallbackPreview into shared component"
```

---

## Task 4: Extract AuthDataCards into Shared Component

**Files:**
- Create: `admin/src/core/components/AuthDataCards.ts`

- [ ] **Step 1: Create AuthDataCards.ts**

```typescript
// ---------------------------------------------------------------------------
// AuthDataCards — UserProfile and SessionInfo components.
// Store-agnostic: take IntrospectData as props.
// ---------------------------------------------------------------------------

import { h, createSignal, createShow } from 'formajs';
import type { IntrospectData } from '../types';

export function fmtDate(iso: string): string {
  return new Date(iso).toLocaleString();
}

export function relativeExpiry(unixSeconds: number): string {
  const diff = unixSeconds * 1000 - Date.now();
  if (diff <= 0) return 'Expired';
  const mins = Math.floor(diff / 60000);
  const secs = Math.floor((diff % 60000) / 1000);
  if (mins > 0) return mins + 'm ' + secs + 's';
  return secs + 's';
}

export function UserProfile(d: IntrospectData) {
  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'User Profile'),
    h('div', null,
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'User ID'),
        h('code', { class: 'sandbox-detail-value' }, d.user.id),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Email'),
        h('span', { class: 'sandbox-detail-value' }, d.user.email),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Name'),
        h('span', { class: 'sandbox-detail-value' }, d.user.name || '(not set)'),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Roles'),
        h('span', { class: 'sandbox-roles' },
          ...(d.user.roles.length > 0
            ? d.user.roles.map((r) => h('span', { class: 'sandbox-role-badge' }, r))
            : [h('span', { class: 'sandbox-role-badge' }, 'user')]),
        ),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Created'),
        h('span', { class: 'sandbox-detail-value' }, fmtDate(d.user.created_at)),
      ),
    ),
  );
}

export function SessionInfo(d: IntrospectData) {
  const [expiry, setExpiry] = createSignal(relativeExpiry(d.token.expires_at));

  const timer = setInterval(() => {
    setExpiry(relativeExpiry(d.token.expires_at));
  }, 1000);

  setTimeout(() => {
    const check = setInterval(() => {
      if (!document.querySelector('.sandbox-expiry-live')) {
        clearInterval(timer);
        clearInterval(check);
      }
    }, 5000);
  }, 0);

  return h('div', { class: 'sandbox-section' },
    h('div', { class: 'sandbox-section-title' }, 'Session'),
    h('div', null,
      createShow(
        () => d.session != null,
        () => h('div', null,
          h('div', { class: 'sandbox-detail-row' },
            h('span', { class: 'sandbox-detail-label' }, 'Session ID'),
            h('code', { class: 'sandbox-detail-value' }, d.session!.id),
          ),
          h('div', { class: 'sandbox-detail-row' },
            h('span', { class: 'sandbox-detail-label' }, 'Created'),
            h('span', { class: 'sandbox-detail-value' }, fmtDate(d.session!.created_at)),
          ),
          h('div', { class: 'sandbox-detail-row' },
            h('span', { class: 'sandbox-detail-label' }, 'Expires'),
            h('span', { class: 'sandbox-detail-value' }, fmtDate(d.session!.expires_at)),
          ),
        ),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Token Expires In'),
        h('span', { class: 'sandbox-expiry-live' }, expiry),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Algorithm'),
        h('code', { class: 'sandbox-detail-value' }, d.token.algorithm),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Issuer'),
        h('span', { class: 'sandbox-detail-value' }, d.token.issuer),
      ),
      h('div', { class: 'sandbox-detail-row' },
        h('span', { class: 'sandbox-detail-label' }, 'Audience'),
        h('span', { class: 'sandbox-detail-value' }, d.token.audience),
      ),
    ),
  );
}
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/core/components/AuthDataCards.ts
git commit -m "feat: extract UserProfile and SessionInfo into shared components"
```

---

## Task 5: Refactor SandboxPage to Use Shared Components

**Files:**
- Modify: `admin/src/tenant/sandbox/SandboxPage.ts`
- Modify: `admin/src/tenant/sandbox/store.ts`

- [ ] **Step 1: Update sandbox store to import IntrospectData from shared types**

In `admin/src/tenant/sandbox/store.ts`, replace the local `IntrospectData` interface with an import:

Remove lines 10-15 (the local `IntrospectData` interface) and add this import at the top:

```typescript
import type { IntrospectData } from '../../core/types';
```

Keep the `export` of `IntrospectData` by adding a re-export:

```typescript
export type { IntrospectData };
```

The full file should be:

```typescript
import { createSignal } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import type { IntrospectData } from '../../core/types';

export type { IntrospectData };

const config = (window as any).__TENANT__ || {};
const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
const slug: string = config.slug || '';

const auth = createAuth({ serverUrl, tenant: slug });

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
    setData(await res.json());
  } catch (e: any) {
    setError(e.message || 'Failed to load auth data');
  }
}

export function logout(): void {
  auth.logout();
  window.location.href = '/t/' + slug + '/login?redirect=/t/' + slug + '/sandbox';
}
```

- [ ] **Step 2: Rewrite SandboxPage.ts to use shared components**

Replace the entire file with:

```typescript
// ---------------------------------------------------------------------------
// SandboxPage.ts — Sandbox introspection page. Uses shared components.
// ---------------------------------------------------------------------------

import { h, createShow } from 'formajs';
import { data, error, rawToken, tenantName, slug, serverUrl, logout } from './store';
import { CallbackPreview } from '../../core/components/CallbackPreview';
import { UserProfile, SessionInfo } from '../../core/components/AuthDataCards';
import { JwtViewer } from '../../core/components/JwtViewer';

export function SandboxPage() {
  const consoleUrl = '/platform/console';

  const errView = createShow(
    () => error() != null,
    () => h('div', { class: 'sandbox-error' },
      h('p', null, 'Failed to load sandbox data.'),
      h('p', { class: 'text-xs mt-sm', style: 'color: var(--text-muted)' }, error),
      h('button', {
        class: 'btn mt-md',
        onClick: () => { window.location.reload(); },
      }, 'Retry'),
    ),
  );

  const mainView = createShow(
    () => data() != null && error() == null,
    () => {
      const d = data()!;
      const token = rawToken()!;
      return h('div', null,
        h('div', { class: 'sandbox-columns' },
          h('div', { class: 'flex-col gap-lg' },
            CallbackPreview({ data: d, slug, serverUrl }),
            UserProfile(d),
            SessionInfo(d),
          ),
          h('div', { class: 'flex-col gap-lg' },
            JwtViewer({ token, slug, serverUrl }),
          ),
        ),
      );
    },
  );

  return h('div', { class: 'sandbox-page' },
    h('div', { class: 'sandbox-banner' },
      h('span', { class: 'sandbox-banner-text' },
        'Sandbox ',
        h('strong', null, tenantName),
        ' \u2014 this is a developer-only view, separate from your production app.',
      ),
      h('a', { href: consoleUrl }, 'Back to Console \u2192'),
    ),
    errView,
    mainView,
    h('div', { class: 'sandbox-footer' },
      h('span', { class: 'sandbox-footer-text' },
        'Your end users never see this page.',
      ),
      h('button', {
        class: 'btn',
        onClick: logout,
      }, 'Log out and try again'),
    ),
  );
}
```

- [ ] **Step 3: Build to verify**

```bash
cd admin && npx tsx build.ts
```

Expected: builds with no errors. The sandbox page output should be functionally identical.

- [ ] **Step 4: Commit**

```bash
git add admin/src/tenant/sandbox/SandboxPage.ts admin/src/tenant/sandbox/store.ts
git commit -m "refactor: SandboxPage uses shared components from core/"
```

---

## Task 6: Add Introspect State to Console Store

**Files:**
- Modify: `admin/src/platform/console-v2/store.ts`

- [ ] **Step 1: Add introspect signals and loadIntrospect function**

At the top of `store.ts`, add the import:

```typescript
import type { IntrospectData } from '../../core/types';
```

After the existing state signals (after `export const [hostedPage, setHostedPage] = ...` line 66), add:

```typescript
export const [introspectData, setIntrospectData] = createSignal<IntrospectData | null>(null);
export const [introspectToken, setIntrospectToken] = createSignal<string | null>(null);
export const [introspectUnavailable, setIntrospectUnavailable] = createSignal(false);
```

After the `saveHostedPage` function (after `deleteTenant`), add:

```typescript
export async function loadIntrospect(slug: string): Promise<void> {
  try {
    // Plane 3 introspect: uses tenant's refresh cookie (same-origin, sent automatically)
    const res = await fetch('/t/' + encodeURIComponent(slug) + '/auth/token', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'include',
      body: JSON.stringify({ grant_type: 'refresh_token' }),
    });
    if (!res.ok) {
      setIntrospectUnavailable(true);
      return;
    }
    const tokenData = await res.json();
    const accessToken = tokenData.access_token;
    if (!accessToken) {
      setIntrospectUnavailable(true);
      return;
    }

    const introspectRes = await fetch('/t/' + encodeURIComponent(slug) + '/auth/introspect', {
      headers: { 'Authorization': 'Bearer ' + accessToken },
      credentials: 'include',
    });
    if (!introspectRes.ok) {
      setIntrospectUnavailable(true);
      return;
    }

    const data = await introspectRes.json();
    batch(() => {
      setIntrospectData(data);
      setIntrospectToken(accessToken);
      setIntrospectUnavailable(false);
    });
  } catch {
    setIntrospectUnavailable(true);
  }
}

export function startVisibilityRefresh(slug: string): () => void {
  const handler = () => {
    if (document.visibilityState === 'visible') {
      loadIntrospect(slug);
    }
  };
  document.addEventListener('visibilitychange', handler);
  return () => document.removeEventListener('visibilitychange', handler);
}
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console-v2/store.ts
git commit -m "feat: add introspect state and loadIntrospect to console store"
```

---

## Task 7: Redesign OverviewView with Two Zones

**Files:**
- Modify: `admin/src/platform/console-v2/views/OverviewView.ts`

- [ ] **Step 1: Rewrite OverviewView.ts**

Replace the entire file with:

```typescript
import { h, createShow } from 'formajs';
import {
  selectedTenant, selectedTenantStats, loadTenantDetail,
  introspectData, introspectToken, introspectUnavailable,
  loadIntrospect, startVisibilityRefresh,
} from '../store';
import { CallbackPreview } from '../../../core/components/CallbackPreview';
import { UserProfile, SessionInfo } from '../../../core/components/AuthDataCards';
import { JwtViewer } from '../../../core/components/JwtViewer';

export function OverviewView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;

  loadTenantDetail(tenant.id);
  loadIntrospect(tenant.slug);
  const cleanup = startVisibilityRefresh(tenant.slug);

  // ── Quick Check Zone ──────────────────────────────────────────────────

  const quickCheck = h('div', { class: 'zone' },
    h('div', { class: 'zone-label' }, 'Quick Check'),
    h('div', { class: 'stats-grid' },
      StatCard('Total Users', () => String(selectedTenantStats()?.total_users ?? '\u2014')),
      StatCard('Active Sessions', () => String(selectedTenantStats()?.active_sessions ?? '\u2014')),
      StatCard('Logins (24h)', () => String(selectedTenantStats()?.logins_24h ?? '\u2014')),
    ),
    createShow(
      () => selectedTenantStats()?.latest_user != null,
      () => {
        const lu = selectedTenantStats()!.latest_user!;
        return h('div', { class: 'console-card' },
          h('div', { class: 'card-label' }, 'Latest User'),
          h('div', { class: 'card-value' }, lu.email),
          h('div', { class: 'card-meta text-muted' },
            new Date(lu.created_at).toLocaleString() + ' \u2014 ' + lu.provider,
          ),
        );
      },
    ),
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Quick Links'),
      h('div', { class: 'quick-links' },
        h('a', { href: '/t/' + tenant.slug + '/login', target: '_blank', class: 'btn-link' }, 'Login Page'),
        h('a', { href: '/t/' + tenant.slug + '/site', target: '_blank', class: 'btn-link' }, 'Your Gated Site'),
        h('a', { href: '/t/' + tenant.slug + '/sandbox', target: '_blank', class: 'btn-link' }, 'Console Sandbox'),
      ),
    ),
  );

  // ── Integration Reference Zone ────────────────────────────────────────

  const authPrompt = h('div', { class: 'auth-prompt' },
    h('p', { class: 'auth-prompt-text' },
      'Sign in to your gated app to see live auth data. This shows you exactly what your end users experience and what your app receives.',
    ),
    h('a', {
      href: '/t/' + tenant.slug + '/login?redirect=/t/' + tenant.slug + '/sandbox',
      target: '_blank',
      class: 'btn btn-primary',
    }, 'Sign in to ' + (tenant.name || tenant.slug)),
  );

  const integrationRef = h('div', { class: 'zone' },
    h('div', { class: 'zone-label' }, 'Integration Reference'),
    createShow(
      () => introspectData() != null && introspectToken() != null,
      () => {
        const d = introspectData()!;
        const token = introspectToken()!;
        return h('div', { class: 'integration-ref-content' },
          CallbackPreview({ data: d, slug: tenant.slug, serverUrl }),
          UserProfile(d),
          SessionInfo(d),
          JwtViewer({ token, slug: tenant.slug, serverUrl }),
        );
      },
    ),
    createShow(
      () => introspectUnavailable(),
      () => authPrompt,
    ),
  );

  // ── Root ───────────────────────────────────────────────────────────────

  return h('div', { class: 'console-view' },
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, () => selectedTenant()?.name || 'Overview'),
      h('p', { class: 'view-subtitle text-muted' }, () => '/t/' + (selectedTenant()?.slug || '')),
    ),
    quickCheck,
    integrationRef,
  );
}

function StatCard(label: string, value: () => string) {
  return h('div', { class: 'stat-card' },
    h('div', { class: 'stat-value' }, value),
    h('div', { class: 'stat-label' }, label),
  );
}
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console-v2/views/OverviewView.ts
git commit -m "feat: redesign Overview with Quick Check + Integration Reference zones"
```

---

## Task 8: Add CSS Tokens and Console Styles

**Files:**
- Modify: `admin/src/core/theme/tokens.css`
- Modify: `admin/src/core/theme/pages.css`

- [ ] **Step 1: Add spacing and radius tokens to tokens.css**

After the `--glow-intensity` line (line 97 in tokens.css), add:

```css

  /* ── Spacing ───────────────────────────────────────────────────────── */
  --radius-sm:      4px;
  --radius-md:      8px;
  --space-xs:       0.25rem;
  --space-sm:       0.5rem;
  --space-md:       1rem;
  --space-lg:       1.5rem;
  --space-xl:       2rem;
```

- [ ] **Step 2: Add console-specific styles to pages.css**

At the end of `pages.css`, add a new section:

```css

/* ==========================================================================
   Console — Layout, Zones, Cards
   ========================================================================== */

.console-view {
  max-width: 960px;
  margin: 0 auto;
  padding: var(--space-xl) var(--space-lg);
  display: flex;
  flex-direction: column;
  gap: var(--space-lg);
}

.view-header {
  margin-bottom: var(--space-sm);
}

.view-title {
  font-size: 1.25rem;
  font-weight: 500;
  color: var(--text);
  margin: 0;
}

.view-subtitle {
  font-size: 0.78rem;
  color: var(--text-dim);
  margin: var(--space-xs) 0 0;
}

/* ── Zones ──────────────────────────────────────────────────────────── */

.zone {
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
}

.zone + .zone {
  margin-top: var(--space-lg);
  padding-top: var(--space-lg);
  border-top: 1px solid var(--border-accent);
}

.zone-label {
  font-size: 0.65rem;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-muted);
}

/* ── Stat Cards ─────────────────────────────────────────────────────── */

.stats-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--space-md);
}

.stat-card {
  border: 1px solid var(--border-accent);
  background: var(--surface-raised);
  padding: var(--space-lg);
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
  border-radius: var(--radius-sm);
}

.stat-value {
  font-size: 1.5rem;
  font-weight: 500;
  color: var(--text);
}

.stat-label {
  font-size: 0.65rem;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: var(--text-muted);
}

/* ── Console Cards ──────────────────────────────────────────────────── */

.console-card {
  background: var(--surface);
  border: 1px solid var(--border);
  padding: var(--space-lg);
  border-radius: var(--radius-sm);
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
}

.card-label {
  font-size: 0.65rem;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-muted);
  margin-bottom: var(--space-xs);
}

.card-value {
  font-size: 0.82rem;
  color: var(--text);
}

.card-meta {
  font-size: 0.72rem;
}

/* ── Quick Links ────────────────────────────────────────────────────── */

.quick-links {
  display: flex;
  gap: var(--space-lg);
}

/* ── Auth Prompt ────────────────────────────────────────────────────── */

.auth-prompt {
  background: var(--surface-raised);
  border: 1px dashed var(--border-accent);
  border-radius: var(--radius-sm);
  padding: var(--space-xl);
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-lg);
}

.auth-prompt-text {
  font-size: 0.82rem;
  color: var(--text-dim);
  line-height: 1.6;
  max-width: 480px;
}

/* ── Integration Reference Content ──────────────────────────────────── */

.integration-ref-content {
  display: flex;
  flex-direction: column;
  gap: var(--space-lg);
}

/* ── Code Block (generic) ───────────────────────────────────────────── */

.code-block {
  font-family: 'DM Mono', 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 0.78rem;
  background: var(--surface-raised);
  border: 1px solid var(--border);
  padding: var(--space-md);
  border-radius: var(--radius-sm);
  overflow-x: auto;
  white-space: pre;
  color: var(--text-dim);
}

/* ── User List ──────────────────────────────────────────────────────── */

.user-list {
  display: flex;
  flex-direction: column;
}

.user-row {
  border-bottom: 1px solid var(--border);
  transition: background var(--duration) ease;
}

.user-row:last-child {
  border-bottom: none;
}

.user-row-summary {
  padding: var(--space-md) var(--space-lg);
  cursor: pointer;
  transition: background var(--duration) ease;
}

.user-row-summary:hover {
  background: var(--surface-raised);
}

.user-email {
  font-size: 0.82rem;
  color: var(--text);
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.user-meta {
  font-size: 0.72rem;
  margin-top: var(--space-xs);
}

.user-detail {
  padding: var(--space-sm) var(--space-lg) var(--space-lg);
}

.user-actions {
  display: flex;
  gap: var(--space-sm);
  margin-top: var(--space-md);
}

.detail-grid {
  display: flex;
  flex-direction: column;
}

/* ── Truncation Indicator ───────────────────────────────────────────── */

.list-truncated {
  font-size: 0.65rem;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-muted);
  text-align: center;
  padding: var(--space-md);
}

/* ── Search Input ───────────────────────────────────────────────────── */

.search-input {
  width: 100%;
  font-size: 0.82rem;
}
```

- [ ] **Step 3: Build to verify**

```bash
cd admin && npx tsx build.ts
```

- [ ] **Step 4: Commit**

```bash
git add admin/src/core/theme/tokens.css admin/src/core/theme/pages.css
git commit -m "feat: add spacing tokens and console-specific CSS styles"
```

---

## Task 9: Polish Users Tab + Truncation Indicator

**Files:**
- Modify: `admin/src/platform/console-v2/views/UsersView.ts`

- [ ] **Step 1: Add truncation indicator to UsersView**

In `UsersView.ts`, after the user list `h('div', { class: 'user-list' }, ...)` closing paren (before the final `);` of the return), add the truncation indicator:

```typescript
    createShow(
      () => users().length >= 100,
      () => h('div', { class: 'list-truncated' }, '100 most recent users shown'),
    ),
```

The full return should end with:

```typescript
    ),  // closes user-list div
    createShow(
      () => users().length >= 100,
      () => h('div', { class: 'list-truncated' }, '100 most recent users shown'),
    ),
  );  // closes console-view div
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console-v2/views/UsersView.ts
git commit -m "feat: add truncation indicator to Users tab"
```

---

## Task 10: Polish Integrate Tab

**Files:**
- Modify: `admin/src/platform/console-v2/views/IntegrateView.ts`

- [ ] **Step 1: Update IntegrateView step labels to use zone-label style**

In `IntegrateView.ts`, change the step label `card-label` divs to include the step number styling. Replace the three `card-label` lines:

Change `h('div', { class: 'card-label' }, 'Step 1 — Add the script tag')` to:
```typescript
h('div', { class: 'card-label' }, 'Step 1 \u2014 Add the script tag')
```

Change `h('div', { class: 'card-label' }, 'Step 2 — Your login page URL')` to:
```typescript
h('div', { class: 'card-label' }, 'Step 2 \u2014 Your login page URL')
```

Change `h('div', { class: 'card-label' }, 'Step 3 — Management API key (backend only)')` to:
```typescript
h('div', { class: 'card-label' }, 'Step 3 \u2014 Management API key (backend only)')
```

Also replace `h('pre', { class: 'sandbox-code' }, bridgeSnippet)` with:
```typescript
h('pre', { class: 'code-block' }, bridgeSnippet)
```

And replace `h('code', { class: 'sandbox-code', style: { display: 'block' } },` with:
```typescript
h('code', { class: 'code-block', style: { display: 'block' } },
```

And replace `h('code', { class: 'sandbox-code', style: { display: 'block', wordBreak: 'break-all' } },` with:
```typescript
h('code', { class: 'code-block', style: { display: 'block', wordBreak: 'break-all' } },
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console-v2/views/IntegrateView.ts
git commit -m "style: polish Integrate tab with code-block class and consistent labels"
```

---

## Task 11: Polish Hosted Page and Settings Tabs

**Files:**
- Modify: `admin/src/platform/console-v2/views/HostedPageView.ts`
- Modify: `admin/src/platform/console-v2/views/SettingsView.ts`

These tabs are already well-structured from previous work. The polish is minimal — just removing inline styles where CSS classes now exist.

- [ ] **Step 1: Clean up HostedPageView inline styles**

In `HostedPageView.ts`, replace:
```typescript
h('code', { style: { flex: '1', fontSize: '0.82rem', color: 'var(--text)' } }, siteUrl),
```
with:
```typescript
h('code', { class: 'card-value', style: { flex: '1' } }, siteUrl),
```

Replace:
```typescript
h('p', { class: 'text-muted', style: { fontSize: '0.72rem', marginTop: '1rem', textAlign: 'center' } },
```
with:
```typescript
h('p', { class: 'text-muted', style: { fontSize: '0.72rem', marginTop: 'var(--space-md)', textAlign: 'center' } },
```

- [ ] **Step 2: Build to verify**

```bash
cd admin && npx tsx build.ts
```

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console-v2/views/HostedPageView.ts admin/src/platform/console-v2/views/SettingsView.ts
git commit -m "style: polish Hosted Page and Settings tabs"
```
