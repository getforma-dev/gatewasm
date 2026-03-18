use axum::{
    extract::{FromRequest, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::crypto::password;
use crate::email;
use crate::error::AppError;
use crate::handlers::auth::{is_valid_email, normalize_email};
use crate::handlers::onboarding::is_reserved_slug;
use crate::jwt::signing;
use crate::middleware::entitlements::ensure_user_capacity;
use crate::middleware::tenant::TenantContext;
use crate::redirects::{default_login_redirect, validate_login_redirect_input};
use crate::security_config::TenantSecurityConfig;
use crate::AppState;

// ── Validation helpers ──────────────────────────────────────────────────

/// Validate that a string is a safe CSS hex color (e.g. `#abc`, `#aabbcc`).
pub fn is_valid_css_color(s: &str) -> bool {
    let s = s.trim();
    if s.starts_with('#') {
        let hex = &s[1..];
        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
    } else {
        false
    }
}

// ── Platform Admin endpoints (authenticated with plat_ key) ─────────────

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    pub owner_email: String,
    pub allowed_providers: Option<Vec<String>>,
    pub max_users: Option<i32>,
    pub signup_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub allowed_providers: Option<Vec<String>>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub allowed_origins: Option<Vec<String>>,
    pub max_users: Option<i32>,
    pub status: Option<String>,
    pub signup_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListTenantsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
}

fn normalize_signup_mode(value: Option<&str>) -> Result<String, AppError> {
    let mode = value
        .unwrap_or("open")
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");
    if !["open", "invite_only"].contains(&mode.as_str()) {
        return Err(AppError::BadRequest {
            error: "invalid_signup_mode",
            message: "signup_mode must be one of: open, invite_only".into(),
        });
    }
    Ok(mode)
}

async fn resolve_tenant_role(
    state: &AppState,
    tenant_id: &str,
    requested_role: Option<&str>,
    default_role: &str,
) -> Result<(String, String), AppError> {
    let role_name = requested_role
        .unwrap_or(default_role)
        .trim()
        .to_ascii_lowercase();
    match role_name.as_str() {
        "admin" => Ok(("role_admin".to_string(), "admin".to_string())),
        "user" => Ok(("role_user".to_string(), "user".to_string())),
        custom_name => {
            let found = sqlx::query_as::<_, (String, String)>(
                "SELECT id, name FROM roles WHERE name = $1 AND (tenant_id IS NULL OR tenant_id = $2)",
            )
            .bind(custom_name)
            .bind(tenant_id)
            .fetch_optional(&state.db)
            .await?;

            match found {
                Some((id, name)) => Ok((id, name)),
                None => Err(AppError::BadRequest {
                    error: "invalid_role",
                    message: format!("Role '{}' not found", custom_name),
                }),
            }
        }
    }
}

/// POST /platform/tenants — Create a new tenant and generate a management API key.
pub async fn create_tenant(
    State(state): State<AppState>,
    Json(body): Json<CreateTenantRequest>,
) -> Result<impl IntoResponse, AppError> {
    let slug = body.slug.trim().to_lowercase();

    if slug.is_empty()
        || slug.len() > 63
        || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AppError::BadRequest {
            error: "invalid_slug",
            message: "Slug must be 1-63 characters, lowercase alphanumeric or hyphens".into(),
        });
    }

    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(AppError::BadRequest {
            error: "invalid_slug",
            message: "Slug must not start or end with a hyphen".into(),
        });
    }

    if is_reserved_slug(&slug) {
        return Err(AppError::BadRequest {
            error: "reserved_slug",
            message: format!("The slug '{}' is reserved and cannot be used", slug),
        });
    }

    let tenant_id = format!("ten_{}", uuid::Uuid::new_v4());
    let api_key = format!("mgmt_{}", uuid::Uuid::new_v4());
    let api_key_prefix = &api_key[..api_key.len().min(13)]; // "mgmt_" + 8 chars
    let api_key_hash = signing::hash_token(&api_key);

    let providers = body
        .allowed_providers
        .unwrap_or_else(|| vec!["email".to_string()]);
    let providers_json = serde_json::to_value(&providers).unwrap_or_default();
    let max_users = body.max_users.unwrap_or(1000);
    let signup_mode = normalize_signup_mode(body.signup_mode.as_deref())?;
    let default_login_redirect = default_login_redirect(&state.config.server_url, &slug);

    // Atomic slug uniqueness: INSERT with ON CONFLICT to avoid SELECT-then-INSERT race
    let result = sqlx::query(
        "INSERT INTO tenants (id, name, slug, owner_email, api_key_hash, api_key_prefix, allowed_providers, max_users, login_redirect_url, signup_mode) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) ON CONFLICT (slug) DO NOTHING",
    )
    .bind(&tenant_id)
    .bind(&body.name)
    .bind(&slug)
    .bind(&body.owner_email)
    .bind(&api_key_hash)
    .bind(api_key_prefix)
    .bind(&providers_json)
    .bind(max_users)
    .bind(&default_login_redirect)
    .bind(&signup_mode)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict {
            error: "slug_taken",
            message: format!("Tenant slug '{}' is already in use", slug),
        });
    }

    sqlx::query(
        "INSERT INTO tenant_entitlements \
         (tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations) \
         VALUES ($1, 'trial', $2, 3, TRUE, TRUE, TRUE) \
         ON CONFLICT (tenant_id) DO NOTHING",
    )
    .bind(&tenant_id)
    .bind(max_users)
    .execute(&state.db)
    .await?;

    tracing::info!(
        tenant_id = %tenant_id,
        slug = %slug,
        owner = %body.owner_email,
        "Tenant created"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "tenant": {
                "id": tenant_id,
                "name": body.name,
                "slug": slug,
                "owner_email": body.owner_email,
                "allowed_providers": providers,
                "max_users": max_users,
                "status": "active",
                "login_redirect_url": default_login_redirect,
                "signup_mode": signup_mode,
            },
            "api_key": api_key,
            "note": "Save this API key in backend secrets only — it will not be shown again."
        })),
    ))
}

