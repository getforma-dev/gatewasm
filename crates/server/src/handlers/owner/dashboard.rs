use axum::response::{Html, IntoResponse, Redirect};
use axum::extract::State;

use crate::frontend::FrontendSnapshot;
use crate::handlers::auth as auth_handlers;
use crate::html_template::{render_page, PageConfig};
use crate::jwt::signing;
use crate::{AppState, RenderMode};

struct OwnerRefreshSessionValidation {
    valid: bool,
    reason: &'static str,
}

async fn validate_owner_refresh_session(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<OwnerRefreshSessionValidation, sqlx::Error> {
    let refresh_tokens =
        auth_handlers::extract_refresh_cookie_candidates(headers, Some("platform"));
    if refresh_tokens.is_empty() {
        return Ok(OwnerRefreshSessionValidation {
            valid: false,
            reason: "missing_cookie",
        });
    }

    let refresh_hashes: Vec<String> = refresh_tokens
        .iter()
        .map(|token| signing::hash_token(token))
        .collect();

    let rows = sqlx::query_as::<_, (bool, bool, bool, bool, bool, bool)>(
        "SELECT
            rt.revoked,
            (rt.expires_at > NOW()) AS not_expired,
            (rt.auth_method = 'sso') AS auth_method_ok,
            rt.mfa_verified,
            (u.tenant_id = 'ten_platform') AS user_tenant_ok,
            EXISTS(
                SELECT 1
                FROM user_roles ur
                WHERE ur.user_id = rt.user_id
                  AND ur.tenant_id = 'ten_platform'
                  AND ur.role_id = 'role_platform_owner'
            ) AS owner_role_ok
         FROM refresh_tokens rt
         JOIN users u ON u.id = rt.user_id
         WHERE rt.token_hash = ANY($1)
           AND rt.tenant_id = 'ten_platform'
         ORDER BY rt.expires_at DESC",
    )
    .bind(&refresh_hashes)
    .fetch_all(&state.db)
    .await?;

    if rows.is_empty() {
        return Ok(OwnerRefreshSessionValidation {
            valid: false,
            reason: "token_not_found",
        });
    };

    let is_prod = state.config.is_production();

    if rows.iter().any(
        |(revoked, not_expired, auth_method_ok, mfa_verified, user_tenant_ok, owner_role_ok)| {
            !*revoked
                && *not_expired
                && (!is_prod || *auth_method_ok)
                && (!is_prod || *mfa_verified)
                && *user_tenant_ok
                && *owner_role_ok
        },
    ) {
        return Ok(OwnerRefreshSessionValidation {
            valid: true,
            reason: "ok",
        });
    }

    let (revoked, not_expired, auth_method_ok, mfa_verified, user_tenant_ok, owner_role_ok) =
        rows[0];
    let reason = if revoked {
        "token_revoked"
    } else if !not_expired {
        "token_expired"
    } else if !user_tenant_ok {
        "wrong_tenant"
    } else if !owner_role_ok {
        "missing_owner_role"
    } else if is_prod && !auth_method_ok {
        "owner_sso_required"
    } else if is_prod && !mfa_verified {
        "mfa_not_verified"
    } else {
        "session_invalid"
    };

    Ok(OwnerRefreshSessionValidation {
        valid: false,
        reason,
    })
}

/// GET /platform/admin/dashboard — Provider control-plane dashboard.
pub async fn dashboard_page(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let frontend = FrontendSnapshot::capture(&state);
    let login_redirect = "/platform/login".to_string();

    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    match validate_owner_refresh_session(&state, &headers).await {
        Ok(result) if result.valid => {}
        Ok(result) => {
            tracing::warn!(
                request_id = request_id.as_deref().unwrap_or(""),
                reason = result.reason,
                "Owner dashboard session invalid; redirecting to login"
            );
            return Redirect::temporary(&login_redirect).into_response();
        }
        Err(error) => {
            tracing::error!(%error, "Failed to validate owner refresh session for dashboard route");
            return Redirect::temporary(&login_redirect).into_response();
        }
    }

    let page = render_page(&PageConfig {
        title: "Admin Dashboard — Forma Auth",
        route_pattern: "/platform/admin/dashboard",
        manifest: frontend.manifest(),
        config_script: None,
        config_json: None,
        body_class: Some("owner-dashboard min-h-screen antialiased"),
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;--grid-opacity:0.25;--glow-size:700px;--glow-intensity:0.04;}"),
        body_prefix: None,
        render_mode: RenderMode::Phase1ClientMount,
        ir_module: None,
        slots: None,
    });

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (axum::http::header::HeaderName::from_static("content-security-policy"), page.csp),
        ],
        Html(page.html),
    )
        .into_response()
}
