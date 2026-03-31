# GateWASM Platform Console Redesign — Design Spec

## Overview

Rebuild the Platform Console with a guided onboarding wizard, a developer sandbox page, and a redesigned post-wizard console. Ships as one release in three phases. The current console stays live at `/platform/console` until the replacement is ready to swap in.

**Phases:**
1. Onboarding Wizard + Sandbox Page (buildable and testable on its own)
2. Console Redesign — tabs, users view, integrate view, settings (layers on top)
3. Hosted Page System — no-code page editor + auth-gated hosting (completes the picture)

---

## Product Principles

### Product Independence

GateWASM is one of three independent products in the Forma ecosystem (kmd, GateWASM, Forma). Each acquires, retains, and monetizes independently. A GateWASM customer who never touches FormaJS or Forma is a fully served customer. Cross-product connections are suggestions, never gates.

### Plane Separation

- **Plane 2 (Platform):** The console where customers manage tenants, users, billing. Platform JWT auth.
- **Plane 3 (Tenant):** The login page, sandbox, and hosted page where end users authenticate. Tenant-scoped JWT auth.
- These are separate user pools, sessions, and cookies. The sandbox makes this visible with a banner.

---

## Phase 1: Onboarding Wizard + Sandbox Page

### Architecture

The wizard lives inside `console-v2/` as a view mode, not a separate page. When a tenant has `onboarding_completed === false`, the console renders the wizard. On completion, it writes `onboarding_completed = true` and `preferred_tab` to the tenant config, then swaps to the full console view.

The sandbox is a standalone page at `/t/{slug}/sandbox` — a new entry point in `tenant/sandbox/` with its own app.ts, store, and component. Tenant-scoped (Plane 3), auth-gated, and admin-restricted.

Both live in `console-v2/` (wizard) and `tenant/sandbox/` (sandbox) — parallel to existing code, zero risk to the live product.

### Onboarding Wizard

Three steps inside the console app shell. No skip on any step. Wizard state persists server-side in the tenant config (`onboarding_step: 1|2|3`) so it resumes correctly if the customer closes the browser.

#### Step 1: "Make it yours"

- Two fields: **App Name** (pre-filled from tenant name) and **Primary Color** (color picker, defaults to `#6366f1`)
- Below the fields: a **live preview** — a mini mockup (~200px wide) of the login page header showing the app name in the chosen color. Connects fields to outcome before they click Next.
- "Next" button advances to Step 2.
- On Next: `PATCH /platform/self-serve/tenants/{id}` with `{ name, primary_color }` and `{ onboarding_step: 2 }`.

#### Step 2: "See it work"

**Waiting state:**
- Prominent "Open login page" button (primary action, larger than Copy). Copy button is secondary.
- On first mount: **auto-open** `/t/{slug}/login` in a new tab via `window.open()`. If popup blocked, fall back to the manual button.
- Pulsing dot animation + "Waiting for your first login..."
- Subtitle: "Register on your login page. This console will advance automatically."
- Login URL displayed as clickable link with copy button.

**Polling:**
- Poll `GET /platform/self-serve/tenants/{id}/stats` with backoff: every **2 seconds** for the first 30 seconds, then every **5 seconds** after that.
- The stats response must include `latest_user: { email, provider, created_at }` (not just a count) so the success state can display user details without a second API call.
- When `total_users` goes from 0 to ≥1, advance to success state.

**Success state:**
- "It works ✓" heading
- Green card showing: first user email, timestamp, provider
- "See what your app receives →" link opens `/t/{slug}/sandbox` in a new tab. The wizard does NOT wait for this — it's optional. The Next button is immediately available.
- "Next" button advances to Step 3.
- On Next: `PATCH /platform/self-serve/tenants/{id}` with `{ onboarding_step: 3 }`.

#### Step 3: "What do you want to do next?"

- Two equal-weight cards side by side:
  - **"Host it here"** — "Don't have an app yet? We'll host an auth-gated page for you. Free forever."
  - **"Add it to your app"** — "Already have an app? Integrate auth in three steps with bridge.js."
