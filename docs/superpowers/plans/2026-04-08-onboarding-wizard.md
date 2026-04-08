# Onboarding Wizard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a full-page onboarding wizard that replaces the current overview checklist, guiding operators through tenant setup in 6 steps before showing the dashboard.

**Architecture:** Single `OnboardingWizard.ts` component gated by `onboarding_completed === false` in ConsoleApp. Each step calls existing `updateTenant` PATCH endpoint. Live branding preview via PostMessage to an iframe of the login page. Live test uses existing WS `tenant:activity` subscription. Overview cleaned up to remove checklist and surfaces.

**Tech Stack:** Rust/Axum (backend PATCH handler, login page PostMessage), TypeScript/FormaJS (wizard component, signals, reactive DOM), PostgreSQL (3 new columns + JSONB key)

---

### Task 1: DB migration — add new columns and mfa_policy default

**Files:**
- Create: `crates/server/src/db/pg_migrations/002_onboarding_wizard.sql`

- [ ] **Step 1: Create migration file**

```sql
-- 002_onboarding_wizard.sql
-- Adds wizard-specific columns to tenants table and mfa_policy default to security_config

ALTER TABLE tenants ADD COLUMN IF NOT EXISTS tenant_intent text;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_dark_mode boolean NOT NULL DEFAULT true;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_button_text text NOT NULL DEFAULT 'Sign in';

-- Set mfa_policy default in security_config JSONB for existing tenants
UPDATE tenants
SET security_config = security_config || '{"mfa_policy": "off"}'
WHERE NOT (security_config ? 'mfa_policy');
```

- [ ] **Step 2: Verify migration applies**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check -p gatewasm-server 2>&1 | tail -5`
Expected: compiles (migrations are loaded at runtime, not compile-time — this just verifies no syntax issues in Rust)

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/db/pg_migrations/002_onboarding_wizard.sql
git commit -m "feat(db): add tenant_intent, branding_dark_mode, branding_button_text columns + mfa_policy default"
```

---

### Task 2: Backend — accept new fields in tenant update handler

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs:234-248` (UpdateTenantRequest struct)
- Modify: `crates/server/src/handlers/onboarding.rs:822-847` (handler SQL blocks)
- Modify: `crates/server/src/handlers/onboarding.rs:856-862` (return SELECT query)

- [ ] **Step 1: Add new fields to UpdateTenantRequest**

In `crates/server/src/handlers/onboarding.rs`, the struct at line 234 currently ends with `primary_color`. Add three new fields before the closing brace:

```rust
    pub tenant_intent: Option<String>,
    pub branding_dark_mode: Option<bool>,
    pub branding_button_text: Option<String>,
