# Console Redesign Phase 3: Hosted Page System

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an auth-gated hosted page at `/t/{slug}/site` where end-users land after login. Includes a page editor in the console's Hosted Page tab, backed by a `tenant_hosted_pages` table. This completes the "Host it here" (Path A) onboarding path — tenants get a free, auth-gated landing page with no code required.

**Architecture:** Page content is stored as a JSON blob (`headline`, `body`, `cta_text`, `cta_link`). The Rust handler at `/t/{slug}/site` checks the Plane 3 session, fetches the JSON, and renders it server-side as a simple branded HTML page. The console's Hosted Page tab provides a visual editor that PATCHes the JSON via a self-serve endpoint. FormaJS directives support is a thin layer — users can toggle to a code view, but the visual editor is the default.

**Tech Stack:** FormaJS (h, createSignal, createShow, mount), Rust/Axum (handlers, sqlx), PostgreSQL

**Spec:** `docs/superpowers/specs/2026-03-31-console-redesign-design.md` — Phase 3 section

---

## File Map

### New Files (Frontend)

| File | Responsibility |
|------|----------------|
| `admin/src/platform/console-v2/views/HostedPageView.ts` | Page editor: headline, body, CTA, live preview |

### New Files (Backend)

| File | Responsibility |
|------|----------------|
| `crates/server/src/handlers/hosted_page.rs` | GET /t/{slug}/site handler (auth-gated, renders page) |
| `crates/server/src/db/pg_migrations/003_hosted_pages.sql` | tenant_hosted_pages table |

### Modified Files

| File | Change |
|------|--------|
| `crates/server/src/handlers/mod.rs` | Register `/t/{slug}/site` route + self-serve hosted-page endpoints |
| `crates/server/src/handlers/onboarding.rs` | Add self-serve hosted-page read/write endpoints |
| `admin/src/platform/console-v2/ConsoleApp.ts` | Add HostedPage tab to sidebar and switch |
| `admin/src/platform/console-v2/store.ts` | Add hosted page state + API functions |
| `admin/build.ts` | No change needed — site page is server-rendered, no new JS entry |

---

## Task 1: DB Migration — Hosted Pages Table

**Files:**
- Create: `crates/server/src/db/pg_migrations/003_hosted_pages.sql`

- [ ] **Step 1: Write the migration**

```sql
-- 003: Add hosted pages table
CREATE TABLE IF NOT EXISTS tenant_hosted_pages (
  tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
  headline TEXT NOT NULL DEFAULT 'Welcome',
  body TEXT NOT NULL DEFAULT '',
  cta_text TEXT NOT NULL DEFAULT '',
  cta_link TEXT NOT NULL DEFAULT '',
  custom_html TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

- [ ] **Step 2: Verify migration runs**

```bash
npm run dev
# Check server logs for migration success
```

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/db/pg_migrations/003_hosted_pages.sql
git commit -m "feat: add tenant_hosted_pages table for Phase 3"
```

---

## Task 2: Self-Serve Hosted Page Endpoints (Rust)

**Files:**
- Modify: `crates/server/src/handlers/onboarding.rs`
- Modify: `crates/server/src/handlers/mod.rs`

- [ ] **Step 1: Add `get_hosted_page` handler**

In `onboarding.rs`:

```rust
/// GET /platform/self-serve/tenants/{id}/hosted-page
pub async fn get_hosted_page(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    // Verify ownership
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    let row = sqlx::query(
        "SELECT headline, body, cta_text, cta_link, custom_html, updated_at \
         FROM tenant_hosted_pages WHERE tenant_id = $1",
    )
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some(r) => Ok(Json(serde_json::json!({
            "headline": r.get::<String, _>("headline"),
            "body": r.get::<String, _>("body"),
            "cta_text": r.get::<String, _>("cta_text"),
            "cta_link": r.get::<String, _>("cta_link"),
            "custom_html": r.get::<Option<String>, _>("custom_html"),
            "updated_at": r.get::<chrono::DateTime<Utc>, _>("updated_at").to_rfc3339(),
        }))),
        None => Ok(Json(serde_json::json!({
            "headline": "Welcome",
            "body": "",
            "cta_text": "",
            "cta_link": "",
            "custom_html": null,
            "updated_at": null,
        }))),
    }
}
```

