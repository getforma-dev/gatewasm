use axum::{
    extract::{FromRequest, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::error::AppError;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// Reject URLs targeting private/internal network addresses (SSRF prevention).
pub fn is_private_url(raw_url: &str) -> bool {
    if let Ok(parsed) = url::Url::parse(raw_url) {
        if let Some(host) = parsed.host_str() {
            let h = host.to_lowercase();
            return h == "localhost"
                || h == "127.0.0.1"
                || h == "0.0.0.0"
                || h == "::1"
                || h == "[::1]"
                || h.starts_with("10.")
                || h.starts_with("172.16.")
                || h.starts_with("172.17.")
                || h.starts_with("172.18.")
                || h.starts_with("172.19.")
                || h.starts_with("172.20.")
                || h.starts_with("172.21.")
                || h.starts_with("172.22.")
                || h.starts_with("172.23.")
                || h.starts_with("172.24.")
                || h.starts_with("172.25.")
                || h.starts_with("172.26.")
                || h.starts_with("172.27.")
                || h.starts_with("172.28.")
                || h.starts_with("172.29.")
                || h.starts_with("172.30.")
                || h.starts_with("172.31.")
                || h.starts_with("192.168.")
                || h == "169.254.169.254"
                || h.starts_with("169.254.")
                // IPv6 private ranges
                || h.starts_with("fc")   // fc00::/7 unique local
                || h.starts_with("fd")   // fd00::/8 unique local
                || h.starts_with("fe80") // fe80::/10 link-local
                || h.ends_with(".internal")
                || h.ends_with(".local");
        }
    }
    false
}

/// Valid webhook event types.
pub const WEBHOOK_EVENTS: &[&str] = &[
    "user.registered",
    "user.logged_in",
    "user.logged_out",
    "user.deleted",
    "user.email_verified",
    "user.password_reset",
    "user.role_changed",
];

// ── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub url: String,
    pub events: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWebhookRequest {
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    pub description: Option<String>,
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct DeliveriesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Handlers ─────────────────────────────────────────────────────────────

/// GET /manage/webhooks — List all webhooks for this tenant.
pub async fn list_webhooks(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let rows = sqlx::query(
        "SELECT id, url, events, active, description, created_at, updated_at \
         FROM webhooks WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await?;

    let webhooks: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "url": r.get::<String, _>("url"),
                "events": r.get::<serde_json::Value, _>("events"),
                "active": r.get::<bool, _>("active"),
                "description": r.get::<Option<String>, _>("description"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "webhooks": webhooks })))
}

/// POST /manage/webhooks — Create a new webhook.
pub async fn create_webhook(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let body: CreateWebhookRequest = axum::Json::<CreateWebhookRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body. Required: url, events".into(),
        })?
        .0;

    // Validate URL
    if !body.url.starts_with("http://") && !body.url.starts_with("https://") {
        return Err(AppError::BadRequest {
            error: "invalid_url",
            message: "Webhook URL must start with http:// or https://".into(),
        });
    }
    if is_private_url(&body.url) {
        return Err(AppError::BadRequest {
            error: "invalid_url",
            message: "URL must not target private/internal addresses".into(),
        });
    }

    // Validate events
    for ev in &body.events {
        if !WEBHOOK_EVENTS.contains(&ev.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_event",
                message: format!(
                    "Unknown event type '{}'. Valid events: {:?}",
                    ev, WEBHOOK_EVENTS
                ),
            });
        }
    }

    if body.events.is_empty() {
        return Err(AppError::BadRequest {
            error: "no_events",
            message: "At least one event type is required".into(),
        });
    }

    let webhook_id = format!("wh_{}", uuid::Uuid::new_v4());

    // Generate webhook secret: whsec_{base64(32 random bytes)}
    use rand::RngCore;
    let mut secret_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret_bytes);
    let secret = format!(
        "whsec_{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, secret_bytes)
    );

    let events_json = serde_json::to_value(&body.events).unwrap_or_default();

    sqlx::query(
        "INSERT INTO webhooks (id, tenant_id, url, secret, events, active, description) \
         VALUES ($1, $2, $3, $4, $5, TRUE, $6)",
    )
    .bind(&webhook_id)
    .bind(&tenant.tenant_id)
    .bind(&body.url)
    .bind(&secret)
    .bind(&events_json)
    .bind(body.description.as_deref().unwrap_or(""))
    .execute(&state.db)
    .await?;

    tracing::info!(webhook_id = %webhook_id, tenant = %tenant.tenant_id, "Webhook created");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "webhook": {
                "id": webhook_id,
                "url": body.url,
                "events": body.events,
                "active": true,
                "description": body.description,
                "secret": secret,
            }
        })),
    ))
}

/// GET /manage/webhooks/{id} — Get webhook details (secret masked).
pub async fn get_webhook(
    State(state): State<AppState>,
    Path(webhook_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let row = sqlx::query(
        "SELECT id, url, secret, events, active, description, created_at, updated_at \
         FROM webhooks WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&webhook_id)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "webhook_not_found",
        message: "Webhook not found".into(),
    })?;

    let secret: String = row.get("secret");
    let masked = if secret.len() > 10 {
        format!("{}...{}", &secret[..10], &secret[secret.len() - 4..])
    } else {
        "***".to_string()
    };

    Ok(Json(serde_json::json!({
        "webhook": {
            "id": row.get::<String, _>("id"),
            "url": row.get::<String, _>("url"),
            "secret": masked,
            "events": row.get::<serde_json::Value, _>("events"),
            "active": row.get::<bool, _>("active"),
            "description": row.get::<Option<String>, _>("description"),
            "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
        }
    })))
}

