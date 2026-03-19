use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use crate::error::AppError;
use crate::AppState;

use super::{clamp_limit, normalize_optional_field, parse_rfc3339_opt, OwnerAnalyticsQuery};

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