- [ ] **Step 2: Add `update_hosted_page` handler**

```rust
#[derive(Debug, Deserialize)]
pub struct UpdateHostedPageRequest {
    pub headline: Option<String>,
    pub body: Option<String>,
    pub cta_text: Option<String>,
    pub cta_link: Option<String>,
    pub custom_html: Option<String>,
}

/// PUT /platform/self-serve/tenants/{id}/hosted-page
pub async fn update_hosted_page(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    let body: UpdateHostedPageRequest = axum::Json::<UpdateHostedPageRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest { error: "invalid_request", message: "Invalid request body".into() })?
        .0;

    sqlx::query(
        "INSERT INTO tenant_hosted_pages (tenant_id, headline, body, cta_text, cta_link, custom_html) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (tenant_id) DO UPDATE SET \
           headline = COALESCE($2, tenant_hosted_pages.headline), \
           body = COALESCE($3, tenant_hosted_pages.body), \
           cta_text = COALESCE($4, tenant_hosted_pages.cta_text), \
           cta_link = COALESCE($5, tenant_hosted_pages.cta_link), \
           custom_html = COALESCE($6, tenant_hosted_pages.custom_html), \
           updated_at = NOW()",
    )
    .bind(&tenant_id)
    .bind(&body.headline.unwrap_or_else(|| "Welcome".to_string()))
    .bind(&body.body.unwrap_or_default())
    .bind(&body.cta_text.unwrap_or_default())
    .bind(&body.cta_link.unwrap_or_default())
    .bind(&body.custom_html)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "status": "saved" })))
}
```

- [ ] **Step 3: Register routes**

In `mod.rs`, add to the self-serve section:

```rust
.route(
    "/platform/self-serve/tenants/{id}/hosted-page",
    get(onboarding::get_hosted_page).put(onboarding::update_hosted_page),
)
```

- [ ] **Step 4: Verify and commit**

```bash
cargo check
git add crates/server/src/handlers/onboarding.rs crates/server/src/handlers/mod.rs
git commit -m "feat: add self-serve hosted page read/write endpoints"
```

---

## Task 3: Hosted Page Route Handler (Rust)

**Files:**
- Create: `crates/server/src/handlers/hosted_page.rs`
- Modify: `crates/server/src/handlers/mod.rs`

The `/t/{slug}/site` route renders the hosted page for authenticated end-users.

- [ ] **Step 1: Create the handler**

