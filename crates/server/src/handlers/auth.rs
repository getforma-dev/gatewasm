use axum::{
    extract::{ConnectInfo, FromRequest, Query, State},
    http::{header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use std::net::SocketAddr;

use crate::compliance::audit::{self, AuditAction, RESULT_FAILURE, RESULT_SUCCESS};
use crate::crypto::password;
use crate::db;
use crate::email;
use crate::error::AppError;
use crate::jwt::signing;
use crate::middleware::entitlements::ensure_user_capacity;
use crate::middleware::fingerprint::extract_client_ip;
use crate::middleware::tenant::TenantContext;

use crate::types::*;
use crate::AppState;

pub fn is_valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Build a Set-Cookie header for refresh tokens.
/// Uses SameSite=None; Secure for cross-origin usage (hosted login → customer app).
/// Falls back to SameSite=Lax (no Secure) for HTTP dev servers.
/// NOTE: Partitioned is intentionally omitted — it prevents cross-site cookie sending
/// in the redirect-then-fetch pattern used by hosted login + Bridge.js.
///
/// In HTTPS mode, the cookie name is prefixed with `__Host-` for strongest cookie
/// protection. `__Host-` requires `Secure; Path=/` — tenant path scoping (`/t/{slug}`)
/// is dropped; tenant isolation relies on the `tenant_id` claim in the signed JWT.
pub fn refresh_cookie_header(
    token: &str,
    max_age: u64,
    server_url: &str,
    tenant_slug: Option<&str>,
) -> (axum::http::HeaderName, HeaderValue) {
    let (cookie_name, cookie_path) = refresh_cookie_scope(tenant_slug);
    let is_https = server_url.starts_with("https://");
    let cookie = if is_https {
        // __Host- prefix: requires Secure + Path=/ (no Domain allowed).
        // Tenant path scoping is sacrificed; tenant_id in the JWT provides isolation.
        format!(
            "__Host-{}={}; HttpOnly; Secure; SameSite=None; Path=/; Max-Age={}",
            cookie_name, token, max_age
        )
    } else {
        // Dev mode: SameSite=Lax (browsers require Secure for SameSite=None)
        // No __Host- prefix (requires Secure which requires HTTPS)
        format!(
            "{}={}; HttpOnly; SameSite=Lax; Path={}; Max-Age={}",
            cookie_name, token, cookie_path, max_age
        )
    };
    (SET_COOKIE, HeaderValue::from_str(&cookie).expect("cookie value is always valid: base64/hex token + alphanumeric slug"))
}

fn scoped_cookie_name(tenant_slug: &str) -> String {
    let normalized: String = tenant_slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    format!("gw_rt_{}", normalized)
}

fn refresh_cookie_scope(tenant_slug: Option<&str>) -> (String, String) {
    match tenant_slug {
        Some(slug) if slug == "platform" => (scoped_cookie_name(slug), "/".to_string()),
        Some(slug) if !slug.is_empty() => (scoped_cookie_name(slug), format!("/t/{}", slug)),
        _ => ("refresh_token".to_string(), "/".to_string()),
    }
}

/// Extract a named cookie value from request headers.
pub fn extract_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    extract_cookie_values(headers, name).into_iter().last()
}

/// Extract all values for a named cookie, preserving request order.
pub fn extract_cookie_values(headers: &axum::http::HeaderMap, name: &str) -> Vec<String> {
    let prefix = format!("{}=", name);
    let mut found: Vec<String> = Vec::new();
    for pair in headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
    {
        let pair = pair.trim();
        if pair.starts_with(&prefix) {
            found.push(pair[prefix.len()..].to_string());
        }
    }
    found
}

/// Extract tenant-scoped refresh cookie, falling back to legacy `refresh_token`.
pub fn extract_refresh_cookie(
    headers: &axum::http::HeaderMap,
    tenant_slug: Option<&str>,
) -> Option<String> {
    extract_refresh_cookie_candidates(headers, tenant_slug)
        .into_iter()
        .next()
}

/// Extract all refresh token cookie candidates for this request.
/// Priority: `__Host-{scoped}` → `{scoped}` → `__Host-refresh_token` → `refresh_token`.
/// This ensures production `__Host-` prefixed cookies are preferred, with legacy fallbacks
/// for backward compatibility during rollout and for HTTP dev mode.
pub fn extract_refresh_cookie_candidates(
    headers: &axum::http::HeaderMap,
    tenant_slug: Option<&str>,
) -> Vec<String> {
    let (scoped_name, _) = refresh_cookie_scope(tenant_slug);
    let host_scoped = format!("__Host-{}", scoped_name);

    // 1. __Host- prefixed scoped cookie (production HTTPS)
    let mut candidates = extract_cookie_values(headers, &host_scoped);
    // 2. Unprefixed scoped cookie (dev HTTP / legacy)
    candidates.extend(extract_cookie_values(headers, &scoped_name));
    // 3. Legacy `refresh_token` variants (oldest fallback)
    if scoped_name != "refresh_token" {
        let host_legacy = "__Host-refresh_token";
        candidates.extend(extract_cookie_values(headers, host_legacy));
        candidates.extend(extract_cookie_values(headers, "refresh_token"));
    }
    candidates.dedup();
    candidates
}

/// Build a Set-Cookie header that clears the refresh token cookie.
/// In HTTPS mode, clears the `__Host-` prefixed cookie at `Path=/`.
pub fn clear_refresh_cookie(
    server_url: &str,
    tenant_slug: Option<&str>,
) -> (axum::http::HeaderName, HeaderValue) {
    let (cookie_name, cookie_path) = refresh_cookie_scope(tenant_slug);
    let is_https = server_url.starts_with("https://");
    let cookie = if is_https {
        format!(
            "__Host-{}=; HttpOnly; Secure; SameSite=None; Path=/; Max-Age=0",
            cookie_name
        )
    } else {
        format!(
            "{}=; HttpOnly; SameSite=Lax; Path={}; Max-Age=0",
            cookie_name, cookie_path
        )
    };
    (SET_COOKIE, HeaderValue::from_str(&cookie).expect("cookie value is always valid: base64/hex token + alphanumeric slug"))
}

/// Clears the legacy cookie used prior to tenant-scoped cookie isolation.
pub fn clear_legacy_refresh_cookie(server_url: &str) -> (axum::http::HeaderName, HeaderValue) {
    let is_https = server_url.starts_with("https://");
    let cookie = if is_https {
        "refresh_token=; HttpOnly; Secure; SameSite=None; Path=/; Max-Age=0"
    } else {
        "refresh_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
    };
    (SET_COOKIE, HeaderValue::from_str(cookie).expect("cookie value is always valid: base64/hex token + alphanumeric slug"))
}

/// Clears the `__Host-refresh_token` cookie that may exist from production deployments
/// after the `__Host-` prefix was introduced. Only emitted in HTTPS mode.
fn clear_host_legacy_refresh_cookie(
    server_url: &str,
) -> Option<(axum::http::HeaderName, HeaderValue)> {
    if server_url.starts_with("https://") {
        Some((
            SET_COOKIE,
            HeaderValue::from_str(
                "__Host-refresh_token=; HttpOnly; Secure; SameSite=None; Path=/; Max-Age=0",
            )
            .expect("cookie value is always valid: base64/hex token + alphanumeric slug"),
        ))
    } else {
        None
    }
}

/// Clears old platform refresh cookie variants that may still exist from older deployments.
fn clear_platform_legacy_path_cookie(server_url: &str) -> (axum::http::HeaderName, HeaderValue) {
    let cookie_name = scoped_cookie_name("platform");
    let is_https = server_url.starts_with("https://");
    let cookie = if is_https {
        format!(
            "{}=; HttpOnly; Secure; SameSite=None; Path=/t/platform; Max-Age=0",
            cookie_name
        )
    } else {
        format!(
            "{}=; HttpOnly; SameSite=Lax; Path=/t/platform; Max-Age=0",
            cookie_name
        )
    };
    (SET_COOKIE, HeaderValue::from_str(&cookie).expect("cookie value is always valid: base64/hex token + alphanumeric slug"))
}

/// Appends Set-Cookie headers that issue the active refresh cookie and clear known legacy variants.
pub fn append_refresh_cookie_headers(
    headers: &mut HeaderMap,
    token: &str,
    max_age: u64,
    server_url: &str,
    tenant_slug: Option<&str>,
) {
    append_refresh_cookie_clear_headers(headers, server_url, tenant_slug);
    let issue = refresh_cookie_header(token, max_age, server_url, tenant_slug);
    headers.append(issue.0, issue.1);
}

/// Appends Set-Cookie headers that clear active and legacy refresh cookie variants.
/// In HTTPS mode this includes both `__Host-` prefixed (current) and unprefixed (pre-migration)
/// variants to ensure a clean transition.
pub fn append_refresh_cookie_clear_headers(
    headers: &mut HeaderMap,
    server_url: &str,
    tenant_slug: Option<&str>,
) {
    // Clear the current active cookie (__Host- in HTTPS, plain in HTTP)
    let clear_scoped = clear_refresh_cookie(server_url, tenant_slug);
    headers.append(clear_scoped.0, clear_scoped.1);

    // In HTTPS mode, also clear the pre-__Host- era unprefixed scoped cookie
    if let Some(hdr) = clear_pre_host_prefix_cookie(server_url, tenant_slug) {
        headers.append(hdr.0, hdr.1);
    }

    // Clear legacy `refresh_token` (unprefixed)
    let clear_legacy = clear_legacy_refresh_cookie(server_url);
    headers.append(clear_legacy.0, clear_legacy.1);

    // Clear `__Host-refresh_token` if in HTTPS mode
    if let Some(hdr) = clear_host_legacy_refresh_cookie(server_url) {
        headers.append(hdr.0, hdr.1);
    }

    if tenant_slug == Some("platform") {
        let clear_platform_legacy = clear_platform_legacy_path_cookie(server_url);
        headers.append(clear_platform_legacy.0, clear_platform_legacy.1);
    }
}

/// Clears the unprefixed scoped cookie that was used in HTTPS mode before the `__Host-`
/// prefix migration. Only emitted in HTTPS mode (in HTTP mode the active cookie IS unprefixed).
fn clear_pre_host_prefix_cookie(
    server_url: &str,
    tenant_slug: Option<&str>,
) -> Option<(axum::http::HeaderName, HeaderValue)> {
    if !server_url.starts_with("https://") {
        return None;
    }
    let (cookie_name, cookie_path) = refresh_cookie_scope(tenant_slug);
    let cookie = format!(
        "{}=; HttpOnly; Secure; SameSite=None; Path={}; Max-Age=0",
        cookie_name, cookie_path
    );
    Some((SET_COOKIE, HeaderValue::from_str(&cookie).expect("cookie value is always valid: base64/hex token + alphanumeric slug")))
}

fn normalize_invite_token(token: Option<&str>) -> Option<String> {
    token
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

#[derive(Debug, Clone)]
pub(crate) struct InviteAcceptance {
    pub role_id: String,
    pub role_name: String,
    pub already_accepted: bool,
}

pub(crate) async fn accept_tenant_invite_for_user(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    user_email: &str,
    invite_token: &str,
) -> Result<InviteAcceptance, AppError> {
    let invite_token = invite_token.trim();
    if invite_token.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_invite_token",
            message: "Invite token is required".into(),
        });
    }

    let token_hash = signing::hash_token(invite_token);
    let normalized_email = normalize_email(user_email);
    let mut tx = state.db.begin().await?;

    let invite = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            chrono::DateTime<Utc>,
            Option<String>,
        ),
    >(
        "SELECT id, email, role_id, status, expires_at, accepted_by_user_id
         FROM tenant_user_invites
         WHERE tenant_id = $1
           AND token_hash = $2
         LIMIT 1
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::BadRequest {
        error: "invalid_invite_token",
        message: "Invite not found".into(),
    })?;

    let (invite_id, invite_email, role_id, status, expires_at, accepted_by_user_id) = invite;
    let role_name: String = sqlx::query_scalar("SELECT name FROM roles WHERE id = $1")
        .bind(&role_id)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or_else(|| "unknown".to_string());
    if normalize_email(&invite_email) != normalized_email {
        return Err(AppError::Forbidden {
            error: "invite_email_mismatch",
            message: "Invite email does not match the signed-in account".into(),
        });
    }

    if status == "revoked" {
        return Err(AppError::BadRequest {
            error: "invite_revoked",
            message: "This invite has been revoked".into(),
        });
    }

    if status == "expired" || expires_at < Utc::now() {
        let _ = sqlx::query(
            "UPDATE tenant_user_invites
             SET status = 'expired', updated_at = NOW()
             WHERE id = $1",
        )
        .bind(&invite_id)
        .execute(&mut *tx)
        .await;
        let _ = tx.commit().await;
        return Err(AppError::BadRequest {
            error: "invite_expired",
            message: "This invite has expired".into(),
        });
    }

    if status == "accepted" {
        if accepted_by_user_id.as_deref() != Some(user_id) {
            return Err(AppError::Forbidden {
                error: "invite_already_used",
                message: "This invite was already accepted by another account".into(),
            });
        }

        sqlx::query(
            "INSERT INTO user_roles (user_id, role_id, tenant_id)
             VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(&role_id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        return Ok(InviteAcceptance {
            role_id,
            role_name,
            already_accepted: true,
        });
    }

    if status != "pending" {
        return Err(AppError::BadRequest {
            error: "invalid_invite_state",
            message: "Invite is not in a usable state".into(),
        });
    }

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(&role_id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE tenant_user_invites
         SET status = 'accepted', accepted_by_user_id = $1, accepted_at = NOW(), updated_at = NOW()
         WHERE id = $2
           AND status = 'pending'",
    )
    .bind(user_id)
    .bind(&invite_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(InviteAcceptance {
        role_id,
        role_name,
        already_accepted: false,
    })
}

fn is_platform_owner_email(state: &AppState, email: &str) -> bool {
    state
        .config
        .platform_owner_emails
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(email))
}

pub(crate) async fn ensure_platform_account_and_owner_role(
    state: &AppState,
    user_id: &str,
    email: &str,
    tenant_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    if tenant_id != Some("ten_platform") {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO platform_accounts (user_id, status, created_at, updated_at)
         VALUES ($1, 'active', NOW(), NOW())
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .execute(&state.db)
    .await?;

    if is_platform_owner_email(state, email) {
        sqlx::query(
            "INSERT INTO user_roles (user_id, role_id, tenant_id)
             VALUES ($1, 'role_platform_owner', 'ten_platform')
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .execute(&state.db)
        .await?;
    }

    Ok(())
}

pub async fn register(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = req.extensions().get::<TenantContext>().cloned();
    let tenant_id = tenant.as_ref().map(|t| t.tenant_id.clone());
    let tenant_slug = tenant.as_ref().map(|t| t.slug.clone());
    let tenant_signup_mode = tenant
        .as_ref()
        .map(|t| t.signup_mode.as_str())
        .unwrap_or("open");
    let tenant_app_name = tenant
        .as_ref()
        .and_then(|t| t.branding_app_name.clone())
        .unwrap_or_else(|| {
            tenant
                .as_ref()
                .map(|t| t.name.clone())
                .unwrap_or_else(|| "Forma Auth".to_string())
        });

    // Extract headers for fingerprinting before consuming the request body
    let req_headers = req.headers().clone();
    let register_client_ip = extract_client_ip(&req_headers, None);

    let body: RegisterRequest = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;
    let invite_token = normalize_invite_token(body.invite_token.as_deref());

    let email = normalize_email(&body.email);
    tracing::info!(email = %email, tenant_id = ?tenant_id, "Processing registration");

    if email.is_empty() || body.password.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_request",
            message: "Email and password are required".into(),
        });
    }

    if !is_valid_email(&email) {
        return Err(AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid email format".into(),
        });
    }

    password::validate_password_complexity(&body.password).map_err(|msg| AppError::BadRequest {
        error: "weak_password",
        message: msg.into(),
    })?;

    if tenant_signup_mode == "invite_only" && invite_token.is_none() {
        return Err(AppError::BadRequest {
            error: "invite_required",
            message: "This tenant is invite-only. Use an invitation link to create an account."
                .into(),
        });
    }

    // Enforce tenant user capacity from entitlements.
    if let Some(ref tid) = tenant_id {
        ensure_user_capacity(&state, tid, 1).await?;
    }

    let user_id = format!("usr_{}", uuid::Uuid::new_v4());
    let cred_id = format!("cred_{}", uuid::Uuid::new_v4());
    let hashed = password::hash_password(&body.password)?;
    let name = body.name.as_deref().unwrap_or("");

    let verification_token = format!("vt_{}", uuid::Uuid::new_v4());
    let verification_expires = Utc::now() + Duration::hours(24);

    let refresh_token = signing::create_refresh_token();
    let refresh_hash = signing::hash_token(&refresh_token);
    let refresh_expires = Utc::now() + Duration::seconds(state.config.refresh_token_expiry as i64);
    let family_id = uuid::Uuid::new_v4().to_string();

    let mut tx = state.db.begin().await?;

    let existing = sqlx::query_scalar::<_, String>(
        "SELECT id FROM users WHERE email = $1 AND ($2::text IS NULL OR tenant_id = $2)",
    )
    .bind(&email)
    .bind(&tenant_id)
    .fetch_optional(&mut *tx)
    .await?;

    if existing.is_some() {
        tracing::info!(email = %email, "User already exists");
        return Err(AppError::Conflict {
            error: "user_exists",
            message: "A user with this email already exists".into(),
        });
    }

    let mut assigned_role_id = "role_user".to_string();
    let mut assigned_role_name = "user".to_string();
    let mut pending_invite_id: Option<String> = None;

    if let Some(token) = invite_token.as_deref() {
        let tid = tenant_id.as_deref().ok_or(AppError::BadRequest {
            error: "invalid_invite_token",
            message: "Invite links are only valid for tenant-scoped signups".into(),
        })?;
        let token_hash = signing::hash_token(token);
        let invite = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                chrono::DateTime<Utc>,
                Option<String>,
            ),
        >(
            "SELECT id, email, role_id, status, expires_at, accepted_by_user_id
             FROM tenant_user_invites
             WHERE tenant_id = $1
               AND token_hash = $2
             LIMIT 1
             FOR UPDATE",
        )
        .bind(tid)
        .bind(&token_hash)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::BadRequest {
            error: "invalid_invite_token",
            message: "Invite not found".into(),
        })?;

        let (invite_id, invite_email, role_id, status, expires_at, accepted_by) = invite;
        let role_name: String = sqlx::query_scalar("SELECT name FROM roles WHERE id = $1")
            .bind(&role_id)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or_else(|| "unknown".to_string());
        if normalize_email(&invite_email) != email {
            return Err(AppError::Forbidden {
                error: "invite_email_mismatch",
                message: "Invite email does not match the registration email".into(),
            });
        }

        if status == "revoked" {
            return Err(AppError::BadRequest {
                error: "invite_revoked",
                message: "This invite has been revoked".into(),
            });
        }

        if status == "accepted" {
            return Err(AppError::Forbidden {
                error: "invite_already_used",
                message: if accepted_by.is_some() {
                    "This invite was already accepted by another account".into()
                } else {
                    "This invite was already accepted".into()
                },
            });
        }

        if status == "expired" || expires_at < Utc::now() {
            let _ = sqlx::query(
                "UPDATE tenant_user_invites
                 SET status = 'expired', updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(&invite_id)
            .execute(&mut *tx)
            .await;
            tx.commit().await.ok();
            return Err(AppError::BadRequest {
                error: "invite_expired",
                message: "This invite has expired".into(),
            });
        }

        if status != "pending" {
            return Err(AppError::BadRequest {
                error: "invalid_invite_state",
                message: "Invite is not in a usable state".into(),
            });
        }

        assigned_role_id = role_id;
        assigned_role_name = role_name;
        pending_invite_id = Some(invite_id);
    }

    sqlx::query("INSERT INTO users (id, email, name, verification_token, verification_token_expires, tenant_id) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(&user_id)
        .bind(&email)
        .bind(name)
        .bind(&verification_token)
        .bind(verification_expires)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO credentials (id, user_id, provider, provider_user_id, hashed_password, tenant_id) VALUES ($1, $2, 'email', $3, $4, $5)",
    )
    .bind(&cred_id)
    .bind(&user_id)
    .bind(&email)
    .bind(&hashed)
    .bind(&tenant_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(&user_id)
    .bind(&assigned_role_id)
    .bind(&tenant_id)
    .execute(&mut *tx)
    .await?;

    if let Some(invite_id) = pending_invite_id.as_deref() {
        let accepted = sqlx::query(
            "UPDATE tenant_user_invites
             SET status = 'accepted', accepted_by_user_id = $1, accepted_at = NOW(), updated_at = NOW()
             WHERE id = $2
               AND status = 'pending'",
        )
        .bind(&user_id)
        .bind(invite_id)
        .execute(&mut *tx)
        .await?;
        if accepted.rows_affected() == 0 {
            return Err(AppError::Conflict {
                error: "invite_conflict",
                message: "Invite could not be accepted; please request a new invite".into(),
            });
        }
    }

    let client_fingerprint = {
        use crate::middleware::fingerprint::{compute_fingerprint, extract_client_ip, extract_user_agent};
        let ip = extract_client_ip(&req_headers, None);
        let ua = extract_user_agent(&req_headers);
        compute_fingerprint(&ua, &ip)
    };

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, family_id, tenant_id, mfa_verified, auth_method, auth_provider, last_activity_at, client_fingerprint) VALUES ($1, $2, $3, $4, $5, $6, FALSE, 'password', 'email', NOW(), $7)",
    )
    .bind(format!("rt_{}", uuid::Uuid::new_v4()))
    .bind(&user_id)
    .bind(&refresh_hash)
    .bind(refresh_expires)
    .bind(&family_id)
    .bind(&tenant_id)
    .bind(&client_fingerprint)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    if let Err(e) =
        ensure_platform_account_and_owner_role(&state, &user_id, &email, tenant_id.as_deref()).await
    {
        tracing::warn!(
            user_id = %user_id,
            email = %email,
            error = %e,
            "Failed to ensure platform account/owner role on register"
        );
    }

    if let Some(slug) = tenant_slug.as_deref() {
        dispatch_verification_email(
            &state,
            slug,
            &tenant_app_name,
            &verification_token,
            &user_id,
            &email,
        );
    }

    let roles = vec![assigned_role_name];
    let (access_token, expires_at) = signing::create_access_token(
        signing::AccessTokenInput {
            user_id: &user_id,
            email: &email,
            name: body.name.as_deref(),
            tenant_id: tenant_id.as_deref(),
            roles: &roles,
            auth_method: "password",
            mfa_verified: false,
            cnf: None,
        },
        &state.config,
        &state.keys,
    )?;

    db::enforce_refresh_token_limit(&state.db, &user_id, tenant.as_ref().map_or(10, |t| t.security_config.max_sessions_per_user) as i64).await;

    tracing::info!(user_id = %user_id, email = %email, "User created");

    // Dispatch webhook event
    if let Some(ref tid) = tenant_id {
        super::webhook_engine::dispatch_event(
            &state.db,
            tid,
            "user.registered",
            serde_json::json!({ "user_id": user_id, "email": email }),
        )
        .await;
    }

    if state.config.compliance.audit_log_enabled {
        audit::log_audit(
            &state.db,
            tenant_id.as_deref().unwrap_or("global"),
            Some(&user_id),
            AuditAction::USER_REGISTER,
            "/auth/register",
            &register_client_ip,
            None,
            None,
            RESULT_SUCCESS,
            None,
            &state.config.compliance.mode.to_string(),
        )
        .await;
    }

    let mut headers = HeaderMap::new();
    append_refresh_cookie_headers(
        &mut headers,
        &refresh_token,
        state.config.refresh_token_expiry,
        &state.config.server_url,
        tenant_slug.as_deref(),
    );

    Ok((
        StatusCode::CREATED,
        headers,
        Json(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": "Bearer",
            "expires_in": state.config.access_token_expiry,
            "expires_at": expires_at,
            "user": {
                "id": user_id,
                "email": email,
                "name": name,
                "avatar": null,
                "email_verified": false,
                "roles": roles,
            }
        })),
    ))
}

