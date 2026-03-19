use axum::{
    extract::{FromRequest, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::error::AppError;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// Permission keys that can be assigned to custom roles.
/// Tenants can also use `custom:*` prefix for their own permissions.
pub const VALID_PERMISSIONS: &[&str] = &[
    "user:read",
    "user:write",
    "user:delete",
    "role:read",
    "role:write",
    "audit:read",
    "config:read",
    "config:write",
    "webhook:read",
    "webhook:write",
];

/// System role names that cannot be used for custom roles.
const RESERVED_ROLE_NAMES: &[&str] = &["user", "admin"];

fn validate_permissions(perms: &[String]) -> Result<(), AppError> {
    for p in perms {
        if !VALID_PERMISSIONS.contains(&p.as_str()) && !p.starts_with("custom:") {
            return Err(AppError::BadRequest {
                error: "invalid_permission",
                message: format!(
                    "Unknown permission '{}'. Use a known key or 'custom:' prefix.",
                    p
                ),
            });
        }
    }
    Ok(())
}

// ── Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct AssignRoleRequest {
    pub role_id: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────

/// GET /manage/roles — List all roles (system + custom) for this tenant.
pub async fn list_roles(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
) -> Result<impl IntoResponse, AppError> {

    let rows = sqlx::query(
        "SELECT id, name, description, is_system, permissions_jsonb, tenant_id, created_at \
         FROM roles \
         WHERE tenant_id IS NULL OR tenant_id = $1 \
         ORDER BY is_system DESC, name",
    )
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await?;

    let roles: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let perms: serde_json::Value = r.get("permissions_jsonb");
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "name": r.get::<String, _>("name"),
                "description": r.get::<Option<String>, _>("description"),
                "is_system": r.get::<bool, _>("is_system"),
                "permissions": perms,
                "tenant_id": r.get::<Option<String>, _>("tenant_id"),
                "created_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
                    .map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "roles": roles })))
}

/// POST /manage/roles — Create a custom role for this tenant.
pub async fn create_role(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let body: CreateRoleRequest = axum::Json::<CreateRoleRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body. Required: name, permissions".into(),
        })?
        .0;

    let name = body.name.trim().to_lowercase();
    if name.is_empty() || name.len() > 64 {
        return Err(AppError::BadRequest {
            error: "invalid_name",
            message: "Role name must be 1-64 characters".into(),
        });
    }

    if RESERVED_ROLE_NAMES.contains(&name.as_str()) {
        return Err(AppError::BadRequest {
            error: "reserved_name",
            message: format!("'{}' is a reserved system role name", name),
        });
    }

    validate_permissions(&body.permissions)?;

    // Check for duplicate in this tenant
    let exists = sqlx::query_scalar::<_, String>(
        "SELECT id FROM roles WHERE name = $1 AND (tenant_id = $2 OR tenant_id IS NULL)",
    )
    .bind(&name)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_some() {
        return Err(AppError::Conflict {
            error: "role_exists",
            message: format!("A role named '{}' already exists", name),
        });
    }

    let role_id = format!("role_{}", uuid::Uuid::new_v4());
    let perms_json = serde_json::to_value(&body.permissions).unwrap_or_default();

    let perms_text = serde_json::to_string(&body.permissions).unwrap_or_else(|_| "[]".into());

    sqlx::query(
        "INSERT INTO roles (id, name, permissions, description, permissions_jsonb, tenant_id, is_system, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, FALSE, NOW())",
    )
    .bind(&role_id)
    .bind(&name)
    .bind(&perms_text)
    .bind(body.description.as_deref().unwrap_or(""))
    .bind(&perms_json)
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await?;

    tracing::info!(role_id = %role_id, name = %name, tenant = %tenant.tenant_id, "Custom role created");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "role": {
                "id": role_id,
                "name": name,
                "description": body.description,
                "permissions": body.permissions,
                "is_system": false,
                "tenant_id": tenant.tenant_id,
            }
        })),
    ))
}

