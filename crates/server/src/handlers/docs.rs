use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
};
use serde_json::{Map, Value};

use crate::handlers::hosted_login::{html_escape, json_for_script};
use crate::middleware::csp;

const OPENAPI_SPEC: &str = include_str!("../../openapi.json");
const PUBLIC_TAGS: &[&str] = &[
    "Health",
    "Auth",
    "OAuth",
    "User",
    "Management",
    "Roles",
    "Webhooks",
    "Organizations",
    "Admin",
    "Self-Serve",
];
const INTERNAL_TAGS: &[&str] = &["Platform", "Compliance"];
const HTTP_METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// Pinned swagger-ui-dist build.
///
/// The version must stay EXACT. unpkg resolves `@5` to whatever is newest, and
/// subresource integrity against a moving target just breaks the page on every
/// upstream release. Bump the version and BOTH hashes together — a stale hash
/// fails closed, which is the safe direction.
///
/// Recompute with:
///   curl -sL https://unpkg.com/swagger-ui-dist@<v>/<file> \
///     | openssl dgst -sha384 -binary | openssl base64 -A
const SWAGGER_UI_VERSION: &str = "5.18.2";
const SWAGGER_UI_CSS_SRI: &str =
    "sha384-rcbEi6xgdPk0iWkAQzT2F3FeBJXdG+ydrawGlfHAFIZG7wU6aKbQaRewysYpmrlW";
const SWAGGER_UI_JS_SRI: &str =
    "sha384-NXtFPpN61oWCuN4D42K6Zd5Rt2+uxeIT36R7kpXBuY9tLnZorzrJ4ykpqwJfgjpZ";

/// Build the Swagger UI page and the policy that blesses it.
///
/// Returns `(html, csp)` rather than a `Response` so the escaping and the
/// pinning are reachable from a plain `#[test]` with no server.
///
/// This page previously loaded third-party JavaScript from unpkg with no
/// integrity and shipped with no CSP at all, on the same origin as the
/// `__Host-` refresh cookies. Anyone able to serve or tamper with that URL had
/// script execution on the auth origin.
fn swagger_ui_document(title: &str, spec_url: &str) -> (String, String) {
    let nonce = csp::generate_csp_nonce();

    // Deliberately its own policy rather than a relaxation of the standard
    // one. Swagger UI injects styles at runtime, so `style-src` needs
    // `'unsafe-inline'` — and a nonce in `style-src` would *disable*
    // `'unsafe-inline'` under CSP3 and break the page, which is why the
    // `<style>` below carries no nonce. Scripts stay nonce-gated plus the one
    // pinned host; there is no `'unsafe-eval'`, and framing, base URI and form
    // submission stay locked down.
    let csp = format!(
        "default-src 'none'; \
         script-src 'nonce-{nonce}' https://unpkg.com; \
         style-src 'unsafe-inline' https://unpkg.com; \
         img-src 'self' data:; \
         font-src 'self' data:; \
         connect-src 'self'; \
         frame-ancestors 'none'; \
         base-uri 'none'; \
         form-action 'none'",
        nonce = nonce.as_str()
    );

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@{v}/swagger-ui.css"
        integrity="{css_sri}" crossorigin="anonymous" referrerpolicy="no-referrer" />
  <style>
    body {{ margin: 0; background: #fafafa; }}
    .swagger-ui .topbar {{ display: none; }}
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@{v}/swagger-ui-bundle.js"
          integrity="{js_sri}" crossorigin="anonymous" referrerpolicy="no-referrer"></script>
  <script nonce="{nonce}">
    SwaggerUIBundle({{
      url: {spec_url},
      dom_id: '#swagger-ui',
      deepLinking: true,
      presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
      layout: "BaseLayout",
    }});
  </script>
</body>
</html>"#,
        title = html_escape(title),
        v = SWAGGER_UI_VERSION,
        css_sri = SWAGGER_UI_CSS_SRI,
        js_sri = SWAGGER_UI_JS_SRI,
        nonce = nonce.as_str(),
        // JavaScript expression position: emit a JSON string literal rather
        // than hand-quoting, so a quote in the path cannot break out.
        spec_url = json_for_script(&Value::String(spec_url.to_string())),
    );

    (html, csp)
}

fn swagger_ui_response(title: &str, spec_url: &str) -> impl IntoResponse {
    let (html, csp) = swagger_ui_document(title, spec_url);
    (
        [
            (
                axum::http::header::HeaderName::from_static("content-security-policy"),
                csp,
            ),
            (
                axum::http::header::HeaderName::from_static("x-content-type-options"),
                "nosniff".to_string(),
            ),
        ],
        Html(html),
    )
}

fn should_keep_public_operation(op: &Value) -> bool {
    let Some(tags) = op.get("tags").and_then(Value::as_array) else {
        // Default deny: operations without explicit public tags are not exported publicly.
        return false;
    };

    let op_tags: Vec<&str> = tags.iter().filter_map(Value::as_str).collect();
    let has_public = op_tags.iter().any(|t| PUBLIC_TAGS.contains(t));
    let has_internal = op_tags.iter().any(|t| INTERNAL_TAGS.contains(t));
    has_public && !has_internal
}

fn build_public_spec() -> Result<String, serde_json::Error> {
    let mut spec: Value = serde_json::from_str(OPENAPI_SPEC)?;

    if let Some(tags) = spec.get_mut("tags").and_then(Value::as_array_mut) {
        tags.retain(|tag| {
            tag.get("name")
                .and_then(Value::as_str)
                .map(|name| PUBLIC_TAGS.contains(&name))
                .unwrap_or(false)
        });
    }

    let mut public_paths = Map::new();
    let source_paths = spec
        .get("paths")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for (path, path_item) in source_paths {
        let Some(path_obj) = path_item.as_object() else {
            continue;
        };

        let mut filtered_path = Map::new();
        for (key, val) in path_obj {
            if HTTP_METHODS.contains(&key.as_str()) {
                if should_keep_public_operation(val) {
                    filtered_path.insert(key.clone(), val.clone());
                }
            } else {
                // Preserve path-level metadata (e.g., shared parameters) for remaining operations.
                filtered_path.insert(key.clone(), val.clone());
            }
        }

        let has_ops = filtered_path
            .keys()
            .any(|k| HTTP_METHODS.contains(&k.as_str()));
        if has_ops {
            public_paths.insert(path, Value::Object(filtered_path));
        }
    }

    if let Some(paths) = spec.get_mut("paths") {
        *paths = Value::Object(public_paths);
    }

    serde_json::to_string_pretty(&spec)
}

/// GET /docs — Public API reference (customer-facing).
pub async fn docs_page() -> impl IntoResponse {
    swagger_ui_response("fAuth Public API Reference", "/openapi.json")
}

/// GET /openapi.json — Public OpenAPI spec (internal paths removed).
pub async fn openapi_json() -> impl IntoResponse {
    match build_public_spec() {
        Ok(spec) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            spec,
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to build public OpenAPI spec");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"server_error","message":"failed to build public spec"}"#.to_string(),
            )
                .into_response()
        }
    }
}

