//! Hosted password authorization. Only a short-lived code reaches the browser.
//! Password checks, lockouts and login rate limiting remain in `auth::login`.
use crate::{error::AppError, jwt::signing, middleware::tenant::TenantContext, AppState};
use axum::{
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use chrono::{Duration, Utc};

fn invalid_request() -> AppError {
    AppError::BadRequest {
        error: "invalid_authorization_request",
        message: "Sign-in request is invalid or expired. Start sign-in again.".into(),
    }
}

pub async fn begin(
    state: &AppState,
    tenant: &TenantContext,
    client_id: &str,
    redirect_uri: &str,
    client_state: &str,
    challenge: &str,
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
    // Serialize the per-tenant count and insert; parallel callers cannot exceed it.
    sqlx::query("SELECT id FROM tenants WHERE id=$1 FOR UPDATE")
        .bind(&tenant.tenant_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM hosted_authorization_requests WHERE tenant_id=$1 AND expires_at < NOW()",
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
    sqlx::query("INSERT INTO hosted_authorization_requests (id, tenant_id, client_id, redirect_uri, client_state, code_challenge, browser_hash, expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(&request_id).bind(&tenant.tenant_id).bind(client_id).bind(redirect_uri)
        .bind(client_state).bind(challenge).bind(signing::hash_token(&browser_nonce))
        .bind(Utc::now() + Duration::minutes(5)).execute(&mut *tx).await?;
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

/// Called only after the existing password verifier succeeds, before minting tokens.
pub async fn complete(
    state: &AppState,
    tenant: &TenantContext,
    user_id: &str,
    request_id: &str,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    // This first hosted-code adapter deliberately supports the quiet password demo.
    // Never turn an MFA-protected account into a password-only OAuth session.
    let mfa_enabled: bool = sqlx::query_scalar(
        "SELECT COALESCE(mfa_enabled, FALSE) FROM users WHERE id=$1 AND tenant_id=$2",
    )
    .bind(user_id)
    .bind(&tenant.tenant_id)
    .fetch_one(&state.db)
    .await?;
    if mfa_enabled
        || tenant.security_config.mfa_policy != "off"
        || state.config.compliance.mfa_required
    {
        return Err(AppError::Forbidden {
            error: "mfa_required",
            message: "Hosted code sign-in requires an MFA-capable adapter for this account.".into(),
        });
    }
    let nonce = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .find_map(|part| part.trim().strip_prefix("fauth_authorization="))
        })
        .filter(|v| v.len() == 43)
        .ok_or_else(invalid_request)?;
    let mut tx = state.db.begin().await?;
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "DELETE FROM hosted_authorization_requests r WHERE r.id=$1 AND r.tenant_id=$2 AND r.browser_hash=$3 AND r.expires_at>NOW() AND EXISTS (SELECT 1 FROM oauth_clients c WHERE c.id=r.client_id AND c.tenant_id=r.tenant_id AND c.redirect_uri=r.redirect_uri) RETURNING client_id, redirect_uri, client_state, code_challenge")
        .bind(request_id).bind(&tenant.tenant_id).bind(signing::hash_token(nonce))
        .fetch_optional(&mut *tx).await?;
    let (client_id, redirect_uri, client_state, challenge) = row.ok_or_else(invalid_request)?;
    let code = signing::create_refresh_token();
    sqlx::query("INSERT INTO auth_codes (code,user_id,client_id,redirect_uri,code_challenge,expires_at,tenant_id,provider) VALUES ($1,$2,$3,$4,$5,$6,$7,'email')")
        .bind(&code).bind(user_id).bind(&client_id).bind(&redirect_uri).bind(challenge)
        .bind(Utc::now() + Duration::seconds(60)).bind(&tenant.tenant_id)
        .execute(&mut *tx).await?;
    let mut redirect = url::Url::parse(&redirect_uri).map_err(|_| invalid_request())?;
    redirect
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &client_state);
    tx.commit().await?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({"redirect_uri": redirect.as_str()})),
    )
        .into_response())
}
