# Branding Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add background color, card color, and scoped custom CSS controls to the branding wizard step, update the login preview to be pixel-accurate, and wire the new fields through the backend.

**Architecture:** Three new nullable DB columns (`branding_bg_color`, `branding_card_color` already need migration; `branding_custom_css` already exists). A shared CSS sanitizer (TS client + Rust server) strips dangerous patterns. The LoginPreview component uses exact theme token values and applies custom CSS reactively. The hosted login renderer emits the new values into personality CSS.

**Tech Stack:** Rust/axum (server), FormaJS/TypeScript (frontend), PostgreSQL (DB), no new dependencies

---

### Task 1: Database Migration — Add branding_bg_color and branding_card_color

**Files:**
- Create: `crates/server/src/db/pg_migrations/003_branding_colors.sql`

- [ ] **Step 1: Create the migration file**

```sql
-- 003_branding_colors.sql
-- Adds background and card color columns for login page branding

ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_bg_color text;
ALTER TABLE tenants ADD COLUMN IF NOT EXISTS branding_card_color text;
```

- [ ] **Step 2: Verify the migration applies cleanly**

Run: `cargo run` (the server auto-applies pending migrations on startup)
Expected: Server starts without errors, new columns exist in tenants table.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/db/pg_migrations/003_branding_colors.sql
git commit -m "feat(db): add branding_bg_color and branding_card_color columns"
```

---

### Task 2: Rust — Add new fields to UpdateTenantRequest and PATCH handler

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs:234-252` (UpdateTenantRequest struct)
- Modify: `crates/server/src/handlers/onboarding.rs:893-901` (after branding_button_text update block)
- Modify: `crates/server/src/handlers/onboarding.rs:614-621` (get_my_tenant SELECT)
- Modify: `crates/server/src/handlers/onboarding.rs:652-680` (get_my_tenant JSON response)
- Modify: `crates/server/src/handlers/onboarding.rs:513-518` (list_my_tenants own SELECT)
- Modify: `crates/server/src/handlers/onboarding.rs:530-534` (list_my_tenants team SELECT)
- Modify: `crates/server/src/handlers/onboarding.rs:554-571` (row_to_json)

- [ ] **Step 1: Add fields to UpdateTenantRequest**

In `crates/server/src/handlers/onboarding.rs`, add three fields after `branding_button_text` (line 250):

```rust
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub allowed_providers: Option<Vec<String>>,
    pub branding_logo_url: Option<String>,
    pub branding_primary_color: Option<String>,
    pub branding_app_name: Option<String>,
    pub login_redirect_url: Option<String>,
    pub allowed_origins: Option<Vec<String>>,
    pub onboarding_completed: Option<bool>,
    pub onboarding_step: Option<i16>,
    pub preferred_tab: Option<String>,
    /// Alias: frontend sends `primary_color`, maps to `branding_primary_color`
    pub primary_color: Option<String>,
    pub tenant_intent: Option<String>,
    pub branding_dark_mode: Option<bool>,
    pub branding_button_text: Option<String>,
    pub signup_mode: Option<String>,
    pub branding_bg_color: Option<String>,
    pub branding_card_color: Option<String>,
    pub branding_custom_css: Option<String>,
}
```

- [ ] **Step 2: Add PATCH handler blocks for the three new fields**

In `update_my_tenant()`, after the `branding_button_text` block (line 901), add:

```rust
    if let Some(ref v) = body.branding_bg_color {
        let val = if v.is_empty() { None } else { Some(v.as_str()) };
        sqlx::query(
            "UPDATE tenants SET branding_bg_color = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(val)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(ref v) = body.branding_card_color {
        let val = if v.is_empty() { None } else { Some(v.as_str()) };
        sqlx::query(
            "UPDATE tenants SET branding_card_color = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(val)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(ref v) = body.branding_custom_css {
        let sanitized = sanitize_custom_css(v);
        let val = if sanitized.is_empty() { None } else { Some(sanitized) };
        sqlx::query(
            "UPDATE tenants SET branding_custom_css = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(val.as_deref())
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }
```

