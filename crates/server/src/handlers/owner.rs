use axum::{
    extract::{FromRequest, Path, Query, State},
    response::{Html, IntoResponse, Redirect},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use crate::error::AppError;
use crate::html_template::{render_page, PageConfig};
use crate::handlers::auth as auth_handlers;
use crate::handlers::auth::normalize_email;
use crate::jwt::signing;
use crate::middleware::platform_user::PlatformUser;
use crate::{AppState, RenderMode};

#[derive(Debug, Deserialize)]
pub struct ListCustomersQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub q: Option<String>,
    pub status: Option<String>,
    pub category: Option<String>,
    pub plan: Option<String>,
    pub created_from: Option<String>,
    pub created_to: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListTenantsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub q: Option<String>,
    pub status: Option<String>,
    pub plan: Option<String>,
    pub owner_user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OwnerAnalyticsQuery {
    pub days: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct OwnerTimelineQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub resource: Option<String>,
    pub request_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListPaymentsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
    pub customer_user_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePaymentEventRequest {
    pub customer_user_id: String,
    pub tenant_id: Option<String>,
    pub amount_cents: i64,
    pub currency: Option<String>,
    pub status: Option<String>,
    pub external_invoice_id: Option<String>,
    pub receipt_url: Option<String>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub notes: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCustomerLifecycleRequest {
    pub status: Option<String>,
    pub category: Option<String>,
    pub company_name: Option<String>,
    pub billing_ref: Option<String>,
    pub notes: Option<String>,
    pub risk_level: Option<String>,
    pub lifecycle_stage: Option<String>,
    pub blocked_reason: Option<String>,
    pub apply_to_tenants: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCustomerTenantStatusRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteCustomerTenantRequest {
    pub confirm_slug: String,
}

#[derive(Debug, Deserialize)]
pub struct ExtendAccessRequest {
    pub days: i64,
    pub tenant_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateOwnerInviteRequest {
    pub email: String,
    pub expires_in_hours: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AcceptOwnerInviteRequest {
    pub token: String,
}

fn clamp_limit(limit: Option<i64>, default_limit: i64, max_limit: i64) -> i64 {
    let value = limit.unwrap_or(default_limit);
    value.clamp(1, max_limit)
}

fn parse_rfc3339_opt(
    raw: &Option<String>,
    field: &'static str,
) -> Result<Option<DateTime<Utc>>, AppError> {
    match raw {
        Some(value) if !value.trim().is_empty() => DateTime::parse_from_rfc3339(value.trim())
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
            .map_err(|_| AppError::BadRequest {
                error: "invalid_datetime",
                message: format!("Invalid RFC3339 datetime for '{}'", field),
            }),
        _ => Ok(None),
    }
}

fn normalize_optional_field(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn sort_clause(raw: Option<&str>) -> &'static str {
    match raw.unwrap_or("created_desc") {
        "created_asc" => "fc.created_at ASC",
        "tenants_desc" => "tenant_count DESC, fc.created_at DESC",
        "revenue_desc" => "lifetime_revenue_cents DESC, fc.created_at DESC",
        _ => "fc.created_at DESC",
    }
}

fn owner_claims(req: &axum::extract::Request) -> Result<signing::Claims, AppError> {
    req.extensions()
        .get::<signing::Claims>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })
}

fn request_id_from_req(req: &axum::extract::Request) -> Option<String> {
    req.headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

struct OwnerEvent<'a> {
    customer_user_id: Option<&'a str>,
    tenant_id: Option<&'a str>,
    actor_user_id: Option<&'a str>,
    actor_email: Option<&'a str>,
    action: &'a str,
    resource: &'a str,
    request_id: Option<&'a str>,
    details: serde_json::Value,
}

struct OwnerRefreshSessionValidation {
    valid: bool,
    reason: &'static str,
}

async fn validate_owner_refresh_session(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<OwnerRefreshSessionValidation, sqlx::Error> {
    let refresh_tokens =
        auth_handlers::extract_refresh_cookie_candidates(headers, Some("platform"));
    if refresh_tokens.is_empty() {
        return Ok(OwnerRefreshSessionValidation {
            valid: false,
            reason: "missing_cookie",
        });
    }

    let refresh_hashes: Vec<String> = refresh_tokens
        .iter()
        .map(|token| signing::hash_token(token))
        .collect();

    let rows = sqlx::query_as::<_, (bool, bool, bool, bool, bool, bool)>(
        "SELECT
            rt.revoked,
            (rt.expires_at > NOW()) AS not_expired,
            (rt.auth_method = 'sso') AS auth_method_ok,
            rt.mfa_verified,
            (u.tenant_id = 'ten_platform') AS user_tenant_ok,
            EXISTS(
                SELECT 1
                FROM user_roles ur
                WHERE ur.user_id = rt.user_id
                  AND ur.tenant_id = 'ten_platform'
                  AND ur.role_id = 'role_platform_owner'
            ) AS owner_role_ok
         FROM refresh_tokens rt
         JOIN users u ON u.id = rt.user_id
         WHERE rt.token_hash = ANY($1)
           AND rt.tenant_id = 'ten_platform'
         ORDER BY rt.expires_at DESC",
    )
    .bind(&refresh_hashes)
    .fetch_all(&state.db)
    .await?;

    if rows.is_empty() {
        return Ok(OwnerRefreshSessionValidation {
            valid: false,
            reason: "token_not_found",
        });
    };

    let is_prod = state.config.is_production();

    if rows.iter().any(
        |(revoked, not_expired, auth_method_ok, mfa_verified, user_tenant_ok, owner_role_ok)| {
            !*revoked
                && *not_expired
                && (!is_prod || *auth_method_ok)
                && (!is_prod || *mfa_verified)
                && *user_tenant_ok
                && *owner_role_ok
        },
    ) {
        return Ok(OwnerRefreshSessionValidation {
            valid: true,
            reason: "ok",
        });
    }

    let (revoked, not_expired, auth_method_ok, mfa_verified, user_tenant_ok, owner_role_ok) =
        rows[0];
    let reason = if revoked {
        "token_revoked"
    } else if !not_expired {
        "token_expired"
    } else if !user_tenant_ok {
        "wrong_tenant"
    } else if !owner_role_ok {
        "missing_owner_role"
    } else if is_prod && !auth_method_ok {
        "owner_sso_required"
    } else if is_prod && !mfa_verified {
        "mfa_not_verified"
    } else {
        "session_invalid"
    };

    Ok(OwnerRefreshSessionValidation {
        valid: false,
        reason,
    })
}

async fn record_owner_event(state: &AppState, event: OwnerEvent<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO owner_timeline_events
         (id, customer_user_id, tenant_id, actor_user_id, actor_email, action, resource, request_id, details, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())",
    )
    .bind(format!("oev_{}", uuid::Uuid::new_v4()))
    .bind(event.customer_user_id)
    .bind(event.tenant_id)
    .bind(event.actor_user_id)
    .bind(event.actor_email)
    .bind(event.action)
    .bind(event.resource)
    .bind(event.request_id)
    .bind(event.details)
    .execute(&state.db)
    .await?;

    Ok(())
}

/// GET /platform/admin/stats — Provider-level summary stats.
pub async fn owner_stats(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let (
        customers,
        active_customers,
        suspended_customers,
        rejected_customers,
        tenants,
        active_tenants,
        suspended_tenants,
        trial_expiring_7d,
        tenant_end_users,
        revenue_30d_cents,
        paid_receipts_30d,
        failed_logins_24h,
    ) = sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64)>(
        "WITH customer_counts AS (
            SELECT
                COUNT(*) FILTER (WHERE status != 'deleted')::BIGINT AS customers,
                COUNT(*) FILTER (WHERE status = 'active')::BIGINT AS active_customers,
                COUNT(*) FILTER (WHERE status = 'suspended')::BIGINT AS suspended_customers,
                COUNT(*) FILTER (WHERE status = 'rejected')::BIGINT AS rejected_customers
            FROM platform_accounts
        ),
        tenant_counts AS (
            SELECT
                COUNT(*) FILTER (WHERE id != 'ten_platform' AND status != 'deleted')::BIGINT AS tenants,
                COUNT(*) FILTER (WHERE id != 'ten_platform' AND status = 'active')::BIGINT AS active_tenants,
                COUNT(*) FILTER (WHERE id != 'ten_platform' AND status = 'suspended')::BIGINT AS suspended_tenants,
                COUNT(*) FILTER (
                    WHERE id != 'ten_platform'
                      AND status = 'active'
                      AND COALESCE(plan, 'trial') = 'trial'
                      AND trial_ends_at IS NOT NULL
                      AND trial_ends_at <= NOW() + INTERVAL '7 days'
                )::BIGINT AS trial_expiring_7d
            FROM tenants
        ),
        user_counts AS (
            SELECT COUNT(*)::BIGINT AS tenant_end_users
            FROM users
            WHERE tenant_id != 'ten_platform' AND tenant_id IS NOT NULL
        ),
        billing_counts AS (
            SELECT
                COALESCE(
                    SUM((amount_cents)::BIGINT) FILTER (
                        WHERE status = 'paid'
                          AND created_at >= NOW() - INTERVAL '30 days'
                    ),
                    0
                )::BIGINT AS revenue_30d_cents,
                COUNT(*) FILTER (
                    WHERE status = 'paid'
                      AND created_at >= NOW() - INTERVAL '30 days'
                )::BIGINT AS paid_receipts_30d
            FROM provider_billing_events
        ),
        login_counts AS (
            SELECT COUNT(*) FILTER (
                WHERE tenant_id != 'ten_platform'
                  AND success = FALSE
                  AND created_at >= NOW() - INTERVAL '24 hours'
            )::BIGINT AS failed_logins_24h
            FROM login_attempts
        )
        SELECT
            cc.customers,
            cc.active_customers,
            cc.suspended_customers,
            cc.rejected_customers,
            tc.tenants,
            tc.active_tenants,
            tc.suspended_tenants,
            tc.trial_expiring_7d,
            uc.tenant_end_users,
            bc.revenue_30d_cents,
            bc.paid_receipts_30d,
            lc.failed_logins_24h
        FROM customer_counts cc
        CROSS JOIN tenant_counts tc
        CROSS JOIN user_counts uc
        CROSS JOIN billing_counts bc
        CROSS JOIN login_counts lc",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "customers": customers,
        "active_customers": active_customers,
        "suspended_customers": suspended_customers,
        "rejected_customers": rejected_customers,
        "tenants": tenants,
        "active_tenants": active_tenants,
        "suspended_tenants": suspended_tenants,
        "tenant_end_users": tenant_end_users,
        "trial_expiring_7d": trial_expiring_7d,
        "revenue_30d_cents": revenue_30d_cents,
        "paid_receipts_30d": paid_receipts_30d,
        "failed_logins_24h": failed_logins_24h,
    })))
}