/// PATCH /manage/webhooks/{id} — Update webhook.
pub async fn update_webhook(
    State(state): State<AppState>,
    Path(webhook_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    // Verify webhook belongs to tenant
    let exists =
        sqlx::query_scalar::<_, String>("SELECT id FROM webhooks WHERE id = $1 AND tenant_id = $2")
            .bind(&webhook_id)
            .bind(&tenant.tenant_id)
            .fetch_optional(&state.db)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "webhook_not_found",
            message: "Webhook not found".into(),
        });
    }

    let body: UpdateWebhookRequest = axum::Json::<UpdateWebhookRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    if let Some(ref url) = body.url {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AppError::BadRequest {
                error: "invalid_url",
                message: "Webhook URL must start with http:// or https://".into(),
            });
        }
        if is_private_url(url) {
            return Err(AppError::BadRequest {
                error: "invalid_url",
                message: "URL must not target private/internal addresses".into(),
            });
        }
        sqlx::query("UPDATE webhooks SET url = $1, updated_at = NOW() WHERE id = $2")
            .bind(url)
            .bind(&webhook_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref events) = body.events {
        for ev in events {
            if !WEBHOOK_EVENTS.contains(&ev.as_str()) {
                return Err(AppError::BadRequest {
                    error: "invalid_event",
                    message: format!("Unknown event type '{}'", ev),
                });
            }
        }
        let json = serde_json::to_value(events).unwrap_or_default();
        sqlx::query("UPDATE webhooks SET events = $1, updated_at = NOW() WHERE id = $2")
            .bind(&json)
            .bind(&webhook_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref desc) = body.description {
        sqlx::query("UPDATE webhooks SET description = $1, updated_at = NOW() WHERE id = $2")
            .bind(desc)
            .bind(&webhook_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(active) = body.active {
        sqlx::query("UPDATE webhooks SET active = $1, updated_at = NOW() WHERE id = $2")
            .bind(active)
            .bind(&webhook_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(serde_json::json!({ "message": "Webhook updated" })))
}

/// DELETE /manage/webhooks/{id} — Delete webhook + deliveries.
pub async fn delete_webhook(
    State(state): State<AppState>,
    Path(webhook_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let result = sqlx::query("DELETE FROM webhooks WHERE id = $1 AND tenant_id = $2")
        .bind(&webhook_id)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "webhook_not_found",
            message: "Webhook not found".into(),
        });
    }

    tracing::info!(webhook_id = %webhook_id, tenant = %tenant.tenant_id, "Webhook deleted");

    Ok(Json(serde_json::json!({ "message": "Webhook deleted" })))
}

/// GET /manage/webhooks/{id}/deliveries — Delivery history (paginated).
pub async fn list_deliveries(
    State(state): State<AppState>,
    Path(webhook_id): Path<String>,
    Query(query): Query<DeliveriesQuery>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    // Verify webhook belongs to tenant
    let exists =
        sqlx::query_scalar::<_, String>("SELECT id FROM webhooks WHERE id = $1 AND tenant_id = $2")
            .bind(&webhook_id)
            .bind(&tenant.tenant_id)
            .fetch_optional(&state.db)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "webhook_not_found",
            message: "Webhook not found".into(),
        });
    }

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let rows = sqlx::query(
        "SELECT id, event_type, payload, response_status, response_body, attempt, max_attempts, \
         status, delivered_at, created_at \
         FROM webhook_deliveries WHERE webhook_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(&webhook_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let deliveries: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "event_type": r.get::<String, _>("event_type"),
                "payload": r.get::<serde_json::Value, _>("payload"),
                "response_status": r.get::<Option<i32>, _>("response_status"),
                "response_body": r.get::<Option<String>, _>("response_body"),
                "attempt": r.get::<i32, _>("attempt"),
                "max_attempts": r.get::<i32, _>("max_attempts"),
                "status": r.get::<String, _>("status"),
                "delivered_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("delivered_at")
                    .map(|t| t.to_rfc3339()),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "deliveries": deliveries })))
}

/// POST /manage/webhooks/{id}/test — Send a test event.
pub async fn test_webhook(
    State(state): State<AppState>,
    Path(webhook_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let row = sqlx::query(
        "SELECT id, url, secret, events FROM webhooks WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&webhook_id)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "webhook_not_found",
        message: "Webhook not found".into(),
    })?;

    let url: String = row.get("url");
    let secret: String = row.get("secret");

    let payload = serde_json::json!({
        "event": "test",
        "tenant_id": tenant.tenant_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "data": {
            "message": "This is a test webhook delivery from GateWASM"
        }
    });

    let delivery_id = format!("whd_{}", uuid::Uuid::new_v4());

    // Attempt delivery
    let result = super::webhook_engine::deliver_single(&url, &secret, &payload).await;

    let (status_str, response_status, response_body) = match result {
        Ok((code, body)) => ("delivered".to_string(), Some(code as i32), Some(body)),
        Err(e) => ("failed".to_string(), None, Some(e.to_string())),
    };

    sqlx::query(
        "INSERT INTO webhook_deliveries (id, webhook_id, event_type, payload, response_status, response_body, status, delivered_at, attempt) \
         VALUES ($1, $2, 'test', $3, $4, $5, $6, NOW(), 1)",
    )
    .bind(&delivery_id)
    .bind(&webhook_id)
    .bind(&payload)
    .bind(response_status)
    .bind(response_body.as_deref())
    .bind(&status_str)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "delivery_id": delivery_id,
            "status": status_str,
            "response_status": response_status,
        })),
    ))
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn tenant_from_req(req: &axum::extract::Request) -> Result<TenantContext, AppError> {
    req.extensions()
        .get::<TenantContext>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Missing tenant context".into(),
        })
}
