use axum::{
    extract::{FromRequest, Path, Query, State},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::AppState;

use super::{
    clamp_limit, normalize_optional_field, owner_claims, parse_rfc3339_opt, record_owner_event,
    request_id_from_req, OwnerEvent,
};

// ── Lifecycle enums ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerStatus {
    Active,
    Suspended,
    Rejected,
}

impl std::fmt::Display for CustomerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Suspended => write!(f, "suspended"),
            Self::Rejected => write!(f, "rejected"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerCategory {
    Standard,
    Education,
    Agency,
    Enterprise,
}

impl std::fmt::Display for CustomerCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::Education => write!(f, "education"),
            Self::Agency => write!(f, "agency"),
            Self::Enterprise => write!(f, "enterprise"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Normal,
    High,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Normal => write!(f, "normal"),
            Self::High => write!(f, "high"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStage {
    Lead,
    Onboarding,
    Active,
    AtRisk,
    Churned,
}

impl std::fmt::Display for LifecycleStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lead => write!(f, "lead"),
            Self::Onboarding => write!(f, "onboarding"),
            Self::Active => write!(f, "active"),
            Self::AtRisk => write!(f, "at_risk"),
            Self::Churned => write!(f, "churned"),
        }
    }
}

// ── Query / request structs ─────────────────────────────────────────────

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
pub struct UpdateCustomerLifecycleRequest {
    pub status: Option<CustomerStatus>,
    pub category: Option<CustomerCategory>,
    pub company_name: Option<String>,
    pub billing_ref: Option<String>,
    pub notes: Option<String>,
    pub risk_level: Option<RiskLevel>,
    pub lifecycle_stage: Option<LifecycleStage>,
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

fn sort_clause(raw: Option<&str>) -> &'static str {
    match raw.unwrap_or("created_desc") {
        "created_asc" => "fc.created_at ASC",
        "tenants_desc" => "tenant_count DESC, fc.created_at DESC",
        "revenue_desc" => "lifetime_revenue_cents DESC, fc.created_at DESC",
        _ => "fc.created_at DESC",
    }
}

#[derive(sqlx::FromRow)]
struct CustomerSnapshotRow {
    user_id: String,
    email: String,
    name: Option<String>,
    status: String,
    category: String,
    risk_level: String,
    company_name: Option<String>,
    billing_ref: Option<String>,
    notes: Option<String>,
    lifecycle_stage: Option<String>,
    blocked_reason: Option<String>,
    last_contacted_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn fetch_customer_snapshot(
    state: &AppState,
    customer_id: &str,
) -> Result<serde_json::Value, AppError> {
    let row = sqlx::query_as::<_, CustomerSnapshotRow>(
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

    Ok(serde_json::json!({
        "user_id": row.user_id,
        "email": row.email,
        "name": row.name,
        "status": row.status,
        "category": row.category,
        "risk_level": row.risk_level,
        "lifecycle_stage": row.lifecycle_stage,
        "company_name": row.company_name,
        "billing_ref": row.billing_ref,
        "notes": row.notes,
        "blocked_reason": row.blocked_reason,
        "last_contacted_at": row.last_contacted_at.map(|d| d.to_rfc3339()),
        "tenant_count": tenant_count,
        "lifetime_revenue_cents": lifetime_revenue_cents,
        "last_payment_at": last_payment_at.map(|d| d.to_rfc3339()),
        "created_at": row.created_at.to_rfc3339(),
        "updated_at": row.updated_at.to_rfc3339(),
    }))
}

#[derive(sqlx::FromRow)]
struct CustomerListRow {
    user_id: String,
    email: String,
    name: Option<String>,
    status: String,
    company_name: Option<String>,
    billing_ref: Option<String>,
    category: String,
    risk_level: String,
    lifecycle_stage: String,
    created_at: DateTime<Utc>,
    tenant_count: i64,
    lifetime_revenue_cents: i64,
    trial_expiring_tenants: i64,
    failed_logins_24h: i64,
    payment_failed_30d: i64,
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

    let rows = sqlx::query_as::<_, CustomerListRow>(&list_sql)
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
        .map(|row| {
                serde_json::json!({
                    "user_id": row.user_id,
                    "email": row.email,
                    "name": row.name,
                    "status": row.status,
                    "category": row.category,
                    "risk_level": row.risk_level,
                    "lifecycle_stage": row.lifecycle_stage,
                    "company_name": row.company_name,
                    "billing_ref": row.billing_ref,
                    "tenant_count": row.tenant_count,
                    "lifetime_revenue_cents": row.lifetime_revenue_cents,
                    "trial_expiring_tenants": row.trial_expiring_tenants,
                    "failed_logins_24h": row.failed_logins_24h,
                    "payment_failed_30d": row.payment_failed_30d,
                    "created_at": row.created_at.to_rfc3339(),
                })
        })
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

#[derive(sqlx::FromRow)]
struct CustomerTenantRow {
    id: String,
    name: String,
    slug: String,
    status: String,
    plan: String,
    max_users: i32,
    trial_ends_at: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
    user_count: i64,
    active_sessions: i64,
    logins_24h: i64,
    active_users_7d: i64,
    payment_failed_30d: i64,
}

/// GET /platform/admin/customers/{id}/tenants — List customer tenants.
pub async fn list_customer_tenants(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let tenants = sqlx::query_as::<_, CustomerTenantRow>(
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
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "name": row.name,
                "slug": row.slug,
                "status": row.status,
                "plan": row.plan,
                "max_users": row.max_users,
                "trial_ends_at": row.trial_ends_at.map(|d| d.to_rfc3339()),
                "created_at": row.created_at.to_rfc3339(),
                "user_count": row.user_count,
                "active_sessions": row.active_sessions,
                "logins_24h": row.logins_24h,
                "active_users_7d": row.active_users_7d,
                "payment_failed_30d": row.payment_failed_30d,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "customer_id": customer_id,
        "tenants": items,
    })))
}