```

- [ ] **Step 2: Add SQL UPDATE blocks for new fields**

In the same file, after the `preferred_tab` UPDATE block (around line 847), add:

```rust
        if let Some(ref intent) = body.tenant_intent {
            sqlx::query("UPDATE tenants SET tenant_intent = $1, updated_at = now() WHERE id = $2")
                .bind(intent)
                .bind(&tenant_id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(dark_mode) = body.branding_dark_mode {
            sqlx::query("UPDATE tenants SET branding_dark_mode = $1, updated_at = now() WHERE id = $2")
                .bind(dark_mode)
                .bind(&tenant_id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(ref btn_text) = body.branding_button_text {
            sqlx::query("UPDATE tenants SET branding_button_text = $1, updated_at = now() WHERE id = $2")
                .bind(btn_text)
                .bind(&tenant_id)
                .execute(&mut *tx)
                .await?;
        }
```

- [ ] **Step 3: Update return SELECT to include new columns**

Find the return SELECT query (around line 856). It currently selects specific columns. Add `tenant_intent`, `branding_dark_mode`, `branding_button_text` to the SELECT list. If it uses `SELECT *`, no change needed — verify by reading the query.

- [ ] **Step 4: Verify it compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check -p gatewasm-server 2>&1 | tail -5`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs
git commit -m "feat(onboarding): accept tenant_intent, branding_dark_mode, branding_button_text in tenant update"
```

---

### Task 3: Backend — add mfa_policy to SecurityConfig

**Files:**
- Modify: `crates/server/src/security_config.rs:6-46` (TenantSecurityConfig struct)

- [ ] **Step 1: Add mfa_policy field**

In `crates/server/src/security_config.rs`, add after the `max_sessions_per_user` field (around line 17):

```rust
    /// MFA policy: "required", "recommended", or "off"
    #[serde(default = "default_mfa_policy")]
    pub mfa_policy: String,
```

- [ ] **Step 2: Add default function**

Add after the existing default functions (look for `fn default_idle_timeout` etc.):

```rust
fn default_mfa_policy() -> String {
    "off".to_string()
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check -p gatewasm-server 2>&1 | tail -5`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/security_config.rs
git commit -m "feat(security): add mfa_policy field to TenantSecurityConfig"
```

---

### Task 4: Backend — PostMessage preview listener on login page

**Files:**
- Modify: `crates/server/src/handlers/hosted_login.rs:95-132` (template rendering)

- [ ] **Step 1: Read the current hosted_login.rs to understand the template structure**

Read `crates/server/src/handlers/hosted_login.rs` fully to understand where `config_script` and `personality_css` are injected. Identify the exact location where the preview script should be added.

- [ ] **Step 2: Add preview PostMessage listener**

The login page renders with a `config_script` that's injected into a `<script>` tag. When `?preview=1` is in the URL, add an additional script block that:
1. Validates message origin against the platform's known origin
2. Disables form submission
3. Listens for `forma:branding` and `forma:providers` messages

In the handler function, after the existing `config_script` construction, add:

```rust
    let preview_mode = uri.query().map_or(false, |q| q.contains("preview=1"));
    let preview_script = if preview_mode {
        let origin = &state.config.server_url;
        format!(r#"
(function() {{
  var origin = '{}';
  // Disable form submission in preview mode
  document.addEventListener('submit', function(e) {{ e.preventDefault(); }}, true);
  window.addEventListener('message', function(e) {{
    if (e.origin !== origin) return;
    var d = e.data;
    if (!d || typeof d !== 'object') return;
    if (d.type === 'forma:branding') {{
      if (d.primaryColor) document.documentElement.style.setProperty('--accent', d.primaryColor);
      if (d.appName) {{
        var el = document.querySelector('[data-branding="app-name"]');
        if (el) el.textContent = d.appName;
      }}
      if (d.buttonText) {{
        var btn = document.querySelector('[data-branding="submit-btn"]');
        if (btn) btn.textContent = d.buttonText;
      }}
      if (d.logoUrl !== undefined) {{
        var logo = document.querySelector('[data-branding="logo"]');
        if (logo) logo.src = d.logoUrl || '';
      }}
      if (d.darkMode !== undefined) {{
        document.body.setAttribute('data-theme', d.darkMode ? 'dark' : 'light');
      }}
    }}
    if (d.type === 'forma:providers') {{
      var btns = document.querySelectorAll('[data-provider]');
      btns.forEach(function(b) {{
        var p = b.getAttribute('data-provider');
        var enabled = d.providers && d.providers.indexOf(p) !== -1;
        b.style.display = enabled ? '' : 'none';
        if (d.noCredentials && d.noCredentials.indexOf(p) !== -1) {{
          b.style.opacity = '0.4';
          b.style.pointerEvents = 'none';
        }} else {{
          b.style.opacity = '';
          b.style.pointerEvents = '';
        }}
      }});
    }}
  }});
}})();
"#, origin)
    } else {
        String::new()
    };
```

Then include `preview_script` in the rendered HTML after the existing config script.

- [ ] **Step 3: Add data-branding and data-provider attributes to login page HTML**

The login page template needs `data-branding="app-name"`, `data-branding="submit-btn"`, `data-branding="logo"`, and `data-provider="google"` etc. attributes on the relevant DOM elements. Find where the login page HTML is constructed and add these attributes to the existing elements. Read the file to identify exact locations.

- [ ] **Step 4: Pass `uri` to the handler if not already available**

Check if the handler function has access to the request URI for detecting `?preview=1`. If not, add the extractor. The Axum handler should already have access via `axum::extract::Query` or `axum::http::Uri`.

- [ ] **Step 5: Verify it compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check -p gatewasm-server 2>&1 | tail -5`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/handlers/hosted_login.rs
git commit -m "feat(login): add PostMessage preview listener for branding/provider live preview"
```

---

### Task 5: Frontend — update Tenant type and wizard store functions

**Files:**
- Modify: `admin/src/platform/console/store.ts:19-34` (Tenant interface)
- Modify: `admin/src/platform/console/store.ts:670-692` (updateTenant function)

- [ ] **Step 1: Add new fields to Tenant interface**

In `admin/src/platform/console/store.ts`, the Tenant interface (line 19). Add after `preferred_tab`:

```ts
  tenant_intent: string | null;
  branding_dark_mode: boolean;
  branding_button_text: string;
```

- [ ] **Step 2: Expand updateTenant field list**

In `admin/src/platform/console/store.ts`, the `updateTenant` function (line 670) has a `fields` parameter typed as `Partial<Pick<Tenant, ...>>`. Add the new fields to the Pick list:

```ts
export async function updateTenant(
  id: string,
  fields: Partial<Pick<Tenant,
    'name' | 'slug' | 'primary_color' | 'login_redirect_url' |
    'onboarding_completed' | 'onboarding_step' | 'preferred_tab' |
    'tenant_intent' | 'branding_dark_mode' | 'branding_button_text'
  >>,
): Promise<Tenant | null> {
```

- [ ] **Step 3: Add updateSecurityConfig helper for mfa_policy**

Find the existing security config update function in store.ts. If one exists, verify it can set `mfa_policy`. If not, the wizard will use the existing `PUT .../security-config` endpoint. Read the file to find the exact function.

- [ ] **Step 4: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat(store): add wizard fields to Tenant type and updateTenant"
```

---

### Task 6: Frontend — OnboardingWizard shell + Step 0 (Intent)

**Files:**
- Create: `admin/src/platform/console/OnboardingWizard.ts`

- [ ] **Step 1: Create the wizard component file with shell and Step 0**

Create `admin/src/platform/console/OnboardingWizard.ts` with the component shell (header, step indicator, navigation, skip link) and Step 0 (Intent Capture).

```ts
import { createSignal, h, createShow } from 'formajs';
import {
  selectedTenant, updateTenant, toast, serverUrl, socket,
} from './store';
import type { Tenant, ConsoleView } from './store';

// ── Types ──────────────────────────────────────────────────────────────

type TenantIntent = 'saas' | 'internal' | 'community' | 'client-portal' | 'marketplace';

interface IntentOption {
  key: TenantIntent;
  title: string;
  description: string;
}

const INTENT_OPTIONS: IntentOption[] = [
  { key: 'saas', title: 'SaaS Application', description: 'Multi-tenant app with user accounts' },
  { key: 'internal', title: 'Internal Tool', description: 'Company-internal app, employees only' },
  { key: 'community', title: 'Community Site', description: 'Open registration, social features' },
  { key: 'client-portal', title: 'Client Portal', description: 'Restricted access for your clients' },
  { key: 'marketplace', title: 'Marketplace / API', description: 'Developer platform, API keys, OAuth clients' },
];

interface IntentDefaults {
  signup_mode: string;
  idle_timeout_minutes: number;
  max_sessions_per_user: number;
  mfa_policy: string;
  allowed_providers: string[];
  preferred_tab: string;
}

const INTENT_DEFAULTS: Record<TenantIntent, IntentDefaults> = {
  saas: { signup_mode: 'open', idle_timeout_minutes: 30, max_sessions_per_user: 5, mfa_policy: 'recommended', allowed_providers: ['email', 'google'], preferred_tab: 'integrate' },
  internal: { signup_mode: 'invite_only', idle_timeout_minutes: 15, max_sessions_per_user: 2, mfa_policy: 'required', allowed_providers: ['email', 'google'], preferred_tab: 'integrate' },
  community: { signup_mode: 'open', idle_timeout_minutes: 60, max_sessions_per_user: 10, mfa_policy: 'off', allowed_providers: ['email', 'google', 'github'], preferred_tab: 'hosted-page' },
  'client-portal': { signup_mode: 'invite_only', idle_timeout_minutes: 30, max_sessions_per_user: 3, mfa_policy: 'recommended', allowed_providers: ['email'], preferred_tab: 'hosted-page' },
  marketplace: { signup_mode: 'open', idle_timeout_minutes: 60, max_sessions_per_user: 10, mfa_policy: 'off', allowed_providers: ['email', 'github'], preferred_tab: 'integrate' },
};

const STEP_LABELS = ['Intent', 'Branding', 'Auth', 'Mode', 'Test', 'Launch'];

// ── Signals ────────────────────────────────────────────────────────────

const [wizardStep, setWizardStep] = createSignal(0);
const [selectedIntent, setSelectedIntent] = createSignal<TenantIntent | null>(null);
const [saving, setSaving] = createSignal(false);

// ── Navigation helpers ─────────────────────────────────────────────────

function nextStep(): void {
  setWizardStep(Math.min(wizardStep() + 1, 5));
  const tenant = selectedTenant();
  if (tenant) {
    void updateTenant(tenant.id, { onboarding_step: (wizardStep() + 1) as any });
  }
}

function prevStep(): void {
  setWizardStep(Math.max(wizardStep() - 1, 0));
}

async function skipWizard(): Promise<void> {
  const tenant = selectedTenant();
  if (!tenant) return;
  // Apply SaaS defaults if no intent selected
  if (!tenant.tenant_intent && !selectedIntent()) {
    const defaults = INTENT_DEFAULTS.saas;
    await updateTenant(tenant.id, {
      tenant_intent: 'saas',
      preferred_tab: defaults.preferred_tab,
    });
  }
  await updateTenant(tenant.id, { onboarding_completed: true, onboarding_step: 6 as any });
  toast.show('Your workspace is ready', 'success');
}

// ── Step 0: Intent Capture ─────────────────────────────────────────────

function StepIntent(): ReturnType<typeof h> {
  const intent = selectedIntent;

  async function handleContinue(): Promise<void> {
    const tenant = selectedTenant();
    const pick = intent();
    if (!tenant || !pick) return;

    setSaving(true);
    const defaults = INTENT_DEFAULTS[pick];
    await updateTenant(tenant.id, {
      tenant_intent: pick,
      preferred_tab: defaults.preferred_tab,
    } as any);
    // Update allowed_providers
    await updateTenant(tenant.id, { allowed_providers: defaults.allowed_providers } as any);
    // Update security config (idle_timeout, max_sessions, mfa_policy)
    // Uses the existing PUT /platform/self-serve/tenants/{id}/security-config endpoint
    try {
      await fetch(serverUrl + '/platform/self-serve/tenants/' + tenant.id + '/security-config', {
        method: 'PUT',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          idle_timeout_minutes: defaults.idle_timeout_minutes,
          max_sessions_per_user: defaults.max_sessions_per_user,
          mfa_policy: defaults.mfa_policy,
        }),
      });
    } catch { /* security config update is best-effort during onboarding */ }
    setSaving(false);
    nextStep();
  }

  return h('div', { class: 'wizard-step wizard-step-intent' },
    h('h1', { class: 'wizard-headline' }, 'What are you building?'),
    h('p', { class: 'wizard-subtitle' }, 'This helps us configure sensible defaults so you don\u2019t have to.'),
    h('div', { class: 'intent-grid' },
      ...INTENT_OPTIONS.map((opt) =>
        h('button', {
          type: 'button',
          class: () => 'intent-card' + (intent() === opt.key ? ' intent-card-selected' : ''),
          onClick: () => setSelectedIntent(opt.key),
        },
          h('div', { class: 'intent-card-title' }, opt.title),
          h('div', { class: 'intent-card-desc' }, opt.description),
        ),
      ),
    ),
    h('div', { class: 'wizard-actions' },
      h('button', {
        type: 'button',
        class: 'btn btn-accent',
        disabled: () => intent() === null || saving(),
        onClick: () => void handleContinue(),
      }, () => saving() ? 'Saving\u2026' : 'Continue \u2192'),
    ),
  );
}

// ── Step 1: Branding (placeholder — implemented in Task 7) ─────────────

function StepBranding(): ReturnType<typeof h> {
  return h('div', { class: 'wizard-step' }, 'Step 1: Branding — not yet implemented');
}

// ── Step 2: Auth Methods (placeholder — implemented in Task 8) ─────────

function StepAuthMethods(): ReturnType<typeof h> {
  return h('div', { class: 'wizard-step' }, 'Step 2: Auth Methods — not yet implemented');
}

// ── Step 3: Mode Selection (placeholder — implemented in Task 9) ───────

function StepModeSelection(): ReturnType<typeof h> {
  return h('div', { class: 'wizard-step' }, 'Step 3: Mode Selection — not yet implemented');
}

// ── Step 4: Live Test (placeholder — implemented in Task 9) ────────────

function StepLiveTest(): ReturnType<typeof h> {
  return h('div', { class: 'wizard-step' }, 'Step 4: Live Test — not yet implemented');
}

// ── Step 5: Launch (placeholder — implemented in Task 10) ──────────────

function StepLaunch(): ReturnType<typeof h> {
  return h('div', { class: 'wizard-step' }, 'Step 5: Launch — not yet implemented');
}

// ── Main Component ─────────────────────────────────────────────────────

export function OnboardingWizard(): ReturnType<typeof h> {
  // Restore step from tenant on mount
  const tenant = selectedTenant();
  if (tenant && tenant.onboarding_step > 1) {
    setWizardStep(Math.min(tenant.onboarding_step - 1, 5));
  }

  // Restore intent from tenant if previously set
  if (tenant?.tenant_intent) {
    setSelectedIntent(tenant.tenant_intent as TenantIntent);
  }

  return h('div', { class: 'wizard-root' },
    // Header
    h('div', { class: 'wizard-header' },
      h('div', { class: 'wizard-logo' }, 'fAuth'),
      h('button', {
        type: 'button',
        class: 'wizard-skip',
        onClick: () => void skipWizard(),
      }, 'Skip setup \u2192'),
    ),
    // Step indicator
    h('div', { class: 'wizard-step-indicator' },
      ...STEP_LABELS.map((label, i) =>
        h('div', {
          class: () => 'wizard-step-dot'
            + (wizardStep() === i ? ' wizard-step-dot-active' : '')
            + (wizardStep() > i ? ' wizard-step-dot-done' : ''),
        }, label),
      ),
    ),
    // Step content
    createShow(() => wizardStep() === 0, StepIntent),
    createShow(() => wizardStep() === 1, StepBranding),
    createShow(() => wizardStep() === 2, StepAuthMethods),
    createShow(() => wizardStep() === 3, StepModeSelection),
    createShow(() => wizardStep() === 4, StepLiveTest),
    createShow(() => wizardStep() === 5, StepLaunch),
    // Back link (not on step 0)
    createShow(
      () => wizardStep() > 0,
      () => h('div', { class: 'wizard-back' },
        h('button', {
          type: 'button',
          class: 'btn btn-ghost btn-sm',
          onClick: prevStep,
        }, '\u2190 Back'),
      ),
    ),
  );
}
```

- [ ] **Step 2: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds (component not yet wired in, but module should parse)

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts
git commit -m "feat(wizard): create OnboardingWizard shell with Step 0 (Intent Capture)"
```

---

### Task 7: Frontend — Step 1 (Branding) with live iframe preview

**Files:**
- Modify: `admin/src/platform/console/OnboardingWizard.ts` (replace StepBranding placeholder)

- [ ] **Step 1: Read the current OnboardingWizard.ts**

Read the file to find the `StepBranding` placeholder function.

- [ ] **Step 2: Replace StepBranding with full implementation**

Replace the `StepBranding` placeholder function with:

```ts
function StepBranding(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  const [appName, setAppName] = createSignal(tenant.branding_app_name || tenant.name);
  const [logoUrl, setLogoUrl] = createSignal(tenant.branding_logo_url || '');
  const [primaryColor, setPrimaryColor] = createSignal(tenant.branding_primary_color || '#6366f1');
  const [darkMode, setDarkMode] = createSignal(tenant.branding_dark_mode ?? true);
  const [buttonText, setButtonText] = createSignal(tenant.branding_button_text || 'Sign in');

  let iframeRef: HTMLIFrameElement | null = null;
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function postBranding(): void {
    if (!iframeRef?.contentWindow) return;
    iframeRef.contentWindow.postMessage({
      type: 'forma:branding',
      primaryColor: primaryColor(),
      logoUrl: logoUrl(),
      appName: appName(),
      darkMode: darkMode(),
      buttonText: buttonText(),
    }, serverUrl);
  }

  function debouncedPost(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(postBranding, 100);
  }

  function immediatePost(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    postBranding();
  }

  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, {
      branding_app_name: appName(),
      branding_logo_url: logoUrl() || undefined,
      branding_primary_color: primaryColor(),
      branding_dark_mode: darkMode(),
      branding_button_text: buttonText(),
    } as any);
    setSaving(false);
    nextStep();
  }

  const iframeSrc = serverUrl + '/t/' + tenant.slug + '/login?preview=1';

  return h('div', { class: 'wizard-step wizard-split-pane' },
    // Left: form controls
    h('div', { class: 'wizard-pane-left' },
      h('h1', { class: 'wizard-headline' }, 'Customize your login page'),
      h('p', { class: 'wizard-subtitle' }, 'Your users will see this when they sign in.'),

      h('label', { class: 'wizard-field' },
        h('span', null, 'App name'),
        h('input', {
          type: 'text',
          class: 'input',
          value: appName(),
          onInput: (e: Event) => { setAppName((e.target as HTMLInputElement).value); debouncedPost(); },
        }),
      ),

      h('label', { class: 'wizard-field' },
        h('span', null, 'Logo URL'),
        h('input', {
          type: 'url',
          class: 'input',
          placeholder: 'https://example.com/logo.png',
          value: logoUrl(),
          onInput: (e: Event) => { setLogoUrl((e.target as HTMLInputElement).value); debouncedPost(); },
        }),
        createShow(
          () => logoUrl() !== '',
          () => h('img', { src: () => logoUrl(), class: 'wizard-logo-preview', alt: 'Logo preview' }),
        ),
      ),

      h('label', { class: 'wizard-field' },
        h('span', null, 'Primary color'),
        h('div', { class: 'wizard-color-row' },
          h('input', {
            type: 'color',
            value: primaryColor(),
            onInput: (e: Event) => { setPrimaryColor((e.target as HTMLInputElement).value); immediatePost(); },
          }),
          h('input', {
            type: 'text',
            class: 'input input-sm',
            value: () => primaryColor(),
            onInput: (e: Event) => { setPrimaryColor((e.target as HTMLInputElement).value); debouncedPost(); },
          }),
        ),
      ),

      h('label', { class: 'wizard-field wizard-field-row' },
        h('span', null, 'Dark mode'),
        h('input', {
          type: 'checkbox',
          checked: () => darkMode(),
          onChange: (e: Event) => { setDarkMode((e.target as HTMLInputElement).checked); immediatePost(); },
        }),
      ),

      h('label', { class: 'wizard-field' },
        h('span', null, 'Button text'),
        h('input', {
          type: 'text',
          class: 'input',
          value: buttonText(),
          onInput: (e: Event) => { setButtonText((e.target as HTMLInputElement).value); debouncedPost(); },
        }),
      ),

      h('div', { class: 'wizard-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-accent',
          disabled: () => saving(),
          onClick: () => void handleContinue(),
        }, () => saving() ? 'Saving\u2026' : 'Continue \u2192'),
      ),
    ),

    // Right: live preview iframe
    h('div', { class: 'wizard-pane-right' },
      h('div', { class: 'wizard-preview-label' }, 'Live Preview'),
      h('iframe', {
        src: iframeSrc,
        class: 'wizard-preview-iframe',
        ref: (el: HTMLIFrameElement) => { iframeRef = el; },
        onLoad: () => setTimeout(postBranding, 300),
      }),
    ),
  );
}
```

Note: The `branding_app_name` and `branding_logo_url` and `branding_primary_color` fields already exist on the Tenant type from the existing schema. The `updateTenant` Pick type may need these added — check and add if missing.

- [ ] **Step 3: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts
git commit -m "feat(wizard): implement Step 1 (Branding) with live PostMessage iframe preview"
```

---

### Task 8: Frontend — Step 2 (Auth Methods) with provider toggles

**Files:**
- Modify: `admin/src/platform/console/OnboardingWizard.ts` (replace StepAuthMethods placeholder)

- [ ] **Step 1: Read the current OnboardingWizard.ts**

Read the file to find the `StepAuthMethods` placeholder function.

- [ ] **Step 2: Replace StepAuthMethods with full implementation**

Replace the `StepAuthMethods` placeholder function with:

```ts
function StepAuthMethods(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  const current = tenant.allowed_providers as string[] || ['email'];
  const [providers, setProviders] = createSignal<string[]>([...current]);

  let iframeRef: HTMLIFrameElement | null = null;

  function toggleProvider(p: string): void {
    const list = providers();
    if (list.includes(p)) {
      // Don't allow removing all providers — keep at least email
      if (p === 'email' && list.length === 1) return;
      setProviders(list.filter((x) => x !== p));
    } else {
      setProviders([...list, p]);
    }
    postProviders();
  }

  function postProviders(): void {
    if (!iframeRef?.contentWindow) return;
    iframeRef.contentWindow.postMessage({
      type: 'forma:providers',
      providers: providers(),
      noCredentials: [], // TODO: detect which providers lack credentials
    }, serverUrl);
  }

  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, {
      allowed_providers: providers(),
    } as any);
    setSaving(false);
    nextStep();
  }

  const iframeSrc = serverUrl + '/t/' + tenant.slug + '/login?preview=1';

  const CREDENTIAL_PROVIDERS = [
    { key: 'email', label: 'Email / Password', group: 'credentials' },
    { key: 'magic_link', label: 'Magic Link', subtitle: 'Passwordless sign-in via email', group: 'credentials' },
  ];

  const SOCIAL_PROVIDERS = [
    { key: 'google', label: 'Google', group: 'social', available: true },
    { key: 'github', label: 'GitHub', group: 'social', available: true },
    { key: 'microsoft', label: 'Microsoft', group: 'social', available: false },
    { key: 'apple', label: 'Apple', group: 'social', available: false },
  ];

  function ProviderToggle(opt: { key: string; label: string; subtitle?: string; available?: boolean }): ReturnType<typeof h> {
    const disabled = opt.available === false;
    return h('label', {
      class: () => 'wizard-provider-toggle' + (disabled ? ' wizard-provider-disabled' : ''),
    },
      h('div', { class: 'wizard-provider-info' },
        h('span', { class: 'wizard-provider-label' }, opt.label),
        opt.subtitle ? h('span', { class: 'wizard-provider-subtitle' }, opt.subtitle) : null,
        disabled ? h('span', { class: 'wizard-provider-badge' }, 'Coming soon') : null,
      ),
      h('input', {
        type: 'checkbox',
        checked: () => providers().includes(opt.key),
        disabled: disabled,
        onChange: () => toggleProvider(opt.key),
      }),
    );
  }

  return h('div', { class: 'wizard-step wizard-split-pane' },
    h('div', { class: 'wizard-pane-left' },
      h('h1', { class: 'wizard-headline' }, 'How will users sign in?'),
      h('p', { class: 'wizard-subtitle' }, 'Choose authentication methods for your app.'),

      h('div', { class: 'wizard-provider-group' },
        h('div', { class: 'wizard-provider-group-label' }, 'Credentials'),
        ...CREDENTIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),

      h('div', { class: 'wizard-provider-group' },
        h('div', { class: 'wizard-provider-group-label' }, 'Social Providers'),
        ...SOCIAL_PROVIDERS.map((p) => ProviderToggle(p)),
      ),

      h('div', { class: 'wizard-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-accent',
          disabled: () => saving(),
          onClick: () => void handleContinue(),
        }, () => saving() ? 'Saving\u2026' : 'Continue \u2192'),
      ),
    ),

    h('div', { class: 'wizard-pane-right' },
      h('div', { class: 'wizard-preview-label' }, 'Live Preview'),
      h('iframe', {
        src: iframeSrc,
        class: 'wizard-preview-iframe',
        ref: (el: HTMLIFrameElement) => { iframeRef = el; },
        onLoad: () => setTimeout(postProviders, 300),
      }),
    ),
  );
}
```

- [ ] **Step 3: Add `allowed_providers` to updateTenant Pick type if missing**

In `store.ts`, check if the `updateTenant` function's `Pick` type includes `allowed_providers`. The backend accepts it but the frontend type needs to allow it. If missing, read the function and add it.

- [ ] **Step 4: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts admin/src/platform/console/store.ts
git commit -m "feat(wizard): implement Step 2 (Auth Methods) with provider toggles and live preview"
```

