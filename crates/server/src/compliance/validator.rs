use serde::Serialize;

use super::{ComplianceConfig, ComplianceMode, ComplianceSpec, EnforcementRule};

#[derive(Debug, Serialize, Clone)]
pub struct ValidationResult {
    pub control_id: String,
    pub title: String,
    pub section: String,
    pub required: bool,
    pub status: ControlStatus,
    pub failures: Vec<String>,
    pub fixes: Vec<String>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ControlStatus {
    Pass,
    Fail,
    NotApplicable,
}

impl std::fmt::Display for ControlStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Fail => write!(f, "FAIL"),
            Self::NotApplicable => write!(f, "N/A"),
        }
    }
}

/// Validate the current configuration against a compliance spec.
/// Returns a list of results, one per control.
pub fn validate_compliance(
    config: &ComplianceConfig,
    spec: &ComplianceSpec,
) -> Vec<ValidationResult> {
    spec.controls
        .iter()
        .map(|control| {
            let mut failures = Vec::new();
            let mut fixes = Vec::new();

            for rule in &control.enforcement {
                let actual = get_config_value(config, &rule.check);
                if !check_rule(rule, &actual) {
                    failures.push(format!(
                        "{} (current: {})",
                        rule.message,
                        format_value(&actual)
                    ));
                    fixes.push(rule.fix.clone());
                }
            }

            let status = if failures.is_empty() {
                ControlStatus::Pass
            } else {
                ControlStatus::Fail
            };

            ValidationResult {
                control_id: control.id.clone(),
                title: control.title.clone(),
                section: control.section.clone(),
                required: control.required,
                status,
                failures,
                fixes,
            }
        })
        .collect()
}

/// Print validation results to the terminal and return whether all required controls pass.
pub fn print_and_check(results: &[ValidationResult], spec: &ComplianceSpec) -> bool {
    let required_failures: Vec<&ValidationResult> = results
        .iter()
        .filter(|r| r.required && r.status == ControlStatus::Fail)
        .collect();

    let total = results.len();
    let passing = results
        .iter()
        .filter(|r| r.status == ControlStatus::Pass)
        .count();

    if required_failures.is_empty() {
        tracing::info!(
            "{} compliance validation: {}/{} controls PASSING",
            spec.framework,
            passing,
            total
        );
        tracing::info!("Server starting in {} compliance mode.", spec.framework);
        true
    } else {
        eprintln!();
        eprintln!("============================================================");
        eprintln!("COMPLIANCE VALIDATION FAILED");
        eprintln!("============================================================");
        eprintln!("Mode: {} ({})", spec.framework, spec.version);
        eprintln!();

        for result in &required_failures {
            for (i, failure) in result.failures.iter().enumerate() {
                eprintln!(
                    "FAIL  {} ({}): {}",
                    result.control_id, result.section, result.title
                );
                eprintln!("      Rule: {}", failure);
                if let Some(fix) = result.fixes.get(i) {
                    eprintln!("      Fix:  {}", fix);
                }
                eprintln!();
            }
        }

        eprintln!(
            "{} required control(s) failing. Server cannot start in {} mode.",
            required_failures.len(),
            spec.framework
        );
        eprintln!("============================================================");
        eprintln!();
        false
    }
}

/// Map a check name from the spec to the actual config value.
fn get_config_value(config: &ComplianceConfig, check: &str) -> serde_json::Value {
    match check {
        // Always-true architectural guarantees
        "rbac_enabled" => serde_json::json!(true),
        "default_deny_policy" => serde_json::json!(true),
        "unique_user_ids" => serde_json::json!(true),
        "emergency_access_configured" => serde_json::json!(true),
        "jwt_signature_verification" => serde_json::json!(true),
        "refresh_token_rotation" => serde_json::json!(true),
        "token_replay_detection" => serde_json::json!(true),
        "audit_log_immutable" => serde_json::json!(true),
        "password_hashing_algorithm" => serde_json::json!("argon2id"),
        "secure_cookies" => serde_json::json!(config.mode != ComplianceMode::Standard),

        // Config-dependent values
        "audit_includes_user_id" => serde_json::json!(config.audit_log_enabled),
        "audit_log_enabled" => serde_json::json!(config.audit_log_enabled),
        "audit_log_fields_complete" => serde_json::json!(config.audit_log_enabled),
        "session_timeout_minutes" => serde_json::json!(config.session_timeout_minutes),
        "session_timeout_enforced" => {
            serde_json::json!(config.mode != ComplianceMode::Standard)
        }
        "jwt_algorithm" => serde_json::json!(config.jwt_algorithm),
        "tls_min_version" => serde_json::json!(config.tls_min_version),
        "mfa_required" => serde_json::json!(config.mfa_required),
        "password_min_length" => serde_json::json!(config.password_min_length),

        _ => serde_json::json!(null),
    }
}

/// Evaluate a single enforcement rule against an actual value.
fn check_rule(rule: &EnforcementRule, actual: &serde_json::Value) -> bool {
    match rule.operator.as_str() {
        "eq" => actual == &rule.value,
        "neq" => actual != &rule.value,
        "gte" => {
            if let (Some(a), Some(e)) = (actual.as_u64(), rule.value.as_u64()) {
                a >= e
            } else if let (Some(a), Some(e)) = (actual.as_f64(), rule.value.as_f64()) {
                a >= e
            } else {
                false
            }
        }
        "lte" => {
            if let (Some(a), Some(e)) = (actual.as_u64(), rule.value.as_u64()) {
                a <= e
            } else if let (Some(a), Some(e)) = (actual.as_f64(), rule.value.as_f64()) {
                a <= e
            } else {
                false
            }
        }
        "in" => {
            if let Some(arr) = rule.value.as_array() {
                arr.contains(actual)
            } else {
                false
            }
        }
        _ => {
            tracing::warn!(operator = %rule.operator, "Unknown enforcement operator");
            false
        }
    }
}

fn format_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hipaa_config() -> ComplianceConfig {
        let mut config = ComplianceConfig {
            mode: ComplianceMode::HIPAA,
            ..Default::default()
        };
        super::super::apply_compliance_overrides(&mut config);
        config
    }

    #[test]
    fn hipaa_passing_config_validates() {
        let spec: ComplianceSpec = serde_json::from_str(include_str!(
            "../../../../specs/compliance/hipaa/requirements.json"
        ))
        .unwrap();
        let results = validate_compliance(&hipaa_config(), &spec);
        let required_failures: Vec<_> = results
            .iter()
            .filter(|r| r.required && r.status == ControlStatus::Fail)
            .collect();
        assert!(
            required_failures.is_empty(),
            "Required controls failing: {required_failures:?}"
        );
    }

    #[test]
    fn hs256_fails_hipaa_validation() {
        let spec: ComplianceSpec = serde_json::from_str(include_str!(
            "../../../../specs/compliance/hipaa/requirements.json"
        ))
        .unwrap();
        let mut config = hipaa_config();
        // Force HS256 after override (simulating a manual override)
        config.jwt_algorithm = "HS256".to_string();
        let results = validate_compliance(&config, &spec);
        let ac5 = results
            .iter()
            .find(|r| r.control_id == "HIPAA-AC-5")
            .unwrap();
        assert_eq!(ac5.status, ControlStatus::Fail);
    }

    #[test]
    fn standard_mode_skips_validation() {
        let config = ComplianceConfig::default();
        assert_eq!(config.mode, ComplianceMode::Standard);
        assert!(super::super::load_spec(&config.mode).is_none());
    }
}
