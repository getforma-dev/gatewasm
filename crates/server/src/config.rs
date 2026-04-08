use std::env;

use crate::compliance::ComplianceConfig;

#[derive(Clone)]
pub struct AppConfig {
    pub port: u16,
    pub host: String,
    pub server_url: String,
    pub database_url: String,
    pub ed25519_private_key_pem: Option<String>,
    pub jwt_issuer: String,
    pub jwt_audience: String,
    pub access_token_expiry: u64,
    pub refresh_token_expiry: u64,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub allowed_origins: Vec<String>,
    pub csrf_secret: String,
    /// Primary key material for MFA secret encryption-at-rest.
    pub mfa_encryption_key: String,
    /// Optional previous key material(s) to support key rotation for existing encrypted MFA secrets.
    pub mfa_encryption_key_previous: Vec<String>,
    /// Platform admin API key for tenant provisioning (e.g., `plat_xxxxx`)
    pub platform_admin_key: Option<String>,
    /// Comma-separated provider-owner email allowlist to auto-assign platform_owner role.
    pub platform_owner_emails: Vec<String>,
    /// Base domain for tenant subdomains (e.g., `gatewasm.io`)
    pub platform_domain: Option<String>,
    /// Resend API key for transactional email delivery (preferred over SMTP).
    pub resend_api_key: Option<String>,
    /// Sender email address for outbound auth emails (used by Resend and as SMTP fallback).
    pub email_from_address: String,
    /// Sender display name for outbound auth emails.
    pub email_from_name: String,
    /// SMTP host for outbound transactional emails.
    pub smtp_host: Option<String>,
    /// SMTP port (typically 587 for STARTTLS).
    pub smtp_port: u16,
    /// SMTP username for authenticated relay.
    pub smtp_username: Option<String>,
    /// SMTP password for authenticated relay.
    pub smtp_password: Option<String>,
    /// Sender email address used in outbound auth emails.
    pub smtp_from_email: Option<String>,
    /// Sender display name used in outbound auth emails.
    pub smtp_from_name: String,
    /// Whether to use STARTTLS for SMTP delivery.
    pub smtp_starttls: bool,
    /// Compliance engine configuration
    pub compliance: ComplianceConfig,
    /// Separate Ed25519 PEM key for signing Security Event Tokens (SSF/SET).
    pub ssf_signing_key_pem: Option<String>,
    /// Path to a MaxMind GeoIP database (`.mmdb`) for impossible-travel detection.
    pub maxmind_db_path: Option<String>,
    /// WebSocket ticket time-to-live in seconds (default 30).
    pub ws_ticket_ttl_secs: u64,
}

impl std::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("server_url", &self.server_url)
            .field("jwt_issuer", &self.jwt_issuer)
            .field("jwt_audience", &self.jwt_audience)
            .finish_non_exhaustive()
    }
}

impl AppConfig {
    /// Returns true when `server_url` points to a non-local address.
    /// Used to gate production-only security checks (MFA enforcement, SSO requirement, etc.).
    pub fn is_production(&self) -> bool {
        !self.server_url.is_empty()
            && !self.server_url.contains("localhost")
            && !self.server_url.contains("127.0.0.1")
    }

    /// Build DATABASE_URL from individual components (DB_HOST, DB_PORT, etc.)
    /// or fall back to the DATABASE_URL env var. This avoids shell quoting issues
    /// when App Runner injects secrets containing special characters.
    fn resolve_database_url() -> String {
        if let Ok(url) = env::var("DATABASE_URL") {
            return url;
        }
        if let (Ok(host), Ok(port), Ok(user), Ok(pass), Ok(name)) = (
            env::var("DB_HOST"),
            env::var("DB_PORT"),
            env::var("DB_USERNAME"),
            env::var("DB_PASSWORD"),
            env::var("DB_NAME"),
        ) {
            let pass = urlencoding::encode(&pass);
            let user = urlencoding::encode(&user);
            format!("postgres://{user}:{pass}@{host}:{port}/{name}?sslmode=require")
        } else {
            "postgres://gatewasm:gatewasm@localhost:5432/gatewasm".to_string()
        }
    }

