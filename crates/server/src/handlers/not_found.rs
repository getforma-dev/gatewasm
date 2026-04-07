use axum::{
    extract::{OriginalUri, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{Html, IntoResponse, Response},
};

use crate::frontend::FrontendSnapshot;
use crate::middleware::csp;
use crate::AppState;

fn wants_html(method: &Method, headers: &HeaderMap) -> bool {
    if method != Method::GET && method != Method::HEAD {
        return false;
    }

    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|accept| {
            accept.contains("text/html")
                || accept.contains("application/xhtml+xml")
                || accept.contains("text/*")
        })
        .unwrap_or(false)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn page_response(state: &AppState, requested_path: &str) -> Response {
    let frontend = FrontendSnapshot::capture(state);
    let css = frontend.manifest().resolve("forma-platform.css");
    let nonce = csp::generate_csp_nonce();
    let safe_path = escape_html(requested_path);

    let html = format!(
        "<!DOCTYPE html>\n\
<html lang=\"en\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>404 — Forma Auth</title>\n\
<link rel=\"stylesheet\" href=\"/_assets/{css}\">\n\
<style nonce=\"{nonce}\">\n\
body{{background:#0a0a0c;color:#e8e6e3;--grid-opacity:0.18;--glow-size:980px;--glow-intensity:0.08;}}\n\
.not-found-page{{min-height:100vh;display:flex;align-items:center;justify-content:center;padding:32px 20px;}}\n\
.not-found-shell{{width:min(760px,100%);display:grid;gap:20px;}}\n\
.not-found-card{{padding:28px;display:grid;gap:20px;position:relative;overflow:hidden;}}\n\
.not-found-card::before{{content:'';position:absolute;inset:auto -12% -32% auto;width:280px;height:280px;border-radius:999px;background:radial-gradient(circle, rgba(196,162,101,.12), transparent 68%);pointer-events:none;}}\n\
.not-found-header{{display:grid;gap:10px;max-width:560px;position:relative;z-index:1;}}\n\
.not-found-code{{display:inline-flex;align-items:center;gap:10px;font-size:11px;text-transform:uppercase;letter-spacing:.16em;color:var(--accent);}}\n\
.not-found-code strong{{font-size:12px;color:var(--text);letter-spacing:.22em;}}\n\
.not-found-title{{margin:0;font-family:'Instrument Serif',Georgia,serif;font-size:clamp(40px,6vw,64px);line-height:0.96;letter-spacing:-.04em;}}\n\
.not-found-copy{{margin:0;color:var(--text-dim);max-width:54ch;line-height:1.7;}}\n\
.not-found-path{{display:grid;gap:8px;position:relative;z-index:1;}}\n\
.not-found-path code{{display:block;overflow-wrap:anywhere;padding:12px 14px;border:1px solid var(--border);background:rgba(255,255,255,.02);color:var(--text-muted);font-size:12px;letter-spacing:.04em;}}\n\
.not-found-actions{{display:flex;flex-wrap:wrap;gap:12px;position:relative;z-index:1;}}\n\
.not-found-footer{{display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap;color:var(--text-muted);font-size:12px;line-height:1.7;}}\n\
@media (max-width: 640px){{.not-found-card{{padding:22px;}}.not-found-actions a{{width:100%;justify-content:center;}}.not-found-footer{{flex-direction:column;}}}}\n\
</style>\n\
</head>\n\
<body>\n\
<main class=\"not-found-page\">\n\
  <section class=\"not-found-shell\">\n\
    <div class=\"card not-found-card\">\n\
      <div class=\"not-found-header\">\n\
        <span class=\"not-found-code\"><span class=\"brand-mark brand-mark-gradient\">◇</span><strong>404</strong> Page Not Found</span>\n\
        <h1 class=\"not-found-title\">This route is not part of the current Forma surface.</h1>\n\
        <p class=\"not-found-copy\">The URL may be mistyped, the route may have moved, or the tenant slug may not exist in this environment.</p>\n\
      </div>\n\
      <div class=\"not-found-path\">\n\
        <span class=\"text-xxs text-muted-dim\">Requested path</span>\n\
        <code>{safe_path}</code>\n\
      </div>\n\
      <div class=\"not-found-actions\">\n\
        <a class=\"btn btn-primary\" href=\"/platform/console\">Open Platform Console</a>\n\
        <a class=\"btn btn-ghost\" href=\"/platform/login\">Platform Login</a>\n\
        <a class=\"btn btn-ghost\" href=\"/\">Tenant Login</a>\n\
      </div>\n\
      <div class=\"not-found-footer\">\n\
        <span>If this should exist, check the route registration and current tenant slug.</span>\n\
        <span>fAuth · Forma Auth</span>\n\
      </div>\n\
    </div>\n\
  </section>\n\
</main>\n\
</body>\n\
</html>",
        css = css,
        nonce = nonce,
        safe_path = safe_path,
    );

    (
        StatusCode::NOT_FOUND,
        [
            (header::CACHE_CONTROL, "no-store".to_string()),
            (
                header::HeaderName::from_static("content-security-policy"),
                csp::build_csp_header(&nonce),
            ),
        ],
        Html(html),
    )
        .into_response()
}

pub async fn not_found_page(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if !wants_html(&method, &headers) {
        return StatusCode::NOT_FOUND.into_response();
    }

    page_response(&state, uri.path())
}
