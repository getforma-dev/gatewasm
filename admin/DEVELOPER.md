# Forma Admin — Developer Guide

How to iterate on the UI: change colors, add components, add pages, modify layouts.

For project setup and running the server, see the root [README.md](../README.md) and [LOCAL-DEV.md](../LOCAL-DEV.md).

## Directory Map

```
admin/src/
├── core/                    # Shared across all pages
│   ├── components/          #   Badge, Card, DataTable, Field, Sidebar, etc.
│   ├── hooks/               #   createAuth, createApi, createSocket, createToast, createPager
│   ├── theme/               #   tokens.css, base.css, components.css, pages.css
│   │   └── fonts/           #   .woff2 font files
│   ├── types.ts             #   Shared TypeScript types
│   └── utils.ts             #   money(), fmtDate(), copyText(), toCsv()
│
├── platform/                # Platform-level pages
│   ├── login/               #   /platform/login (SSR + islands)
│   ├── onboarding/          #   /platform/onboarding (SSR + islands)
│   ├── console/             #   /platform/console (CSR, auth-gated)
│   └── benchmark/           #   /platform/benchmark (SSR + islands)
│
├── tenant/                  # Tenant-scoped pages
│   ├── login/               #   /t/:slug/login (CSR)
│   ├── dashboard/           #   /t/:slug/dashboard (CSR, auth-gated)
│   └── admin/               #   /t/:slug/admin (CSR, auth-gated)
│
└── admin/                   # Owner admin
    └── dashboard/           #   /platform/admin/dashboard (CSR, auth-gated)
```

Every page folder has the same structure:
- **`app.ts`** — Entry point. Either `activateIslands()` (SSR) or `mount()` (CSR)
- **`store.ts`** — All reactive state, API calls, business logic
- **`*.ts`** — Components (import state from store, render with `h()`)

---

## Common Tasks

### Change the accent color

Edit `src/core/theme/tokens.css`:
```css
--accent:         #c4a265;    /* main accent (currently warm gold) */
--accent-hover:   #d4b275;    /* hover state */
--accent-dim:     rgba(196, 162, 101, 0.19);
--accent-glow:    rgba(196, 162, 101, 0.09);
--accent-border:  rgba(196, 162, 101, 0.25);
```

Change all five values to your new color. The entire UI updates — buttons, links, badges, glows, borders all use these tokens.

**Per-tenant accent colors** are set in `dashboard.rs` via `personality_css`:
```rust
format!("body{{--accent:{pc};}}", pc = primary_color)
```
This overrides the default at the page level.

### Change background / surface colors

Same file, top of `:root`:
```css
--bg:             #0a0a0c;      /* page background */
--surface:        #111114;      /* cards, panels */
--surface-raised: #16161a;      /* elevated surfaces */
--surface-overlay:#1c1c21;      /* modals, dropdowns */
```

### Change fonts

1. Add `.woff2` files to `src/core/theme/fonts/`
2. Update `@font-face` declarations in `tokens.css`
3. Update `base.css` where `font-family` is set

### Change text / button styles

- **Buttons**: `src/core/theme/components.css` → search for `.btn`
- **Inputs**: `src/core/theme/base.css` → search for `input, select, textarea`
- **Typography**: `src/core/theme/base.css` → search for `body`, `h1`-`h6`

---

### Add a new component

**1. Create the file** in `src/core/components/`:

```typescript
// src/core/components/MyWidget.ts
import { h } from 'formajs';

export function MyWidget(props: { label: string; onClick: () => void }) {
  return h('button', {
    class: 'btn primary',
    onClick: props.onClick,
  }, props.label);
}
```

**2. Export it** from `src/core/components/index.ts`:
```typescript
export { MyWidget } from './MyWidget';
```

**3. Use it** in any page component:
```typescript
import { MyWidget } from '../../core/components/MyWidget';
// or: import { MyWidget } from '@core/components';

MyWidget({ label: 'Click me', onClick: () => console.log('clicked') })
```

**Rules for components:**
- Components are **pure rendering** — no fetch calls, no auth logic
- State comes from props (signals or plain values)
- Use `h()` for all DOM creation (never `document.createElement`)
- Reactive props are functions: `value: () => store.count` not `value: store.count`
- CSS classes go in `components.css` or inline via `style` attribute

