# Email, Verification & Invite UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Polish the invite acceptance flow, rebrand all email templates to dark theme with gold accents, make email verification non-blocking with a console banner, and add shared workspace role badges.

**Architecture:** Backend gets one new public endpoint (invite resolve), one modified endpoint (verify-email returns HTML redirect), and rebranded email templates. Frontend adds invite-aware login page, verification banner, and workspace role badges. No schema changes.

**Tech Stack:** Rust/Axum, sqlx (Postgres), Resend, FormaJS (SolidJS-like with `h()`, `createSignal`, `createShow`)

---

### Task 1: Rebrand Email Templates (Dark Theme + Gold)

**Files:**
- Modify: `crates/server/src/email.rs:180-237` (wrapper + button)
- Modify: `crates/server/src/email.rs:243-413` (all content builders)

- [ ] **Step 1: Rewrite `email_html_wrapper()` with dark theme**

Replace the function at lines 180-222 with:

```rust
fn email_html_wrapper(inner: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<title>fAuth</title>
<!--[if mso]>
<style>body,table,td{{font-family:Arial,sans-serif !important;}}</style>
<![endif]-->
</head>
<body style="margin:0;padding:0;background-color:#0a0a0c;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Helvetica Neue',Arial,sans-serif;-webkit-font-smoothing:antialiased;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color:#0a0a0c;">
  <tr>
    <td align="center" style="padding:40px 16px;">
      <table role="presentation" width="600" cellpadding="0" cellspacing="0" style="max-width:600px;width:100%;background-color:#111114;border-radius:8px;overflow:hidden;border:1px solid #1e1e24;">
        <!-- Header -->
        <tr>
          <td style="padding:32px 40px 0 40px;background-color:#111114;">
            <p style="margin:0 0 4px;font-size:14px;font-weight:700;color:#c4a265;letter-spacing:0.5px;">fAuth</p>
          </td>
        </tr>
        <!-- Body -->
        <tr>
          <td style="padding:24px 40px 40px 40px;font-size:16px;line-height:1.6;color:#e8e6e3;background-color:#111114;">
            {inner}
          </td>
        </tr>
        <!-- Footer -->
        <tr>
          <td style="padding:20px 40px;border-top:1px solid #1e1e24;font-size:12px;color:#6b6a72;text-align:center;background-color:#111114;">
            &copy; {year} Forma &middot; Secure authentication infrastructure
          </td>
        </tr>
      </table>
    </td>
  </tr>
</table>
</body>
</html>"#,
        inner = inner,
        year = chrono::Utc::now().format("%Y"),
    )
}
```

- [ ] **Step 2: Rewrite `email_button()` with gold CTA**

Replace the function at lines 224-237 with:

```rust
fn email_button(href: &str, label: &str) -> String {
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:24px 0;">
  <tr>
    <td align="center" style="background-color:#c4a265;border-radius:6px;">
      <a href="{href}" target="_blank" style="display:inline-block;padding:14px 32px;font-size:16px;font-weight:600;color:#0a0a0c;text-decoration:none;border-radius:6px;">{label}</a>
    </td>
  </tr>
</table>"#,
        href = href,
        label = label,
    )
}
```

- [ ] **Step 3: Update inline text colors in all content builders**

In `verification_email_content()`, `password_reset_email_content()`, `tenant_invite_email_content()`, and `platform_team_invite_email_content()`, replace all inline color references:

| Find | Replace |
|------|---------|
| `color:#111827` | `color:#e8e6e3` |
| `color:#1f2937` | `color:#e8e6e3` |
| `color:#374151` | `color:#e8e6e3` |
| `color:#6b7280` | `color:#6b6a72` |
| `color:#4f46e5` | `color:#c4a265` |
| `style="color:#4f46e5;"` | `style="color:#c4a265;"` |

- [ ] **Step 4: Verify cargo check**

Run: `cargo check -p gatewasm-server`
Expected: clean build

---

### Task 2: Invite Resolve Endpoint

**Files:**
- Modify: `crates/server/src/handlers/owner/invites.rs` (add `resolve_invite` function)
- Modify: `crates/server/src/handlers/mod.rs` (add route)