```rust
use axum::{
    extract::{Extension, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;
use sqlx::Row;

use crate::handlers::auth::extract_refresh_cookie;
use crate::handlers::hosted_login::html_escape;
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// GET /t/{slug}/site — Auth-gated hosted page for end-users.
///
/// Unauthenticated → redirect to login with redirect param.
/// Authenticated → render the hosted page content from tenant_hosted_pages.
pub async fn site_page(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    req: axum::extract::Request,
) -> Response {
    let slug = &tenant.slug;

    // Auth check: valid refresh cookie for this tenant?
    let authenticated = match extract_refresh_cookie(req.headers(), Some(slug)) {
        Some(rt) => {
            let hash = signing::hash_token(&rt);
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM refresh_tokens WHERE token_hash = $1 AND revoked = FALSE AND expires_at > $2 AND tenant_id = $3)",
            )
            .bind(&hash)
            .bind(Utc::now())
            .bind(&tenant.tenant_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(false)
        }
        None => false,
    };

    if !authenticated {
        let login_url = format!("/t/{}/login?redirect=/t/{}/site", slug, slug);
        return Redirect::temporary(&login_url).into_response();
    }

    // Fetch hosted page content
    let page_data = sqlx::query(
        "SELECT headline, body, cta_text, cta_link, custom_html FROM tenant_hosted_pages WHERE tenant_id = $1",
    )
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let app_name = tenant.branding_app_name.as_deref().unwrap_or(&tenant.name);
    let primary_color = &tenant.branding_primary_color;

    let (headline, body_text, cta_text, cta_link) = match &page_data {
        Some(r) => (
            r.get::<String, _>("headline"),
            r.get::<String, _>("body"),
            r.get::<String, _>("cta_text"),
            r.get::<String, _>("cta_link"),
        ),
        None => ("Welcome".to_string(), String::new(), String::new(), String::new()),
    };

    // Render a simple branded HTML page — no JS framework needed
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:system-ui,-apple-system,sans-serif;background:#0a0a0c;color:#e8e6e3;min-height:100vh;display:flex;align-items:center;justify-content:center}}
.page{{max-width:640px;width:100%;padding:2rem;text-align:center}}
h1{{font-size:2rem;font-weight:500;margin-bottom:1rem;color:{color}}}
.body-text{{font-size:1rem;line-height:1.6;color:#a8a29e;margin-bottom:1.5rem}}
.cta{{display:inline-block;padding:0.65rem 1.5rem;background:{color};color:white;text-decoration:none;font-size:0.9rem;font-weight:500;border-radius:4px}}
.cta:hover{{opacity:0.9}}
.footer{{margin-top:3rem;font-size:0.72rem;color:#57534e}}
.footer a{{color:#78716c;text-decoration:none}}
.footer a:hover{{text-decoration:underline}}
</style>
</head>
<body>
<div class="page">
<h1>{headline}</h1>
{body_html}
{cta_html}
<div class="footer">Powered by <a href="https://getforma.dev">Forma Auth</a></div>
</div>
</body>
</html>"#,
        title = html_escape(app_name),
        color = html_escape(primary_color),
        headline = html_escape(&headline),
        body_html = if body_text.is_empty() { String::new() } else { format!("<p class=\"body-text\">{}</p>", html_escape(&body_text)) },
        cta_html = if cta_text.is_empty() { String::new() } else { format!("<a class=\"cta\" href=\"{}\">{}</a>", html_escape(&cta_link), html_escape(&cta_text)) },
    );

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Html(html),
    ).into_response()
}
```

- [ ] **Step 2: Register the module and route**

In `mod.rs`, add `pub mod hosted_page;` and register the route in the tenant_auth router:

```rust
.route("/site", get(hosted_page::site_page))
```

- [ ] **Step 3: Update the default `login_redirect_url` for new tenants**

In `onboarding.rs` `create_tenant`, change the default redirect from dashboard to site:

```rust
// Change:
let default_login_redirect = default_login_redirect(&state.config.server_url, &slug);
// To:
let default_login_redirect = format!("{}/t/{}/site", state.config.server_url.trim_end_matches('/'), slug);
```

This means new tenants send their end-users to `/t/{slug}/site` after login instead of dashboard.

- [ ] **Step 4: Verify and commit**

```bash
cargo check
git add crates/server/src/handlers/hosted_page.rs crates/server/src/handlers/mod.rs crates/server/src/handlers/onboarding.rs
git commit -m "feat: add /t/{slug}/site hosted page route with auth gate"
```

---

## Task 4: Hosted Page Tab in Console

**Files:**
- Create: `admin/src/platform/console-v2/views/HostedPageView.ts`
- Modify: `admin/src/platform/console-v2/store.ts`
- Modify: `admin/src/platform/console-v2/ConsoleApp.ts`

- [ ] **Step 1: Add hosted page state to store**

In `store.ts`, add types and state:

