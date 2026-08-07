// Re-export CSP helpers from the forma-server framework crate.
// All call sites (`crate::middleware::csp::generate_csp_nonce()`, etc.) continue working.
pub use forma_server::csp::{build_csp_header, generate_csp_nonce, CspNonce};

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
