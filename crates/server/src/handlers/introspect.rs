use axum::{extract::State, response::IntoResponse, Json};

use crate::error::AppError;
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// GET /auth/introspect
///
/// Returns decoded JWT claims, session info, and user profile for the
/// currently authenticated tenant user (Plane 3). Used by the sandbox page.
pub async fn introspect(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = req
        .extensions()
        .get::<signing::Claims>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    let tenant = req
        .extensions()
        .get::<TenantContext>()
        .cloned()
        .ok_or(AppError::BadRequest {
            error: "missing_tenant",
            message: "Tenant context not found".into(),
        })?;

    // ── User profile ───────────────────────────────────────────────
    let user: Option<UserRow> = sqlx::query_as(
        "SELECT id, email, name, created_at FROM users WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&claims.sub)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?;

    let user = user.ok_or(AppError::NotFound {
        error: "user_not_found",
        message: "User not found".into(),
    })?;

    // ── User roles ─────────────────────────────────────────────────
    let role_ids: Vec<String> = sqlx::query_scalar(
        "SELECT role_id FROM user_roles WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(&claims.sub)
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // ── Active session (most recent non-revoked, non-expired refresh token) ──
    let session: Option<SessionRow> = sqlx::query_as(
        "SELECT id, created_at, expires_at FROM refresh_tokens \
         WHERE user_id = $1 AND tenant_id = $2 AND revoked = false AND expires_at > NOW() \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&claims.sub)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?;

    // ── Build response ─────────────────────────────────────────────
    Ok(Json(serde_json::json!({
        "user": {
            "id": user.id,
            "email": user.email,
            "name": user.name,
            "roles": role_ids,
            "created_at": user.created_at.to_rfc3339(),
        },
        "session": session.map(|s| serde_json::json!({
            "id": s.id,
            "created_at": s.created_at.to_rfc3339(),
            "expires_at": s.expires_at.to_rfc3339(),
        })),
        "tenant": {
            "id": tenant.tenant_id,
            "slug": tenant.slug,
        },
        "token": {
            "issuer": claims.iss,
            "audience": claims.aud,
            "issued_at": claims.iat,
            "expires_at": claims.exp,
            "algorithm": "EdDSA",
        },
    })))
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    email: String,
    name: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
}
