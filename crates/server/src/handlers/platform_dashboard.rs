use axum::{
    extract::State,
    response::{Html, IntoResponse},
};

use crate::frontend::FrontendSnapshot;
use crate::html_template::{render_page, PageConfig};
use crate::{AppState, RenderMode};

/// GET /platform/console — Platform console for tenant owners.
/// Serves an HTML shell that loads external CSS and JS bundles via /_assets/.
pub async fn dashboard_page(State(state): State<AppState>) -> impl IntoResponse {
    let frontend = FrontendSnapshot::capture(&state);
    let config_script = format!(
        "window.__FORMA_SERVER_URL__=\"{}\";",
        state.config.server_url
    );

    let page = render_page(&PageConfig {
        title: "Platform Console — Forma Auth",
        route_pattern: "/platform/console",
        manifest: frontend.manifest(),
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;}"),
        body_prefix: None,
        render_mode: RenderMode::Phase1ClientMount,
        ir_module: None,
        slots: None,
    });

    // Add frame-src 'self' to allow the onboarding wizard's live preview iframe
    // of the tenant login page (same origin)
    let csp = page.csp + "; frame-src 'self'";

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (axum::http::header::HeaderName::from_static("content-security-policy"), csp),
        ],
        Html(page.html),
    ).into_response()
}
