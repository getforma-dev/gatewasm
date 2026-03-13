use axum::{
    extract::{FromRequest, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::error::AppError;
use crate::middleware::entitlements::ensure_organization_capacity;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrganizationRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: String,
    pub role: Option<String>,
}

fn tenant_from_req(req: &axum::extract::Request) -> Result<TenantContext, AppError> {
    req.extensions()
        .get::<TenantContext>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Missing tenant context".into(),
        })
}

fn normalize_slug(slug: &str) -> Result<String, AppError> {
    let slug = slug.trim().to_lowercase();
    if slug.is_empty() || slug.len() > 64 {
        return Err(AppError::BadRequest {
            error: "invalid_slug",
            message: "Organization slug must be 1-64 characters".into(),
        });
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::BadRequest {
            error: "invalid_slug",
            message: "Organization slug must use lowercase letters, numbers, and hyphens only"
                .into(),
        });
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(AppError::BadRequest {
            error: "invalid_slug",
            message: "Organization slug cannot start or end with a hyphen".into(),
        });
    }
    Ok(slug)
}

async fn ensure_org_belongs_to_tenant(
    state: &AppState,
    organization_id: &str,
    tenant_id: &str,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, String>(
        "SELECT id FROM organizations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(organization_id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "organization_not_found",
            message: "Organization not found".into(),
        });
    }

    Ok(())
}

/// GET /manage/organizations — List organizations for the authenticated tenant.
pub async fn list_organizations(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let rows = sqlx::query(
        "SELECT id, name, slug, metadata, created_at, updated_at \
         FROM organizations WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<String, _>("id"),
                "name": row.get::<String, _>("name"),
                "slug": row.get::<String, _>("slug"),
                "metadata": row.get::<serde_json::Value, _>("metadata"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            })
        })
        .collect();
    let total = items.len();

    Ok(Json(serde_json::json!({
        "organizations": items,
        "total": total,
    })))
}

/// POST /manage/organizations — Create an organization in the authenticated tenant.
pub async fn create_organization(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let body: CreateOrganizationRequest =
        axum::Json::<CreateOrganizationRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: name, slug".into(),
            })?
            .0;

    let name = body.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(AppError::BadRequest {
            error: "invalid_name",
            message: "Organization name must be 1-120 characters".into(),
        });
    }
    let slug = normalize_slug(&body.slug)?;

    ensure_organization_capacity(&state, &tenant.tenant_id, 1).await?;

    let duplicate = sqlx::query_scalar::<_, String>(
        "SELECT id FROM organizations WHERE tenant_id = $1 AND slug = $2",
    )
    .bind(&tenant.tenant_id)
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;

    if duplicate.is_some() {
        return Err(AppError::Conflict {
            error: "organization_exists",
            message: format!("An organization with slug '{}' already exists", slug),
        });
    }

    let organization_id = format!("org_{}", uuid::Uuid::new_v4());
    let metadata = body.metadata.unwrap_or_else(|| serde_json::json!({}));

    sqlx::query(
        "INSERT INTO organizations (id, tenant_id, name, slug, metadata) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&organization_id)
    .bind(&tenant.tenant_id)
    .bind(name)
    .bind(&slug)
    .bind(&metadata)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "organization": {
                "id": organization_id,
                "name": name,
                "slug": slug,
                "metadata": metadata,
            }
        })),
    ))
}

/// GET /manage/organizations/{id} — Retrieve a single organization.
pub async fn get_organization(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let row = sqlx::query(
        "SELECT id, name, slug, metadata, created_at, updated_at \
         FROM organizations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&organization_id)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "organization_not_found",
        message: "Organization not found".into(),
    })?;

    Ok(Json(serde_json::json!({
        "id": row.get::<String, _>("id"),
        "name": row.get::<String, _>("name"),
        "slug": row.get::<String, _>("slug"),
        "metadata": row.get::<serde_json::Value, _>("metadata"),
        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
    })))
}