/// GET /platform/tenants — List all tenants.
pub async fn list_tenants(
    State(state): State<AppState>,
    Query(params): Query<ListTenantsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let tenants = if let Some(status) = &params.status {
        sqlx::query_as::<_, (String, String, String, String, String, String, serde_json::Value, i32, i32, chrono::DateTime<Utc>, serde_json::Value)>(
            "SELECT id, name, slug, owner_email, status, COALESCE(signup_mode, 'open') AS signup_mode, allowed_providers, max_users, mau_count, created_at, COALESCE(security_config, '{}') AS security_config FROM tenants WHERE status = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, (String, String, String, String, String, String, serde_json::Value, i32, i32, chrono::DateTime<Utc>, serde_json::Value)>(
            "SELECT id, name, slug, owner_email, status, COALESCE(signup_mode, 'open') AS signup_mode, allowed_providers, max_users, mau_count, created_at, COALESCE(security_config, '{}') AS security_config FROM tenants ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    };

    let items: Vec<serde_json::Value> = tenants
        .into_iter()
        .map(
            |(id, name, slug, owner, status, signup_mode, providers, max_users, mau, created, sec_config_json)| {
                let sec_config = crate::security_config::TenantSecurityConfig::from_json(&sec_config_json);
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "slug": slug,
                    "owner_email": owner,
                    "status": status,
                    "signup_mode": signup_mode,
                    "allowed_providers": providers,
                    "max_users": max_users,
                    "mau_count": mau,
                    "security_config": sec_config,
                    "created_at": created.to_rfc3339(),
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({
        "tenants": items,
        "limit": limit,
        "offset": offset,
    })))
}

/// GET /platform/tenants/:id — Get a single tenant.
pub async fn get_tenant(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, String, String, String, serde_json::Value, serde_json::Value, i32, i32, chrono::DateTime<Utc>, chrono::DateTime<Utc>, serde_json::Value)>(
        "SELECT id, name, slug, owner_email, status, COALESCE(signup_mode, 'open') AS signup_mode, allowed_providers, allowed_origins, max_users, mau_count, created_at, updated_at, COALESCE(security_config, '{}') AS security_config FROM tenants WHERE id = $1",
    )
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: format!("No tenant with id '{}'", tenant_id),
    })?;

    let (
        id,
        name,
        slug,
        owner,
        status,
        signup_mode,
        providers,
        origins,
        max_users,
        mau,
        created,
        updated,
        security_config_json,
    ) = row;

    let security_config = crate::security_config::TenantSecurityConfig::from_json(&security_config_json);

    // Count users for this tenant
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "id": id,
        "name": name,
        "slug": slug,
        "owner_email": owner,
        "status": status,
        "signup_mode": signup_mode,
        "allowed_providers": providers,
        "allowed_origins": origins,
        "max_users": max_users,
        "mau_count": mau,
        "user_count": user_count,
        "security_config": security_config,
        "created_at": created.to_rfc3339(),
        "updated_at": updated.to_rfc3339(),
    })))
}

