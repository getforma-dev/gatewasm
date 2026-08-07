use axum::{
    extract::{FromRequest, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use subtle::ConstantTimeEq;

use crate::compliance::audit::{self, AuditAction, RESULT_FAILURE, RESULT_SUCCESS};
use crate::db;
use crate::error::AppError;
use crate::handlers::auth::{append_refresh_cookie_headers, extract_refresh_cookie_candidates};
use crate::jwt::signing;
use crate::middleware::dpop;
use crate::middleware::fingerprint::{compute_fingerprint, extract_client_ip, extract_user_agent};
use crate::middleware::tenant::TenantContext;
use crate::security_config::TenantSecurityConfig;
use crate::types::*;
use crate::AppState;

#[derive(Debug, Clone, FromRow)]
struct RefreshTokenRow {
    token_hash: String,
    user_id: String,
    expires_at: chrono::DateTime<Utc>,
    revoked: bool,
    family_id: Option<String>,
    tenant_id: Option<String>,
    mfa_verified: bool,
    auth_method: String,
    auth_provider: Option<String>,
    created_at: chrono::DateTime<Utc>,
    last_activity_at: Option<chrono::DateTime<Utc>>,
    client_fingerprint: Option<String>,
    risk_score: i32,
}

fn request_id_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string()
}

pub async fn token_exchange(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant_ctx = req.extensions().get::<TenantContext>().cloned();
    let route_tenant_id = tenant_ctx.as_ref().map(|t| t.tenant_id.clone());
    let route_tenant_slug = tenant_ctx.as_ref().map(|t| t.slug.clone());
    let security_config = tenant_ctx
        .as_ref()
        .map(|t| t.security_config.clone())
        .unwrap_or_default();
    let headers = req.headers().clone();
    let request_id = request_id_from_headers(&headers);
    let body: TokenRequest = Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    tracing::info!(
        request_id = %request_id,
        grant_type = %body.grant_type,
        client_id = ?body.client_id,
        redirect_uri = ?body.redirect_uri,
        code_prefix = ?body.code.as_deref().map(|c| crate::text::truncate_chars(c, 12)),
        "Token exchange requested"
    );
    match body.grant_type.as_str() {
        "authorization_code" => {
            handle_auth_code(
                state,
                body,
                headers,
                route_tenant_id.as_deref(),
                route_tenant_slug.as_deref(),
                &request_id,
                &security_config,
            )
            .await
        }
        "refresh_token" => {
            handle_refresh(
                state,
                body,
                headers,
                route_tenant_id.as_deref(),
                route_tenant_slug.as_deref(),
                &request_id,
                &security_config,
            )
            .await
        }
        _ => Err(AppError::BadRequest {
            error: "unsupported_grant_type",
            message: format!("Unsupported grant_type: {}", body.grant_type),
        }),
    }
}

async fn handle_auth_code(
    state: AppState,
    body: TokenRequest,
    headers: axum::http::HeaderMap,
    expected_tenant_id: Option<&str>,
    expected_tenant_slug: Option<&str>,
    request_id: &str,
    security_config: &TenantSecurityConfig,
) -> Result<axum::response::Response, AppError> {
    let code = body.code.as_deref().ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing authorization code".into(),
    })?;

    let code_verifier = body.code_verifier.as_deref().ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing code_verifier (PKCE required)".into(),
    })?;

    let req_client_id = body.client_id.as_deref().ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing client_id".into(),
    })?;

    let req_redirect_uri = body.redirect_uri.as_deref().ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing redirect_uri".into(),
    })?;

    let auth_code_row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT user_id, code_challenge, tenant_id, provider FROM auth_codes WHERE code = $1 AND client_id = $2 AND redirect_uri = $3 AND expires_at > $4 AND used = FALSE",
    )
    .bind(code)
    .bind(req_client_id)
    .bind(req_redirect_uri)
    .bind(Utc::now())
    .fetch_optional(&state.db)
    .await?;

    let (user_id, stored_code_challenge, tenant_id, auth_provider) =
        auth_code_row.ok_or_else(|| {
            tracing::warn!(
                request_id = %request_id,
                code_prefix = %crate::text::truncate_chars(code, 12),
                client_id = %req_client_id,
                redirect_uri = %req_redirect_uri,
                "Authorization code rejected: no matching code row"
            );
            AppError::BadRequest {
                error: "invalid_grant",
                message: "Invalid or expired authorization code".into(),
            }
        })?;

    if let Some(expected) = expected_tenant_id {
        if tenant_id.as_deref() != Some(expected) {
            tracing::warn!(
                request_id = %request_id,
                expected_tenant_id = %expected,
                token_tenant_id = ?tenant_id,
                "Authorization code rejected: tenant mismatch"
            );
            return Err(AppError::Unauthorized {
                error: "invalid_grant",
                message: "Authorization code does not belong to this tenant".into(),
            });
        }
    }

    tracing::info!(
        request_id = %request_id,
        user_id = %user_id,
        tenant_id = ?tenant_id,
        auth_provider = ?auth_provider,
        "Authorization code lookup succeeded"
    );

    // PKCE verification
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let computed_challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    if computed_challenge.as_bytes().ct_eq(stored_code_challenge.as_bytes()).unwrap_u8() != 1 {
        tracing::warn!(
            request_id = %request_id,
            computed = %computed_challenge,
            stored = %stored_code_challenge,
            "Authorization code rejected: PKCE mismatch"
        );
        return Err(AppError::BadRequest {
            error: "invalid_grant",
            message: "PKCE verification failed".into(),
        });
    }

    // Mark auth code as used (atomic)
    let result = sqlx::query("UPDATE auth_codes SET used = TRUE WHERE code = $1 AND used = FALSE")
        .bind(code)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest {
            error: "invalid_grant",
            message: "Authorization code already used".into(),
        });
    }

    let user_row = sqlx::query_as::<_, (String, Option<String>, Option<String>, bool)>(
        "SELECT email, name, avatar, COALESCE(email_verified, FALSE) FROM users WHERE id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::InternalError {
        error: "server_error",
        message: "User not found".into(),
    })?;

    let (email, name, avatar, email_verified) = user_row;

    let roles = db::fetch_user_roles(&state.db, &user_id, tenant_id.as_deref()).await;

    let auth_method = if auth_provider.as_deref() == Some("email") {
        "password"
    } else {
        "sso"
    };

    // ── DPoP proof validation (sender-constrained tokens) ────────────
    let dpop_cnf = validate_dpop_if_needed(
        &headers,
        &state.db,
        security_config,
        "POST",
        &state.config.server_url,
        "/auth/token",
        request_id,
    )
    .await?;

    let (access_token, expires_at) = signing::create_access_token(
        signing::AccessTokenInput {
            user_id: &user_id,
            email: &email,
            name: name.as_deref(),
            tenant_id: tenant_id.as_deref(),
            roles: &roles,
            auth_method,
            mfa_verified: false,
            cnf: dpop_cnf.clone(),
        },
        &state.config,
        &state.keys,
    )?;

    let refresh_token = signing::create_refresh_token();
    let refresh_hash = signing::hash_token(&refresh_token);
    let refresh_expires = Utc::now() + Duration::seconds(state.config.refresh_token_expiry as i64);
    let family_id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, family_id, tenant_id, mfa_verified, auth_method, auth_provider, last_activity_at) VALUES ($1, $2, $3, $4, $5, $6, FALSE, $7, $8, NOW())",
    )
    .bind(format!("rt_{}", uuid::Uuid::new_v4()))
    .bind(&user_id)
    .bind(&refresh_hash)
    .bind(refresh_expires)
    .bind(&family_id)
    .bind(&tenant_id)
    .bind(auth_method)
    .bind(&auth_provider)
    .execute(&state.db)
    .await?;

    db::enforce_refresh_token_limit(&state.db, &user_id, security_config.max_sessions_per_user as i64).await;

    let mut headers = HeaderMap::new();
    append_refresh_cookie_headers(
        &mut headers,
        &refresh_token,
        state.config.refresh_token_expiry,
        &state.config.server_url,
        expected_tenant_slug,
    );

    let token_type = if dpop_cnf.is_some() { "DPoP" } else { "Bearer" };

    tracing::info!(
        request_id = %request_id,
        user_id = %user_id,
        tenant_id = ?tenant_id,
        auth_method = %auth_method,
        auth_provider = ?auth_provider,
        tenant_slug_scope = ?expected_tenant_slug,
        token_type = %token_type,
        "Authorization code exchange succeeded"
    );

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": token_type,
            "expires_in": state.config.access_token_expiry,
            "expires_at": expires_at,
            "user": {
                "id": user_id,
                "email": email,
                "name": name,
                "avatar": avatar,
                "email_verified": email_verified,
                "roles": roles,
                "auth_method": auth_method,
                "mfa_verified": false,
            }
        })),
    )
        .into_response())
}

