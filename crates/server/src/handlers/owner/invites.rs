use axum::{
    extract::{FromRequest, Path, State},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde::Deserialize;

use crate::error::AppError;
use crate::handlers::auth::normalize_email;
use crate::jwt::signing;
use crate::middleware::platform_user::PlatformUser;
use crate::AppState;

use super::owner_claims;

#[derive(Debug, Deserialize)]
pub struct CreateOwnerInviteRequest {
    pub email: String,
    pub expires_in_hours: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AcceptOwnerInviteRequest {
    pub token: String,
}

/// GET /platform/admin/invites — List provider-owner invites.
pub async fn list_owner_invites(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let _claims = owner_claims(&req)?;

    // Keep statuses accurate for read paths.
    let _ = sqlx::query(
        "UPDATE owner_invites
         SET status = 'expired', updated_at = NOW()
         WHERE status = 'pending' AND expires_at < NOW()",
    )
    .execute(&state.db)
    .await;

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            chrono::DateTime<Utc>,
            Option<chrono::DateTime<Utc>>,
            Option<chrono::DateTime<Utc>>,
            chrono::DateTime<Utc>,
            chrono::DateTime<Utc>,
        ),
    >(
        "SELECT oi.id, oi.email, oi.status, oi.invited_by_user_id, oi.accepted_by_user_id,
                oi.expires_at, oi.accepted_at, oi.revoked_at, oi.created_at, oi.updated_at
         FROM owner_invites oi
         ORDER BY oi.created_at DESC
         LIMIT 200",
    )
    .fetch_all(&state.db)
    .await?;

    let invites: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                email,
                status,
                invited_by_user_id,
                accepted_by_user_id,
                expires_at,
                accepted_at,
                revoked_at,
                created_at,
                updated_at,
            )| {
                serde_json::json!({
                    "id": id,
                    "email": email,
                    "status": status,
                    "invited_by_user_id": invited_by_user_id,
                    "accepted_by_user_id": accepted_by_user_id,
                    "expires_at": expires_at.to_rfc3339(),
                    "accepted_at": accepted_at.map(|v| v.to_rfc3339()),
                    "revoked_at": revoked_at.map(|v| v.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                    "updated_at": updated_at.to_rfc3339(),
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({ "invites": invites })))
}

/// POST /platform/admin/invites — Create provider-owner invite.
pub async fn create_owner_invite(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = owner_claims(&req)?;
    let body: CreateOwnerInviteRequest =
        axum::Json::<CreateOwnerInviteRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: email".into(),
            })?
            .0;

    let email = normalize_email(&body.email);
    if email.is_empty() || !crate::handlers::auth::is_valid_email(&email) {
        return Err(AppError::BadRequest {
            error: "invalid_email",
            message: "A valid email is required".into(),
        });
    }

    let expires_in = body.expires_in_hours.unwrap_or(72).clamp(1, 24 * 30);
    let expires_at = Utc::now() + Duration::hours(expires_in);
    let invite_id = format!("oinv_{}", uuid::Uuid::new_v4());
    let invite_token = format!("oinv_{}", uuid::Uuid::new_v4());
    let token_hash = signing::hash_token(&invite_token);

    let mut tx = state.db.begin().await?;

    // Ensure one active invite per email.
    sqlx::query(
        "UPDATE owner_invites
         SET status = 'revoked', revoked_at = NOW(), updated_at = NOW()
         WHERE status = 'pending' AND lower(email) = lower($1)",
    )
    .bind(&email)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO owner_invites
         (id, email, token_hash, invited_by_user_id, status, expires_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'pending', $5, NOW(), NOW())",
    )
    .bind(&invite_id)
    .bind(&email)
    .bind(&token_hash)
    .bind(&claims.sub)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let invite_url = format!(
        "{}/platform/login?owner_invite={}",
        state.config.server_url, invite_token
    );

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "invite": {
                "id": invite_id,
                "email": email,
                "status": "pending",
                "expires_at": expires_at.to_rfc3339(),
                "invited_by_user_id": claims.sub,
            },
            "invite_token": invite_token,
            "invite_url": invite_url,
        })),
    ))
}

/// POST /platform/admin/invites/{id}/revoke
/// — Revoke a pending provider-owner invite.
pub async fn revoke_owner_invite(
    State(state): State<AppState>,
    Path(invite_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let _claims = owner_claims(&req)?;

    let updated = sqlx::query(
        "UPDATE owner_invites
         SET status = 'revoked', revoked_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(&invite_id)
    .execute(&state.db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "invite_not_found",
            message: "Pending invite not found".into(),
        });
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "invite_id": invite_id,
        "status": "revoked",
    })))
}

/// POST /platform/admin/invites/accept — Accept an owner invite as a platform user.
pub async fn accept_owner_invite(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user =
        req.extensions()
            .get::<PlatformUser>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Platform authentication required".into(),
            })?;
    let body: AcceptOwnerInviteRequest =
        axum::Json::<AcceptOwnerInviteRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: token".into(),
            })?
            .0;

    let token = body.token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_invite_token",
            message: "Invite token is required".into(),
        });
    }

    let token_hash = signing::hash_token(token);
    let user_email = normalize_email(&platform_user.email);
    let mut tx = state.db.begin().await?;

    let invite = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            chrono::DateTime<Utc>,
            Option<String>,
        ),
    >(
        "SELECT id, email, status, expires_at, accepted_by_user_id
         FROM owner_invites
         WHERE token_hash = $1
         LIMIT 1
         FOR UPDATE",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound {
        error: "invite_not_found",
        message: "Invite not found".into(),
    })?;

    let (invite_id, invite_email, status, expires_at, accepted_by_user_id) = invite;
    let invite_email_norm = normalize_email(&invite_email);
    if invite_email_norm != user_email {
        return Err(AppError::Forbidden {
            error: "invite_email_mismatch",
            message: "Invite email does not match the signed-in platform account".into(),
        });
    }

    if status == "accepted" {
        if accepted_by_user_id.as_deref() == Some(platform_user.user_id.as_str()) {
            tx.commit().await?;
            return Ok(Json(serde_json::json!({
                "accepted": true,
                "already_accepted": true,
                "invite_id": invite_id,
            })));
        }
        return Err(AppError::Forbidden {
            error: "invite_already_used",
            message: "This invite was already accepted by another account".into(),
        });
    }

    if status == "revoked" {
        return Err(AppError::BadRequest {
            error: "invite_revoked",
            message: "This invite has been revoked".into(),
        });
    }

    if status == "expired" || expires_at < Utc::now() {
        let _ = sqlx::query(
            "UPDATE owner_invites SET status = 'expired', updated_at = NOW() WHERE id = $1",
        )
        .bind(&invite_id)
        .execute(&mut *tx)
        .await;

        return Err(AppError::BadRequest {
            error: "invite_expired",
            message: "This invite has expired".into(),
        });
    }

    sqlx::query(
        "INSERT INTO platform_accounts (user_id, status, created_at, updated_at)
         VALUES ($1, 'active', NOW(), NOW())
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&platform_user.user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&platform_user.user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE owner_invites
         SET status = 'accepted',
             accepted_by_user_id = $2,
             accepted_at = NOW(),
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(&invite_id)
    .bind(&platform_user.user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "accepted": true,
        "already_accepted": false,
        "invite_id": invite_id,
        "email": platform_user.email,
    })))
}
