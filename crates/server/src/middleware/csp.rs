// Re-export CSP helpers from the forma-server framework crate.
// All call sites (`crate::middleware::csp::generate_csp_nonce()`, etc.) continue working.
pub use forma_server::csp::{build_csp_header, generate_csp_nonce, CspNonce};

use axum::{
    extract::Request,
    http::{header, HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};

/// Fallback CSP for HTML responses whose handler did not set one, plus two
/// cheap global headers.
///
/// CSP attachment used to be entirely per-handler opt-in — `main.rs` applied
/// only CORS, tracing and request-id — which is how `/docs`, `/internal/docs`
/// and the verify-email page each shipped with no policy at all. This makes
/// "has a CSP" a property of the server instead of something every new handler
/// has to remember.
///
/// It is a floor, not a replacement. Handlers that mint a nonce still set their
/// own stricter, nonce-bearing header and this leaves it untouched. The
/// fallback cannot carry a nonce — it runs after the body is built — so it is
/// deliberately script-free: a page that forgets its own policy breaks
/// visibly in development rather than silently serving unprotected.
pub async fn default_csp(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;

    let is_html = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));

    if is_html && !res.headers().contains_key("content-security-policy") {
        res.headers_mut().insert(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(
                "default-src 'none'; img-src 'self' data:; style-src 'self'; \
                 frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
            ),
        );
    }

    // Only /_assets sets nosniff today. `entry().or_insert` so a handler with
    // a deliberate value keeps it.
    res.headers_mut()
        .entry(HeaderName::from_static("x-content-type-options"))
        .or_insert(HeaderValue::from_static("nosniff"));
    res.headers_mut()
        .entry(HeaderName::from_static("referrer-policy"))
        .or_insert(HeaderValue::from_static("strict-origin-when-cross-origin"));

    res
}


#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    /// Extract one directive's value from a policy string.
    ///
    /// Assertions here are deliberately directive-scoped. A bare
    /// `!header.contains("unsafe-inline")` was how this file used to check
    /// strictness, and it would now fail against a policy that is strictly
    /// *stronger*: forma-server 0.2.0 adds `style-src-attr 'unsafe-inline'`,
    /// which permits `style=""` attributes in markup while leaving `style-src`
    /// nonce-only and touching scripts not at all. A substring match cannot
    /// tell a scoped relaxation from a blanket one.
    fn directive<'a>(csp: &'a str, name: &str) -> Option<&'a str> {
        csp.split(';')
            .map(str::trim)
            .find_map(|d| d.strip_prefix(name).filter(|rest| rest.starts_with(' ')))
            .map(str::trim)
    }

    #[test]
    fn nonce_is_base64_and_unique() {
        let a = generate_csp_nonce();
        let b = generate_csp_nonce();
        assert_ne!(a.as_str(), b.as_str(), "Nonces must be unique");
        // 32 bytes in base64 = 44 chars (with padding)
        assert_eq!(a.as_str().len(), 44);
        assert!(STANDARD.decode(a.as_str()).is_ok());
    }

    #[test]
    fn csp_header_contains_nonce() {
        let nonce = generate_csp_nonce();
        let header = build_csp_header(&nonce);
        assert!(header.contains(&format!("'nonce-{}'", nonce.as_str())));
    }

    #[test]
    fn csp_header_is_strict() {
        let nonce = generate_csp_nonce();
        let header = build_csp_header(&nonce);
        let n = nonce.as_str();

        assert_eq!(directive(&header, "default-src"), Some("'none'"));

        // Scripts: nonce-gated, never inline, never eval.
        let script_src = directive(&header, "script-src").expect("script-src");
        assert!(script_src.contains(&format!("'nonce-{n}'")));
        assert!(
            !script_src.contains("unsafe-inline"),
            "script-src must not allow inline: {script_src}"
        );
        assert!(!script_src.contains("unsafe-eval"));

        // <style> elements: nonce-gated. `style-src-attr` is a separate
        // directive and is allowed to be more permissive.
        let style_src = directive(&header, "style-src").expect("style-src");
        assert!(style_src.contains(&format!("'nonce-{n}'")));
        assert!(
            !style_src.contains("unsafe-inline"),
            "style-src must not allow inline: {style_src}"
        );

        assert_eq!(directive(&header, "frame-ancestors"), Some("'none'"));
        assert_eq!(directive(&header, "base-uri"), Some("'none'"));
        assert_eq!(directive(&header, "form-action"), Some("'self'"));
        assert!(!header.contains("unsafe-eval"));
    }
}