/// Check whether the account is currently locked out due to too many failed login attempts.
/// Returns `Ok(())` if the account is not locked, or `Err((retry_after, AppError))` if it is.
fn check_account_lockout(
    locked_until: Option<DateTime<Utc>>,
) -> Result<(), (u64, AppError)> {
    if let Some(locked_dt) = locked_until {
        let now = Utc::now();
        if locked_dt > now {
            let retry_after = (locked_dt - now).num_seconds().max(1) as u64;
            return Err((
                retry_after,
                AppError::AccountLocked {
                    error: "account_locked",
                    message: format!(
                        "Account is locked. Try again in {} seconds.",
                        retry_after
                    ),
                    retry_after,
                },
            ));
        }
    }
    Ok(())
}

/// Fire-and-forget geolocation classification + impossible-travel detection.
/// Logs warnings and emits SSF/audit events as needed; errors are swallowed.
async fn check_impossible_travel(
    state: &AppState,
    tenant: &TenantContext,
    user_id: &str,
    email: &str,
    refresh_hash: &str,
    login_ip: &str,
    login_ip_type: &super::geolocation::IpType,
    login_lat: Option<f64>,
    login_lon: Option<f64>,
    ua: Option<&str>,
) {
    if !tenant.security_config.impossible_travel_enabled {
        return;
    }
    let (cur_lat, cur_lon) = match (login_lat, login_lon) {
        (Some(lat), Some(lon)) => (lat, lon),
        _ => return,
    };

    // Query user's most recent previous login location
    let prev = match sqlx::query_as::<_, (f64, f64, DateTime<Utc>, String)>(
        "SELECT login_lat, login_lon, created_at, COALESCE(login_ip_type, 'unknown') \
         FROM refresh_tokens \
         WHERE user_id = $1 AND tenant_id = $2 \
           AND login_lat IS NOT NULL AND login_lon IS NOT NULL \
           AND token_hash != $3 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(&tenant.tenant_id)
    .bind(refresh_hash)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "impossible-travel: failed to query previous login");
            return;
        }
    };

    let (prev_lat, prev_lon, prev_time, prev_ip_type_str) = prev;
    let prev_ip_type = super::geolocation::IpType::from_str_value(&prev_ip_type_str);
    let now = Utc::now();
    let threshold = tenant.security_config.impossible_travel_speed_threshold_kmh;
    let confidence = super::geolocation::assess_travel_confidence(
        prev_lat, prev_lon, prev_time, &prev_ip_type,
        cur_lat, cur_lon, now, login_ip_type,
        threshold,
        100.0,
    );

    match confidence {
        super::geolocation::TravelConfidence::Critical => {
            tracing::warn!(
                user_id = %user_id,
                distance_km = super::geolocation::haversine_km(prev_lat, prev_lon, cur_lat, cur_lon),
                "Impossible travel detected: CRITICAL"
            );
            if tenant.security_config.caep_enabled {
                if let Err(e) = super::ssf_engine::emit_ssf_event(
                    &state.db,
                    &tenant.tenant_id,
                    "device-compliance-change",
                    &serde_json::json!({
                        "user_id": user_id,
                        "email": email,
                        "risk_level": "critical",
                        "reason": "impossible_travel",
                        "distance_km": super::geolocation::haversine_km(prev_lat, prev_lon, cur_lat, cur_lon),
                        "login_ip": login_ip,
                    }),
                    &state.keys,
                )
                .await
                {
                    tracing::error!(error = %e, "Failed to emit SSF device-compliance-change event");
                }
            }
            if state.config.compliance.audit_log_enabled {
                audit::log_audit(
                    &state.db,
                    &tenant.tenant_id,
                    Some(user_id),
                    AuditAction::USER_LOGIN,
                    "/auth/login",
                    login_ip,
                    ua,
                    None,
                    RESULT_SUCCESS,
                    Some("impossible_travel:critical"),
                    &state.config.compliance.mode.to_string(),
                )
                .await;
            }
        }
        super::geolocation::TravelConfidence::Elevated => {
            tracing::warn!(
                user_id = %user_id,
                distance_km = super::geolocation::haversine_km(prev_lat, prev_lon, cur_lat, cur_lon),
                "Impossible travel detected: ELEVATED"
            );
            if tenant.security_config.caep_enabled {
                if let Err(e) = super::ssf_engine::emit_ssf_event(
                    &state.db,
                    &tenant.tenant_id,
                    "impossible-travel",
                    &serde_json::json!({
                        "user_id": user_id,
                        "email": email,
                        "risk_level": "elevated",
                        "distance_km": super::geolocation::haversine_km(prev_lat, prev_lon, cur_lat, cur_lon),
                        "login_ip": login_ip,
                    }),
                    &state.keys,
                )
                .await
                {
                    tracing::error!(error = %e, "Failed to emit SSF impossible-travel event");
                }
            }
            if state.config.compliance.audit_log_enabled {
                audit::log_audit(
                    &state.db,
                    &tenant.tenant_id,
                    Some(user_id),
                    AuditAction::USER_LOGIN,
                    "/auth/login",
                    login_ip,
                    ua,
                    None,
                    RESULT_SUCCESS,
                    Some("impossible_travel:elevated"),
                    &state.config.compliance.mode.to_string(),
                )
                .await;
            }
        }
        super::geolocation::TravelConfidence::Low => {
            if state.config.compliance.audit_log_enabled {
                audit::log_audit(
                    &state.db,
                    &tenant.tenant_id,
                    Some(user_id),
                    AuditAction::USER_LOGIN,
                    "/auth/login",
                    login_ip,
                    ua,
                    None,
                    RESULT_SUCCESS,
                    Some("impossible_travel:low"),
                    &state.config.compliance.mode.to_string(),
                )
                .await;
            }
        }
        super::geolocation::TravelConfidence::Skip => {
            // No action
        }
    }
}