- [ ] **Step 3: Add the CSS sanitizer function**

Add this function near the top of `onboarding.rs` (after the imports):

```rust
/// Sanitize operator-provided CSS declarations.
/// Strips dangerous patterns while allowing safe styling properties.
/// Input is treated as declaration-only (no selectors) — the caller wraps in a scoped block.
fn sanitize_custom_css(input: &str) -> String {
    let truncated = if input.len() > 2000 { &input[..2000] } else { input };
    let mut result = truncated.to_string();
    // Strip dangerous patterns (case-insensitive)
    let lower = result.to_lowercase();
    let patterns = [
        "url(", "@import", "expression(", "javascript:", "-moz-binding",
        "behavior:", "</style", "<script", "<!--",
    ];
    for pat in &patterns {
        // Remove all occurrences regardless of case
        let mut pos = 0;
        let mut cleaned = String::with_capacity(result.len());
        let lower_bytes = lower.as_bytes();
        let pat_bytes = pat.as_bytes();
        while pos < lower_bytes.len() {
            if pos + pat_bytes.len() <= lower_bytes.len()
                && &lower_bytes[pos..pos + pat_bytes.len()] == pat_bytes
            {
                pos += pat_bytes.len();
            } else {
                cleaned.push(result.as_bytes()[pos] as char);
                pos += 1;
            }
        }
        result = cleaned;
    }
    // Strip HTML angle brackets and backslash escapes
    result.retain(|c| c != '<' && c != '>' && c != '\\');
    // Strip selector blocks: any { } pairs (declarations only, no selectors)
    result.retain(|c| c != '{' && c != '}');
    result.trim().to_string()
}
```

- [ ] **Step 4: Update get_my_tenant SELECT and response**

In `get_my_tenant()`, update the SELECT query (line 614-622) to include the new columns:

```rust
    let row = sqlx::query(
        "SELECT id, name, slug, owner_email, status, allowed_providers, allowed_origins, \
         max_users, COALESCE(plan, 'trial') AS plan, trial_ends_at, \
         branding_logo_url, branding_primary_color, branding_app_name, \
         branding_bg_color, branding_card_color, branding_custom_css, \
         login_redirect_url, \
         onboarding_completed, onboarding_step, preferred_tab, \
         tenant_intent, branding_dark_mode, branding_button_text, \
         created_at, updated_at \
         FROM tenants WHERE id = $1 AND status != 'deleted'",
    )
```

And in the JSON response (after `branding_button_text`, line 677), add:

```rust
        "branding_bg_color": row.get::<Option<String>, _>("branding_bg_color"),
        "branding_card_color": row.get::<Option<String>, _>("branding_card_color"),
        "branding_custom_css": row.get::<Option<String>, _>("branding_custom_css"),
```

- [ ] **Step 5: Update list_my_tenants SELECT queries and row_to_json**

In `list_my_tenants()`, update both SELECT queries (own_rows at line 513-519 and team_rows at line 529-534) to include the new columns. Add `branding_bg_color, branding_card_color, branding_custom_css` to the SELECT list.

For the own_rows query:
```rust
    let own_rows = sqlx::query(
        "SELECT id, name, slug, status, allowed_providers, max_users, \
         COALESCE(plan, 'trial') AS plan, trial_ends_at, created_at, \
         onboarding_completed, onboarding_step, preferred_tab, \
         branding_primary_color, branding_bg_color, branding_card_color, \
         branding_custom_css, login_redirect_url, \
         tenant_intent, branding_dark_mode, branding_button_text \
         FROM tenants WHERE owner_user_id = $1 AND status != 'deleted' \
         ORDER BY created_at DESC",
    )
```

