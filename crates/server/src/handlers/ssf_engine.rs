//! SSF Event Engine — Outbox pattern with LISTEN/NOTIFY for sub-second push delivery.
//!
//! - `emit_ssf_event()`: writes a signed SET (Security Event Token) to the outbox
//! - `process_ssf_outbox()`: background worker delivers pending events via HTTP push
//! - `run_ssf_worker()`: spawned in main.rs, wakes on pg_notify or 1s fallback

use hmac::{Hmac, Mac};
use jsonwebtoken::{encode, Algorithm, Header};
use serde::Serialize;
use sha2::Sha256;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::compliance::audit::{self, AuditAction, RESULT_FAILURE, RESULT_SUCCESS};
use crate::jwt::keys::KeyPair;

type HmacSha256 = Hmac<Sha256>;

/// Maximum concurrent SSF push deliveries.
static SSF_SEMAPHORE: std::sync::LazyLock<Arc<Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(Semaphore::new(10)));

/// Exponential backoff schedule (in seconds) for push delivery retries.
/// Index = attempt number (0-based after first failure).
const BACKOFF_SECS: &[i64] = &[60, 300, 1800, 7200, 43200, 86400, 86400, 86400];

/// Max delivery attempts before marking as failed.
const MAX_ATTEMPTS: i32 = 8;

/// Consecutive failures threshold to pause a stream.
const PAUSE_THRESHOLD: i32 = 5;

/// Consecutive failures threshold to disable a stream.
const DISABLE_THRESHOLD: i32 = 50;

// ── SET (Security Event Token) claims ────────────────────────────────────

#[derive(Debug, Serialize)]
struct SetClaims {
    iss: String,
    iat: i64,
    jti: String,
    events: serde_json::Value,
    seq_num: i64,
}

// ── emit_ssf_event ───────────────────────────────────────────────────────

/// Write an SSF event to the outbox for every active stream that subscribes to `event_type`.
///
/// The SET is a JWT signed with Ed25519. After insertion, fires `pg_notify` for
/// sub-second push delivery by the background worker.
pub async fn emit_ssf_event(
    db: &PgPool,
    tenant_id: &str,
    event_type: &str,
    subject: &serde_json::Value,
    keys: &KeyPair,
) -> Result<(), sqlx::Error> {
    // Find active streams for this tenant that include this event_type
    let rows = sqlx::query_as::<_, (uuid::Uuid, String)>(
        "SELECT id, tenant_id FROM ssf_streams \
         WHERE tenant_id = $1 AND status = 'enabled' AND $2 = ANY(event_types)",
    )
    .bind(tenant_id)
    .bind(event_type)
    .fetch_all(db)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    for (stream_id, _stream_tenant_id) in &rows {
        // Get next sequence number
        let seq: i64 = sqlx::query_scalar("SELECT nextval('ssf_sequence_seq')")
            .fetch_one(db)
            .await?;

        // Build the SET (Security Event Token)
        let event_uri = format!("urn:ietf:params:ssf:event-type:{}", event_type);
        let claims = SetClaims {
            iss: tenant_id.to_string(),
            iat: chrono::Utc::now().timestamp(),
            jti: uuid::Uuid::new_v4().to_string(),
            events: serde_json::json!({
                event_uri: {
                    "subject": subject
                }
            }),
            seq_num: seq,
        };

        // Sign with Ed25519 (same key used for access tokens)
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(keys.kid.clone());
        header.typ = Some("secevent+jwt".to_string());

        let set_token = match encode(&header, &claims, &keys.encoding_key) {
            Ok(token) => token,
            Err(e) => {
                tracing::error!(error = %e, stream_id = %stream_id, "Failed to sign SET token");
                continue;
            }
        };

        // Insert into outbox
        sqlx::query(
            "INSERT INTO ssf_outbox \
             (tenant_id, stream_id, sequence_number, event_type, set_token, status) \
             VALUES ($1, $2, $3, $4, $5, 'pending')",
        )
        .bind(tenant_id)
        .bind(stream_id)
        .bind(seq)
        .bind(event_type)
        .bind(&set_token)
        .execute(db)
        .await?;

        // Fire LISTEN/NOTIFY for sub-second wakeup
        let _ = sqlx::query("SELECT pg_notify('ssf_new_event', $1)")
            .bind(stream_id.to_string())
            .execute(db)
            .await;

        // Audit: ssf.event.created
        audit::log_audit(
            db,
            tenant_id,
            None,
            AuditAction::SSF_EVENT_CREATED,
            &format!("ssf/stream/{}", stream_id),
            "system",
            None,
            None,
            RESULT_SUCCESS,
            None,
            "standard",
        )
        .await;

        tracing::info!(
            stream_id = %stream_id,
            event_type = %event_type,
            sequence = seq,
            "SSF event queued in outbox"
        );
    }

    Ok(())
}

