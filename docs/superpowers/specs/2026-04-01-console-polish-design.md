# Console Polish & Overview Redesign — Design Spec

## Overview

The GateWASM platform console is a tenant management tool. After the onboarding wizard, the console's Overview tab is hollow — just three stat cards and a latest-user line. Meanwhile, the standalone sandbox page at `/t/{slug}/sandbox` contains the most useful developer reference data (JWT viewer, callback preview, session info, user profile) behind an extra click.

This redesign makes the console the daily driver by surfacing sandbox-style integration reference data directly in the Overview tab, organized into two logical zones. It also applies a CSS polish pass across all console tabs for consistent layout, typography, and spacing.

**No new backend endpoints.** No palette changes. No changes to login pages, onboarding, wizard, or sidebar.

---

## Product Context

- GateWASM is a standalone auth platform. Its visual identity (dark bg + warm gold accent) is intentionally distinct from the Forma brand. No palette changes in this work.
- The console is multi-tenant. A platform user may own multiple tenants. The Overview is always scoped to a specific selected tenant.
- All console views are the same regardless of pricing tier. No tier-gating.
- **Plane 2** = Platform. Console auth, tenant management. Platform JWT.
- **Plane 3** = Tenant. End-user auth, login pages, sandbox, hosted page. Tenant-scoped JWT + refresh cookie.
- The Overview's Integration Reference zone requires Plane 3 session data. The Quick Check zone uses only Plane 2 data and must always render instantly.

---

## Architecture

### Shared Component Extraction

The sandbox page (`SandboxPage.ts`) currently inlines four components and several helpers. Extract them into store-agnostic shared components that take props (introspect data), not store references.

| New file | Contents |
|----------|----------|
| `admin/src/core/components/JwtViewer.ts` | `JwtViewer` component + `decodeJwt`, `b64decode` helpers |
| `admin/src/core/components/CallbackPreview.ts` | `CallbackPreview` component |
| `admin/src/core/components/AuthDataCards.ts` | `UserProfile`, `SessionInfo` + `fmtDate`, `relativeExpiry` helpers |

The `IntrospectData` type moves to `admin/src/core/types.ts`.

`SandboxPage.ts` is refactored to import from these shared files. It must work identically after extraction — this is the safety net before the Overview changes.

### Overview Tab — Two-Zone Layout

The Overview (`admin/src/platform/console/views/OverviewView.ts`) becomes a single scrollable view with two visually distinct zones.

#### Quick Check Zone (top)

Always renders immediately from Plane 2 stats data. No dependency on Plane 3 session.

- **Stat cards row** — Total Users, Active Sessions, Logins (24h). Existing data, refined card styling.
- **Latest User card** — email, provider, timestamp. Existing data, better styled.
- **Quick Links** — three links with clear labels:
  - "Login Page" → `/t/{slug}/login` (the front door)
  - "Your Gated Site" → `/t/{slug}/site` (what end users see — the hosted page behind auth)
  - "Console Sandbox" → `/t/{slug}/sandbox` (the developer debug surface)

#### Integration Reference Zone (bottom)

Separated from Quick Check by a visual divider with the label "Integration Reference" (11px uppercase, tracking, `--text-muted`).

**When Plane 3 session exists** (introspect call succeeds):
- `CallbackPreview` — JS/TS/curl code snippets showing what the app receives
- `UserProfile` — authed user's decoded profile data
- `SessionInfo` — session ID, token expiry countdown, algorithm, issuer, audience
- `JwtViewer` — decoded access token with header/payload/signature, raw toggle, refresh token info

**When Plane 3 session does NOT exist** (introspect call 401s):
- Auth prompt card with dashed `--border-accent` border to distinguish from data cards
- Copy: "Sign in to your gated app to see live auth data. This shows you exactly what your end users experience and what your app receives."
- Prominent button: "Sign in to [App Name]" → opens `/t/{slug}/login?redirect=/t/{slug}/sandbox` in new tab
- The button opens a new tab. The user authenticates and lands on the sandbox. They can come back to the console tab whenever — the visibility-change listener will pick up the new session.

### Data Flow

- **Quick Check zone**: Uses existing `loadTenantDetail(id)` which fetches tenant + stats via Plane 2 Platform JWT. No changes.
- **Integration Reference zone**: New `loadIntrospect(slug)` function in the console store. Calls `GET /t/{slug}/auth/introspect` — same endpoint the sandbox uses. The Plane 3 refresh cookie is same-origin and sent automatically.
  - On success: stores `IntrospectData` + raw token in console store signals.
  - On 401/error: sets a `introspectUnavailable` flag. The Overview reads this flag to show the auth prompt.
  - A failed introspect **never** blocks the Quick Check zone from rendering.
- **Visibility-change refresh**: When the console tab regains focus (`document.visibilitychange` event), re-fire the introspect call. This handles the flow where the user clicked the auth prompt, authenticated in another tab, and returned. The Integration Reference zone populates automatically without a page reload.