```typescript
export interface HostedPageData {
  headline: string;
  body: string;
  cta_text: string;
  cta_link: string;
  custom_html: string | null;
  updated_at: string | null;
}

export const [hostedPage, setHostedPage] = createSignal<HostedPageData | null>(null);

export async function loadHostedPage(tenantId: string): Promise<void> {
  try {
    const data = await api.get<HostedPageData>(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/hosted-page',
    );
    setHostedPage(data);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to load hosted page', 'error');
  }
}

export async function saveHostedPage(tenantId: string, data: Partial<HostedPageData>): Promise<void> {
  try {
    await api.put(
      '/platform/self-serve/tenants/' + encodeURIComponent(tenantId) + '/hosted-page',
      data,
    );
    toast.show('Page saved', 'success');
    await loadHostedPage(tenantId);
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to save page', 'error');
  }
}
```

Also add `api.put` to `createApi.ts` if it doesn't exist (same pattern as `api.patch`).

Update the view type to include `'hosted-page'`:

```typescript
export const [view, setView] = createSignal<'tenants' | 'wizard' | 'overview' | 'users' | 'hosted-page' | 'integrate' | 'settings'>('tenants');
```

Update `selectTenant` to handle `preferred_tab: 'hosted-page'`:

```typescript
if (tab === 'hosted-page') setView('hosted-page');
else if (tab === 'integrate') setView('integrate');
else setView('overview');
```

- [ ] **Step 2: Create HostedPageView**

```typescript
import { h, createSignal, createShow } from 'formajs';
import { Field } from '../../../core/components/Field';
import { selectedTenant, hostedPage, loadHostedPage, saveHostedPage, toast } from '../store';

export function HostedPageView() {
  const tenant = selectedTenant()!;
  const serverUrl: string = (window as any).__FORMA_SERVER_URL__ || window.location.origin;
  const siteUrl = serverUrl + '/t/' + tenant.slug + '/site';

  loadHostedPage(tenant.id);

  const [headline, setHeadline] = createSignal('');
  const [body, setBody] = createSignal('');
  const [ctaText, setCtaText] = createSignal('');
  const [ctaLink, setCtaLink] = createSignal('');
  const [saving, setSaving] = createSignal(false);
  const [initialized, setInitialized] = createSignal(false);

  // Sync form fields when data loads
  const syncFields = () => {
    const p = hostedPage();
    if (p && !initialized()) {
      setHeadline(p.headline || 'Welcome');
      setBody(p.body || '');
      setCtaText(p.cta_text || '');
      setCtaLink(p.cta_link || '');
      setInitialized(true);
    }
  };

  async function save() {
    setSaving(true);
    await saveHostedPage(tenant.id, {
      headline: headline(),
      body: body(),
      cta_text: ctaText(),
      cta_link: ctaLink(),
    });
    setSaving(false);
  }

  return h('div', { class: 'console-view' },
    // Trigger sync on render
    h('span', { style: { display: 'none' } }, syncFields),
    h('div', { class: 'view-header' },
      h('h2', { class: 'view-title' }, 'Hosted Page'),
      h('p', { class: 'view-subtitle text-muted' }, 'Your free, auth-gated landing page'),
    ),

    // Live URL
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Page URL'),
      h('div', { style: { display: 'flex', alignItems: 'center', gap: '0.5rem' } },
        h('code', { style: { flex: '1', fontSize: '0.82rem', color: 'var(--text)' } }, siteUrl),
        h('a', { href: siteUrl, target: '_blank', class: 'btn btn-sm btn-ghost' }, 'Preview'),
        h('button', {
          class: 'btn btn-sm btn-ghost',
          onClick: () => { navigator.clipboard.writeText(siteUrl); toast.show('Copied', 'success'); },
        }, 'Copy'),
      ),
    ),

    // Editor
    h('div', { class: 'console-card' },
      h('div', { class: 'card-label' }, 'Page Content'),
      Field({ label: 'Headline', name: 'headline', value: headline, onChange: setHeadline, placeholder: 'Welcome to My App' }),
      h('div', { class: 'field' },
        h('label', null, 'Body Text'),
        h('textarea', {
          value: body,
          onInput: (e: Event) => setBody((e.target as HTMLTextAreaElement).value),
          placeholder: 'Tell your visitors what this is about...',
          rows: 4,
          style: { width: '100%', resize: 'vertical' },
        }),
      ),
      Field({ label: 'CTA Button Text', name: 'cta-text', value: ctaText, onChange: setCtaText, placeholder: 'Get Started' }),
      Field({ label: 'CTA Button Link', name: 'cta-link', value: ctaLink, onChange: setCtaLink, placeholder: 'https://...' }),
      h('button', {
        class: 'btn btn-primary btn-sm',
        style: { marginTop: '0.75rem' },
        disabled: saving,
        onClick: save,
      }, () => saving() ? 'Saving...' : 'Save Page'),
    ),

    // Upsell (non-intrusive)
    h('p', { class: 'text-muted', style: { fontSize: '0.72rem', marginTop: '1rem', textAlign: 'center' } },
      'Need more pages or a full app? ',
      h('a', { href: 'https://getforma.dev', target: '_blank', class: 'btn-link' }, 'Check out Forma'),
    ),
  );
}
```

