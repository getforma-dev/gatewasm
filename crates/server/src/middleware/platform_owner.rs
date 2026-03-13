use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::error::AppError;
use crate::jwt::signing;
use crate::AppState;

/// Middleware for provider control-plane owner actions.
///
/// Requires:
/// 1) Authenticated JWT claims injected by require_auth
/// 2) tenant_id == ten_platform
/// 3) role includes platform_owner
/// 4) SSO auth method + MFA verified (bypassed in local dev)
pub async fn require_platform_owner(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let claims = req
        .extensions()
        .get::<signing::Claims>()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    if claims.tenant_id.as_deref() != Some("ten_platform") {
        return Err(AppError::Forbidden {
            error: "forbidden",
            message: "Platform owner access is only available on platform accounts".into(),
        });
    }

    if !claims.roles.iter().any(|r| r == "platform_owner") {
        return Err(AppError::Forbidden {
            error: "forbidden",
            message: "Platform owner role required".into(),
        });
    }

    let is_prod = state.config.is_production();

    if is_prod && claims.auth_method != "sso" {
        return Err(AppError::Forbidden {
            error: "owner_sso_required",
            message: "Platform owner access requires SSO login (Google or GitHub)".into(),
        });
    }

    if is_prod && !claims.mfa_verified {
        return Err(AppError::Forbidden {
            error: "owner_mfa_required",
            message: "Platform owner access requires MFA verification".into(),
        });
    }

    tracing::debug!(
        auth_plane = "owner",
        auth_mode = if is_prod { "platform_owner_sso_mfa" } else { "platform_owner_local_dev" },
        user = %claims.email,
        is_production = is_prod,
        "Provider control-plane request authorized"
    );

    Ok(next.run(req).await)
}
