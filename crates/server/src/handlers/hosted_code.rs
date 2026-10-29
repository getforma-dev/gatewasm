//! Hosted password authorization. Only a short-lived code reaches the browser.
use crate::{error::AppError, jwt::signing, middleware::tenant::TenantContext, AppState};
use axum::{
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use chrono::{Duration, Utc};
use sqlx::{Postgres, Transaction};

pub(crate) fn invalid_request() -> AppError {
    AppError::BadRequest {
        error: "invalid_authorization_request",
        message: "Sign-in request is invalid or expired. Start sign-in again.".into(),
    }
}

fn invalid_invite() -> AppError {
    AppError::BadRequest {
        error: "invalid_invite_token",
        message: "An active invitation for this account is required.".into(),
    }
}

fn nonce(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .find_map(|p| p.trim().strip_prefix("fauth_authorization="))
        })
        .filter(|v| v.len() == 43)
        .ok_or_else(invalid_request)
}

/// Serialize hosted writes for the tenant and read current policy, not a stale cache.
async fn lock_tenant(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    tenant: &TenantContext,
) -> Result<(), AppError> {
    let (status, providers, security): (String, serde_json::Value, serde_json::Value) =
        sqlx::query_as(
            "SELECT status, allowed_providers, security_config FROM tenants WHERE id=$1 FOR UPDATE",
        )
        .bind(&tenant.tenant_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(invalid_request)?;
    if status != "active"
        || !providers
            .as_array()
            .is_some_and(|p| p.iter().any(|v| v == "email"))
    {
        return Err(invalid_request());
    }
    let policy: crate::security_config::TenantSecurityConfig =
        serde_json::from_value(security).map_err(|_| invalid_request())?;
    if policy.mfa_policy != "off" || state.config.compliance.mfa_required {
        return Err(AppError::Forbidden {
            error: "mfa_required",
            message: "Hosted code sign-in requires an MFA-capable adapter for this account.".into(),
        });
    }
    Ok(())
}

pub(crate) async fn validate_invite_hash(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    hash: &str,
) -> Result<(), AppError> {
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tenant_user_invites i JOIN roles r ON r.id=i.role_id WHERE i.tenant_id=$1 AND i.token_hash=$2 AND i.status='pending' AND i.expires_at>clock_timestamp() AND (r.tenant_id=$1 OR (r.tenant_id IS NULL AND r.id IN ('role_admin','role_user'))))")
        .bind(tenant_id).bind(hash).fetch_one(&mut **tx).await?;
    if valid {
        Ok(())
    } else {
        Err(invalid_invite())
    }
}

pub async fn begin(
    state: &AppState,
    tenant: &TenantContext,
    client_id: &str,
    redirect_uri: &str,
    client_state: &str,
    challenge: &str,
    par: Option<(&str, Option<&str>)>,
) -> Result<Response, AppError> {
    if client_state.is_empty()
        || client_state.len() > 1024
        || challenge.len() != 43
        || !challenge
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid_request());
    }
    let request_id = signing::create_refresh_token();
    let browser_nonce = signing::create_refresh_token();
    let mut tx = state.db.begin().await?;
    // Do not enforce MFA at begin: existing users receive the established explicit
    // MFA refusal after password verification. Registration enforces it before writes.
    sqlx::query("SELECT id FROM tenants WHERE id=$1 FOR UPDATE")
        .bind(&tenant.tenant_id)
        .execute(&mut *tx)
        .await?;
    let invite_hash = par.and_then(|(_, hash)| hash);
    if let Some((uri, _)) = par {
        let expires: Option<chrono::DateTime<Utc>> = sqlx::query_scalar("UPDATE par_requests SET consumed=TRUE WHERE request_uri=$1 AND client_id=$2 AND tenant_id=$3 AND consumed=FALSE AND expires_at>clock_timestamp() RETURNING expires_at")
            .bind(uri).bind(client_id).bind(&tenant.tenant_id).fetch_optional(&mut *tx).await?;
        if expires.is_none_or(|expires| expires <= Utc::now()) {
            return Err(invalid_request());
        }
    }
    if let Some(hash) = invite_hash {
        validate_invite_hash(&mut tx, &tenant.tenant_id, hash).await?;
    }
    sqlx::query(
        "DELETE FROM hosted_authorization_requests WHERE tenant_id=$1 AND expires_at<clock_timestamp()",
    )
    .bind(&tenant.tenant_id)
    .execute(&mut *tx)
    .await?;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM hosted_authorization_requests WHERE tenant_id=$1")
            .bind(&tenant.tenant_id)
            .fetch_one(&mut *tx)
            .await?;
    if count >= 1000 {
        return Err(AppError::TooManyRequests);
    }
    sqlx::query("INSERT INTO hosted_authorization_requests (id,tenant_id,client_id,redirect_uri,client_state,code_challenge,browser_hash,expires_at,invite_token_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&request_id).bind(&tenant.tenant_id).bind(client_id).bind(redirect_uri).bind(client_state)
        .bind(challenge).bind(signing::hash_token(&browser_nonce)).bind(Utc::now()+Duration::minutes(5))
        .bind(invite_hash).execute(&mut *tx).await?;
    tx.commit().await?;
    let mut response = Redirect::temporary(&format!(
        "/t/{}/login?authorization_request={}",
        tenant.slug, request_id
    ))
    .into_response();
    let secure = if state.config.server_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    response.headers_mut().insert(header::SET_COOKIE, format!("fauth_authorization={browser_nonce}; HttpOnly; SameSite=Lax; Path=/t/{}; Max-Age=300{secure}", tenant.slug).parse().map_err(|_| invalid_request())?);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(response)
}

