//! Token Introspection Endpoint (RFC 7662)
//!
//! `POST /oauth/introspect` — allows resource servers to validate access tokens.
//! Requires management API key (`mgmt_` prefix) for authentication.
//! Feature-gated by `tenant.security_config.introspection_enabled`.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use serde::Deserialize;

use crate::error::AppError;
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// RFC 7662 introspection request body (application/x-www-form-urlencoded).
#[derive(Debug, Deserialize)]
pub struct IntrospectionRequest {
    pub token: String,
}

/// Inactive token response per RFC 7662 — never reveals _why_ the token is inactive.
fn inactive_response() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "active": false })),
    )
}

/// POST /oauth/introspect
///
/// Validates an access token and returns its claims per RFC 7662.
/// The management key authenticates the caller (resource server), and the
/// `TenantContext` is resolved by the `require_management_auth` middleware.
pub async fn introspect(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    axum::Form(form): axum::Form<IntrospectionRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Feature gate: introspection must be enabled for this tenant
    if !tenant.security_config.introspection_enabled {
        return Err(AppError::Forbidden {
            error: "introspection_not_enabled",
            message: "Token introspection is not enabled for this tenant".into(),
        });
    }

    // Verify the presented access token
    let claims = match signing::verify_access_token(&form.token, &state.config, &state.keys) {
        Ok(c) => c,
        Err(_) => return Ok(inactive_response().into_response()),
    };

    // Ensure the token belongs to this tenant (if tenant_id is present in claims)
    if let Some(ref token_tenant) = claims.tenant_id {
        if *token_tenant != tenant.tenant_id {
            return Ok(inactive_response().into_response());
        }
    }

    // Build the active response per RFC 7662
    let mut resp = serde_json::json!({
        "active": true,
        "sub": claims.sub,
        "exp": claims.exp,
        "iat": claims.iat,
        "iss": claims.iss,
        "aud": tenant.tenant_id,
        "scope": "openid profile email",
        "token_type": "Bearer",
    });

    // Include DPoP/mTLS confirmation claim if present
    if let Some(cnf) = &claims.cnf {
        resp["cnf"] = cnf.clone();
    }

    Ok((StatusCode::OK, Json(resp)).into_response())
}
