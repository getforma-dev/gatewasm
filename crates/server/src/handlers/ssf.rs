//! SSF (Shared Signals Framework) Stream Management API
//!
//! CRUD endpoints for SSF streams, gated by `caep_enabled` on compliance-plan tenants.
//! Routes: `/manage/ssf/streams/*`

use axum::{
    extract::{FromRequest, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::compliance::audit::{self, AuditAction, RESULT_SUCCESS};
use crate::error::AppError;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// Valid SSF event types (CAEP + SSF core).
pub const SSF_EVENT_TYPES: &[&str] = &[
    "session-revoked",
    "credential-change",
    "token-claims-change",
    "assurance-level-change",
    "device-compliance-change",
    "verification",
];

// ── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateStreamRequest {
    pub receiver_url: Option<String>,
    pub delivery_method: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStreamRequest {
    pub receiver_url: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PollStreamRequest {
    #[serde(default)]
    pub ack: i64,
    pub max_events: Option<i64>,
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

/// Check that CAEP/SSF is enabled for this tenant (compliance plan + caep_enabled).
fn require_caep(tenant: &TenantContext) -> Result<(), AppError> {
    if !tenant.security_config.caep_enabled {
        return Err(AppError::Forbidden {
            error: "caep_not_enabled",
            message: "SSF/CAEP is not enabled for this tenant. Requires compliance plan with caep_enabled.".into(),
        });
    }
    Ok(())
}

/// Generate an SSF shared secret: `ssf_{base64(32 random bytes)}`
fn generate_ssf_secret() -> String {
    use rand::RngCore;
    let mut secret_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret_bytes);
    format!(
        "ssf_{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, secret_bytes)
    )
}

/// Mask a secret: show first 8 chars + `...`
fn mask_secret(secret: &str) -> String {
    if secret.len() > 8 {
        format!("{}...", &secret[..8])
    } else {
        "***".to_string()
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────

/// POST /manage/ssf/streams — Create an SSF stream.
pub async fn create_stream(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    require_caep(&tenant)?;

    let body: CreateStreamRequest =
        axum::Json::<CreateStreamRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: delivery_method, event_types".into(),
            })?
            .0;

    // Validate delivery_method
    if body.delivery_method != "push" && body.delivery_method != "poll" {
        return Err(AppError::BadRequest {
            error: "invalid_delivery_method",
            message: "delivery_method must be 'push' or 'poll'".into(),
        });
    }

    // Push requires a receiver_url
    if body.delivery_method == "push" {
        match &body.receiver_url {
            Some(url) if url.starts_with("https://") || url.starts_with("http://") => {}
            Some(_) => {
                return Err(AppError::BadRequest {
                    error: "invalid_url",
                    message: "receiver_url must start with http:// or https://".into(),
                });
            }
            None => {
                return Err(AppError::BadRequest {
                    error: "missing_receiver_url",
                    message: "receiver_url is required for push delivery method".into(),
                });
            }
        }
    }

    // Validate event_types
    if body.event_types.is_empty() {
        return Err(AppError::BadRequest {
            error: "no_event_types",
            message: "At least one event type is required".into(),
        });
    }
    for et in &body.event_types {
        if !SSF_EVENT_TYPES.contains(&et.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_event_type",
                message: format!(
                    "Unknown event type '{}'. Valid types: {:?}",
                    et, SSF_EVENT_TYPES
                ),
            });
        }
    }

    let shared_secret = generate_ssf_secret();

    let row = sqlx::query(
        "INSERT INTO ssf_streams (tenant_id, receiver_url, delivery_method, event_types, shared_secret, status) \
         VALUES ($1, $2, $3, $4, $5, 'enabled') \
         RETURNING id, created_at, updated_at",
    )
    .bind(&tenant.tenant_id)
    .bind(body.receiver_url.as_deref())
    .bind(&body.delivery_method)
    .bind(&body.event_types)
    .bind(&shared_secret)
    .fetch_one(&state.db)
    .await?;

    let stream_id: uuid::Uuid = row.get("id");
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

    // Audit: ssf.stream.created
    if state.config.compliance.audit_log_enabled {
        audit::log_audit(
            &state.db,
            &tenant.tenant_id,
            None,
            AuditAction::SSF_STREAM_CREATED,
            &format!("ssf/stream/{}", stream_id),
            "0.0.0.0",
            None,
            None,
            RESULT_SUCCESS,
            None,
            &state.config.compliance.mode.to_string(),
        )
        .await;
    }

    tracing::info!(stream_id = %stream_id, tenant = %tenant.tenant_id, "SSF stream created");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "stream": {
                "id": stream_id.to_string(),
                "receiver_url": body.receiver_url,
                "delivery_method": body.delivery_method,
                "event_types": body.event_types,
                "shared_secret": shared_secret,
                "status": "enabled",
                "consecutive_failures": 0,
                "last_delivery_at": null,
                "created_at": created_at.to_rfc3339(),
            }
        })),
    ))
}