- Below cards: "You can switch anytime from your console." (one-liner, small text)
- "Go to Console" button starts **disabled/grayed out**. Becomes active when a card is selected. Selected card shows visible highlight (border color change).
- This is a **first-run preference only** — determines which tab is active on first console load. Both tabs always exist. Not an identity assignment.
- On "Go to Console": `PATCH /platform/self-serve/tenants/{id}` with `{ onboarding_completed: true, preferred_tab: "hosted-page" | "integrate" }`.

### Sandbox Page

A developer debug tool at `/t/{slug}/sandbox`. Shows what auth actually returns. Always available after onboarding, not just during the wizard.

#### Access Control (Cross-Plane Check — Novel Pattern)

This is the first route in the codebase that checks both Plane 3 and Plane 2 auth:

1. Check Plane 3 session cookie — is the visitor authenticated as a tenant end-user?
2. If not authenticated: redirect to `/t/{slug}/login?redirect=/t/{slug}/sandbox`
3. If authenticated: query Plane 2 membership — is this user also a platform member of this tenant?
4. If not a platform member: redirect to `/t/{slug}/site` (they're a regular end-user, not an admin)
5. If both checks pass: render the sandbox page

This requires a new middleware or handler-level check. Existing tenant routes only check Plane 3. Existing platform routes only check Plane 2. The sandbox is architecturally novel — call it out in implementation.

#### Layout

Two-column, no sidebar, no navigation. Focused single-page debug tool.

**Top banner:** "You're viewing this as an end user of [App Name] — this is what your customers' auth data looks like." + "Back to Console" link.

**Left column (lead with value, not metadata):**

1. **"Your app receives this"** — the bridge.onStateChange callback with real data. Copy button. Language toggle: TypeScript / JavaScript / curl. This is the most important element on the page.
2. **User Profile** — email, user ID, name, role, created timestamp.
3. **Session** — session ID, created at, expires in (live countdown or relative time, e.g., "Expires in 14 minutes"), provider.

**Right column:**

1. **Access Token (JWT)** — decoded with color-coded sections:
   - Header (amber) — alg, typ, kid
   - Payload (indigo) — sub, email, name, tenant_id, tenant_slug, role, permissions, iss, aud, iat, exp
   - Signature (green) — "Ed25519 — verified ✓"
   - "Show raw" toggle reveals the base64-encoded token for jwt.io / API testing
   - "Copy" button for the raw token
2. **Refresh Token** (collapsed by default) — "Managed by bridge.js — your app never touches this directly." Educates the customer that token refresh is handled for them.

**Footer:** "Sandbox — developer debug view. Your end users never see this page." + "Log out and try again" button.

#### API

New endpoint: `GET /t/{slug}/auth/introspect`
- Requires valid Plane 3 session
- Returns: decoded access token (header, payload), session data (id, created_at, expires_at, provider), user profile (id, email, name, role)
- The sandbox page renders exactly what this returns — no client-side JWT decoding needed

### File Structure

```
admin/src/
├── platform/
│   ├── console/                 ← EXISTING (untouched until swap)
│   │
│   ├── console-v2/              ← NEW
│   │   ├── app.ts               entry point (init → mount)
│   │   ├── store.ts             clean store
│   │   ├── ConsoleApp.ts        app shell (sidebar + view router)
│   │   ├── wizard/
│   │   │   ├── WizardFlow.ts    orchestrator (step state machine)
│   │   │   ├── BrandingStep.ts  step 1: name + color + live preview
│   │   │   ├── TestAuthStep.ts  step 2: waiting, polling, success
│   │   │   └── ChoosePathStep.ts step 3: two cards, tab preference
│   │   └── views/               (Phase 2 — stubbed initially)
│   │       ├── OverviewView.ts
│   │       ├── UsersView.ts
│   │       ├── HostedPageView.ts (Phase 3)
│   │       ├── IntegrateView.ts
│   │       └── SettingsView.ts
│   │
│   ├── login/                   ← EXISTING (no changes)
│   └── onboarding/              ← EXISTING (no changes)
│
├── tenant/
│   ├── sandbox/                 ← NEW
│   │   ├── app.ts               entry point
│   │   ├── store.ts             auth introspection + token decode
│   │   └── SandboxPage.ts       two-column layout
│   │
│   ├── login/                   ← EXISTING (no changes)
│   ├── dashboard/               ← EXISTING (no changes)
│   └── admin/                   ← EXISTING (no changes)
│
└── core/
    ├── components/              ← SHARED (add: JwtViewer, CodeBlockTabs)
    ├── hooks/                   ← SHARED (add: createPoller)
    └── theme/                   ← SHARED (no changes)
```

### Build Config

During development, both consoles exist as separate entry points:

```typescript
// build.ts — development
{ entry: 'src/platform/console/app.ts',    outfile: 'platform-console.js' },     // live
{ entry: 'src/platform/console-v2/app.ts', outfile: 'platform-console-v2.js' },  // dev
{ entry: 'src/tenant/sandbox/app.ts',      outfile: 'tenant-sandbox.js' },       // new
```

After swap (all phases complete and reviewed):

```typescript
// build.ts — production swap
{ entry: 'src/platform/console-v2/app.ts', outfile: 'platform-console.js' },
// delete console/ folder
```

### Redirect Query Param (Phase 1 Prerequisite)

The tenant login page (`/t/{slug}/login`) does NOT currently respect a `redirect` query parameter. After auth it always navigates to `config.login_redirect_url`. Both the sandbox and the hosted page (Phase 3) need redirect-after-login:

- `/t/{slug}/login?redirect=/t/{slug}/sandbox` → after auth, go to sandbox
- `/t/{slug}/login?redirect=/t/{slug}/site` → after auth, go to hosted page

**Implementation:** In `tenant/login/store.ts`, after successful auth (line 148 and the silent refresh path), check `urlParams.get('redirect')`. If present and it starts with `/t/{slug}/` (same-tenant path — no open redirects), navigate there instead of `loginRedirectUrl`. This is a small change to the existing login store, not a new page.

### Server-Side Changes (Rust)

**New routes:**
- `GET /platform/console-v2` — serves console-v2 page (dev only, same handler pattern as existing console)
- `GET /t/{slug}/sandbox` — sandbox page (tenant-scoped, cross-plane auth gate)

**New API endpoint:**
- `GET /t/{slug}/auth/introspect` — returns decoded JWT + session + user profile for current Plane 3 session

**DB changes:**
- `tenants` table: add `onboarding_completed` (boolean, default false), `onboarding_step` (smallint, default 1), `preferred_tab` (text, nullable)

**Enriched existing endpoint:**
- `GET /platform/self-serve/tenants/{id}/stats` — add `latest_user: { email, provider, created_at }` to the response when `total_users > 0`

**Cross-plane middleware:**
- New middleware or handler-level check for sandbox: verify Plane 3 session cookie AND Plane 2 tenant membership. First route in the codebase with dual-plane auth. Needs explicit implementation attention.

### Data Flow: Wizard Polling

```
Console (Plane 2)                     Login Page (Plane 3)
─────────────────                     ──────────────────

Step 2 mounts
  │
  ├─ window.open(/t/{slug}/login)     Tab opens
  │                                     │
  ├─ Poll: GET .../stats              Customer registers
  │   2s intervals (first 30s)          │
  │   then 5s intervals                 └─ POST /t/{slug}/auth/register
  │                                           │
  ├─ Response: {                              └─ Server creates user
  │   total_users: 1,
  │   latest_user: {
  │     email: "victor@test.com",
  │     provider: "email",
  │     created_at: "..."
  │   }
  │ }
  │
  └─ Wizard advances to success state
     Shows email, timestamp, provider
     Optional: "See what your app receives →" opens sandbox
```

---

## Phase 2: Console Redesign

Replaces the current tenant detail view with a tabbed interface. The sidebar stays. The wizard detection stays. Post-wizard, the console shows tabs.

### Tabs

1. **Overview** — stats (total users, active sessions, logins 24h), recent activity (last 5 signups/logins), quick health check
2. **Users** — search, filter (status, role, provider), paginated user list, click → user detail (email, role, sessions, block/unblock, force password reset, revoke sessions)
3. **Hosted Page** — Phase 3 (stubbed with "coming soon" in Phase 2, or hidden)
4. **Integrate** — bridge.js guide (pre-filled with real slug/URL), backend automation (curl examples), API key management (reveal, copy, rotate)
5. **Settings** — branding (app name, logo, color), auth config (providers, allowed origins, redirect URL), danger zone (delete tenant, bottom, red, confirmation prompt)

### Default Tab

Determined by `preferred_tab` from the wizard:
- "hosted-page" → Hosted Page tab active (Phase 3; falls back to Overview until then)
- "integrate" → Integrate tab active
- Returning users who completed onboarding before → Overview tab

### Store Separation

The Phase 2 store splits the monolith:
- `store.ts` — core state (tenants, selected tenant, view, wizard state, auth)
- View-specific data loaded on demand via `ensureViewData(view)` pattern (same as owner dashboard)
- No separate store files per view — keep it in one file but organized with clear sections

---

## Phase 3: Hosted Page System

### Page Editor

In the Hosted Page tab of the console. Writes to a JSON blob stored in the tenant config.

**Fields:**
- Headline (text)
- Body (text/markdown)
- CTA text (text)
- CTA link (URL)
- Branding inherited from tenant settings (app name, color, logo)

**Code view toggle:** Users can drop into raw mode and use FormaJS directives. Visual editor is the default. Every hosted page is a FormaJS page under the hood, but the user never needs to know this.

Non-intrusive link at the bottom: "Need more pages or a full app? Check out Forma" — suggestion, not a gate.

### Serving

- `GET /t/{slug}/site` — auth-gated. Unauthenticated → redirect to `/t/{slug}/login?redirect=/t/{slug}/site`. Authenticated → render the hosted page.
- Page content stored in `tenant_hosted_pages` table: `tenant_id`, `content_json`, `updated_at`.
- Server-rendered from the JSON blob. Near-zero marginal cost per tenant.
- API: `GET /manage/hosted-page` (read), `PUT /manage/hosted-page` (write) — management API key auth.

### Route Summary (Post-Phase 3)

```
/t/{slug}/login    — where end users authenticate
/t/{slug}/site     — where end users land (hosted page, customer's product)
/t/{slug}/sandbox  — where tenant admins inspect auth data (developer tool)
```

---

## Migration: Existing Tenants

When the swap happens, every existing tenant has `onboarding_completed` defaulting to `false`. Without a migration, existing customers — including your own test tenants — would be thrown into the wizard on their next console visit.

**Required migration (runs at swap time):**
```sql
UPDATE tenants SET onboarding_completed = true, onboarding_step = 3
WHERE created_at < '2026-XX-XX';  -- swap date
```

This ensures only tenants created AFTER the swap get the wizard. Existing tenants go straight to the full console.

---

## Swap Procedure

When all three phases pass review:

1. Update `build.ts`: point `/platform/console` at `console-v2/app.ts`
2. Update Rust router: point `/platform/console` handler at the v2 manifest entry
3. Remove `console-v2` dev route
4. **Verify the live console at `/platform/console` renders console-v2 correctly in production with real tenant data.** Test with existing tenants (should skip wizard) and a fresh tenant (should enter wizard).
5. Keep `admin/src/platform/console/` (old code) for one deploy cycle as a rollback path
6. Deploy
7. After confirming production is stable: delete `admin/src/platform/console/`, rename `console-v2/` to `console/` (optional, cosmetic), deploy cleanup