### Add a reactive component

```typescript
import { h, createSignal, createShow } from 'formajs';

export function Counter() {
  const [count, setCount] = createSignal(0);

  return h('div', { class: 'counter' },
    h('span', null, () => `Count: ${count()}`),  // reactive text
    h('button', {
      onClick: () => setCount(count() + 1),
    }, 'Increment'),
    createShow(
      () => count() > 10,                         // conditional rendering
      () => h('span', { class: 'badge' }, 'High!'),
    ),
  );
}
```

**Key patterns:**
- `() => expr` makes text/attributes reactive (re-renders when signals change)
- `createShow(when, thenFn, elseFn?)` — conditional rendering
- `createSwitch(signal, [{ match, render }])` — multi-branch (like a switch statement)
- `createList(signal, keyFn, mapFn)` — reactive list rendering

---

### Add a new page

This is a 4-step process: frontend, build config, Rust handler, route registration.

**Step 1: Frontend** — Create the page folder:

```
src/platform/my-page/
├── app.ts           # entry point
├── store.ts         # state + API calls
└── MyPageShell.ts   # main component
```

`app.ts` (CSR page, auth-gated):
```typescript
import { mount } from 'formajs';
import { createAuth } from '../../core/hooks/createAuth';
import { createApi } from '../../core/hooks/createApi';
import { MyPageShell } from './MyPageShell';

const auth = createAuth({ serverUrl: window.location.origin, tenant: 'platform' });

async function boot() {
  try {
    await auth.silentRefresh();
    auth.startAutoRefresh();
  } catch {
    window.location.href = '/platform/login';
    return;
  }
  mount(() => MyPageShell(), '#app');
}
boot();
```

`app.ts` (SSR page, public):
```typescript
import { activateIslands } from 'formajs';
import { MyPageRoot } from './MyPageRoot';
import { MyIsland } from './MyIsland';

void MyPageRoot; // compiler traces this for IR generation
activateIslands({ MyIsland });
```

**Step 2: Build config** — Add entry to `build.ts`:

```typescript
// In the entries array (~line 19):
{ entryPoints: ['src/platform/my-page/app.ts'], outfile: 'dist/platform-my-page.js' },
```

And add the route mapping (~line 176):
```typescript
'/platform/my-page': { js: ['platform-my-page'], css: ['forma-platform'] },
```

**Step 3: Rust handler** — Create `handlers/my_page.rs`:

```rust
use axum::{extract::State, response::{Html, IntoResponse, Response}};
use crate::html_template::{render_page, PageConfig};
use crate::{AppState, RenderMode};

pub async fn my_page(State(state): State<AppState>) -> Response {
    let page = render_page(&PageConfig {
        title: "My Page — Forma Auth",
        route_pattern: "/platform/my-page",
        manifest: &state.asset_manifest,
        config_script: None,
        body_class: None,
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;}"),
        body_prefix: None,
        render_mode: RenderMode::Phase1ClientMount,
        ir_module: None,
        slots: None,
    });
    (
        [(axum::http::header::CACHE_CONTROL, "no-store".to_string()),
         (axum::http::header::HeaderName::from_static("content-security-policy"), page.csp)],
        Html(page.html),
    ).into_response()
}
```

**Step 4: Register the route** — In `handlers/mod.rs`:
```rust
pub mod my_page;

// Add to router:
.route("/platform/my-page", get(my_page::my_page))
```

**Step 5: Build + test:**
```bash
cd admin && npx tsx build.ts --ssr        # rebuild JS + IR
cd .. && cargo build -p gatewasm-server   # rebuild Rust (picks up new dist/)
# restart server, visit http://127.0.0.1:3101/platform/my-page
```

---

## WebSocket — Real-Time Invalidation

The dashboard receives live updates via WebSocket. The WS connection is an **invalidation layer** — it tells the client "something changed, refetch." HTTP remains the sole data transport.

### Architecture

