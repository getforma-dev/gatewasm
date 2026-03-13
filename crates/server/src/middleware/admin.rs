use axum::{extract::Request, middleware::Next, response::Response};

use crate::error::AppError;
use crate::jwt::signing;

/// Middleware that requires the JWT-authenticated user to have the "admin" role.
/// Must be applied AFTER require_auth (which injects Claims into extensions).
pub async fn require_admin_role(req: Request, next: Next) -> Result<Response, AppError> {
    let claims = req
        .extensions()
        .get::<signing::Claims>()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    if !claims.roles.iter().any(|r| r == "admin") {
        return Err(AppError::Forbidden {
            error: "forbidden",
            message: "Admin role required".into(),
        });
    }

    Ok(next.run(req).await)
}