/// GET /manage/ssf/streams — List SSF streams for tenant.
pub async fn list_streams(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    require_caep(&tenant)?;

    let rows = sqlx::query(
        "SELECT id, receiver_url, delivery_method, event_types, shared_secret, status, \
         consecutive_failures, last_delivery_at, last_error, created_at, updated_at \
         FROM ssf_streams WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await?;

    let streams: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let secret: String = r.get("shared_secret");
            let event_types: Vec<String> = r.get("event_types");
            serde_json::json!({
                "id": r.get::<uuid::Uuid, _>("id").to_string(),
                "receiver_url": r.get::<Option<String>, _>("receiver_url"),
                "delivery_method": r.get::<String, _>("delivery_method"),
                "event_types": event_types,
                "shared_secret": mask_secret(&secret),
                "status": r.get::<String, _>("status"),
                "consecutive_failures": r.get::<i32, _>("consecutive_failures"),
                "last_delivery_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_delivery_at")
                    .map(|t| t.to_rfc3339()),
                "last_error": r.get::<Option<String>, _>("last_error"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "streams": streams })))
}

/// GET /manage/ssf/streams/:id — Get stream details + health metrics.
pub async fn get_stream(
    State(state): State<AppState>,
    Path(stream_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    require_caep(&tenant)?;

    let stream_uuid = uuid::Uuid::parse_str(&stream_id).map_err(|_| AppError::BadRequest {
        error: "invalid_id",
        message: "Invalid stream ID format".into(),
    })?;

    let row = sqlx::query(
        "SELECT id, receiver_url, delivery_method, event_types, shared_secret, \
         previous_shared_secret, secret_rotated_at, status, \
         consecutive_failures, last_delivery_at, last_error, \
         created_at, updated_at \
         FROM ssf_streams WHERE id = $1 AND tenant_id = $2",
    )
    .bind(stream_uuid)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "stream_not_found",
        message: "SSF stream not found".into(),
    })?;

    let secret: String = row.get("shared_secret");
    let event_types: Vec<String> = row.get("event_types");
    let has_previous_secret: bool = row.get::<Option<String>, _>("previous_shared_secret").is_some();

    Ok(Json(serde_json::json!({
        "stream": {
            "id": row.get::<uuid::Uuid, _>("id").to_string(),
            "receiver_url": row.get::<Option<String>, _>("receiver_url"),
            "delivery_method": row.get::<String, _>("delivery_method"),
            "event_types": event_types,
            "shared_secret": mask_secret(&secret),
            "has_previous_secret": has_previous_secret,
            "secret_rotated_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("secret_rotated_at")
                .map(|t| t.to_rfc3339()),
            "status": row.get::<String, _>("status"),
            "consecutive_failures": row.get::<i32, _>("consecutive_failures"),
            "last_delivery_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_delivery_at")
                .map(|t| t.to_rfc3339()),
            "last_error": row.get::<Option<String>, _>("last_error"),
            "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
        }
    })))
}

