use axum::{
    extract::{FromRequest, Path, State},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::AppError;
use crate::middleware::platform_team::PlatformMember;
use crate::AppState;

// ── Helpers ────────────────────────────────────────────────────────────

/// Extract the `PlatformMember` from request extensions (set by
/// `require_platform_member` middleware).
fn caller(req: &axum::extract::Request) -> Result<PlatformMember, AppError> {
    req.extensions()
        .get::<PlatformMember>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Platform membership context missing".into(),
        })
}

// ── Request bodies ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TenantAssignment {
    pub tenant_id: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTeamMemberRequest {
    pub status: Option<String>,
    pub tenant_assignments: Option<Vec<TenantAssignment>>,
}

// ── GET /platform/admin/team ───────────────────────────────────────────

/// List all platform team members scoped to this account owner.
pub async fn list_team_members(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    let rows = sqlx::query_as::<
        _,
        (
            String,                    // pm.id
            String,                    // pm.user_id
            String,                    // pm.status
            Option<String>,            // pm.invited_by_user_id
            String,                    // u.email
            Option<String>,            // u.name
            Option<DateTime<Utc>>,     // pm.last_login_at
            DateTime<Utc>,             // pm.created_at
            bool,                      // is_account_owner
            serde_json::Value,         // tenant_assignments (json)
        ),
    >(
        "SELECT pm.id, pm.user_id, pm.status, pm.invited_by_user_id,
                u.email, u.name, pm.last_login_at, pm.created_at,
                (pm.account_owner_user_id = pm.user_id) AS is_account_owner,
                COALESCE(
                  json_agg(json_build_object(
                    'tenant_id', pmt.tenant_id,
                    'role', pmt.role,
                    'tenant_name', t.name
                  )) FILTER (WHERE pmt.id IS NOT NULL),
                  '[]'::json
                ) AS tenant_assignments
         FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         LEFT JOIN platform_member_tenants pmt ON pmt.member_id = pm.id
         LEFT JOIN tenants t ON t.id = pmt.tenant_id
         WHERE pm.account_owner_user_id = $1
         GROUP BY pm.id, pm.user_id, pm.status, pm.invited_by_user_id,
                  u.email, u.name, pm.last_login_at, pm.created_at
         ORDER BY (pm.account_owner_user_id = pm.user_id) DESC,
                  pm.created_at ASC",
    )
    .bind(&me.user_id)
    .fetch_all(&state.db)
    .await?;

    let members: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(id, user_id, status, invited_by_user_id, email, name, last_login_at, created_at, is_account_owner, tenant_assignments)| {
                serde_json::json!({
                    "id": id,
                    "user_id": user_id,
                    "is_account_owner": is_account_owner,
                    "status": status,
                    "invited_by_user_id": invited_by_user_id,
                    "email": email,
                    "name": name,
                    "last_login_at": last_login_at.map(|v| v.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                    "tenant_assignments": tenant_assignments,
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({ "members": members })))
}

// ── GET /platform/admin/team/:id ───────────────────────────────────────

/// Get a single platform team member with assigned tenants (scoped to caller's account).
pub async fn get_team_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    let member = sqlx::query_as::<
        _,
        (
            String,                    // pm.id
            String,                    // pm.user_id
            String,                    // pm.status
            Option<String>,            // pm.invited_by_user_id
            String,                    // u.email
            Option<String>,            // u.name
            Option<DateTime<Utc>>,     // pm.last_login_at
            DateTime<Utc>,             // pm.created_at
            DateTime<Utc>,             // pm.updated_at
            bool,                      // is_account_owner
        ),
    >(
        "SELECT pm.id, pm.user_id, pm.status, pm.invited_by_user_id,
                u.email, u.name, pm.last_login_at, pm.created_at, pm.updated_at,
                (pm.account_owner_user_id = pm.user_id) AS is_account_owner
         FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         WHERE pm.id = $1 AND pm.account_owner_user_id = $2",
    )
    .bind(&member_id)
    .bind(&me.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "member_not_found",
        message: "Platform team member not found".into(),
    })?;

    let (id, user_id, status, invited_by_user_id, email, name, last_login_at, created_at, updated_at, is_account_owner) = member;

    // Fetch assigned tenants with per-tenant roles.
    let tenant_rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT t.id, t.name, t.slug, pmt.role
         FROM platform_member_tenants pmt
         JOIN tenants t ON t.id = pmt.tenant_id
         WHERE pmt.member_id = $1
         ORDER BY t.name ASC",
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;

    let tenants: Vec<serde_json::Value> = tenant_rows
        .into_iter()
        .map(|(tid, tname, tslug, role)| {
            serde_json::json!({
                "tenant_id": tid,
                "name": tname,
                "slug": tslug,
                "role": role,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "member": {
            "id": id,
            "user_id": user_id,
            "is_account_owner": is_account_owner,
            "status": status,
            "invited_by_user_id": invited_by_user_id,
            "email": email,
            "name": name,
            "last_login_at": last_login_at.map(|v| v.to_rfc3339()),
            "created_at": created_at.to_rfc3339(),
            "updated_at": updated_at.to_rfc3339(),
            "tenant_assignments": tenants,
        }
    })))
}