- [ ] **Step 1: Add `resolve_invite` handler**

Add to `invites.rs` before the `accept_owner_invite` function:

```rust
/// GET /platform/admin/invites/resolve?token={token}
/// Public endpoint — no auth required. Resolves invite details for the login page.
pub async fn resolve_invite(
    State(state): State<AppState>,
    Query(params): Query<ResolveInviteQuery>,
) -> Result<impl IntoResponse, AppError> {
    let token = params.token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest {
            error: "missing_token",
            message: "Invite token is required".into(),
        });
    }

    let token_hash = signing::hash_token(token);

    let invite = sqlx::query_as::<_, (String, String, String, Vec<String>, Option<String>)>(
        "SELECT oi.email, oi.role, oi.status, oi.tenant_ids, oi.account_owner_user_id
         FROM owner_invites oi
         WHERE oi.token_hash = $1
         LIMIT 1",
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "invite_not_found",
        message: "Invite not found".into(),
    })?;

    let (invite_email, role, status, tenant_ids, account_owner_user_id) = invite;

    if status != "pending" {
        return Err(AppError::BadRequest {
            error: "invite_not_pending",
            message: format!("This invite has been {}", status),
        });
    }

    // Verify the account owner is still active
    if let Some(ref owner_id) = account_owner_user_id {
        let owner_active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM platform_members WHERE account_owner_user_id = $1 AND user_id = $1 AND status = 'active')",
        )
        .bind(owner_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

        if !owner_active {
            return Err(AppError::BadRequest {
                error: "account_inactive",
                message: "The inviting account is no longer active".into(),
            });
        }
    }

    // Check if a user already exists for this email
    let user_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE lower(email) = lower($1) AND tenant_id = 'ten_platform')",
    )
    .bind(&invite_email)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    // Get inviter email
    let inviter_email: Option<String> = if let Some(ref owner_id) = account_owner_user_id {
        sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
            .bind(owner_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    // Resolve tenant names
    let tenant_names: Vec<String> = if tenant_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, (String,)>("SELECT name FROM tenants WHERE id = ANY($1)")
            .bind(&tenant_ids)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(n,)| n)
            .collect()
    };

    Ok(Json(serde_json::json!({
        "invite_email": invite_email,
        "inviter_email": inviter_email,
        "role": role,
        "tenant_names": tenant_names,
        "user_exists": user_exists,
        "status": status,
    })))
}
```

Also add the query struct near the top of invites.rs:

```rust
#[derive(Debug, Deserialize)]
pub struct ResolveInviteQuery {
    pub token: String,
}
```

- [ ] **Step 2: Register the route (public, no auth)**

In `handlers/mod.rs`, add a new public route near the `owner_invite_accept_routes` section. Find the invite accept routes and add the resolve route alongside:

```rust
// ── Public invite resolution (no auth) ───────────────────────────
let invite_resolve_routes = Router::new()
    .route(
        "/platform/admin/invites/resolve",
        get(owner::resolve_invite),
    );
```