/// Send the verification email for a newly registered user.
/// Logs a warning if delivery fails or SMTP is not configured; never propagates errors.
fn dispatch_verification_email(
    state: &AppState,
    tenant_slug: &str,
    tenant_app_name: &str,
    verification_token: &str,
    user_id: &str,
    email: &str,
) {
    let server_url = state.config.server_url.clone();
    let slug = tenant_slug.to_owned();
    let app_name = tenant_app_name.to_owned();
    let vt = verification_token.to_owned();
    let uid = user_id.to_owned();
    let addr = email.to_owned();
    let config = state.config.clone();

    tokio::spawn(async move {
        let (subject, text, html) =
            email::verification_email_content(&server_url, &slug, &app_name, &vt);
        if email::email_delivery_configured(&config) {
            if let Err(error) =
                email::send_email(&config, &addr, &subject, &text, &html).await
            {
                tracing::warn!(
                    user_id = %uid,
                    email = %addr,
                    %error,
                    "Failed to send verification email"
                );
            }
        } else {
            tracing::warn!(
                user_id = %uid,
                email = %addr,
                "SMTP is not configured; verification email not delivered"
            );
        }
    });
}

pub async fn login(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = req.extensions().get::<TenantContext>().cloned();
    let tenant_id = tenant.as_ref().map(|t| t.tenant_id.clone());
    let tenant_slug = tenant.as_ref().map(|t| t.slug.clone());

    // Extract IP and User-Agent from request before consuming body
    let ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string());
    let ua = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Extract headers for fingerprinting before consuming the request body
    let login_req_headers = req.headers().clone();
    let login_client_ip = extract_client_ip(&login_req_headers, None);

    let body: LoginRequest = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;
    let invite_token = normalize_invite_token(body.invite_token.as_deref());

    let email = normalize_email(&body.email);
    tracing::info!(email = %email, "Processing login");

    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            bool,
            i32,
            Option<chrono::DateTime<Utc>>,
        ),
    >(
        "SELECT u.id, u.email, u.name, c.hashed_password, COALESCE(u.email_verified, FALSE), COALESCE(u.login_attempts, 0), u.locked_until FROM users u JOIN credentials c ON c.user_id = u.id WHERE u.email = $1 AND c.provider = 'email' AND ($2::text IS NULL OR u.tenant_id = $2)",
    )
    .bind(&email)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?;

    let (user_id, db_email, name, hashed_pw, email_verified, login_attempt_count, locked_until) =
        match row {
            Some(r) => r,
            None => {
                tracing::info!(email = %email, "Login failed — user not found");
                db::log_login_attempt(
                    &state.db,
                    &email,
                    None,
                    false,
                    ip.as_deref(),
                    ua.as_deref(),
                    Some("user_not_found"),
                    tenant_id.as_deref(),
                )
                .await;
                return Err(AppError::Unauthorized {
                    error: "invalid_credentials",
                    message: "Email or password is incorrect".into(),
                });
            }
        };

    let max_login_attempts = state
        .config
        .compliance
        .max_failed_logins
        .max(1)
        .min(i32::MAX as u32) as i32;
    let lockout_duration_minutes = state
        .config
        .compliance
        .lockout_duration_minutes
        .max(1)
        .min(i64::MAX as u64) as i64;

    // Check account lockout
    if let Err((_retry_after, lockout_err)) = check_account_lockout(locked_until) {
        db::log_login_attempt(
            &state.db,
            &email,
            Some(&user_id),
            false,
            ip.as_deref(),
            ua.as_deref(),
            Some("account_locked"),
            tenant_id.as_deref(),
        )
        .await;
        return Err(lockout_err);
    }

    let hashed_pw = match hashed_pw {
        Some(pw) => pw,
        None => {
            tracing::info!(email = %email, "Login failed — no password credential");
            db::log_login_attempt(
                &state.db,
                &email,
                Some(&user_id),
                false,
                ip.as_deref(),
                ua.as_deref(),
                Some("no_password_credential"),
                tenant_id.as_deref(),
            )
            .await;
            return Err(AppError::Unauthorized {
                error: "invalid_credentials",
                message: "Email or password is incorrect".into(),
            });
        }
    };

    match password::verify_password(&body.password, &hashed_pw) {
        Ok(true) => {}
        _ => {
            tracing::info!(email = %email, "Login failed — wrong password");
            let new_count = login_attempt_count + 1;
            if new_count >= max_login_attempts {
                let lock_until = Utc::now() + Duration::minutes(lockout_duration_minutes);
                sqlx::query(
                    "UPDATE users SET login_attempts = $1, locked_until = $2 WHERE id = $3",
                )
                .bind(new_count)
                .bind(lock_until)
                .bind(&user_id)
                .execute(&state.db)
                .await
                .ok();
                tracing::warn!(
                    email = %email,
                    user_id = %user_id,
                    threshold = max_login_attempts,
                    lockout_minutes = lockout_duration_minutes,
                    "Account locked after failed login attempts"
                );
            } else {
                sqlx::query("UPDATE users SET login_attempts = $1 WHERE id = $2")
                    .bind(new_count)
                    .bind(&user_id)
                    .execute(&state.db)
                    .await
                    .ok();
            }
            db::log_login_attempt(
                &state.db,
                &email,
                Some(&user_id),
                false,
                ip.as_deref(),
                ua.as_deref(),
                Some("wrong_password"),
                tenant_id.as_deref(),
            )
            .await;
            if state.config.compliance.audit_log_enabled {
                audit::log_audit(
                    &state.db,
                    tenant_id.as_deref().unwrap_or("global"),
                    Some(&user_id),
                    AuditAction::USER_LOGIN_FAILED,
                    "/auth/login",
                    &login_client_ip,
                    ua.as_deref(),
                    None,
                    RESULT_FAILURE,
                    Some("wrong_password"),
                    &state.config.compliance.mode.to_string(),
                )
                .await;
            }
            return Err(AppError::Unauthorized {
                error: "invalid_credentials",
                message: "Email or password is incorrect".into(),
            });
        }
    }

    // Reset login attempts on success
    sqlx::query("UPDATE users SET login_attempts = 0, locked_until = NULL WHERE id = $1")
        .bind(&user_id)
        .execute(&state.db)
        .await
        .ok();

    db::log_login_attempt(
        &state.db,
        &db_email,
        Some(&user_id),
        true,
        ip.as_deref(),
        ua.as_deref(),
        None,
        tenant_id.as_deref(),
    )
    .await;

    if let Err(e) =
        ensure_platform_account_and_owner_role(&state, &user_id, &db_email, tenant_id.as_deref())
            .await
    {
        tracing::warn!(
            user_id = %user_id,
            email = %db_email,
            error = %e,
            "Failed to ensure platform account/owner role on login"
        );
    }

    // Update last_login_at for platform members (best-effort)
    if tenant_id.as_deref() == Some("ten_platform") {
        let _ = sqlx::query(
            "UPDATE platform_members SET last_login_at = NOW(), updated_at = NOW() WHERE user_id = $1"
        )
        .bind(&user_id)
        .execute(&state.db)
        .await;
    }

    if let Some(token) = invite_token.as_deref() {
        let tid = tenant_id.as_deref().ok_or(AppError::BadRequest {
            error: "invalid_invite_token",
            message: "Invite links are only valid for tenant-scoped login".into(),
        })?;
        let invite_acceptance =
            accept_tenant_invite_for_user(&state, tid, &user_id, &db_email, token).await?;
        tracing::info!(
            user_id = %user_id,
            tenant_id = %tid,
            role_id = %invite_acceptance.role_id,
            role_name = %invite_acceptance.role_name,
            already_accepted = invite_acceptance.already_accepted,
            "Applied tenant invite during login"
        );
    }

    let roles = db::fetch_user_roles(&state.db, &user_id, tenant_id.as_deref()).await;

    let (access_token, expires_at) = signing::create_access_token(
        signing::AccessTokenInput {
            user_id: &user_id,
            email: &db_email,
            name: name.as_deref(),
            tenant_id: tenant_id.as_deref(),
            roles: &roles,
            auth_method: "password",
            mfa_verified: false,
            cnf: None,
        },
        &state.config,
        &state.keys,
    )?;

    let refresh_token = signing::create_refresh_token();
    let refresh_hash = signing::hash_token(&refresh_token);
    let refresh_expires = Utc::now() + Duration::seconds(state.config.refresh_token_expiry as i64);
    let family_id = uuid::Uuid::new_v4().to_string();

    let login_fingerprint = {
        use crate::middleware::fingerprint::{compute_fingerprint, extract_client_ip, extract_user_agent};
        let fp_ip = extract_client_ip(&login_req_headers, None);
        let fp_ua = extract_user_agent(&login_req_headers);
        compute_fingerprint(&fp_ua, &fp_ip)
    };

    // ── Geolocation / impossible-travel detection ──────────────────────
    let login_ip_str = &login_client_ip;
    let login_ip_type = super::geolocation::classify_ip(login_ip_str);
    let login_geo = super::geolocation::geolocate_ip(&login_ip_str);
    let login_lat = login_geo.map(|(lat, _)| lat);
    let login_lon = login_geo.map(|(_, lon)| lon);

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, family_id, tenant_id, mfa_verified, auth_method, auth_provider, last_activity_at, client_fingerprint, login_ip, login_ip_type, login_lat, login_lon) VALUES ($1, $2, $3, $4, $5, $6, FALSE, 'password', 'email', NOW(), $7, $8, $9, $10, $11)",
    )
    .bind(format!("rt_{}", uuid::Uuid::new_v4()))
    .bind(&user_id)
    .bind(&refresh_hash)
    .bind(refresh_expires)
    .bind(&family_id)
    .bind(&tenant_id)
    .bind(&login_fingerprint)
    .bind(&login_ip_str)
    .bind(login_ip_type.as_str())
    .bind(login_lat)
    .bind(login_lon)
    .execute(&state.db)
    .await?;

    // ── Impossible travel check (only when tenant has it enabled) ──────
    if let Some(ref t) = tenant {
        check_impossible_travel(
            &state,
            t,
            &user_id,
            &db_email,
            &refresh_hash,
            login_ip_str,
            &login_ip_type,
            login_lat,
            login_lon,
            ua.as_deref(),
        )
        .await;
    }

    db::enforce_refresh_token_limit(&state.db, &user_id, tenant.as_ref().map_or(10, |t| t.security_config.max_sessions_per_user) as i64).await;

    tracing::info!(user_id = %user_id, email = %db_email, "Login success");

    // Dispatch webhook event
    if let Some(ref tid) = tenant_id {
        super::webhook_engine::dispatch_event(
            &state.db,
            tid,
            "user.logged_in",
            serde_json::json!({ "user_id": user_id, "email": db_email }),
        )
        .await;
    }

    if state.config.compliance.audit_log_enabled {
        audit::log_audit(
            &state.db,
            tenant_id.as_deref().unwrap_or("global"),
            Some(&user_id),
            AuditAction::USER_LOGIN,
            "/auth/login",
            &login_client_ip,
            ua.as_deref(),
            None,
            RESULT_SUCCESS,
            None,
            &state.config.compliance.mode.to_string(),
        )
        .await;
    }

    let mut headers = HeaderMap::new();
    append_refresh_cookie_headers(
        &mut headers,
        &refresh_token,
        state.config.refresh_token_expiry,
        &state.config.server_url,
        tenant_slug.as_deref(),
    );

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": "Bearer",
            "expires_in": state.config.access_token_expiry,
            "expires_at": expires_at,
            "user": {
                "id": user_id,
                "email": db_email,
                "name": name,
                "avatar": null,
                "email_verified": email_verified,
                "roles": roles,
            }
        })),
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = req.extensions().get::<TenantContext>().cloned();
    let tenant_id = tenant.as_ref().map(|t| t.tenant_id.clone());
    let tenant_slug = tenant.as_ref().map(|t| t.slug.clone());
    // Claims may be in extensions (if require_auth ran) or we try to decode from the header.
    // Logout is not behind require_auth so we best-effort decode for event dispatch.
    let claims = req
        .extensions()
        .get::<signing::Claims>()
        .cloned()
        .or_else(|| {
            req.headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|h| h.strip_prefix("Bearer "))
                .and_then(|token| {
                    signing::verify_access_token(token, &state.config, &state.keys).ok()
                })
        });
    let cookie_rt = extract_refresh_cookie(req.headers(), tenant_slug.as_deref());
    let logout_client_ip = extract_client_ip(req.headers(), None);

    let body: LogoutRequest = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    // Revoke refresh token from body OR cookie
    let rt_to_revoke = body.refresh_token.as_deref().or(cookie_rt.as_deref());
    tracing::info!(has_token = rt_to_revoke.is_some(), "Processing logout");
    if let Some(refresh_token) = rt_to_revoke {
        let hash = signing::hash_token(refresh_token);
        sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE token_hash = $1")
            .bind(&hash)
            .execute(&state.db)
            .await
            .ok();
    }

    // Dispatch webhook event
    if let Some(ref tid) = tenant_id {
        if let Some(ref c) = claims {
            super::webhook_engine::dispatch_event(
                &state.db,
                tid,
                "user.logged_out",
                serde_json::json!({ "user_id": c.sub }),
            )
            .await;
        }
    }

    // Emit SSF session-revoked event (if tenant has CAEP enabled)
    if let Some(ref t) = tenant {
        if t.security_config.caep_enabled {
            if let Some(ref c) = claims {
                if let Err(e) = super::ssf_engine::emit_ssf_event(
                    &state.db,
                    &t.tenant_id,
                    "session-revoked",
                    &serde_json::json!({
                        "user_id": c.sub,
                        "email": c.email,
                    }),
                    &state.keys,
                )
                .await
                {
                    tracing::error!(error = %e, "Failed to emit SSF session-revoked event");
                }
            }
        }
    }

    if state.config.compliance.audit_log_enabled {
        audit::log_audit(
            &state.db,
            tenant_id.as_deref().unwrap_or("global"),
            claims.as_ref().map(|c| c.sub.as_str()),
            AuditAction::USER_LOGOUT,
            "/auth/logout",
            &logout_client_ip,
            None,
            None,
            RESULT_SUCCESS,
            None,
            &state.config.compliance.mode.to_string(),
        )
        .await;
    }

    Ok((StatusCode::NO_CONTENT, {
        let mut headers = HeaderMap::new();
        append_refresh_cookie_clear_headers(
            &mut headers,
            &state.config.server_url,
            tenant_slug.as_deref(),
        );
        headers
    }))
}