/// GET /platform/admin/analytics — Time-series analytics for provider ops.
pub async fn owner_analytics(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let params: OwnerAnalyticsQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(OwnerAnalyticsQuery { days: None }))
        .0;
    let days = params.days.unwrap_or(30).clamp(7, 180);
    let window_start = Utc::now() - Duration::days(days);

    let signup_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT to_char(date_trunc('day', pa.created_at), 'YYYY-MM-DD') AS day, COUNT(*)
         FROM platform_accounts pa
         WHERE pa.created_at >= $1
         GROUP BY day
         ORDER BY day ASC",
    )
    .bind(window_start)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let paid_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT to_char(date_trunc('day', pbe.created_at), 'YYYY-MM-DD') AS day,
                COALESCE(SUM(pbe.amount_cents), 0)::BIGINT
         FROM provider_billing_events pbe
         WHERE pbe.status = 'paid'
           AND pbe.created_at >= $1
         GROUP BY day
         ORDER BY day ASC",
    )
    .bind(window_start)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let login_rows = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT to_char(date_trunc('day', la.created_at), 'YYYY-MM-DD') AS day,
                COUNT(*) FILTER (WHERE la.success = TRUE) AS success_count,
                COUNT(*) FILTER (WHERE la.success = FALSE) AS failure_count
         FROM login_attempts la
         WHERE la.tenant_id != 'ten_platform'
           AND la.created_at >= $1
         GROUP BY day
         ORDER BY day ASC",
    )
    .bind(window_start)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let top_categories = sqlx::query_as::<_, (String, i64)>(
        "SELECT COALESCE(NULLIF(pa.category, ''), 'standard') AS category, COUNT(*)
         FROM platform_accounts pa
         WHERE pa.status != 'deleted'
         GROUP BY category
         ORDER BY COUNT(*) DESC
         LIMIT 8",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let customer_signups: Vec<serde_json::Value> = signup_rows
        .into_iter()
        .map(|(day, count)| serde_json::json!({ "day": day, "count": count }))
        .collect();

    let revenue_paid_cents: Vec<serde_json::Value> = paid_rows
        .into_iter()
        .map(|(day, cents)| serde_json::json!({ "day": day, "amount_cents": cents }))
        .collect();

    let login_volume: Vec<serde_json::Value> = login_rows
        .into_iter()
        .map(|(day, success_count, failure_count)| {
            serde_json::json!({
                "day": day,
                "success_count": success_count,
                "failure_count": failure_count,
            })
        })
        .collect();

    let categories: Vec<serde_json::Value> = top_categories
        .into_iter()
        .map(|(category, count)| serde_json::json!({ "category": category, "count": count }))
        .collect();

    Ok(Json(serde_json::json!({
        "window_days": days,
        "window_start": window_start.to_rfc3339(),
        "customer_signups": customer_signups,
        "revenue_paid_cents": revenue_paid_cents,
        "login_volume": login_volume,
        "top_categories": categories,
    })))
}

/// GET /platform/admin/customers — List customer platform accounts.
pub async fn list_customers(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let params: ListCustomersQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(ListCustomersQuery {
            limit: None,
            offset: None,
            q: None,
            status: None,
            category: None,
            plan: None,
            created_from: None,
            created_to: None,
            sort: None,
        }))
        .0;

    let limit = clamp_limit(params.limit, 50, 250);
    let offset = params.offset.unwrap_or(0).max(0);

    let q_pattern = params.q.as_ref().and_then(|q| {
        let escaped = q
            .trim()
            .to_lowercase()
            .replace('%', "\\%")
            .replace('_', "\\_");
        if escaped.is_empty() {
            None
        } else {
            Some(format!("%{}%", escaped))
        }
    });
    let status_filter = normalize_optional_field(&params.status);
    let category_filter = normalize_optional_field(&params.category);
    let plan_filter = normalize_optional_field(&params.plan);
    let created_from = parse_rfc3339_opt(&params.created_from, "created_from")?;
    let created_to = parse_rfc3339_opt(&params.created_to, "created_to")?;
    let order_by = sort_clause(params.sort.as_deref());

    let list_sql = format!(
        "WITH filtered_customers AS (
            SELECT
                pa.user_id,
                u.email,
                u.name,
                pa.status,
                pa.company_name,
                pa.billing_ref,
                COALESCE(pa.category, 'standard') AS category,
                COALESCE(pa.risk_level, 'normal') AS risk_level,
                COALESCE(pa.lifecycle_stage, 'active') AS lifecycle_stage,
                pa.created_at
            FROM platform_accounts pa
            JOIN users u ON u.id = pa.user_id
            WHERE u.tenant_id = 'ten_platform'
              AND ($1::TEXT IS NULL OR u.email ILIKE $1 OR COALESCE(u.name,'') ILIKE $1 OR COALESCE(pa.company_name,'') ILIKE $1)
              AND ($2::TEXT IS NULL OR pa.status = $2)
              AND ($3::TEXT IS NULL OR COALESCE(pa.category, 'standard') = $3)
              AND ($4::TIMESTAMPTZ IS NULL OR pa.created_at >= $4)
              AND ($5::TIMESTAMPTZ IS NULL OR pa.created_at <= $5)
        ),
        tenant_agg AS (
            SELECT
                t.owner_user_id,
                COUNT(*) FILTER (WHERE t.status != 'deleted')::BIGINT AS tenant_count,
                COUNT(*) FILTER (
                    WHERE t.status = 'active'
                      AND COALESCE(t.plan, 'trial') = 'trial'
                      AND t.trial_ends_at IS NOT NULL
                      AND t.trial_ends_at <= NOW() + INTERVAL '7 days'
                )::BIGINT AS trial_expiring_tenants,
                BOOL_OR(COALESCE(t.plan, 'trial') = $6::TEXT) AS matches_plan
            FROM tenants t
            WHERE t.id != 'ten_platform'
            GROUP BY t.owner_user_id
        ),
        billing_agg AS (
            SELECT
                pbe.customer_user_id,
                COALESCE(
                    SUM((pbe.amount_cents)::BIGINT) FILTER (WHERE pbe.status = 'paid'),
                    0
                )::BIGINT AS lifetime_revenue_cents,
                COUNT(*) FILTER (
                    WHERE pbe.status = 'failed'
                      AND pbe.created_at >= NOW() - INTERVAL '30 days'
                )::BIGINT AS payment_failed_30d
            FROM provider_billing_events pbe
            GROUP BY pbe.customer_user_id
        ),
        login_agg AS (
            SELECT
                t.owner_user_id,
                COUNT(*) FILTER (
                    WHERE la.success = FALSE
                      AND la.created_at >= NOW() - INTERVAL '24 hours'
                )::BIGINT AS failed_logins_24h
            FROM login_attempts la
            JOIN tenants t ON t.id = la.tenant_id
            WHERE t.id != 'ten_platform'
            GROUP BY t.owner_user_id
        )
        SELECT
            fc.user_id,
            fc.email,
            fc.name,
            fc.status,
            fc.company_name,
            fc.billing_ref,
            fc.category,
            fc.risk_level,
            fc.lifecycle_stage,
            fc.created_at,
            COALESCE(ta.tenant_count, 0)::BIGINT AS tenant_count,
            COALESCE(ba.lifetime_revenue_cents, 0)::BIGINT AS lifetime_revenue_cents,
            COALESCE(ta.trial_expiring_tenants, 0)::BIGINT AS trial_expiring_tenants,
            COALESCE(la.failed_logins_24h, 0)::BIGINT AS failed_logins_24h,
            COALESCE(ba.payment_failed_30d, 0)::BIGINT AS payment_failed_30d
        FROM filtered_customers fc
        LEFT JOIN tenant_agg ta ON ta.owner_user_id = fc.user_id
        LEFT JOIN billing_agg ba ON ba.customer_user_id = fc.user_id
        LEFT JOIN login_agg la ON la.owner_user_id = fc.user_id
        WHERE ($6::TEXT IS NULL OR COALESCE(ta.matches_plan, FALSE))
        ORDER BY {}
        LIMIT $7 OFFSET $8",
        order_by
    );

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            DateTime<Utc>,
            i64,
            i64,
            i64,
            i64,
            i64,
        ),
    >(&list_sql)
    .bind(q_pattern.as_deref())
    .bind(status_filter.as_deref())
    .bind(category_filter.as_deref())
    .bind(created_from)
    .bind(created_to)
    .bind(plan_filter.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "WITH filtered_customers AS (
            SELECT
                pa.user_id
            FROM platform_accounts pa
            JOIN users u ON u.id = pa.user_id
            WHERE u.tenant_id = 'ten_platform'
              AND ($1::TEXT IS NULL OR u.email ILIKE $1 OR COALESCE(u.name,'') ILIKE $1 OR COALESCE(pa.company_name,'') ILIKE $1)
              AND ($2::TEXT IS NULL OR pa.status = $2)
              AND ($3::TEXT IS NULL OR COALESCE(pa.category, 'standard') = $3)
              AND ($4::TIMESTAMPTZ IS NULL OR pa.created_at >= $4)
              AND ($5::TIMESTAMPTZ IS NULL OR pa.created_at <= $5)
        ),
        tenant_agg AS (
            SELECT
                t.owner_user_id,
                BOOL_OR(COALESCE(t.plan, 'trial') = $6::TEXT) AS matches_plan
            FROM tenants t
            WHERE t.id != 'ten_platform'
            GROUP BY t.owner_user_id
        )
        SELECT COUNT(*)
        FROM filtered_customers fc
        LEFT JOIN tenant_agg ta ON ta.owner_user_id = fc.user_id
        WHERE ($6::TEXT IS NULL OR COALESCE(ta.matches_plan, FALSE))",
    )
    .bind(q_pattern.as_deref())
    .bind(status_filter.as_deref())
    .bind(category_filter.as_deref())
    .bind(created_from)
    .bind(created_to)
    .bind(plan_filter.as_deref())
    .fetch_one(&state.db)
    .await?;

    let customers: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                user_id,
                email,
                name,
                status,
                company_name,
                billing_ref,
                category,
                risk_level,
                lifecycle_stage,
                created_at,
                tenant_count,
                lifetime_revenue_cents,
                trial_expiring_tenants,
                failed_logins_24h,
                payment_failed_30d,
            )| {
                serde_json::json!({
                    "user_id": user_id,
                    "email": email,
                    "name": name,
                    "status": status,
                    "category": category,
                    "risk_level": risk_level,
                    "lifecycle_stage": lifecycle_stage,
                    "company_name": company_name,
                    "billing_ref": billing_ref,
                    "tenant_count": tenant_count,
                    "lifetime_revenue_cents": lifetime_revenue_cents,
                    "trial_expiring_tenants": trial_expiring_tenants,
                    "failed_logins_24h": failed_logins_24h,
                    "payment_failed_30d": payment_failed_30d,
                    "created_at": created_at.to_rfc3339(),
                })
            },
        )
        .collect();

    let status_facets = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*)
         FROM platform_accounts
         WHERE status != 'deleted'
         GROUP BY status
         ORDER BY COUNT(*) DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let category_facets = sqlx::query_as::<_, (String, i64)>(
        "SELECT COALESCE(NULLIF(category, ''), 'standard') AS category, COUNT(*)
         FROM platform_accounts
         WHERE status != 'deleted'
         GROUP BY category
         ORDER BY COUNT(*) DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let plan_facets = sqlx::query_as::<_, (String, i64)>(
        "SELECT COALESCE(plan, 'trial') AS plan, COUNT(*)
         FROM tenants
         WHERE id != 'ten_platform'
           AND status != 'deleted'
         GROUP BY plan
         ORDER BY COUNT(*) DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "customers": customers,
        "total": total,
        "limit": limit,
        "offset": offset,
        "facets": {
            "status": status_facets.into_iter().map(|(k, v)| serde_json::json!({"key": k, "count": v})).collect::<Vec<_>>(),
            "category": category_facets.into_iter().map(|(k, v)| serde_json::json!({"key": k, "count": v})).collect::<Vec<_>>(),
            "plan": plan_facets.into_iter().map(|(k, v)| serde_json::json!({"key": k, "count": v})).collect::<Vec<_>>(),
        }
    })))
}

