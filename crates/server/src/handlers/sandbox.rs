use axum::{
    extract::{Extension, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;

use crate::handlers::auth::extract_refresh_cookie;
use crate::handlers::hosted_login::{html_escape, json_for_script};
use crate::html_template::{render_page, PageConfig};
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::{AppState, RenderMode};

/// GET /sandbox — Debug sandbox page for platform owners.
///
/// Cross-plane auth gate:
/// - Plane 3 (tenant session): user must have a valid refresh cookie for this tenant.
/// - Plane 2 (platform ownership): user's email must match the tenant's `owner_email`.
///
/// Unauthenticated visitors → redirect to login.
/// Authenticated but not a platform owner → redirect to dashboard.
/// Both checks pass → render the sandbox page.
pub async fn sandbox_page(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    req: axum::extract::Request,
) -> Response {
    let slug = &tenant.slug;

    // ── Plane 3 check: does the user have a valid tenant session? ──────
    let session_user_email = match extract_refresh_cookie(req.headers(), Some(slug)) {
        Some(rt) => {
            let hash = signing::hash_token(&rt);
            // Look up the refresh token and join to users to get the email in one query.
            let row = sqlx::query_as::<_, (String,)>(
                "SELECT u.email FROM refresh_tokens rt \
                 JOIN users u ON u.id = rt.user_id \
                 WHERE rt.token_hash = $1 AND rt.revoked = FALSE \
                 AND rt.expires_at > $2 AND rt.tenant_id = $3",
            )
            .bind(&hash)
            .bind(Utc::now())
            .bind(&tenant.tenant_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

            match row {
                Some((email,)) => email,
                None => {
                    // Cookie exists but session is invalid/expired — redirect to login.
                    let login_url = format!(
                        "/t/{}/login?redirect=/t/{}/sandbox",
                        slug, slug
                    );
                    return Redirect::temporary(&login_url).into_response();
                }
            }
        }
        None => {
            // No refresh cookie at all — redirect to login.
            let login_url = format!(
                "/t/{}/login?redirect=/t/{}/sandbox",
                slug, slug
            );
            return Redirect::temporary(&login_url).into_response();
        }
    };

    // ── Plane 2 check: is this user the platform owner of the tenant? ──
    let is_owner = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(\
            SELECT 1 FROM tenants \
            WHERE id = $1 AND LOWER(owner_email) = LOWER($2) AND status != 'deleted'\
        )",
    )
    .bind(&tenant.tenant_id)
    .bind(&session_user_email)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if !is_owner {
        // Authenticated end-user but not the platform owner → dashboard.
        let dashboard_url = format!("/t/{}/dashboard", slug);
        return Redirect::temporary(&dashboard_url).into_response();
    }

    // ── Both planes pass — render the sandbox page ─────────────────────
    let tenant_name = tenant.branding_app_name.as_deref().unwrap_or(&tenant.name);

    let config_script = format!(
        "window.__TENANT__={};window.__FORMA_SERVER_URL__=\"{}\";",
        json_for_script(&serde_json::json!({
            "slug": slug,
            "name": tenant_name,
        })),
        state.config.server_url,
    );

    let page = render_page(&PageConfig {
        title: &format!("{} — Sandbox", html_escape(tenant_name)),
        route_pattern: "/t/:slug/sandbox",
        manifest: &state.asset_manifest,
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;}"),
        body_prefix: None,
        render_mode: RenderMode::Phase1ClientMount,
        ir_module: None,
        slots: None,
    });

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (
                axum::http::header::HeaderName::from_static("content-security-policy"),
                page.csp,
            ),
        ],
        Html(page.html),
    )
        .into_response()
}
