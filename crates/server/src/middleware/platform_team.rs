use axum::{
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::Response,
};

use crate::error::AppError;
use crate::jwt::signing;
use crate::middleware::platform_user::PlatformUser;
use crate::AppState;

/// Extracted platform team member info, inserted into request extensions by
/// `require_platform_member` or `resolve_platform_member`.
///
/// Ownership is structural: `account_owner_user_id == user_id` means
/// "I am the account owner of this team."
#[derive(Debug, Clone)]
pub struct PlatformMember {
    pub member_id: String,
    pub account_owner_user_id: String,
    pub user_id: String,
    pub email: String,
}

impl PlatformMember {
    /// Returns `true` when this member IS the account owner (structural ownership).
    pub fn is_account_owner(&self) -> bool {
        self.account_owner_user_id == self.user_id
    }
}

/// Per-tenant access role injected by `require_tenant_access`.
/// Downstream handlers can read this to make authorization decisions.
#[derive(Debug, Clone)]
pub struct TenantAccessRole(pub String); // "owner" | "admin" | "editor" | "viewer"

/// Middleware: verifies the caller is an active platform team **account owner**.
///
/// Used on team management routes (`/platform/admin/team/*`) where only the
/// account owner (the customer who created tenants) can manage their team.
///
/// Requires:
/// 1) JWT Claims injected by `require_auth`
/// 2) `claims.tenant_id == "ten_platform"`
/// 3) A self-referencing row in `platform_members` (account_owner_user_id = user_id)
///
/// On success, injects `PlatformMember` into request extensions.
pub async fn require_platform_member(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let claims = req
        .extensions()
        .get::<signing::Claims>()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    if claims.tenant_id.as_deref() != Some("ten_platform") {
        return Err(AppError::Forbidden {
            error: "forbidden",
            message: "Platform team access is only available on platform accounts".into(),
        });
    }

    let user_id = claims.sub.clone();

    // Find the self-referencing row: account_owner_user_id = user_id
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT pm.id, pm.account_owner_user_id, pm.user_id, u.email \
         FROM platform_members pm \
         JOIN users u ON u.id = pm.user_id \
         WHERE pm.account_owner_user_id = pm.user_id \
           AND pm.user_id = $1 \
           AND pm.status = 'active'",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?;

    let (member_id, account_owner_user_id, fetched_user_id, email) =
        row.ok_or(AppError::Forbidden {
            error: "not_account_owner",
            message: "You must be an account owner to access team management".into(),
        })?;

    tracing::debug!(
        user_id = %fetched_user_id,
        "Platform account owner authenticated"
    );

    req.extensions_mut().insert(PlatformMember {
        member_id,
        account_owner_user_id,
        user_id: fetched_user_id,
        email,
    });

    Ok(next.run(req).await)
}

/// Middleware: requires the caller to be an account owner.
///
/// Must run AFTER `require_platform_member` (needs `PlatformMember` in
/// extensions). This is a guard for team management routes.
pub async fn require_account_owner(
    State(_state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let member = req
        .extensions()
        .get::<PlatformMember>()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Platform membership context missing — ensure require_platform_member runs first".into(),
        })?;

    if !member.is_account_owner() {
        return Err(AppError::Forbidden {
            error: "not_account_owner",
            message: "Only the account owner can manage team members".into(),
        });
    }

    tracing::debug!(
        user_id = %member.user_id,
        "Account owner check passed"
    );

    Ok(next.run(req).await)
}

// ── Self-serve RBAC middleware ─────────────────────────────────────────

/// Middleware: resolves `PlatformMember` for the authenticated `PlatformUser`.
///
/// Runs after `require_platform_user` (needs `PlatformUser` in extensions).
/// Prefers the self-referencing row (account owner) if one exists, otherwise
/// picks any active membership. If no membership at all, the request continues
/// without `PlatformMember` — downstream checks decide whether that's acceptable.
pub async fn resolve_platform_member(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let user_id = req
        .extensions()
        .get::<PlatformUser>()
        .map(|u| u.user_id.clone());

    if let Some(uid) = user_id {
        let row = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT pm.id, pm.account_owner_user_id, pm.user_id, u.email \
             FROM platform_members pm \
             JOIN users u ON u.id = pm.user_id \
             WHERE pm.user_id = $1 AND pm.status = 'active' \
             ORDER BY (pm.account_owner_user_id = pm.user_id) DESC \
             LIMIT 1",
        )
        .bind(&uid)
        .fetch_optional(&state.db)
        .await?;

        if let Some((member_id, account_owner_user_id, fetched_user_id, email)) = row {
            tracing::debug!(
                user_id = %fetched_user_id,
                is_account_owner = %(account_owner_user_id == fetched_user_id),
                "Resolved platform member for self-serve request"
            );

            req.extensions_mut().insert(PlatformMember {
                member_id,
                account_owner_user_id,
                user_id: fetched_user_id,
                email,
            });
        }
    }

    Ok(next.run(req).await)
}