/// PATCH /manage/ssf/streams/:id — Update stream (receiver_url, event_types, status).
pub async fn update_stream(
    State(state): State<AppState>,
    Path(stream_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    require_caep(&tenant)?;

    let stream_uuid = uuid::Uuid::parse_str(&stream_id).map_err(|_| AppError::BadRequest {
        error: "invalid_id",
        message: "Invalid stream ID format".into(),
    })?;

    // Verify stream belongs to tenant
    let exists = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM ssf_streams WHERE id = $1 AND tenant_id = $2",
    )
    .bind(stream_uuid)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "stream_not_found",
            message: "SSF stream not found".into(),
        });
    }

    let body: UpdateStreamRequest =
        axum::Json::<UpdateStreamRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body".into(),
            })?
            .0;

    if let Some(ref url) = body.receiver_url {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AppError::BadRequest {
                error: "invalid_url",
                message: "receiver_url must start with http:// or https://".into(),
            });
        }
        sqlx::query("UPDATE ssf_streams SET receiver_url = $1, updated_at = NOW() WHERE id = $2")
            .bind(url)
            .bind(stream_uuid)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref event_types) = body.event_types {
        if event_types.is_empty() {
            return Err(AppError::BadRequest {
                error: "no_event_types",
                message: "At least one event type is required".into(),
            });
        }
        for et in event_types {
            if !SSF_EVENT_TYPES.contains(&et.as_str()) {
                return Err(AppError::BadRequest {
                    error: "invalid_event_type",
                    message: format!("Unknown event type '{}'", et),
                });
            }
        }
        sqlx::query(
            "UPDATE ssf_streams SET event_types = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(event_types)
        .bind(stream_uuid)
        .execute(&state.db)
        .await?;
    }

    if let Some(ref status) = body.status {
        if status != "enabled" && status != "paused" {
            return Err(AppError::BadRequest {
                error: "invalid_status",
                message: "Status must be 'enabled' or 'paused'".into(),
            });
        }
        sqlx::query("UPDATE ssf_streams SET status = $1, updated_at = NOW() WHERE id = $2")
            .bind(status)
            .bind(stream_uuid)
            .execute(&state.db)
            .await?;

        // Audit: ssf.stream.resumed when status is set to enabled (i.e., resuming)
        if status == "enabled" && state.config.compliance.audit_log_enabled {
            audit::log_audit(
                &state.db,
                &tenant.tenant_id,
                None,
                AuditAction::SSF_STREAM_RESUMED,
                &format!("ssf/stream/{}", stream_id),
                "0.0.0.0",
                None,
                None,
                RESULT_SUCCESS,
                None,
                &state.config.compliance.mode.to_string(),
            )
            .await;
        }
    }

    tracing::info!(stream_id = %stream_id, tenant = %tenant.tenant_id, "SSF stream updated");

    Ok(Json(serde_json::json!({ "message": "Stream updated" })))
}

/// DELETE /manage/ssf/streams/:id — Delete stream (cascades to outbox entries).
pub async fn delete_stream(
    State(state): State<AppState>,
    Path(stream_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    require_caep(&tenant)?;

    let stream_uuid = uuid::Uuid::parse_str(&stream_id).map_err(|_| AppError::BadRequest {
        error: "invalid_id",
        message: "Invalid stream ID format".into(),
    })?;

    let result = sqlx::query("DELETE FROM ssf_streams WHERE id = $1 AND tenant_id = $2")
        .bind(stream_uuid)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "stream_not_found",
            message: "SSF stream not found".into(),
        });
    }

    tracing::info!(stream_id = %stream_id, tenant = %tenant.tenant_id, "SSF stream deleted");

    Ok(Json(serde_json::json!({ "message": "Stream deleted" })))
}

/// POST /manage/ssf/streams/:id/verify — Send a verification event.
pub async fn verify_stream(
    State(state): State<AppState>,
    Path(stream_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    require_caep(&tenant)?;

    let stream_uuid = uuid::Uuid::parse_str(&stream_id).map_err(|_| AppError::BadRequest {
        error: "invalid_id",
        message: "Invalid stream ID format".into(),
    })?;

    // Verify stream belongs to tenant and is enabled
    let row = sqlx::query("SELECT id, status FROM ssf_streams WHERE id = $1 AND tenant_id = $2")
        .bind(stream_uuid)
        .bind(&tenant.tenant_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound {
            error: "stream_not_found",
            message: "SSF stream not found".into(),
        })?;

    let status: String = row.get("status");
    if status != "enabled" {
        return Err(AppError::BadRequest {
            error: "stream_not_enabled",
            message: "Stream must be enabled to send verification events".into(),
        });
    }

    // Get next sequence number
    let seq: i64 =
        sqlx::query_scalar("SELECT nextval('ssf_sequence_seq')")
            .fetch_one(&state.db)
            .await?;

    // Build a verification SET (Security Event Token) payload
    let set_payload = serde_json::json!({
        "iss": tenant.tenant_id,
        "iat": chrono::Utc::now().timestamp(),
        "jti": uuid::Uuid::new_v4().to_string(),
        "events": {
            "urn:ietf:params:ssf:event-type:verification": {
                "state": uuid::Uuid::new_v4().to_string()
            }
        }
    });

    // Insert into outbox
    sqlx::query(
        "INSERT INTO ssf_outbox (tenant_id, stream_id, sequence_number, event_type, set_token, status) \
         VALUES ($1, $2, $3, $4, $5, 'pending')",
    )
    .bind(&tenant.tenant_id)
    .bind(stream_uuid)
    .bind(seq)
    .bind("urn:ietf:params:ssf:event-type:verification")
    .bind(set_payload.to_string())
    .execute(&state.db)
    .await?;

    tracing::info!(stream_id = %stream_id, sequence = seq, "SSF verification event queued");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "message": "Verification event queued",
            "sequence_number": seq,
        })),
    ))
}

