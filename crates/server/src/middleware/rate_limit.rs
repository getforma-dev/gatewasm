use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

type DefaultGovernorLayer = GovernorLayer<
    tower_governor::key_extractor::PeerIpKeyExtractor,
    governor::middleware::NoOpMiddleware<governor::clock::QuantaInstant>,
    axum::body::Body,
>;

/// Rate limit for login endpoints: replenishes 6 tokens/sec, burst of 10 per IP.
pub fn login_limiter() -> DefaultGovernorLayer {
    let config = GovernorConfigBuilder::default()
        .per_second(6)
        .burst_size(10)
        .finish()
        .expect("Failed to build login rate limiter");
    GovernorLayer::new(config)
}

/// Rate limit for register endpoints: replenishes 12 tokens/sec, burst of 5 per IP.
/// Override burst size with REGISTER_RATE_LIMIT_BURST env var (useful for e2e tests).
pub fn register_limiter() -> DefaultGovernorLayer {
    let burst: u32 = std::env::var("REGISTER_RATE_LIMIT_BURST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let config = GovernorConfigBuilder::default()
        .per_second(12)
        .burst_size(burst)
        .finish()
        .expect("Failed to build register rate limiter");
    GovernorLayer::new(config)
}

/// Rate limit for token endpoints: replenishes 2 tokens/sec, burst of 30 per IP.
pub fn token_limiter() -> DefaultGovernorLayer {
    let config = GovernorConfigBuilder::default()
        .per_second(2)
        .burst_size(30)
        .finish()
        .expect("Failed to build token rate limiter");
    GovernorLayer::new(config)
}

/// Rate limit for MFA code verification: replenishes 1 token/sec, burst of 6 per IP.
pub fn mfa_verify_limiter() -> DefaultGovernorLayer {
    let config = GovernorConfigBuilder::default()
        .per_second(1)
        .burst_size(6)
        .finish()
        .expect("Failed to build MFA verify rate limiter");
    GovernorLayer::new(config)
}