#[derive(sqlx::FromRow)]
struct OwnerTenantListRow {
    id: String,
    slug: String,
    name: String,
    status: String,
    plan: String,
    owner_user_id: Option<String>,
    owner_email: Option<String>,
    company_name: Option<String>,
    trial_ends_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    user_count: i64,
    active_sessions: i64,
    last_login_at: Option<DateTime<Utc>>,
    failed_logins_24h: i64,
    active_users_7d: i64,
    payment_failed_30d: i64,
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

    let rows = sqlx::query_as::<_, OwnerTenantListRow>(
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
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "slug": row.slug,
                "name": row.name,
                "status": row.status,
                "plan": row.plan,
                "owner_user_id": row.owner_user_id,
                "owner_email": row.owner_email,
                "owner_company": row.company_name,
                "trial_ends_at": row.trial_ends_at.map(|d| d.to_rfc3339()),
                "created_at": row.created_at.to_rfc3339(),
                "user_count": row.user_count,
                "active_sessions": row.active_sessions,
                "last_login_at": row.last_login_at.map(|d| d.to_rfc3339()),
                "failed_logins_24h": row.failed_logins_24h,
                "active_users_7d": row.active_users_7d,
                "payment_failed_30d": row.payment_failed_30d,
            })
        })
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

#[derive(sqlx::FromRow)]
struct LifecycleEventRow {
    id: String,
    action: String,
    actor_user_id: Option<String>,
    actor_email: Option<String>,
    resource: String,
    request_id: String,
    tenant_id: Option<String>,
    details: serde_json::Value,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LoginAttemptRow {
    id: String,
    success: bool,
    ip_address: Option<String>,
    user_agent: Option<String>,
    failure_reason: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct AuditEventRow {
    event_id: String,
    action: String,
    resource: String,
    result: String,
    failure_reason: Option<String>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    timestamp: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct PaymentSummaryRow {
    paid_cents: i64,
    pending_cents: i64,
    failed_cents: i64,
    refunded_cents: i64,
    total_events: i64,
}

#[derive(sqlx::FromRow)]
struct RecentPaymentRow {
    id: String,
    tenant_id: Option<String>,
    amount_cents: i64,
    currency: String,
    status: String,
    external_invoice_id: Option<String>,
    receipt_url: Option<String>,
    created_at: DateTime<Utc>,
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

    let lifecycle_rows = sqlx::query_as::<_, LifecycleEventRow>(
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
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "action": row.action,
                "actor_user_id": row.actor_user_id,
                "actor_email": row.actor_email,
                "resource": row.resource,
                "request_id": row.request_id,
                "tenant_id": row.tenant_id,
                "details": row.details,
                "created_at": row.created_at.to_rfc3339(),
            })
        })
        .collect();