async fn handle_refresh(
    state: AppState,
    body: TokenRequest,
    headers: axum::http::HeaderMap,
    expected_tenant_id: Option<&str>,
    expected_tenant_slug: Option<&str>,
    request_id: &str,
    security_config: &TenantSecurityConfig,
) -> Result<axum::response::Response, AppError> {
    let client_ip = extract_client_ip(&headers, None);
    let explicit_refresh_token = body.refresh_token.clone();
    let cookie_flow = explicit_refresh_token.is_none();
    let now = Utc::now();

    let candidate_tokens: Vec<String> = if let Some(token) = explicit_refresh_token {
        vec![token]
    } else {
        let candidates = extract_refresh_cookie_candidates(&headers, expected_tenant_slug);
        if candidates.is_empty() {
            tracing::warn!(
                request_id = %request_id,
                tenant_slug = ?expected_tenant_slug,
                "Refresh token rejected: no cookie candidates present"
            );
            return Err(AppError::BadRequest {
                error: "invalid_request",
                message: "Missing refresh_token".into(),
            });
        }
        candidates
    };

    let candidate_hashes: Vec<String> = candidate_tokens
        .iter()
        .map(|token| signing::hash_token(token))
        .collect();

    let mut rows = sqlx::query_as::<_, RefreshTokenRow>(
        "SELECT
            rt.token_hash,
            rt.user_id,
            rt.expires_at,
            rt.revoked,
            rt.family_id,
            rt.tenant_id,
            COALESCE(rt.mfa_verified, FALSE) AS mfa_verified,
            COALESCE(rt.auth_method, 'password') AS auth_method,
            rt.auth_provider,
            rt.created_at,
            rt.last_activity_at,
            rt.client_fingerprint,
            COALESCE(rt.risk_score, 0) AS risk_score
         FROM refresh_tokens rt
         WHERE rt.token_hash = ANY($1)",
    )
    .bind(&candidate_hashes)
    .fetch_all(&state.db)
    .await?;

    // Keep only tokens that belong to this route's tenant context (if present).
    if let Some(expected) = expected_tenant_id {
        let pre_filter = rows.len();
        rows.retain(|row| row.tenant_id.as_deref() == Some(expected));
        if pre_filter > 0 && rows.is_empty() {
            tracing::warn!(
                request_id = %request_id,
                expected_tenant_id = %expected,
                "Refresh token candidates did not match route tenant context"
            );
        }
    }

    let selected = if !cookie_flow {
        let token_hash = &candidate_hashes[0];
        let row = rows
            .iter()
            .find(|row| row.token_hash == *token_hash)
            .cloned()
            .ok_or(AppError::Unauthorized {
                error: "invalid_grant",
                message: "Invalid refresh token".into(),
            })?;

        if row.revoked {
            if let Some(ref fid) = row.family_id {
                tracing::warn!(
                    request_id = %request_id,
                    user_id = %row.user_id,
                    family_id = %fid,
                    "Refresh token replay detected — revoking entire family"
                );
                sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE family_id = $1")
                    .bind(fid)
                    .execute(&state.db)
                    .await
                    .ok();
            }
            if state.config.compliance.audit_log_enabled {
                audit::log_audit(
                    &state.db,
                    row.tenant_id.as_deref().unwrap_or("global"),
                    Some(&row.user_id),
                    AuditAction::USER_TOKEN_REVOKED,
                    "/auth/token",
                    &client_ip,
                    None,
                    None,
                    RESULT_FAILURE,
                    Some("token_reuse_detected"),
                    &state.config.compliance.mode.to_string(),
                )
                .await;
            }
            return Err(AppError::Unauthorized {
                error: "token_reuse_detected",
                message: "Refresh token has been reused — all sessions revoked for security".into(),
            });
        }

        if row.expires_at < now {
            tracing::warn!(
                request_id = %request_id,
                user_id = %row.user_id,
                "Refresh token rejected: explicit token expired"
            );
            return Err(AppError::Unauthorized {
                error: "token_expired",
                message: "Refresh token has expired".into(),
            });
        }

        row
    } else {
        // Browser cookie flow: choose the newest non-revoked, non-expired candidate.
        // This is robust against duplicate legacy cookies during migration.
        let revoked_count = rows.iter().filter(|row| row.revoked).count();
        let expired_count = rows
            .iter()
            .filter(|row| !row.revoked && row.expires_at <= now)
            .count();
        let valid_count = rows
            .iter()
            .filter(|row| !row.revoked && row.expires_at > now)
            .count();
        let best = rows
            .iter()
            .filter(|row| !row.revoked && row.expires_at > now)
            .max_by_key(|row| row.created_at)
            .cloned();

        if let Some(row) = best {
            tracing::info!(
                request_id = %request_id,
                candidate_count = candidate_hashes.len(),
                matched_rows = rows.len(),
                valid_rows = valid_count,
                revoked_rows = revoked_count,
                expired_rows = expired_count,
                selected_created_at = %row.created_at,
                "Refresh cookie candidate selected"
            );
            row
        } else if rows.iter().any(|row| !row.revoked && row.expires_at <= now) {
            tracing::warn!(
                request_id = %request_id,
                candidate_count = candidate_hashes.len(),
                matched_rows = rows.len(),
                expired_rows = expired_count,
                revoked_rows = revoked_count,
                "Refresh token rejected: cookie candidates are expired"
            );
            return Err(AppError::Unauthorized {
                error: "token_expired",
                message: "Refresh token has expired".into(),
            });
        } else {
            tracing::warn!(
                request_id = %request_id,
                candidate_count = candidate_hashes.len(),
                matched_rows = rows.len(),
                revoked_rows = revoked_count,
                "Refresh token rejected: no active cookie candidate"
            );
            return Err(AppError::Unauthorized {
                error: "invalid_grant",
                message: "No active refresh session. Sign in again.".into(),
            });
        }
    };

    let token_hash = selected.token_hash;
    let user_id = selected.user_id;
    let tenant_id = selected.tenant_id;
    let mfa_verified = selected.mfa_verified;
    let auth_method = selected.auth_method;
    let auth_provider = selected.auth_provider;
    let family_id = selected.family_id;

    // ── Idle session timeout check ──────────────────────────────────────
    // If the token has a last_activity_at timestamp, check whether the
    // session has been idle longer than the tenant's configured timeout.
    // NULL last_activity_at (legacy tokens) are treated as current to
    // avoid breaking existing sessions.
    let idle_timeout_minutes = security_config.idle_timeout_minutes;
    if idle_timeout_minutes > 0 {
        if let Some(last_activity) = selected.last_activity_at {
            let idle_cutoff = now - Duration::minutes(idle_timeout_minutes as i64);
            if last_activity < idle_cutoff {
                tracing::warn!(
                    request_id = %request_id,
                    user_id = %user_id,
                    last_activity_at = %last_activity,
                    idle_timeout_minutes = idle_timeout_minutes,
                    "Refresh token rejected: session idle timeout exceeded"
                );

                // Revoke the stale token
                sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE token_hash = $1")
                    .bind(&token_hash)
                    .execute(&state.db)
                    .await
                    .ok();

                if state.config.compliance.audit_log_enabled {
                    audit::log_audit(
                        &state.db,
                        tenant_id.as_deref().unwrap_or("global"),
                        Some(&user_id),
                        AuditAction::USER_TOKEN_REVOKED,
                        "/auth/token",
                        &client_ip,
                        None,
                        None,
                        RESULT_FAILURE,
                        Some("idle_session_timeout"),
                        &state.config.compliance.mode.to_string(),
                    )
                    .await;
                }

                return Err(AppError::Unauthorized {
                    error: "session_expired",
                    message: "Session expired due to inactivity".into(),
                });
            }
        }
    }

    // ── Client fingerprint drift check ────────────────────────────────────
    // Compute the current request's fingerprint and compare it to the
    // stored fingerprint. If they differ, increment risk_score. If
    // risk_score reaches the threshold (>= 3), revoke the entire token
    // family as a security precaution.
    // NULL stored fingerprint (legacy tokens) is treated as a match.
    let current_fingerprint = {
        let fp_ip = extract_client_ip(&headers, None);
        let fp_ua = extract_user_agent(&headers);
        compute_fingerprint(&fp_ua, &fp_ip)
    };

    let stored_fingerprint = selected.client_fingerprint;
    let stored_risk_score = selected.risk_score;

    let fingerprint_matches = match stored_fingerprint.as_deref() {
        Some(stored) => stored == current_fingerprint,
        None => true, // legacy token — treat as match
    };

    let new_risk_score = if fingerprint_matches {
        stored_risk_score
    } else {
        tracing::info!(
            request_id = %request_id,
            user_id = %user_id,
            old_risk_score = stored_risk_score,
            "Client fingerprint drift detected — incrementing risk score"
        );
        stored_risk_score + 1
    };

    if new_risk_score >= 3 {
        // Risk threshold exceeded — revoke entire family
        if let Some(ref fid) = family_id {
            tracing::warn!(
                request_id = %request_id,
                user_id = %user_id,
                family_id = %fid,
                risk_score = new_risk_score,
                "Risk score threshold reached — revoking entire token family"
            );
            sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE family_id = $1")
                .bind(fid)
                .execute(&state.db)
                .await
                .ok();
        }

        if state.config.compliance.audit_log_enabled {
            audit::log_audit(
                &state.db,
                tenant_id.as_deref().unwrap_or("global"),
                Some(&user_id),
                AuditAction::USER_TOKEN_REVOKED,
                "/auth/token",
                &extract_client_ip(&headers, None),
                None,
                None,
                RESULT_FAILURE,
                Some("fingerprint_risk_threshold"),
                &state.config.compliance.mode.to_string(),
            )
            .await;
        }

        return Err(AppError::Unauthorized {
            error: "token_risk_exceeded",
            message: "Session terminated due to suspicious activity".into(),
        });
    }

    let (email, name, email_verified) = sqlx::query_as::<_, (String, Option<String>, bool)>(
        "SELECT email, name, COALESCE(email_verified, FALSE) FROM users WHERE id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::InternalError {
        error: "server_error",
        message: "User not found".into(),
    })?;

    let roles = db::fetch_user_roles(&state.db, &user_id, tenant_id.as_deref()).await;

    // ── DPoP proof validation (sender-constrained tokens) ────────────
    let dpop_cnf = validate_dpop_if_needed(
        &headers,
        &state.db,
        security_config,
        "POST",
        &state.config.server_url,
        "/auth/token",
        request_id,
    )
    .await?;

    // Transaction: revoke old + insert new atomically (inherit family_id)
    let mut tx = state.db.begin().await?;

    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(&mut *tx)
        .await?;

    let (access_token, expires_at) = signing::create_access_token(
        signing::AccessTokenInput {
            user_id: &user_id,
            email: &email,
            name: name.as_deref(),
            tenant_id: tenant_id.as_deref(),
            roles: &roles,
            auth_method: &auth_method,
            mfa_verified,
            cnf: dpop_cnf.clone(),
        },
        &state.config,
        &state.keys,
    )?;

    let new_refresh_token = signing::create_refresh_token();
    let new_refresh_hash = signing::hash_token(&new_refresh_token);
    let refresh_expires = Utc::now() + Duration::seconds(state.config.refresh_token_expiry as i64);

    let new_family_id = family_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, family_id, tenant_id, mfa_verified, auth_method, auth_provider, last_activity_at, client_fingerprint, risk_score) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), $10, $11)",
    )
    .bind(format!("rt_{}", uuid::Uuid::new_v4()))
    .bind(&user_id)
    .bind(&new_refresh_hash)
    .bind(refresh_expires)
    .bind(&new_family_id)
    .bind(&tenant_id)
    .bind(mfa_verified)
    .bind(&auth_method)
    .bind(&auth_provider)
    .bind(&current_fingerprint)
    .bind(new_risk_score)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    db::enforce_refresh_token_limit(&state.db, &user_id, security_config.max_sessions_per_user as i64).await;

    if state.config.compliance.audit_log_enabled {
        audit::log_audit(
            &state.db,
            tenant_id.as_deref().unwrap_or("global"),
            Some(&user_id),
            AuditAction::USER_TOKEN_REFRESH,
            "/auth/token",
            &client_ip,
            None,
            None,
            RESULT_SUCCESS,
            None,
            &state.config.compliance.mode.to_string(),
        )
        .await;
    }

    let mut headers = HeaderMap::new();
    append_refresh_cookie_headers(
        &mut headers,
        &new_refresh_token,
        state.config.refresh_token_expiry,
        &state.config.server_url,
        expected_tenant_slug,
    );

    let token_type = if dpop_cnf.is_some() { "DPoP" } else { "Bearer" };

    tracing::info!(
        request_id = %request_id,
        user_id = %user_id,
        tenant_id = ?tenant_id,
        auth_method = %auth_method,
        mfa_verified,
        cookie_flow,
        token_type = %token_type,
        "Refresh token exchange succeeded"
    );

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "access_token": access_token,
            "refresh_token": new_refresh_token,
            "token_type": token_type,
            "expires_in": state.config.access_token_expiry,
            "expires_at": expires_at,
            "user": {
                "id": user_id,
                "email": email,
                "name": name,
                "avatar": null,
                "email_verified": email_verified,
                "roles": roles,
                "auth_method": auth_method,
                "mfa_verified": mfa_verified,
            }
        })),
    )
        .into_response())
}

