# Onboarding Wizard — Post-Tenant-Creation Setup Flow

**Date:** 2026-04-08
**Status:** Draft

## Problem

After creating a tenant, operators land on an empty dashboard with a flat checklist ("Recommended Next Steps") at the bottom. This is the weakest moment in the product — four zeroes, an empty chart, and scattered quick links. Clerk, Auth0, and BetterAuth all solve this with guided onboarding before the dashboard. The checklist items are literally wizard steps presented non-sequentially.

## Solution

A full-page onboarding wizard that renders when `onboarding_completed === false` for a selected tenant. Six steps that configure the tenant, demonstrate the auth flow, and build operator confidence before they see the dashboard. Every step uses existing APIs and infrastructure — the wizard is orchestration, not new backend work.

### Design Principles

- **Feels like a conversation, not a form.** Each step is one decision or one configuration surface.
- **Every step is skippable.** "Skip setup →" link in the top-right applies sensible defaults and drops the user into the dashboard. Power users blast through in 20 seconds.
- **No new backend systems.** Branding, providers, hosted page, WS events — all exist. The wizard wraps them.
- **Existing components move, not duplicate.** The branding fields, provider config, and hosted page editor already work. The wizard calls the same `updateTenant` PATCH endpoint.

## Wizard Lifecycle

**Entry gate:** `switchToWorkspaceInternal` checks `tenant.onboarding_completed`. If false, render `OnboardingWizard` instead of the normal workspace views (sidebar, plane tabs, overview). URL stays `/platform/console/{slug}`.

**State:** A `wizardStep` signal (0–5) drives rendering. `onboarding_step` on the tenant record is PATCHed at each transition for resume support.

**Resume:** On wizard mount, the tenant is freshly loaded (via `hydrateTenantView` in `switchToWorkspaceInternal`). The `onboarding_step` value seeds `wizardStep`. Previous step data is validated against the fresh tenant state — no stale assumptions.

**Skip:** Subtle "Skip setup →" link, top-right. Sets `onboarding_completed = true`, `onboarding_step = 6`, applies SaaS defaults if no intent was selected, transitions to normal dashboard.

**Completion:** After Step 5, PATCH `onboarding_completed = true`. `switchToWorkspaceInternal` re-runs and renders the normal dashboard. Toast: "Your workspace is ready." Dashboard opens to the `preferred_tab` view.

**Navigation:** "Back" link on Steps 1–5. "Continue →" button on every step. Internal signal only — no browser history manipulation, no URL changes.

## Step 0: Intent Capture

**Purpose:** Pre-configure sensible defaults so the operator doesn't have to make low-level decisions.

**Layout:** Full-page centered card. fAuth logo top-center. Headline: "What are you building?" Subtitle: "This helps us configure sensible defaults so you don't have to."

**Options:** Five cards in a grid, each with icon, title, and one-line description:

| Intent | Description |
|--------|-------------|
| SaaS Application | Multi-tenant app with user accounts |
| Internal Tool | Company-internal app, employees only |
| Community Site | Open registration, social features |
| Client Portal | Restricted access for your clients |
| Marketplace / API | Developer platform, API keys, OAuth clients |

Single selection. Click to highlight, "Continue →" below.

**Silent configuration (one PATCH on Continue):**

| Field | SaaS | Internal | Community | Client Portal | Marketplace |
|-------|------|----------|-----------|---------------|-------------|
| `signup_mode` | open | invite_only | open | invite_only | open |
| `security_config.idle_timeout_minutes` | 30 | 15 | 60 | 30 | 60 |
| `security_config.max_sessions_per_user` | 5 | 2 | 10 | 3 | 10 |
| `security_config.mfa_policy` | recommended | required | off | recommended | off |
| `allowed_providers` | email, google | email, google | email, google, github | email | email, github |
| `preferred_tab` | integrate | integrate | hosted-page | hosted-page | integrate |

**New DB field:** `tenant_intent` (text, nullable). Stores selection for badge/reference. Not used for logic beyond initial defaults.

**New security_config field:** `mfa_policy` (string: `"required"` | `"recommended"` | `"off"`). Added to the existing `security_config` JSONB column. Not surfaced in the wizard — appears in the Security tab with the intent badge.

**Magic Link representation:** Stored as `"magic_link"` in the `allowed_providers` array, same pattern as `"email"`, `"google"`, `"github"`.

**Skip default:** If the entire wizard is skipped, SaaS Application profile is applied.

## Step 1: Branding

**Purpose:** Customize the login page appearance with real-time visual feedback.

**Layout:** Split pane. Left (~45%): form controls. Right (~55%): live iframe of `/t/{slug}/login?preview=1`.

**Form controls:**

- **App name** — text input, pre-filled with tenant name
- **Logo** — URL input (paste a hosted image URL). Shows thumbnail preview. File upload (drag-and-drop) is a future enhancement — the DB field is `branding_logo_url` (a URL, not a blob).
- **Primary color** — hex color picker, pre-filled with `#6366f1`
- **Dark / Light mode** — toggle switch
- **Button text** — text input, default "Sign in"

