use axum::{
    extract::{Extension, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;

use crate::handlers::auth::extract_refresh_cookie;
use crate::html_template::{render_page, PageConfig};
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::{AppState, RenderMode};

/// GET /login — Serves a self-contained hosted login page built with Forma.js.
/// Goes through extract_tenant middleware (for branding) but NOT require_auth.
/// If the user has a valid refresh cookie, returns 302 redirect (zero flash).
pub async fn login_page(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    req: axum::extract::Request,
) -> Response {
    // Server-side redirect: if user has a valid refresh cookie, skip the login page
    if let Some(rt) = extract_refresh_cookie(req.headers(), Some(&tenant.slug)) {
        let hash = signing::hash_token(&rt);
        let valid = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM refresh_tokens WHERE token_hash = $1 AND revoked = FALSE AND expires_at > $2 AND tenant_id = $3)"
        )
        .bind(&hash)
        .bind(Utc::now())
        .bind(&tenant.tenant_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

        if valid {
            return Redirect::temporary(&tenant.login_redirect_url).into_response();
        }
    }

    // no-store prevents bfcache — back button makes a fresh server request
    let page = login_page_render(&state, &tenant).await;
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

async fn login_page_render(
    state: &AppState,
    tenant: &TenantContext,
) -> crate::html_template::PageOutput {
    let app_name = tenant.branding_app_name.as_deref().unwrap_or(&tenant.name);
    let primary_color = &tenant.branding_primary_color;
    let logo_html = tenant
        .branding_logo_url
        .as_ref()
        .map(|url| {
            format!(
                r#"<div class="logo-area"><img src="{}" alt="{}" class="logo-img" /></div>"#,
                html_escape(url),
                html_escape(app_name)
            )
        })
        .unwrap_or_default();

    // Ensure a default hosted-login oauth_client exists for this tenant.
    let hosted_client_id = format!("{}-hosted", tenant.slug);
    let hosted_redirect_uri = format!("{}/t/{}/login", state.config.server_url, tenant.slug);
    let _created = sqlx::query(
        "INSERT INTO oauth_clients (id, redirect_uri, name, tenant_id) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (id) DO UPDATE SET redirect_uri = $2",
    )
    .bind(&hosted_client_id)
    .bind(&hosted_redirect_uri)
    .bind(format!("{} Hosted Login", app_name))
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await;

    let config_json = serde_json::json!({
        "slug": tenant.slug,
        "name": app_name,
        "logo_url": tenant.branding_logo_url,
        "primary_color": primary_color,
        "providers": tenant.allowed_providers,
        "signup_mode": tenant.signup_mode,
        "server_url": format!("/t/{}", tenant.slug),
        "login_redirect_url": tenant.login_redirect_url,
        "hosted_redirect_uri": hosted_redirect_uri,
        "client_id": hosted_client_id,
    });

    let config_script = format!(
        "window.__TENANT__={};",
        json_for_script(&config_json)
    );

    let page_title = format!("{} — Sign In", html_escape(app_name));

    let personality_css = format!(
        "body{{background:#0a0a0c;color:#e8e6e3;--accent:{pc};}}",
        pc = primary_color
    );

    render_page(&PageConfig {
        title: &page_title,
        route_pattern: "/t/:slug/login",
        manifest: &state.asset_manifest,
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some(&personality_css),
        body_prefix: if logo_html.is_empty() { None } else { Some(&logo_html) },
        render_mode: RenderMode::Phase1ClientMount,
        ir_module: None,
        slots: None,
    })
}

/// Simple HTML escaping for untrusted strings injected into HTML attributes/content.
pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Sanitize a JSON string for safe embedding inside <script> tags.
/// Prevents </script> breakout by escaping </ sequences.
pub(crate) fn json_for_script(json: &serde_json::Value) -> String {
    json.to_string().replace("</", "<\\/")
}
