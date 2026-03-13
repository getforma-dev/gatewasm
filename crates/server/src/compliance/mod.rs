pub mod audit;
pub mod enforcer;
pub mod reporter;
pub mod validator;

use serde::{Deserialize, Serialize};

// ── Compliance Mode ─────────────────────────────────────────────────

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComplianceMode {
    #[default]
    Standard,
    #[serde(rename = "hipaa")]
    HIPAA,
    #[serde(rename = "owasp")]
    OWASP,
    #[serde(rename = "soc2")]
    SOC2,
}

impl std::fmt::Display for ComplianceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::HIPAA => write!(f, "hipaa"),
            Self::OWASP => write!(f, "owasp"),
            Self::SOC2 => write!(f, "soc2"),
        }
    }
}

impl std::str::FromStr for ComplianceMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" | "" => Ok(Self::Standard),
            "hipaa" => Ok(Self::HIPAA),
            "owasp" => Ok(Self::OWASP),
            "soc2" => Ok(Self::SOC2),
            other => Err(format!("Unknown compliance mode: {other}")),
        }
    }
}

// ── Compliance Spec Types ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ComplianceSpec {
    pub framework: String,
    pub version: String,
    pub description: String,
    pub controls: Vec<Control>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Control {
    pub id: String,
    pub section: String,
    pub title: String,
    pub description: String,
    pub required: bool,
    pub enforcement: Vec<EnforcementRule>,
    pub tests: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EnforcementRule {
    pub check: String,
    pub operator: String,
    pub value: serde_json::Value,
    pub message: String,
    pub fix: String,
}

// ── Spec Loader ─────────────────────────────────────────────────────

static HIPAA_SPEC_JSON: &str = include_str!("../../../../specs/compliance/hipaa/requirements.json");

pub fn load_spec(mode: &ComplianceMode) -> Option<ComplianceSpec> {
    match mode {
        ComplianceMode::Standard => None,
        ComplianceMode::HIPAA => {
            let spec: ComplianceSpec =
                serde_json::from_str(HIPAA_SPEC_JSON).expect("Failed to parse HIPAA spec");
            Some(spec)
        }
        ComplianceMode::OWASP | ComplianceMode::SOC2 => {
            // Future: load OWASP / SOC2 specs
            tracing::warn!(mode = %mode, "Compliance spec not yet available");
            None
        }
    }
}

// ── Compliance Config ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ComplianceConfig {
    pub mode: ComplianceMode,
    pub session_timeout_minutes: u64,
    pub max_failed_logins: u32,
    pub lockout_duration_minutes: u64,
    pub password_min_length: usize,
    pub password_require_uppercase: bool,
    pub password_require_lowercase: bool,
    pub password_require_number: bool,
    pub password_require_special: bool,
    pub mfa_required: bool,
    pub jwt_algorithm: String,
    pub tls_min_version: String,
    pub audit_log_enabled: bool,
    pub inactive_account_disable_days: u64,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            mode: ComplianceMode::Standard,
            session_timeout_minutes: 60,
            max_failed_logins: 5,
            lockout_duration_minutes: 15,
            password_min_length: 8,
            password_require_uppercase: false,
            password_require_lowercase: false,
            password_require_number: false,
            password_require_special: false,
            mfa_required: false,
            jwt_algorithm: "EdDSA".to_string(),
            tls_min_version: "1.2".to_string(),
            audit_log_enabled: true,
            inactive_account_disable_days: 0,
        }
    }
}