```
Handler mutation (login, invite, etc.)
  → emit_activity() or emit_team_changed()
    → Tokio broadcast channel (EventBus)
      → WS handler filters by tenant_id + subscribed topics
        → Client receives JSON: { type: "event", topic, hint }
          → Store callback fires → refetches via HTTP API
```

### `createSocket()` Hook

**File:** `src/core/hooks/createSocket.ts`

```typescript
import { createSocket } from '../../core/hooks/createSocket';

const socket = createSocket(auth);  // takes Auth dependency

// Connect after auth succeeds (in app.ts or store init)
socket.connect(serverUrl + '/ws/ticket');

// Subscribe to invalidation topics (in store.ts)
socket.subscribe('team:changed', (hint) => {
  loadTeamMembers();
});

socket.subscribe('metrics:updated', () => {
  if (view() === 'overview') loadTenantDetail(tenantId);
});

socket.subscribe('tenant:activity', () => {
  if (view() === 'audit-log') loadAuditLog(tenantId);
});

// Disconnect on signout
socket.disconnect();
```

**API:**

| Method | Description |
|--------|-------------|
| `connect(ticketUrl)` | POST ticketUrl for ticket, open WS connection |
| `disconnect()` | Close WS, stop reconnection |
| `subscribe(topic, callback)` | Register callback for topic. Returns `unsubscribe()` fn. |
| `state()` | Signal: `'connecting'` / `'connected'` / `'disconnected'` / `'reconnecting'` |

**Behavior:**
- Subscribes are stored before connect — safe to call `subscribe()` during module init
- Each callback is debounced at 500ms per-topic (burst of 50 logins = 1 refetch)
- Auto-reconnects with exponential backoff (1s → 2s → 4s → ... → 30s cap, ±25% jitter)
- On `auth_expired` from server: does `silentRefresh()` → new ticket → reconnect
- On `server_shutdown`: enters backoff reconnect (server deploy)

### Available Topics

| Topic | Emitted when | Hint values |
|-------|-------------|-------------|
| `team:changed` | Invite created/accepted/revoked, member updated/removed | `invite_created`, `invite_accepted`, `invite_revoked`, `member_updated`, `member_removed` |
| `tenant:activity` | Login, signup, block/unblock, MFA, passkey | `login`, `signup`, `end_user_blocked`, `end_user_unblocked`, `end_user_deleted`, `mfa_verified`, `passkey_registered` |
| `metrics:updated` | Same events as `tenant:activity` (dual-emitted) | none |

**Scoping:** Each WS connection is scoped by the JWT's `tenant_id`. A platform console connection (tenant_id = `"ten_platform"`) receives `team:changed` and platform-level `tenant:activity`. It does NOT receive `tenant:activity` from child tenants (different tenant_id).

### Lifecycle in `app.ts`

```typescript
// Standard boot sequence for auth-gated CSR pages:
auth.silentRefresh()
  .then(() => {
    auth.startAutoRefresh();
    socket.connect(serverUrl + '/ws/ticket');  // after auth, before mount
    mount(() => App(), '#app');
  });

// On signout:
socket.disconnect();
auth.stopAutoRefresh();
await auth.logout();
```

### Adding a New Topic

1. **Server:** Add topic string to `VALID_TOPICS` in `crates/server/src/ws/handler.rs`
2. **Server:** Add emit call in the handler that mutates data:
   ```rust
   crate::ws::events::emit_activity(&state.event_bus, &tenant_id, "your_hint");
   // or for non-activity topics:
   state.event_bus.send(WsEvent {
       tenant_id: tenant_id.to_string(),
       topic: "your_topic".into(),
       hint: Some("your_hint".into()),
       timestamp: chrono::Utc::now().timestamp_millis(),
   });
   ```
3. **Client:** Subscribe in the store:
   ```typescript
   socket.subscribe('your_topic', (hint) => {
     // refetch relevant data
   });
   ```

### Debugging

- Open browser devtools → Network → WS tab to see messages
- Messages are flat JSON, human-readable: `{ type: "event", topic: "team:changed", hint: "invite_accepted" }`
- `GET /health` returns `ws_connections` count
- Server logs connection open/close with `tenant_id` and `account_id`

---

## Build Pipeline

