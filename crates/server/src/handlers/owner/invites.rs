use axum::{
    extract::{FromRequest, Path, Query, State},
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde::Deserialize;

use crate::error::AppError;
use crate::handlers::auth::normalize_email;
use crate::jwt::signing;
use crate::middleware::platform_team::PlatformMember;
use crate::middleware::platform_user::PlatformUser;
use crate::AppState;

// ── Helpers ────────────────────────────────────────────────────────────

/// Extract the `PlatformMember` from request extensions (set by
/// `require_platform_member` middleware on team routes).
fn caller(req: &axum::extract::Request) -> Result<PlatformMember, AppError> {
    req.extensions()
        .get::<PlatformMember>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Platform membership context missing".into(),
        })
}

// ── Request / query types ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateOwnerInviteRequest {
    pub email: String,
    pub role: String,
    pub tenant_ids: Option<Vec<String>>,
    pub expires_in_hours: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AcceptOwnerInviteRequest {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct ResolveInviteQuery {
    pub token: String,
}

// ── GET /platform/admin/invites/resolve ────────────────────────────────

/// Public endpoint (no auth) — resolves invite details for the login page.
/// Returns invite email, role, tenant names, and whether the user already has an account.
pub async fn resolve_invite(
    State(state): State<AppState>,
    Query(params): Query<ResolveInviteQuery>,
) -> Result<impl IntoResponse, AppError> {
    let token = params.token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest {
            error: "missing_token",
            message: "Invite token is required".into(),
        });
    }

    let token_hash = signing::hash_token(token);

    let invite = sqlx::query_as::<_, (String, String, String, Vec<String>, Option<String>, chrono::DateTime<Utc>)>(
        "SELECT oi.email, oi.role, oi.status, oi.tenant_ids, oi.account_owner_user_id, oi.expires_at
         FROM owner_invites oi
         WHERE oi.token_hash = $1
         LIMIT 1",
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "invite_not_found",
        message: "Invite not found".into(),
    })?;

    let (invite_email, role, status, tenant_ids, account_owner_user_id, expires_at) = invite;

    if status != "pending" {
        return Err(AppError::BadRequest {
            error: "invite_not_pending",
            message: format!("This invite has been {}", status),
        });
    }

    if expires_at < Utc::now() {
        return Err(AppError::BadRequest {
            error: "invite_expired",
            message: "This invite has expired".into(),
        });
    }

    // Verify the account owner is still active
    if let Some(ref owner_id) = account_owner_user_id {
        let owner_active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM platform_members WHERE account_owner_user_id = $1 AND user_id = $1 AND status = 'active')",
        )
        .bind(owner_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

        if !owner_active {
            return Err(AppError::BadRequest {
                error: "account_inactive",
                message: "The inviting account is no longer active".into(),
            });
        }
    }

    // Check if a user already exists for this email
    let user_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE lower(email) = lower($1) AND tenant_id = 'ten_platform')",
    )
    .bind(&invite_email)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    // Get inviter email
    let inviter_email: Option<String> = if let Some(ref owner_id) = account_owner_user_id {
        sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
            .bind(owner_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    // Resolve tenant names
    let tenant_names: Vec<String> = if tenant_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, (String,)>("SELECT name FROM tenants WHERE id = ANY($1)")
            .bind(&tenant_ids)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(n,)| n)
            .collect()
    };

    Ok(Json(serde_json::json!({
        "invite_email": invite_email,
        "inviter_email": inviter_email,
        "role": role,
        "tenant_names": tenant_names,
        "user_exists": user_exists,
        "status": status,
    })))
}