**New DB fields:**

- `branding_dark_mode` (boolean, default true)
- `branding_button_text` (text, default 'Sign in')

**Live preview (PostMessage bridge):**

The `?preview=1` flag on the login page enables:
1. A `window.addEventListener('message', ...)` listener with **origin validation** — only accepts messages from the platform's known origin
2. Form submission disabled (preview only)
3. Accepts `{ type: 'forma:branding', primaryColor, logoUrl, appName, darkMode, buttonText }` messages
4. Applies values to CSS vars and DOM text in real-time

Wizard sends PostMessage on every input change (100ms debounce for text, immediate for color/toggle). No DB writes until "Continue →".

**On Continue:** Single PATCH with all branding fields.

**Not in wizard:** Favicon, custom CSS, background images, email templates, custom fonts. These live in a Settings/Branding section on the dashboard, accessible after onboarding.

## Step 2: Auth Methods

**Purpose:** Choose how end users authenticate.

**Layout:** Same split pane as Step 1. Left: provider toggles. Right: same live iframe.

**Provider toggles (left), grouped:**

**Credentials:**
- Email / Password — toggle (default from Step 0)
- Magic Link — toggle (off by default). Subtitle: "Passwordless sign-in via email"

**Social Providers:**
- Google — toggle with status
- GitHub — toggle with status
- Microsoft — toggle (greyed, "Coming soon")
- Apple — toggle (greyed, "Coming soon")

Pre-toggled per Step 0 intent selection.

**OAuth credential states (per social provider):**

1. **Platform credentials available** — platform operator configured global OAuth apps. Toggle works immediately. Subtle text: "Using platform credentials."
2. **No credentials** — toggle is available to express intent. Shows: "Requires OAuth credentials. [Configure in Settings after setup →]." Provider added to `allowed_providers` but won't render on login page until credentials exist.
3. **Tenant has own credentials** — post-onboarding only, via Settings.

**Live preview:** PostMessage sends `{ type: 'forma:providers', providers: ['email', 'google'] }`. Login page shows/hides OAuth buttons. Providers in state 2 (no credentials) appear **dimmed with a lock icon** in the preview, not hidden — so the operator sees the intended layout.

**On Continue:** PATCH `allowed_providers`.

**Not in wizard:** OAuth credential entry, SAML/SSO, custom OIDC providers. Those belong in Settings.

## Step 3: Mode Selection

**Purpose:** Choose between hosted page and SDK integration.

**Layout:** Full-width centered (no split pane). Two large option cards side by side.

**Option A: Hosted Page (default, recommended)**
- Icon: globe/window
- Title: "Launch a hosted page"
- Subtitle: "A working login page and protected site in 30 seconds. No code required."
- Bullets: hosted at `/t/{slug}/site`, end users sign in and see branded page, edit content from dashboard
- Badge: "Recommended"

**Option B: Integrate**
- Icon: code/terminal
- Title: "Integrate into your app"
- Subtitle: "Drop our SDK into your codebase. Full control over the auth flow."
- Bullets: API keys and code snippets, framework-specific guides, login page still works for testing

Single selection (radio-style). Option A pre-selected. "Continue →" below.

**On Continue:** PATCH `preferred_tab` to `'hosted-page'` or `'integrate'`.

**Does NOT disable anything.** Both paths remain available from the dashboard. This sets the default focus.

## Step 4: Live Test

**Purpose:** The "aha moment." Operator signs in as an end user and watches the session materialize in real-time.

**Layout:** Full-width centered. Three visual states:

**State 1 — Prompt:**
- Headline: "Try signing in as an end user"
- Subtitle: "We'll open your login page in a new tab. Sign in and watch the session appear here in real-time."
- Button: "Open Login Page →" — opens `/t/{slug}/login` via `window.open`
- Transitions to State 2 on click

**State 2 — Waiting:**
- Headline: "Waiting for your first sign-in..."
- Pulsing animation (dot/ring, not a spinner)
- "Reopen login page →" link
- After 60 seconds: subtitle changes to "Still waiting — make sure you're signing in at the page we opened. [Reopen →]"
- Listens on existing `tenant:activity` WS subscription. On `login_succeeded` or `session_started` event → State 3

**State 3 — Success:**
- Checkmark animation (scale-in, ~300ms ease-out)
- Headline: "It works! Your first session is live."
- Session card: email, provider icon + name, timestamp
- "Continue →" appears after ~500ms settle delay

**Edge case — existing session:** On mount, check tenant stats. If active sessions > 0, skip to modified State 3: "You already have an active session" with details.

**Edge case — tab closed mid-wait:** WS events may have fired while away. On resume, check tenant stats and show State 3 if sessions exist.

**WS mechanics:** Uses existing `socket.subscribe('tenant:activity', ...)`. No new topics. The wizard just adds a check during State 2.

## Step 5: Integration or Launch

**Purpose:** Confirm the setup and hand off to the dashboard.