For the team_rows query:
```rust
    let team_rows = sqlx::query(
        "SELECT t.id, t.name, t.slug, t.status, t.allowed_providers, t.max_users, \
         COALESCE(t.plan, 'trial') AS plan, t.trial_ends_at, t.created_at, \
         t.onboarding_completed, t.onboarding_step, t.preferred_tab, \
         t.branding_primary_color, t.branding_bg_color, t.branding_card_color, \
         t.branding_custom_css, t.login_redirect_url, \
         t.tenant_intent, t.branding_dark_mode, t.branding_button_text, pmt.role AS team_role \
         FROM platform_members pm \
         JOIN platform_member_tenants pmt ON pmt.member_id = pm.id \
         JOIN tenants t ON t.id = pmt.tenant_id \
         WHERE pm.user_id = $1 \
           AND pm.account_owner_user_id != $1 \
           AND pm.status = 'active' \
           AND t.status != 'deleted' \
         ORDER BY t.created_at DESC",
    )
```

In `row_to_json()`, add the new fields to the JSON (after `branding_button_text`, line 571):

```rust
            "branding_bg_color": row.get::<Option<String>, _>("branding_bg_color"),
            "branding_card_color": row.get::<Option<String>, _>("branding_card_color"),
            "branding_custom_css": row.get::<Option<String>, _>("branding_custom_css"),
```

- [ ] **Step 6: Verify build**

Run: `cargo build`
Expected: Compiles without errors.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/handlers/onboarding.rs
git commit -m "feat(api): add branding_bg_color, branding_card_color, branding_custom_css to self-serve PATCH"
```

---

### Task 3: Rust — Update hosted_login.rs personality CSS

**Files:**
- Modify: `crates/server/src/handlers/hosted_login.rs:71-90` (login_page_render)
- Modify: `crates/server/src/handlers/hosted_login.rs:164-167` (personality_css)
- Modify: `crates/server/src/middleware/tenant.rs:23-47` (TenantContext struct)
- Modify: `crates/server/src/middleware/tenant.rs:70-98` (from_db_row)

- [ ] **Step 1: Add branding_bg_color and branding_card_color to TenantContext**

In `crates/server/src/middleware/tenant.rs`, add two fields after `branding_custom_css` (line 43):

```rust
    pub branding_custom_css: Option<String>,
    pub branding_bg_color: Option<String>,
    pub branding_card_color: Option<String>,
```

In `from_db_row()`, add the mapping (after `branding_custom_css`, line 94):

```rust
            branding_custom_css: row.get("branding_custom_css"),
            branding_bg_color: row.get("branding_bg_color"),
            branding_card_color: row.get("branding_card_color"),
```

- [ ] **Step 2: Update personality CSS in hosted_login.rs**

Replace the personality_css block (lines 164-167) with:

```rust
    let bg_color = tenant.branding_bg_color.as_deref().unwrap_or("#0a0a0c");
    let mut personality_css = format!(
        "body{{background:{bg};color:#e8e6e3;--accent:{pc};}}",
        bg = bg_color,
        pc = primary_color,
    );
    if let Some(ref card_color) = tenant.branding_card_color {
        personality_css.push_str(&format!(".tl-card{{background:{};}}",card_color));
    }
    if let Some(ref custom_css) = tenant.branding_custom_css {
        if !custom_css.is_empty() {
            personality_css.push_str(&format!(".tl-card{{{}}}", custom_css));
        }
    }
```

- [ ] **Step 3: Verify build**

Run: `cargo build`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/hosted_login.rs crates/server/src/middleware/tenant.rs
git commit -m "feat(login): render branding bg_color, card_color, custom_css in personality CSS"
```

---

### Task 4: TypeScript — Update Tenant type and updateTenant fields

**Files:**
- Modify: `admin/src/platform/console/store.ts:20-43` (Tenant interface)
- Modify: `admin/src/platform/console/store.ts:695` (updateTenant fields Pick)

- [ ] **Step 1: Add new fields to Tenant interface**

In `admin/src/platform/console/store.ts`, add after `branding_primary_color` (line 35):

```typescript
  branding_bg_color?: string | null;
  branding_card_color?: string | null;
  branding_custom_css?: string | null;
```

- [ ] **Step 2: Add new fields to updateTenant's Pick type**