---

### Task 9: Frontend — Step 3 (Mode Selection) and Step 4 (Live Test)

**Files:**
- Modify: `admin/src/platform/console/OnboardingWizard.ts` (replace StepModeSelection and StepLiveTest placeholders)

- [ ] **Step 1: Read the current OnboardingWizard.ts**

Read the file to find both placeholder functions.

- [ ] **Step 2: Replace StepModeSelection with full implementation**

```ts
function StepModeSelection(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  const [modeChoice, setModeChoice] = createSignal<'hosted-page' | 'integrate'>(
    (tenant.preferred_tab === 'integrate' ? 'integrate' : 'hosted-page') as any,
  );

  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, { preferred_tab: modeChoice() });
    setSaving(false);
    nextStep();
  }

  return h('div', { class: 'wizard-step wizard-step-centered' },
    h('h1', { class: 'wizard-headline' }, 'How do you want to use authentication?'),
    h('p', { class: 'wizard-subtitle' }, 'You can always switch later from the dashboard.'),

    h('div', { class: 'mode-grid' },
      h('button', {
        type: 'button',
        class: () => 'mode-card' + (modeChoice() === 'hosted-page' ? ' mode-card-selected' : ''),
        onClick: () => setModeChoice('hosted-page'),
      },
        h('div', { class: 'mode-card-badge' }, 'Recommended'),
        h('div', { class: 'mode-card-title' }, 'Launch a hosted page'),
        h('div', { class: 'mode-card-subtitle' }, 'A working login page and protected site in 30 seconds. No code required.'),
        h('ul', { class: 'mode-card-list' },
          h('li', null, 'Hosted at /t/' + tenant.slug + '/site'),
          h('li', null, 'End users sign in and see your branded page'),
          h('li', null, 'Edit content anytime from the dashboard'),
        ),
      ),
      h('button', {
        type: 'button',
        class: () => 'mode-card' + (modeChoice() === 'integrate' ? ' mode-card-selected' : ''),
        onClick: () => setModeChoice('integrate'),
      },
        h('div', { class: 'mode-card-title' }, 'Integrate into your app'),
        h('div', { class: 'mode-card-subtitle' }, 'Drop our SDK into your codebase. Full control over the auth flow.'),
        h('ul', { class: 'mode-card-list' },
          h('li', null, 'API keys and code snippets'),
          h('li', null, 'Framework-specific guides'),
          h('li', null, 'Login page still works for testing'),
        ),
      ),
    ),

    h('div', { class: 'wizard-actions' },
      h('button', {
        type: 'button',
        class: 'btn btn-accent',
        disabled: () => saving(),
        onClick: () => void handleContinue(),
      }, () => saving() ? 'Saving\u2026' : 'Continue \u2192'),
    ),
  );
}
```

