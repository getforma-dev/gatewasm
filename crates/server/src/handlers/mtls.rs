use axum::{
    extract::{FromRequest, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::error::AppError;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

// ── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterCertRequest {
    pub fingerprint: String,
    pub client_id: String,
    pub name: Option<String>,
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

/// Validate that the fingerprint follows the `sha256:<hex>` format.
fn validate_fingerprint(fp: &str) -> Result<(), AppError> {
    if !fp.starts_with("sha256:") {
        return Err(AppError::BadRequest {
            error: "invalid_fingerprint",
            message: "Fingerprint must start with 'sha256:' followed by hex digits".into(),
        });
    }

    let hex_part = fp.strip_prefix("sha256:").unwrap_or(fp);
    if hex_part.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_fingerprint",
            message: "Fingerprint hex portion must not be empty".into(),
        });
    }

    // Must be valid hex characters
    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest {
            error: "invalid_fingerprint",
            message: "Fingerprint hex portion contains invalid characters".into(),
        });
    }

    Ok(())
}

fn check_mtls_enabled(tenant: &TenantContext) -> Result<(), AppError> {
    if !tenant.security_config.mtls_enabled {
        return Err(AppError::Forbidden {
            error: "mtls_not_enabled",
            message: "mTLS is not enabled for this tenant. Enable it via PATCH /manage/tenants/{id}/security".into(),
        });
    }
    Ok(())
}

// ── Handlers ─────────────────────────────────────────────────────────────

/// POST /manage/certificates — Register a client certificate fingerprint.
pub async fn register_certificate(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    check_mtls_enabled(&tenant)?;

    let body: RegisterCertRequest =
        axum::Json::<RegisterCertRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: fingerprint, client_id".into(),
            })?
            .0;

    validate_fingerprint(&body.fingerprint)?;

    if body.client_id.trim().is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_client_id",
            message: "client_id must not be empty".into(),
        });
    }

    let cert_id = format!("cert_{}", uuid::Uuid::new_v4());
    let name = body.name.unwrap_or_else(|| "Certificate".to_string());

    sqlx::query(
        "INSERT INTO client_certificates (id, fingerprint, client_id, tenant_id, name) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&cert_id)
    .bind(&body.fingerprint)
    .bind(&body.client_id)
    .bind(&tenant.tenant_id)
    .bind(&name)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.is_unique_violation() {
                return AppError::Conflict {
                    error: "duplicate_fingerprint",
                    message: "A certificate with this fingerprint already exists for this tenant"
                        .into(),
                };
            }
        }
        AppError::InternalError {
            error: "server_error",
            message: format!("Failed to register certificate: {}", e),
        }
    })?;

    tracing::info!(
        cert_id = %cert_id,
        tenant = %tenant.tenant_id,
        fingerprint = %body.fingerprint,
        "Client certificate registered"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "certificate": {
                "id": cert_id,
                "fingerprint": body.fingerprint,
                "client_id": body.client_id,
                "name": name,
                "tenant_id": tenant.tenant_id,
            }
        })),
    ))
}

/// GET /manage/certificates — List active (non-revoked) certificates for this tenant.
pub async fn list_certificates(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    check_mtls_enabled(&tenant)?;

    let rows = sqlx::query(
        "SELECT id, fingerprint, client_id, name, created_at \
         FROM client_certificates \
         WHERE tenant_id = $1 AND revoked_at IS NULL \
         ORDER BY created_at DESC",
    )
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await?;

    let certificates: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "fingerprint": r.get::<String, _>("fingerprint"),
                "client_id": r.get::<String, _>("client_id"),
                "name": r.get::<String, _>("name"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "certificates": certificates })))
}

/// DELETE /manage/certificates/{fingerprint} — Revoke a certificate (soft delete).
pub async fn revoke_certificate(
    State(state): State<AppState>,
    Path(fingerprint): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    check_mtls_enabled(&tenant)?;

    // URL-decode the fingerprint (the colon in sha256:hex may be encoded)
    let decoded_fingerprint =
        urlencoding::decode(&fingerprint).unwrap_or_else(|_| fingerprint.clone().into());

    let result = sqlx::query(
        "UPDATE client_certificates \
         SET revoked_at = NOW() \
         WHERE tenant_id = $1 AND fingerprint = $2 AND revoked_at IS NULL",
    )
    .bind(&tenant.tenant_id)
    .bind(decoded_fingerprint.as_ref())
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "certificate_not_found",
            message: "No active certificate found with this fingerprint".into(),
        });
    }

    tracing::info!(
        tenant = %tenant.tenant_id,
        fingerprint = %decoded_fingerprint,
        "Client certificate revoked"
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "message": "Certificate revoked",
            "fingerprint": decoded_fingerprint.as_ref(),
        })),
    ))
}