#[derive(sqlx::FromRow)]
struct Pending {
    client_id: String,
    redirect_uri: String,
    client_state: String,
    code_challenge: String,
    invite_token_hash: Option<String>,
    expires_at: chrono::DateTime<Utc>,
}

async fn consume(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &TenantContext,
    request_id: &str,
    headers: &HeaderMap,
) -> Result<Pending, AppError> {
    let pending: Pending = sqlx::query_as("DELETE FROM hosted_authorization_requests r WHERE r.id=$1 AND r.tenant_id=$2 AND r.browser_hash=$3 AND r.expires_at>clock_timestamp() AND EXISTS (SELECT 1 FROM oauth_clients c WHERE c.id=r.client_id AND c.tenant_id=r.tenant_id AND c.redirect_uri=r.redirect_uri) RETURNING client_id,redirect_uri,client_state,code_challenge,invite_token_hash,expires_at")
        .bind(request_id).bind(&tenant.tenant_id).bind(signing::hash_token(nonce(headers)?))
        .fetch_optional(&mut **tx).await?.ok_or_else(invalid_request)?;
    // Keep the registered callback stable until user/invitation/code commit.
    let client: Option<String> = sqlx::query_scalar(
        "SELECT id FROM oauth_clients WHERE id=$1 AND tenant_id=$2 AND redirect_uri=$3 FOR SHARE",
    )
    .bind(&pending.client_id)
    .bind(&tenant.tenant_id)
    .bind(&pending.redirect_uri)
    .fetch_optional(&mut **tx)
    .await?;
    if client.is_none() || pending.expires_at <= Utc::now() {
        return Err(invalid_request());
    }
    Ok(pending)
}

/// Preflight before password/login side effects. Final consumption repeats every check.
pub(crate) async fn preflight(
    state: &AppState,
    tenant: &TenantContext,
    request_id: &str,
    headers: &HeaderMap,
    email: &str,
    require_invite: bool,
) -> Result<(), AppError> {
    let mut tx = state.db.begin().await?;
    lock_tenant(&mut tx, state, tenant).await?;
    let pending = consume(&mut tx, tenant, request_id, headers).await?;
    if let Some(hash) = pending.invite_token_hash.as_deref() {
        invitation(&mut tx, tenant, hash, email).await?;
    } else if require_invite {
        return Err(invalid_invite());
    }
    tx.rollback().await?;
    Ok(())
}

/// Presentation only; submission repeats all checks under the mutation transaction.
pub(crate) async fn registration_allowed(
    state: &AppState,
    tenant: &TenantContext,
    request_id: &str,
    headers: &HeaderMap,
) -> bool {
    async {
        let mut tx = state.db.begin().await?;
        lock_tenant(&mut tx, state, tenant).await?;
        let pending = consume(&mut tx, tenant, request_id, headers).await?;
        validate_invite_hash(
            &mut tx,
            &tenant.tenant_id,
            pending
                .invite_token_hash
                .as_deref()
                .ok_or_else(invalid_invite)?,
        )
        .await?;
        tx.rollback().await?;
        Ok::<(), AppError>(())
    }
    .await
    .is_ok()
}

