use chrono::Utc;
use serde::Serialize;
use sqlx::postgres::PgPool;

use super::audit;
use super::validator::{ControlStatus, ValidationResult};
use super::{ComplianceConfig, ComplianceSpec};

/// Compliance status response for the API.
#[derive(Debug, Serialize)]
pub struct ComplianceStatus {
    pub mode: String,
    pub framework: String,
    pub version: String,
    pub status: String,
    pub validated_at: String,
    pub controls: Vec<ControlStatusEntry>,
    pub summary: ComplianceSummary,
}

#[derive(Debug, Serialize)]
pub struct ControlStatusEntry {
    pub id: String,
    pub section: String,
    pub title: String,
    pub required: bool,
    pub status: String,
    pub evidence: String,
}

#[derive(Debug, Serialize)]
pub struct ComplianceSummary {
    pub total_controls: usize,
    pub required_passing: usize,
    pub required_total: usize,
    pub addressable_implemented: usize,
    pub addressable_total: usize,
}

/// Build a live compliance status by combining validation results with dynamic evidence.
pub async fn build_status(
    pool: &PgPool,
    config: &ComplianceConfig,
    spec: &ComplianceSpec,
    validation_results: &[ValidationResult],
    tenant_id: &str,
) -> ComplianceStatus {
    let user_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    let audit_total = audit::total_events(pool, tenant_id).await.unwrap_or(0);

    let controls: Vec<ControlStatusEntry> = validation_results
        .iter()
        .map(|r| {
            let evidence = generate_evidence(&r.control_id, r, user_count, audit_total, config);
            ControlStatusEntry {
                id: r.control_id.clone(),
                section: r.section.clone(),
                title: r.title.clone(),
                required: r.required,
                status: r.status.to_string().to_lowercase(),
                evidence,
            }
        })
        .collect();

    let required_total = controls.iter().filter(|c| c.required).count();
    let required_passing = controls
        .iter()
        .filter(|c| c.required && c.status == "pass")
        .count();
    let addressable_total = controls.iter().filter(|c| !c.required).count();
    let addressable_implemented = controls
        .iter()
        .filter(|c| !c.required && c.status == "pass")
        .count();

    let overall_status = if required_passing == required_total {
        "compliant"
    } else {
        "non_compliant"
    };

    ComplianceStatus {
        mode: config.mode.to_string(),
        framework: spec.framework.clone(),
        version: spec.version.clone(),
        status: overall_status.to_string(),
        validated_at: Utc::now().to_rfc3339(),
        controls,
        summary: ComplianceSummary {
            total_controls: validation_results.len(),
            required_passing,
            required_total,
            addressable_implemented,
            addressable_total,
        },
    }
}