- [ ] **Step 3: Replace StepLiveTest with full implementation**

```ts
function StepLiveTest(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  type TestState = 'prompt' | 'waiting' | 'success';
  const [testState, setTestState] = createSignal<TestState>('prompt');
  const [nudge, setNudge] = createSignal(false);
  const [sessionInfo, setSessionInfo] = createSignal<{ email: string; provider: string; time: string } | null>(null);
  let nudgeTimer: ReturnType<typeof setTimeout> | null = null;

  // Check if tenant already has sessions on mount
  // The selectedTenantStats from store would have active_sessions — check it
  // For simplicity, we detect via the WS subscription

  function openLoginPage(): void {
    window.open(serverUrl + '/t/' + tenant.slug + '/login', '_blank');
    setTestState('waiting');
    setNudge(false);

    // 60-second nudge timer
    nudgeTimer = setTimeout(() => setNudge(true), 60000);

    // Listen for tenant activity via existing WS subscription
    // The socket is already subscribed to tenant:activity in store.ts
    // We add a one-time check that polls for the session
    pollForSession();
  }

  async function pollForSession(): Promise<void> {
    // Poll every 3 seconds for a new session (WS events are handled globally,
    // but we need to detect the session in the wizard context)
    const interval = setInterval(async () => {
      if (testState() !== 'waiting') {
        clearInterval(interval);
        return;
      }
      try {
        const res = await fetch(serverUrl + '/t/' + tenant.slug + '/admin/tenant/detail?window=24h', {
          credentials: 'include',
          credentials: 'include',
        });
        if (res.ok) {
          const data = await res.json();
          if (data.active_sessions > 0) {
            clearInterval(interval);
            if (nudgeTimer) clearTimeout(nudgeTimer);
            setSessionInfo({
              email: data.latest_user?.email || 'Unknown',
              provider: data.latest_user?.provider || 'email',
              time: new Date().toLocaleTimeString(),
            });
            setTestState('success');
          }
        }
      } catch { /* ignore polling errors */ }
    }, 3000);
  }

  return h('div', { class: 'wizard-step wizard-step-centered' },
    // State 1: Prompt
    createShow(() => testState() === 'prompt', () =>
      h('div', { class: 'wizard-test-prompt' },
        h('h1', { class: 'wizard-headline' }, 'Try signing in as an end user'),
        h('p', { class: 'wizard-subtitle' }, 'We\u2019ll open your login page in a new tab. Sign in and watch the session appear here in real-time.'),
        h('button', {
          type: 'button',
          class: 'btn btn-accent btn-lg',
          onClick: openLoginPage,
        }, 'Open Login Page \u2192'),
      ),
    ),

    // State 2: Waiting
    createShow(() => testState() === 'waiting', () =>
      h('div', { class: 'wizard-test-waiting' },
        h('div', { class: 'wizard-pulse' }),
        h('h1', { class: 'wizard-headline' }, 'Waiting for your first sign-in\u2026'),
        h('p', { class: 'wizard-subtitle' }, () =>
          nudge()
            ? 'Still waiting \u2014 make sure you\u2019re signing in at the page we opened.'
            : 'Sign in at the page we just opened. This will update the moment we detect your session.',
        ),
        h('button', {
          type: 'button',
          class: 'btn btn-ghost btn-sm',
          onClick: () => window.open(serverUrl + '/t/' + tenant.slug + '/login', '_blank'),
        }, 'Reopen login page \u2192'),
      ),
    ),

    // State 3: Success
    createShow(() => testState() === 'success', () =>
      h('div', { class: 'wizard-test-success' },
        h('div', { class: 'wizard-checkmark' }, '\u2713'),
        h('h1', { class: 'wizard-headline' }, 'It works! Your first session is live.'),
        createShow(() => sessionInfo() !== null, () =>
          h('div', { class: 'wizard-session-card' },
            h('div', null, sessionInfo()!.email),
            h('div', { class: 'wizard-session-meta' },
              sessionInfo()!.provider + ' \u00B7 ' + sessionInfo()!.time,
            ),
          ),
        ),
        h('div', { class: 'wizard-actions' },
          h('button', {
            type: 'button',
            class: 'btn btn-accent',
            onClick: nextStep,
          }, 'Continue \u2192'),
        ),
      ),
    ),
  );
}
```

