use sha2::{Digest, Sha256};
use std::net::IpAddr;

/// Compute client fingerprint: SHA-256(User-Agent + "|" + IP/24 subnet).
pub fn compute_fingerprint(user_agent: &str, ip: &str) -> String {
    let subnet = normalize_to_subnet(ip);
    let mut hasher = Sha256::new();
    hasher.update(user_agent.as_bytes());
    hasher.update(b"|");
    hasher.update(subnet.as_bytes());
    hex::encode(hasher.finalize())
}

/// Normalize IPv4 to /24, IPv6 to /48.
fn normalize_to_subnet(ip: &str) -> String {
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(addr)) => {
            let o = addr.octets();
            format!("{}.{}.{}.0/24", o[0], o[1], o[2])
        }
        Ok(IpAddr::V6(addr)) => {
            let s = addr.segments();
            format!("{:x}:{:x}:{:x}::/48", s[0], s[1], s[2])
        }
        Err(_) => "unknown".to_string(),
    }
}

/// Extract client IP from X-Forwarded-For header (first entry) with basic validation.
///
/// Only accepts values that parse as valid IP addresses. Spoofed or garbage
/// X-Forwarded-For values fall through to the fallback.
pub fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    peer_ip: Option<&str>,
) -> String {
    let fallback = peer_ip.unwrap_or("0.0.0.0");

    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim())
        .filter(|s| s.parse::<IpAddr>().is_ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

/// Extract User-Agent from headers.
pub fn extract_user_agent(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_subnet_same_fingerprint() {
        let fp1 = compute_fingerprint("Mozilla/5.0 Chrome", "192.168.1.100");
        let fp2 = compute_fingerprint("Mozilla/5.0 Chrome", "192.168.1.200");
        assert_eq!(fp1, fp2, "IPs in same /24 should produce same fingerprint");
    }

    #[test]
    fn different_subnet_different_fingerprint() {
        let fp1 = compute_fingerprint("Mozilla/5.0 Chrome", "192.168.1.100");
        let fp2 = compute_fingerprint("Mozilla/5.0 Chrome", "10.0.0.50");
        assert_ne!(
            fp1, fp2,
            "IPs in different /24 should produce different fingerprint"
        );
    }

    #[test]
    fn different_ua_different_fingerprint() {
        let fp1 = compute_fingerprint("Mozilla/5.0 Chrome", "192.168.1.100");
        let fp2 = compute_fingerprint("curl/7.88.1", "192.168.1.100");
        assert_ne!(
            fp1, fp2,
            "Different user agents should produce different fingerprint"
        );
    }

    #[test]
    fn ipv6_normalized_to_48() {
        let fp1 = compute_fingerprint("UA", "2001:db8:abcd::1");
        let fp2 = compute_fingerprint("UA", "2001:db8:abcd::ffff");
        assert_eq!(
            fp1, fp2,
            "IPv6 in same /48 should produce same fingerprint"
        );
    }

    #[test]
    fn ipv6_different_48_different_fingerprint() {
        let fp1 = compute_fingerprint("UA", "2001:db8:abcd::1");
        let fp2 = compute_fingerprint("UA", "2001:db8:1234::1");
        assert_ne!(
            fp1, fp2,
            "IPv6 in different /48 should produce different fingerprint"
        );
    }

    #[test]
    fn invalid_ip_returns_deterministic_fingerprint() {
        let fp1 = compute_fingerprint("UA", "not-an-ip");
        let fp2 = compute_fingerprint("UA", "also-not-ip");
        assert_eq!(
            fp1, fp2,
            "Invalid IPs both map to 'unknown' so should match"
        );
    }

    #[test]
    fn fingerprint_is_hex_sha256() {
        let fp = compute_fingerprint("Mozilla/5.0", "10.0.0.1");
        assert_eq!(fp.len(), 64, "SHA-256 hex should be 64 chars");
        assert!(
            fp.chars().all(|c| c.is_ascii_hexdigit()),
            "Should be valid hex"
        );
    }

    #[test]
    fn extract_client_ip_from_xff() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.50, 70.41.3.18".parse().unwrap(),
        );
        assert_eq!(extract_client_ip(&headers, None), "203.0.113.50");
    }

    #[test]
    fn extract_client_ip_missing_xff() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(extract_client_ip(&headers, None), "0.0.0.0");
    }

    #[test]
    fn extract_client_ip_rejects_spoofed_xff() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "not-an-ip, 70.41.3.18".parse().unwrap(),
        );
        assert_eq!(
            extract_client_ip(&headers, Some("10.0.0.1")),
            "10.0.0.1",
            "Invalid IP in XFF should fall back to peer IP"
        );
    }

    #[test]
    fn extract_client_ip_uses_peer_fallback() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(extract_client_ip(&headers, Some("172.16.0.5")), "172.16.0.5");
    }

    #[test]
    fn extract_user_agent_present() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("user-agent", "TestBrowser/1.0".parse().unwrap());
        assert_eq!(extract_user_agent(&headers), "TestBrowser/1.0");
    }

    #[test]
    fn extract_user_agent_missing() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(extract_user_agent(&headers), "unknown");
    }
}
