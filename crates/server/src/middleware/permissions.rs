use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::error::AppError;
use crate::jwt::signing;
use crate::AppState;

/// Resolved permissions for the authenticated user, injected into request extensions.
#[derive(Debug, Clone)]
pub struct UserPermissions {
    pub permissions: Vec<String>,
}

impl UserPermissions {
    pub fn has(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm || p == "*")
    }
}

/// Middleware that resolves user permissions from their assigned roles.
/// Must be applied AFTER require_auth (which injects Claims into extensions).
pub async fn resolve_permissions(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if let Some(claims) = req.extensions().get::<signing::Claims>().cloned() {
        // Scope permissions to the tenant from the request context (or the JWT tenant_id).
        // System roles (tenant_id IS NULL) are always included.
        let tenant_id = req
            .extensions()
            .get::<crate::middleware::tenant::TenantContext>()
            .map(|t| t.tenant_id.clone())
            .or_else(|| claims.tenant_id.clone());

        let perms: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT val FROM (\
               SELECT jsonb_array_elements_text(r.permissions_jsonb) AS val \
               FROM roles r JOIN user_roles ur ON ur.role_id = r.id \
               WHERE ur.user_id = $1 AND (r.tenant_id IS NULL OR r.tenant_id = $2)\
             ) sub",
        )
        .bind(&claims.sub)
        .bind(&tenant_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        req.extensions_mut()
            .insert(UserPermissions { permissions: perms });
    }

    Ok(next.run(req).await)
}
