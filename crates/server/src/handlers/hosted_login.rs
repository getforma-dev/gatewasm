use axum::{
    extract::{Extension, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;

use crate::frontend::FrontendSnapshot;
use crate::handlers::auth::extract_refresh_cookie;
use crate::handlers::management::{is_safe_css_color, is_valid_css_color, sanitize_custom_css};
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
    // Extract ?redirect= query param and validate it's a same-tenant path
    let safe_redirect = extract_safe_redirect(req.uri().query(), &tenant.slug);
    let preview_mode = extract_preview_mode(req.uri().query());

    // Server-side redirect: if user has a valid refresh cookie, skip the login page
    if !preview_mode {
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
                let target = safe_redirect.as_deref().unwrap_or(&tenant.login_redirect_url);
                return Redirect::temporary(target).into_response();
            }
        }
    }

    // no-store prevents bfcache — back button makes a fresh server request
    let page = login_page_render(&state, &tenant, preview_mode).await;

    // In preview mode, relax CSP to allow framing from the platform console
    let csp = if preview_mode {
        page.csp
            .replace("frame-ancestors 'none'", &format!("frame-ancestors {}", state.config.server_url))
    } else {
        page.csp
    };

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (
                axum::http::header::HeaderName::from_static("content-security-policy"),
                csp,
            ),
        ],
        Html(page.html),
    )
        .into_response()
}

async fn login_page_render(
    state: &AppState,
    tenant: &TenantContext,
    preview_mode: bool,
) -> crate::html_template::PageOutput {
    let frontend = FrontendSnapshot::capture(state);
    let app_name = tenant.branding_app_name.as_deref().unwrap_or(&tenant.name);
    // Hex-validated on both write paths, but it lands in the same inline
    // <style> as the other three branding values, so it gets the same
    // render-side fallback rather than being trusted because of where it came
    // from. `--accent` is also read back out into the JSON config below.
    let primary_color: &str = if is_valid_css_color(&tenant.branding_primary_color) {
        &tenant.branding_primary_color
    } else {
        "#6366f1"
    };
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

    // Build preview script if ?preview=1 — adds PostMessage listener for live branding
    let preview_js = if preview_mode {
        let origin = &state.config.server_url;
        format!(
            r#";(function(){{
var origin='{}';
document.addEventListener('submit',function(e){{e.preventDefault();}},true);
window.addEventListener('message',function(e){{
if(e.origin!==origin)return;
var d=e.data;if(!d||typeof d!=='object')return;
if(d.type==='forma:branding'){{
if(d.primaryColor)document.documentElement.style.setProperty('--accent',d.primaryColor);
var el=document.querySelector('[data-branding="app-name"]');
if(el&&d.appName)el.textContent=d.appName;
var btn=document.querySelector('[data-branding="submit-btn"]');
if(btn&&d.buttonText)btn.textContent=d.buttonText;
var logo=document.querySelector('[data-branding="logo"]');
if(logo&&d.logoUrl!==undefined)logo.src=d.logoUrl||'';
if(d.darkMode!==undefined)document.body.setAttribute('data-theme',d.darkMode?'dark':'light');
}}
if(d.type==='forma:providers'){{
var btns=document.querySelectorAll('[data-provider]');
btns.forEach(function(b){{
var p=b.getAttribute('data-provider');
var on=d.providers&&d.providers.indexOf(p)!==-1;
b.style.display=on?'':'none';
if(d.noCredentials&&d.noCredentials.indexOf(p)!==-1){{b.style.opacity='0.4';b.style.pointerEvents='none';}}
else{{b.style.opacity='';b.style.pointerEvents='';}}
}});
}}
}});
}})();"#,
            origin
        )
    } else {
        String::new()
    };

    let config_script = format!(
        "window.__TENANT__={};{}",
        json_for_script(&config_json),
        preview_js
    );

    let page_title = format!("{} — Sign In", html_escape(app_name));

    // These values are interpolated into an inline <style> on a page served to
    // unauthenticated visitors. They are validated on write, but rows predate
    // that validator and a new write path could always be added, so fall back
    // rather than trust the column. A fallback is logged: it changes what a
    // tenant's users see, and nobody would otherwise know which tenant to tell.
    let bg_color = match tenant.branding_bg_color.as_deref() {
        Some(c) if !is_safe_css_color(c) => {
            tracing::warn!(
                tenant = %tenant.slug,
                field = "branding_bg_color",
                "stored branding value is not a safe CSS colour; rendering the default"
            );
            "#0a0a0c"
        }
        Some(c) => c,
        None => "#0a0a0c",
    };
    let mut personality_css = format!(
        "body{{background:{bg};color:#e8e6e3;--accent:{pc};}}",
        bg = bg_color,
        pc = primary_color,
    );
    match tenant.branding_card_color.as_deref() {
        Some(c) if is_safe_css_color(c) => {
            personality_css.push_str(&format!(".tl-card{{background:{c};}}"));
        }
        Some(_) => tracing::warn!(
            tenant = %tenant.slug,
            field = "branding_card_color",
            "stored branding value is not a safe CSS colour; omitting it"
        ),
        None => {}
    }
    if let Some(custom_css) = tenant.branding_custom_css.as_deref() {
        // Re-sanitize on read. `sanitize_custom_css` is idempotent: its output
        // is a subset of the grammar it accepts.
        let safe = sanitize_custom_css(custom_css);
        if safe.trim() != custom_css.trim() {
            tracing::warn!(
                tenant = %tenant.slug,
                field = "branding_custom_css",
                "stored custom CSS lost declarations on sanitizing; the tenant's \
                 login page differs from what they saved"
            );
        }
        if !safe.is_empty() {
            personality_css.push_str(&format!(".tl-card{{{safe}}}"));
        }
    }

    render_page(&PageConfig {
        title: &page_title,
        route_pattern: "/t/:slug/login",
        manifest: frontend.manifest(),
        config_script: Some(&config_script),
        config_json: None,
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

/// Serialize a JSON value for safe embedding inside a `<script>` element.
///
/// Script data has two exits, not one: `</script` and `<!--`. The latter puts
/// the tokenizer into script-data-escaped state, where the page's real
/// `</script>` stops closing the element and the rest of the document is
/// swallowed. Escaping only `</` — as this did — misses it.
///
/// Both begin with `<`, and `serde_json` emits a bare `<` only inside a string
/// literal, where `<` is byte-identical. So escaping every `<` is both
/// complete and value-preserving: the output parses back to the same value.
/// U+2028/U+2029 are escaped too — legal in JSON strings, but illegal raw
/// line terminators in JavaScript before ES2019.
pub(crate) fn json_for_script(json: &serde_json::Value) -> String {
    json.to_string()
        .replace('<', "\\u003C")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Extract a safe `?redirect=` param from the query string.
///
/// Only allows paths that start with `/t/{slug}/` (same-tenant relative paths).
/// Returns `None` for empty, absolute-URL, or cross-tenant values.
fn extract_safe_redirect(query: Option<&str>, slug: &str) -> Option<String> {
    let q = query?;
    let prefix = format!("/t/{}/", slug);
    for pair in q.split('&') {
        if let Some(value) = pair.strip_prefix("redirect=") {
            let decoded = urlencoding::decode(value).ok()?;
            if decoded.starts_with(&prefix) {
                return Some(decoded.into_owned());
            }
        }
    }
    None
}

fn extract_preview_mode(query: Option<&str>) -> bool {
    let q = match query {
        Some(q) => q,
        None => return false,
    };

    q.split('&').any(|pair| {
        pair == "preview=1" || pair == "preview=true"
    })
}