/// PATCH /platform/tenants/:id — Update a tenant.
pub async fn update_tenant(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Json(body): Json<UpdateTenantRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Verify tenant exists
    let exists = sqlx::query_scalar::<_, String>("SELECT id FROM tenants WHERE id = $1")
        .bind(&tenant_id)
        .fetch_optional(&state.db)
        .await?;

    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "tenant_not_found",
            message: format!("No tenant with id '{}'", tenant_id),
        });
    }

    if let Some(ref name) = body.name {
        sqlx::query("UPDATE tenants SET name = $1, updated_at = NOW() WHERE id = $2")
            .bind(name)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref providers) = body.allowed_providers {
        let json = serde_json::to_value(providers).unwrap_or_default();
        sqlx::query("UPDATE tenants SET allowed_providers = $1, updated_at = NOW() WHERE id = $2")
            .bind(&json)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref v) = body.google_client_id {
        sqlx::query("UPDATE tenants SET google_client_id = $1, updated_at = NOW() WHERE id = $2")
            .bind(v)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref v) = body.google_client_secret {
        sqlx::query(
            "UPDATE tenants SET google_client_secret = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;
    }

    if let Some(ref v) = body.github_client_id {
        sqlx::query("UPDATE tenants SET github_client_id = $1, updated_at = NOW() WHERE id = $2")
            .bind(v)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref v) = body.github_client_secret {
        sqlx::query(
            "UPDATE tenants SET github_client_secret = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;
    }

    if let Some(ref origins) = body.allowed_origins {
        let json = serde_json::to_value(origins).unwrap_or_default();
        sqlx::query("UPDATE tenants SET allowed_origins = $1, updated_at = NOW() WHERE id = $2")
            .bind(&json)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(max_users) = body.max_users {
        sqlx::query("UPDATE tenants SET max_users = $1, updated_at = NOW() WHERE id = $2")
            .bind(max_users)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
        sqlx::query(
            "UPDATE tenant_entitlements SET max_users = $1, updated_at = NOW() WHERE tenant_id = $2",
        )
        .bind(max_users)
        .bind(&tenant_id)
        .execute(&state.db)
        .await
        .ok();
    }

    if let Some(ref status) = body.status {
        if !["active", "suspended", "deleted"].contains(&status.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_status",
                message: "Status must be one of: active, suspended, deleted".into(),
            });
        }
        sqlx::query("UPDATE tenants SET status = $1, updated_at = NOW() WHERE id = $2")
            .bind(status)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref signup_mode) = body.signup_mode {
        let mode = normalize_signup_mode(Some(signup_mode.as_str()))?;
        sqlx::query("UPDATE tenants SET signup_mode = $1, updated_at = NOW() WHERE id = $2")
            .bind(mode)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    // Invalidate tenant cache so changes take effect immediately
    if let Ok(Some(slug)) =
        sqlx::query_scalar::<_, String>("SELECT slug FROM tenants WHERE id = $1")
            .bind(&tenant_id)
            .fetch_optional(&state.db)
            .await
    {
        state.tenant_cache.invalidate(&slug);
    }

    tracing::info!(tenant_id = %tenant_id, "Tenant updated");

    // Return updated tenant
    get_tenant(State(state), Path(tenant_id)).await
}

/// DELETE /platform/tenants/:id — Soft-delete a tenant (set status = 'deleted').
pub async fn delete_tenant(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let result =
        sqlx::query("UPDATE tenants SET status = 'deleted', updated_at = NOW() WHERE id = $1")
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "tenant_not_found",
            message: format!("No tenant with id '{}'", tenant_id),
        });
    }

    // Revoke all active sessions for the deleted tenant
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE tenant_id = $1 AND revoked = FALSE")
        .bind(&tenant_id)
        .execute(&state.db)
        .await
        .ok();

    // Invalidate tenant cache so the deletion takes effect immediately
    if let Ok(Some(slug)) =
        sqlx::query_scalar::<_, String>("SELECT slug FROM tenants WHERE id = $1")
            .bind(&tenant_id)
            .fetch_optional(&state.db)
            .await
    {
        state.tenant_cache.invalidate(&slug);
    }

    tracing::info!(tenant_id = %tenant_id, "Tenant soft-deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// POST /platform/tenants/:id/rotate-key — Rotate the management API key.
pub async fn rotate_api_key(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let exists = sqlx::query_scalar::<_, String>("SELECT id FROM tenants WHERE id = $1")
        .bind(&tenant_id)
        .fetch_optional(&state.db)
        .await?;

    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "tenant_not_found",
            message: format!("No tenant with id '{}'", tenant_id),
        });
    }

    let new_key = format!("mgmt_{}", uuid::Uuid::new_v4());
    let prefix = &new_key[..new_key.len().min(13)];
    let hash = signing::hash_token(&new_key);

    sqlx::query("UPDATE tenants SET api_key_hash = $1, api_key_prefix = $2, updated_at = NOW() WHERE id = $3")
        .bind(&hash)
        .bind(prefix)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    tracing::info!(tenant_id = %tenant_id, "API key rotated");

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "api_key": new_key,
            "note": "Save this API key in backend secrets only — it will not be shown again. The old key is now invalid."
        })),
    ))
}

// ── Management API endpoints (authenticated with mgmt_ key) ─────────────

#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub allowed_providers: Option<Vec<String>>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub allowed_origins: Option<Vec<String>>,
    pub branding_logo_url: Option<String>,
    pub branding_primary_color: Option<String>,
    pub branding_app_name: Option<String>,
    pub branding_custom_css: Option<String>,
    pub login_redirect_url: Option<String>,
    pub signup_mode: Option<String>,
}

