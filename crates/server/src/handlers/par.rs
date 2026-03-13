use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use chrono::{Duration, Utc};

use crate::error::AppError;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// POST /oauth/par — Push authorization request parameters (RFC 9126).
///
/// Accepts `application/x-www-form-urlencoded` body with the same parameters
/// that would normally be sent to `/auth/authorize` as query params.
/// Returns a single-use `request_uri` (60-second TTL) that the client presents
/// at `/auth/authorize?request_uri=...&client_id=...`.
pub async fn par_endpoint(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    body: String,
) -> Result<impl IntoResponse, AppError> {
    // 1. Parse form-urlencoded body
    let params: Vec<(String, String)> = serde_urlencoded::from_str(&body).map_err(|_| {
        AppError::BadRequest {
            error: "invalid_request",
            message: "Malformed form body".into(),
        }
    })?;

    let get_param = |name: &str| -> Option<String> {
        params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };

    let client_id = get_param("client_id").ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing required parameter: client_id".into(),
    })?;

    let redirect_uri = get_param("redirect_uri").unwrap_or_default();

    let response_type = get_param("response_type").ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing required parameter: response_type".into(),
    })?;

    if response_type != "code" {
        return Err(AppError::BadRequest {
            error: "unsupported_response_type",
            message: "Only response_type=code is supported".into(),
        });
    }

    let code_challenge = get_param("code_challenge").ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing required parameter: code_challenge (PKCE required)".into(),
    })?;

    let code_challenge_method = get_param("code_challenge_method").ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing required parameter: code_challenge_method".into(),
    })?;

    if code_challenge_method != "S256" {
        return Err(AppError::BadRequest {
            error: "invalid_request",
            message: "Only code_challenge_method=S256 is supported".into(),
        });
    }

    let state_param = get_param("state").ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing required parameter: state".into(),
    })?;

    let provider = get_param("provider").ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing required parameter: provider".into(),
    })?;

    // Optional params
    let scope = get_param("scope");
    let nonce = get_param("nonce");
    let invite_token = get_param("invite_token");

    // 2. Validate client_id exists in oauth_clients for this tenant
    let registered_redirect = sqlx::query_scalar::<_, String>(
        "SELECT redirect_uri FROM oauth_clients WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&client_id)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::BadRequest {
        error: "invalid_client",
        message: format!("Unknown client_id: {}", client_id),
    })?;

    // 3. Validate redirect_uri matches the registered one
    if redirect_uri != registered_redirect {
        return Err(AppError::BadRequest {
            error: "invalid_redirect_uri",
            message: "redirect_uri does not match registered client".into(),
        });
    }

    // 4. Check provider is allowed
    if !tenant.allowed_providers.contains(&provider) {
        return Err(AppError::BadRequest {
            error: "provider_not_allowed",
            message: format!("Provider '{}' is not enabled for this tenant", provider),
        });
    }

    // 5. Generate request_uri with 128-bit random hex
    let request_uri = format!(
        "urn:ietf:params:oauth:request_uri:{}",
        hex::encode(uuid::Uuid::new_v4().as_bytes())
    );

    // 6. Store parameters as JSONB
    let parameters = serde_json::json!({
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "response_type": response_type,
        "scope": scope,
        "state": state_param,
        "code_challenge": code_challenge,
        "code_challenge_method": code_challenge_method,
        "provider": provider,
        "nonce": nonce,
        "invite_token": invite_token,
    });

    let expires_at = Utc::now() + Duration::seconds(60);

    sqlx::query(
        "INSERT INTO par_requests (request_uri, client_id, parameters, tenant_id, expires_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&request_uri)
    .bind(&client_id)
    .bind(&parameters)
    .bind(&tenant.tenant_id)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    tracing::info!(
        request_uri = %request_uri,
        client_id = %client_id,
        tenant_id = %tenant.tenant_id,
        "PAR request created"
    );

    // 7. Return 201 Created
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "request_uri": request_uri,
            "expires_in": 60
        })),
    ))
}

/// Delete expired PAR requests. Called from the background cleanup loop.
pub async fn cleanup_expired_par(pool: &sqlx::PgPool) {
    let result = sqlx::query("DELETE FROM par_requests WHERE expires_at < NOW()")
        .execute(pool)
        .await;
    match result {
        Ok(res) if res.rows_affected() > 0 => {
            tracing::debug!(deleted = res.rows_affected(), "Cleaned up expired PAR requests");
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to clean up expired PAR requests");
        }
        _ => {}
    }
}