**Layout depends on Step 3 choice:**

**Hosted Page path:**
- Headline: "Your hosted page is live"
- Large URL: `https://{domain}/t/{slug}/site` with copy button
- "Open in new tab →" link
- Summary card: logo thumbnail, color swatch, app name, enabled providers, mode badge
- "Launch Dashboard →" button

**Integrate path:**
- Headline: "Add authentication to your app"
- Public client identifier display (tenant slug / ID — NOT the management API key). Copy button.
- Framework tabs: React, Next.js, Vanilla JS, REST API. Each shows a minimal code snippet with tenant slug and server URL interpolated.
- Login page URL with copy button: "This works for testing even without integration."
- "Launch Dashboard →" button

**On "Launch Dashboard →":** PATCH `onboarding_completed = true`, `onboarding_step = 6`. Toast: "Your workspace is ready." Dashboard opens to `preferred_tab` view.

**Management API key is NOT shown here.** It's a server-side secret. Showing it next to frontend framework snippets invites client-side misuse. It lives in Settings > API Keys with proper context.

## Overview Cleanup

With the wizard handling onboarding, the overview becomes a clean operational dashboard.

**Remove:**
- "Recommended Next Steps" checklist section (OverviewView.ts, lines 52–100)
- "Hide Checklist" button
- "Tenant Surfaces" section (Login Page / Protected Site / Auth Reference cards) — move these links to the Integrate tab

**Keep:**
- Quick Check stats (4 cards)
- Authentication Trend chart
- Provider Mix
- Recent Activity
- Tenant Auth Status
- Latest User card

**Add:**
- Intent badge in overview header, next to tenant name. Small pill: "SaaS App" / "Internal Tool" / etc.
- In the Security tab, top line: "Your security defaults are tuned for a SaaS app. [Customize →]"

## Architecture

**Single component:** `admin/src/platform/console/OnboardingWizard.ts` (~500 lines). Step rendering via `wizardStep` signal and `createSwitch`.

**CSS:** New section in `admin/src/core/theme/components.css` (~150 lines). Wizard layout, split pane, step cards, animations.

**PostMessage listener:** ~20 lines of JS added to the login page template in `crates/server/src/handlers/hosted_login.rs`. Gated by `?preview=1`. Origin-validated.

**No new API endpoints.** All writes go through existing `PATCH /platform/self-serve/tenants/{id}` and `PUT .../security-config`.

**Existing DB fields (already in schema, no migration needed):**
- `onboarding_completed` (boolean, default false) — wizard entry gate
- `onboarding_step` (smallint, default 1) — resume support, set to 6 on completion/skip

**New DB fields (3 columns + 1 JSONB key):**
- `tenant_intent` (text, nullable) — stores intent selection for badge/reference
- `branding_dark_mode` (boolean, default true) — login page theme
- `branding_button_text` (text, default 'Sign in') — login button label
- `security_config.mfa_policy` (string inside existing JSONB column) — `"required"` | `"recommended"` | `"off"`. Migration sets default `"off"` for existing tenants via JSONB merge.

**DB migration:** One migration adding 3 columns to the `tenants` table and setting `mfa_policy` default in `security_config` JSONB for existing rows.

## Files to Modify

| File | Change |
|------|--------|
| `admin/src/platform/console/OnboardingWizard.ts` | **New.** Wizard component with 6 steps. |
| `admin/src/platform/console/ConsoleApp.ts` | Import wizard, render when `onboarding_completed === false` in workspace mode |
| `admin/src/platform/console/store.ts` | Add wizard-related exports, update `switchToWorkspaceInternal` to gate on onboarding flag |
| `admin/src/platform/console/views/OverviewView.ts` | Remove checklist section, remove Tenant Surfaces, add intent badge |
| `admin/src/core/theme/components.css` | Wizard layout styles, split pane, animations |
| `crates/server/src/handlers/hosted_login.rs` | Add PostMessage listener (gated by `?preview=1`, origin-validated) |
| `crates/server/src/handlers/onboarding.rs` | Existing file. Accept new fields in `UpdateTenantRequest` (`tenant_intent`, `branding_dark_mode`, `branding_button_text`), persist to DB |
| `crates/server/src/db/pg_migrations/` | **New migration file.** Add 3 columns to tenants table, set `mfa_policy` default in `security_config` JSONB |

## Out of Scope

- Favicon, custom CSS, background images, email template branding, custom fonts (post-onboarding Settings)
- OAuth credential entry in wizard (Settings > Providers)
- SAML/SSO, custom OIDC providers
- Microsoft and Apple OAuth (shown as "Coming soon")
- Custom HTML injection zones — planned as `footer_links` field (array of `{label, url}` pairs for ToS/Privacy/Support links) on the tenant record. Covers 90% of injection zone use cases without sanitization complexity. Will be a post-onboarding branding field in Settings.
- i18n / localization
- Animated backgrounds
- Logo file upload / drag-and-drop (URL input for now, file upload is a future enhancement)