pub async fn delete_account(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let delete_client_ip = extract_client_ip(req.headers(), None);
    let claims = req
        .extensions()
        .get::<signing::Claims>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    let user_id = claims.sub.clone();

    // Verify tenant scoping: the user's token tenant must match the request tenant context
    let tenant = req.extensions().get::<TenantContext>().cloned();
    let tenant_id = tenant.as_ref().map(|t| t.tenant_id.clone());
    if let (Some(ref claim_tid), Some(ref req_tid)) = (&claims.tenant_id, &tenant_id) {
        if claim_tid != req_tid {
            return Err(AppError::Forbidden {
                error: "forbidden",
                message: "Tenant mismatch".into(),
            });
        }
    }

    tracing::info!(user_id = %user_id, "Deleting account");

    // Revoke all refresh tokens for this user before deletion
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1")
        .bind(&user_id)
        .execute(&state.db)
        .await?;

    // Tenant-scoped deletion when within a tenant context; otherwise platform-level deletion
    if let Some(ref tid) = tenant_id {
        sqlx::query("DELETE FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(&user_id)
            .bind(tid)
            .execute(&state.db)
            .await?;
    } else {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(&user_id)
            .execute(&state.db)
            .await?;
    }

    // Dispatch webhook event
    if let Some(ref tid) = tenant_id {
        super::webhook_engine::dispatch_event(
            &state.db,
            tid,
            "user.deleted",
            serde_json::json!({ "user_id": user_id }),
        )
        .await;
    }

    // Audit log
    if state.config.compliance.audit_log_enabled {
        audit::log_audit(
            &state.db,
            tenant_id.as_deref().unwrap_or("global"),
            Some(&user_id),
            AuditAction::USER_ACCOUNT_DELETE,
            "/auth/delete-account",
            &delete_client_ip,
            None,
            None,
            RESULT_SUCCESS,
            None,
            &state.config.compliance.mode.to_string(),
        )
        .await;
    }

    tracing::info!(user_id = %user_id, "Account deleted");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims =
        req.extensions()
            .get::<signing::Claims>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Not authenticated".into(),
            })?;

    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT r.name FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1",
    )
    .bind(&claims.sub)
    .fetch_all(&state.db)
    .await
    .unwrap_or_else(|_| claims.roles.clone());

    let permissions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT val FROM (\
           SELECT jsonb_array_elements_text(r.permissions_jsonb) AS val \
           FROM roles r JOIN user_roles ur ON ur.role_id = r.id \
           WHERE ur.user_id = $1\
         ) sub",
    )
    .bind(&claims.sub)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let (name, avatar, mfa_enabled, email_verified): (Option<String>, Option<String>, bool, bool) = sqlx::query_as(
        "SELECT name, avatar, COALESCE(mfa_enabled, FALSE), COALESCE(email_verified, FALSE) FROM users WHERE id = $1",
    )
            .bind(&claims.sub)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .unwrap_or((claims.name.clone(), None, false, false));

    // Check if user is an account owner (has a self-referencing platform_members row).
    let is_account_owner: bool = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM platform_members
           WHERE user_id = $1 AND account_owner_user_id = $1 AND status = 'active'
         )"
    )
    .bind(&claims.sub)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    // For non-owners, find their highest per-tenant role across all team memberships.
    let platform_role: Option<String> = if is_account_owner {
        None // Account owners don't need a per-tenant role — they have full access
    } else {
        sqlx::query_scalar(
            "SELECT pmt.role FROM platform_members pm \
             JOIN platform_member_tenants pmt ON pmt.member_id = pm.id \
             WHERE pm.user_id = $1 AND pm.status = 'active' \
             ORDER BY CASE pmt.role WHEN 'admin' THEN 0 WHEN 'editor' THEN 1 WHEN 'viewer' THEN 2 END \
             LIMIT 1"
        )
        .bind(&claims.sub)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
    };

    Ok(Json(serde_json::json!({
        "id": claims.sub,
        "email": claims.email,
        "name": name,
        "avatar": avatar,
        "email_verified": email_verified,
        "roles": roles,
        "permissions": permissions,
        "auth_method": claims.auth_method,
        "mfa_verified": claims.mfa_verified,
        "mfa_enabled": mfa_enabled,
        "is_account_owner": is_account_owner,
        "platform_role": platform_role,
    })))
}

