use axum::{
    extract::{Extension, State},
    response::{Html, IntoResponse, Response},
};

use crate::assets::AssetManifest;
use crate::handlers::hosted_login::{html_escape, json_for_script};
use crate::html_template::{render_page, PageConfig};
use crate::middleware::tenant::TenantContext;
use crate::{AppState, RenderMode};

/// GET /dashboard — Post-login dashboard built with Forma.js.
/// Goes through extract_tenant middleware but NOT require_auth —
/// auth is handled client-side so expired tokens get HTML (not 401 JSON).
pub async fn dashboard_page(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
) -> Response {
    render_dashboard_page(tenant, false, &state.asset_manifest)
}

/// GET /admin — Tenant admin console route, separate from the end-user dashboard.
pub async fn admin_console_page(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
) -> Response {
    render_dashboard_page(tenant, true, &state.asset_manifest)
}

fn render_dashboard_page(
    tenant: TenantContext,
    admin_console_mode: bool,
    manifest: &AssetManifest,
) -> Response {
    let app_name = tenant.branding_app_name.as_deref().unwrap_or(&tenant.name);
    let primary_color = &tenant.branding_primary_color;

    let config_json = serde_json::json!({
        "slug": tenant.slug,
        "name": app_name,
        "is_admin": false,
        "admin_console_mode": admin_console_mode,
        "initial_view": if admin_console_mode { "stats" } else { "profile" },
        "server_url": format!("/t/{}", tenant.slug),
        "login_redirect_url": tenant.login_redirect_url,
        "primary_color": primary_color,
        "cdn_domain": "cdn.getforma.dev",
        "client_id": format!("{}-hosted", tenant.slug),
    });

    let config_script = format!(
        "window.__TENANT__={};",
        json_for_script(&config_json)
    );

    let page_title = if admin_console_mode {
        format!("{} — Admin Console", html_escape(app_name))
    } else {
        format!("{} — Dashboard", html_escape(app_name))
    };

    let route_pattern = if admin_console_mode {
        "/t/:slug/admin"
    } else {
        "/t/:slug/dashboard"
    };

    // Personality CSS: different ambience per page, plus tenant accent color
    let personality_css = if admin_console_mode {
        format!("body{{background:#0a0a0c;color:#e8e6e3;--grid-opacity:0.25;--glow-size:700px;--glow-intensity:0.04;--accent:{pc};}}", pc = primary_color)
    } else {
        format!("body{{background:#0a0a0c;color:#e8e6e3;--grid-opacity:0.20;--glow-size:800px;--glow-intensity:0.05;--accent:{pc};}}", pc = primary_color)
    };

    let page = render_page(&PageConfig {
        title: &page_title,
        route_pattern,
        manifest,
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some(&personality_css),
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