/// GET /platform/admin/customers/{id} — Retrieve a customer profile.
pub async fn get_customer(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let customer = fetch_customer_snapshot(&state, &customer_id).await?;
    Ok(Json(serde_json::json!({ "customer": customer })))
}

/// GET /platform/admin/customers/{id}/tenants — List customer tenants.
pub async fn list_customer_tenants(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let tenants = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            i32,
            Option<chrono::DateTime<Utc>>,
            chrono::DateTime<Utc>,
            i64,
            i64,
            i64,
            i64,
            i64,
        ),
    >(
        "WITH scoped_tenants AS (
            SELECT
                t.id,
                t.name,
                t.slug,
                t.status,
                COALESCE(t.plan, 'trial') AS plan,
                t.max_users,
                t.trial_ends_at,
                t.created_at
            FROM tenants t
            WHERE t.owner_user_id = $1
              AND t.id != 'ten_platform'
              AND t.status != 'deleted'
        ),
        user_agg AS (
            SELECT u.tenant_id, COUNT(*)::BIGINT AS user_count
            FROM users u
            GROUP BY u.tenant_id
        ),
        session_agg AS (
            SELECT rt.tenant_id, COUNT(*)::BIGINT AS active_sessions
            FROM refresh_tokens rt
            WHERE rt.revoked = FALSE
              AND rt.expires_at > NOW()
            GROUP BY rt.tenant_id
        ),
        login_agg AS (
            SELECT
                la.tenant_id,
                COUNT(*) FILTER (
                    WHERE la.success = TRUE
                      AND la.created_at > NOW() - INTERVAL '24 hours'
                )::BIGINT AS logins_24h,
                COUNT(DISTINCT la.user_id) FILTER (
                    WHERE la.success = TRUE
                      AND la.created_at >= NOW() - INTERVAL '7 days'
                )::BIGINT AS active_users_7d
            FROM login_attempts la
            GROUP BY la.tenant_id
        ),
        billing_agg AS (
            SELECT
                pbe.tenant_id,
                COUNT(*) FILTER (
                    WHERE pbe.status = 'failed'
                      AND pbe.created_at >= NOW() - INTERVAL '30 days'
                )::BIGINT AS payment_failed_30d
            FROM provider_billing_events pbe
            WHERE pbe.tenant_id IS NOT NULL
            GROUP BY pbe.tenant_id
        )
        SELECT
            st.id,
            st.name,
            st.slug,
            st.status,
            st.plan,
            st.max_users,
            st.trial_ends_at,
            st.created_at,
            COALESCE(ua.user_count, 0)::BIGINT AS user_count,
            COALESCE(sa.active_sessions, 0)::BIGINT AS active_sessions,
            COALESCE(la.logins_24h, 0)::BIGINT AS logins_24h,
            COALESCE(la.active_users_7d, 0)::BIGINT AS active_users_7d,
            COALESCE(ba.payment_failed_30d, 0)::BIGINT AS payment_failed_30d
        FROM scoped_tenants st
        LEFT JOIN user_agg ua ON ua.tenant_id = st.id
        LEFT JOIN session_agg sa ON sa.tenant_id = st.id
        LEFT JOIN login_agg la ON la.tenant_id = st.id
        LEFT JOIN billing_agg ba ON ba.tenant_id = st.id
        ORDER BY st.created_at DESC",
    )
    .bind(&customer_id)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = tenants
        .into_iter()
        .map(
            |(
                id,
                name,
                slug,
                status,
                plan,
                max_users,
                trial_ends_at,
                created_at,
                user_count,
                active_sessions,
                logins_24h,
                active_users_7d,
                payment_failed_30d,
            )| {
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "slug": slug,
                    "status": status,
                    "plan": plan,
                    "max_users": max_users,
                    "trial_ends_at": trial_ends_at.map(|d| d.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                    "user_count": user_count,
                    "active_sessions": active_sessions,
                    "logins_24h": logins_24h,
                    "active_users_7d": active_users_7d,
                    "payment_failed_30d": payment_failed_30d,
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({
        "customer_id": customer_id,
        "tenants": items,
    })))
}