/// PATCH /manage/organizations/{id} — Update an organization.
pub async fn update_organization(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    ensure_org_belongs_to_tenant(&state, &organization_id, &tenant.tenant_id).await?;

    let body: UpdateOrganizationRequest =
        axum::Json::<UpdateOrganizationRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body".into(),
            })?
            .0;

    if let Some(name) = body.name.as_deref() {
        let name = name.trim();
        if name.is_empty() || name.len() > 120 {
            return Err(AppError::BadRequest {
                error: "invalid_name",
                message: "Organization name must be 1-120 characters".into(),
            });
        }
        sqlx::query("UPDATE organizations SET name = $1, updated_at = NOW() WHERE id = $2")
            .bind(name)
            .bind(&organization_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(slug_value) = body.slug.as_deref() {
        let slug = normalize_slug(slug_value)?;
        let duplicate = sqlx::query_scalar::<_, String>(
            "SELECT id FROM organizations WHERE tenant_id = $1 AND slug = $2 AND id != $3",
        )
        .bind(&tenant.tenant_id)
        .bind(&slug)
        .bind(&organization_id)
        .fetch_optional(&state.db)
        .await?;

        if duplicate.is_some() {
            return Err(AppError::Conflict {
                error: "organization_exists",
                message: format!("An organization with slug '{}' already exists", slug),
            });
        }

        sqlx::query("UPDATE organizations SET slug = $1, updated_at = NOW() WHERE id = $2")
            .bind(&slug)
            .bind(&organization_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(metadata) = body.metadata {
        sqlx::query("UPDATE organizations SET metadata = $1, updated_at = NOW() WHERE id = $2")
            .bind(&metadata)
            .bind(&organization_id)
            .execute(&state.db)
            .await?;
    }

    let row = sqlx::query(
        "SELECT id, name, slug, metadata, created_at, updated_at \
         FROM organizations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&organization_id)
    .bind(&tenant.tenant_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "id": row.get::<String, _>("id"),
        "name": row.get::<String, _>("name"),
        "slug": row.get::<String, _>("slug"),
        "metadata": row.get::<serde_json::Value, _>("metadata"),
        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
        "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
    })))
}

/// DELETE /manage/organizations/{id} — Delete an organization.
pub async fn delete_organization(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;

    let result = sqlx::query("DELETE FROM organizations WHERE id = $1 AND tenant_id = $2")
        .bind(&organization_id)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "organization_not_found",
            message: "Organization not found".into(),
        });
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /manage/organizations/{id}/members — List members for an organization.
pub async fn list_members(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    ensure_org_belongs_to_tenant(&state, &organization_id, &tenant.tenant_id).await?;

    let rows = sqlx::query(
        "SELECT m.user_id, m.role, m.created_at, u.email, u.name \
         FROM organization_members m \
         JOIN users u ON u.id = m.user_id \
         WHERE m.organization_id = $1 AND m.tenant_id = $2 \
         ORDER BY m.created_at DESC",
    )
    .bind(&organization_id)
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await?;

    let members: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "user_id": row.get::<String, _>("user_id"),
                "email": row.get::<String, _>("email"),
                "name": row.get::<Option<String>, _>("name"),
                "role": row.get::<String, _>("role"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "members": members })))
}

/// POST /manage/organizations/{id}/members — Add or update a member role.
pub async fn add_member(
    State(state): State<AppState>,
    Path(organization_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    ensure_org_belongs_to_tenant(&state, &organization_id, &tenant.tenant_id).await?;

    let body: AddMemberRequest = axum::Json::<AddMemberRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body. Required: user_id".into(),
        })?
        .0;

    let role = body.role.unwrap_or_else(|| "member".to_string());
    if role.trim().is_empty() || role.len() > 32 {
        return Err(AppError::BadRequest {
            error: "invalid_role",
            message: "Organization member role must be 1-32 characters".into(),
        });
    }

    let user_exists =
        sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(&body.user_id)
            .bind(&tenant.tenant_id)
            .fetch_optional(&state.db)
            .await?;

    if user_exists.is_none() {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: "User not found in this tenant".into(),
        });
    }

    sqlx::query(
        "INSERT INTO organization_members (organization_id, user_id, tenant_id, role) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (organization_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(&organization_id)
    .bind(&body.user_id)
    .bind(&tenant.tenant_id)
    .bind(role.trim())
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "organization_id": organization_id,
            "user_id": body.user_id,
            "role": role.trim(),
        })),
    ))
}

/// DELETE /manage/organizations/{id}/members/{user_id} — Remove a member.
pub async fn remove_member(
    State(state): State<AppState>,
    Path((organization_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = tenant_from_req(&req)?;
    ensure_org_belongs_to_tenant(&state, &organization_id, &tenant.tenant_id).await?;

    let result = sqlx::query(
        "DELETE FROM organization_members WHERE organization_id = $1 AND user_id = $2 AND tenant_id = $3",
    )
    .bind(&organization_id)
    .bind(&user_id)
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "member_not_found",
            message: "Organization member not found".into(),
        });
    }

    Ok(StatusCode::NO_CONTENT)
}