impl ComplianceConfig {
    pub fn from_env() -> Self {
        let mode = std::env::var("COMPLIANCE_MODE")
            .unwrap_or_default()
            .parse()
            .unwrap_or_default();

        let mut config = Self {
            mode,
            session_timeout_minutes: std::env::var("SESSION_TIMEOUT_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            max_failed_logins: std::env::var("MAX_FAILED_LOGINS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            lockout_duration_minutes: std::env::var("LOCKOUT_DURATION_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            password_min_length: std::env::var("PASSWORD_MIN_LENGTH")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8),
            password_require_uppercase: std::env::var("PASSWORD_REQUIRE_UPPERCASE")
                .ok()
                .map(|v| v == "true")
                .unwrap_or(false),
            password_require_lowercase: std::env::var("PASSWORD_REQUIRE_LOWERCASE")
                .ok()
                .map(|v| v == "true")
                .unwrap_or(false),
            password_require_number: std::env::var("PASSWORD_REQUIRE_NUMBER")
                .ok()
                .map(|v| v == "true")
                .unwrap_or(false),
            password_require_special: std::env::var("PASSWORD_REQUIRE_SPECIAL")
                .ok()
                .map(|v| v == "true")
                .unwrap_or(false),
            mfa_required: std::env::var("MFA_REQUIRED")
                .ok()
                .map(|v| v == "true")
                .unwrap_or(false),
            jwt_algorithm: std::env::var("JWT_ALGORITHM").unwrap_or_else(|_| "EdDSA".to_string()),
            tls_min_version: std::env::var("TLS_MIN_VERSION").unwrap_or_else(|_| "1.2".to_string()),
            audit_log_enabled: std::env::var("AUDIT_LOG_ENABLED")
                .ok()
                .map(|v| v == "true")
                .unwrap_or(true),
            inactive_account_disable_days: std::env::var("INACTIVE_ACCOUNT_DISABLE_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        };

        apply_compliance_overrides(&mut config);
        config
    }
}

/// Override config values that are weaker than what the compliance mode requires.
/// Logs every override so the developer sees exactly what changed and why.
pub fn apply_compliance_overrides(config: &mut ComplianceConfig) {
    match config.mode {
        ComplianceMode::HIPAA => {
            // HIPAA 164.312(a)(2)(iii): Automatic logoff
            if config.session_timeout_minutes > 15 {
                tracing::info!(
                    "HIPAA override: session_timeout_minutes {} -> 15",
                    config.session_timeout_minutes
                );
                config.session_timeout_minutes = 15;
            }
            // HIPAA 164.312(d): Person or entity authentication
            if !config.mfa_required {
                tracing::info!("HIPAA override: mfa_required false -> true");
                config.mfa_required = true;
            }
            // HIPAA 164.312(a)(2)(iv): Encryption — reject HS256
            if config.jwt_algorithm == "HS256" {
                tracing::info!("HIPAA override: jwt_algorithm HS256 -> EdDSA");
                config.jwt_algorithm = "EdDSA".to_string();
            }
            // HIPAA: Brute force lockout
            if config.max_failed_logins > 5 {
                tracing::info!(
                    "HIPAA override: max_failed_logins {} -> 5",
                    config.max_failed_logins
                );
                config.max_failed_logins = 5;
            }
            // Force audit logging
            if !config.audit_log_enabled {
                tracing::info!("HIPAA override: audit_log_enabled false -> true");
            }
            config.audit_log_enabled = true;
        }
        ComplianceMode::OWASP => {
            if config.password_min_length < 8 {
                tracing::info!(
                    "OWASP override: password_min_length {} -> 8",
                    config.password_min_length
                );
                config.password_min_length = 8;
            }
            if config.session_timeout_minutes > 30 {
                tracing::info!(
                    "OWASP override: session_timeout_minutes {} -> 30",
                    config.session_timeout_minutes
                );
                config.session_timeout_minutes = 30;
            }
            if config.max_failed_logins > 10 {
                tracing::info!(
                    "OWASP override: max_failed_logins {} -> 10",
                    config.max_failed_logins
                );
                config.max_failed_logins = 10;
            }
            config.audit_log_enabled = true;
        }
        ComplianceMode::SOC2 => {
            if config.password_min_length < 8 {
                config.password_min_length = 8;
            }
            if !config.mfa_required {
                tracing::info!("SOC2 override: mfa_required false -> true");
                config.mfa_required = true;
            }
            if config.session_timeout_minutes > 30 {
                tracing::info!(
                    "SOC2 override: session_timeout_minutes {} -> 30",
                    config.session_timeout_minutes
                );
                config.session_timeout_minutes = 30;
            }
            config.audit_log_enabled = true;
            config.password_require_uppercase = true;
            config.password_require_number = true;
        }
        ComplianceMode::Standard => {}
    }
}