---

## CSS Polish

### Token Additions (`tokens.css`)

No palette changes. Add structural tokens for consistent spacing:

```
--radius-sm: 4px
--radius-md: 8px
--space-xs: 0.25rem
--space-sm: 0.5rem
--space-md: 1rem
--space-lg: 1.5rem
--space-xl: 2rem
```

### Console-Specific Styles

New section in `pages.css` (or extracted to `console.css` if cleaner):

- **`.console-view`** — max-width 960px, centered, consistent vertical rhythm with space tokens
- **`.zone`** — Quick Check and Integration Reference zones. Clean top-border separator between zones with zone label (0.65rem uppercase, +0.08em tracking, `--text-muted`)
- **`.stat-card`** — `--border-accent` border, `--surface-raised` bg, space-lg padding, value at 1.5rem weight 500 (bump to 600 if thin on screen), label at 0.65rem uppercase tracking
- **`.console-card`** — consistent padding, `--surface` bg, `--border` border, proper heading/content spacing
- **`.card-label`** — 0.65rem uppercase, +0.08em tracking, `--text-muted`, margin-bottom
- **`.auth-prompt`** — `--surface-raised` bg, dashed `--border-accent` border, centered text, prominent action button
- **`.code-block`** — replaces `sandbox-code` references in console views. DM Mono, proper padding, `--surface-raised` bg

### Type Scale

- View title: 1.25rem, weight 500
- View subtitle: 0.78rem, `--text-dim`
- Card headings / zone labels: 0.65rem uppercase, +0.08em tracking, `--text-muted`
- Body: 0.82rem, weight 400, 1.6 line-height
- Code: DM Mono, 0.78rem
- Stat values: 1.5rem, weight 500 (try first, bump to 600 if visually thin)
- Labels: 0.6rem uppercase, +0.1em tracking

### Inline Style Cleanup

Where inline `style: { ... }` in `h()` calls map to repeatable patterns (card padding, flex gaps, font sizes), replace with CSS classes. True one-offs (a single textarea's resize property) stay inline.

---

## Tabs Polish Pass

Styling only — no functional changes to Users, Integrate, Hosted Page, or Settings tabs.

### Users Tab
- Consistent `.console-card` on search container
- User rows: space-token padding, `--border` separators, `--surface-raised` hover
- Expanded detail: `.detail-row` classes (already in place from CR)
- Action buttons: consistent `.btn-sm .btn-ghost`
- **Truncation indicator**: when user list length = 100, show "100 most recent users shown" in `.card-label` style at bottom of list. One line, not pagination.

### Integrate Tab
- Code blocks: DM Mono, `.code-block` class, `--surface-raised` bg
- Section cards: `.console-card` treatment
- Step labels: zone-label style (small uppercase)

### Hosted Page Tab
- Editor fields: consistent Field styling
- URL display: monospace, proper card container
- Button row: consistent layout

### Settings Tab
- Branding section: consistent card + field styling
- Tenant Info: `.detail-row` classes (done)
- Danger Zone: red-tinted treatment, proper spacing (delete already wired up from CR)

---

## Scope Boundaries

### In Scope
- Shared component extraction from SandboxPage
- Overview redesign with two-zone layout
- Console store: introspect call + visibility-change refresh
- CSS tokens (spacing only) + console-specific styles
- All five tabs styling pass
- Users tab truncation indicator

### NOT In Scope
- No palette or color token changes
- No changes to login pages, onboarding, wizard, or sidebar
- No changes to sandbox page layout (imports shared components, keeps its own layout)
- No pagination on Users tab
- No new backend endpoints
- No tier-gating on any console view
- No changes to hosted page editor functionality
- No changes to Integrate tab code snippets or API key logic
- No Rust handler changes

---

## Build Order

1. **Extraction** — Extract shared components from SandboxPage into `core/components/`
2. **Sandbox verification** — Confirm SandboxPage works identically after refactor
3. **Overview redesign** — Two-zone layout with introspect integration + auth prompt
4. **CSS tokens** — Add spacing tokens, console-specific styles
5. **Tabs polish pass** — Users, Integrate, Hosted Page, Settings styling

---

## Files Touched

| Action | Files |
|--------|-------|
| Create | `core/components/JwtViewer.ts`, `core/components/CallbackPreview.ts`, `core/components/AuthDataCards.ts` |
| Modify | `core/types.ts`, `core/theme/tokens.css`, `core/theme/pages.css`, `tenant/sandbox/SandboxPage.ts`, `platform/console/store.ts`, `platform/console/views/OverviewView.ts`, `platform/console/views/UsersView.ts`, `platform/console/views/IntegrateView.ts`, `platform/console/views/HostedPageView.ts`, `platform/console/views/SettingsView.ts`, `platform/console/ConsoleApp.ts` |
| No change | Wizard steps, sidebar, login pages, build.ts, Rust handlers |