/// GET /platform/admin/team/invites — List invites scoped to caller's account.
pub async fn list_owner_invites(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    // Expire stale invites.
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
            String,
            Vec<String>,
        ),
    >(
        "SELECT oi.id, oi.email, oi.status, oi.invited_by_user_id, oi.accepted_by_user_id,
                oi.expires_at, oi.accepted_at, oi.revoked_at, oi.created_at, oi.updated_at,
                oi.role, oi.tenant_ids
         FROM owner_invites oi
         WHERE oi.account_owner_user_id = $1
         ORDER BY oi.created_at DESC
         LIMIT 200",
    )
    .bind(&me.user_id)
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
                role,
                tenant_ids,
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
                    "role": role,
                    "tenant_ids": tenant_ids,
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({ "invites": invites })))
}

/// POST /platform/admin/team/invites — Create a team invite.
pub async fn create_owner_invite(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    let body: CreateOwnerInviteRequest =
        axum::Json::<CreateOwnerInviteRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: email, role".into(),
            })?
            .0;

    let email = normalize_email(&body.email);
    if email.is_empty() || !crate::handlers::auth::is_valid_email(&email) {
        return Err(AppError::BadRequest {
            error: "invalid_email",
            message: "A valid email is required".into(),
        });
    }

    // ── Role validation ────────────────────────────────────────────────
    let role = body.role.trim().to_lowercase();
    if !matches!(role.as_str(), "admin" | "editor" | "viewer") {
        return Err(AppError::BadRequest {
            error: "invalid_role",
            message: "Role must be one of: admin, editor, viewer".into(),
        });
    }

    // ── Tenant validation ──────────────────────────────────────────────
    let tenant_ids: Vec<String> = body
        .tenant_ids
        .unwrap_or_default()
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    if tenant_ids.is_empty() {
        return Err(AppError::BadRequest {
            error: "missing_tenant_ids",
            message: "At least one tenant_id is required".into(),
        });
    }

    // Validate all tenant IDs belong to this account owner.
    let valid_count = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM tenants \
         WHERE id = ANY($1) AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_ids)
    .bind(&me.user_id)
    .fetch_one(&state.db)
    .await
    .map(|(c,)| c)
    .unwrap_or(0);

    if valid_count != tenant_ids.len() as i64 {
        return Err(AppError::BadRequest {
            error: "invalid_tenant_ids",
            message: "One or more tenant IDs are invalid or not owned by you".into(),
        });
    }

    // ── Duplicate check (scoped to this account) ──────────────────────
    let already_member = sqlx::query_as::<_, (String,)>(
        "SELECT pm.id FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         WHERE pm.account_owner_user_id = $1
           AND lower(u.email) = lower($2)
           AND pm.status = 'active'
         LIMIT 1",
    )
    .bind(&me.user_id)
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;

    if already_member.is_some() {
        return Err(AppError::Conflict {
            error: "already_member",
            message: "This email is already an active team member".into(),
        });
    }

    let expires_in = body.expires_in_hours.unwrap_or(72).clamp(1, 24 * 30);
    let expires_at = Utc::now() + Duration::hours(expires_in);
    let invite_id = format!("oinv_{}", uuid::Uuid::new_v4());
    let invite_token = format!("oinv_{}", uuid::Uuid::new_v4());
    let token_hash = signing::hash_token(&invite_token);

    let mut tx = state.db.begin().await?;

    // Ensure one active invite per email per account.
    sqlx::query(
        "UPDATE owner_invites
         SET status = 'revoked', revoked_at = NOW(), updated_at = NOW()
         WHERE status = 'pending' AND lower(email) = lower($1) AND account_owner_user_id = $2",
    )
    .bind(&email)
    .bind(&me.user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO owner_invites
         (id, email, token_hash, invited_by_user_id, status, expires_at, role, tenant_ids, account_owner_user_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, $8, NOW(), NOW())",
    )
    .bind(&invite_id)
    .bind(&email)
    .bind(&token_hash)
    .bind(&me.user_id)
    .bind(expires_at)
    .bind(&role)
    .bind(&tenant_ids)
    .bind(&me.user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let invite_url = format!(
        "{}/platform/login?owner_invite={}",
        state.config.server_url, invite_token
    );

    // ── Send invite email (best-effort) ────────────────────────────────
    // Resolve tenant names for the email body.
    let tenant_names: Vec<String> = if tenant_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, (String,)>(
            "SELECT name FROM tenants WHERE id = ANY($1)",
        )
        .bind(&tenant_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(n,)| n)
        .collect()
    };

    let (subject, text_body, html_body) =
        crate::email::platform_team_invite_email_content(&invite_url, &role, &me.email, &tenant_names);

    if let Err(err) = crate::email::send_email(&state.config, &email, &subject, &text_body, &html_body).await {
        tracing::warn!(
            to = %email,
            error = %err,
            "Failed to send platform team invite email"
        );
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "invite": {
                "id": invite_id,
                "email": email,
                "status": "pending",
                "expires_at": expires_at.to_rfc3339(),
                "invited_by_user_id": me.user_id,
                "role": role,
                "tenant_ids": tenant_ids,
            },
            "invite_token": invite_token,
            "invite_url": invite_url,
        })),
    ))
}

