use axum::{
    extract::{Extension, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;
use sqlx::Row;

use crate::handlers::auth::extract_refresh_cookie;
use crate::handlers::hosted_login::html_escape;
use crate::jwt::signing;
use crate::middleware::csp::generate_csp_nonce;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// GET /t/{slug}/site — Auth-gated hosted page for end-users.
pub async fn site_page(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    req: axum::extract::Request,
) -> Response {
    let slug = &tenant.slug;

    // Auth check: valid refresh cookie for this tenant?
    let authenticated = match extract_refresh_cookie(req.headers(), Some(slug)) {
        Some(rt) => {
            let hash = signing::hash_token(&rt);
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM refresh_tokens WHERE token_hash = $1 AND revoked = FALSE AND expires_at > $2 AND tenant_id = $3)",
            )
            .bind(&hash)
            .bind(Utc::now())
            .bind(&tenant.tenant_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(false)
        }
        None => false,
    };

    if !authenticated {
        let login_url = format!("/t/{}/login?redirect=/t/{}/site", slug, slug);
        return Redirect::temporary(&login_url).into_response();
    }

    // Fetch hosted page content
    let page_data = sqlx::query(
        "SELECT headline, body, cta_text, cta_link, custom_html FROM tenant_hosted_pages WHERE tenant_id = $1",
    )
    .bind(&tenant.tenant_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let app_name = tenant.branding_app_name.as_deref().unwrap_or(&tenant.name);
    let primary_color = &tenant.branding_primary_color;

    let (headline, body_text, cta_text, cta_link) = match &page_data {
        Some(r) => (
            r.get::<String, _>("headline"),
            r.get::<String, _>("body"),
            r.get::<String, _>("cta_text"),
            r.get::<String, _>("cta_link"),
        ),
        None => (
            "Welcome".to_string(),
            String::new(),
            String::new(),
            String::new(),
        ),
    };

    let nonce = generate_csp_nonce();

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style nonce="{nonce}">
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:system-ui,-apple-system,sans-serif;background:#0a0a0c;color:#e8e6e3;min-height:100vh;display:flex;align-items:center;justify-content:center}}
.page{{max-width:640px;width:100%;padding:2rem;text-align:center}}
h1{{font-size:2rem;font-weight:500;margin-bottom:1rem;color:{color}}}
.body-text{{font-size:1rem;line-height:1.6;color:#a8a29e;margin-bottom:1.5rem}}
.cta{{display:inline-block;padding:0.65rem 1.5rem;background:{color};color:white;text-decoration:none;font-size:0.9rem;font-weight:500;border-radius:4px}}
.cta:hover{{opacity:0.9}}
.footer{{margin-top:3rem;font-size:0.72rem;color:#57534e}}
.footer a{{color:#78716c;text-decoration:none}}
.footer a:hover{{text-decoration:underline}}
</style>
</head>
<body>
<div class="page">
<h1>{headline}</h1>
{body_html}
{cta_html}
<div class="footer">Powered by <a href="https://getforma.dev">Forma Auth</a></div>
</div>
</body>
</html>"#,
        nonce = nonce,
        title = html_escape(app_name),
        color = html_escape(primary_color),
        headline = html_escape(&headline),
        body_html = if body_text.is_empty() {
            String::new()
        } else {
            format!(
                "<p class=\"body-text\">{}</p>",
                html_escape(&body_text)
            )
        },
        cta_html = if cta_text.is_empty() {
            String::new()
        } else {
            format!(
                "<a class=\"cta\" href=\"{}\">{}</a>",
                html_escape(&cta_link),
                html_escape(&cta_text)
            )
        },
    );

    let csp = format!(
        "default-src 'none'; style-src 'nonce-{}'; img-src https:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        nonce
    );

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (axum::http::header::HeaderName::from_static("content-security-policy"), csp),
        ],
        Html(html),
    )
        .into_response()
}