// ── process_ssf_outbox ───────────────────────────────────────────────────

/// Process pending outbox entries: deliver push events, skip poll events.
pub async fn process_ssf_outbox(db: &PgPool) {
    // Fetch pending outbox entries ready for delivery
    let rows = match sqlx::query_as::<_, (i64, uuid::Uuid, String, String, i32)>(
        "SELECT o.id, o.stream_id, o.set_token, o.event_type, o.attempts \
         FROM ssf_outbox o \
         WHERE o.status = 'pending' AND o.next_attempt_at <= NOW() \
         ORDER BY o.next_attempt_at ASC \
         LIMIT 50",
    )
    .fetch_all(db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "Failed to fetch SSF outbox entries");
            return;
        }
    };

    if rows.is_empty() {
        return;
    }

    for (outbox_id, stream_id, set_token, _event_type, attempts) in rows {
        // Look up the stream's delivery method, receiver_url, and shared_secret
        let stream_info = match sqlx::query_as::<_, (String, Option<String>, String, String)>(
            "SELECT delivery_method, receiver_url, shared_secret, status \
             FROM ssf_streams WHERE id = $1",
        )
        .bind(stream_id)
        .fetch_optional(db)
        .await
        {
            Ok(Some(info)) => info,
            Ok(None) => {
                // Stream was deleted — mark outbox entry as failed
                let _ = sqlx::query(
                    "UPDATE ssf_outbox SET status = 'failed', last_error = 'stream deleted' WHERE id = $1",
                )
                .bind(outbox_id)
                .execute(db)
                .await;
                continue;
            }
            Err(e) => {
                tracing::error!(error = %e, outbox_id = outbox_id, "Failed to fetch stream info");
                continue;
            }
        };

        let (delivery_method, receiver_url, shared_secret, stream_status) = stream_info;

        // Skip poll streams — they are delivered on-demand
        if delivery_method == "poll" {
            continue;
        }

        // Skip if stream is not enabled
        if stream_status != "enabled" {
            continue;
        }

        let receiver_url = match receiver_url {
            Some(url) => url,
            None => {
                let _ = sqlx::query(
                    "UPDATE ssf_outbox SET status = 'failed', last_error = 'no receiver_url' WHERE id = $1",
                )
                .bind(outbox_id)
                .execute(db)
                .await;
                continue;
            }
        };

        // Deliver via push
        let db = db.clone();
        let sem = SSF_SEMAPHORE.clone();

        tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let result = deliver_push(&receiver_url, &shared_secret, &set_token).await;

            match result {
                Ok(status_code) if (200..300).contains(&status_code) => {
                    // Success — mark delivered, reset stream failures
                    let _ = sqlx::query(
                        "UPDATE ssf_outbox SET status = 'delivered', delivered_at = NOW(), \
                         attempts = $1 WHERE id = $2",
                    )
                    .bind(attempts + 1)
                    .bind(outbox_id)
                    .execute(&db)
                    .await;

                    let _ = sqlx::query(
                        "UPDATE ssf_streams SET consecutive_failures = 0, \
                         last_delivery_at = NOW(), last_error = NULL, updated_at = NOW() \
                         WHERE id = $1",
                    )
                    .bind(stream_id)
                    .execute(&db)
                    .await;

                    // Audit: ssf.event.delivered
                    audit::log_audit(
                        &db,
                        "system",
                        None,
                        AuditAction::SSF_EVENT_DELIVERED,
                        &format!("ssf/stream/{}", stream_id),
                        "system",
                        None,
                        None,
                        RESULT_SUCCESS,
                        None,
                        "standard",
                    )
                    .await;

                    tracing::info!(outbox_id = outbox_id, stream_id = %stream_id, "SSF event delivered");
                }
                Ok(status_code) => {
                    // HTTP error response
                    handle_delivery_failure(
                        &db,
                        outbox_id,
                        stream_id,
                        attempts,
                        &format!("HTTP {}", status_code),
                    )
                    .await;
                }
                Err(e) => {
                    // Network/timeout error
                    handle_delivery_failure(
                        &db, outbox_id, stream_id, attempts, &e,
                    )
                    .await;
                }
            }
        });
    }
}