/// Middleware: verifies the caller has access to the specific tenant in the URL,
/// and enforces write-access restrictions for viewers.
///
/// Logic:
/// 1. Extract tenant_id from `/platform/self-serve/tenants/{id}/...`
/// 2. If caller is account owner AND tenant's `owner_user_id` matches → full access
/// 3. Otherwise check `platform_member_tenants` for a matching row
/// 4. Viewer role + non-GET/HEAD → 403 (read-only)
/// 5. Inject `TenantAccessRole` for downstream handlers
///
/// If no `PlatformMember` is present (legacy owner path), falls back to
/// checking `PlatformUser.user_id == tenants.owner_user_id`.
pub async fn require_tenant_access(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let tenant_id = req
        .uri()
        .path()
        .strip_prefix("/platform/self-serve/tenants/")
        .and_then(|rest| rest.split('/').next())
        .filter(|s| !s.is_empty())
        .map(|s| urlencoding::decode(s).unwrap_or_default().into_owned());

    if let Some(tid) = tenant_id {
        // Look up the tenant's owner to decide access path.
        let tenant_owner: Option<String> = sqlx::query_scalar(
            "SELECT owner_user_id FROM tenants WHERE id = $1 AND status != 'deleted'",
        )
        .bind(&tid)
        .fetch_optional(&state.db)
        .await?;

        let tenant_owner = match tenant_owner {
            Some(o) => o,
            None => {
                return Err(AppError::NotFound {
                    error: "tenant_not_found",
                    message: "Tenant not found".into(),
                });
            }
        };

        // Determine the caller's user_id from PlatformMember or PlatformUser.
        let caller_user_id = req
            .extensions()
            .get::<PlatformMember>()
            .map(|m| m.user_id.clone())
            .or_else(|| {
                req.extensions()
                    .get::<PlatformUser>()
                    .map(|u| u.user_id.clone())
            });

        let caller_user_id = match caller_user_id {
            Some(uid) => uid,
            None => {
                return Err(AppError::Unauthorized {
                    error: "unauthorized",
                    message: "Not authenticated".into(),
                });
            }
        };

        // Path A: caller IS the tenant owner → full access
        if caller_user_id == tenant_owner {
            req.extensions_mut()
                .insert(TenantAccessRole("owner".to_string()));
            return Ok(next.run(req).await);
        }

        // Path B: caller is an invited team member → check platform_member_tenants
        if let Some(member) = req.extensions().get::<PlatformMember>() {
            // Find their membership row scoped to the tenant owner's account
            let role: Option<String> = sqlx::query_scalar(
                "SELECT pmt.role FROM platform_member_tenants pmt \
                 JOIN platform_members pm ON pm.id = pmt.member_id \
                 WHERE pm.user_id = $1 \
                   AND pm.account_owner_user_id = $2 \
                   AND pmt.tenant_id = $3 \
                   AND pm.status = 'active'",
            )
            .bind(&member.user_id)
            .bind(&tenant_owner)
            .bind(&tid)
            .fetch_optional(&state.db)
            .await?;

            match role {
                Some(r) => {
                    // Viewer + mutation → reject
                    if r == "viewer"
                        && req.method() != Method::GET
                        && req.method() != Method::HEAD
                    {
                        return Err(AppError::Forbidden {
                            error: "viewer_read_only",
                            message: "Viewer role has read-only access".into(),
                        });
                    }
                    req.extensions_mut()
                        .insert(TenantAccessRole(r));
                    return Ok(next.run(req).await);
                }
                None => {
                    return Err(AppError::Forbidden {
                        error: "tenant_not_assigned",
                        message: "You do not have access to this tenant".into(),
                    });
                }
            }
        }

        // No PlatformMember at all and not the tenant owner → forbidden
        return Err(AppError::Forbidden {
            error: "tenant_not_assigned",
            message: "You do not have access to this tenant".into(),
        });
    }

    // No tenant_id in URL (e.g. /platform/self-serve/tenants list endpoint) → pass through
    Ok(next.run(req).await)
}
