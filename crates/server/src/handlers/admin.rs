use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::error::AppError;
use crate::jwt::signing;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn tenant_id_from_claims(req: &axum::extract::Request) -> Result<String, AppError> {
    let claims = req
        .extensions()
        .get::<signing::Claims>()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;
    claims.tenant_id.clone().ok_or(AppError::BadRequest {
        error: "no_tenant",
        message: "User has no tenant".into(),
    })
}

/// GET /admin/stats — Tenant statistics for admin dashboard.
pub async fn stats(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant_id = tenant_id_from_claims(&req)?;

    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
        .bind(&tenant_id)
        .fetch_one(&state.db)
        .await?;

    let verified_users: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE tenant_id = $1 AND email_verified = TRUE",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    let active_sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE tenant_id = $1 AND revoked = FALSE AND expires_at > NOW()",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    let blocked_users: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE tenant_id = $1 AND locked_until > NOW()",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    let logins_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM login_attempts WHERE user_id IN (SELECT id FROM users WHERE tenant_id = $1) AND success = TRUE AND created_at > NOW() - INTERVAL '24 hours'",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    let failed_logins_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM login_attempts WHERE user_id IN (SELECT id FROM users WHERE tenant_id = $1) AND success = FALSE AND created_at > NOW() - INTERVAL '24 hours'",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    let providers: Vec<(String, i64)> = sqlx::query_as(
        "SELECT c.provider, COUNT(*) FROM credentials c JOIN users u ON u.id = c.user_id WHERE u.tenant_id = $1 GROUP BY c.provider",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let provider_breakdown: serde_json::Value = providers
        .into_iter()
        .map(|(p, c)| (p, serde_json::Value::Number(c.into())))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    Ok(Json(serde_json::json!({
        "total_users": total_users,
        "verified_users": verified_users,
        "blocked_users": blocked_users,
        "active_sessions": active_sessions,
        "logins_24h": logins_24h,
        "failed_logins_24h": failed_logins_24h,
        "providers": provider_breakdown,
    })))
}

/// GET /admin/audit-log — Login attempts for the admin's tenant.
pub async fn audit_log(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant_id = tenant_id_from_claims(&req)?;

    let params: PaginationQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(PaginationQuery {
            limit: None,
            offset: None,
        }))
        .0;

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let rows = sqlx::query_as::<_, (String, String, Option<String>, bool, Option<String>, Option<String>, Option<String>, chrono::DateTime<Utc>)>(
        "SELECT la.id, la.email, la.user_id, la.success, la.ip_address, la.user_agent, la.failure_reason, la.created_at
         FROM login_attempts la
         WHERE la.user_id IN (SELECT id FROM users WHERE tenant_id = $1)
         ORDER BY la.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(&tenant_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(id, email, user_id, success, ip, ua, reason, created)| {
            serde_json::json!({
                "id": id,
                "email": email,
                "user_id": user_id,
                "success": success,
                "ip_address": ip,
                "user_agent": ua,
                "failure_reason": reason,
                "created_at": created.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "audit_log": items,
        "limit": limit,
        "offset": offset,
    })))
}