/// Handle a failed delivery: exponential backoff, stream health tracking.
async fn handle_delivery_failure(
    db: &PgPool,
    outbox_id: i64,
    stream_id: uuid::Uuid,
    attempts: i32,
    error_msg: &str,
) {
    let new_attempts = attempts + 1;

    if new_attempts >= MAX_ATTEMPTS {
        // Max retries exhausted — mark failed
        let _ = sqlx::query(
            "UPDATE ssf_outbox SET status = 'failed', attempts = $1, \
             last_error = $2 WHERE id = $3",
        )
        .bind(new_attempts)
        .bind(error_msg)
        .bind(outbox_id)
        .execute(db)
        .await;

        // Audit: ssf.event.failed (permanent failure)
        audit::log_audit(
            db,
            "system",
            None,
            AuditAction::SSF_EVENT_FAILED,
            &format!("ssf/stream/{}", stream_id),
            "system",
            None,
            None,
            RESULT_FAILURE,
            Some(error_msg),
            "standard",
        )
        .await;
    } else {
        // Schedule retry with exponential backoff
        let backoff_idx = (new_attempts - 1).min(BACKOFF_SECS.len() as i32 - 1) as usize;
        let delay_secs = BACKOFF_SECS[backoff_idx];
        let next_attempt = chrono::Utc::now() + chrono::Duration::seconds(delay_secs);

        let _ = sqlx::query(
            "UPDATE ssf_outbox SET attempts = $1, next_attempt_at = $2, \
             last_error = $3 WHERE id = $4",
        )
        .bind(new_attempts)
        .bind(next_attempt)
        .bind(error_msg)
        .bind(outbox_id)
        .execute(db)
        .await;
    }

    // Increment consecutive failures on the stream
    let _ = sqlx::query(
        "UPDATE ssf_streams SET consecutive_failures = consecutive_failures + 1, \
         last_error = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(error_msg)
    .bind(stream_id)
    .execute(db)
    .await;

    // Check stream health: pause at 5+ failures, disable at 50+
    let failures: Option<i32> = sqlx::query_scalar(
        "SELECT consecutive_failures FROM ssf_streams WHERE id = $1",
    )
    .bind(stream_id)
    .fetch_optional(db)
    .await
    .unwrap_or(None);

    if let Some(f) = failures {
        if f >= DISABLE_THRESHOLD {
            let _ = sqlx::query(
                "UPDATE ssf_streams SET status = 'disabled', updated_at = NOW() WHERE id = $1",
            )
            .bind(stream_id)
            .execute(db)
            .await;

            // Audit: ssf.stream.disabled
            audit::log_audit(
                db,
                "system",
                None,
                AuditAction::SSF_STREAM_DISABLED,
                &format!("ssf/stream/{}", stream_id),
                "system",
                None,
                None,
                RESULT_FAILURE,
                Some(&format!("{} consecutive failures", f)),
                "standard",
            )
            .await;

            tracing::warn!(stream_id = %stream_id, failures = f, "SSF stream disabled due to excessive failures");
        } else if f >= PAUSE_THRESHOLD {
            let _ = sqlx::query(
                "UPDATE ssf_streams SET status = 'paused', updated_at = NOW() WHERE id = $1 AND status = 'enabled'",
            )
            .bind(stream_id)
            .execute(db)
            .await;

            // Audit: ssf.stream.paused
            audit::log_audit(
                db,
                "system",
                None,
                AuditAction::SSF_STREAM_PAUSED,
                &format!("ssf/stream/{}", stream_id),
                "system",
                None,
                None,
                RESULT_FAILURE,
                Some(&format!("{} consecutive failures", f)),
                "standard",
            )
            .await;

            tracing::warn!(stream_id = %stream_id, failures = f, "SSF stream paused due to consecutive failures");
        }
    }

    tracing::warn!(
        outbox_id = outbox_id,
        stream_id = %stream_id,
        attempts = new_attempts,
        error = %error_msg,
        "SSF push delivery failed"
    );
}

// ── Push delivery with HMAC-SHA256 ──────────────────────────────────────

/// Deliver a SET token via HTTP POST with HMAC-SHA256 signature.
/// Returns the HTTP status code on success.
async fn deliver_push(
    url: &str,
    shared_secret: &str,
    set_token: &str,
) -> Result<u16, String> {
    // Compute HMAC-SHA256 of the SET token body
    let mut mac = HmacSha256::new_from_slice(shared_secret.as_bytes())
        .map_err(|e| format!("HMAC init error: {}", e))?;
    hmac::Mac::update(&mut mac, set_token.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let res = client
        .post(url)
        .header("Content-Type", "application/secevent+jwt")
        .header("X-SSF-Signature", format!("sha256={}", signature))
        .header("User-Agent", "fAuth-SSF/1.0")
        .body(set_token.to_string())
        .send()
        .await
        .map_err(|e| format!("HTTP request error: {}", e))?;

    Ok(res.status().as_u16())
}

// ── Background worker ───────────────────────────────────────────────────

/// Run the SSF background worker. Wakes on LISTEN/NOTIFY or 1-second fallback poll.
/// Also cleans up expired rotated secrets.
pub async fn run_ssf_worker(db: PgPool, keys: KeyPair) {
    // Set up PgListener for ssf_new_event channel
    let mut listener = match sqlx::postgres::PgListener::connect_with(&db).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "Failed to create SSF PgListener — falling back to polling only");
            // Fall back to polling-only mode
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                process_ssf_outbox(&db).await;
                cleanup_expired_secrets(&db).await;
            }
        }
    };

    if let Err(e) = listener.listen("ssf_new_event").await {
        tracing::error!(error = %e, "Failed to LISTEN on ssf_new_event");
    }

    let _ = keys; // keys kept for future use (e.g. re-signing)

    tracing::info!("SSF worker started (LISTEN/NOTIFY + 1s fallback)");

    let mut cleanup_tick: u64 = 0;

    loop {
        // tokio::select! — wake on NOTIFY or 1-second timeout
        tokio::select! {
            notification = listener.recv() => {
                match notification {
                    Ok(n) => {
                        tracing::debug!(payload = %n.payload(), "SSF NOTIFY received");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "SSF PgListener error, will retry");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
        }

        process_ssf_outbox(&db).await;

        cleanup_tick += 1;
        // Run secret cleanup every ~60 seconds (60 ticks * 1s)
        if cleanup_tick % 60 == 0 {
            cleanup_expired_secrets(&db).await;
        }
    }
}

/// Clean up expired `previous_shared_secret` values where the grace period has elapsed.
async fn cleanup_expired_secrets(db: &PgPool) {
    let result = sqlx::query(
        "UPDATE ssf_streams SET previous_shared_secret = NULL, updated_at = NOW() \
         WHERE previous_shared_secret IS NOT NULL \
         AND secret_rotated_at IS NOT NULL \
         AND secret_rotated_at + (secret_grace_hours || ' hours')::interval < NOW()",
    )
    .execute(db)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(
                count = r.rows_affected(),
                "Cleaned up expired SSF previous secrets"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to clean up expired SSF secrets");
        }
        _ => {}
    }
}
