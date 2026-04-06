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
pub struct UpdateTeamMemberRequest {
    pub role: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetMemberTenantsRequest {
    pub tenant_ids: Vec<String>,
}

// ── GET /platform/admin/team ───────────────────────────────────────────

/// List all platform team members with tenant counts.
pub async fn list_team_members(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let _caller = caller(&req)?;

    let rows = sqlx::query_as::<
        _,
        (
            String,            // pm.id
            String,            // pm.user_id
            String,            // pm.role
            String,            // pm.status
            Option<String>,    // pm.invited_by_user_id
            String,            // u.email
            Option<String>,    // u.name
            Option<DateTime<Utc>>, // pm.last_login_at
            DateTime<Utc>,     // pm.created_at
            i64,               // tenant_count
        ),
    >(
        "SELECT pm.id, pm.user_id, pm.role, pm.status, pm.invited_by_user_id,
                u.email, u.name, pm.last_login_at, pm.created_at,
                COUNT(pmt.id)::bigint AS tenant_count
         FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         LEFT JOIN platform_member_tenants pmt ON pmt.member_id = pm.id
         GROUP BY pm.id, pm.user_id, pm.role, pm.status, pm.invited_by_user_id,
                  u.email, u.name, pm.last_login_at, pm.created_at
         ORDER BY CASE pm.role
                    WHEN 'owner'  THEN 0
                    WHEN 'admin'  THEN 1
                    WHEN 'editor' THEN 2
                    WHEN 'viewer' THEN 3
                    ELSE 4
                  END,
                  pm.created_at ASC",
    )
    .fetch_all(&state.db)
    .await?;

    let members: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(id, user_id, role, status, invited_by_user_id, email, name, last_login_at, created_at, tenant_count)| {
                serde_json::json!({
                    "id": id,
                    "user_id": user_id,
                    "role": role,
                    "status": status,
                    "invited_by_user_id": invited_by_user_id,
                    "email": email,
                    "name": name,
                    "last_login_at": last_login_at.map(|v| v.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                    "tenant_count": tenant_count,
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({ "members": members })))
}

// ── GET /platform/admin/team/:id ───────────────────────────────────────

/// Get a single platform team member with assigned tenants.
pub async fn get_team_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let _caller = caller(&req)?;

    let member = sqlx::query_as::<
        _,
        (
            String,            // pm.id
            String,            // pm.user_id
            String,            // pm.role
            String,            // pm.status
            Option<String>,    // pm.invited_by_user_id
            String,            // u.email
            Option<String>,    // u.name
            Option<DateTime<Utc>>, // pm.last_login_at
            DateTime<Utc>,     // pm.created_at
            DateTime<Utc>,     // pm.updated_at
        ),
    >(
        "SELECT pm.id, pm.user_id, pm.role, pm.status, pm.invited_by_user_id,
                u.email, u.name, pm.last_login_at, pm.created_at, pm.updated_at
         FROM platform_members pm
         JOIN users u ON u.id = pm.user_id
         WHERE pm.id = $1",
    )
    .bind(&member_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "member_not_found",
        message: "Platform team member not found".into(),
    })?;

    let (id, user_id, role, status, invited_by_user_id, email, name, last_login_at, created_at, updated_at) = member;

    // Fetch assigned tenants separately.
    let tenant_rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT t.id, t.name, t.slug
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
        .map(|(tid, tname, tslug)| {
            serde_json::json!({
                "tenant_id": tid,
                "name": tname,
                "slug": tslug,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "member": {
            "id": id,
            "user_id": user_id,
            "role": role,
            "status": status,
            "invited_by_user_id": invited_by_user_id,
            "email": email,
            "name": name,
            "last_login_at": last_login_at.map(|v| v.to_rfc3339()),
            "created_at": created_at.to_rfc3339(),
            "updated_at": updated_at.to_rfc3339(),
            "tenants": tenants,
        }
    })))
}

// ── PATCH /platform/admin/team/:id ─────────────────────────────────────

/// Update a platform team member's role and/or status.
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