/// POST /platform/admin/team/invites/{id}/revoke
/// — Revoke a pending invite (scoped to caller's account).
pub async fn revoke_owner_invite(
    State(state): State<AppState>,
    Path(invite_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    let updated = sqlx::query(
        "UPDATE owner_invites
         SET status = 'revoked', revoked_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND status = 'pending' AND account_owner_user_id = $2",
    )
    .bind(&invite_id)
    .bind(&me.user_id)
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
/// (This route stays under require_platform_user, not team routes.)
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
            String,                    // id
            String,                    // email
            String,                    // status
            chrono::DateTime<Utc>,     // expires_at
            Option<String>,            // accepted_by_user_id
            String,                    // role
            Vec<String>,               // tenant_ids
            Option<String>,            // account_owner_user_id
        ),
    >(
        "SELECT id, email, status, expires_at, accepted_by_user_id, role, tenant_ids, account_owner_user_id
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

    let (invite_id, invite_email, status, expires_at, accepted_by_user_id, invite_role, invite_tenant_ids, invite_account_owner) = invite;

    let invite_account_owner = invite_account_owner.ok_or(AppError::BadRequest {
        error: "invalid_invite",
        message: "This invite is missing account context and cannot be accepted".into(),
    })?;

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

    // ── Create platform_members row (scoped to inviter's account) ─────
    let member_id = format!("pm_{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO platform_members (id, account_owner_user_id, user_id, invited_by_user_id, status, created_at, updated_at)
         VALUES ($1, $2, $3, (SELECT invited_by_user_id FROM owner_invites WHERE id = $4), 'active', NOW(), NOW())
         ON CONFLICT (account_owner_user_id, user_id) DO NOTHING",
    )
    .bind(&member_id)
    .bind(&invite_account_owner)
    .bind(&platform_user.user_id)
    .bind(&invite_id)
    .execute(&mut *tx)
    .await?;

    // ── Create platform_member_tenants rows with per-tenant role ──────
    // Retrieve the actual member_id (may differ if ON CONFLICT hit).
    let actual_member_id: String = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM platform_members WHERE account_owner_user_id = $1 AND user_id = $2",
    )
    .bind(&invite_account_owner)
    .bind(&platform_user.user_id)
    .fetch_one(&mut *tx)
    .await
    .map(|(id,)| id)?;

    for tid in &invite_tenant_ids {
        let pmt_id = format!("pmt_{}", uuid::Uuid::new_v4());
        sqlx::query(
            "INSERT INTO platform_member_tenants (id, member_id, tenant_id, role, granted_by_user_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, (SELECT invited_by_user_id FROM owner_invites WHERE id = $5), NOW(), NOW())
             ON CONFLICT (member_id, tenant_id) DO UPDATE SET role = EXCLUDED.role, updated_at = NOW()",
        )
        .bind(&pmt_id)
        .bind(&actual_member_id)
        .bind(tid)
        .bind(&invite_role)
        .bind(&invite_id)
        .execute(&mut *tx)
        .await?;
    }

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
        "role": invite_role,
    })))
}