/// POST /manage/ssf/streams/:id/rotate-secret — Rotate HMAC secret.
pub async fn rotate_secret(
    State(state): State<AppState>,
    Path(stream_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    require_caep(&tenant)?;

    let stream_uuid = uuid::Uuid::parse_str(&stream_id).map_err(|_| AppError::BadRequest {
        error: "invalid_id",
        message: "Invalid stream ID format".into(),
    })?;

    // Get current secret
    let row = sqlx::query(
        "SELECT id, shared_secret FROM ssf_streams WHERE id = $1 AND tenant_id = $2",
    )
    .bind(stream_uuid)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "stream_not_found",
        message: "SSF stream not found".into(),
    })?;

    let current_secret: String = row.get("shared_secret");
    let new_secret = generate_ssf_secret();

    // Move current to previous, set new secret, mark rotation time
    sqlx::query(
        "UPDATE ssf_streams SET \
         previous_shared_secret = $1, \
         shared_secret = $2, \
         secret_rotated_at = NOW(), \
         updated_at = NOW() \
         WHERE id = $3",
    )
    .bind(&current_secret)
    .bind(&new_secret)
    .bind(stream_uuid)
    .execute(&state.db)
    .await?;

    tracing::info!(stream_id = %stream_id, tenant = %tenant.tenant_id, "SSF stream secret rotated");

    Ok(Json(serde_json::json!({
        "shared_secret": new_secret,
        "previous_shared_secret": mask_secret(&current_secret),
        "secret_rotated_at": chrono::Utc::now().to_rfc3339(),
        "grace_period_hours": 48,
    })))
}

/// POST /manage/ssf/streams/:id/poll — Poll for events (poll-based streams only).
pub async fn poll_stream(
    State(state): State<AppState>,
    Path(stream_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    require_caep(&tenant)?;

    let stream_uuid = uuid::Uuid::parse_str(&stream_id).map_err(|_| AppError::BadRequest {
        error: "invalid_id",
        message: "Invalid stream ID format".into(),
    })?;

    let body: PollStreamRequest =
        axum::Json::<PollStreamRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Accepts: ack (number), max_events (number)".into(),
            })?
            .0;

    // Verify stream belongs to tenant and check delivery_method
    let row = sqlx::query(
        "SELECT id, delivery_method, last_ack_sequence \
         FROM ssf_streams WHERE id = $1 AND tenant_id = $2",
    )
    .bind(stream_uuid)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "stream_not_found",
        message: "SSF stream not found".into(),
    })?;

    let delivery_method: String = row.get("delivery_method");
    if delivery_method != "poll" {
        return Err(AppError::BadRequest {
            error: "not_poll_stream",
            message: "Poll endpoint is only available for streams with delivery_method 'poll'".into(),
        });
    }

    let mut last_ack: i64 = row.get("last_ack_sequence");

    // Clamp max_events to [1, 1000], default 100
    let max_events = body.max_events.unwrap_or(100).max(1).min(1000);

    // Advance ack if requested ack is greater than current
    if body.ack > last_ack {
        sqlx::query(
            "UPDATE ssf_streams SET last_ack_sequence = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(body.ack)
        .bind(stream_uuid)
        .execute(&state.db)
        .await?;

        // Audit: ssf.poll.ack
        if state.config.compliance.audit_log_enabled {
            audit::log_audit(
                &state.db,
                &tenant.tenant_id,
                None,
                AuditAction::SSF_POLL_ACK,
                &format!("ssf/stream/{}", stream_id),
                "0.0.0.0",
                None,
                None,
                RESULT_SUCCESS,
                None,
                &state.config.compliance.mode.to_string(),
            )
            .await;
        }

        last_ack = body.ack;
    }

    // Fetch outbox entries after last_ack_sequence, ordered by sequence_number
    // Fetch one extra to determine more_available
    let rows = sqlx::query(
        "SELECT sequence_number, set_token FROM ssf_outbox \
         WHERE stream_id = $1 AND sequence_number > $2 \
         ORDER BY sequence_number ASC \
         LIMIT $3",
    )
    .bind(stream_uuid)
    .bind(last_ack)
    .bind(max_events + 1) // +1 to detect more_available
    .fetch_all(&state.db)
    .await?;

    let more_available = rows.len() as i64 > max_events;
    let take_count = if more_available {
        max_events as usize
    } else {
        rows.len()
    };

    let sets: Vec<String> = rows[..take_count]
        .iter()
        .map(|r| r.get::<String, _>("set_token"))
        .collect();

    tracing::info!(
        stream_id = %stream_id,
        tenant = %tenant.tenant_id,
        ack = body.ack,
        returned = sets.len(),
        more = more_available,
        "SSF poll"
    );

    Ok(Json(serde_json::json!({
        "sets": sets,
        "more_available": more_available,
    })))
}