    // Fetch target member.
    let target = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, user_id, role FROM platform_members WHERE id = $1",
    )
    .bind(&member_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "member_not_found",
        message: "Platform team member not found".into(),
    })?;

    let (_target_id, target_user_id, target_role) = target;

    // ── Authorization rules ────────────────────────────────────────────
    if target_user_id == me.user_id {
        return Err(AppError::Forbidden {
            error: "cannot_modify_self",
            message: "You cannot modify your own membership".into(),
        });
    }

    if target_role == "owner" {
        return Err(AppError::Forbidden {
            error: "cannot_modify_owner",
            message: "The owner account cannot be modified".into(),
        });
    }

    if target_role == "admin" && !me.is_owner() {
        return Err(AppError::Forbidden {
            error: "insufficient_role",
            message: "Only the owner can modify admin members".into(),
        });
    }

    // ── Validate & apply role change ───────────────────────────────────
    if let Some(ref new_role) = body.role {
        let valid_roles = ["admin", "editor", "viewer"];
        if !valid_roles.contains(&new_role.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_role",
                message: format!("Role must be one of: {}", valid_roles.join(", ")),
            });
        }

        if new_role == "admin" && !me.is_owner() {
            return Err(AppError::Forbidden {
                error: "insufficient_role",
                message: "Only the owner can promote members to admin".into(),
            });
        }

        sqlx::query(
            "UPDATE platform_members SET role = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(new_role)
        .bind(&member_id)
        .execute(&state.db)
        .await?;

        // Admins see all tenants — clear any scoped assignments.
        if new_role == "admin" {
            sqlx::query("DELETE FROM platform_member_tenants WHERE member_id = $1")
                .bind(&member_id)
                .execute(&state.db)
                .await?;
        }
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
        .execute(&state.db)
        .await?;
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

// ── DELETE /platform/admin/team/:id ────────────────────────────────────

/// Remove a platform team member.
pub async fn remove_team_member(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    let target = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, user_id, role FROM platform_members WHERE id = $1",
    )
    .bind(&member_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "member_not_found",
        message: "Platform team member not found".into(),
    })?;

    let (_target_id, target_user_id, target_role) = target;

    // ── Authorization rules ────────────────────────────────────────────
    if target_user_id == me.user_id {
        return Err(AppError::Forbidden {
            error: "cannot_remove_self",
            message: "You cannot remove yourself from the team".into(),
        });
    }

    if target_role == "owner" {
        return Err(AppError::Forbidden {
            error: "cannot_remove_owner",
            message: "The owner account cannot be removed".into(),
        });
    }

    if target_role == "admin" && !me.is_owner() {
        return Err(AppError::Forbidden {
            error: "insufficient_role",
            message: "Only the owner can remove admin members".into(),
        });
    }

    // Delete member (CASCADE handles platform_member_tenants).
    sqlx::query("DELETE FROM platform_members WHERE id = $1")
        .bind(&member_id)
        .execute(&state.db)
        .await?;

    // Also remove their platform role assignment.
    sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND tenant_id = 'ten_platform'")
        .bind(&target_user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ── PUT /platform/admin/team/:id/tenants ───────────────────────────────

/// Replace a member's tenant assignments (full replace).
pub async fn set_member_tenants(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let me = caller(&req)?;

    let body: SetMemberTenantsRequest =
        axum::Json::<SetMemberTenantsRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: tenant_ids (array)".into(),
            })?
            .0;

    // Verify member exists and check their role.
    let target = sqlx::query_as::<_, (String, String)>(
        "SELECT id, role FROM platform_members WHERE id = $1",
    )
    .bind(&member_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "member_not_found",
        message: "Platform team member not found".into(),
    })?;

    let (_target_id, target_role) = target;

    // Owner and admin see all tenants — scoped assignments not allowed.
    if target_role == "owner" || target_role == "admin" {
        return Err(AppError::BadRequest {
            error: "role_not_scoped",
            message: "Owner and admin roles have access to all tenants and cannot be scoped".into(),
        });
    }

    let tenant_count = body.tenant_ids.len() as i64;

    let mut tx = state.db.begin().await?;

    // Clear existing assignments.
    sqlx::query("DELETE FROM platform_member_tenants WHERE member_id = $1")
        .bind(&member_id)
        .execute(&mut *tx)
        .await?;

    // Insert new assignments.
    for tenant_id in &body.tenant_ids {
        let assignment_id = format!("pmt_{}", uuid::Uuid::new_v4());
        sqlx::query(
            "INSERT INTO platform_member_tenants (id, member_id, tenant_id, granted_by_user_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, NOW(), NOW())",
        )
        .bind(&assignment_id)
        .bind(&member_id)
        .bind(tenant_id)
        .bind(&me.user_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "tenant_count": tenant_count,
    })))
}