/// GET /manage/users — List users for the authenticated tenant.
pub async fn list_users(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let params: ListUsersQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(ListUsersQuery {
            limit: None,
            offset: None,
            email: None,
        }))
        .0;

    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let users = if let Some(ref email_filter) = params.email {
        let pattern = format!("%{}%", email_filter.to_lowercase());
        sqlx::query_as::<_, (String, String, Option<String>, Option<String>, bool, chrono::DateTime<Utc>)>(
            "SELECT id, email, name, avatar, email_verified, created_at FROM users WHERE tenant_id = $1 AND email ILIKE $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4",
        )
        .bind(&tenant.tenant_id)
        .bind(&pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, (String, String, Option<String>, Option<String>, bool, chrono::DateTime<Utc>)>(
            "SELECT id, email, name, avatar, email_verified, created_at FROM users WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&tenant.tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    };

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
        .bind(&tenant.tenant_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let items: Vec<serde_json::Value> = users
        .into_iter()
        .map(|(id, email, name, avatar, verified, created)| {
            serde_json::json!({
                "id": id,
                "email": email,
                "name": name,
                "avatar": avatar,
                "email_verified": verified,
                "created_at": created.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "users": items,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

/// GET /manage/users/:id — Get a single user for the authenticated tenant.
pub async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, bool, i32, Option<chrono::DateTime<Utc>>, chrono::DateTime<Utc>)>(
        "SELECT id, email, name, avatar, email_verified, COALESCE(login_attempts, 0), locked_until, created_at FROM users WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "user_not_found",
        message: format!("No user with id '{}' in this tenant", user_id),
    })?;

    let (id, email, name, avatar, verified, login_attempts, locked_until, created) = row;

    // Consolidated query: roles, permissions, providers, and active sessions in one round-trip
    let detail_row = sqlx::query_as::<_, (serde_json::Value, serde_json::Value, serde_json::Value, i64)>(
        "SELECT \
           COALESCE((SELECT jsonb_agg(r.name) FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1), '[]'::jsonb), \
           COALESCE((SELECT jsonb_agg(DISTINCT val) FROM (SELECT jsonb_array_elements_text(r.permissions_jsonb) AS val FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1) sub), '[]'::jsonb), \
           COALESCE((SELECT jsonb_agg(provider) FROM credentials WHERE user_id = $1), '[]'::jsonb), \
           (SELECT COUNT(*) FROM refresh_tokens WHERE user_id = $1 AND revoked = FALSE AND expires_at > NOW())",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;

    let (roles_json, perms_json, providers_json, active_sessions) = detail_row;
    let roles: Vec<String> = serde_json::from_value(roles_json).unwrap_or_default();
    let permissions: Vec<String> = serde_json::from_value(perms_json).unwrap_or_default();
    let providers: Vec<String> = serde_json::from_value(providers_json).unwrap_or_default();

    Ok(Json(serde_json::json!({
        "id": id,
        "email": email,
        "name": name,
        "avatar": avatar,
        "email_verified": verified,
        "roles": roles,
        "permissions": permissions,
        "providers": providers,
        "login_attempts": login_attempts,
        "locked_until": locked_until.map(|dt| dt.to_rfc3339()),
        "active_sessions": active_sessions,
        "created_at": created.to_rfc3339(),
    })))
}

/// DELETE /manage/users/:id — Delete a user from the authenticated tenant.
pub async fn delete_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let result = sqlx::query("DELETE FROM users WHERE id = $1 AND tenant_id = $2")
        .bind(&user_id)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: format!("No user with id '{}' in this tenant", user_id),
        });
    }

    // Dispatch webhook event
    super::webhook_engine::dispatch_event(
        &state.db,
        &tenant.tenant_id,
        "user.deleted",
        serde_json::json!({ "user_id": user_id }),
    )
    .await;

    tracing::info!(user_id = %user_id, tenant_id = %tenant.tenant_id, "User deleted via management API");
    Ok(StatusCode::NO_CONTENT)
}

/// POST /manage/users/:id/block — Lock a user account.
pub async fn block_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    // Lock for a very long time (effectively permanent until unblocked)
    let lock_until = Utc::now() + chrono::Duration::days(36500);

    let result = sqlx::query("UPDATE users SET locked_until = $1 WHERE id = $2 AND tenant_id = $3")
        .bind(lock_until)
        .bind(&user_id)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: format!("No user with id '{}' in this tenant", user_id),
        });
    }

    // Revoke all their sessions within this tenant
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE")
        .bind(&user_id)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await
        .ok();

    tracing::info!(user_id = %user_id, tenant_id = %tenant.tenant_id, "User blocked via management API");

    Ok(Json(serde_json::json!({
        "message": "User has been blocked",
        "user_id": user_id,
    })))
}

/// POST /manage/users/:id/unblock — Unlock a user account.
pub async fn unblock_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let result = sqlx::query(
        "UPDATE users SET locked_until = NULL, login_attempts = 0 WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: format!("No user with id '{}' in this tenant", user_id),
        });
    }

    tracing::info!(user_id = %user_id, tenant_id = %tenant.tenant_id, "User unblocked via management API");

    Ok(Json(serde_json::json!({
        "message": "User has been unblocked",
        "user_id": user_id,
    })))
}

