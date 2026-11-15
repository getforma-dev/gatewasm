//! Per-client rate limiting for the public auth routes.
//!
//! Every limiter keys on the client address resolved by [`ClientIpKeyExtractor`]:
//! the address in `X-Forwarded-For` when, and only when, the TCP peer is a
//! trusted reverse proxy; otherwise the TCP peer itself. Behind the Caddy
//! sidecar on the beta host every connection has Caddy's address as its peer,
//! so keying on the peer alone put the whole service in one bucket per limiter.
//!
//! The trusted proxies come from `TRUSTED_PROXY_CIDRS` (comma-separated IP
//! networks). Unset, the default admits loopback and Docker's default local
//! address pool, which is where the Caddy container lives. An empty value
//! trusts nobody, so every request keys on its peer address.
//!
//! Caddy 2.8.4 with no `trusted_proxies` configured discards any
//! `X-Forwarded-For` the client sent and sets the header to exactly the
//! client's remote IP (`modules/caddyhttp/reverseproxy/reverseproxy.go`,
//! `addForwardedHeaders`: `clientXFF := clientIP`, prior values are joined in
//! only `if trusted`, then `req.Header.Set("X-Forwarded-For", clientXFF)`).
//! This extractor reads the rightmost value, which is always the address
//! appended by the nearest trusted hop, so a client can never choose its own
//! key even if a future Caddy configuration starts preserving upstream values.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request};
use ipnet::IpNet;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::KeyExtractor;
use tower_governor::{GovernorError, GovernorLayer};

/// Environment variable naming the IP networks whose `X-Forwarded-For` is trusted.
pub const TRUSTED_PROXY_CIDRS_ENV: &str = "TRUSTED_PROXY_CIDRS";

/// Loopback plus Docker's default local address pool (`172.17.0.0/16` through
/// `172.31.0.0/16`, all inside `172.16.0.0/12`). The beta host's `fauth`
/// network is allocated from that pool and is the only path to the container.
pub const DEFAULT_TRUSTED_PROXY_CIDRS: &str = "127.0.0.0/8,::1/128,172.16.0.0/12";

const X_FORWARDED_FOR: &str = "x-forwarded-for";

type ClientGovernorLayer = GovernorLayer<
    ClientIpKeyExtractor,
    governor::middleware::NoOpMiddleware<governor::clock::QuantaInstant>,
    axum::body::Body,
>;

/// Keys requests on the real client address when the peer is a trusted proxy.
#[derive(Clone, Debug)]
pub struct ClientIpKeyExtractor {
    trusted_proxies: Arc<[IpNet]>,
}

impl ClientIpKeyExtractor {
    /// Build an extractor that trusts `X-Forwarded-For` from the given networks only.
    pub fn new(trusted_proxies: Vec<IpNet>) -> Self {
        Self {
            trusted_proxies: trusted_proxies.into(),
        }
    }

    /// Read `TRUSTED_PROXY_CIDRS`, falling back to [`DEFAULT_TRUSTED_PROXY_CIDRS`]
    /// when unset. A malformed entry aborts startup: a wrong trust boundary must
    /// not degrade silently into either a shared bucket or a spoofable one.
    pub fn from_env() -> Self {
        let raw = std::env::var(TRUSTED_PROXY_CIDRS_ENV)
            .unwrap_or_else(|_| DEFAULT_TRUSTED_PROXY_CIDRS.to_string());
        let trusted = parse_trusted_proxies(&raw).unwrap_or_else(|entry| {
            panic!(
                "{TRUSTED_PROXY_CIDRS_ENV} entry {entry:?} is not an IP network (expected e.g. 172.18.0.0/16)"
            )
        });
        tracing::info!(
            trusted_proxies = ?trusted,
            "Rate limiters key on the X-Forwarded-For client address when the peer is a trusted proxy"
        );
        Self::new(trusted)
    }

    fn is_trusted_proxy(&self, peer: IpAddr) -> bool {
        self.trusted_proxies.iter().any(|net| net.contains(&peer))
    }