In `updateTenant()`, update the `fields` parameter type to include the new fields. The current Pick list (line 695) becomes:

```typescript
  fields: Partial<Pick<Tenant, 'name' | 'slug' | 'primary_color' | 'login_redirect_url' | 'onboarding_completed' | 'onboarding_step' | 'preferred_tab' | 'tenant_intent' | 'branding_dark_mode' | 'branding_button_text' | 'branding_app_name' | 'branding_logo_url' | 'branding_primary_color' | 'branding_bg_color' | 'branding_card_color' | 'branding_custom_css' | 'allowed_providers' | 'signup_mode'>>,
```

- [ ] **Step 3: Verify type-check**

Run: `cd admin && npx tsc --noEmit 2>&1 | grep -v OverviewView`
Expected: No new errors.

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat(store): add branding_bg_color, branding_card_color, branding_custom_css to Tenant type"
```

---

### Task 5: Client-side CSS sanitizer

**Files:**
- Create: `admin/src/platform/console/sanitizeCSS.ts`

- [ ] **Step 1: Create the sanitizer module**

```typescript
/**
 * Sanitize operator-provided CSS declarations.
 * Strips dangerous patterns; input is declarations-only (no selectors).
 * Mirrors the server-side sanitizer in onboarding.rs.
 */

const BLOCKED_PATTERNS = [
  'url(', '@import', 'expression(', 'javascript:', '-moz-binding',
  'behavior:', '</style', '<script', '<!--',
];

