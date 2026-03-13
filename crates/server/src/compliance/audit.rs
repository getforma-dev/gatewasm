use sqlx::postgres::PgPool;

/// All auditable auth actions.
pub struct AuditAction;

impl AuditAction {
    pub const USER_REGISTER: &'static str = "user.register";
    pub const USER_LOGIN: &'static str = "user.login";
    pub const USER_LOGIN_FAILED: &'static str = "user.login_failed";
    pub const USER_LOGOUT: &'static str = "user.logout";
    pub const USER_TOKEN_REFRESH: &'static str = "user.token_refresh";
    pub const USER_TOKEN_REVOKED: &'static str = "user.token_revoked";
    pub const USER_PASSWORD_CHANGE: &'static str = "user.password_change";
    pub const USER_PASSWORD_RESET_REQUEST: &'static str = "user.password_reset_request";
    pub const USER_PASSWORD_RESET_COMPLETE: &'static str = "user.password_reset_complete";
    pub const USER_ACCOUNT_DELETE: &'static str = "user.account_delete";
    pub const USER_EMAIL_VERIFIED: &'static str = "user.email_verified";
    pub const ACCESS_GRANTED: &'static str = "access.granted";
    pub const ACCESS_DENIED: &'static str = "access.denied";
    pub const SESSION_TIMEOUT: &'static str = "session.timeout";
    pub const ACCOUNT_LOCKED: &'static str = "account.locked";
    pub const ACCOUNT_UNLOCKED: &'static str = "account.unlocked";
    // SSF audit actions
    pub const SSF_EVENT_CREATED: &'static str = "ssf.event.created";
    pub const SSF_EVENT_DELIVERED: &'static str = "ssf.event.delivered";
    pub const SSF_EVENT_FAILED: &'static str = "ssf.event.failed";
    pub const SSF_STREAM_CREATED: &'static str = "ssf.stream.created";
    pub const SSF_STREAM_PAUSED: &'static str = "ssf.stream.paused";
    pub const SSF_STREAM_DISABLED: &'static str = "ssf.stream.disabled";
    pub const SSF_STREAM_RESUMED: &'static str = "ssf.stream.resumed";
    pub const SSF_POLL_ACK: &'static str = "ssf.poll.ack";
}

pub const RESULT_SUCCESS: &str = "success";
pub const RESULT_FAILURE: &str = "failure";
pub const RESULT_DENIED: &str = "denied";

/// Log an audit event to the append-only audit_log table.
///
/// COMPLIANCE: This function only INSERTs. Never UPDATE or DELETE audit_log rows.
#[allow(clippy::too_many_arguments)]
pub async fn log_audit(
    pool: &PgPool,
    tenant_id: &str,
    user_id: Option<&str>,
    action: &str,
    resource: &str,
    ip: &str,
    user_agent: Option<&str>,
    session_id: Option<&str>,
    result: &str,
    failure_reason: Option<&str>,
    compliance_mode: &str,
) {
    let event_id = format!("evt_{}", uuid::Uuid::new_v4());
    let timestamp = chrono::Utc::now();

    let res = sqlx::query(
        "INSERT INTO audit_log (event_id, timestamp, tenant_id, user_id, action, resource, ip_address, user_agent, session_id, result, failure_reason, compliance_mode) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"
    )
    .bind(&event_id)
    .bind(timestamp)
    .bind(tenant_id)
    .bind(user_id)
    .bind(action)
    .bind(resource)
    .bind(ip)
    .bind(user_agent)
    .bind(session_id)
    .bind(result)
    .bind(failure_reason)
    .bind(compliance_mode)
    .execute(pool)
    .await;

    if let Err(e) = res {
        tracing::error!(error = %e, action = action, "Failed to write audit log entry");
    }
}

/// Count audit events by action for a tenant (for reports).
pub async fn count_by_action(
    pool: &PgPool,
    tenant_id: &str,
    days: i64,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let since = chrono::Utc::now() - chrono::Duration::days(days);
    sqlx::query_as::<_, (String, i64)>(
        "SELECT action, COUNT(*) as count FROM audit_log WHERE tenant_id = $1 AND timestamp > $2 GROUP BY action ORDER BY count DESC",
    )
    .bind(tenant_id)
    .bind(since)
    .fetch_all(pool)
    .await
}

/// Count total audit events for a tenant.
pub async fn total_events(pool: &PgPool, tenant_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
}
