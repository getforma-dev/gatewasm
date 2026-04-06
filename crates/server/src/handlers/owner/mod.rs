mod analytics;
mod billing;
mod customers;
mod dashboard;
mod invites;
mod team;

pub use analytics::*;
pub use billing::*;
pub use customers::*;
pub use dashboard::*;
pub use invites::*;
pub use team::*;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::AppError;
use crate::jwt::signing;
use crate::AppState;

// ── Shared helpers used across multiple submodules ───────────────────────

#[derive(Debug, Deserialize)]
pub(super) struct OwnerAnalyticsQuery {
    pub days: Option<i64>,
}

pub(crate) fn clamp_limit(limit: Option<i64>, default_limit: i64, max_limit: i64) -> i64 {
    let value = limit.unwrap_or(default_limit);
    value.clamp(1, max_limit)
}

pub(crate) fn parse_rfc3339_opt(
    raw: &Option<String>,
    field: &'static str,
) -> Result<Option<DateTime<Utc>>, AppError> {
    match raw {
        Some(value) if !value.trim().is_empty() => DateTime::parse_from_rfc3339(value.trim())
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
            .map_err(|_| AppError::BadRequest {
                error: "invalid_datetime",
                message: format!("Invalid RFC3339 datetime for '{}'", field),
            }),
        _ => Ok(None),
    }
}

pub(crate) fn normalize_optional_field(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn owner_claims(req: &axum::extract::Request) -> Result<signing::Claims, AppError> {
    req.extensions()
        .get::<signing::Claims>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })
}

pub(crate) fn request_id_from_req(req: &axum::extract::Request) -> Option<String> {
    req.headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) struct OwnerEvent<'a> {
    pub customer_user_id: Option<&'a str>,
    pub tenant_id: Option<&'a str>,
    pub actor_user_id: Option<&'a str>,
    pub actor_email: Option<&'a str>,
    pub action: &'a str,
    pub resource: &'a str,
    pub request_id: Option<&'a str>,
    pub details: serde_json::Value,
}

pub(crate) async fn record_owner_event(state: &AppState, event: OwnerEvent<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO owner_timeline_events
         (id, customer_user_id, tenant_id, actor_user_id, actor_email, action, resource, request_id, details, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())",
    )
    .bind(format!("oev_{}", uuid::Uuid::new_v4()))
    .bind(event.customer_user_id)
    .bind(event.tenant_id)
    .bind(event.actor_user_id)
    .bind(event.actor_email)
    .bind(event.action)
    .bind(event.resource)
    .bind(event.request_id)
    .bind(event.details)
    .execute(&state.db)
    .await?;

    Ok(())
}