- [ ] **Step 4: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts
git commit -m "feat(wizard): implement Step 3 (Mode Selection) and Step 4 (Live Test with WS detection)"
```

---

### Task 10: Frontend — Step 5 (Launch) with completion logic

**Files:**
- Modify: `admin/src/platform/console/OnboardingWizard.ts` (replace StepLaunch placeholder)

- [ ] **Step 1: Read the current OnboardingWizard.ts**

Read the file to find the `StepLaunch` placeholder function.

- [ ] **Step 2: Replace StepLaunch with full implementation**

```ts
function StepLaunch(): ReturnType<typeof h> {
  const tenant = selectedTenant();
  if (!tenant) return h('div', null, 'No tenant selected');

  const isHosted = (tenant.preferred_tab || 'hosted-page') === 'hosted-page';
  const siteUrl = serverUrl + '/t/' + tenant.slug + '/site';
  const loginUrl = serverUrl + '/t/' + tenant.slug + '/login';
  const [copied, setCopied] = createSignal(false);
  const [activeFramework, setActiveFramework] = createSignal('react');

  function copyToClipboard(text: string): void {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  async function handleLaunch(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, { onboarding_completed: true, onboarding_step: 6 as any });
    setSaving(false);
    toast.show('Your workspace is ready', 'success');
    // The ConsoleApp will detect onboarding_completed = true and render the dashboard
  }

  const FRAMEWORK_SNIPPETS: Record<string, string> = {
    react: `import { FormaAuth } from '@getforma/react';

function App() {
  return (
    <FormaAuth
      tenant="${tenant.slug}"
      serverUrl="${serverUrl}"
    />
  );
}`,
    nextjs: `// app/layout.tsx
import { FormaProvider } from '@getforma/nextjs';

export default function Layout({ children }) {
  return (
    <FormaProvider tenant="${tenant.slug}" serverUrl="${serverUrl}">
      {children}
    </FormaProvider>
  );
}`,
    vanilla: `<script src="${serverUrl}/t/${tenant.slug}/auth.js"></script>
<script>
  FormaAuth.init({
    tenant: '${tenant.slug}',
    serverUrl: '${serverUrl}',
  });
</script>`,
    rest: `# Get a session token
curl -X POST ${serverUrl}/t/${tenant.slug}/auth/login \\
  -H 'Content-Type: application/json' \\
  -d '{"email": "user@example.com", "password": "..."}'`,
  };

  if (isHosted) {
    return h('div', { class: 'wizard-step wizard-step-centered' },
      h('h1', { class: 'wizard-headline' }, 'Your hosted page is live'),
      h('p', { class: 'wizard-subtitle' }, 'End users can sign in and see your branded page right now.'),

      h('div', { class: 'wizard-url-display' },
        h('code', null, siteUrl),
        h('button', {
          type: 'button',
          class: 'btn btn-sm btn-ghost',
          onClick: () => copyToClipboard(siteUrl),
        }, () => copied() ? 'Copied!' : 'Copy'),
      ),

      h('a', {
        href: siteUrl,
        target: '_blank',
        class: 'wizard-link',
      }, 'Open in new tab \u2192'),

      h('div', { class: 'wizard-summary-card' },
        h('div', { class: 'wizard-summary-label' }, 'Your configuration'),
        h('div', { class: 'wizard-summary-row' },
          h('span', null, 'App name'),
          h('span', null, tenant.branding_app_name || tenant.name),
        ),
        h('div', { class: 'wizard-summary-row' },
          h('span', null, 'Primary color'),
          h('span', null,
            h('span', {
              class: 'wizard-color-swatch',
              style: () => 'background:' + (tenant.branding_primary_color || '#6366f1'),
            }),
            tenant.branding_primary_color || '#6366f1',
          ),
        ),
        h('div', { class: 'wizard-summary-row' },
          h('span', null, 'Providers'),
          h('span', null, ((tenant as any).allowed_providers || ['email']).join(', ')),
        ),
        h('div', { class: 'wizard-summary-row' },
          h('span', null, 'Mode'),
          h('span', null, 'Hosted Page'),
        ),
      ),

      h('div', { class: 'wizard-actions' },
        h('button', {
          type: 'button',
          class: 'btn btn-accent btn-lg',
          disabled: () => saving(),
          onClick: () => void handleLaunch(),
        }, () => saving() ? 'Launching\u2026' : 'Launch Dashboard \u2192'),
      ),
    );
  }

  // Integrate path
  return h('div', { class: 'wizard-step wizard-step-centered' },
    h('h1', { class: 'wizard-headline' }, 'Add authentication to your app'),
    h('p', { class: 'wizard-subtitle' }, 'Your login page is ready. Pick a framework to get started.'),

    h('div', { class: 'wizard-client-id' },
      h('span', { class: 'wizard-client-id-label' }, 'Tenant slug'),
      h('code', null, tenant.slug),
      h('button', {
        type: 'button',
        class: 'btn btn-sm btn-ghost',
        onClick: () => copyToClipboard(tenant.slug),
      }, () => copied() ? 'Copied!' : 'Copy'),
    ),

    h('div', { class: 'wizard-framework-tabs' },
      ...['react', 'nextjs', 'vanilla', 'rest'].map((fw) =>
        h('button', {
          type: 'button',
          class: () => 'wizard-fw-tab' + (activeFramework() === fw ? ' wizard-fw-tab-active' : ''),
          onClick: () => setActiveFramework(fw),
        }, fw === 'nextjs' ? 'Next.js' : fw === 'rest' ? 'REST API' : fw.charAt(0).toUpperCase() + fw.slice(1)),
      ),
    ),

    h('pre', { class: 'wizard-code-block' },
      h('code', null, () => FRAMEWORK_SNIPPETS[activeFramework()] || ''),
    ),

    h('div', { class: 'wizard-url-display wizard-url-sm' },
      h('span', null, 'Login page for testing: '),
      h('code', null, loginUrl),
      h('button', {
        type: 'button',
        class: 'btn btn-sm btn-ghost',
        onClick: () => copyToClipboard(loginUrl),
      }, 'Copy'),
    ),

    h('div', { class: 'wizard-actions' },
      h('button', {
        type: 'button',
        class: 'btn btn-accent btn-lg',
        disabled: () => saving(),
        onClick: () => void handleLaunch(),
      }, () => saving() ? 'Launching\u2026' : 'Launch Dashboard \u2192'),
    ),
  );
}
```

- [ ] **Step 3: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts
git commit -m "feat(wizard): implement Step 5 (Launch) with hosted/integrate paths and completion"
```