/// GET /manage/stats — Tenant usage statistics.
pub async fn tenant_stats(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    // Consolidated stats: user counts + session counts + login attempts in two queries
    let stats_row = sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64)>(
        "SELECT \
           COUNT(*), \
           COUNT(*) FILTER (WHERE email_verified = TRUE), \
           COUNT(*) FILTER (WHERE locked_until > NOW()), \
           (SELECT COUNT(*) FROM refresh_tokens WHERE tenant_id = $1 AND revoked = FALSE AND expires_at > NOW()), \
           (SELECT COUNT(*) FROM login_attempts WHERE user_id IN (SELECT id FROM users WHERE tenant_id = $1) AND success = TRUE AND created_at > NOW() - INTERVAL '24 hours'), \
           (SELECT COUNT(*) FROM login_attempts WHERE user_id IN (SELECT id FROM users WHERE tenant_id = $1) AND success = FALSE AND created_at > NOW() - INTERVAL '24 hours') \
         FROM users WHERE tenant_id = $1",
    )
    .bind(&tenant.tenant_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0, 0, 0, 0, 0));

    let (total_users, verified_users, blocked_users, active_sessions, logins_24h, failed_logins_24h) = stats_row;

    let providers: Vec<(String, i64)> = sqlx::query_as(
        "SELECT c.provider, COUNT(*) FROM credentials c JOIN users u ON u.id = c.user_id WHERE u.tenant_id = $1 GROUP BY c.provider",
    )
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let provider_breakdown: serde_json::Value = providers
        .into_iter()
        .map(|(p, c)| (p, serde_json::Value::Number(c.into())))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    Ok(Json(serde_json::json!({
        "tenant_id": tenant.tenant_id,
        "total_users": total_users,
        "verified_users": verified_users,
        "blocked_users": blocked_users,
        "active_sessions": active_sessions,
        "logins_24h": logins_24h,
        "failed_logins_24h": failed_logins_24h,
        "providers": provider_breakdown,
    })))
}

/// GET /manage/audit-log — Login attempts for the authenticated tenant.
pub async fn audit_log(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let params: AuditLogQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(AuditLogQuery {
            limit: None,
            offset: None,
        }))
        .0;

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    // Base query: join login_attempts with users to scope by tenant_id
    let rows = sqlx::query_as::<_, (String, String, Option<String>, bool, Option<String>, Option<String>, Option<String>, chrono::DateTime<Utc>)>(
        "SELECT la.id, la.email, la.user_id, la.success, la.ip_address, la.user_agent, la.failure_reason, la.created_at
         FROM login_attempts la
         WHERE la.user_id IN (SELECT id FROM users WHERE tenant_id = $1)
         ORDER BY la.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(&tenant.tenant_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(id, email, user_id, success, ip, ua, reason, created)| {
            serde_json::json!({
                "id": id,
                "email": email,
                "user_id": user_id,
                "success": success,
                "ip_address": ip,
                "user_agent": ua,
                "failure_reason": reason,
                "created_at": created.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "audit_log": items,
        "limit": limit,
        "offset": offset,
    })))
}