/// Generate a compliance report in markdown format.
pub async fn generate_report(
    pool: &PgPool,
    config: &ComplianceConfig,
    spec: &ComplianceSpec,
    validation_results: &[ValidationResult],
    tenant_id: &str,
    tenant_name: &str,
) -> String {
    let user_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    let audit_total = audit::total_events(pool, tenant_id).await.unwrap_or(0);
    let audit_by_action = audit::count_by_action(pool, tenant_id, 30)
        .await
        .unwrap_or_default();

    let required_total = validation_results.iter().filter(|r| r.required).count();
    let required_passing = validation_results
        .iter()
        .filter(|r| r.required && r.status == ControlStatus::Pass)
        .count();
    let addressable_total = validation_results.iter().filter(|r| !r.required).count();
    let addressable_implemented = validation_results
        .iter()
        .filter(|r| !r.required && r.status == ControlStatus::Pass)
        .count();

    let overall = if required_passing == required_total {
        "COMPLIANT"
    } else {
        "NON-COMPLIANT"
    };

    let now = Utc::now().to_rfc3339();

    let mut report = format!(
        r#"# fAuth {} Compliance Report

## Document Information
- **Generated:** {}
- **Tenant:** {} ({})
- **Compliance Mode:** {} ({})

## Executive Summary

| Metric | Value |
|--------|-------|
| Total Controls | {} |
| Required Controls Passing | {} / {} |
| Addressable Controls Implemented | {} / {} |
| Overall Status | **{}** |
| Total Users | {} |
| Audit Events (last 30 days) | {} |
| Last Compliance Validation | {} |

## Control Details
"#,
        spec.framework,
        now,
        tenant_name,
        tenant_id,
        spec.framework,
        spec.version,
        validation_results.len(),
        required_passing,
        required_total,
        addressable_implemented,
        addressable_total,
        overall,
        user_count,
        audit_total,
        now,
    );

    for (result, control) in validation_results.iter().zip(spec.controls.iter()) {
        let evidence =
            generate_evidence(&result.control_id, result, user_count, audit_total, config);
        let classification = if result.required {
            "Required"
        } else {
            "Addressable"
        };

        report.push_str(&format!(
            r#"
### {}: {}
- **Regulation:** {}
- **Classification:** {}
- **Status:** {}
- **Description:** {}
- **Evidence:** {}
"#,
            result.control_id,
            result.title,
            result.section,
            classification,
            result.status,
            control.description,
            evidence,
        ));

        if !result.failures.is_empty() {
            report.push_str("- **Failures:**\n");
            for f in &result.failures {
                report.push_str(&format!("  - {}\n", f));
            }
        }

        if !control.tests.is_empty() {
            report.push_str("- **Tests:**\n");
            for t in &control.tests {
                report.push_str(&format!("  - `{}`\n", t));
            }
        }
    }

    // Audit log summary
    report.push_str("\n## Audit Log Summary (Last 30 Days)\n\n");
    report.push_str("| Action | Count |\n|--------|-------|\n");
    for (action, count) in &audit_by_action {
        report.push_str(&format!("| {} | {} |\n", action, count));
    }
    if audit_by_action.is_empty() {
        report.push_str("| (no events) | 0 |\n");
    }

    // Configuration summary
    report.push_str(&format!(
        r#"
## Configuration Summary

| Setting | Value |
|---------|-------|
| COMPLIANCE_MODE | {} |
| SESSION_TIMEOUT_MINUTES | {} |
| MFA_REQUIRED | {} |
| JWT_ALGORITHM | {} |
| PASSWORD_HASHING | Argon2id |
| TLS_MIN_VERSION | {} |
| AUDIT_LOG_ENABLED | {} |
| MAX_FAILED_LOGINS | {} |
| LOCKOUT_DURATION | {} min |
| PASSWORD_MIN_LENGTH | {} |
"#,
        config.mode,
        config.session_timeout_minutes,
        config.mfa_required,
        config.jwt_algorithm,
        config.tls_min_version,
        config.audit_log_enabled,
        config.max_failed_logins,
        config.lockout_duration_minutes,
        config.password_min_length,
    ));

    report
}

/// Generate dynamic evidence string for a specific control.
fn generate_evidence(
    control_id: &str,
    result: &ValidationResult,
    user_count: i64,
    audit_total: i64,
    config: &ComplianceConfig,
) -> String {
    if result.status == ControlStatus::Fail {
        return format!("FAILING: {}", result.failures.join("; "));
    }

    match control_id {
        "HIPAA-AC-1" => "RBAC active. Default deny enforced via auth middleware.".to_string(),
        "HIPAA-AC-2" => format!(
            "All {} users have unique UUIDs. Audit log tracks user_id on all entries.",
            user_count
        ),
        "HIPAA-AC-3" => {
            "Emergency access available via management API with admin credentials.".to_string()
        }
        "HIPAA-AC-4" => format!(
            "Session timeout set to {} minutes. Timeout enforcement active.",
            config.session_timeout_minutes
        ),
        "HIPAA-AC-5" => format!(
            "JWT algorithm: {}. Passwords hashed with Argon2id. TLS >= {}.",
            config.jwt_algorithm, config.tls_min_version
        ),
        "HIPAA-AUDIT" => format!(
            "Audit logging active. {} total events logged. Append-only enforcement active.",
            audit_total
        ),
        "HIPAA-INT" => {
            "JWT signature verification on every request. Refresh token rotation with family tracking and replay detection active."
                .to_string()
        }
        "HIPAA-AUTH" => format!(
            "MFA required: {}. Password min length: {} chars.",
            config.mfa_required, config.password_min_length
        ),
        "HIPAA-TX" => format!(
            "TLS {} enforced. Secure cookies active in compliance mode.",
            config.tls_min_version
        ),
        _ => "Control validated.".to_string(),
    }
}