---

### Task 11: Frontend — wire wizard into ConsoleApp

**Files:**
- Modify: `admin/src/platform/console/ConsoleApp.ts:1-39` (imports)
- Modify: `admin/src/platform/console/ConsoleApp.ts:720-772` (workspace rendering)

- [ ] **Step 1: Read ConsoleApp.ts to find exact import and workspace rendering locations**

Read `admin/src/platform/console/ConsoleApp.ts` lines 1-40 for imports, and lines 700-780 for the workspace mode switch.

- [ ] **Step 2: Add OnboardingWizard import**

Add to the import section:

```ts
import { OnboardingWizard } from './OnboardingWizard';
```

- [ ] **Step 3: Gate workspace rendering on onboarding_completed**

In the workspace `render` function within the `createSwitch(mode, [...])` block, wrap the existing workspace shell in a conditional. The workspace match case currently renders a `div.shell` directly. Change it to:

```ts
{
  match: 'workspace',
  render: () => {
    // Show wizard if onboarding not completed
    return createShow(
      () => selectedTenant() !== null && !selectedTenant()!.onboarding_completed,
      () => OnboardingWizard(),
      () => h('div', { class: 'shell', 'data-plane': () => plane() },
        // ... existing workspace shell content ...
      ),
    );
  },
},
```