    /// The limiter key for a request: the forwarded client address when the
    /// peer is a trusted proxy and the header carries a usable address,
    /// otherwise the peer address. `None` without `ConnectInfo`.
    pub fn client_ip<T>(&self, req: &Request<T>) -> Option<IpAddr> {
        let peer = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip().to_canonical())?;
        if !self.is_trusted_proxy(peer) {
            return Some(peer);
        }
        Some(forwarded_client_ip(req.headers()).unwrap_or(peer))
    }
}

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        self.client_ip(req).ok_or(GovernorError::UnableToExtractKey)
    }
}

/// Parse a comma-separated list of IP networks. Bare addresses are accepted as
/// single-host networks. An empty list is valid and trusts no proxy. On failure
/// returns the offending entry.
pub fn parse_trusted_proxies(raw: &str) -> Result<Vec<IpNet>, String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .parse::<IpNet>()
                .or_else(|_| entry.parse::<IpAddr>().map(IpNet::from))
                .map_err(|_| entry.to_string())
        })
        .collect()
}

/// The rightmost address in the last `X-Forwarded-For` header, which is the
/// value the nearest trusted proxy appended. `None` when the header is absent
/// or its rightmost entry is not an IP address (with or without a port).
fn forwarded_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let last_header = headers.get_all(X_FORWARDED_FOR).iter().next_back()?;
    let value = last_header.to_str().ok()?;
    let rightmost = value.rsplit(',').next()?.trim();
    parse_forwarded_address(rightmost)
}

fn parse_forwarded_address(entry: &str) -> Option<IpAddr> {
    if entry.is_empty() {
        return None;
    }
    let bracketed = entry
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'));
    let ip = entry
        .parse::<IpAddr>()
        .or_else(|_| entry.parse::<SocketAddr>().map(|addr| addr.ip()))
        .ok()
        .or_else(|| bracketed.and_then(|inner| inner.parse::<IpAddr>().ok()))?;
    Some(ip.to_canonical())
}

/// Build a governor layer keyed by client address. `per_second` is governor's
/// replenish interval in seconds (one token every `per_second` seconds).
fn limiter(extractor: &ClientIpKeyExtractor, per_second: u64, burst: u32) -> ClientGovernorLayer {
    let config = GovernorConfigBuilder::default()
        .key_extractor(extractor.clone())
        .per_second(per_second)
        .burst_size(burst)
        .finish()
        .expect("Failed to build rate limiter");
    GovernorLayer::new(config)
}

fn burst_from_env(var: &str, default: u32) -> u32 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Rate limit for login endpoints: one token every 6 s, burst of 10 per client.
/// Override burst size with LOGIN_RATE_LIMIT_BURST env var (useful for e2e tests).
pub fn login_limiter(extractor: &ClientIpKeyExtractor) -> ClientGovernorLayer {
    limiter(extractor, 6, burst_from_env("LOGIN_RATE_LIMIT_BURST", 10))
}

/// Rate limit for register endpoints: one token every 12 s, burst of 5 per client.
/// Override burst size with REGISTER_RATE_LIMIT_BURST env var (useful for e2e tests).
pub fn register_limiter(extractor: &ClientIpKeyExtractor) -> ClientGovernorLayer {
    limiter(
        extractor,
        12,
        burst_from_env("REGISTER_RATE_LIMIT_BURST", 5),
    )
}

/// Rate limit for token endpoints: one token every 2 s, burst of 30 per client.
/// Override burst size with TOKEN_RATE_LIMIT_BURST env var (useful for e2e tests).
pub fn token_limiter(extractor: &ClientIpKeyExtractor) -> ClientGovernorLayer {
    limiter(extractor, 2, burst_from_env("TOKEN_RATE_LIMIT_BURST", 30))
}