    pub fn from_env() -> Self {
        let csrf_secret = env::var("CSRF_SECRET").unwrap_or_else(|_| {
            use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
            let bytes: [u8; 32] = rand::random();
            URL_SAFE_NO_PAD.encode(bytes)
        });
        let server_url_val = env::var("SERVER_URL").unwrap_or_default();
        let is_production = !server_url_val.is_empty()
            && !server_url_val.contains("localhost")
            && !server_url_val.contains("127.0.0.1");

        let mfa_encryption_key = env::var("MFA_ENCRYPTION_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                if is_production {
                    panic!("MFA_ENCRYPTION_KEY must be set in production (SERVER_URL={})", server_url_val);
                }
                tracing::warn!(
                    "MFA_ENCRYPTION_KEY is not set; falling back to CSRF_SECRET-derived key material (set MFA_ENCRYPTION_KEY in production)"
                );
                csrf_secret.clone()
            });
        let mfa_encryption_key_previous: Vec<String> = env::var("MFA_ENCRYPTION_KEY_PREVIOUS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            port: env::var("PORT")
                .unwrap_or_else(|_| "3001".to_string())
                .parse()
                .unwrap_or(3001),
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_url: env::var("SERVER_URL").unwrap_or_else(|_| {
                let port = env::var("PORT").unwrap_or_else(|_| "3001".to_string());
                format!("http://localhost:{}", port)
            }),
            database_url: Self::resolve_database_url(),
            ed25519_private_key_pem: env::var("ED25519_PRIVATE_KEY").ok(),
            jwt_issuer: env::var("JWT_ISSUER").unwrap_or_else(|_| "gatewasm".to_string()),
            jwt_audience: env::var("JWT_AUDIENCE").unwrap_or_else(|_| "gatewasm-app".to_string()),
            access_token_expiry: env::var("JWT_ACCESS_TOKEN_EXPIRY")
                .unwrap_or_else(|_| "900".to_string())
                .parse()
                .unwrap_or(900),
            refresh_token_expiry: env::var("JWT_REFRESH_TOKEN_EXPIRY")
                .unwrap_or_else(|_| "2592000".to_string())
                .parse()
                .unwrap_or(2592000),
            google_client_id: env::var("GOOGLE_CLIENT_ID").ok(),
            google_client_secret: env::var("GOOGLE_CLIENT_SECRET").ok(),
            github_client_id: env::var("GITHUB_CLIENT_ID").ok(),
            github_client_secret: env::var("GITHUB_CLIENT_SECRET").ok(),
            allowed_origins: env::var("ALLOWED_ORIGINS")
                .unwrap_or_else(|_| "http://localhost:3000,http://localhost:5173".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            csrf_secret,
            mfa_encryption_key,
            mfa_encryption_key_previous,
            platform_admin_key: env::var("PLATFORM_ADMIN_KEY").ok(),
            platform_owner_emails: env::var("PLATFORM_OWNER_EMAILS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            platform_domain: env::var("PLATFORM_DOMAIN").ok(),
            resend_api_key: env::var("RESEND_API_KEY")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            email_from_address: env::var("EMAIL_FROM_ADDRESS")
                .unwrap_or_else(|_| "noreply@getforma.dev".to_string()),
            email_from_name: env::var("EMAIL_FROM_NAME")
                .unwrap_or_else(|_| "Forma Auth".to_string()),
            smtp_host: env::var("SMTP_HOST")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            smtp_port: env::var("SMTP_PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(587),
            smtp_username: env::var("SMTP_USERNAME")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            smtp_password: env::var("SMTP_PASSWORD")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            smtp_from_email: env::var("SMTP_FROM_EMAIL")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            smtp_from_name: env::var("SMTP_FROM_NAME").unwrap_or_else(|_| "Forma Auth".to_string()),
            smtp_starttls: env::var("SMTP_STARTTLS")
                .map(|v| {
                    let v = v.trim().to_ascii_lowercase();
                    !(v == "0" || v == "false" || v == "no" || v == "off")
                })
                .unwrap_or(true),
            compliance: ComplianceConfig::from_env(),
            ssf_signing_key_pem: env::var("SSF_SIGNING_KEY").ok(),
            maxmind_db_path: env::var("MAXMIND_DB_PATH").ok(),
            ws_ticket_ttl_secs: env::var("WS_TICKET_TTL_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
        }
    }
}