/// GET /manage/config — Get tenant configuration.
pub async fn get_config(
    State(_state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    Ok(Json(serde_json::json!({
        "tenant_id": tenant.tenant_id,
        "slug": tenant.slug,
        "name": tenant.name,
        "allowed_providers": tenant.allowed_providers,
        "google_configured": tenant.google_client_id.is_some(),
        "github_configured": tenant.github_client_id.is_some(),
        "allowed_origins": tenant.allowed_origins,
        "status": tenant.status,
        "signup_mode": tenant.signup_mode,
        "security_config": tenant.security_config,
    })))
}

/// PATCH /manage/config — Update tenant configuration (OAuth creds, providers, origins).
pub async fn update_config(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let body: UpdateConfigRequest = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    if let Some(ref providers) = body.allowed_providers {
        let json = serde_json::to_value(providers).unwrap_or_default();
        sqlx::query("UPDATE tenants SET allowed_providers = $1, updated_at = NOW() WHERE id = $2")
            .bind(&json)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref v) = body.google_client_id {
        sqlx::query("UPDATE tenants SET google_client_id = $1, updated_at = NOW() WHERE id = $2")
            .bind(v)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref v) = body.google_client_secret {
        sqlx::query(
            "UPDATE tenants SET google_client_secret = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;
    }

    if let Some(ref v) = body.github_client_id {
        sqlx::query("UPDATE tenants SET github_client_id = $1, updated_at = NOW() WHERE id = $2")
            .bind(v)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref v) = body.github_client_secret {
        sqlx::query(
            "UPDATE tenants SET github_client_secret = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;
    }

    if let Some(ref origins) = body.allowed_origins {
        let json = serde_json::to_value(origins).unwrap_or_default();
        sqlx::query("UPDATE tenants SET allowed_origins = $1, updated_at = NOW() WHERE id = $2")
            .bind(&json)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    // Branding fields
    if let Some(ref v) = body.branding_logo_url {
        sqlx::query("UPDATE tenants SET branding_logo_url = $1, updated_at = NOW() WHERE id = $2")
            .bind(v)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }
    if let Some(ref v) = body.branding_primary_color {
        if !is_valid_css_color(v) {
            return Err(AppError::BadRequest {
                error: "invalid_color",
                message: "branding_primary_color must be a valid CSS hex color (e.g. #abc, #aabbcc)".into(),
            });
        }
        sqlx::query(
            "UPDATE tenants SET branding_primary_color = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;
    }
    if let Some(ref v) = body.branding_app_name {
        sqlx::query("UPDATE tenants SET branding_app_name = $1, updated_at = NOW() WHERE id = $2")
            .bind(v)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }
    if let Some(ref v) = body.branding_custom_css {
        sqlx::query(
            "UPDATE tenants SET branding_custom_css = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant.tenant_id)
        .execute(&state.db)
        .await?;
    }
    if let Some(ref v) = body.login_redirect_url {
        let validated = validate_login_redirect_input(&state.config.server_url, &tenant.slug, v)?;
        sqlx::query("UPDATE tenants SET login_redirect_url = $1, updated_at = NOW() WHERE id = $2")
            .bind(validated)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(ref signup_mode) = body.signup_mode {
        let mode = normalize_signup_mode(Some(signup_mode.as_str()))?;
        sqlx::query("UPDATE tenants SET signup_mode = $1, updated_at = NOW() WHERE id = $2")
            .bind(mode)
            .bind(&tenant.tenant_id)
            .execute(&state.db)
            .await?;
    }

    // Invalidate tenant cache so changes take effect immediately
    state.tenant_cache.invalidate(&tenant.slug);

    tracing::info!(tenant_id = %tenant.tenant_id, "Tenant config updated via management API");

    Ok(Json(serde_json::json!({
        "message": "Configuration updated successfully"
    })))
}

#[derive(Debug, Deserialize)]
pub struct ListTenantInvitesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTenantInviteRequest {
    pub email: String,
    pub role: Option<String>,
    pub expires_in_hours: Option<i64>,
}

/// GET /manage/invites (alias: /admin/invites) — List tenant invites.
pub async fn list_tenant_invites(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let params: ListTenantInvitesQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(ListTenantInvitesQuery {
            limit: None,
            offset: None,
            status: None,
        }))
        .0;

    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let status_filter = params
        .status
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_ascii_lowercase());

    sqlx::query(
        "UPDATE tenant_user_invites
         SET status = 'expired', updated_at = NOW()
         WHERE tenant_id = $1
           AND status = 'pending'
           AND expires_at < NOW()",
    )
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await
    .ok();

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            chrono::DateTime<Utc>,
            Option<chrono::DateTime<Utc>>,
            chrono::DateTime<Utc>,
            chrono::DateTime<Utc>,
        ),
    >(
        "SELECT tui.id, tui.email, tui.role_id, r.name, tui.status,
                tui.invited_by_user_id, tui.accepted_by_user_id, tui.expires_at,
                tui.accepted_at, tui.created_at, tui.updated_at
         FROM tenant_user_invites tui
         LEFT JOIN roles r ON r.id = tui.role_id
         WHERE tui.tenant_id = $1
           AND ($2::TEXT IS NULL OR tui.status = $2)
         ORDER BY tui.created_at DESC
         LIMIT $3 OFFSET $4",
    )
    .bind(&tenant.tenant_id)
    .bind(status_filter.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM tenant_user_invites
         WHERE tenant_id = $1
           AND ($2::TEXT IS NULL OR status = $2)",
    )
    .bind(&tenant.tenant_id)
    .bind(status_filter.as_deref())
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let invites: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                email,
                role_id,
                role_name,
                status,
                invited_by_user_id,
                accepted_by_user_id,
                expires_at,
                accepted_at,
                created_at,
                updated_at,
            )| {
                serde_json::json!({
                    "id": id,
                    "email": email,
                    "role_id": role_id,
                    "role_name": role_name.unwrap_or_else(|| "unknown".to_string()),
                    "status": status,
                    "invited_by_user_id": invited_by_user_id,
                    "accepted_by_user_id": accepted_by_user_id,
                    "expires_at": expires_at.to_rfc3339(),
                    "accepted_at": accepted_at.map(|ts| ts.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                    "updated_at": updated_at.to_rfc3339(),
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({
        "invites": invites,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

/// POST /manage/invites (alias: /admin/invites) — Create a tenant invite.
pub async fn create_tenant_invite(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;
    let inviter_claims = req.extensions().get::<signing::Claims>().cloned();

    let body: CreateTenantInviteRequest =
        axum::Json::<CreateTenantInviteRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: email".into(),
            })?
            .0;

    let email = normalize_email(&body.email);
    if email.is_empty() || !is_valid_email(&email) {
        return Err(AppError::BadRequest {
            error: "invalid_email",
            message: "A valid email is required".into(),
        });
    }

    let (role_id, role_name) =
        resolve_tenant_role(&state, &tenant.tenant_id, body.role.as_deref(), "admin").await?;

    let expires_in_hours = body.expires_in_hours.unwrap_or(72).clamp(1, 168);
    let expires_at = Utc::now() + chrono::Duration::hours(expires_in_hours);

    let invite_id = format!("tinv_{}", uuid::Uuid::new_v4());
    let invite_token = format!("tinv_{}", uuid::Uuid::new_v4());
    let token_hash = signing::hash_token(&invite_token);

    sqlx::query(
        "UPDATE tenant_user_invites
         SET status = 'revoked', updated_at = NOW()
         WHERE tenant_id = $1
           AND lower(email) = lower($2)
           AND status = 'pending'",
    )
    .bind(&tenant.tenant_id)
    .bind(&email)
    .execute(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO tenant_user_invites
         (id, tenant_id, email, role_id, token_hash, status, invited_by_user_id, expires_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7, NOW(), NOW())",
    )
    .bind(&invite_id)
    .bind(&tenant.tenant_id)
    .bind(&email)
    .bind(&role_id)
    .bind(&token_hash)
    .bind(inviter_claims.as_ref().map(|claims| claims.sub.as_str()))
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    let invite_url = format!(
        "{}/t/{}/login?invite_token={}",
        state.config.server_url.trim_end_matches('/'),
        tenant.slug,
        invite_token
    );

    let app_name = tenant
        .branding_app_name
        .as_deref()
        .unwrap_or(&tenant.name)
        .to_string();
    let inviter_email = inviter_claims.as_ref().map(|claims| claims.email.as_str());
    let (subject, text, html) =
        email::tenant_invite_email_content(&app_name, &invite_url, &role_name, inviter_email);

    let email_delivery = if email::email_delivery_configured(&state.config) {
        match email::send_email(&state.config, &email, &subject, &text, &html).await {
            Ok(()) => "sent",
            Err(error) => {
                tracing::warn!(tenant_id = %tenant.tenant_id, invite_id = %invite_id, %error, "Failed to send tenant invite email");
                "failed"
            }
        }
    } else {
        tracing::warn!(
            tenant_id = %tenant.tenant_id,
            invite_id = %invite_id,
            "SMTP is not configured; returning invite URL without sending email"
        );
        "not_configured"
    };

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "invite": {
                "id": invite_id,
                "email": email,
                "role_id": role_id,
                "role_name": role_name,
                "status": "pending",
                "expires_at": expires_at.to_rfc3339(),
            },
            "invite_token": invite_token,
            "invite_url": invite_url,
            "email_delivery": email_delivery,
        })),
    ))
}

/// POST /manage/invites/{id}/revoke (alias: /admin/invites/{id}/revoke) — Revoke a pending tenant invite.
pub async fn revoke_tenant_invite(
    State(state): State<AppState>,
    Path(invite_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let result = sqlx::query(
        "UPDATE tenant_user_invites
         SET status = 'revoked', updated_at = NOW()
         WHERE id = $1
           AND tenant_id = $2
           AND status = 'pending'",
    )
    .bind(&invite_id)
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "invite_not_found",
            message: "Pending invite not found".into(),
        });
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "invite_id": invite_id,
    })))
}

