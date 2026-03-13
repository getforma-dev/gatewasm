use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use chrono::Utc;

use crate::compliance::audit::{self, AuditAction, RESULT_DENIED};
use crate::compliance::ComplianceMode;
use crate::error::AppError;
use crate::jwt::signing::Claims;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// Compliance enforcement middleware.
///
/// Runs on authenticated requests when compliance mode is not Standard.
/// Enforces:
/// - Session timeout (based on last_activity of refresh token)
/// - MFA verification (when mfa_required = true)
pub async fn enforce_compliance(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let mode = &state.config.compliance.mode;

    // Skip enforcement in Standard mode
    if *mode == ComplianceMode::Standard {
        return Ok(next.run(req).await);
    }

    let claims = req.extensions().get::<Claims>().cloned();
    let tenant = req.extensions().get::<TenantContext>().cloned();

    // Only enforce on authenticated requests (claims present)
    let claims = match claims {
        Some(c) => c,
        None => return Ok(next.run(req).await),
    };

    let tenant_id = tenant
        .as_ref()
        .map(|t| t.tenant_id.as_str())
        .unwrap_or("unknown");
    let resource = req.uri().path().to_string();
    let ip = extract_ip(&req);

    // ── Session Timeout Enforcement ─────────────────────────────────
    let timeout_minutes = state.config.compliance.session_timeout_minutes;
    if timeout_minutes > 0 {
        // Check if the user's active refresh token has recent activity
        let timed_out = check_session_timeout(&state, &claims.sub, timeout_minutes).await;
        if timed_out {
            audit::log_audit(
                &state.db,
                tenant_id,
                Some(&claims.sub),
                AuditAction::SESSION_TIMEOUT,
                &resource,
                &ip,
                None,
                None,
                RESULT_DENIED,
                Some("Session expired due to inactivity"),
                &mode.to_string(),
            )
            .await;

            return Err(AppError::Unauthorized {
                error: "session_timeout",
                message: "Session expired due to inactivity. Please log in again.".into(),
            });
        }

        // Update last_activity on the user's active tokens
        update_last_activity(&state, &claims.sub).await;
    }

    // ── MFA Enforcement ─────────────────────────────────────────────
    if state.config.compliance.mfa_required {
        let mfa_ok = check_mfa_status(&state, &claims.sub).await;
        if !mfa_ok {
            audit::log_audit(
                &state.db,
                tenant_id,
                Some(&claims.sub),
                AuditAction::ACCESS_DENIED,
                &resource,
                &ip,
                None,
                None,
                RESULT_DENIED,
                Some("MFA not completed"),
                &mode.to_string(),
            )
            .await;

            return Err(AppError::Forbidden {
                error: "mfa_required",
                message: "Multi-factor authentication is required. Please set up MFA.".into(),
            });
        }
    }

    Ok(next.run(req).await)
}

/// Check if the user's session has timed out based on last_activity.
async fn check_session_timeout(state: &AppState, user_id: &str, timeout_minutes: u64) -> bool {
    let cutoff = Utc::now() - chrono::Duration::minutes(timeout_minutes as i64);

    // Find any active (non-revoked, non-expired) token with recent activity
    let has_active = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM refresh_tokens \
         WHERE user_id = $1 AND revoked = FALSE AND expires_at > $2 \
         AND (last_activity IS NULL OR last_activity > $3)",
    )
    .bind(user_id)
    .bind(Utc::now())
    .bind(cutoff)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    has_active == 0
}

/// Update last_activity timestamp on the user's active tokens.
async fn update_last_activity(state: &AppState, user_id: &str) {
    sqlx::query(
        "UPDATE refresh_tokens SET last_activity = $1 WHERE user_id = $2 AND revoked = FALSE",
    )
    .bind(Utc::now())
    .bind(user_id)
    .execute(&state.db)
    .await
    .ok();
}

/// Check if the user has completed MFA for their current session.
async fn check_mfa_status(state: &AppState, user_id: &str) -> bool {
    // First check if user has MFA enabled on their account
    let mfa_enabled = sqlx::query_scalar::<_, bool>(
        "SELECT COALESCE(mfa_enabled, FALSE) FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .unwrap_or(false);

    if !mfa_enabled {
        // User hasn't set up MFA yet — deny access so they must set it up
        return false;
    }

    // Check if any active token has mfa_verified = true
    let verified = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM refresh_tokens \
         WHERE user_id = $1 AND revoked = FALSE AND expires_at > $2 AND mfa_verified = TRUE",
    )
    .bind(user_id)
    .bind(Utc::now())
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    verified > 0
}

fn extract_ip(req: &Request) -> String {
    use std::net::SocketAddr;
    req.extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "0.0.0.0".to_string())
}
