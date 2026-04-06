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
/// `require_platform_member`. Downstream middleware and handlers can retrieve
/// this from `req.extensions()`.
#[derive(Debug, Clone)]
pub struct PlatformMember {
    pub member_id: String,
    pub user_id: String,
    pub email: String,
    pub role: String, // owner | admin | editor | viewer
}

impl PlatformMember {
    /// Returns `true` when the member holds the `owner` role.
    pub fn is_owner(&self) -> bool {
        self.role == "owner"
    }

    /// Returns `true` for `owner` or `admin` roles.
    pub fn is_admin_or_above(&self) -> bool {
        self.role == "owner" || self.role == "admin"
    }

    /// Returns `true` for `owner`, `admin`, or `editor` roles.
    pub fn is_editor_or_above(&self) -> bool {
        self.role == "owner" || self.role == "admin" || self.role == "editor"
    }
}

/// Middleware: verifies the caller is an active platform team member.
///
/// Requires:
/// 1) JWT Claims injected by `require_auth` (runs first in the layer stack)
/// 2) `claims.tenant_id == "ten_platform"`
/// 3) An active row in `platform_members` for the caller's `user_id`
///
/// On success, injects `PlatformMember` into request extensions so that
/// `require_platform_team_admin` and `require_platform_editor_or_above`
/// can consume it without an additional DB round-trip.
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

    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT pm.id, pm.user_id, pm.role, u.email \
         FROM platform_members pm \
         JOIN users u ON u.id = pm.user_id \
         WHERE pm.user_id = $1 AND pm.status = 'active'",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?;

    let (member_id, fetched_user_id, role, email) = row.ok_or(AppError::Forbidden {
        error: "not_platform_member",
        message: "You are not an active platform team member".into(),
    })?;

    tracing::debug!(
        user_id = %fetched_user_id,
        role = %role,
        "Platform member authenticated"
    );

    req.extensions_mut().insert(PlatformMember {
        member_id,
        user_id: fetched_user_id,
        email,
        role,
    });

    Ok(next.run(req).await)
}

/// Middleware: requires `owner` or `admin` role.
///
/// Must run AFTER `require_platform_member` (needs `PlatformMember` in
/// extensions).
pub async fn require_platform_team_admin(
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

    if !member.is_admin_or_above() {
        return Err(AppError::Forbidden {
            error: "insufficient_role",
            message: "Owner or Admin role required for this action".into(),
        });
    }

    tracing::debug!(
        user_id = %member.user_id,
        role = %member.role,
        "Platform team admin check passed"
    );

    Ok(next.run(req).await)
}

/// Middleware: requires `owner`, `admin`, or `editor` role (rejects `viewer`).
///
/// Must run AFTER `require_platform_member` (needs `PlatformMember` in
/// extensions).
pub async fn require_platform_editor_or_above(
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

    if !member.is_editor_or_above() {
        return Err(AppError::Forbidden {
            error: "insufficient_role",
            message: "Editor or above role required for this action".into(),
        });
    }

    tracing::debug!(
        user_id = %member.user_id,
        role = %member.role,
        "Platform team editor-or-above check passed"
    );

    Ok(next.run(req).await)
}

// ── Self-serve RBAC middleware ─────────────────────────────────────────

/// Middleware: resolves `PlatformMember` for the authenticated `PlatformUser`.
///
/// Runs after `require_platform_user` (needs `PlatformUser` in extensions).
/// If the user is an active platform member, `PlatformMember` is inserted into
/// request extensions for downstream middleware. If the user is NOT a member
/// (e.g. a tenant owner who hasn't been added to the team table), the request
/// continues without `PlatformMember` — downstream checks decide whether that
/// is acceptable.
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
            "SELECT pm.id, pm.user_id, pm.role, u.email \
             FROM platform_members pm \
             JOIN users u ON u.id = pm.user_id \
             WHERE pm.user_id = $1 AND pm.status = 'active'",
        )
        .bind(&uid)
        .fetch_optional(&state.db)
        .await?;

        if let Some((member_id, fetched_user_id, role, email)) = row {
            tracing::debug!(
                user_id = %fetched_user_id,
                role = %role,
                "Resolved platform member for self-serve request"
            );

            req.extensions_mut().insert(PlatformMember {
                member_id,
                user_id: fetched_user_id,
                email,
                role,
            });
        }
    }

    Ok(next.run(req).await)
}

/// Middleware: rejects viewers on mutation requests (non-GET/HEAD).
///
/// Allows GET and HEAD requests through for all roles (including viewer).
/// Must run after `resolve_platform_member` so that `PlatformMember` is
/// available in extensions. If no `PlatformMember` is present (legacy owner
/// path), the request is allowed through.
pub async fn require_write_access(
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if req.method() == Method::GET || req.method() == Method::HEAD {
        return Ok(next.run(req).await);
    }

    if let Some(member) = req.extensions().get::<PlatformMember>() {
        if !member.is_editor_or_above() {
            return Err(AppError::Forbidden {
                error: "viewer_read_only",
                message: "Viewer role has read-only access".into(),
            });
        }
    }

    Ok(next.run(req).await)
}

/// Middleware: verifies the caller has access to the specific tenant in the URL.
///
/// Owner/Admin roles have access to all tenants. Editor/Viewer must be
/// assigned via `platform_member_tenants`. If no `PlatformMember` is present
/// (legacy owner path), the request is allowed through — the individual
/// handler's `owner_user_id` check still applies.
///
/// Extracts the tenant ID from the URL path pattern
/// `/platform/self-serve/tenants/{id}/...`.
pub async fn require_tenant_access(
    State(state): State<AppState>,
    req: Request,
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
        if let Some(member) = req.extensions().get::<PlatformMember>() {
            if !member.is_admin_or_above() {
                let assigned = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM platform_member_tenants \
                     WHERE member_id = $1 AND tenant_id = $2",
                )
                .bind(&member.member_id)
                .bind(&tid)
                .fetch_one(&state.db)
                .await
                .unwrap_or(0);

                if assigned == 0 {
                    return Err(AppError::Forbidden {
                        error: "tenant_not_assigned",
                        message: "You do not have access to this tenant".into(),
                    });
                }
            }
        }
    }

    Ok(next.run(req).await)
}