Then merge it into the app alongside the other public routes (near the health check or tenant routes that don't require auth).

- [ ] **Step 3: Verify cargo check**

Run: `cargo check -p gatewasm-server`
Expected: clean build

---

### Task 3: Verify-Email Returns HTML Redirect

**Files:**
- Modify: `crates/server/src/handlers/auth.rs:1793-1838` (verify_email handler)

- [ ] **Step 1: Update verify_email to return HTML redirect**

Replace the handler's return at line 1837 (`Ok(Json(MessageResponse { message: "Email verified successfully" }))`) with an HTML redirect page. The full updated handler:

After the webhook dispatch (line 1835), replace the return with:

```rust
    // Determine redirect URL based on tenant
    let redirect_url = if tenant_id.as_deref() == Some("ten_platform") {
        "/platform/console?verified=true".to_string()
    } else {
        format!("/platform/login?verified=true")
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="2;url={redirect_url}">
<title>Email Verified — fAuth</title>
<style>
  body {{ margin:0; background:#0a0a0c; color:#e8e6e3; font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif; display:flex; align-items:center; justify-content:center; min-height:100vh; }}
  .card {{ background:#111114; border:1px solid #1e1e24; border-radius:12px; padding:48px; text-align:center; max-width:400px; }}
  .check {{ font-size:48px; margin-bottom:16px; }}
  h1 {{ font-size:20px; margin:0 0 8px; color:#e8e6e3; }}
  p {{ font-size:14px; color:#6b6a72; margin:0; }}
  a {{ color:#c4a265; }}
</style>
</head>
<body>
<div class="card">
  <div class="check">&#10003;</div>
  <h1>Email Verified</h1>
  <p>Redirecting you to the dashboard...</p>
  <p style="margin-top:16px;"><a href="{redirect_url}">Click here if not redirected</a></p>
</div>
</body>
</html>"#,
        redirect_url = redirect_url,
    );

    Ok(axum::response::Html(html).into_response())
```

Note: The function return type changes from `Result<impl IntoResponse, AppError>` — it stays the same since `Html` also implements `IntoResponse`.

Also update the error case to return a similar branded HTML page instead of JSON for the invalid token error.

- [ ] **Step 2: Verify cargo check**

Run: `cargo check -p gatewasm-server`
Expected: clean build

---

### Task 4: Login Page — Invite-Aware Banner + Smart Mode

**Files:**
- Modify: `admin/src/platform/login/store.ts` (add invite resolve logic)
- Modify: `admin/src/platform/login/LoginPage.ts` (add invite banner)
- Modify: `admin/src/platform/login/AuthCard.ts` (toggle login/signup mode, auto-fill email)

- [ ] **Step 1: Add invite resolve signals and fetch to login store.ts**

Add new signals near the existing query param handling (around line 64):

```typescript
// ── Invite resolution ──────────────────────────────────────────────
export const [inviteInfo, setInviteInfo] = createSignal<{
  invite_email: string;
  inviter_email: string | null;
  role: string;
  tenant_names: string[];
  user_exists: boolean;
} | null>(null);
export const [inviteLoading, setInviteLoading] = createSignal(false);
export const [signupMode, setSignupMode] = createSignal(false);

// Auto-resolve invite on load
if (ownerInviteToken) {
  setInviteLoading(true);
  fetch(serverUrl + '/platform/admin/invites/resolve?token=' + encodeURIComponent(ownerInviteToken))
    .then((r) => r.ok ? r.json() : null)
    .then((data: any) => {
      if (data) {
        setInviteInfo(data);
        setEmailInput(data.invite_email);
        if (!data.user_exists) {
          setSignupMode(true);
        }
      }
    })
    .catch(() => { /* resolve failed, show normal login */ })
    .finally(() => setInviteLoading(false));
}
```

Also add signup-related signals:

```typescript
export const [nameInput, setNameInput] = createSignal('');
export const [confirmPasswordInput, setConfirmPasswordInput] = createSignal('');
```

- [ ] **Step 2: Add invite banner to LoginPage.ts**

In `LoginPage.ts`, add the invite banner inside the auth section, above the card. Import the signals:

```typescript
import { inviteInfo, inviteLoading } from './store';
```

Add before the `h('div', { class: 'card' },` line:

```typescript
// Invite banner
createShow(
  () => inviteInfo() !== null && !inviteLoading(),
  () => h('div', { class: 'invite-banner' },
    h('p', { class: 'invite-banner-text' },
      () => {
        const info = inviteInfo()!;
        return (info.inviter_email || 'A team member') +
          ' invited you to join as ' + info.role.charAt(0).toUpperCase() + info.role.slice(1) +
          (info.tenant_names.length > 0 ? ' on ' + info.tenant_names.join(', ') : '');
      },
    ),
  ),
),
```

Update the card header to be dynamic:

```typescript
h('div', { class: 'card-head' },
  h('h2', null, () => inviteInfo()
    ? (inviteInfo()!.user_exists ? 'Sign in to accept invite' : 'Create an account to accept')
    : 'Platform Login'),
  h('p', null, () => inviteInfo()
    ? 'You need a fAuth account to join the team.'
    : 'Sign in to manage your tenants and account settings.'),
),
```

Update the "New here?" footer to be invite-aware:

```typescript
h('div', {
  class: 'new-here mt-lg divider-top text-center text-sm text-muted-dim',
},
  () => inviteInfo() && !inviteInfo()!.user_exists
    ? null  // Hide "New here?" when already in signup mode
    : [
        'New here? ',
        h('a', {
          href: () => '/platform/onboarding' +
            (ownerInviteToken ? '?owner_invite=' + encodeURIComponent(ownerInviteToken) : ''),
        }, 'Start onboarding'),
      ],
),
```

- [ ] **Step 3: Update AuthCard.ts — auto-fill email, disable when invite**

In the email input field, make it readonly when invite is resolved:

```typescript
// Import from store
import { inviteInfo, signupMode, nameInput, setNameInput } from './store';
```

Update the email input to disable when invite-filled:

```typescript
h('input', {
  // ... existing attrs ...
  readonly: () => inviteInfo() !== null,
  class: () => 'input' + (inviteInfo() ? ' input-disabled' : ''),
}),
```

Add the "New here?" toggle at the bottom for non-invite users, or show signup fields when `signupMode()`:

```typescript
// Name field (only in signup mode)
createShow(
  () => signupMode(),
  () => h('div', { class: 'field' },
    h('label', { for: 'name' }, 'Name'),
    h('input', {
      id: 'name',
      type: 'text',
      class: 'input',
      placeholder: 'Your name',
      onInput: (e: Event) => setNameInput((e.target as HTMLInputElement).value),
    }),
  ),
),
```

- [ ] **Step 4: Add invite banner CSS**

Add to the login page CSS (check which CSS file the login page uses and add):

```css
.invite-banner {
  background: rgba(196, 162, 101, 0.1);
  border: 1px solid rgba(196, 162, 101, 0.25);
  border-radius: 8px;
  padding: 16px 20px;
  margin-bottom: 16px;
}
.invite-banner-text {
  font-size: 14px;
  color: var(--text);
  margin: 0;
}
```

- [ ] **Step 5: Frontend build**

Run: `cd admin && npm run build`
Expected: clean build

---

### Task 5: Verification Banner in Console

**Files:**
- Modify: `admin/src/platform/console/ConsoleApp.ts` (add banner)
- Modify: `admin/src/platform/console/store.ts` (add resend function, verified toast)

- [ ] **Step 1: Add resend verification function to store.ts**

Add near the other API functions:

```typescript
export async function resendVerification(): Promise<boolean> {
  try {
    await api.post('/t/platform/auth/resend-verification', {});
    toast.show('Verification email sent — check your inbox', 'success');
    return true;
  } catch (e) {
    toast.show(e instanceof Error ? e.message : 'Failed to send verification email', 'error');
    return false;
  }
}
```

- [ ] **Step 2: Add verified toast detection in init()**

In the `init()` function, after the invite acceptance block and before URL mode detection, add:

```typescript
// Check for verified toast
const verifiedParam = new URLSearchParams(window.location.search).get('verified');
if (verifiedParam === 'true') {
  toast.show('Email verified successfully', 'success');
  const url = new URL(window.location.href);
  url.searchParams.delete('verified');
  history.replaceState(null, '', url.pathname + url.search);
}
```

- [ ] **Step 3: Add verification banner to ConsoleApp.ts**

Import `currentUser` and `resendVerification` from store. Add at the top of the account view, before the tab buttons:

```typescript
// Email verification banner
createShow(
  () => currentUser() !== null && !currentUser()!.email_verified,
  () => h('div', { class: 'verify-banner' },
    h('span', null, 'Verify your email address to secure your account.'),
    h('button', {
      type: 'button',
      class: 'btn btn-sm btn-ghost',
      onClick: () => void resendVerification(),
    }, 'Resend verification'),
  ),
),
```

- [ ] **Step 4: Add verify-banner CSS**

```css
.verify-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 20px;
  background: rgba(251, 191, 36, 0.08);
  border: 1px solid rgba(251, 191, 36, 0.2);
  border-radius: 8px;
  font-size: 14px;
  color: var(--warning);
  margin-bottom: 16px;
}
```

- [ ] **Step 5: Add resend-verification backend endpoint**

In `crates/server/src/handlers/auth.rs`, add:

```rust
pub async fn resend_verification(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = req.extensions().get::<signing::Claims>().ok_or(AppError::Unauthorized {
        error: "unauthorized",
        message: "Not authenticated".into(),
    })?;

    let user = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT id, email, COALESCE(email_verified, FALSE) FROM users WHERE id = $1",
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "user_not_found",
        message: "User not found".into(),
    })?;

    let (user_id, email, verified) = user;

    if verified {
        return Ok(Json(serde_json::json!({ "message": "Email is already verified" })));
    }

    let token = format!("vt_{}", uuid::Uuid::new_v4());
    let expires = chrono::Utc::now() + chrono::Duration::hours(24);

    sqlx::query(
        "UPDATE users SET verification_token = $1, verification_token_expires = $2 WHERE id = $3",
    )
    .bind(&token)
    .bind(expires)
    .bind(&user_id)
    .execute(&state.db)
    .await?;

    let tenant_slug = claims.tenant_id.as_deref().unwrap_or("platform");
    let slug = if tenant_slug == "ten_platform" { "platform" } else { tenant_slug };
    let (subject, text_body, html_body) =
        crate::email::verification_email_content(&state.config.server_url, slug, "fAuth", &token);

    if let Err(err) = crate::email::send_email(&state.config, &email, &subject, &text_body, &html_body).await {
        tracing::warn!(to = %email, error = %err, "Failed to send verification email");
        return Err(AppError::InternalServerError {
            error: "email_failed",
            message: "Failed to send verification email".into(),
        });
    }

    Ok(Json(serde_json::json!({ "message": "Verification email sent" })))
}
```

Register the route in `handlers/mod.rs` under the self-serve authenticated routes or tenant auth routes:

```rust
.route("/auth/resend-verification", post(auth::resend_verification))
```

- [ ] **Step 6: Verify both builds**

Run: `cargo check -p gatewasm-server && cd admin && npm run build`
Expected: both clean

---

### Task 6: Shared Workspace Role Badge in Nav

**Files:**
- Modify: `admin/src/platform/console/store.ts` (add team_role to Tenant type)
- Modify: `admin/src/platform/console/ConsoleApp.ts` (show badge in workspace nav)

- [ ] **Step 1: Add team_role to Tenant type in store.ts**

Find the `Tenant` interface and add:

```typescript
team_role?: string; // "admin" | "editor" | "viewer" — only for access === 'team'
```

- [ ] **Step 2: Show role badge in workspace header**

In `ConsoleApp.ts`, find the workspace header/nav area where the tenant name is displayed. Add a badge next to it when the tenant is a team workspace:

```typescript
// After the tenant name display:
createShow(
  () => selectedTenant()?.access === 'team' && selectedTenant()?.team_role,
  () => h('span', { class: 'badge badge-subtle' },
    () => {
      const role = selectedTenant()!.team_role!;
      return role.charAt(0).toUpperCase() + role.slice(1);
    },
  ),
),
```

- [ ] **Step 3: Frontend build**

Run: `cd admin && npm run build`
Expected: clean build

---

### Task 7: Build & End-to-End Verification

- [ ] **Step 1: Full cargo check**

Run: `cargo check -p gatewasm-server`
Expected: zero warnings

- [ ] **Step 2: Full frontend build**

Run: `cd admin && npm run build`
Expected: clean build

- [ ] **Step 3: Reset dev database and restart server**

```bash
npm run db:reset
npm run dev:server
```

- [ ] **Step 4: Test invite resolve endpoint**

```bash
# Create a user, tenant, and invite (reuse existing flow)
# Then test resolve:
curl -s "http://localhost:3001/platform/admin/invites/resolve?token=INVITE_TOKEN" | python3 -m json.tool
```

Expected: JSON with `invite_email`, `user_exists`, `role`, `tenant_names`

- [ ] **Step 5: Test verify-email returns HTML**

Navigate to `/t/platform/auth/verify-email?token=VALID_TOKEN` in browser.
Expected: branded dark page with checkmark, auto-redirects to console

- [ ] **Step 6: Test email branding**

Send a test invite and check the email in inbox.
Expected: dark background, gold CTA button, fAuth branding
