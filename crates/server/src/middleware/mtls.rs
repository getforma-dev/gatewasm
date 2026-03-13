/// mTLS certificate fingerprint extraction from API Gateway headers.
///
/// In production, mTLS termination happens at the API Gateway (e.g., AWS API Gateway,
/// Cloudflare, or an Nginx reverse proxy). The gateway sets an `X-Client-Cert` header
/// containing the client certificate (URL-encoded PEM) or pre-computed fingerprint.
///
/// This module extracts and normalizes the fingerprint for downstream validation.

/// Extract the client certificate fingerprint from the `X-Client-Cert` header.
///
/// The header value can be:
/// 1. A pre-computed fingerprint in `sha256:<hex>` format (preferred)
/// 2. A URL-encoded PEM certificate (future: compute SHA-256 of DER-encoded cert)
///
/// Returns `None` if the header is missing, empty, or unrecognizable.
pub fn extract_client_cert_fingerprint(headers: &axum::http::HeaderMap) -> Option<String> {
    let cert_header = headers.get("x-client-cert")?.to_str().ok()?;

    if cert_header.is_empty() {
        return None;
    }

    // If the header already contains a sha256: fingerprint, use it directly
    if cert_header.starts_with("sha256:") {
        let hex_part = &cert_header[7..];
        if !hex_part.is_empty() && hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(cert_header.to_string());
        }
    }

    // Future: URL-decode the PEM, parse the certificate, compute SHA-256 of DER bytes.
    // For now, if API Gateway sends a raw fingerprint without the sha256: prefix,
    // we don't accept it — the gateway must be configured to send sha256:<hex>.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn extracts_sha256_fingerprint() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-client-cert",
            "sha256:a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
                .parse()
                .unwrap(),
        );
        let fp = extract_client_cert_fingerprint(&headers);
        assert_eq!(
            fp,
            Some(
                "sha256:a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
                    .to_string()
            )
        );
    }

    #[test]
    fn returns_none_for_missing_header() {
        let headers = HeaderMap::new();
        assert_eq!(extract_client_cert_fingerprint(&headers), None);
    }

    #[test]
    fn returns_none_for_empty_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-client-cert", "".parse().unwrap());
        assert_eq!(extract_client_cert_fingerprint(&headers), None);
    }

    #[test]
    fn returns_none_for_non_sha256_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-client-cert", "md5:abc123".parse().unwrap());
        assert_eq!(extract_client_cert_fingerprint(&headers), None);
    }

    #[test]
    fn returns_none_for_invalid_hex() {
        let mut headers = HeaderMap::new();
        headers.insert("x-client-cert", "sha256:not-valid-hex!".parse().unwrap());
        assert_eq!(extract_client_cert_fingerprint(&headers), None);
    }

    #[test]
    fn returns_none_for_sha256_prefix_only() {
        let mut headers = HeaderMap::new();
        headers.insert("x-client-cert", "sha256:".parse().unwrap());
        assert_eq!(extract_client_cert_fingerprint(&headers), None);
    }
}