/// GET /platform/admin/tenants — Global tenant inventory for provider ops.
pub async fn list_tenants(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let params: ListTenantsQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(ListTenantsQuery {
            limit: None,
            offset: None,
            q: None,
            status: None,
            plan: None,
            owner_user_id: None,
        }))
        .0;

    let limit = clamp_limit(params.limit, 80, 250);
    let offset = params.offset.unwrap_or(0).max(0);
    let q_pattern = params.q.as_ref().and_then(|q| {
        let escaped = q
            .trim()
            .to_lowercase()
            .replace('%', "\\%")
            .replace('_', "\\_");
        if escaped.is_empty() {
            None
        } else {
            Some(format!("%{}%", escaped))
        }
    });
    let status_filter = normalize_optional_field(&params.status).map(|s| s.to_lowercase());
    let plan_filter = normalize_optional_field(&params.plan).map(|s| s.to_lowercase());
    let owner_filter = normalize_optional_field(&params.owner_user_id);

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<DateTime<Utc>>,
            DateTime<Utc>,
            i64,
            i64,
            Option<DateTime<Utc>>,
            i64,
            i64,
            i64,
        ),
    >(
        "WITH filtered_tenants AS (
            SELECT
                t.id,
                t.slug,
                COALESCE(t.name, t.slug) AS name,
                t.status,
                COALESCE(t.plan, 'trial') AS plan,
                t.owner_user_id,
                owner_u.email AS owner_email,
                pa.company_name,
                t.trial_ends_at,
                t.created_at
            FROM tenants t
            LEFT JOIN users owner_u ON owner_u.id = t.owner_user_id
            LEFT JOIN platform_accounts pa ON pa.user_id = t.owner_user_id
            WHERE t.id != 'ten_platform'
              AND t.status != 'deleted'
              AND ($1::TEXT IS NULL OR t.slug ILIKE $1 OR COALESCE(t.name, '') ILIKE $1 OR COALESCE(owner_u.email, '') ILIKE $1 OR COALESCE(pa.company_name, '') ILIKE $1)
              AND ($2::TEXT IS NULL OR t.status = $2)
              AND ($3::TEXT IS NULL OR COALESCE(t.plan, 'trial') = $3)
              AND ($4::TEXT IS NULL OR t.owner_user_id = $4)
        ),
        user_agg AS (
            SELECT u.tenant_id, COUNT(*)::BIGINT AS user_count
            FROM users u
            GROUP BY u.tenant_id
        ),
        session_agg AS (
            SELECT rt.tenant_id, COUNT(*)::BIGINT AS active_sessions
            FROM refresh_tokens rt
            WHERE rt.revoked = FALSE
              AND rt.expires_at > NOW()
            GROUP BY rt.tenant_id
        ),
        login_agg AS (
            SELECT
                la.tenant_id,
                MAX(la.created_at) FILTER (WHERE la.success = TRUE) AS last_login_at,
                COUNT(*) FILTER (
                    WHERE la.success = FALSE
                      AND la.created_at >= NOW() - INTERVAL '24 hours'
                )::BIGINT AS failed_logins_24h,
                COUNT(DISTINCT la.user_id) FILTER (
                    WHERE la.success = TRUE
                      AND la.created_at >= NOW() - INTERVAL '7 days'
                )::BIGINT AS active_users_7d
            FROM login_attempts la
            GROUP BY la.tenant_id
        ),
        billing_agg AS (
            SELECT
                pbe.tenant_id,
                COUNT(*) FILTER (
                    WHERE pbe.status = 'failed'
                      AND pbe.created_at >= NOW() - INTERVAL '30 days'
                )::BIGINT AS payment_failed_30d
            FROM provider_billing_events pbe
            WHERE pbe.tenant_id IS NOT NULL
            GROUP BY pbe.tenant_id
        )
        SELECT
            ft.id,
            ft.slug,
            ft.name,
            ft.status,
            ft.plan,
            ft.owner_user_id,
            ft.owner_email,
            ft.company_name,
            ft.trial_ends_at,
            ft.created_at,
            COALESCE(ua.user_count, 0)::BIGINT AS user_count,
            COALESCE(sa.active_sessions, 0)::BIGINT AS active_sessions,
            la.last_login_at,
            COALESCE(la.failed_logins_24h, 0)::BIGINT AS failed_logins_24h,
            COALESCE(la.active_users_7d, 0)::BIGINT AS active_users_7d,
            COALESCE(ba.payment_failed_30d, 0)::BIGINT AS payment_failed_30d
        FROM filtered_tenants ft
        LEFT JOIN user_agg ua ON ua.tenant_id = ft.id
        LEFT JOIN session_agg sa ON sa.tenant_id = ft.id
        LEFT JOIN login_agg la ON la.tenant_id = ft.id
        LEFT JOIN billing_agg ba ON ba.tenant_id = ft.id
        ORDER BY ft.created_at DESC
        LIMIT $5 OFFSET $6",
    )
    .bind(q_pattern.as_deref())
    .bind(status_filter.as_deref())
    .bind(plan_filter.as_deref())
    .bind(owner_filter.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "WITH filtered_tenants AS (
            SELECT t.id
            FROM tenants t
            LEFT JOIN users owner_u ON owner_u.id = t.owner_user_id
            LEFT JOIN platform_accounts pa ON pa.user_id = t.owner_user_id
            WHERE t.id != 'ten_platform'
              AND t.status != 'deleted'
              AND ($1::TEXT IS NULL OR t.slug ILIKE $1 OR COALESCE(t.name, '') ILIKE $1 OR COALESCE(owner_u.email, '') ILIKE $1 OR COALESCE(pa.company_name, '') ILIKE $1)
              AND ($2::TEXT IS NULL OR t.status = $2)
              AND ($3::TEXT IS NULL OR COALESCE(t.plan, 'trial') = $3)
              AND ($4::TEXT IS NULL OR t.owner_user_id = $4)
        )
        SELECT COUNT(*) FROM filtered_tenants",
    )
    .bind(q_pattern.as_deref())
    .bind(status_filter.as_deref())
    .bind(plan_filter.as_deref())
    .bind(owner_filter.as_deref())
    .fetch_one(&state.db)
    .await?;

    let status_facets = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*)
         FROM tenants
         WHERE id != 'ten_platform' AND status != 'deleted'
         GROUP BY status
         ORDER BY COUNT(*) DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let plan_facets = sqlx::query_as::<_, (String, i64)>(
        "SELECT COALESCE(plan, 'trial') AS plan, COUNT(*)
         FROM tenants
         WHERE id != 'ten_platform' AND status != 'deleted'
         GROUP BY plan
         ORDER BY COUNT(*) DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let tenants: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                slug,
                name,
                status,
                plan,
                owner_user_id,
                owner_email,
                owner_company,
                trial_ends_at,
                created_at,
                user_count,
                active_sessions,
                last_login_at,
                failed_logins_24h,
                active_users_7d,
                payment_failed_30d,
            )| {
                serde_json::json!({
                    "id": id,
                    "slug": slug,
                    "name": name,
                    "status": status,
                    "plan": plan,
                    "owner_user_id": owner_user_id,
                    "owner_email": owner_email,
                    "owner_company": owner_company,
                    "trial_ends_at": trial_ends_at.map(|d| d.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                    "user_count": user_count,
                    "active_sessions": active_sessions,
                    "last_login_at": last_login_at.map(|d| d.to_rfc3339()),
                    "failed_logins_24h": failed_logins_24h,
                    "active_users_7d": active_users_7d,
                    "payment_failed_30d": payment_failed_30d,
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({
        "tenants": tenants,
        "total": total,
        "limit": limit,
        "offset": offset,
        "facets": {
            "status": status_facets.into_iter().map(|(k, v)| serde_json::json!({"key": k, "count": v})).collect::<Vec<_>>(),
            "plan": plan_facets.into_iter().map(|(k, v)| serde_json::json!({"key": k, "count": v})).collect::<Vec<_>>(),
        }
    })))
}

/// GET /platform/admin/customers/{id}/insights
/// — Customer 360 data: profile, lifecycle history, auth events, and payment summary.
pub async fn customer_insights(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let customer = fetch_customer_snapshot(&state, &customer_id).await?;
    let customer_email = customer
        .get("email")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();

    let lifecycle_rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            String,
            Option<String>,
            serde_json::Value,
            DateTime<Utc>,
        ),
    >(
        "SELECT id, action, actor_user_id, actor_email, resource,
                COALESCE(request_id, id) AS request_id,
                tenant_id, details, created_at
         FROM owner_timeline_events
         WHERE customer_user_id = $1
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(&customer_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let lifecycle_history: Vec<serde_json::Value> = lifecycle_rows
        .into_iter()
        .map(
            |(
                id,
                action,
                actor_user_id,
                actor_email,
                resource,
                request_id,
                tenant_id,
                details,
                created_at,
            )| {
                serde_json::json!({
                    "id": id,
                    "action": action,
                    "actor_user_id": actor_user_id,
                    "actor_email": actor_email,
                    "resource": resource,
                    "request_id": request_id,
                    "tenant_id": tenant_id,
                    "details": details,
                    "created_at": created_at.to_rfc3339(),
                })
            },
        )
        .collect();

    let mut auth_events: Vec<(DateTime<Utc>, serde_json::Value)> = Vec::new();

    let login_rows = sqlx::query_as::<
        _,
        (
            String,
            bool,
            Option<String>,
            Option<String>,
            Option<String>,
            DateTime<Utc>,
        ),
    >(
        "SELECT id, success, ip_address, user_agent, failure_reason, created_at
         FROM login_attempts
         WHERE tenant_id = 'ten_platform'
           AND (user_id = $1 OR email = $2)
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(&customer_id)
    .bind(&customer_email)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for (id, success, ip_address, user_agent, failure_reason, created_at) in login_rows {
        auth_events.push((
            created_at,
            serde_json::json!({
                "id": id,
                "source": "login_attempt",
                "action": if success { "login.success" } else { "login.failure" },
                "result": if success { "success" } else { "failure" },
                "ip_address": ip_address,
                "user_agent": user_agent,
                "failure_reason": failure_reason,
                "created_at": created_at.to_rfc3339(),
            }),
        ));
    }

    let audit_rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            DateTime<Utc>,
        ),
    >(
        "SELECT event_id, action, resource, result, failure_reason, ip_address, user_agent, timestamp
         FROM audit_log
         WHERE tenant_id = 'ten_platform'
           AND user_id = $1
         ORDER BY timestamp DESC
         LIMIT 50",
    )
    .bind(&customer_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for (event_id, action, resource, result, failure_reason, ip_address, user_agent, timestamp) in
        audit_rows
    {
        auth_events.push((
            timestamp,
            serde_json::json!({
                "id": event_id,
                "source": "audit_log",
                "action": action,
                "resource": resource,
                "result": result,
                "failure_reason": failure_reason,
                "ip_address": ip_address,
                "user_agent": user_agent,
                "created_at": timestamp.to_rfc3339(),
            }),
        ));
    }

    auth_events.sort_by(|a, b| b.0.cmp(&a.0));
    let recent_auth_events: Vec<serde_json::Value> =
        auth_events.into_iter().take(60).map(|(_, ev)| ev).collect();

    let payment_summary = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        "SELECT
            COALESCE(SUM(amount_cents) FILTER (WHERE status = 'paid'), 0)::BIGINT AS paid_cents,
            COALESCE(SUM(amount_cents) FILTER (WHERE status = 'pending'), 0)::BIGINT AS pending_cents,
            COALESCE(SUM(amount_cents) FILTER (WHERE status = 'failed'), 0)::BIGINT AS failed_cents,
            COALESCE(SUM(amount_cents) FILTER (WHERE status = 'refunded'), 0)::BIGINT AS refunded_cents,
            COUNT(*)::BIGINT AS total_events
         FROM provider_billing_events
         WHERE customer_user_id = $1",
    )
    .bind(&customer_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0, 0, 0, 0));

    let recent_payment_rows = sqlx::query_as::<
        _,
        (
            String,
            Option<String>,
            i64,
            String,
            String,
            Option<String>,
            Option<String>,
            DateTime<Utc>,
        ),
    >(
        "SELECT id, tenant_id, amount_cents::BIGINT, currency, status, external_invoice_id, receipt_url, created_at
         FROM provider_billing_events
         WHERE customer_user_id = $1
         ORDER BY created_at DESC
         LIMIT 30",
    )
    .bind(&customer_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let recent_payments: Vec<serde_json::Value> = recent_payment_rows
        .into_iter()
        .map(
            |(
                id,
                tenant_id,
                amount_cents,
                currency,
                status,
                external_invoice_id,
                receipt_url,
                created_at,
            )| {
                serde_json::json!({
                    "id": id,
                    "tenant_id": tenant_id,
                    "amount_cents": amount_cents,
                    "currency": currency,
                    "status": status,
                    "external_invoice_id": external_invoice_id,
                    "receipt_url": receipt_url,
                    "created_at": created_at.to_rfc3339(),
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({
        "customer": customer,
        "lifecycle_history": lifecycle_history,
        "recent_auth_events": recent_auth_events,
        "payment_summary": {
            "paid_cents": payment_summary.0,
            "pending_cents": payment_summary.1,
            "failed_cents": payment_summary.2,
            "refunded_cents": payment_summary.3,
            "total_events": payment_summary.4,
        },
        "recent_payments": recent_payments,
    })))
}

/// GET /platform/admin/timeline — Provider audit timeline with filters.
pub async fn owner_timeline(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let params: OwnerTimelineQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(OwnerTimelineQuery {
            limit: None,
            offset: None,
            actor: None,
            action: None,
            resource: None,
            request_id: None,
            from: None,
            to: None,
        }))
        .0;

    let limit = clamp_limit(params.limit, 50, 250);
    let offset = params.offset.unwrap_or(0).max(0);
    let actor_filter = normalize_optional_field(&params.actor);
    let actor_like = actor_filter.as_ref().map(|s| {
        format!(
            "%{}%",
            s.to_lowercase().replace('%', "\\%").replace('_', "\\_")
        )
    });
    let action_filter = normalize_optional_field(&params.action);
    let resource_filter = normalize_optional_field(&params.resource);
    let request_filter = normalize_optional_field(&params.request_id).map(|s| {
        format!(
            "%{}%",
            s.to_lowercase().replace('%', "\\%").replace('_', "\\_")
        )
    });
    let from = parse_rfc3339_opt(&params.from, "from")?;
    let to = parse_rfc3339_opt(&params.to, "to")?;

    let rows = sqlx::query_as::<
        _,
        (
            String,
            DateTime<Utc>,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            serde_json::Value,
            String,
            Option<String>,
        ),
    >(
        "SELECT ev.event_id, ev.created_at, ev.actor_user_id, ev.actor_email, ev.action, ev.resource,
                ev.request_id, ev.details, ev.source, ev.result
         FROM (
            SELECT oe.id AS event_id,
                   oe.created_at,
                   oe.actor_user_id,
                   oe.actor_email,
                   oe.action,
                   oe.resource,
                   COALESCE(oe.request_id, oe.id) AS request_id,
                   oe.details,
                   'owner_event'::TEXT AS source,
                   NULL::TEXT AS result
              FROM owner_timeline_events oe
            UNION ALL
            SELECT al.event_id,
                   al.timestamp AS created_at,
                   al.user_id AS actor_user_id,
                   u.email AS actor_email,
                   al.action,
                   al.resource,
                   al.event_id AS request_id,
                   jsonb_build_object(
                     'ip_address', al.ip_address,
                     'user_agent', al.user_agent,
                     'failure_reason', al.failure_reason,
                     'compliance_mode', al.compliance_mode
                   ) AS details,
                   'audit_log'::TEXT AS source,
                   al.result
              FROM audit_log al
              LEFT JOIN users u ON u.id = al.user_id
         ) ev
         WHERE ($1::TEXT IS NULL OR ev.actor_user_id = $1 OR COALESCE(ev.actor_email, '') ILIKE $2)
           AND ($3::TEXT IS NULL OR ev.action = $3)
           AND ($4::TEXT IS NULL OR ev.resource = $4)
           AND ($5::TEXT IS NULL OR ev.request_id ILIKE $5)
           AND ($6::TIMESTAMPTZ IS NULL OR ev.created_at >= $6)
           AND ($7::TIMESTAMPTZ IS NULL OR ev.created_at <= $7)
         ORDER BY ev.created_at DESC
         LIMIT $8 OFFSET $9",
    )
    .bind(actor_filter.as_deref())
    .bind(actor_like.as_deref())
    .bind(action_filter.as_deref())
    .bind(resource_filter.as_deref())
    .bind(request_filter.as_deref())
    .bind(from)
    .bind(to)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM (
            SELECT oe.id AS event_id,
                   oe.created_at,
                   oe.actor_user_id,
                   oe.actor_email,
                   oe.action,
                   oe.resource,
                   COALESCE(oe.request_id, oe.id) AS request_id
              FROM owner_timeline_events oe
            UNION ALL
            SELECT al.event_id,
                   al.timestamp AS created_at,
                   al.user_id AS actor_user_id,
                   u.email AS actor_email,
                   al.action,
                   al.resource,
                   al.event_id AS request_id
              FROM audit_log al
              LEFT JOIN users u ON u.id = al.user_id
         ) ev
         WHERE ($1::TEXT IS NULL OR ev.actor_user_id = $1 OR COALESCE(ev.actor_email, '') ILIKE $2)
           AND ($3::TEXT IS NULL OR ev.action = $3)
           AND ($4::TEXT IS NULL OR ev.resource = $4)
           AND ($5::TEXT IS NULL OR ev.request_id ILIKE $5)
           AND ($6::TIMESTAMPTZ IS NULL OR ev.created_at >= $6)
           AND ($7::TIMESTAMPTZ IS NULL OR ev.created_at <= $7)",
    )
    .bind(actor_filter.as_deref())
    .bind(actor_like.as_deref())
    .bind(action_filter.as_deref())
    .bind(resource_filter.as_deref())
    .bind(request_filter.as_deref())
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let events: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                event_id,
                created_at,
                actor_user_id,
                actor_email,
                action,
                resource,
                request_id,
                details,
                source,
                result,
            )| {
                serde_json::json!({
                    "event_id": event_id,
                    "created_at": created_at.to_rfc3339(),
                    "actor_user_id": actor_user_id,
                    "actor_email": actor_email,
                    "action": action,
                    "resource": resource,
                    "request_id": request_id,
                    "details": details,
                    "source": source,
                    "result": result,
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({
        "events": events,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

/// PATCH /platform/admin/customers/{id}/lifecycle
/// — Update customer lifecycle metadata and optionally apply access changes to all their tenants.
pub async fn update_customer_lifecycle(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = owner_claims(&req)?;
    let request_id = request_id_from_req(&req);
    let body: UpdateCustomerLifecycleRequest =
        axum::Json::<UpdateCustomerLifecycleRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body".into(),
            })?
            .0;

    let status = normalize_optional_field(&body.status).map(|s| s.to_lowercase());
    let category = normalize_optional_field(&body.category).map(|s| s.to_lowercase());
    let company_name = normalize_optional_field(&body.company_name);
    let billing_ref = normalize_optional_field(&body.billing_ref);
    let notes = normalize_optional_field(&body.notes);
    let risk_level = normalize_optional_field(&body.risk_level).map(|s| s.to_lowercase());
    let lifecycle_stage = normalize_optional_field(&body.lifecycle_stage).map(|s| s.to_lowercase());
    let blocked_reason = normalize_optional_field(&body.blocked_reason);
    let clear_blocked_reason = body.blocked_reason.is_some() && blocked_reason.is_none();
    let apply_to_tenants = body.apply_to_tenants.unwrap_or(true);

    if let Some(ref next_status) = status {
        if !["active", "suspended", "rejected"].contains(&next_status.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_status",
                message: "Status must be one of: active, suspended, rejected".into(),
            });
        }
    }
    if let Some(ref next_category) = category {
        if !["standard", "education", "agency", "enterprise"].contains(&next_category.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_category",
                message: "Category must be one of: standard, education, agency, enterprise".into(),
            });
        }
    }
    if let Some(ref level) = risk_level {
        if !["low", "normal", "high"].contains(&level.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_risk_level",
                message: "Risk level must be one of: low, normal, high".into(),
            });
        }
    }
    if let Some(ref stage) = lifecycle_stage {
        if !["lead", "onboarding", "active", "at_risk", "churned"].contains(&stage.as_str()) {
            return Err(AppError::BadRequest {
                error: "invalid_lifecycle_stage",
                message:
                    "Lifecycle stage must be one of: lead, onboarding, active, at_risk, churned"
                        .into(),
            });
        }
    }

    let exists: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM platform_accounts WHERE user_id = $1")
            .bind(&customer_id)
            .fetch_optional(&state.db)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "customer_not_found",
            message: "Customer not found".into(),
        });
    }

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE platform_accounts
         SET status = COALESCE($1, status),
             category = COALESCE($2, category),
             company_name = COALESCE($3, company_name),
             billing_ref = COALESCE($4, billing_ref),
             notes = COALESCE($5, notes),
             risk_level = COALESCE($6, risk_level),
             lifecycle_stage = COALESCE($7, lifecycle_stage),
             blocked_reason = CASE WHEN $8 THEN NULL ELSE COALESCE($9, blocked_reason) END,
             last_contacted_at = NOW(),
             updated_at = NOW()
         WHERE user_id = $10",
    )
    .bind(status.as_deref())
    .bind(category.as_deref())
    .bind(company_name.as_deref())
    .bind(billing_ref.as_deref())
    .bind(notes.as_deref())
    .bind(risk_level.as_deref())
    .bind(lifecycle_stage.as_deref())
    .bind(clear_blocked_reason)
    .bind(blocked_reason.as_deref())
    .bind(&customer_id)
    .execute(&mut *tx)
    .await?;

    let mut affected_tenants = 0_u64;
    let tenant_slugs: Vec<String> = if apply_to_tenants {
        sqlx::query_scalar(
            "SELECT slug
             FROM tenants
             WHERE owner_user_id = $1
               AND id != 'ten_platform'
               AND status != 'deleted'",
        )
        .bind(&customer_id)
        .fetch_all(&mut *tx)
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    if apply_to_tenants {
        if matches!(status.as_deref(), Some("suspended" | "rejected")) {
            affected_tenants = sqlx::query(
                "UPDATE tenants
                 SET status = 'suspended', updated_at = NOW()
                 WHERE owner_user_id = $1
                   AND id != 'ten_platform'
                   AND status IN ('active', 'suspended')",
            )
            .bind(&customer_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        } else if matches!(status.as_deref(), Some("active")) {
            affected_tenants = sqlx::query(
                "UPDATE tenants
                 SET status = 'active', updated_at = NOW()
                 WHERE owner_user_id = $1
                   AND id != 'ten_platform'
                   AND status = 'suspended'",
            )
            .bind(&customer_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
    }

    tx.commit().await?;
    for slug in tenant_slugs {
        state.tenant_cache.invalidate(&slug);
    }

    if let Err(error) = record_owner_event(
        &state,
        OwnerEvent {
            customer_user_id: Some(&customer_id),
            tenant_id: None,
            actor_user_id: Some(&claims.sub),
            actor_email: Some(claims.email.as_str()),
            action: "customer.lifecycle.updated",
            resource: "platform_account",
            request_id: request_id.as_deref(),
            details: serde_json::json!({
                "status": status,
                "category": category,
                "company_name": company_name,
                "billing_ref": billing_ref,
                "risk_level": risk_level,
                "lifecycle_stage": lifecycle_stage,
                "blocked_reason": blocked_reason,
                "apply_to_tenants": apply_to_tenants,
                "affected_tenants": affected_tenants,
            }),
        },
    )
    .await
    {
        tracing::warn!(customer_id = %customer_id, %error, "Failed to record owner timeline event");
    }

    let customer = fetch_customer_snapshot(&state, &customer_id).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "affected_tenants": affected_tenants,
        "customer": customer,
    })))
}

