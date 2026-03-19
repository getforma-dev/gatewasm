use axum::{
    extract::{FromRequest, Query, State},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::error::AppError;
use crate::AppState;

use super::{
    clamp_limit, normalize_optional_field, owner_claims, parse_rfc3339_opt, record_owner_event,
    request_id_from_req, OwnerEvent,
};

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
