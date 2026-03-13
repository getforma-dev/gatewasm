use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use sqlx::postgres::PgPool;

use crate::error::AppError;
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

#[derive(Debug, Clone)]
pub struct Entitlements {
    pub plan: String,
    pub max_users: i32,
    pub max_organizations: i32,
    pub allow_custom_roles: bool,
    pub allow_webhooks: bool,
    pub allow_organizations: bool,
}

fn tenant_id_from_request(req: &Request) -> Result<String, AppError> {
    if let Some(tenant) = req.extensions().get::<TenantContext>() {
        return Ok(tenant.tenant_id.clone());
    }

    if let Some(claims) = req.extensions().get::<signing::Claims>() {
        if let Some(tenant_id) = &claims.tenant_id {
            return Ok(tenant_id.clone());
        }
    }

    Err(AppError::Unauthorized {
        error: "unauthorized",
        message: "Missing tenant context".into(),
    })
}

/// Load entitlements for a tenant. Falls back to legacy tenant fields when no row exists.
pub async fn load_entitlements(pool: &PgPool, tenant_id: &str) -> Result<Entitlements, AppError> {
    let row = sqlx::query_as::<_, (String, i32, i32, bool, bool, bool)>(
        "SELECT plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations \
         FROM tenant_entitlements WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    if let Some((
        plan,
        max_users,
        max_organizations,
        allow_custom_roles,
        allow_webhooks,
        allow_organizations,
    )) = row
    {
        return Ok(Entitlements {
            plan,
            max_users,
            max_organizations,
            allow_custom_roles,
            allow_webhooks,
            allow_organizations,
        });
    }

    let fallback = sqlx::query_as::<_, (String, i32)>(
        "SELECT COALESCE(plan, 'trial'), COALESCE(max_users, 1000) FROM tenants WHERE id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    let (plan, max_users) = fallback.unwrap_or_else(|| ("trial".to_string(), 1000));
    Ok(Entitlements {
        plan,
        max_users,
        max_organizations: 3,
        allow_custom_roles: true,
        allow_webhooks: true,
        allow_organizations: true,
    })
}

pub async fn ensure_user_capacity(
    state: &AppState,
    tenant_id: &str,
    additional_users: i64,
) -> Result<(), AppError> {
    let ent = load_entitlements(&state.db, tenant_id).await?;
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    if user_count + additional_users > ent.max_users as i64 {
        return Err(AppError::Forbidden {
            error: "user_limit_reached",
            message: format!(
                "User limit reached for this tenant (limit: {}). Upgrade plan or increase entitlements.",
                ent.max_users
            ),
        });
    }
    Ok(())
}

pub async fn ensure_organization_capacity(
    state: &AppState,
    tenant_id: &str,
    additional_orgs: i64,
) -> Result<(), AppError> {
    let ent = load_entitlements(&state.db, tenant_id).await?;
    let org_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    if org_count + additional_orgs > ent.max_organizations as i64 {
        return Err(AppError::Forbidden {
            error: "organization_limit_reached",
            message: format!(
                "Organization limit reached for this tenant (limit: {}). Upgrade plan or increase entitlements.",
                ent.max_organizations
            ),
        });
    }
    Ok(())
}

async fn require_feature<F>(
    State(state): State<AppState>,
    req: Request,
    next: Next,
    feature_name: &'static str,
    feature_enabled: F,
) -> Result<Response, AppError>
where
    F: Fn(&Entitlements) -> bool,
{
    let tenant_id = tenant_id_from_request(&req)?;
    let ent = load_entitlements(&state.db, &tenant_id).await?;

    if !feature_enabled(&ent) {
        return Err(AppError::Forbidden {
            error: "feature_not_enabled",
            message: format!(
                "Feature '{}' is not enabled for plan '{}' on this tenant.",
                feature_name, ent.plan
            ),
        });
    }

    Ok(next.run(req).await)
}

/// Middleware for endpoints requiring custom-role capability.
pub async fn require_custom_roles(
    state: State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    require_feature(state, req, next, "custom_roles", |e| e.allow_custom_roles).await
}

/// Middleware for endpoints requiring webhooks capability.
pub async fn require_webhooks(
    state: State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    require_feature(state, req, next, "webhooks", |e| e.allow_webhooks).await
}

/// Middleware for endpoints requiring organizations capability.
pub async fn require_organizations(
    state: State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    require_feature(state, req, next, "organizations", |e| e.allow_organizations).await
}