/// POST /manage/users/:id/revoke-sessions — Revoke all sessions for a user.
pub async fn revoke_user_sessions(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    // Verify user belongs to this tenant
    let exists =
        sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(&user_id)
            .bind(&tenant.tenant_id)
            .fetch_optional(&state.db)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: format!("No user with id '{}' in this tenant", user_id),
        });
    }

    let result = sqlx::query(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE",
    )
    .bind(&user_id)
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await?;

    tracing::info!(
        user_id = %user_id,
        tenant_id = %tenant.tenant_id,
        revoked = result.rows_affected(),
        "User sessions revoked via management API"
    );

    Ok(Json(serde_json::json!({
        "message": "All sessions revoked",
        "user_id": user_id,
        "revoked_count": result.rows_affected(),
    })))
}

// ── Admin Create User ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AdminCreateUserRequest {
    pub email: String,
    pub password: String,
    pub name: Option<String>,
    pub role: Option<String>,
}

/// POST /manage/users — Admin-create a user for the authenticated tenant.
/// Does NOT issue JWT tokens. The user will login separately.
pub async fn admin_create_user(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant =
        req.extensions()
            .get::<TenantContext>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Missing tenant context".into(),
            })?;

    let body: AdminCreateUserRequest = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    let email = normalize_email(&body.email);

    if email.is_empty() || body.password.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_request",
            message: "Email and password are required".into(),
        });
    }

    if !is_valid_email(&email) {
        return Err(AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid email format".into(),
        });
    }

    password::validate_password_complexity(&body.password).map_err(|msg| AppError::BadRequest {
        error: "weak_password",
        message: msg.into(),
    })?;

    ensure_user_capacity(&state, &tenant.tenant_id, 1).await?;

    let (role_id, role_name) =
        resolve_tenant_role(&state, &tenant.tenant_id, body.role.as_deref(), "user").await?;

    let existing =
        sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE email = $1 AND tenant_id = $2")
            .bind(&email)
            .bind(&tenant.tenant_id)
            .fetch_optional(&state.db)
            .await?;

    if existing.is_some() {
        return Err(AppError::Conflict {
            error: "user_exists",
            message: "A user with this email already exists in this tenant".into(),
        });
    }

    let user_id = format!("usr_{}", uuid::Uuid::new_v4());
    let cred_id = format!("cred_{}", uuid::Uuid::new_v4());
    let hashed = password::hash_password(&body.password)?;
    let name = body.name.as_deref().unwrap_or("");

    let mut tx = state.db.begin().await?;

    sqlx::query(
        "INSERT INTO users (id, email, name, email_verified, tenant_id) VALUES ($1, $2, $3, FALSE, $4)",
    )
    .bind(&user_id)
    .bind(&email)
    .bind(name)
    .bind(&tenant.tenant_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO credentials (id, user_id, provider, provider_user_id, hashed_password, tenant_id) VALUES ($1, $2, 'email', $3, $4, $5)",
    )
    .bind(&cred_id)
    .bind(&user_id)
    .bind(&email)
    .bind(&hashed)
    .bind(&tenant.tenant_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(&user_id)
        .bind(role_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    tracing::info!(
        user_id = %user_id,
        email = %email,
        role = role_name,
        tenant_id = %tenant.tenant_id,
        "User created via management API"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "user": {
                "id": user_id,
                "email": email,
                "name": name,
                "email_verified": false,
                "roles": [role_name],
            },
            "note": "User created. They must login separately to obtain tokens."
        })),
    ))
}