/// POST /platform/admin/customers/{id}/extend-access
/// — Extend trial access and optionally reactivate suspended tenant(s).
pub async fn extend_customer_access(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = owner_claims(&req)?;
    let request_id = request_id_from_req(&req);
    let body: ExtendAccessRequest = axum::Json::<ExtendAccessRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body. Required: days".into(),
        })?
        .0;

    if body.days < 1 || body.days > 365 {
        return Err(AppError::BadRequest {
            error: "invalid_days",
            message: "days must be between 1 and 365".into(),
        });
    }

    let exists: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM platform_accounts WHERE user_id = $1")
            .bind(&customer_id)
            .fetch_optional(&state.db)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "customer_not_found",
            message: "Customer not found".into(),
        });
    }

    let target_tenant_ids: Option<Vec<String>> = body
        .tenant_ids
        .as_ref()
        .map(|ids| {
            ids.iter()
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty())
                .collect()
        })
        .filter(|ids: &Vec<String>| !ids.is_empty());

    let tenant_slugs: Vec<String> = if let Some(ids) = target_tenant_ids.as_ref() {
        sqlx::query_scalar(
            "SELECT slug
             FROM tenants
             WHERE owner_user_id = $1
               AND id = ANY($2)
               AND id != 'ten_platform'
               AND status != 'deleted'",
        )
        .bind(&customer_id)
        .bind(ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_scalar(
            "SELECT slug
             FROM tenants
             WHERE owner_user_id = $1
               AND id != 'ten_platform'
               AND status != 'deleted'",
        )
        .bind(&customer_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    };

    let affected = if let Some(ids) = target_tenant_ids.as_ref() {
        sqlx::query(
            "UPDATE tenants
             SET trial_ends_at = GREATEST(COALESCE(trial_ends_at, NOW()), NOW()) + make_interval(days => $1::INT),
                 status = CASE WHEN status = 'suspended' THEN 'active' ELSE status END,
                 updated_at = NOW()
             WHERE owner_user_id = $2
               AND id = ANY($3)
               AND id != 'ten_platform'
               AND status != 'deleted'",
        )
        .bind(body.days as i32)
        .bind(&customer_id)
        .bind(ids)
        .execute(&state.db)
        .await?
        .rows_affected()
    } else {
        sqlx::query(
            "UPDATE tenants
             SET trial_ends_at = GREATEST(COALESCE(trial_ends_at, NOW()), NOW()) + make_interval(days => $1::INT),
                 status = CASE WHEN status = 'suspended' THEN 'active' ELSE status END,
                 updated_at = NOW()
             WHERE owner_user_id = $2
               AND id != 'ten_platform'
               AND status != 'deleted'",
        )
        .bind(body.days as i32)
        .bind(&customer_id)
        .execute(&state.db)
        .await?
        .rows_affected()
    };

    for slug in tenant_slugs {
        state.tenant_cache.invalidate(&slug);
    }

    if let Err(error) = record_owner_event(
        &state,
        OwnerEvent {
            customer_user_id: Some(&customer_id),
            tenant_id: None,
            actor_user_id: Some(&claims.sub),
            actor_email: Some(claims.email.as_str()),
            action: "customer.access.extended",
            resource: "tenant",
            request_id: request_id.as_deref(),
            details: serde_json::json!({
                "days_added": body.days,
                "target_tenant_ids": target_tenant_ids,
                "affected_tenants": affected,
            }),
        },
    )
    .await
    {
        tracing::warn!(customer_id = %customer_id, %error, "Failed to record owner timeline event");
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "days_added": body.days,
        "affected_tenants": affected,
    })))
}

