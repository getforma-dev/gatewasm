use axum::extract::FromRequest;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{Duration, Utc};

use crate::crypto::password;
use crate::email;
use crate::error::AppError;
use crate::handlers::auth::normalize_email;
use crate::middleware::tenant::TenantContext;
use crate::responses::MessageResponse;
use crate::types::*;
use crate::AppState;

/// Helper to get tenant_id from request extensions
fn get_tenant_id(req: &axum::extract::Request) -> Option<String> {
    req.extensions()
        .get::<TenantContext>()
        .map(|t| t.tenant_id.clone())
}

/// Helper to get tenant context from request extensions.
fn get_tenant_context(req: &axum::extract::Request) -> Option<TenantContext> {
    req.extensions().get::<TenantContext>().cloned()
}

pub async fn forgot_password(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = get_tenant_context(&req);
    let tenant_id = get_tenant_id(&req);

    let body: ForgotPasswordRequest = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    let email = normalize_email(&body.email);
    tracing::info!(email = %email, tenant_id = ?tenant_id, "Processing forgot-password request");

    // Look up user — but always return 200 to avoid leaking whether email exists
    let user = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE email = $1 AND ($2::text IS NULL OR tenant_id = $2)",
    )
    .bind(&email)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?;

    if let Some((user_id,)) = user {
        // Check that user has a password credential
        let has_password: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM credentials WHERE user_id = $1 AND provider = 'email'",
        )
        .bind(&user_id)
        .fetch_one(&state.db)
        .await?;

        if has_password > 0 {
            let reset_token = format!("rst_{}", uuid::Uuid::new_v4());
            let reset_expires = Utc::now() + Duration::hours(1);

            sqlx::query(
                "UPDATE users SET password_reset_token = $1, password_reset_expires = $2 WHERE id = $3",
            )
            .bind(&reset_token)
            .bind(reset_expires)
            .bind(&user_id)
            .execute(&state.db)
            .await?;

            if let Some(ref tenant_ctx) = tenant {
                let app_name = tenant_ctx
                    .branding_app_name
                    .as_deref()
                    .unwrap_or(&tenant_ctx.name);
                let (subject, text, html) = email::password_reset_email_content(
                    &state.config.server_url,
                    &tenant_ctx.slug,
                    app_name,
                    &reset_token,
                );
                if email::email_delivery_configured(&state.config) {
                    if let Err(error) =
                        email::send_email(&state.config, &email, &subject, &text, &html).await
                    {
                        tracing::warn!(
                            user_id = %user_id,
                            email = %email,
                            %error,
                            "Failed to send password reset email"
                        );
                    }
                } else {
                    tracing::warn!(
                        user_id = %user_id,
                        email = %email,
                        "SMTP is not configured; password reset email not delivered"
                    );
                }
            }
        }
    }

    // Always return success to prevent email enumeration
    Ok((
        StatusCode::OK,
        Json(MessageResponse {
            message: "If an account with that email exists, a password reset link has been sent.",
        }),
    ))
}

pub async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!("Processing password reset");

    // Reset token is globally unique, tenant scoping is implicit
    let user_id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM users WHERE password_reset_token = $1 AND password_reset_expires > $2",
    )
    .bind(&body.token)
    .bind(Utc::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::BadRequest {
        error: "invalid_token",
        message: "Invalid or expired reset token".into(),
    })?;

    password::validate_password_complexity(&body.new_password).map_err(|msg| {
        AppError::BadRequest {
            error: "weak_password",
            message: msg.into(),
        }
    })?;

    let hashed = password::hash_password(&body.new_password)?;

    // Wrap password update + token clear + session revocation in a single transaction
    let mut tx = state.db.begin().await?;

    // Update password
    sqlx::query(
        "UPDATE credentials SET hashed_password = $1 WHERE user_id = $2 AND provider = 'email'",
    )
    .bind(&hashed)
    .bind(&user_id)
    .execute(&mut *tx)
    .await?;

    // Clear reset token
    sqlx::query(
        "UPDATE users SET password_reset_token = NULL, password_reset_expires = NULL WHERE id = $1",
    )
    .bind(&user_id)
    .execute(&mut *tx)
    .await?;

    // Revoke all refresh tokens for security
    let revoked = sqlx::query(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1 AND revoked = FALSE",
    )
    .bind(&user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        user_id = %user_id,
        revoked_tokens = revoked.rows_affected(),
        "Password reset complete — all refresh tokens revoked"
    );

    // Reset login attempts / lockout
    if let Err(error) =
        sqlx::query("UPDATE users SET login_attempts = 0, locked_until = NULL WHERE id = $1")
            .bind(&user_id)
            .execute(&state.db)
            .await
    {
        tracing::warn!(
            user_id = %user_id,
            %error,
            "Failed to clear login_attempts/locked_until after password reset"
        );
    }

    // Dispatch webhook event
    let tenant_id: Option<String> = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    if let Some(ref tid) = tenant_id {
        super::webhook_engine::dispatch_event(
            &state.db,
            tid,
            "user.password_reset",
            serde_json::json!({ "user_id": user_id }),
        )
        .await;
    }

    Ok((
        StatusCode::OK,
        Json(MessageResponse {
            message: "Password has been reset successfully. Please log in with your new password.",
        }),
    ))
}