async fn invitation(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &TenantContext,
    hash: &str,
    email: &str,
) -> Result<(String, String), AppError> {
    let (id, role_id, invited_email, expires_at): (String, String, String, chrono::DateTime<Utc>) = sqlx::query_as(
        "SELECT i.id,i.role_id,i.email,i.expires_at FROM tenant_user_invites i JOIN roles r ON r.id=i.role_id WHERE i.tenant_id=$1 AND i.token_hash=$2 AND i.status='pending' AND i.expires_at>clock_timestamp() AND (r.tenant_id=$1 OR (r.tenant_id IS NULL AND r.id IN ('role_admin','role_user'))) FOR UPDATE OF i,r")
        .bind(&tenant.tenant_id).bind(hash).fetch_optional(&mut **tx).await?.ok_or_else(invalid_invite)?;
    if expires_at <= Utc::now() {
        return Err(invalid_invite());
    }
    if super::auth::normalize_email(&invited_email) != email {
        return Err(AppError::Forbidden {
            error: "invite_email_mismatch",
            message: "Invite email does not match the account email".into(),
        });
    }
    Ok((id, role_id))
}

async fn accept_invite(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &TenantContext,
    user_id: &str,
    invite_id: &str,
    role_id: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO user_roles (user_id,role_id,tenant_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(user_id).bind(role_id).bind(&tenant.tenant_id).execute(&mut **tx).await?;
    let accepted = sqlx::query("UPDATE tenant_user_invites SET status='accepted',accepted_by_user_id=$1,accepted_at=NOW(),updated_at=NOW() WHERE id=$2 AND tenant_id=$3 AND status='pending' AND expires_at>clock_timestamp()")
        .bind(user_id).bind(invite_id).bind(&tenant.tenant_id).execute(&mut **tx).await?;
    if accepted.rows_affected() != 1 {
        return Err(invalid_invite());
    }
    Ok(())
}

async fn issue_code(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &TenantContext,
    user_id: &str,
    pending: Pending,
) -> Result<String, AppError> {
    if pending.expires_at <= Utc::now() {
        return Err(invalid_request());
    }
    let code = signing::create_refresh_token();
    sqlx::query("INSERT INTO auth_codes (code,user_id,client_id,redirect_uri,code_challenge,expires_at,tenant_id,provider) VALUES ($1,$2,$3,$4,$5,$6,$7,'email')")
        .bind(&code).bind(user_id).bind(&pending.client_id).bind(&pending.redirect_uri).bind(&pending.code_challenge)
        .bind(Utc::now()+Duration::seconds(60)).bind(&tenant.tenant_id).execute(&mut **tx).await?;
    let mut redirect = url::Url::parse(&pending.redirect_uri).map_err(|_| invalid_request())?;
    redirect
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &pending.client_state);
    Ok(redirect.into())
}

fn response(status: StatusCode, redirect: String) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({"redirect_uri":redirect})),
    )
        .into_response()
}

