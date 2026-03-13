use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};

use crate::compliance::{self, validator};
use crate::error::AppError;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// GET /api/v1/compliance/status
///
/// Returns the current compliance status for the tenant.
pub async fn compliance_status(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = req
        .extensions()
        .get::<TenantContext>()
        .ok_or(AppError::BadRequest {
            error: "missing_tenant",
            message: "Tenant context required".into(),
        })?
        .clone();

    let mode = &state.config.compliance.mode;
    let spec = compliance::load_spec(mode).ok_or(AppError::BadRequest {
        error: "no_compliance_mode",
        message: format!(
            "No compliance spec for mode: {mode}. Set COMPLIANCE_MODE to hipaa, owasp, or soc2."
        ),
    })?;

    let results = validator::validate_compliance(&state.config.compliance, &spec);
    let status = compliance::reporter::build_status(
        &state.db,
        &state.config.compliance,
        &spec,
        &results,
        &tenant.tenant_id,
    )
    .await;

    Ok(Json(status))
}

#[derive(serde::Deserialize)]
pub struct ReportQuery {
    pub format: Option<String>,
}

/// GET /api/v1/compliance/report
///
/// Generates a downloadable compliance report.
/// ?format=json for JSON, otherwise returns markdown.
pub async fn compliance_report(
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = req
        .extensions()
        .get::<TenantContext>()
        .ok_or(AppError::BadRequest {
            error: "missing_tenant",
            message: "Tenant context required".into(),
        })?
        .clone();

    let mode = &state.config.compliance.mode;
    let spec = compliance::load_spec(mode).ok_or(AppError::BadRequest {
        error: "no_compliance_mode",
        message: format!("No compliance spec for mode: {mode}"),
    })?;

    let results = validator::validate_compliance(&state.config.compliance, &spec);

    if query.format.as_deref() == Some("json") {
        let status = compliance::reporter::build_status(
            &state.db,
            &state.config.compliance,
            &spec,
            &results,
            &tenant.tenant_id,
        )
        .await;
        Ok(Json(serde_json::json!(status)).into_response())
    } else {
        let report = compliance::reporter::generate_report(
            &state.db,
            &state.config.compliance,
            &spec,
            &results,
            &tenant.tenant_id,
            &tenant.name,
        )
        .await;

        Ok((
            [(
                axum::http::header::CONTENT_TYPE,
                "text/markdown; charset=utf-8",
            )],
            report,
        )
            .into_response())
    }
}
