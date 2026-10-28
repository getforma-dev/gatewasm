use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::db;
use crate::error::AppError;
use crate::handlers::auth::extract_refresh_cookie;
use crate::jwt::signing;
use crate::AppState;

/// GET /.well-known/web-identity
///
/// FedCM spec discovery endpoint. Returns the IdP's provider URL.
/// Global route — no auth, no tenant context required.
pub async fn web_identity(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "provider_urls": [format!("{}/fedcm/config.json", state.config.server_url)]
    }))
}

/// GET /fedcm/config.json
///
/// FedCM IdP manifest. Tells the browser where to find accounts and token endpoints.
/// Global route — no auth required.
pub async fn config(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "accounts_endpoint": "/fedcm/accounts",
        "id_assertion_endpoint": "/fedcm/token",
        "login_url": "/login",
        "branding": {
            "background_color": "#0a0a0a",
            "color": "#ffffff",
            "icons": [{
                "url": format!("{}/favicon.ico", state.config.server_url),
                "size": 32
            }]
        }
    }))
}

/// GET /fedcm/accounts
///
/// FedCM accounts list. The browser sends this request with credentials (cookies)
/// automatically. Returns the logged-in user's account info, or an empty list
/// if no valid session cookie is present.
///
/// This does NOT use JWT Bearer auth — the browser calls this endpoint directly
/// with the user's session cookies.
pub async fn accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // FedCM accounts endpoint is called by the browser without tenant context.
    // We try all possible cookie names (any tenant slug) by looking for any
    // refresh cookie with a known pattern.
    let refresh_token = find_any_refresh_cookie(&headers);

    let refresh_token = match refresh_token {
        Some(rt) => rt,
        None => {
            return Ok(Json(serde_json::json!({ "accounts": [] })));
        }
    };

    let hash = signing::hash_token(&refresh_token);
    let now = Utc::now();

    // Look up the refresh token to find the user and tenant
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT rt.user_id, rt.tenant_id, rt.tenant_id
         FROM refresh_tokens rt
         WHERE rt.token_hash = $1
           AND rt.revoked = FALSE
           AND rt.expires_at > $2
         LIMIT 1",
    )
    .bind(&hash)
    .bind(now)
    .fetch_optional(&state.db)
    .await?;

    let (user_id, tenant_id, _) = match row {
        Some(r) => r,
        None => {
            return Ok(Json(serde_json::json!({ "accounts": [] })));
        }
    };

    // Fetch user details
    let user = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT id, email, name FROM users WHERE id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?;

    let (uid, email, name) = match user {
        Some(u) => u,
        None => {
            return Ok(Json(serde_json::json!({ "accounts": [] })));
        }
    };

    // Fetch approved clients for this user+tenant
    let approved_clients: Vec<String> = sqlx::query_scalar(
        "SELECT client_id FROM fedcm_approved_clients WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(&uid)
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let display_name = name.as_deref().unwrap_or("");
    let given_name = display_name.split_whitespace().next().unwrap_or("");

    Ok(Json(serde_json::json!({
        "accounts": [{
            "id": uid,
            "email": email,
            "name": display_name,
            "given_name": given_name,
            "picture": "",
            "approved_clients": approved_clients
        }]
    })))
}

#[derive(Debug, Deserialize)]
pub struct TokenAssertionRequest {
    pub account_id: String,
    pub client_id: String,
    pub nonce: Option<String>,
}

/// POST /fedcm/token
///
/// FedCM token assertion endpoint. The browser calls this after the user
/// confirms the identity provider selection. Issues a short-lived access token.
///
/// Requires a valid session cookie (same as accounts endpoint).
pub async fn token_assertion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TokenAssertionRequest>,
) -> Result<impl IntoResponse, AppError> {
    let refresh_token = find_any_refresh_cookie(&headers).ok_or(AppError::Unauthorized {
        error: "no_session",
        message: "No valid session cookie found".into(),
    })?;

    let hash = signing::hash_token(&refresh_token);
    let now = Utc::now();

    // Validate session and get user + tenant
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT rt.user_id, rt.tenant_id
         FROM refresh_tokens rt
         WHERE rt.token_hash = $1
           AND rt.revoked = FALSE
           AND rt.expires_at > $2
         LIMIT 1",
    )
    .bind(&hash)
    .bind(now)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized {
        error: "invalid_session",
        message: "Session has expired or been revoked".into(),
    })?;

    let (user_id, tenant_id) = row;

    // Verify the account_id matches the session user
    if body.account_id != user_id {
        return Err(AppError::BadRequest {
            error: "account_mismatch",
            message: "Account ID does not match session".into(),
        });
    }

    // Record client approval (idempotent)
    let approval_id = format!("fca_{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO fedcm_approved_clients (id, user_id, tenant_id, client_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, tenant_id, client_id) DO NOTHING",
    )
    .bind(&approval_id)
    .bind(&user_id)
    .bind(&tenant_id)
    .bind(&body.client_id)
    .execute(&state.db)
    .await?;

    // Fetch user details for the access token
    let user = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT email, name FROM users WHERE id = $1",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await?;

    let (email, name) = user;

    // Fetch roles
    let roles = db::fetch_user_roles(&state.db, &user_id, Some(&tenant_id)).await;

    // Issue a short-lived access token
    let (access_token, _) = signing::create_access_token(
        signing::AccessTokenInput {
            user_id: &user_id,
            email: &email,
            name: name.as_deref(),
            tenant_id: Some(&tenant_id),
            roles: &roles,
            auth_method: "fedcm",
            mfa_verified: false,
            cnf: None,
        },
        &state.config,
        &state.keys,
    ).await?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "token": access_token })),
    ))
}

/// Search all cookies for any refresh token cookie (any tenant slug).
///
/// FedCM endpoints are called by the browser without tenant context, so we
/// need to look for any cookie matching the refresh token naming pattern:
/// `__Host-gw_rt_*`, `gw_rt_*`, `__Host-refresh_token`, or `refresh_token`.
// Both indices come from `find('=')`. `=` is one byte, so `eq_pos` and
// `eq_pos + 1` are always char boundaries whatever the rest contains.
#[allow(clippy::string_slice)]
fn find_any_refresh_cookie(headers: &HeaderMap) -> Option<String> {
    // First try the generic fallback (no tenant slug)
    if let Some(rt) = extract_refresh_cookie(headers, None) {
        return Some(rt);
    }

    // Scan all cookies for any `gw_rt_*` or `__Host-gw_rt_*` pattern
    for value in headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
    {
        let pair = value.trim();
        if let Some(eq_pos) = pair.find('=') {
            let name = &pair[..eq_pos];
            let val = &pair[eq_pos + 1..];
            if (name.starts_with("gw_rt_") || name.starts_with("__Host-gw_rt_"))
                && !val.is_empty()
            {
                return Some(val.to_string());
            }
        }
    }

    None
}
