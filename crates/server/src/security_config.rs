use serde::{Deserialize, Serialize};

/// Per-tenant security configuration, stored as JSONB in the `security_config` column.
/// Fields are grouped by plan tier: Foundation (all ON by default), Pro, Enterprise, Compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSecurityConfig {
    // ── Foundation (default ON) ─────────────────────────────────────
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_minutes: u32,
    #[serde(default = "default_true")]
    pub host_prefix_cookies: bool,
    #[serde(default = "default_true")]
    pub csp_enabled: bool,
    #[serde(default = "default_true")]
    pub client_fingerprint: bool,
    #[serde(default = "default_max_sessions")]
    pub max_sessions_per_user: u32,

    // ── Pro (default OFF) ───────────────────────────────────────────
    #[serde(default)]
    pub webauthn_enabled: bool,
    #[serde(default = "default_attestation")]
    pub webauthn_attestation: String,
    #[serde(default)]
    pub webauthn_require_device_bound: bool,
    #[serde(default)]
    pub fedcm_enabled: bool,

    // ── Enterprise (default OFF) ────────────────────────────────────
    #[serde(default)]
    pub dpop_required: bool,
    #[serde(default)]
    pub par_required: bool,
    #[serde(default)]
    pub mtls_enabled: bool,
    #[serde(default)]
    pub introspection_enabled: bool,

    // ── Compliance (default OFF) ────────────────────────────────────
    #[serde(default)]
    pub caep_enabled: bool,
    #[serde(default)]
    pub impossible_travel_enabled: bool,
    #[serde(default = "default_travel_speed")]
    pub impossible_travel_speed_threshold_kmh: u32,
}

fn default_idle_timeout() -> u32 {
    30
}
fn default_true() -> bool {
    true
}
fn default_attestation() -> String {
    "none".to_string()
}
fn default_travel_speed() -> u32 {
    800
}
fn default_max_sessions() -> u32 {
    10
}

impl Default for TenantSecurityConfig {
    fn default() -> Self {
        Self {
            idle_timeout_minutes: 30,
            host_prefix_cookies: true,
            csp_enabled: true,
            client_fingerprint: true,
            max_sessions_per_user: 10,
            webauthn_enabled: false,
            webauthn_attestation: "none".to_string(),
            webauthn_require_device_bound: false,
            fedcm_enabled: false,
            dpop_required: false,
            par_required: false,
            mtls_enabled: false,
            introspection_enabled: false,
            caep_enabled: false,
            impossible_travel_enabled: false,
            impossible_travel_speed_threshold_kmh: 800,
        }
    }
}