```
npm run dev          # watch mode (JS + CSS, no SSR, no hashing)
npx tsx build.ts     # production build (minified, hashed, compressed)
npx tsx build.ts --ssr  # + SSR IR files for server-side rendering
```

**What each step does:**

| Step | Command | When to run |
|------|---------|-------------|
| CSS only | `npm run build:css` | Changed theme CSS |
| JS only | `npm run build:js` | Changed component code |
| Full build | `npx tsx build.ts` | Before committing |
| Full + SSR | `npx tsx build.ts --ssr` | Changed SSR page structure |
| FormaJS rebuild | `cd npm-modules/formajs && npm run build` | Changed FormaJS source |
| Rust rebuild | `cargo build -p gatewasm-server` | Changed Rust code OR dist/ |
| Server restart | Kill + re-run cargo | After Rust rebuild or dist hash change |

**The build outputs:**
- `dist/*.js` — Page bundles (content-hashed)
- `dist/*.css` — Stylesheets (content-hashed)
- `dist/*.ir` — SSR bytecode (content-hashed)
- `dist/*.br` / `*.gz` — Compressed variants
- `dist/manifest.json` — Route-to-asset mapping (Rust reads this)
- `dist/sw.js` — Service worker

## Rules That Will Break Things

1. **FormaJS alias** — esbuild resolves `'formajs'` to `npm-modules/formajs/dist/index.js`. If you change FormaJS source, you MUST run `npm run build` in the formajs directory first, then rebuild admin.

2. **rust-embed watches dist/** — The Rust server embeds `dist/` at compile time. Changing dist files requires `cargo build` to pick them up. New files or renamed files require Rust rebuild.

3. **Manifest route patterns must match** — The route string in `build.ts` (`'/platform/my-page'`) must exactly match the route pattern used in `render_page()` in Rust. Mismatch = page loads with no JS/CSS.

4. **Config JSON must be snake_case** — `window.__TENANT__` and `window.__PLATFORM_CONFIG__` use snake_case keys. The frontend reads `config.server_url`, not `config.serverUrl`.

5. **SSR pages need `--ssr` flag** — Running `npx tsx build.ts` without `--ssr` skips IR generation. SSR pages fall back to empty shell (Phase1ClientMount).

6. **Don't use `document.createElement` in components** — Use `h()` from formajs. Direct DOM manipulation breaks SSR hydration and reactive updates.

7. **Reactive values must be functions** — `h('span', null, count)` is static. `h('span', null, () => count())` is reactive. Missing the `() =>` wrapper is the #1 bug.

## Testing

```bash
npx playwright test                    # all 84 tests
npx playwright test e2e/smoke.spec.ts  # smoke tests only
npx playwright test -g 'tenant login'  # grep for test name
npx playwright test --headed           # see the browser
npx playwright test --debug            # step through
```

**Add a test** — Create or edit a file in `e2e/`:
```typescript
import { test, expect } from '@playwright/test';
import { collectErrors, expectNoConsoleErrors } from './helpers';

test('my page loads', async ({ page }) => {
  const errors = collectErrors(page);
  await page.goto('/platform/my-page');
  await page.waitForTimeout(2000);
  await expectNoConsoleErrors(errors);
  await expect(page.locator('h1')).toBeVisible();
});
```

For auth-gated pages, use `loginAsTestUser(browser)` from `auth-helper.ts`.

## OAuth Setup (Real Credentials)

To get real Google/GitHub OAuth working locally:

**Google:**
1. Go to https://console.cloud.google.com/apis/credentials
2. Create OAuth 2.0 Client ID (Web application)
3. Set redirect URI: `http://127.0.0.1:3101/auth/callback/google`
4. Set env vars: `GOOGLE_CLIENT_ID=xxx GOOGLE_CLIENT_SECRET=xxx`

**GitHub:**
1. Go to https://github.com/settings/developers → New OAuth App
2. Set callback URL: `http://127.0.0.1:3101/auth/callback/github`
3. Set env vars: `GITHUB_CLIENT_ID=xxx GITHUB_CLIENT_SECRET=xxx`

Currently running with `dummy` values — buttons render but OAuth flow will fail at the provider redirect.