/// Rate limit for MFA code verification: one token every 1 s, burst of 6 per client.
pub fn mfa_verify_limiter(extractor: &ClientIpKeyExtractor) -> ClientGovernorLayer {
    limiter(extractor, 1, 6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    const PROXY: &str = "172.18.0.3:51234";
    const STRANGER: &str = "198.51.100.7:40000";
    const CLIENT_A: &str = "203.0.113.10";
    const CLIENT_B: &str = "203.0.113.11";

    fn extractor() -> ClientIpKeyExtractor {
        ClientIpKeyExtractor::new(parse_trusted_proxies(DEFAULT_TRUSTED_PROXY_CIDRS).unwrap())
    }

    fn request(peer: &str, forwarded_for: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().uri("/");
        if let Some(value) = forwarded_for {
            builder = builder.header(X_FORWARDED_FOR, value);
        }
        let mut req = builder.body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        req
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn key(peer: &str, forwarded_for: Option<&str>) -> IpAddr {
        extractor().extract(&request(peer, forwarded_for)).unwrap()
    }

    #[test]
    fn trusted_peer_with_forwarded_header_uses_client_key() {
        assert_eq!(key(PROXY, Some(CLIENT_A)), ip(CLIENT_A));
        assert_eq!(key("127.0.0.1:9", Some(CLIENT_A)), ip(CLIENT_A));
        assert_eq!(key("[::1]:9", Some(CLIENT_A)), ip(CLIENT_A));
    }

    #[test]
    fn trusted_peer_uses_rightmost_forwarded_value() {
        // A client-supplied prefix preserved by an upstream proxy never wins.
        assert_eq!(
            key(PROXY, Some("10.0.0.1, 192.0.2.99 , 203.0.113.10")),
            ip(CLIENT_A)
        );
        // Several X-Forwarded-For headers: the last header's last value.
        let mut req = request(PROXY, Some("192.0.2.99"));
        req.headers_mut()
            .append(X_FORWARDED_FOR, CLIENT_B.parse().unwrap());
        assert_eq!(extractor().extract(&req).unwrap(), ip(CLIENT_B));
    }

    #[test]
    fn trusted_peer_accepts_port_and_bracket_forms() {
        assert_eq!(key(PROXY, Some("203.0.113.10:5555")), ip(CLIENT_A));
        assert_eq!(key(PROXY, Some("2001:db8::1")), ip("2001:db8::1"));
        assert_eq!(key(PROXY, Some("[2001:db8::1]")), ip("2001:db8::1"));
        assert_eq!(key(PROXY, Some("[2001:db8::1]:443")), ip("2001:db8::1"));
        // IPv4-mapped IPv6 collapses onto the IPv4 key.
        assert_eq!(key(PROXY, Some("::ffff:203.0.113.10")), ip(CLIENT_A));
    }

    #[test]
    fn untrusted_peer_with_spoofed_header_uses_peer_key() {
        let peer = ip("198.51.100.7");
        assert_eq!(key(STRANGER, Some(CLIENT_A)), peer);
        assert_eq!(key(STRANGER, Some("127.0.0.1")), peer);
        assert_eq!(key(STRANGER, None), peer);
    }

    #[test]
    fn missing_or_garbage_header_uses_peer_key() {
        let proxy = ip("172.18.0.3");
        assert_eq!(key(PROXY, None), proxy);
        assert_eq!(key(PROXY, Some("")), proxy);
        assert_eq!(key(PROXY, Some("not-an-ip")), proxy);
        assert_eq!(key(PROXY, Some("203.0.113.10, unknown")), proxy);
        assert_eq!(key(PROXY, Some("203.0.113.10,")), proxy);
        assert_eq!(key(PROXY, Some("999.1.1.1")), proxy);
        assert_eq!(key(PROXY, Some("[203.0.113.10")), proxy);
    }

    #[test]
    fn missing_connect_info_cannot_extract_a_key() {
        let req = Request::builder()
            .uri("/")
            .header(X_FORWARDED_FOR, CLIENT_A)
            .body(Body::empty())
            .unwrap();
        assert!(matches!(
            extractor().extract(&req),
            Err(GovernorError::UnableToExtractKey)
        ));
    }

    #[test]
    fn empty_trust_list_always_uses_peer_key() {
        let nobody = ClientIpKeyExtractor::new(parse_trusted_proxies("").unwrap());
        assert_eq!(
            nobody.extract(&request(PROXY, Some(CLIENT_A))).unwrap(),
            ip("172.18.0.3")
        );
        assert_eq!(
            nobody
                .extract(&request("127.0.0.1:9", Some(CLIENT_A)))
                .unwrap(),
            ip("127.0.0.1")
        );
    }

    #[test]
    fn trust_list_parsing() {
        let nets = parse_trusted_proxies(" 172.18.0.0/16, 10.1.2.3 ,::1/128,, ").unwrap();
        assert_eq!(nets.len(), 3);
        assert!(nets[0].contains(&ip("172.18.255.1")));
        assert!(!nets[0].contains(&ip("172.19.0.1")));
        assert!(nets[1].contains(&ip("10.1.2.3")));
        assert!(!nets[1].contains(&ip("10.1.2.4")));
        assert!(nets[2].contains(&ip("::1")));

        assert_eq!(parse_trusted_proxies("").unwrap(), Vec::<IpNet>::new());
        assert_eq!(
            parse_trusted_proxies("172.18.0.0/16,caddy").unwrap_err(),
            "caddy"
        );
        assert_eq!(
            parse_trusted_proxies("10.0.0.0/33").unwrap_err(),
            "10.0.0.0/33"
        );
        // The shipped default must parse.
        parse_trusted_proxies(DEFAULT_TRUSTED_PROXY_CIDRS).unwrap();
    }

    #[test]
    fn default_trust_covers_docker_pool_but_not_the_internet() {
        let e = extractor();
        for trusted in [
            "127.0.0.1",
            "127.8.8.8",
            "::1",
            "172.17.0.1",
            "172.18.0.2",
            "172.31.255.254",
        ] {
            assert!(
                e.is_trusted_proxy(ip(trusted)),
                "{trusted} should be trusted"
            );
        }
        for untrusted in [
            "172.32.0.1",
            "172.15.255.255",
            "10.0.0.5",
            "192.168.1.1",
            "203.0.113.10",
            "2001:db8::1",
        ] {
            assert!(
                !e.is_trusted_proxy(ip(untrusted)),
                "{untrusted} should not be trusted"
            );
        }
    }

    #[tokio::test]
    async fn forwarded_clients_get_separate_buckets() {
        let burst = 3;
        let app = Router::new()
            .route("/", get(|| async { StatusCode::OK }))
            .layer(limiter(&extractor(), 60, burst));

        async fn hit(app: &Router, peer: &str, forwarded_for: Option<&str>) -> StatusCode {
            app.clone()
                .oneshot(request(peer, forwarded_for))
                .await
                .unwrap()
                .status()
        }

        // Client A exhausts its burst through the proxy.
        for _ in 0..burst {
            assert_eq!(hit(&app, PROXY, Some(CLIENT_A)).await, StatusCode::OK);
        }
        assert_eq!(
            hit(&app, PROXY, Some(CLIENT_A)).await,
            StatusCode::TOO_MANY_REQUESTS
        );

        // Client B, through the same proxy, still passes.
        assert_eq!(hit(&app, PROXY, Some(CLIENT_B)).await, StatusCode::OK);

        // A direct peer claiming to be client B is keyed on itself, not on B.
        for _ in 0..burst {
            assert_eq!(hit(&app, STRANGER, Some(CLIENT_B)).await, StatusCode::OK);
        }
        assert_eq!(
            hit(&app, STRANGER, Some(CLIENT_B)).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        // B's own bucket was not drained by the stranger.
        assert_eq!(hit(&app, PROXY, Some(CLIENT_B)).await, StatusCode::OK);
    }
}