/// Called after the existing password verifier. Bound invitations cannot be swapped
/// by a browser body field, and role assignment is committed with the code only.
pub async fn complete(
    state: &AppState,
    tenant: &TenantContext,
    user_id: &str,
    request_id: &str,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin().await?;
    lock_tenant(&mut tx, state, tenant).await?;
    let pending = consume(&mut tx, tenant, request_id, headers).await?;
    let (email, mfa): (String,bool) = sqlx::query_as("SELECT email,COALESCE(mfa_enabled,FALSE) FROM users WHERE id=$1 AND tenant_id=$2 FOR UPDATE")
        .bind(user_id).bind(&tenant.tenant_id).fetch_one(&mut *tx).await?;
    if mfa {
        return Err(AppError::Forbidden {
            error: "mfa_required",
            message: "Hosted code sign-in requires an MFA-capable adapter for this account.".into(),
        });
    }
    if let Some(hash) = pending.invite_token_hash.as_deref() {
        let (id, role) = invitation(&mut tx, tenant, hash, &email).await?;
        accept_invite(&mut tx, tenant, user_id, &id, &role).await?;
    }
    sqlx::query("UPDATE users SET login_attempts=0,locked_until=NULL WHERE id=$1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    let redirect = issue_code(&mut tx, tenant, user_id, pending).await?;
    tx.commit().await?;
    let ip = crate::middleware::fingerprint::extract_client_ip(headers, None);
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    crate::db::log_login_attempt(
        &state.db,
        &email,
        Some(user_id),
        true,
        Some(&ip),
        ua,
        None,
        Some(&tenant.tenant_id),
    )
    .await;
    // Preserve platform-only successful-login bookkeeping, after the validated
    // invitation/code commit; rejected continuations cannot bootstrap roles.
    if let Err(error) = super::auth::ensure_platform_account_and_owner_role(
        state,
        user_id,
        &email,
        Some(&tenant.tenant_id),
    )
    .await
    {
        tracing::warn!(user_id, %error, "Failed to ensure platform account on hosted login");
    }
    if tenant.tenant_id == "ten_platform" {
        let _ = sqlx::query(
            "UPDATE platform_members SET last_login_at=NOW(),updated_at=NOW() WHERE user_id=$1",
        )
        .bind(user_id)
        .execute(&state.db)
        .await;
    }
    Ok(response(StatusCode::OK, redirect))
}

/// Invite-first identity creation. No refresh token/session or signing operation
/// exists on this path. Failed validation rolls back invitation/request/user/code.
pub async fn register(
    state: &AppState,
    tenant: &TenantContext,
    body: &crate::types::RegisterRequest,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    if body.invite_token.is_some() {
        return Err(invalid_request());
    }
    let email = super::auth::normalize_email(&body.email);
    let request_id = body
        .authorization_request
        .as_deref()
        .ok_or_else(invalid_request)?;
    preflight(state, tenant, request_id, headers, &email, true).await?;
    let hashed = crate::crypto::password::hash_password(&body.password)?;
    let mut tx = state.db.begin().await?;
    lock_tenant(&mut tx, state, tenant).await?;
    let pending = consume(&mut tx, tenant, request_id, headers).await?;
    let (invite_id, role_id) = invitation(
        &mut tx,
        tenant,
        pending
            .invite_token_hash
            .as_deref()
            .ok_or_else(invalid_invite)?,
        &email,
    )
    .await?;
    let existing: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE tenant_id=$1 AND email=$2)")
            .bind(&tenant.tenant_id)
            .bind(&email)
            .fetch_one(&mut *tx)
            .await?;
    if existing {
        return Err(AppError::Conflict {
            error: "user_exists",
            message: "A user with this email already exists. Sign in instead.".into(),
        });
    }
    let limit: i32 = sqlx::query_scalar("SELECT COALESCE(e.max_users,t.max_users,1000) FROM tenants t LEFT JOIN tenant_entitlements e ON e.tenant_id=t.id WHERE t.id=$1")
        .bind(&tenant.tenant_id).fetch_one(&mut *tx).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id=$1")
        .bind(&tenant.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= i64::from(limit) {
        return Err(AppError::Forbidden {
            error: "user_limit_reached",
            message: "User limit reached for this tenant.".into(),
        });
    }
    let user_id = format!("usr_{}", uuid::Uuid::new_v4());
    let (verification, verification_hash) = super::auth::new_verification_challenge();
    // The client association comes from the validated, consumed hosted request.
    sqlx::query("INSERT INTO users (id,email,name,verification_token,verification_token_expires,tenant_id,verification_email,verification_client_id,verification_delivery,verification_delivery_at) VALUES ($1,$2,$3,$4,$5,$6,$2,$7,'pending',clock_timestamp())")
        .bind(&user_id).bind(&email).bind(body.name.as_deref().unwrap_or("")).bind(&verification_hash)
        .bind(Utc::now()+Duration::hours(super::auth::VERIFICATION_TTL_HOURS)).bind(&tenant.tenant_id)
        .bind(&pending.client_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO credentials (id,user_id,provider,provider_user_id,hashed_password,tenant_id) VALUES ($1,$2,'email',$3,$4,$5)")
        .bind(format!("cred_{}",uuid::Uuid::new_v4())).bind(&user_id).bind(&email).bind(hashed).bind(&tenant.tenant_id).execute(&mut *tx).await?;
    accept_invite(&mut tx, tenant, &user_id, &invite_id, &role_id).await?;
    let redirect = issue_code(&mut tx, tenant, &user_id, pending).await?;
    tx.commit().await?;
    super::auth::dispatch_verification_email(
        state,
        &tenant.slug,
        tenant.branding_app_name.as_deref().unwrap_or(&tenant.name),
        &verification,
        &user_id,
        &email,
    );
    super::webhook_engine::dispatch_event(
        &state.db,
        &tenant.tenant_id,
        "user.registered",
        serde_json::json!({"user_id":user_id,"email":email}),
    )
    .await;
    if state.config.compliance.audit_log_enabled {
        use crate::compliance::audit::{self, AuditAction, RESULT_SUCCESS};
        audit::log_audit(
            &state.db,
            &tenant.tenant_id,
            Some(&user_id),
            AuditAction::USER_REGISTER,
            "/auth/register",
            &crate::middleware::fingerprint::extract_client_ip(headers, None),
            None,
            None,
            RESULT_SUCCESS,
            None,
            &state.config.compliance.mode.to_string(),
        )
        .await;
    }
    crate::ws::events::emit_activity(&state.event_bus, &tenant.tenant_id, "signup");
    Ok(response(StatusCode::CREATED, redirect))
}