impl TenantSecurityConfig {
    /// Parse from a JSONB value, falling back to defaults for missing fields.
    pub fn from_json(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    /// Returns a list of violations for features that require a higher plan tier.
    /// Empty vec means the config is valid for the given plan.
    pub fn validate_for_plan(&self, plan: &str) -> Vec<String> {
        let mut violations = Vec::new();

        let is_pro_or_above = matches!(plan, "pro" | "enterprise" | "compliance");
        let is_enterprise_or_above = matches!(plan, "enterprise" | "compliance");
        let is_compliance = plan == "compliance";

        // Pro features
        if self.webauthn_enabled && !is_pro_or_above {
            violations.push("webauthn_enabled requires pro plan or above".to_string());
        }
        if self.fedcm_enabled && !is_pro_or_above {
            violations.push("fedcm_enabled requires pro plan or above".to_string());
        }

        // Enterprise features
        if self.dpop_required && !is_enterprise_or_above {
            violations.push("dpop_required requires enterprise plan or above".to_string());
        }
        if self.par_required && !is_enterprise_or_above {
            violations.push("par_required requires enterprise plan or above".to_string());
        }
        if self.mtls_enabled && !is_enterprise_or_above {
            violations.push("mtls_enabled requires enterprise plan or above".to_string());
        }
        if self.introspection_enabled && !is_enterprise_or_above {
            violations.push("introspection_enabled requires enterprise plan or above".to_string());
        }

        // Compliance features
        if self.caep_enabled && !is_compliance {
            violations.push("caep_enabled requires compliance plan".to_string());
        }
        if self.impossible_travel_enabled && !is_compliance {
            violations.push("impossible_travel_enabled requires compliance plan".to_string());
        }

        // Range validation
        if self.max_sessions_per_user < 1 || self.max_sessions_per_user > 100 {
            violations.push("max_sessions_per_user must be between 1 and 100".to_string());
        }

        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_foundation_features_on() {
        let config = TenantSecurityConfig::default();
        assert_eq!(config.idle_timeout_minutes, 30);
        assert!(config.host_prefix_cookies);
        assert!(config.csp_enabled);
        assert!(config.client_fingerprint);
        assert!(!config.webauthn_enabled);
        assert!(!config.dpop_required);
        assert!(!config.caep_enabled);
    }

    #[test]
    fn from_json_empty_object_uses_defaults() {
        let val = serde_json::json!({});
        let config = TenantSecurityConfig::from_json(&val);
        assert_eq!(config.idle_timeout_minutes, 30);
        assert!(config.host_prefix_cookies);
        assert!(config.csp_enabled);
        assert!(config.client_fingerprint);
    }

    #[test]
    fn from_json_partial_override() {
        let val = serde_json::json!({
            "idle_timeout_minutes": 15,
            "webauthn_enabled": true
        });
        let config = TenantSecurityConfig::from_json(&val);
        assert_eq!(config.idle_timeout_minutes, 15);
        assert!(config.webauthn_enabled);
        // Other defaults still hold
        assert!(config.host_prefix_cookies);
        assert!(!config.dpop_required);
    }

    #[test]
    fn from_json_invalid_falls_back_to_default() {
        let val = serde_json::json!("not an object");
        let config = TenantSecurityConfig::from_json(&val);
        assert_eq!(config.idle_timeout_minutes, 30);
    }

    #[test]
    fn validate_for_plan_trial_rejects_pro_features() {
        let config = TenantSecurityConfig {
            webauthn_enabled: true,
            ..Default::default()
        };
        let violations = config.validate_for_plan("trial");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("webauthn_enabled"));
    }

    #[test]
    fn validate_for_plan_pro_allows_pro_features() {
        let config = TenantSecurityConfig {
            webauthn_enabled: true,
            fedcm_enabled: true,
            ..Default::default()
        };
        let violations = config.validate_for_plan("pro");
        assert!(violations.is_empty());
    }

    #[test]
    fn validate_for_plan_pro_rejects_enterprise_features() {
        let config = TenantSecurityConfig {
            dpop_required: true,
            par_required: true,
            ..Default::default()
        };
        let violations = config.validate_for_plan("pro");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn validate_for_plan_enterprise_allows_enterprise_features() {
        let config = TenantSecurityConfig {
            webauthn_enabled: true,
            dpop_required: true,
            par_required: true,
            mtls_enabled: true,
            introspection_enabled: true,
            ..Default::default()
        };
        let violations = config.validate_for_plan("enterprise");
        assert!(violations.is_empty());
    }

    #[test]
    fn validate_for_plan_enterprise_rejects_compliance_features() {
        let config = TenantSecurityConfig {
            caep_enabled: true,
            impossible_travel_enabled: true,
            ..Default::default()
        };
        let violations = config.validate_for_plan("enterprise");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn validate_for_plan_compliance_allows_everything() {
        let config = TenantSecurityConfig {
            webauthn_enabled: true,
            fedcm_enabled: true,
            dpop_required: true,
            par_required: true,
            mtls_enabled: true,
            introspection_enabled: true,
            caep_enabled: true,
            impossible_travel_enabled: true,
            ..Default::default()
        };
        let violations = config.validate_for_plan("compliance");
        assert!(violations.is_empty());
    }

    #[test]
    fn validate_for_plan_foundation_only_is_always_valid() {
        let config = TenantSecurityConfig::default();
        for plan in &["trial", "foundation", "pro", "enterprise", "compliance"] {
            assert!(config.validate_for_plan(plan).is_empty());
        }
    }

    #[test]
    fn default_max_sessions_is_ten() {
        let config = TenantSecurityConfig::default();
        assert_eq!(config.max_sessions_per_user, 10);
    }

    #[test]
    fn from_json_custom_max_sessions() {
        let val = serde_json::json!({ "max_sessions_per_user": 5 });
        let config = TenantSecurityConfig::from_json(&val);
        assert_eq!(config.max_sessions_per_user, 5);
    }

    #[test]
    fn from_json_missing_max_sessions_uses_default() {
        let val = serde_json::json!({});
        let config = TenantSecurityConfig::from_json(&val);
        assert_eq!(config.max_sessions_per_user, 10);
    }

    #[test]
    fn serialization_roundtrip() {
        let config = TenantSecurityConfig {
            idle_timeout_minutes: 15,
            max_sessions_per_user: 5,
            webauthn_enabled: true,
            dpop_required: true,
            ..Default::default()
        };
        let json = serde_json::to_value(&config).unwrap();
        let parsed = TenantSecurityConfig::from_json(&json);
        assert_eq!(parsed.idle_timeout_minutes, 15);
        assert_eq!(parsed.max_sessions_per_user, 5);
        assert!(parsed.webauthn_enabled);
        assert!(parsed.dpop_required);
        assert!(parsed.host_prefix_cookies); // default preserved
    }
}