#[derive(sqlx::FromRow)]
struct LoginHistoryRow {
    id: String,
    email: String,
    success: bool,
    ip_address: Option<String>,
    user_agent: Option<String>,
    failure_reason: Option<String>,
    created_at: chrono::DateTime<Utc>,
}

pub async fn login_history(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims =
        req.extensions()
            .get::<signing::Claims>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Not authenticated".into(),
            })?;

    let rows = sqlx::query_as::<_, LoginHistoryRow>(
        "SELECT id, email, success, ip_address, user_agent, failure_reason, created_at FROM login_attempts WHERE user_id = $1 ORDER BY created_at DESC LIMIT 20",
    )
    .bind(&claims.sub)
    .fetch_all(&state.db)
    .await?;

    let attempts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "email": row.email,
                "success": row.success,
                "ip_address": row.ip_address,
                "user_agent": row.user_agent,
                "failure_reason": row.failure_reason,
                "created_at": row.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "history": attempts })))
}

pub async fn verify_email(
    State(state): State<AppState>,
    Query(params): Query<VerifyEmailQuery>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!("Processing email verification");

    let user_id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM users WHERE verification_token = $1 AND verification_token_expires > $2",
    )
    .bind(&params.token)
    .bind(Utc::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::BadRequest {
        error: "invalid_token",
        message: "Invalid or expired verification token".into(),
    })?;

    sqlx::query(
        "UPDATE users SET email_verified = TRUE, verification_token = NULL, verification_token_expires = NULL WHERE id = $1",
    )
    .bind(&user_id)
    .execute(&state.db)
    .await?;

    tracing::info!(user_id = %user_id, "Email verified");

    // Dispatch webhook — look up tenant_id for this user
    let tenant_id: Option<String> = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    if let Some(ref tid) = tenant_id {
        super::webhook_engine::dispatch_event(
            &state.db,
            tid,
            "user.email_verified",
            serde_json::json!({ "user_id": user_id }),
        )
        .await;
    }

    // Always redirect to login — avoids wrong-session issues when verification
    // link is opened in a different browser profile than the one that registered.
    let redirect_url = "/platform/login?verified=true";

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
  .check {{ font-size:48px; margin-bottom:16px; color:#c4a265; }}
  h1 {{ font-size:20px; margin:0 0 8px; color:#e8e6e3; }}
  p {{ font-size:14px; color:#6b6a72; margin:0; }}
  a {{ color:#c4a265; text-decoration:none; }}
  a:hover {{ text-decoration:underline; }}
</style>
</head>
<body>
<div class="card">
  <div class="check">&#10003;</div>
  <h1>Email Verified</h1>
  <p>Redirecting you to sign in...</p>
  <p style="margin-top:16px;"><a href="{redirect_url}">Click here if not redirected</a></p>
</div>
</body>
</html>"#,
        redirect_url = redirect_url,
    );

    Ok(axum::response::Html(html).into_response())
}

/// POST /t/{slug}/auth/resend-verification — Resend verification email.
/// Authenticated. Rate-limited via register_limiter (5 burst per IP).
pub async fn resend_verification(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = req
        .extensions()
        .get::<signing::Claims>()
        .ok_or(AppError::Unauthorized {
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
    let expires = Utc::now() + chrono::Duration::hours(24);

    sqlx::query(
        "UPDATE users SET verification_token = $1, verification_token_expires = $2 WHERE id = $3",
    )
    .bind(&token)
    .bind(expires)
    .bind(&user_id)
    .execute(&state.db)
    .await?;

    let tenant_slug = claims.tenant_id.as_deref().unwrap_or("platform");
    let slug = if tenant_slug == "ten_platform" {
        "platform"
    } else {
        tenant_slug
    };
    let (subject, text_body, html_body) =
        crate::email::verification_email_content(&state.config.server_url, slug, "fAuth", &token);

    if let Err(err) =
        crate::email::send_email(&state.config, &email, &subject, &text_body, &html_body).await
    {
        tracing::warn!(to = %email, error = %err, "Failed to send verification email");
        return Err(AppError::InternalError {
            error: "email_failed",
            message: "Failed to send verification email".into(),
        });
    }

    Ok(Json(serde_json::json!({ "message": "Verification email sent" })))
}

#[cfg(test)]
mod tests {
    use super::extract_cookie;
    use axum::http::HeaderMap;

    #[test]
    fn extract_cookie_prefers_last_value_when_duplicates_exist() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "gw_rt_platform=old_path_token; gw_rt_platform=new_root_token"
                .parse()
                .unwrap(),
        );

        let value = extract_cookie(&headers, "gw_rt_platform");
        assert_eq!(value.as_deref(), Some("new_root_token"));
    }

    #[test]
    fn append_refresh_cookie_headers_issues_platform_cookie_last() {
        let mut headers = HeaderMap::new();
        super::append_refresh_cookie_headers(
            &mut headers,
            "active_token",
            3600,
            "https://auth.getforma.dev",
            Some("platform"),
        );

        let cookies: Vec<String> = headers
            .get_all(axum::http::header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .map(|value| value.to_string())
            .collect();

        // Legacy /t/platform cleanup header (unprefixed, old path)
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.contains("gw_rt_platform=")
                    && !cookie.starts_with("__Host-")
                    && cookie.contains("Path=/t/platform")
                    && cookie.contains("Max-Age=0")),
            "Expected stale /t/platform cookie cleanup header, got {:?}",
            cookies
        );

        // Active cookie should be __Host- prefixed with Path=/
        let last_cookie = cookies.last().expect("missing Set-Cookie headers");
        assert!(
            last_cookie.starts_with("__Host-gw_rt_platform=active_token;"),
            "Expected __Host- prefixed active cookie, got: {}",
            last_cookie
        );
        assert!(last_cookie.contains("Path=/"));
        assert!(last_cookie.contains("Max-Age=3600"));
        assert!(!last_cookie.contains("Max-Age=0"));
    }
}
