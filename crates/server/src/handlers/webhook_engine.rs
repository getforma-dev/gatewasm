//! Webhook delivery engine — dispatches events to tenant webhooks.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;

type HmacSha256 = Hmac<Sha256>;

/// Maximum concurrent webhook deliveries across all tenants.
static WEBHOOK_SEMAPHORE: std::sync::LazyLock<Arc<Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(Semaphore::new(20)));

/// Dispatch an event to all matching active webhooks for a tenant.
/// This is fire-and-forget — errors are logged, not propagated.
pub async fn dispatch_event(
    db: &PgPool,
    tenant_id: &str,
    event_type: &str,
    payload: serde_json::Value,
) {
    let db = db.clone();
    let tenant_id = tenant_id.to_string();
    let event_type = event_type.to_string();

    tokio::spawn(async move {
        if let Err(e) = dispatch_inner(&db, &tenant_id, &event_type, &payload).await {
            tracing::error!(error = %e, tenant = %tenant_id, event = %event_type, "Webhook dispatch failed");
        }
    });
}

async fn dispatch_inner(
    db: &PgPool,
    tenant_id: &str,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Find active webhooks that subscribe to this event type
    let event_json = serde_json::json!([event_type]);
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, url, secret FROM webhooks \
         WHERE tenant_id = $1 AND active = TRUE AND events @> $2::jsonb",
    )
    .bind(tenant_id)
    .bind(&event_json)
    .fetch_all(db)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    let full_payload = serde_json::json!({
        "event": event_type,
        "tenant_id": tenant_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "data": payload,
    });

    for (webhook_id, url, secret) in rows {
        let db = db.clone();
        let payload = full_payload.clone();
        let webhook_id = webhook_id.clone();
        let sem = WEBHOOK_SEMAPHORE.clone();

        tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let delivery_id = format!("whd_{}", uuid::Uuid::new_v4());
            let result = deliver_single(&url, &secret, &payload).await;

            let (status_str, response_status, response_body, next_retry) = match result {
                Ok((code, body)) => {
                    if (200..300).contains(&code) {
                        ("delivered", Some(code as i32), Some(body), None)
                    } else {
                        let retry = chrono::Utc::now() + chrono::Duration::seconds(60);
                        ("pending", Some(code as i32), Some(body), Some(retry))
                    }
                }
                Err(e) => {
                    let retry = chrono::Utc::now() + chrono::Duration::seconds(60);
                    ("pending", None, Some(e.to_string()), Some(retry))
                }
            };

            let event_type_str = payload["event"].as_str().unwrap_or("unknown");

            let _ = sqlx::query(
                "INSERT INTO webhook_deliveries \
                 (id, webhook_id, event_type, payload, response_status, response_body, status, delivered_at, next_retry_at, attempt) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, CASE WHEN $7 = 'delivered' THEN NOW() ELSE NULL END, $8, 1)",
            )
            .bind(&delivery_id)
            .bind(&webhook_id)
            .bind(event_type_str)
            .bind(&payload)
            .bind(response_status)
            .bind(response_body.as_deref())
            .bind(status_str)
            .bind(next_retry)
            .execute(&db)
            .await;
        });
    }

    Ok(())
}

/// Deliver a single webhook call with HMAC-SHA256 signature.
/// Returns (status_code, response_body) on success.
pub async fn deliver_single(
    url: &str,
    secret: &str,
    payload: &serde_json::Value,
) -> Result<(u16, String), Box<dyn std::error::Error + Send + Sync>> {
    let body = serde_json::to_string(payload)?;

    // Compute HMAC-SHA256 signature
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())?;
    hmac::Mac::update(&mut mac, body.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let res = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Forma-Signature", format!("sha256={}", signature))
        .header("User-Agent", "fAuth-Webhook/1.0")
        .body(body)
        .send()
        .await?;

    let status = res.status().as_u16();
    let body = res.text().await.unwrap_or_default();

    Ok((status, body))
}

/// Process pending webhook retries. Called from a background loop in main.rs.
pub async fn process_retries(db: &PgPool) {
    let rows = match sqlx::query_as::<_, (String, String, String, String, serde_json::Value, i32, i32)>(
        "SELECT wd.id, w.url, w.secret, wd.event_type, wd.payload, wd.attempt, wd.max_attempts \
         FROM webhook_deliveries wd \
         JOIN webhooks w ON w.id = wd.webhook_id \
         WHERE wd.status = 'pending' AND wd.next_retry_at <= NOW() AND wd.attempt < wd.max_attempts \
         LIMIT 50",
    )
    .fetch_all(db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "Failed to fetch webhook retries");
            return;
        }
    };

    for (delivery_id, url, secret, _event_type, payload, attempt, max_attempts) in rows {
        let db = db.clone();
        let sem = WEBHOOK_SEMAPHORE.clone();
        tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let result = deliver_single(&url, &secret, &payload).await;
            let new_attempt = attempt + 1;

            match result {
                Ok((code, body)) if (200..300).contains(&code) => {
                    let _ = sqlx::query(
                        "UPDATE webhook_deliveries SET status = 'delivered', response_status = $1, \
                         response_body = $2, delivered_at = NOW(), attempt = $3 WHERE id = $4",
                    )
                    .bind(code as i32)
                    .bind(&body)
                    .bind(new_attempt)
                    .bind(&delivery_id)
                    .execute(&db)
                    .await;
                }
                Ok((code, body)) => {
                    let (status, next_retry) = if new_attempt >= max_attempts {
                        ("failed", None)
                    } else {
                        // Exponential backoff: 1m, 5m, 30m
                        let delay = match new_attempt {
                            1 => 60,
                            2 => 300,
                            _ => 1800,
                        };
                        (
                            "pending",
                            Some(chrono::Utc::now() + chrono::Duration::seconds(delay)),
                        )
                    };
                    let _ = sqlx::query(
                        "UPDATE webhook_deliveries SET status = $1, response_status = $2, \
                         response_body = $3, attempt = $4, next_retry_at = $5 WHERE id = $6",
                    )
                    .bind(status)
                    .bind(code as i32)
                    .bind(&body)
                    .bind(new_attempt)
                    .bind(next_retry)
                    .bind(&delivery_id)
                    .execute(&db)
                    .await;
                }
                Err(e) => {
                    let (status, next_retry) = if new_attempt >= max_attempts {
                        ("failed", None)
                    } else {
                        let delay = match new_attempt {
                            1 => 60,
                            2 => 300,
                            _ => 1800,
                        };
                        (
                            "pending",
                            Some(chrono::Utc::now() + chrono::Duration::seconds(delay)),
                        )
                    };
                    let _ = sqlx::query(
                        "UPDATE webhook_deliveries SET status = $1, response_body = $2, \
                         attempt = $3, next_retry_at = $4 WHERE id = $5",
                    )
                    .bind(status)
                    .bind(e.to_string())
                    .bind(new_attempt)
                    .bind(next_retry)
                    .bind(&delivery_id)
                    .execute(&db)
                    .await;
                }
            }
        });
    }
}