This uses `createShow`'s else branch (third argument). If `onboarding_completed` is false, show the wizard. Otherwise, show the normal workspace.

**Important:** Read the existing code carefully. The workspace `render` block may be a large inline function. The change wraps it in a `createShow` — the existing code becomes the else branch.

- [ ] **Step 4: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/ConsoleApp.ts
git commit -m "feat(console): gate workspace on onboarding_completed, show wizard when false"
```

---

### Task 12: Frontend — wizard CSS

**Files:**
- Modify: `admin/src/core/theme/components.css` (append after line 3318)

- [ ] **Step 1: Read the end of components.css to verify append location**

Read `admin/src/core/theme/components.css` last 20 lines to confirm where to append.

- [ ] **Step 2: Append wizard styles**

Add after the last line of `components.css`:

```css
/* ── Onboarding Wizard ──────────────────────────────────────────────── */

.wizard-root {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  background: var(--bg);
  padding: 32px 24px;
}

.wizard-header {
  width: 100%;
  max-width: 900px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 32px;
}

.wizard-logo {
  font-size: 1.1rem;
  font-weight: 700;
  color: var(--accent);
  letter-spacing: -0.02em;
}

.wizard-skip {
  background: none;
  border: none;
  color: var(--text-dim);
  font-size: 0.8rem;
  cursor: pointer;
  padding: 6px 12px;
  border-radius: 6px;
  transition: color 0.15s, background 0.15s;
}

.wizard-skip:hover {
  color: var(--text);
  background: var(--surface-raised);
}

.wizard-step-indicator {
  display: flex;
  gap: 24px;
  margin-bottom: 48px;
}

.wizard-step-dot {
  font-size: 0.7rem;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding-bottom: 8px;
  border-bottom: 2px solid transparent;
  transition: color 0.2s, border-color 0.2s;
}

.wizard-step-dot-active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}

.wizard-step-dot-done {
  color: var(--text-dim);
  border-bottom-color: var(--border);
}

.wizard-step {
  width: 100%;
  max-width: 900px;
  animation: wizardFadeIn 0.25s ease-out;
}

@keyframes wizardFadeIn {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}

.wizard-step-centered {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}

.wizard-headline {
  font-size: 1.5rem;
  font-weight: 600;
  color: var(--text);
  margin: 0 0 8px;
}

.wizard-subtitle {
  font-size: 0.9rem;
  color: var(--text-dim);
  margin: 0 0 32px;
  max-width: 500px;
}

.wizard-actions {
  margin-top: 32px;
  display: flex;
  gap: 12px;
}

.wizard-back {
  position: fixed;
  bottom: 32px;
  left: 32px;
}

/* Intent grid (Step 0) */

.intent-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 12px;
  width: 100%;
  max-width: 700px;
}

.intent-card {
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 20px;
  text-align: left;
  cursor: pointer;
  transition: border-color 0.15s, box-shadow 0.15s;
}

.intent-card:hover {
  border-color: var(--border-strong);
}

.intent-card-selected {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent);
}

.intent-card-title {
  font-weight: 600;
  font-size: 0.9rem;
  color: var(--text);
  margin-bottom: 4px;
}

.intent-card-desc {
  font-size: 0.78rem;
  color: var(--text-dim);
}

/* Split pane (Steps 1-2) */

.wizard-split-pane {
  display: grid;
  grid-template-columns: 45% 55%;
  gap: 32px;
  max-width: 1000px;
}

.wizard-pane-left {
  display: flex;
  flex-direction: column;
}

.wizard-pane-right {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.wizard-preview-label {
  font-size: 0.7rem;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.wizard-preview-iframe {
  width: 100%;
  height: 500px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: #0a0a0c;
}

.wizard-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 16px;
  font-size: 0.82rem;
  color: var(--text-dim);
}

.wizard-field-row {
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
}

.wizard-color-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.wizard-color-row input[type="color"] {
  width: 36px;
  height: 36px;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 2px;
  cursor: pointer;
  background: transparent;
}

.wizard-logo-preview {
  max-width: 120px;
  max-height: 40px;
  margin-top: 4px;
  border-radius: 4px;
}

/* Provider toggles (Step 2) */

.wizard-provider-group {
  margin-bottom: 20px;
}

.wizard-provider-group-label {
  font-size: 0.7rem;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 8px;
}

.wizard-provider-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border: 1px solid var(--border);
  border-radius: 8px;
  margin-bottom: 6px;
  cursor: pointer;
  transition: border-color 0.15s;
}

.wizard-provider-toggle:hover {
  border-color: var(--border-strong);
}

.wizard-provider-disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.wizard-provider-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.wizard-provider-label {
  font-size: 0.85rem;
  font-weight: 500;
  color: var(--text);
}

.wizard-provider-subtitle {
  font-size: 0.75rem;
  color: var(--text-muted);
}

.wizard-provider-badge {
  font-size: 0.65rem;
  color: var(--text-muted);
  background: var(--surface-overlay);
  padding: 2px 6px;
  border-radius: 4px;
  width: fit-content;
}

/* Mode selection (Step 3) */

.mode-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  width: 100%;
  max-width: 700px;
}

.mode-card {
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 24px;
  text-align: left;
  cursor: pointer;
  transition: border-color 0.15s, box-shadow 0.15s;
  position: relative;
}

.mode-card:hover {
  border-color: var(--border-strong);
}

.mode-card-selected {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent);
}

.mode-card-badge {
  font-size: 0.65rem;
  color: var(--accent);
  background: var(--accent-dim);
  border: 1px solid var(--accent-border);
  padding: 2px 8px;
  border-radius: 4px;
  width: fit-content;
  margin-bottom: 12px;
}

.mode-card-title {
  font-weight: 600;
  font-size: 1rem;
  color: var(--text);
  margin-bottom: 6px;
}

.mode-card-subtitle {
  font-size: 0.82rem;
  color: var(--text-dim);
  margin-bottom: 12px;
}

.mode-card-list {
  list-style: none;
  padding: 0;
  margin: 0;
  font-size: 0.78rem;
  color: var(--text-dim);
}

.mode-card-list li {
  padding: 3px 0;
}

.mode-card-list li::before {
  content: '\2713  ';
  color: var(--accent);
}

/* Live test (Step 4) */

.wizard-test-prompt,
.wizard-test-waiting,
.wizard-test-success {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}

.wizard-pulse {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: var(--accent);
  opacity: 0.3;
  margin-bottom: 24px;
  animation: wizardPulse 1.5s ease-in-out infinite;
}

@keyframes wizardPulse {
  0%, 100% { transform: scale(1); opacity: 0.3; }
  50% { transform: scale(1.3); opacity: 0.15; }
}

.wizard-checkmark {
  width: 56px;
  height: 56px;
  border-radius: 50%;
  background: var(--success-dim);
  border: 2px solid var(--success);
  color: var(--success);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 1.5rem;
  margin-bottom: 24px;
  animation: wizardCheckIn 0.3s ease-out;
}

@keyframes wizardCheckIn {
  from { transform: scale(0); }
  to { transform: scale(1); }
}

.wizard-session-card {
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 16px 24px;
  margin-top: 16px;
  text-align: center;
}

.wizard-session-meta {
  font-size: 0.78rem;
  color: var(--text-dim);
  margin-top: 4px;
}

/* Launch step (Step 5) */

.wizard-url-display {
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 14px 20px;
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 16px 0;
  font-size: 0.9rem;
}