/// PATCH /manage/roles/{id} — Update a custom role.
pub async fn update_role(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    // Fetch the role
    let row = sqlx::query("SELECT id, is_system, tenant_id FROM roles WHERE id = $1")
        .bind(&role_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound {
            error: "role_not_found",
            message: "Role not found".into(),
        })?;

    let is_system: bool = row.get("is_system");
    if is_system {
        return Err(AppError::Forbidden {
            error: "system_role",
            message: "System roles cannot be modified".into(),
        });
    }

    let role_tenant: Option<String> = row.get("tenant_id");
    if role_tenant.as_deref() != Some(&tenant.tenant_id) {
        return Err(AppError::NotFound {
            error: "role_not_found",
            message: "Role not found".into(),
        });
    }

    let body: UpdateRoleRequest = axum::Json::<UpdateRoleRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    if let Some(ref name) = body.name {
        let name = name.trim().to_lowercase();
        if RESERVED_ROLE_NAMES.contains(&name.as_str()) {
            return Err(AppError::BadRequest {
                error: "reserved_name",
                message: format!("'{}' is a reserved system role name", name),
            });
        }
        sqlx::query("UPDATE roles SET name = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(&name)
            .bind(&role_id)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref desc) = body.description {
        sqlx::query("UPDATE roles SET description = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(desc)
            .bind(&role_id)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref perms) = body.permissions {
        validate_permissions(perms)?;
        let json = serde_json::to_value(perms).unwrap_or_default();
        sqlx::query("UPDATE roles SET permissions_jsonb = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(&json)
            .bind(&role_id)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(serde_json::json!({ "message": "Role updated" })))
}

/// DELETE /manage/roles/{id} — Delete a custom role.
pub async fn delete_role(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
    Extension(tenant): Extension<TenantContext>,
) -> Result<impl IntoResponse, AppError> {

    let row = sqlx::query("SELECT id, is_system, tenant_id FROM roles WHERE id = $1")
        .bind(&role_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound {
            error: "role_not_found",
            message: "Role not found".into(),
        })?;

    let is_system: bool = row.get("is_system");
    if is_system {
        return Err(AppError::Forbidden {
            error: "system_role",
            message: "System roles cannot be deleted".into(),
        });
    }

    let role_tenant: Option<String> = row.get("tenant_id");
    if role_tenant.as_deref() != Some(&tenant.tenant_id) {
        return Err(AppError::NotFound {
            error: "role_not_found",
            message: "Role not found".into(),
        });
    }

    // Remove all user_roles assignments for this role
    sqlx::query("DELETE FROM user_roles WHERE role_id = $1")
        .bind(&role_id)
        .execute(&state.db)
        .await?;

    sqlx::query("DELETE FROM roles WHERE id = $1 AND tenant_id = $2")
        .bind(&role_id)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;

    tracing::info!(role_id = %role_id, tenant = %tenant.tenant_id, "Custom role deleted");

    Ok(Json(serde_json::json!({ "message": "Role deleted" })))
}

/// POST /manage/users/{id}/roles — Assign a role to a user.
pub async fn assign_role(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    // Verify user belongs to this tenant
    let user_exists =
        sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(&user_id)
            .bind(&tenant.tenant_id)
            .fetch_optional(&state.db)
            .await?;

    if user_exists.is_none() {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: "User not found in this tenant".into(),
        });
    }

    let body: AssignRoleRequest = axum::Json::<AssignRoleRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body. Required: role_id".into(),
        })?
        .0;

    // Verify role exists and is accessible to this tenant
    let role_exists = sqlx::query_scalar::<_, String>(
        "SELECT id FROM roles WHERE id = $1 AND (tenant_id IS NULL OR tenant_id = $2)",
    )
    .bind(&body.role_id)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?;

    if role_exists.is_none() {
        return Err(AppError::NotFound {
            error: "role_not_found",
            message: "Role not found".into(),
        });
    }

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(&user_id)
    .bind(&body.role_id)
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await?;

    // Dispatch webhook event
    super::webhook_engine::dispatch_event(
        &state.db,
        &tenant.tenant_id,
        "user.role_changed",
        serde_json::json!({ "user_id": user_id, "role_id": body.role_id, "action": "assigned" }),
    )
    .await;

    tracing::info!(user_id = %user_id, role_id = %body.role_id, tenant = %tenant.tenant_id, "Role assigned");

    Ok(Json(serde_json::json!({ "message": "Role assigned" })))
}

/// DELETE /manage/users/{id}/roles/{role_id} — Remove a role from a user.
pub async fn remove_role(
    State(state): State<AppState>,
    Path((user_id, role_id)): Path<(String, String)>,
    Extension(tenant): Extension<TenantContext>,
) -> Result<impl IntoResponse, AppError> {

    // Verify user belongs to this tenant
    let user_exists =
        sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(&user_id)
            .bind(&tenant.tenant_id)
            .fetch_optional(&state.db)
            .await?;

    if user_exists.is_none() {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: "User not found in this tenant".into(),
        });
    }

    let result = sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND role_id = $2")
        .bind(&user_id)
        .bind(&role_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "assignment_not_found",
            message: "User does not have this role".into(),
        });
    }

    // Dispatch webhook event
    super::webhook_engine::dispatch_event(
        &state.db,
        &tenant.tenant_id,
        "user.role_changed",
        serde_json::json!({ "user_id": user_id, "role_id": role_id, "action": "removed" }),
    )
    .await;

    tracing::info!(user_id = %user_id, role_id = %role_id, tenant = %tenant.tenant_id, "Role removed");

    Ok(Json(serde_json::json!({ "message": "Role removed" })))
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