export function sanitizeCSS(input: string): string {
  let result = input.slice(0, 2000);

  // Strip blocked patterns (case-insensitive)
  for (const pat of BLOCKED_PATTERNS) {
    const regex = new RegExp(pat.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
    result = result.replace(regex, '');
  }

  // Strip angle brackets, backslash escapes, and selector braces
  result = result.replace(/[<>\\{}]/g, '');

  return result.trim();
}
```

- [ ] **Step 2: Verify type-check**

Run: `cd admin && npx tsc --noEmit 2>&1 | grep -v OverviewView`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add admin/src/platform/console/sanitizeCSS.ts
git commit -m "feat: add client-side CSS sanitizer for branding custom CSS"
```

---

### Task 6: Update LoginPreview — pixel-accurate tokens and new props

**Files:**
- Modify: `admin/src/platform/console/LoginPreview.ts`
- Modify: `admin/src/core/theme/components.css` (`.lp-*` classes)

- [ ] **Step 1: Update LoginPreviewProps interface and rootVars**

Rewrite `admin/src/platform/console/LoginPreview.ts`:

```typescript
/**
 * LoginPreview — native reactive mock of the tenant login page
 * wrapped in a mini browser chrome for a polished preview feel.
 */
import { h, createShow } from 'formajs';
import { GoogleIcon, GitHubIcon, EyeIcon } from '../../core/components/ui/BrandIcons';
import { sanitizeCSS } from './sanitizeCSS';

interface LoginPreviewProps {
  appName: () => string;
  logoUrl: () => string;
  primaryColor: () => string;
  darkMode: () => boolean;
  buttonText: () => string;
  providers: () => string[];
  slug?: () => string;
  bgColor?: () => string;
  cardColor?: () => string;
  customCSS?: () => string;
}

// Token-accurate defaults from tokens.css
const DARK_TOKENS = {
  bg: '#0a0a0c', surface: '#111114', surfaceRaised: '#16161a',
  border: '#2a2a33', text: '#e8e6e3', textDim: '#6b6a72', textMuted: '#52525b',
};
const LIGHT_TOKENS = {
  bg: '#f8fafc', surface: '#ffffff', surfaceRaised: '#f1f5f9',
  border: '#cbd5e1', text: '#0f172a', textDim: '#475569', textMuted: '#94a3b8',
};

export function LoginPreview(props: LoginPreviewProps) {
  const hasGoogle = () => props.providers().includes('google');
  const hasGithub = () => props.providers().includes('github');
  const hasOAuth = () => hasGoogle() || hasGithub();
  const hasEmail = () => props.providers().includes('email');

  const tokens = () => props.darkMode() ? DARK_TOKENS : LIGHT_TOKENS;

  const rootVars = () => {
    const t = tokens();
    return {
      '--lp-accent': props.primaryColor(),
      '--lp-bg': props.bgColor ? (props.bgColor() || t.bg) : t.bg,
      '--lp-card-bg': props.cardColor ? (props.cardColor() || t.surface) : t.surface,
      '--lp-card-border': t.border,
      '--lp-text': t.text,
      '--lp-text-dim': t.textDim,
      '--lp-text-muted': t.textMuted,
      '--lp-input-bg': t.surface,
      '--lp-input-border': t.border,
      '--lp-oauth-bg': t.surfaceRaised,
      '--lp-btn-text': props.darkMode() ? t.bg : '#ffffff',
    };
  };

  const slugText = props.slug ? props.slug : () => 'your-app';

  // Sanitize and inject custom CSS into the card
  const customStyle = () => {
    if (!props.customCSS) return '';
    const raw = props.customCSS();
    if (!raw) return '';
    return sanitizeCSS(raw);
  };

  return h('div', { class: 'lp-browser', style: rootVars },
    // Browser chrome
    h('div', { class: 'lp-chrome' },
      h('div', { class: 'lp-chrome-dots' },
        h('span', { class: 'lp-dot lp-dot-red' }),
        h('span', { class: 'lp-dot lp-dot-yellow' }),
        h('span', { class: 'lp-dot lp-dot-green' }),
      ),
      h('div', { class: 'lp-chrome-bar' },
        h('span', { class: 'lp-chrome-lock' }, '\uD83D\uDD12'),
        h('span', null, () => 'yoursite.com/t/' + slugText() + '/login'),
      ),
    ),

    // Page body
    h('div', { class: 'lp-page' },
      h('div', { class: 'lp-card', style: () => customStyle() },
        // Logo
        h('div', { class: 'lp-logo-area' },
          createShow(
            () => props.logoUrl() !== '',
            () => h('img', {
              src: () => props.logoUrl(),
              alt: 'Logo',
              class: 'lp-logo',
              onError: (e: Event) => (e.target as HTMLElement).classList.add('lp-logo-broken'),
              onLoad: (e: Event) => (e.target as HTMLElement).classList.remove('lp-logo-broken'),
            }),
          ),
          h('div', { class: 'lp-app-name' }, () => props.appName() || 'Your App'),
        ),

        h('div', { class: 'lp-subtitle' }, 'Sign in to your account'),

        // OAuth
        createShow(hasOAuth, () =>
          h('div', { class: 'lp-oauth-buttons' },
            createShow(hasGoogle, () =>
              h('div', { class: 'lp-btn lp-btn-oauth' }, GoogleIcon(), 'Continue with Google'),
            ),
            createShow(hasGithub, () =>
              h('div', { class: 'lp-btn lp-btn-oauth' }, GitHubIcon(), 'Continue with GitHub'),
            ),
          ),
        ),

        // Divider
        createShow(() => hasOAuth() && hasEmail(), () =>
          h('div', { class: 'lp-divider' },
            h('span', { class: 'lp-divider-line' }),
            h('span', null, 'OR'),
            h('span', { class: 'lp-divider-line' }),
          ),
        ),

        // Email/password
        createShow(hasEmail, () =>
          h('div', { class: 'lp-form' },
            h('div', { class: 'lp-field' },
              h('label', null, 'EMAIL'),
              h('div', { class: 'lp-input' }, 'you@example.com'),
            ),
            h('div', { class: 'lp-field' },
              h('label', null, 'PASSWORD'),
              h('div', { class: 'lp-input lp-input-password' },
                h('span', null, '\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022'),
                h('span', { class: 'lp-eye' }, EyeIcon()),
              ),
            ),
            h('div', { class: 'lp-forgot' },
              h('span', null, 'Forgot password?'),
            ),
            h('label', { class: 'lp-remember' },
              h('input', { type: 'checkbox', disabled: true }),
              'Remember email',
            ),
            h('div', {
              class: 'lp-btn lp-btn-primary',
              style: () => ({
                background: props.primaryColor(),
                borderColor: props.primaryColor(),
              }),
            }, () => props.buttonText() || 'Sign in'),
          ),
        ),

        // Sign up toggle
        h('div', { class: 'lp-toggle' }, "Don\u2019t have an account? ",
          h('span', { class: 'lp-toggle-link' }, 'Sign up'),
        ),

        h('div', { class: 'lp-secured' },
          'Secured by ',
          h('span', { class: 'lp-secured-link' }, 'fAuth'),
        ),
      ),
    ),
  );
}
```

- [ ] **Step 2: Update CSS for pixel-accurate proportions**

In `admin/src/core/theme/components.css`, update the `.lp-page` and `.lp-card` classes:

```css
.lp-page {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 32px 20px 36px;
  background: var(--lp-bg);
  transition: background 0.2s;
  min-height: 460px;
}

.lp-card {
  width: 100%;
  max-width: 400px;
  border: 1px solid var(--lp-card-border);
  padding: 2rem 1.75rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  background: var(--lp-card-bg);
  box-shadow: 0 4px 12px rgba(0,0,0,0.3);
  transition: background 0.2s, border-color 0.2s;
}
```

Add the sign-up toggle and link styles after `.lp-secured-link`:

```css
.lp-toggle {
  text-align: center;
  font-size: 0.65rem;
  color: var(--lp-text-dim);
  margin-top: 0.5rem;
  padding-top: 0.5rem;
  border-top: 1px solid var(--lp-card-border);
  font-weight: 300;
}

.lp-toggle-link {
  color: var(--lp-accent);
  cursor: default;
}
```

Update `.lp-app-name` font-size to match the real page:

```css
.lp-app-name {
  font-family: 'Instrument Serif', Georgia, serif;
  font-size: 1.35rem;
  font-weight: 400;
  letter-spacing: -0.02em;
  color: var(--lp-text);
}
```

Update `.lp-subtitle` font-size:

```css
.lp-subtitle {
  font-size: 0.78rem;
  text-align: center;
  margin-bottom: 0.5rem;
  font-weight: 300;
  color: var(--lp-text-dim);
}
```

Update `.lp-btn` padding and font-size to match real page:

```css
.lp-btn {
  width: 100%;
  border: 1px solid var(--lp-card-border);
  padding: 0.55rem 0.8rem;
  font-size: 0.8rem;
  font-family: inherit;
  font-weight: 400;
  letter-spacing: 0.04em;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  cursor: default;
  transition: background 0.2s, border-color 0.2s, color 0.2s;
}

.lp-btn svg { width: 18px; height: 18px; }
```

Update `.lp-field label`, `.lp-input`, `.lp-forgot`, `.lp-remember` font sizes to match the real page:

```css
.lp-field label {
  display: block;
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  margin-bottom: 0.35rem;
  font-weight: 400;
  color: var(--lp-text-dim);
}

.lp-input {
  width: 100%;
  border: 1px solid var(--lp-input-border);
  background: var(--lp-input-bg);
  color: var(--lp-text-muted);
  padding: 0.55rem 0.8rem;
  font-size: 0.8rem;
  font-family: inherit;
  box-sizing: border-box;
  transition: background 0.2s, border-color 0.2s, color 0.2s;
}

.lp-forgot {
  text-align: right;
  margin-top: -0.25rem;
  margin-bottom: 0.25rem;
  font-size: 0.72rem;
  cursor: default;
  color: var(--lp-accent);
}

.lp-remember {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.75rem;
  margin-bottom: 0.25rem;
  cursor: default;
  font-weight: 300;
  color: var(--lp-text-dim);
}
```

- [ ] **Step 3: Verify type-check and build**

Run: `cd admin && npx tsc --noEmit 2>&1 | grep -v OverviewView && npm run build 2>&1 | grep "Manifest written"`
Expected: Clean compile and build.

- [ ] **Step 4: Commit**

```bash
git add admin/src/platform/console/LoginPreview.ts admin/src/core/theme/components.css
git commit -m "feat(preview): pixel-accurate login preview with bg/card/css support"
```

---

### Task 7: Update OnboardingWizard — new controls in StepBranding

**Files:**
- Modify: `admin/src/platform/console/OnboardingWizard.ts` (StepBranding function)

- [ ] **Step 1: Add new signals and controls to StepBranding**

In `StepBranding()`, add new signals after the existing ones (after `buttonText` signal):

```typescript
  const [bgColor, setBgColor] = createSignal(tenant.branding_bg_color || '');
  const [cardColor, setCardColor] = createSignal(tenant.branding_card_color || '');
  const [customCSS, setCustomCSS] = createSignal(tenant.branding_custom_css || '');
  const [cssExpanded, setCssExpanded] = createSignal(false);
```

Add default color helpers:

```typescript
  const defaultBg = () => darkMode() ? '#0a0a0c' : '#f8fafc';
  const defaultCard = () => darkMode() ? '#111114' : '#ffffff';
```

- [ ] **Step 2: Update handleContinue to save new fields**

Update the `updateTenant` call in `handleContinue` to include the new fields:

```typescript
  async function handleContinue(): Promise<void> {
    setSaving(true);
    await updateTenant(tenant.id, {
      branding_app_name: appName(),
      branding_logo_url: logoUrl() || null,
      branding_primary_color: primaryColor(),
      branding_dark_mode: darkMode(),
      branding_button_text: buttonText(),
      branding_bg_color: bgColor() || null,
      branding_card_color: cardColor() || null,
      branding_custom_css: customCSS() || null,
    });
    setSaving(false);
    advanceStep();
  }
```

- [ ] **Step 3: Add background color picker after primary color**

After the primary color field, add:

```typescript
      h('label', { class: 'wizard-field' },
        h('span', null, 'Page background'),
        h('div', { class: 'wizard-color-row' },
          h('input', {
            type: 'color',
            value: () => bgColor() || defaultBg(),
            onInput: (e: Event) => setBgColor((e.target as HTMLInputElement).value),
          }),
          h('input', {
            type: 'text',
            class: 'input input-sm',
            placeholder: () => defaultBg(),
            value: () => bgColor(),
            onInput: (e: Event) => setBgColor((e.target as HTMLInputElement).value),
          }),
          createShow(() => bgColor() !== '', () =>
            h('button', {
              type: 'button',
              class: 'btn btn-ghost btn-sm',
              onClick: () => setBgColor(''),
            }, 'Reset'),
          ),
        ),
      ),
```

- [ ] **Step 4: Add card background color picker**

After the background color field:

```typescript
      h('label', { class: 'wizard-field' },
        h('span', null, 'Card background'),
        h('div', { class: 'wizard-color-row' },
          h('input', {
            type: 'color',
            value: () => cardColor() || defaultCard(),
            onInput: (e: Event) => setCardColor((e.target as HTMLInputElement).value),
          }),
          h('input', {
            type: 'text',
            class: 'input input-sm',
            placeholder: () => defaultCard(),
            value: () => cardColor(),
            onInput: (e: Event) => setCardColor((e.target as HTMLInputElement).value),
          }),
          createShow(() => cardColor() !== '', () =>
            h('button', {
              type: 'button',
              class: 'btn btn-ghost btn-sm',
              onClick: () => setCardColor(''),
            }, 'Reset'),
          ),
        ),
      ),
```

- [ ] **Step 5: Add Custom CSS collapsible section**

Before the "Remove fAuth branding" field, add:

```typescript
      h('div', { class: 'wizard-field' },
        h('button', {
          type: 'button',
          class: 'wizard-css-toggle',
          onClick: () => setCssExpanded((v: boolean) => !v),
        },
          h('span', null, 'Custom CSS'),
          h('span', { class: 'wizard-css-arrow' }, () => cssExpanded() ? '\u25B2' : '\u25BC'),
        ),
        createShow(cssExpanded, () =>
          h('div', { class: 'wizard-css-editor' },
            h('textarea', {
              class: 'wizard-css-textarea',
              rows: 8,
              placeholder: "font-family: 'Inter', sans-serif;\nborder-radius: 12px;",
              value: () => customCSS(),
              onInput: (e: Event) => setCustomCSS((e.target as HTMLTextAreaElement).value),
            }),
            h('div', { class: 'wizard-css-hint' },
              'Scoped to the login card. Properties like font-family, color, border-radius work. No url(), @import, or expression().'),
            createShow(() => customCSS().length > 0, () =>
              h('div', { class: 'wizard-css-count' }, () => customCSS().length + ' / 2000'),
            ),
          ),
        ),
      ),
```

- [ ] **Step 6: Pass new props to LoginPreview**

Update the LoginPreview call in StepBranding:

```typescript
      LoginPreview({
        appName,
        logoUrl,
        primaryColor,
        darkMode,
        buttonText,
        providers: previewProviders,
        slug: () => tenant.slug,
        bgColor,
        cardColor,
        customCSS,
      }),
```

- [ ] **Step 7: Add CSS for the custom CSS editor**

In `admin/src/core/theme/components.css`, add after the `.logo-box-clear` rule:

```css
/* ── Custom CSS editor ───────────────────────────────────────────── */

.wizard-css-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  background: none;
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 12px;
  color: var(--text-dim);
  font-size: 0.78rem;
  cursor: pointer;
  transition: border-color 0.15s;
}

.wizard-css-toggle:hover {
  border-color: var(--border-strong);
}

.wizard-css-arrow {
  font-size: 0.6rem;
  color: var(--text-muted);
}

.wizard-css-editor {
  margin-top: 8px;
}

.wizard-css-textarea {
  width: 100%;
  background: #0a0a0c;
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  color: var(--text);
  font-family: 'DM Mono', monospace;
  font-size: 0.75rem;
  line-height: 1.6;
  resize: vertical;
  box-sizing: border-box;
}

.wizard-css-textarea::placeholder {
  color: var(--text-muted);
}

.wizard-css-hint {
  font-size: 0.68rem;
  color: var(--text-muted);
  margin-top: 6px;
  line-height: 1.4;
}

.wizard-css-count {
  font-size: 0.65rem;
  color: var(--text-muted);
  text-align: right;
  margin-top: 4px;
  font-family: 'DM Mono', monospace;
}
```

- [ ] **Step 8: Verify type-check and build**

Run: `cd admin && npx tsc --noEmit 2>&1 | grep -v OverviewView && npm run build 2>&1 | grep "Manifest written"`
Expected: Clean compile and build.

- [ ] **Step 9: Commit**

```bash
git add admin/src/platform/console/OnboardingWizard.ts admin/src/core/theme/components.css
git commit -m "feat(wizard): add bg color, card color, and custom CSS controls to branding step"
```

---

### Task 8: Smoke test the full flow

**Files:** None (manual verification)

- [ ] **Step 1: Rebuild server and frontend**

Run: `cargo build && cd admin && npm run build`

- [ ] **Step 2: Start the server and test the wizard flow**

1. Navigate to the platform console, create a new workspace
2. Reach the Branding step
3. Verify: changing app name updates the preview instantly
4. Verify: pasting a logo URL shows the image in both the logo box and the preview
5. Verify: changing primary color updates the button and accent color in preview
6. Verify: changing background color updates the preview page background
7. Verify: changing card color updates the preview card background
8. Verify: toggling dark mode flips all token-derived colors in the preview
9. Verify: expanding Custom CSS and typing `border-radius: 16px; font-size: 1.1rem;` applies to the preview card
10. Verify: typing `url(evil)` in the CSS editor — it gets stripped, nothing shows
11. Click Continue, then Skip Setup to land on the dashboard
12. Open the actual login page `/t/{slug}/login` — verify the personality CSS matches what the preview showed

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "fix: smoke test corrections for branding polish"
```