/// GET /internal/docs — Internal API reference (Forma-only).
pub async fn internal_docs_page() -> impl IntoResponse {
    swagger_ui_response("fAuth Internal API Reference", "/internal/openapi.json")
}

/// GET /internal/openapi.json — Full OpenAPI spec (platform-admin protected).
pub async fn internal_openapi_json() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        OPENAPI_SPEC,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swagger_assets_are_pinned_and_integrity_checked() {
        let (html, _) = swagger_ui_document("t", "/openapi.json");
        // An exact version: unpkg resolves a range to whatever is newest, and
        // SRI against a moving target breaks on every upstream release.
        assert!(html.contains(&format!("swagger-ui-dist@{SWAGGER_UI_VERSION}/")));
        assert!(!html.contains("swagger-ui-dist@5/"));
        // Every third-party subresource carries integrity.
        for (url_marker, sri) in [
            ("swagger-ui.css", SWAGGER_UI_CSS_SRI),
            ("swagger-ui-bundle.js", SWAGGER_UI_JS_SRI),
        ] {
            assert!(html.contains(url_marker));
            assert!(html.contains(sri), "{url_marker} is missing its integrity hash");
        }
        assert!(sri_looks_real(SWAGGER_UI_CSS_SRI));
        assert!(sri_looks_real(SWAGGER_UI_JS_SRI));
    }

    /// Guards against a placeholder hash being committed: sha384 base64 is 64
    /// characters, so a stub like "sha384-REPLACE_ME" fails here rather than
    /// silently disabling integrity at runtime.
    fn sri_looks_real(sri: &str) -> bool {
        sri.strip_prefix("sha384-")
            .is_some_and(|b| b.len() == 64 && b.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')))
    }

    #[test]
    fn swagger_page_ships_a_policy_that_matches_its_markup() {
        let (html, csp) = swagger_ui_document("t", "/openapi.json");

        // The inline bootstrap is nonce-gated, and that same nonce is in the
        // policy. Without this the page is unprotected; with a mismatch it is
        // simply broken.
        let nonce = csp
            .split("'nonce-")
            .nth(1)
            .and_then(|r| r.split('\'').next())
            .expect("script-src carries a nonce");
        assert!(html.contains(&format!("<script nonce=\"{nonce}\">")));

        // The one host the page actually loads from, and nothing looser.
        assert!(csp.contains("script-src 'nonce-"));
        assert!(csp.contains("https://unpkg.com"));
        assert!(!csp.contains("unsafe-eval"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.contains("form-action 'none'"));
        // Swagger injects styles at runtime, so style-src must allow inline —
        // but that must not leak into script-src.
        let script_src = csp
            .split(';')
            .map(str::trim)
            .find(|d| d.starts_with("script-src "))
            .expect("script-src");
        assert!(!script_src.contains("unsafe-inline"), "{script_src}");
    }

    #[test]
    fn spec_url_is_emitted_as_a_json_literal() {
        let (html, _) = swagger_ui_document("t", "/openapi.json");
        assert!(html.contains("url: \"/openapi.json\""));
    }
}
