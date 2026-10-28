use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::Row;

use crate::error::AppError;
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
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
        Some(h) if h.starts_with("Bearer ") => h.strip_prefix("Bearer ").unwrap_or(h),
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

    audit_key_use(
        &state,
        req.uri().path(),
        &ctx.tenant_id,
        "management.key.use",
    )
    .await?;
    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

/// Resolve tenant context from a `mgmt_` API key (existing behavior).
async fn resolve_from_api_key(api_key: &str, state: &AppState) -> Result<TenantContext, AppError> {
    let prefix = crate::text::truncate_chars(api_key, 13); // "mgmt_" + 8 chars
    let key_hash = signing::hash_token(api_key);

    let row = sqlx::query(
        "SELECT id, name, slug, allowed_providers, google_client_id, google_client_secret, \
         github_client_id, github_client_secret, allowed_origins, status, login_redirect_url, \
         signup_mode, plan, trial_ends_at, owner_user_id, branding_logo_url, \
         branding_primary_color, branding_app_name, branding_custom_css, \
         branding_bg_color, branding_card_color, \
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

    // Check trial expiry (mirrors extract_tenant logic)
    let plan: Option<String> = row.get("plan");
    let trial_ends_at: Option<chrono::DateTime<Utc>> = row.get("trial_ends_at");
    if plan.as_deref().unwrap_or("trial") == "trial" {
        if let Some(trial_ends) = trial_ends_at {
            if trial_ends < Utc::now() {
                return Err(AppError::Forbidden {
                    error: "trial_expired",
                    message: "Trial has expired. Visit /get-started to upgrade.".into(),
                });
            }
        }
    }

    Ok(TenantContext::from_db_row(&row, &state.config.server_url))
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
        Some(h) if h.starts_with("Bearer plat_") => h.strip_prefix("Bearer ").unwrap_or(h),
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

    audit_key_use(&state, req.uri().path(), "platform", "platform.key.use").await?;
    Ok(next.run(req).await)
}

/// Every accepted privileged-key request is durably audited before its handler runs.
/// No bearer value, token prefix, query string or secret value enters this record.
async fn audit_key_use(
    state: &AppState,
    resource: &str,
    tenant: &str,
    action: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_log (event_id,timestamp,tenant_id,action,resource,result,compliance_mode) VALUES ($1,NOW(),$2,$3,$4,'success',$5)")
        .bind(format!("evt_{}",uuid::Uuid::new_v4())).bind(tenant).bind(action).bind(resource)
        .bind(state.config.compliance.mode.to_string()).execute(&state.db).await?;
    Ok(())
}