/// POST /platform/admin/customers/{id}/tenants/{tenant_id}/status
/// — Toggle one tenant status from the provider dashboard.
pub async fn update_customer_tenant_status(
    State(state): State<AppState>,
    Path((customer_id, tenant_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = owner_claims(&req)?;
    let request_id = request_id_from_req(&req);
    let body: UpdateCustomerTenantStatusRequest =
        axum::Json::<UpdateCustomerTenantStatusRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: status".into(),
            })?
            .0;

    let next_status = body.status.trim().to_lowercase();
    if !["active", "suspended"].contains(&next_status.as_str()) {
        return Err(AppError::BadRequest {
            error: "invalid_status",
            message: "Status must be one of: active, suspended".into(),
        });
    }

    let slug: Option<String> = sqlx::query_scalar(
        "SELECT slug
         FROM tenants
         WHERE id = $1
           AND owner_user_id = $2
           AND id != 'ten_platform'
           AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&customer_id)
    .fetch_optional(&state.db)
    .await?;

    let slug = slug.ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found for customer".into(),
    })?;

    sqlx::query("UPDATE tenants SET status = $1, updated_at = NOW() WHERE id = $2")
        .bind(&next_status)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    state.tenant_cache.invalidate(&slug);

    if let Err(error) = record_owner_event(
        &state,
        OwnerEvent {
            customer_user_id: Some(&customer_id),
            tenant_id: Some(&tenant_id),
            actor_user_id: Some(&claims.sub),
            actor_email: Some(claims.email.as_str()),
            action: "tenant.status.updated",
            resource: "tenant",
            request_id: request_id.as_deref(),
            details: serde_json::json!({
                "tenant_slug": slug,
                "status": &next_status,
            }),
        },
    )
    .await
    {
        tracing::warn!(customer_id = %customer_id, tenant_id = %tenant_id, %error, "Failed to record owner timeline event");
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "tenant_id": tenant_id,
        "status": next_status,
    })))
}

/// DELETE /platform/admin/customers/{id}/tenants/{tenant_id}
/// — Soft-delete one customer tenant with explicit slug confirmation.
pub async fn delete_customer_tenant(
    State(state): State<AppState>,
    Path((customer_id, tenant_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = owner_claims(&req)?;
    let request_id = request_id_from_req(&req);
    let body: DeleteCustomerTenantRequest =
        axum::Json::<DeleteCustomerTenantRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: confirm_slug".into(),
            })?
            .0;

    let confirm_slug = body.confirm_slug.trim();
    if confirm_slug.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_request",
            message: "confirm_slug is required".into(),
        });
    }

    let slug: Option<String> = sqlx::query_scalar(
        "SELECT slug
         FROM tenants
         WHERE id = $1
           AND owner_user_id = $2
           AND id != 'ten_platform'
           AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&customer_id)
    .fetch_optional(&state.db)
    .await?;

    let slug = slug.ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found for customer".into(),
    })?;

    if confirm_slug != slug {
        return Err(AppError::BadRequest {
            error: "confirmation_mismatch",
            message: format!("Confirmation slug must match '{}'", slug),
        });
    }

    sqlx::query("UPDATE tenants SET status = 'deleted', updated_at = NOW() WHERE id = $1")
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    let revoked_refresh_tokens = sqlx::query(
        "UPDATE refresh_tokens
         SET revoked = TRUE
         WHERE tenant_id = $1
           AND revoked = FALSE",
    )
    .bind(&tenant_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    state.tenant_cache.invalidate(&slug);

    if let Err(error) = record_owner_event(
        &state,
        OwnerEvent {
            customer_user_id: Some(&customer_id),
            tenant_id: Some(&tenant_id),
            actor_user_id: Some(&claims.sub),
            actor_email: Some(claims.email.as_str()),
            action: "tenant.deleted",
            resource: "tenant",
            request_id: request_id.as_deref(),
            details: serde_json::json!({
                "tenant_slug": slug,
                "status": "deleted",
                "revoked_refresh_tokens": revoked_refresh_tokens,
            }),
        },
    )
    .await
    {
        tracing::warn!(customer_id = %customer_id, tenant_id = %tenant_id, %error, "Failed to record owner timeline event");
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "tenant_id": tenant_id,
        "slug": slug,
        "status": "deleted",
        "revoked_refresh_tokens": revoked_refresh_tokens,
    })))
}