// ── PATCH /platform/admin/team/:id ─────────────────────────────────────

/// Update a platform team member's status and/or per-tenant role assignments.
pub async fn update_team_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    let body: UpdateTeamMemberRequest =
        axum::Json::<UpdateTeamMemberRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body".into(),
            })?
            .0;

    // Fetch target member scoped to this account.
    let target = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT id, user_id, (account_owner_user_id = user_id) AS is_owner \
         FROM platform_members \
         WHERE id = $1 AND account_owner_user_id = $2",
    )
    .bind(&member_id)
    .bind(&me.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "member_not_found",
        message: "Platform team member not found".into(),
    })?;

    let (_target_id, target_user_id, target_is_owner) = target;

    // ── Authorization rules ────────────────────────────────────────────
    if target_user_id == me.user_id {
        return Err(AppError::Forbidden {
            error: "cannot_modify_self",
            message: "You cannot modify your own membership".into(),
        });
    }

    if target_is_owner {
        return Err(AppError::Forbidden {
            error: "cannot_modify_owner",
            message: "The account owner cannot be modified".into(),
        });
    }

    let mut tx = state.db.begin().await?;

    // ── Validate & apply tenant assignments ───────────────────────────
    if let Some(ref assignments) = body.tenant_assignments {
        let valid_roles = ["admin", "editor", "viewer"];

        for a in assignments {
            if !valid_roles.contains(&a.role.as_str()) {
                return Err(AppError::BadRequest {
                    error: "invalid_role",
                    message: format!("Role must be one of: {}", valid_roles.join(", ")),
                });
            }
        }

        // Validate all tenant IDs belong to this account owner.
        let tenant_ids: Vec<String> = assignments.iter().map(|a| a.tenant_id.clone()).collect();
        if !tenant_ids.is_empty() {
            let valid_count = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM tenants \
                 WHERE id = ANY($1) AND owner_user_id = $2 AND status != 'deleted'",
            )
            .bind(&tenant_ids)
            .bind(&me.user_id)
            .fetch_one(&mut *tx)
            .await
            .map(|(c,)| c)
            .unwrap_or(0);

            if valid_count != tenant_ids.len() as i64 {
                return Err(AppError::BadRequest {
                    error: "invalid_tenant_ids",
                    message: "One or more tenant IDs are invalid or not owned by you".into(),
                });
            }
        }

        // Clear existing assignments.
        sqlx::query("DELETE FROM platform_member_tenants WHERE member_id = $1")
            .bind(&member_id)
            .execute(&mut *tx)
            .await?;

        // Insert new assignments with roles.
        for a in assignments {
            let pmt_id = format!("pmt_{}", uuid::Uuid::new_v4());
            sqlx::query(
                "INSERT INTO platform_member_tenants (id, member_id, tenant_id, role, granted_by_user_id, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
            )
            .bind(&pmt_id)
            .bind(&member_id)
            .bind(&a.tenant_id)
            .bind(&a.role)
            .bind(&me.user_id)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query("UPDATE platform_members SET updated_at = NOW() WHERE id = $1")
            .bind(&member_id)
            .execute(&mut *tx)
            .await?;
    }

    // ── Validate & apply status change ─────────────────────────────────
    if let Some(ref new_status) = body.status {
        let valid_statuses = ["active", "suspended"];
        if !valid_statuses.contains(&new_status.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_status",
                message: format!("Status must be one of: {}", valid_statuses.join(", ")),
            });
        }

        sqlx::query(
            "UPDATE platform_members SET status = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(new_status)
        .bind(&member_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ── DELETE /platform/admin/team/:id ────────────────────────────────────

/// Remove a platform team member (scoped to caller's account).
pub async fn remove_team_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    let target = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT id, user_id, (account_owner_user_id = user_id) AS is_owner \
         FROM platform_members \
         WHERE id = $1 AND account_owner_user_id = $2",
    )
    .bind(&member_id)
    .bind(&me.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "member_not_found",
        message: "Platform team member not found".into(),
    })?;

    let (_target_id, target_user_id, target_is_owner) = target;

    // ── Authorization rules ────────────────────────────────────────────
    if target_user_id == me.user_id {
        return Err(AppError::Forbidden {
            error: "cannot_remove_self",
            message: "You cannot remove yourself from the team".into(),
        });
    }

    if target_is_owner {
        return Err(AppError::Forbidden {
            error: "cannot_remove_owner",
            message: "The account owner cannot be removed".into(),
        });
    }

    // Delete member (CASCADE handles platform_member_tenants).
    sqlx::query("DELETE FROM platform_members WHERE id = $1")
        .bind(&member_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "success": true })))
}
