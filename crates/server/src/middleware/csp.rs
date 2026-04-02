// Re-export CSP helpers from the forma-server framework crate.
// All call sites (`crate::middleware::csp::generate_csp_nonce()`, etc.) continue working.
pub use forma_server::csp::{build_csp_header, generate_csp_nonce};

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    #[test]
    fn nonce_is_base64_and_unique() {
        let a = generate_csp_nonce();
        let b = generate_csp_nonce();
        assert_ne!(a, b, "Nonces must be unique");
        // 32 bytes in base64 = 44 chars (with padding)
        assert_eq!(a.len(), 44);
        // Should be valid base64
        assert!(STANDARD.decode(&a).is_ok());
    }

    #[test]
    fn csp_header_contains_nonce() {
        let nonce = generate_csp_nonce();
        let header = build_csp_header(&nonce);
        assert!(header.contains(&format!("'nonce-{}'", nonce)));
    }

    #[test]
    fn csp_header_is_strict() {
        let header = build_csp_header("test-nonce");
        assert!(header.contains("default-src 'none'"));
        assert!(header.contains("script-src 'nonce-test-nonce' 'self'"));
        // style-src: nonce + self (FormaJS v1.0.9+ uses CSSOM, no unsafe-inline needed)
        assert!(header.contains("style-src 'nonce-test-nonce' 'self'"));
        assert!(!header.contains("unsafe-inline"), "CSP must NOT allow unsafe-inline");
        assert!(header.contains("frame-ancestors 'none'"));
        assert!(header.contains("base-uri 'none'"));
        assert!(header.contains("form-action 'self'"));
        assert!(!header.contains("unsafe-eval"));
    }
}
