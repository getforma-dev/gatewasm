use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::error::AppError;
use crate::jwt::signing;
use crate::AppState;

/// Authenticated platform user (registered on the `platform` tenant).
#[derive(Debug, Clone)]
pub struct PlatformUser {
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
}

/// Middleware that validates a JWT Bearer token and requires `tenant_id == "ten_platform"`.
/// Injects `PlatformUser` into request extensions.
pub async fn require_platform_user(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return Err(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing or invalid Authorization header".into(),
            });
        }
    };

    let claims = signing::verify_access_token(token, &state.config, &state.keys).map_err(|_| {
        AppError::Unauthorized {
            error: "invalid_token",
            message: "Token is expired or invalid".into(),
        }
    })?;

    // Must be a platform tenant user
    match claims.tenant_id.as_deref() {
        Some("ten_platform") => {}
        _ => {
            return Err(AppError::Forbidden {
                error: "not_platform_user",
                message:
                    "This endpoint requires a platform tenant account. Register at /platform/onboarding."
                        .into(),
            });
        }
    }

    // Platform owner emails are now allowed on self-serve API
    // (they need access to the console for team management, tenant creation, etc.)

    req.extensions_mut().insert(PlatformUser {
        user_id: claims.sub,
        email: claims.email,
        name: claims.name,
    });

    Ok(next.run(req).await)
}