/// GET /platform/admin/payments — Provider billing events and receipts.
pub async fn list_payments(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let params: ListPaymentsQuery = Query::try_from_uri(req.uri())
        .unwrap_or(Query(ListPaymentsQuery {
            limit: None,
            offset: None,
            status: None,
            customer_user_id: None,
            from: None,
            to: None,
        }))
        .0;

    let limit = clamp_limit(params.limit, 50, 250);
    let offset = params.offset.unwrap_or(0).max(0);
    let status_filter = normalize_optional_field(&params.status).map(|s| s.to_lowercase());
    let customer_filter = normalize_optional_field(&params.customer_user_id);
    let from = parse_rfc3339_opt(&params.from, "from")?;
    let to = parse_rfc3339_opt(&params.to, "to")?;

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<DateTime<Utc>>,
            Option<DateTime<Utc>>,
            DateTime<Utc>,
        ),
    >(
        "SELECT pbe.id, pbe.customer_user_id, u.email, u.name, pbe.tenant_id, t.slug,
                pbe.amount_cents::BIGINT, pbe.currency, pbe.status, pbe.external_invoice_id,
                pbe.receipt_url, pbe.period_start, pbe.period_end, pbe.created_at
         FROM provider_billing_events pbe
         JOIN users u ON u.id = pbe.customer_user_id
         LEFT JOIN tenants t ON t.id = pbe.tenant_id
         WHERE u.tenant_id = 'ten_platform'
           AND ($1::TEXT IS NULL OR pbe.customer_user_id = $1)
           AND ($2::TEXT IS NULL OR pbe.status = $2)
           AND ($3::TIMESTAMPTZ IS NULL OR pbe.created_at >= $3)
           AND ($4::TIMESTAMPTZ IS NULL OR pbe.created_at <= $4)
         ORDER BY pbe.created_at DESC
         LIMIT $5 OFFSET $6",
    )
    .bind(customer_filter.as_deref())
    .bind(status_filter.as_deref())
    .bind(from)
    .bind(to)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM provider_billing_events pbe
         WHERE ($1::TEXT IS NULL OR pbe.customer_user_id = $1)
           AND ($2::TEXT IS NULL OR pbe.status = $2)
           AND ($3::TIMESTAMPTZ IS NULL OR pbe.created_at >= $3)
           AND ($4::TIMESTAMPTZ IS NULL OR pbe.created_at <= $4)",
    )
    .bind(customer_filter.as_deref())
    .bind(status_filter.as_deref())
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let summary = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        "SELECT
            COALESCE(SUM(amount_cents) FILTER (WHERE status = 'paid'), 0)::BIGINT AS paid_cents,
            COALESCE(SUM(amount_cents) FILTER (WHERE status = 'pending'), 0)::BIGINT AS pending_cents,
            COALESCE(SUM(amount_cents) FILTER (WHERE status = 'refunded'), 0)::BIGINT AS refunded_cents,
            COUNT(*) FILTER (WHERE status = 'paid') AS paid_count,
            COUNT(*) FILTER (WHERE status = 'failed') AS failed_count
         FROM provider_billing_events pbe
         WHERE ($1::TEXT IS NULL OR pbe.customer_user_id = $1)
           AND ($2::TEXT IS NULL OR pbe.status = $2)
           AND ($3::TIMESTAMPTZ IS NULL OR pbe.created_at >= $3)
           AND ($4::TIMESTAMPTZ IS NULL OR pbe.created_at <= $4)",
    )
    .bind(customer_filter.as_deref())
    .bind(status_filter.as_deref())
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0, 0, 0, 0));

    let payments: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                customer_user_id,
                customer_email,
                customer_name,
                tenant_id,
                tenant_slug,
                amount_cents,
                currency,
                status,
                external_invoice_id,
                receipt_url,
                period_start,
                period_end,
                created_at,
            )| {
                serde_json::json!({
                    "id": id,
                    "customer_user_id": customer_user_id,
                    "customer_email": customer_email,
                    "customer_name": customer_name,
                    "tenant_id": tenant_id,
                    "tenant_slug": tenant_slug,
                    "amount_cents": amount_cents,
                    "currency": currency,
                    "status": status,
                    "external_invoice_id": external_invoice_id,
                    "receipt_url": receipt_url,
                    "period_start": period_start.map(|d| d.to_rfc3339()),
                    "period_end": period_end.map(|d| d.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({
        "payments": payments,
        "total": total,
        "limit": limit,
        "offset": offset,
        "summary": {
            "paid_cents": summary.0,
            "pending_cents": summary.1,
            "refunded_cents": summary.2,
            "paid_count": summary.3,
            "failed_count": summary.4,
        }
    })))
}

/// POST /platform/admin/payments — Record a payment/receipt event.
pub async fn create_payment(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = owner_claims(&req)?;
    let request_id = request_id_from_req(&req);
    let body: CreatePaymentEventRequest =
        axum::Json::<CreatePaymentEventRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid payment payload".into(),
            })?
            .0;

    if body.amount_cents <= 0 {
        return Err(AppError::BadRequest {
            error: "invalid_amount",
            message: "amount_cents must be greater than zero".into(),
        });
    }

    let customer_user_id = body.customer_user_id.trim().to_string();
    if customer_user_id.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_customer",
            message: "customer_user_id is required".into(),
        });
    }

    let customer_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM platform_accounts pa
            JOIN users u ON u.id = pa.user_id
            WHERE pa.user_id = $1 AND u.tenant_id = 'ten_platform'
        )",
    )
    .bind(&customer_user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);
    if !customer_exists {
        return Err(AppError::NotFound {
            error: "customer_not_found",
            message: "Customer not found".into(),
        });
    }

    let tenant_id = normalize_optional_field(&body.tenant_id);
    if let Some(ref tid) = tenant_id {
        let valid_tenant: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1
                FROM tenants
                WHERE id = $1
                  AND owner_user_id = $2
                  AND status != 'deleted'
            )",
        )
        .bind(tid)
        .bind(&customer_user_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);
        if !valid_tenant {
            return Err(AppError::BadRequest {
                error: "invalid_tenant",
                message: "tenant_id does not belong to the provided customer".into(),
            });
        }
    }

    let status = body
        .status
        .as_deref()
        .unwrap_or("paid")
        .trim()
        .to_lowercase();
    if !["pending", "paid", "failed", "refunded"].contains(&status.as_str()) {
        return Err(AppError::BadRequest {
            error: "invalid_payment_status",
            message: "status must be one of: pending, paid, failed, refunded".into(),
        });
    }

    let period_start = parse_rfc3339_opt(&body.period_start, "period_start")?;
    let period_end = parse_rfc3339_opt(&body.period_end, "period_end")?;
    let currency = body
        .currency
        .as_deref()
        .unwrap_or("usd")
        .trim()
        .to_lowercase();
    let external_invoice_id = normalize_optional_field(&body.external_invoice_id);
    let receipt_url = normalize_optional_field(&body.receipt_url);
    let notes = normalize_optional_field(&body.notes);
    let metadata = body
        .metadata
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));

    let payment_id = format!("pay_{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO provider_billing_events
         (id, customer_user_id, tenant_id, amount_cents, currency, status,
          external_invoice_id, receipt_url, period_start, period_end, notes,
          metadata, created_by_user_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), NOW())",
    )
    .bind(&payment_id)
    .bind(&customer_user_id)
    .bind(tenant_id.as_deref())
    .bind(body.amount_cents)
    .bind(&currency)
    .bind(&status)
    .bind(external_invoice_id.as_deref())
    .bind(receipt_url.as_deref())
    .bind(period_start)
    .bind(period_end)
    .bind(notes.as_deref())
    .bind(metadata)
    .bind(&claims.sub)
    .execute(&state.db)
    .await?;

    if let Err(error) = record_owner_event(
        &state,
        OwnerEvent {
            customer_user_id: Some(&customer_user_id),
            tenant_id: tenant_id.as_deref(),
            actor_user_id: Some(&claims.sub),
            actor_email: Some(claims.email.as_str()),
            action: "billing.payment.recorded",
            resource: "provider_billing_event",
            request_id: request_id.as_deref(),
            details: serde_json::json!({
                "payment_id": &payment_id,
                "amount_cents": body.amount_cents,
                "currency": &currency,
                "status": &status,
                "external_invoice_id": &external_invoice_id,
            }),
        },
    )
    .await
    {
        tracing::warn!(customer_user_id = %customer_user_id, payment_id = %payment_id, %error, "Failed to record owner timeline event");
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "payment": {
                "id": payment_id,
                "customer_user_id": customer_user_id,
                "tenant_id": tenant_id,
                "amount_cents": body.amount_cents,
                "currency": currency,
                "status": status,
                "external_invoice_id": external_invoice_id,
                "receipt_url": receipt_url,
                "period_start": period_start.map(|d| d.to_rfc3339()),
                "period_end": period_end.map(|d| d.to_rfc3339()),
                "notes": notes,
            }
        })),
    ))
}

