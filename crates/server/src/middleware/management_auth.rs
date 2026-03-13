use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::Row;

use crate::error::AppError;
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::redirects::resolve_login_redirect;
use crate::security_config::TenantSecurityConfig;
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

/// Middleware that authenticates management API requests.
///
/// Accepts management API key authentication only:
/// `Authorization: Bearer mgmt_xxxxx...`
pub async fn require_management_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let bearer = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return Err(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing or invalid Authorization header".into(),
            });
        }
    };

    if !bearer.starts_with("mgmt_") {
        return Err(AppError::Unauthorized {
            error: "invalid_api_key",
            message: "Management API requires a mgmt_ API key".into(),
        });
    }

    let ctx = resolve_from_api_key(bearer, &state).await?;
    tracing::debug!(tenant_id = %ctx.tenant_id, auth_mode = "mgmt_key", "Management API authenticated");

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

/// Resolve tenant context from a `mgmt_` API key (existing behavior).
async fn resolve_from_api_key(api_key: &str, state: &AppState) -> Result<TenantContext, AppError> {
    let prefix = &api_key[..api_key.len().min(13)]; // "mgmt_" + 8 chars
    let key_hash = signing::hash_token(api_key);

    let row = sqlx::query(
        "SELECT id, name, slug, allowed_providers, google_client_id, google_client_secret, \
         github_client_id, github_client_secret, allowed_origins, status, login_redirect_url, \
         signup_mode, plan, trial_ends_at, owner_user_id, branding_logo_url, \
         branding_primary_color, branding_app_name, branding_custom_css, \
         COALESCE(security_config, '{}') AS security_config \
         FROM tenants WHERE api_key_prefix = $1 AND api_key_hash = $2",
    )
    .bind(prefix)
    .bind(&key_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to look up management API key");
        AppError::InternalError {
            error: "server_error",
            message: "Failed to authenticate management API key".into(),
        }
    })?
    .ok_or(AppError::Unauthorized {
        error: "invalid_api_key",
        message: "Invalid management API key".into(),
    })?;

    let status: String = row.get("status");
    if status != "active" {
        return Err(AppError::Forbidden {
            error: "tenant_suspended",
            message: "This tenant has been suspended".into(),
        });
    }

    let slug: String = row.get("slug");
    let providers_json: serde_json::Value = row.get("allowed_providers");
    let origins_json: serde_json::Value = row.get("allowed_origins");
    let login_redirect_url: Option<String> = row.get("login_redirect_url");

    let allowed_providers: Vec<String> = serde_json::from_value(providers_json).unwrap_or_default();
    let allowed_origins: Vec<String> = serde_json::from_value(origins_json).unwrap_or_default();

    let security_config_json: serde_json::Value = row.get("security_config");
    let security_config = TenantSecurityConfig::from_json(&security_config_json);

    Ok(TenantContext {
        tenant_id: row.get("id"),
        name: row.get("name"),
        slug: slug.clone(),
        allowed_providers,
        google_client_id: row.get("google_client_id"),
        google_client_secret: row.get("google_client_secret"),
        github_client_id: row.get("github_client_id"),
        github_client_secret: row.get("github_client_secret"),
        allowed_origins,
        status,
        signup_mode: row.get::<Option<String>, _>("signup_mode").unwrap_or_else(|| "open".to_string()),
        plan: row.get::<Option<String>, _>("plan").unwrap_or_else(|| "starter".to_string()),
        trial_ends_at: row.get("trial_ends_at"),
        owner_user_id: row.get("owner_user_id"),
        branding_logo_url: row.get("branding_logo_url"),
        branding_primary_color: row.get::<Option<String>, _>("branding_primary_color").unwrap_or_else(|| "#6366f1".to_string()),
        branding_app_name: row.get("branding_app_name"),
        branding_custom_css: row.get("branding_custom_css"),
        login_redirect_url: resolve_login_redirect(
            &state.config.server_url,
            &slug,
            login_redirect_url.as_deref(),
        ),
        security_config,
    })
}

/// Middleware that authenticates platform admin requests using the `PLATFORM_ADMIN_KEY`.
pub async fn require_platform_admin(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer plat_") => &h[7..],
        _ => {
            return Err(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing or invalid platform admin key".into(),
            });
        }
    };

    let expected = state
        .config
        .platform_admin_key
        .as_deref()
        .ok_or(AppError::InternalError {
            error: "server_error",
            message: "Platform admin key not configured".into(),
        })?;

    let mac_key = state.config.csrf_secret.as_bytes();
    let mut expected_mac =
        HmacSha256::new_from_slice(mac_key).map_err(|_| AppError::InternalError {
            error: "server_error",
            message: "Failed to initialize admin key verifier".into(),
        })?;
    expected_mac.update(expected.as_bytes());
    let expected_tag = expected_mac.finalize().into_bytes();

    let mut presented_mac =
        HmacSha256::new_from_slice(mac_key).map_err(|_| AppError::InternalError {
            error: "server_error",
            message: "Failed to initialize admin key verifier".into(),
        })?;
    presented_mac.update(token.as_bytes());

    if presented_mac.verify_slice(&expected_tag).is_err() {
        return Err(AppError::Unauthorized {
            error: "invalid_admin_key",
            message: "Invalid platform admin key".into(),
        });
    }

    Ok(next.run(req).await)
}