.wizard-url-display code {
  flex: 1;
  color: var(--accent);
}

.wizard-url-sm {
  font-size: 0.8rem;
  padding: 10px 16px;
}

.wizard-link {
  font-size: 0.82rem;
  color: var(--accent);
  text-decoration: none;
  margin-bottom: 24px;
}

.wizard-link:hover {
  text-decoration: underline;
}

.wizard-summary-card {
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 20px;
  width: 100%;
  max-width: 400px;
  margin: 16px 0;
}

.wizard-summary-label {
  font-size: 0.7rem;
  color: var(--text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 12px;
}

.wizard-summary-row {
  display: flex;
  justify-content: space-between;
  font-size: 0.82rem;
  padding: 6px 0;
  border-bottom: 1px solid var(--border-dim);
}

.wizard-summary-row:last-child {
  border-bottom: none;
}

.wizard-summary-row span:first-child {
  color: var(--text-dim);
}

.wizard-summary-row span:last-child {
  color: var(--text);
  display: flex;
  align-items: center;
  gap: 6px;
}

.wizard-color-swatch {
  display: inline-block;
  width: 12px;
  height: 12px;
  border-radius: 3px;
  border: 1px solid var(--border);
}

.wizard-client-id {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 24px;
  font-size: 0.85rem;
}

.wizard-client-id-label {
  color: var(--text-dim);
}

.wizard-client-id code {
  color: var(--accent);
  background: var(--surface-raised);
  padding: 4px 10px;
  border-radius: 6px;
  border: 1px solid var(--border);
}

.wizard-framework-tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 16px;
}

.wizard-fw-tab {
  background: none;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 14px;
  color: var(--text-dim);
  font-size: 0.8rem;
  cursor: pointer;
  transition: color 0.15s, border-color 0.15s, background 0.15s;
}

.wizard-fw-tab:hover {
  border-color: var(--border-strong);
  color: var(--text);
}

.wizard-fw-tab-active {
  background: var(--surface-raised);
  border-color: var(--accent);
  color: var(--accent);
}

.wizard-code-block {
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 16px 20px;
  text-align: left;
  width: 100%;
  max-width: 600px;
  overflow-x: auto;
  font-size: 0.78rem;
  line-height: 1.6;
  margin-bottom: 24px;
}
```

- [ ] **Step 3: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 4: Commit**

```bash
git add admin/src/core/theme/components.css
git commit -m "feat(theme): add onboarding wizard CSS styles"
```

---

### Task 13: Frontend — overview cleanup

**Files:**
- Modify: `admin/src/platform/console/views/OverviewView.ts:69-170` (remove checklist + surfaces)

- [ ] **Step 1: Read OverviewView.ts to find exact sections to remove**

Read `admin/src/platform/console/views/OverviewView.ts` lines 60-180 to see the checklist and Tenant Surfaces sections with exact boundaries.

- [ ] **Step 2: Remove the setup checklist section**

Remove the entire checklist card block (lines 69-143 approximately). This includes:
- The "Recommended Next Steps" label
- All 4 checklist items (claim URL, preview login, create session, choose focus)
- The "Hide Checklist" button
- The `createShow` wrapper that gates on `onboarding_completed === false`

- [ ] **Step 3: Remove the Tenant Surfaces section**

Remove the Tenant Surfaces quick links card (lines 145-170 approximately). This includes:
- The "Tenant Surfaces" label
- Login Page, Protected Site, Auth Reference quick link items

Note: These links should eventually be moved to the Integrate tab, but that's a separate task. For now, just remove them from Overview.

- [ ] **Step 4: Add intent badge to overview header**

Find the overview header section (where tenant name and slug are displayed). Add after the slug display:

```ts
createShow(
  () => tenantState()?.tenant_intent != null,
  () => h('span', { class: 'badge badge-dim' },
    () => {
      const labels: Record<string, string> = {
        saas: 'SaaS App',
        internal: 'Internal Tool',
        community: 'Community Site',
        'client-portal': 'Client Portal',
        marketplace: 'Marketplace / API',
      };
      return labels[tenantState()?.tenant_intent || ''] || '';
    },
  ),
),
```

- [ ] **Step 5: Also remove the ChecklistItem helper function**

Find the `ChecklistItem` helper function (around line 460) and remove it, since nothing uses it anymore.

- [ ] **Step 6: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 7: Commit**

```bash
git add admin/src/platform/console/views/OverviewView.ts
git commit -m "feat(overview): remove checklist and surfaces sections, add intent badge"
```

---

### Task 14: Security tab — intent defaults note

**Files:**
- Modify: `admin/src/platform/console/views/SecurityConfigView.ts` (add note at top)

- [ ] **Step 1: Read SecurityConfigView.ts to find the render entry point**

Read the first 30 lines to find where the view content starts rendering.

- [ ] **Step 2: Add intent defaults note at the top of the view**

Add at the beginning of the view's return content, before the existing security toggles:

```ts
createShow(
  () => selectedTenant()?.tenant_intent != null,
  () => h('div', { class: 'security-intent-note' },
    () => {
      const labels: Record<string, string> = {
        saas: 'SaaS app',
        internal: 'internal tool',
        community: 'community site',
        'client-portal': 'client portal',
        marketplace: 'marketplace / API',
      };
      const intent = selectedTenant()?.tenant_intent || '';
      return 'Your security defaults are tuned for a ' + (labels[intent] || intent) + '.';
    },
    ' ',
    h('a', {
      href: '#',
      class: 'security-intent-link',
      onClick: (e: Event) => e.preventDefault(),
    }, 'Customize \u2192'),
  ),
),
```

- [ ] **Step 3: Add minimal CSS for the note**

In `components.css`, append:

```css
.security-intent-note {
  font-size: 0.82rem;
  color: var(--text-dim);
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px 16px;
  margin-bottom: 20px;
}

.security-intent-link {
  color: var(--accent);
  text-decoration: none;
}

.security-intent-link:hover {
  text-decoration: underline;
}
```

- [ ] **Step 4: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds

- [ ] **Step 5: Commit**

```bash
git add admin/src/platform/console/views/SecurityConfigView.ts admin/src/core/theme/components.css
git commit -m "feat(security): add intent defaults note at top of security config view"
```

---

### Task 15: Build verification and final integration test

**Files:** None (verification only)

- [ ] **Step 1: Verify Rust backend compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check -p gatewasm-server 2>&1 | tail -5`
Expected: compiles with no errors

- [ ] **Step 2: Verify frontend builds**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm run build 2>&1 | tail -5`
Expected: build succeeds (budget warnings are pre-existing and acceptable)

- [ ] **Step 3: Run existing e2e tests**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && npm test 2>&1 | tail -20`
Expected: existing tests pass. The `wizard-flow.spec.ts` test that asserts "removed wizard" (line 64) will need updating since the wizard is now back — update the assertion to expect the wizard to render for new tenants with `onboarding_completed === false`.

- [ ] **Step 4: Update e2e test for wizard presence**

In `admin/e2e/wizard-flow.spec.ts`, find the assertion that checks the wizard was removed (around line 64: `await expect(page.getByText('Make it yours')).toHaveCount(0)`). Update it to either:
- Remove the assertion (wizard exists now), or
- Change it to assert the wizard IS present for uncompleted tenants

Read the file to determine the exact change needed.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "test: update e2e tests for onboarding wizard"
```