async fn fetch_customer_snapshot(
    state: &AppState,
    customer_id: &str,
) -> Result<serde_json::Value, AppError> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<DateTime<Utc>>,
            DateTime<Utc>,
            DateTime<Utc>,
        ),
    >(
        "SELECT pa.user_id, u.email, u.name, pa.status,
                COALESCE(pa.category, 'standard') AS category,
                COALESCE(pa.risk_level, 'normal') AS risk_level,
                pa.company_name, pa.billing_ref, pa.notes,
                COALESCE(pa.lifecycle_stage, 'active') AS lifecycle_stage,
                pa.blocked_reason, pa.last_contacted_at, pa.created_at, pa.updated_at
         FROM platform_accounts pa
         JOIN users u ON u.id = pa.user_id
         WHERE pa.user_id = $1
           AND u.tenant_id = 'ten_platform'",
    )
    .bind(customer_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "customer_not_found",
        message: "Customer not found".into(),
    })?;

    let tenant_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM tenants
         WHERE owner_user_id = $1
           AND id != 'ten_platform'
           AND status != 'deleted'",
    )
    .bind(customer_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let lifetime_revenue_cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_cents), 0)::BIGINT
         FROM provider_billing_events
         WHERE customer_user_id = $1
           AND status = 'paid'",
    )
    .bind(customer_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let last_payment_at: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT MAX(created_at)
         FROM provider_billing_events
         WHERE customer_user_id = $1",
    )
    .bind(customer_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let (
        user_id,
        email,
        name,
        status,
        category,
        risk_level,
        company_name,
        billing_ref,
        notes,
        lifecycle_stage,
        blocked_reason,
        last_contacted_at,
        created_at,
        updated_at,
    ) = row;

    Ok(serde_json::json!({
        "user_id": user_id,
        "email": email,
        "name": name,
        "status": status,
        "category": category,
        "risk_level": risk_level,
        "lifecycle_stage": lifecycle_stage,
        "company_name": company_name,
        "billing_ref": billing_ref,
        "notes": notes,
        "blocked_reason": blocked_reason,
        "last_contacted_at": last_contacted_at.map(|d| d.to_rfc3339()),
        "tenant_count": tenant_count,
        "lifetime_revenue_cents": lifetime_revenue_cents,
        "last_payment_at": last_payment_at.map(|d| d.to_rfc3339()),
        "created_at": created_at.to_rfc3339(),
        "updated_at": updated_at.to_rfc3339(),
    }))
}

/// GET /platform/admin/invites — List provider-owner invites.
pub async fn list_owner_invites(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let _claims = owner_claims(&req)?;

    // Keep statuses accurate for read paths.
    let _ = sqlx::query(
        "UPDATE owner_invites
         SET status = 'expired', updated_at = NOW()
         WHERE status = 'pending' AND expires_at < NOW()",
    )
    .execute(&state.db)
    .await;

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            chrono::DateTime<Utc>,
            Option<chrono::DateTime<Utc>>,
            Option<chrono::DateTime<Utc>>,
            chrono::DateTime<Utc>,
            chrono::DateTime<Utc>,
        ),
    >(
        "SELECT oi.id, oi.email, oi.status, oi.invited_by_user_id, oi.accepted_by_user_id,
                oi.expires_at, oi.accepted_at, oi.revoked_at, oi.created_at, oi.updated_at
         FROM owner_invites oi
         ORDER BY oi.created_at DESC
         LIMIT 200",
    )
    .fetch_all(&state.db)
    .await?;

    let invites: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                email,
                status,
                invited_by_user_id,
                accepted_by_user_id,
                expires_at,
                accepted_at,
                revoked_at,
                created_at,
                updated_at,
            )| {
                serde_json::json!({
                    "id": id,
                    "email": email,
                    "status": status,
                    "invited_by_user_id": invited_by_user_id,
                    "accepted_by_user_id": accepted_by_user_id,
                    "expires_at": expires_at.to_rfc3339(),
                    "accepted_at": accepted_at.map(|v| v.to_rfc3339()),
                    "revoked_at": revoked_at.map(|v| v.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                    "updated_at": updated_at.to_rfc3339(),
                })
            },
        )
        .collect();

    Ok(Json(serde_json::json!({ "invites": invites })))
}

/// POST /platform/admin/invites — Create provider-owner invite.
pub async fn create_owner_invite(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = owner_claims(&req)?;
    let body: CreateOwnerInviteRequest =
        axum::Json::<CreateOwnerInviteRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: email".into(),
            })?
            .0;

    let email = normalize_email(&body.email);
    if email.is_empty() || !crate::handlers::auth::is_valid_email(&email) {
        return Err(AppError::BadRequest {
            error: "invalid_email",
            message: "A valid email is required".into(),
        });
    }

    let expires_in = body.expires_in_hours.unwrap_or(72).clamp(1, 24 * 30);
    let expires_at = Utc::now() + Duration::hours(expires_in);
    let invite_id = format!("oinv_{}", uuid::Uuid::new_v4());
    let invite_token = format!("oinv_{}", uuid::Uuid::new_v4());
    let token_hash = signing::hash_token(&invite_token);

    let mut tx = state.db.begin().await?;

    // Ensure one active invite per email.
    sqlx::query(
        "UPDATE owner_invites
         SET status = 'revoked', revoked_at = NOW(), updated_at = NOW()
         WHERE status = 'pending' AND lower(email) = lower($1)",
    )
    .bind(&email)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO owner_invites
         (id, email, token_hash, invited_by_user_id, status, expires_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'pending', $5, NOW(), NOW())",
    )
    .bind(&invite_id)
    .bind(&email)
    .bind(&token_hash)
    .bind(&claims.sub)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let invite_url = format!(
        "{}/platform/login?owner_invite={}",
        state.config.server_url, invite_token
    );

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "invite": {
                "id": invite_id,
                "email": email,
                "status": "pending",
                "expires_at": expires_at.to_rfc3339(),
                "invited_by_user_id": claims.sub,
            },
            "invite_token": invite_token,
            "invite_url": invite_url,
        })),
    ))
}

/// POST /platform/admin/invites/{id}/revoke
/// — Revoke a pending provider-owner invite.
pub async fn revoke_owner_invite(
    State(state): State<AppState>,
    Path(invite_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let _claims = owner_claims(&req)?;

    let updated = sqlx::query(
        "UPDATE owner_invites
         SET status = 'revoked', revoked_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(&invite_id)
    .execute(&state.db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "invite_not_found",
            message: "Pending invite not found".into(),
        });
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "invite_id": invite_id,
        "status": "revoked",
    })))
}

/// POST /platform/admin/invites/accept — Accept an owner invite as a platform user.
pub async fn accept_owner_invite(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user =
        req.extensions()
            .get::<PlatformUser>()
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "unauthorized",
                message: "Platform authentication required".into(),
            })?;
    let body: AcceptOwnerInviteRequest =
        axum::Json::<AcceptOwnerInviteRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body. Required: token".into(),
            })?
            .0;

    let token = body.token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_invite_token",
            message: "Invite token is required".into(),
        });
    }

    let token_hash = signing::hash_token(token);
    let user_email = normalize_email(&platform_user.email);
    let mut tx = state.db.begin().await?;

    let invite = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            chrono::DateTime<Utc>,
            Option<String>,
        ),
    >(
        "SELECT id, email, status, expires_at, accepted_by_user_id
         FROM owner_invites
         WHERE token_hash = $1
         LIMIT 1
         FOR UPDATE",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound {
        error: "invite_not_found",
        message: "Invite not found".into(),
    })?;

    let (invite_id, invite_email, status, expires_at, accepted_by_user_id) = invite;
    let invite_email_norm = normalize_email(&invite_email);
    if invite_email_norm != user_email {
        return Err(AppError::Forbidden {
            error: "invite_email_mismatch",
            message: "Invite email does not match the signed-in platform account".into(),
        });
    }

    if status == "accepted" {
        if accepted_by_user_id.as_deref() == Some(platform_user.user_id.as_str()) {
            tx.commit().await?;
            return Ok(Json(serde_json::json!({
                "accepted": true,
                "already_accepted": true,
                "invite_id": invite_id,
            })));
        }
        return Err(AppError::Forbidden {
            error: "invite_already_used",
            message: "This invite was already accepted by another account".into(),
        });
    }

    if status == "revoked" {
        return Err(AppError::BadRequest {
            error: "invite_revoked",
            message: "This invite has been revoked".into(),
        });
    }

    if status == "expired" || expires_at < Utc::now() {
        let _ = sqlx::query(
            "UPDATE owner_invites SET status = 'expired', updated_at = NOW() WHERE id = $1",
        )
        .bind(&invite_id)
        .execute(&mut *tx)
        .await;

        return Err(AppError::BadRequest {
            error: "invite_expired",
            message: "This invite has expired".into(),
        });
    }

    sqlx::query(
        "INSERT INTO platform_accounts (user_id, status, created_at, updated_at)
         VALUES ($1, 'active', NOW(), NOW())
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&platform_user.user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&platform_user.user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE owner_invites
         SET status = 'accepted',
             accepted_by_user_id = $2,
             accepted_at = NOW(),
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(&invite_id)
    .bind(&platform_user.user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "accepted": true,
        "already_accepted": false,
        "invite_id": invite_id,
        "email": platform_user.email,
    })))
}

/// GET /platform/admin/dashboard — Provider control-plane dashboard.
pub async fn dashboard_page(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let login_redirect = "/platform/login".to_string();

    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    match validate_owner_refresh_session(&state, &headers).await {
        Ok(result) if result.valid => {}
        Ok(result) => {
            tracing::warn!(
                request_id = request_id.as_deref().unwrap_or(""),
                reason = result.reason,
                "Owner dashboard session invalid; redirecting to login"
            );
            return Redirect::temporary(&login_redirect).into_response();
        }
        Err(error) => {
            tracing::error!(%error, "Failed to validate owner refresh session for dashboard route");
            return Redirect::temporary(&login_redirect).into_response();
        }
    }

    let page = render_page(&PageConfig {
        title: "Admin Dashboard — Forma Auth",
        route_pattern: "/platform/admin/dashboard",
        manifest: &state.asset_manifest,
        config_script: None,
        body_class: Some("owner-dashboard min-h-screen antialiased"),
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;--grid-opacity:0.25;--glow-size:700px;--glow-intensity:0.04;}"),
        body_prefix: None,
        render_mode: RenderMode::Phase1ClientMount,
        ir_module: None,
        slots: None,
    });

    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (axum::http::header::HeaderName::from_static("content-security-policy"), page.csp),
        ],
        Html(page.html),
    )
        .into_response()
}