/// Validate DPoP proof if the tenant requires it (or if a DPoP header is present).
///
/// Returns `Some(cnf_value)` with `{ "jkt": "<thumbprint>" }` if DPoP was validated,
/// or `None` if DPoP is not required and no proof was provided.
async fn validate_dpop_if_needed(
    headers: &HeaderMap,
    db: &sqlx::PgPool,
    security_config: &TenantSecurityConfig,
    expected_method: &str,
    server_url: &str,
    path: &str,
    request_id: &str,
) -> Result<Option<serde_json::Value>, AppError> {
    let dpop_header = headers
        .get("DPoP")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if security_config.dpop_required && dpop_header.is_none() {
        tracing::warn!(
            request_id = %request_id,
            "DPoP proof required but missing"
        );
        return Err(AppError::BadRequest {
            error: "invalid_dpop_proof",
            message: "DPoP proof required".into(),
        });
    }

    let proof = match dpop_header {
        Some(p) => p,
        None => return Ok(None),
    };

    let expected_uri = format!("{}{}", server_url, path);

    let (jti, thumbprint) =
        dpop::validate_dpop_proof(&proof, expected_method, &expected_uri).map_err(|e| {
            tracing::warn!(
                request_id = %request_id,
                error = %e,
                "DPoP proof validation failed"
            );
            AppError::BadRequest {
                error: "invalid_dpop_proof",
                message: e,
            }
        })?;

    // Check jti replay
    dpop::check_and_store_jti(db, &jti).await.map_err(|e| {
        tracing::warn!(
            request_id = %request_id,
            jti = %jti,
            error = %e,
            "DPoP jti replay check failed"
        );
        AppError::BadRequest {
            error: "invalid_dpop_proof",
            message: e,
        }
    })?;

    tracing::info!(
        request_id = %request_id,
        thumbprint = %thumbprint,
        "DPoP proof validated — binding thumbprint to access token"
    );

    Ok(Some(serde_json::json!({ "jkt": thumbprint })))
}