- [ ] **Step 3: Add HostedPage tab to ConsoleApp sidebar and switch**

In `ConsoleApp.ts`, add to the sidebar items (after 'integrate'):

```typescript
{ id: 'hosted-page', label: 'Hosted Page', icon: ICONS.hostedPage },
```

Add to the switch cases:

```typescript
{ match: 'hosted-page', render: () => HostedPageView() },
```

Add a Hosted Page icon to the ICONS object:

```typescript
hostedPage: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/></svg>',
```

- [ ] **Step 4: Build and verify**

```bash
npm run build
cargo check
```

- [ ] **Step 5: Test end-to-end**

1. Create a tenant, complete wizard (pick Path A "Host it here")
2. Console should open with Hosted Page tab active
3. Fill in headline, body, CTA → Save
4. Open `/t/{slug}/site` — see the rendered page
5. Verify auth gate: in incognito, `/t/{slug}/site` → redirects to login → after login → shows page

- [ ] **Step 6: Commit**

```bash
git add admin/src/platform/console-v2/views/HostedPageView.ts admin/src/platform/console-v2/store.ts admin/src/platform/console-v2/ConsoleApp.ts admin/src/core/hooks/createApi.ts
git commit -m "feat: add Hosted Page tab with editor and /t/{slug}/site route"
```

---

## Task 5: Update Default Redirect and Polish

**Files:**
- Modify: various files for polish

- [ ] **Step 1: Verify the redirect flow**

New tenants should now default to `/t/{slug}/site` after login (changed in Task 3 Step 3). Test:
1. Create a new tenant
2. Register on the tenant login page
3. Should land on `/t/{slug}/site` (the hosted page) instead of `/t/{slug}/dashboard`
4. If the hosted page has no content yet, it shows "Welcome" with the tenant's branding

- [ ] **Step 2: Test the sandbox redirect for owners**

The wizard's popup opens with `?redirect=/t/{slug}/sandbox`. After registration:
- Owner email matches → sandbox renders
- Different email → sandbox bounces to `/t/{slug}/site` (update `sandbox.rs` to redirect to `/site` instead of `/dashboard`)

Update `sandbox.rs` line 86-87:

```rust
// Change: let dashboard_url = format!("/t/{}/dashboard", slug);
// To:
let site_url = format!("/t/{}/site", slug);
return Redirect::temporary(&site_url).into_response();
```

- [ ] **Step 3: Build, verify, commit**

```bash
npm run build
cargo check
git add -A
git commit -m "feat: Phase 3 complete — hosted page system with editor and auth-gated serving"
```