/// PATCH /manage/tenants/:id/security — Update tenant security configuration with plan-gated validation.
///
/// Accepts a partial JSON object. Only provided keys are merged into the existing config
/// (shallow patch semantics). The merged config is validated against the tenant's current plan,
/// and 403 is returned if any features require a higher plan tier.
pub async fn update_security_config(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    // Extract tenant context from the mgmt key (injected by require_management_auth)
    let tenant = req
        .extensions()
        .get::<TenantContext>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Missing tenant context".into(),
        })?;

    // Verify the path tenant_id matches the authenticated tenant
    if tenant.tenant_id != tenant_id {
        return Err(AppError::Forbidden {
            error: "tenant_mismatch",
            message: "Tenant ID in path does not match the authenticated tenant".into(),
        });
    }

    // Parse the request body
    let updates: serde_json::Value = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body — expected a JSON object".into(),
        })?
        .0;

    if !updates.is_object() {
        return Err(AppError::BadRequest {
            error: "invalid_request",
            message: "Request body must be a JSON object".into(),
        });
    }

    // 1. Load current security_config from DB
    let current_json: serde_json::Value = sqlx::query_scalar(
        "SELECT COALESCE(security_config, '{}') FROM tenants WHERE id = $1",
    )
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: format!("No tenant with id '{}'", tenant_id),
    })?;

    // 2. Shallow merge: only overwrite keys present in the update
    let mut merged = current_json.clone();
    if let (Some(base), Some(patch)) = (merged.as_object_mut(), updates.as_object()) {
        for (key, value) in patch {
            base.insert(key.clone(), value.clone());
        }
    }

    // 3. Parse merged config with TenantSecurityConfig::from_json()
    let config = TenantSecurityConfig::from_json(&merged);

    // 4. Load tenant's plan from DB
    let plan: String = sqlx::query_scalar(
        "SELECT COALESCE(te.plan, 'trial') FROM tenant_entitlements te WHERE te.tenant_id = $1",
    )
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or_else(|| "trial".to_string());

    // 5. Validate with config.validate_for_plan(plan)
    let violations = config.validate_for_plan(&plan);

    // 6. If violations: return 403 Forbidden with violation messages
    if !violations.is_empty() {
        return Err(AppError::Forbidden {
            error: "plan_violation",
            message: format!(
                "Security config violates plan '{}' restrictions: {}",
                plan,
                violations.join("; ")
            ),
        });
    }

    // 7. Save merged config to DB
    let config_value = serde_json::to_value(&config).map_err(|_| AppError::InternalError {
        error: "server_error",
        message: "Failed to serialize security config".into(),
    })?;

    sqlx::query("UPDATE tenants SET security_config = $1, updated_at = NOW() WHERE id = $2")
        .bind(&config_value)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    // 8. Invalidate tenant cache
    state.tenant_cache.invalidate(&tenant.slug);

    tracing::info!(
        tenant_id = %tenant_id,
        plan = %plan,
        "Tenant security config updated"
    );

    // 9. Return JSON with the new security_config
    Ok(Json(serde_json::json!({
        "security_config": config,
        "plan": plan,
    })))
}