    let mut auth_events: Vec<(DateTime<Utc>, serde_json::Value)> = Vec::new();

    let login_rows = sqlx::query_as::<_, LoginAttemptRow>(
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

    for row in login_rows {
        auth_events.push((
            row.created_at,
            serde_json::json!({
                "id": row.id,
                "source": "login_attempt",
                "action": if row.success { "login.success" } else { "login.failure" },
                "result": if row.success { "success" } else { "failure" },
                "ip_address": row.ip_address,
                "user_agent": row.user_agent,
                "failure_reason": row.failure_reason,
                "created_at": row.created_at.to_rfc3339(),
            }),
        ));
    }

    let audit_rows = sqlx::query_as::<_, AuditEventRow>(
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

    for row in audit_rows {
        auth_events.push((
            row.timestamp,
            serde_json::json!({
                "id": row.event_id,
                "source": "audit_log",
                "action": row.action,
                "resource": row.resource,
                "result": row.result,
                "failure_reason": row.failure_reason,
                "ip_address": row.ip_address,
                "user_agent": row.user_agent,
                "created_at": row.timestamp.to_rfc3339(),
            }),
        ));
    }

    auth_events.sort_by(|a, b| b.0.cmp(&a.0));
    let recent_auth_events: Vec<serde_json::Value> =
        auth_events.into_iter().take(60).map(|(_, ev)| ev).collect();

    let payment_summary = sqlx::query_as::<_, PaymentSummaryRow>(
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
    .unwrap_or(PaymentSummaryRow { paid_cents: 0, pending_cents: 0, failed_cents: 0, refunded_cents: 0, total_events: 0 });

    let recent_payment_rows = sqlx::query_as::<_, RecentPaymentRow>(
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
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "tenant_id": row.tenant_id,
                "amount_cents": row.amount_cents,
                "currency": row.currency,
                "status": row.status,
                "external_invoice_id": row.external_invoice_id,
                "receipt_url": row.receipt_url,
                "created_at": row.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "customer": customer,
        "lifecycle_history": lifecycle_history,
        "recent_auth_events": recent_auth_events,
        "payment_summary": {
            "paid_cents": payment_summary.paid_cents,
            "pending_cents": payment_summary.pending_cents,
            "failed_cents": payment_summary.failed_cents,
            "refunded_cents": payment_summary.refunded_cents,
            "total_events": payment_summary.total_events,
        },
        "recent_payments": recent_payments,
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

    let status = body.status;
    let category = body.category;
    let company_name = normalize_optional_field(&body.company_name);
    let billing_ref = normalize_optional_field(&body.billing_ref);
    let notes = normalize_optional_field(&body.notes);
    let risk_level = body.risk_level;
    let lifecycle_stage = body.lifecycle_stage;
    let blocked_reason = normalize_optional_field(&body.blocked_reason);
    let clear_blocked_reason = body.blocked_reason.is_some() && blocked_reason.is_none();
    let apply_to_tenants = body.apply_to_tenants.unwrap_or(true);

    // String representations for SQL binding.
    let status_str = status.as_ref().map(|v| v.to_string());
    let category_str = category.as_ref().map(|v| v.to_string());
    let risk_level_str = risk_level.as_ref().map(|v| v.to_string());
    let lifecycle_stage_str = lifecycle_stage.as_ref().map(|v| v.to_string());

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
    .bind(status_str.as_deref())
    .bind(category_str.as_deref())
    .bind(company_name.as_deref())
    .bind(billing_ref.as_deref())
    .bind(notes.as_deref())
    .bind(risk_level_str.as_deref())
    .bind(lifecycle_stage_str.as_deref())
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
        if matches!(&status, Some(CustomerStatus::Suspended | CustomerStatus::Rejected)) {
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
        } else if matches!(&status, Some(CustomerStatus::Active)) {
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
