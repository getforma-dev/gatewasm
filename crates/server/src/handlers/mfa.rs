use axum::{
    extract::{FromRequest, State},
    response::IntoResponse,
    Json,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use chrono::{Duration, Utc};
use data_encoding::BASE32_NOPAD;
use qrcode::render::svg;
use qrcode::QrCode;
use rand::RngCore;
use ring::{aead, digest, hmac};
use serde::Deserialize;

use crate::crypto::password;
use crate::error::AppError;
use crate::handlers::auth::extract_refresh_cookie_candidates;
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct SetupMfaRequest {
    pub rotate: Option<bool>,
    pub recovery_password: Option<String>,
    pub recovery_via_sso: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyMfaRequest {
    pub code: String,
}

const MFA_SECRET_PREFIX_CURRENT: &str = "enc2";
const MFA_SECRET_PREFIX_LEGACY: &str = "enc1";

fn claims_from_request(req: &axum::extract::Request) -> Result<signing::Claims, AppError> {
    req.extensions()
        .get::<signing::Claims>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })
}

fn generate_totp_secret() -> String {
    let mut bytes = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    BASE32_NOPAD.encode(&bytes)
}

fn derive_mfa_encryption_key(key_material: &str) -> [u8; 32] {
    let digest = digest::digest(&digest::SHA256, key_material.as_bytes());
    let mut out = [0_u8; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

fn encrypt_mfa_secret_with_material(
    secret: &str,
    key_material: &str,
    prefix: &str,
) -> Result<String, AppError> {
    let key_bytes = derive_mfa_encryption_key(key_material);
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &key_bytes).map_err(|_| {
        AppError::InternalError {
            error: "server_error",
            message: "Failed to initialize MFA encryption key".into(),
        }
    })?;
    let key = aead::LessSafeKey::new(unbound);

    let mut nonce = [0_u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let nonce_value = aead::Nonce::assume_unique_for_key(nonce);

    let mut in_out = secret.as_bytes().to_vec();
    key.seal_in_place_append_tag(nonce_value, aead::Aad::empty(), &mut in_out)
        .map_err(|_| AppError::InternalError {
            error: "server_error",
            message: "Failed to encrypt MFA secret".into(),
        })?;

    Ok(format!(
        "{}:{}:{}",
        prefix,
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(&in_out),
    ))
}

fn decrypt_mfa_secret_with_material(
    stored: &str,
    key_material: &str,
    prefix: &str,
) -> Result<String, AppError> {
    let expected_prefix = format!("{prefix}:");
    if !stored.starts_with(&expected_prefix) {
        return Err(AppError::InternalError {
            error: "server_error",
            message: "MFA secret format is invalid".into(),
        });
    }
    let mut parts = stored.splitn(3, ':');
    let _prefix = parts.next();
    let nonce_b64 = parts.next().ok_or(AppError::InternalError {
        error: "server_error",
        message: "MFA secret format is invalid".into(),
    })?;
    let cipher_b64 = parts.next().ok_or(AppError::InternalError {
        error: "server_error",
        message: "MFA secret format is invalid".into(),
    })?;

    let nonce_vec =
        URL_SAFE_NO_PAD
            .decode(nonce_b64.as_bytes())
            .map_err(|_| AppError::InternalError {
                error: "server_error",
                message: "MFA secret nonce could not be decoded".into(),
            })?;
    if nonce_vec.len() != 12 {
        return Err(AppError::InternalError {
            error: "server_error",
            message: "MFA secret nonce length is invalid".into(),
        });
    }
    let mut nonce = [0_u8; 12];
    nonce.copy_from_slice(&nonce_vec);

    let mut cipher =
        URL_SAFE_NO_PAD
            .decode(cipher_b64.as_bytes())
            .map_err(|_| AppError::InternalError {
                error: "server_error",
                message: "MFA secret payload could not be decoded".into(),
            })?;

    let key_bytes = derive_mfa_encryption_key(key_material);
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &key_bytes).map_err(|_| {
        AppError::InternalError {
            error: "server_error",
            message: "Failed to initialize MFA encryption key".into(),
        }
    })?;
    let key = aead::LessSafeKey::new(unbound);
    let plain = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::empty(),
            &mut cipher,
        )
        .map_err(|_| AppError::InternalError {
            error: "server_error",
            message: "MFA secret could not be decrypted".into(),
        })?;

    String::from_utf8(plain.to_vec()).map_err(|_| AppError::InternalError {
        error: "server_error",
        message: "MFA secret is not valid UTF-8".into(),
    })
}

fn legacy_mfa_key_material(state: &AppState) -> String {
    format!("{}|mfa-secret-v1", state.config.csrf_secret)
}

fn encrypt_mfa_secret(secret: &str, state: &AppState) -> Result<String, AppError> {
    encrypt_mfa_secret_with_material(
        secret,
        &state.config.mfa_encryption_key,
        MFA_SECRET_PREFIX_CURRENT,
    )
}

fn decrypt_mfa_secret(stored: &str, state: &AppState) -> Result<String, AppError> {
    if !stored.contains(':') {
        return Ok(stored.to_string());
    }

    if stored.starts_with(&format!("{MFA_SECRET_PREFIX_CURRENT}:")) {
        let mut key_materials =
            Vec::with_capacity(1 + state.config.mfa_encryption_key_previous.len());
        key_materials.push(state.config.mfa_encryption_key.as_str());
        key_materials.extend(
            state
                .config
                .mfa_encryption_key_previous
                .iter()
                .map(String::as_str),
        );

        for key_material in key_materials {
            if let Ok(secret) =
                decrypt_mfa_secret_with_material(stored, key_material, MFA_SECRET_PREFIX_CURRENT)
            {
                return Ok(secret);
            }
        }

        return Err(AppError::InternalError {
            error: "server_error",
            message: "MFA secret could not be decrypted with configured keys".into(),
        });
    }

    if stored.starts_with(&format!("{MFA_SECRET_PREFIX_LEGACY}:")) {
        return decrypt_mfa_secret_with_material(
            stored,
            &legacy_mfa_key_material(state),
            MFA_SECRET_PREFIX_LEGACY,
        );
    }

    Ok(stored.to_string())
}

fn hotp(secret: &[u8], counter: u64) -> u32 {
    let mut msg = [0u8; 8];
    for i in 0..8 {
        msg[7 - i] = ((counter >> (i * 8)) & 0xff) as u8;
    }

    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret);
    let tag = hmac::sign(&key, &msg);
    let bytes = tag.as_ref();
    let offset = (bytes[19] & 0x0f) as usize;
    let binary = ((u32::from(bytes[offset]) & 0x7f) << 24)
        | (u32::from(bytes[offset + 1]) << 16)
        | (u32::from(bytes[offset + 2]) << 8)
        | u32::from(bytes[offset + 3]);
    binary % 1_000_000
}

fn verify_totp(secret_b32: &str, input_code: &str, now: chrono::DateTime<Utc>) -> bool {
    let code_trimmed = input_code.trim();
    if code_trimmed.len() != 6 || !code_trimmed.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let expected = match code_trimmed.parse::<u32>() {
        Ok(v) => v,
        Err(_) => return false,
    };

    let secret = match BASE32_NOPAD.decode(secret_b32.as_bytes()) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let step = 30_i64;
    let base_counter = now.timestamp().div_euclid(step);
    // Allow +/- 60s drift to tolerate small device clock skew.
    for drift in [-2_i64, -1, 0, 1, 2] {
        let counter = (base_counter + drift) as u64;
        if hotp(&secret, counter) == expected {
            return true;
        }
    }
    false
}

fn mfa_issuer() -> &'static str {
    "GateWASM"
}

fn otp_auth_uri(secret: &str, email: &str) -> String {
    let label = format!("{}:{}", mfa_issuer(), email);
    format!(
        "otpauth://totp/{}?secret={}&issuer={}&algorithm=SHA1&digits=6&period=30",
        urlencoding::encode(&label),
        secret,
        urlencoding::encode(mfa_issuer()),
    )
}

fn otp_auth_qr_data_url(uri: &str) -> Option<String> {
    let qr = QrCode::new(uri.as_bytes()).ok()?;
    let svg = qr
        .render::<svg::Color<'_>>()
        .min_dimensions(192, 192)
        .quiet_zone(true)
        .build();
    Some(format!(
        "data:image/svg+xml;base64,{}",
        STANDARD.encode(svg.as_bytes())
    ))
}

/// GET /auth/mfa/status — Returns current MFA setup + session status.
pub async fn status(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = claims_from_request(&req)?;

    let (mut mfa_enabled, mfa_secret): (bool, Option<String>) = sqlx::query_as(
        "SELECT COALESCE(mfa_enabled, FALSE), mfa_secret
         FROM users
         WHERE id = $1",
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or((false, None));

    let has_secret = mfa_secret
        .as_ref()
        .map(|secret| !secret.trim().is_empty())
        .unwrap_or(false);
    let mut recovery_required = false;
    if mfa_enabled && has_secret {
        if let Some(stored_secret) = mfa_secret.as_ref() {
            if decrypt_mfa_secret(stored_secret, &state).is_err() {
                tracing::warn!(
                    user_id = %claims.sub,
                    "Stored MFA secret is unreadable; forcing MFA recovery flow"
                );
                mfa_enabled = false;
                recovery_required = true;
            }
        }
    }

    let required =
        state.config.compliance.mfa_required || claims.roles.iter().any(|r| r == "platform_owner");

    Ok(Json(serde_json::json!({
        "mfa_enabled": mfa_enabled,
        "has_secret": has_secret,
        "recovery_required": recovery_required,
        "mfa_verified": claims.mfa_verified,
        "auth_method": claims.auth_method,
        "required": required,
    })))
}

/// POST /auth/mfa/setup — Generates/rotates TOTP secret for authenticated user.
pub async fn setup(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = claims_from_request(&req)?;
    let body: SetupMfaRequest = axum::Json::<SetupMfaRequest>::from_request(req, &state)
        .await
        .map(|j| j.0)
        .unwrap_or(SetupMfaRequest {
            rotate: None,
            recovery_password: None,
            recovery_via_sso: None,
        });

    let (email, existing_secret, mfa_enabled): (String, Option<String>, bool) = sqlx::query_as(
        "SELECT email, mfa_secret, COALESCE(mfa_enabled, FALSE)
         FROM users
         WHERE id = $1",
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized {
        error: "unauthorized",
        message: "User account not found".into(),
    })?;

    let existing_secret_plain = match existing_secret.as_ref() {
        Some(secret) if !secret.trim().is_empty() => match decrypt_mfa_secret(secret, &state) {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(
                    user_id = %claims.sub,
                    error = ?error,
                    "Stored MFA secret unreadable; recovering by provisioning a new secret"
                );
                None
            }
        },
        _ => None,
    };

    let rotate = body.rotate.unwrap_or(false);
    let needs_recovery = mfa_enabled && existing_secret_plain.is_none();

    if rotate && mfa_enabled && !claims.mfa_verified && !needs_recovery {
        let recovery_password = body
            .recovery_password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if let Some(password_input) = recovery_password {
            let stored_hash: Option<String> = sqlx::query_scalar(
                "SELECT hashed_password
                 FROM credentials
                 WHERE user_id = $1 AND provider = 'email'",
            )
            .bind(&claims.sub)
            .fetch_optional(&state.db)
            .await?;

            let stored_hash = stored_hash.ok_or(AppError::Forbidden {
                error: "mfa_recovery_password_unavailable",
                message: "Password-based MFA reset is not available for this account".into(),
            })?;

            let password_matches = password::verify_password(password_input, &stored_hash)?;
            if !password_matches {
                return Err(AppError::Unauthorized {
                    error: "invalid_credentials",
                    message: "Invalid account password for MFA reset".into(),
                });
            }
        } else if body.recovery_via_sso.unwrap_or(false) {
            let is_sso = claims.auth_method.eq_ignore_ascii_case("sso");
            let is_owner = claims.roles.iter().any(|role| role == "platform_owner");
            let token_age_seconds = Utc::now().timestamp().saturating_sub(claims.iat as i64);
            let is_recent_sso = token_age_seconds <= 600;

            if !is_sso || !is_owner {
                return Err(AppError::Forbidden {
                    error: "mfa_sso_recovery_not_allowed",
                    message: "SSO recovery requires an owner SSO session".into(),
                });
            }
            if !is_recent_sso {
                return Err(AppError::Forbidden {
                    error: "mfa_sso_reauth_required",
                    message: "Reauthenticate with SSO, then retry MFA reset".into(),
                });
            }
        } else {
            return Err(AppError::Forbidden {
                error: "mfa_reauth_required",
                message: "MFA rotation requires a recently MFA-verified session".into(),
            });
        }
    }

    if mfa_enabled && !rotate && !needs_recovery {
        return Ok(Json(serde_json::json!({
            "secret": serde_json::Value::Null,
            "otpauth_uri": serde_json::Value::Null,
            "mfa_enabled": true,
            "rotated": false,
            "already_configured": true,
            "recovered": false,
        })));
    }

    let secret = if !rotate && !needs_recovery {
        existing_secret_plain.unwrap_or_else(generate_totp_secret)
    } else {
        generate_totp_secret()
    };
    let encrypted_secret = encrypt_mfa_secret(&secret, &state)?;
    let force_disable_until_verify = rotate || needs_recovery;

    sqlx::query(
        "UPDATE users
         SET mfa_secret = $1,
             mfa_enabled = CASE WHEN $2 THEN FALSE ELSE mfa_enabled END,
             mfa_attempts = 0,
             mfa_locked_until = NULL,
             updated_at = NOW()
         WHERE id = $3",
    )
    .bind(&encrypted_secret)
    .bind(force_disable_until_verify)
    .bind(&claims.sub)
    .execute(&state.db)
    .await?;

    let uri = otp_auth_uri(&secret, &email);
    let qr_data_url = otp_auth_qr_data_url(&uri).ok_or(AppError::InternalError {
        error: "server_error",
        message: "Failed to generate MFA QR code".into(),
    })?;

    Ok(Json(serde_json::json!({
        "secret": secret,
        "otpauth_uri": uri,
        "otpauth_qr_data_url": qr_data_url,
        "mfa_enabled": if force_disable_until_verify {
            false
        } else {
            mfa_enabled
        },
        "rotated": rotate,
        "recovered": needs_recovery,
    })))
}

/// POST /auth/mfa/verify — Verifies TOTP and upgrades current session to MFA-verified.
pub async fn verify(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let claims = claims_from_request(&req)?;
    let tenant_ctx = req.extensions().get::<TenantContext>().cloned();
    let tenant_slug = tenant_ctx.as_ref().map(|t| t.slug.as_str());
    let tenant_id = claims.tenant_id.clone();
    let headers = req.headers().clone();

    let body: VerifyMfaRequest = axum::Json::<VerifyMfaRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body. Required: code".into(),
        })?
        .0;

    let (email, name, mfa_secret, mfa_attempts, mfa_locked_until): (
        String,
        Option<String>,
        Option<String>,
        i32,
        Option<chrono::DateTime<Utc>>,
    ) = sqlx::query_as(
        "SELECT email, name, mfa_secret, COALESCE(mfa_attempts, 0), mfa_locked_until
         FROM users
         WHERE id = $1",
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized {
        error: "unauthorized",
        message: "User account not found".into(),
    })?;

    let lockout_duration_minutes = state.config.compliance.lockout_duration_minutes.max(1);
    let max_failed_attempts =
        i32::try_from(state.config.compliance.max_failed_logins.max(1)).unwrap_or(5);

    if let Some(locked_until) = mfa_locked_until.filter(|ts| *ts > Utc::now()) {
        let retry_after = (locked_until - Utc::now()).num_seconds().max(1) as u64;
        return Err(AppError::AccountLocked {
            error: "mfa_locked",
            message: "Too many invalid MFA attempts. Try again later.".into(),
            retry_after,
        });
    }

    let secret = mfa_secret.ok_or(AppError::BadRequest {
        error: "mfa_not_configured",
        message: "MFA setup is required before verification".into(),
    })?;
    let secret = decrypt_mfa_secret(&secret, &state).map_err(|_| AppError::BadRequest {
        error: "mfa_recovery_required",
        message: "MFA needs to be reconfigured before verification".into(),
    })?;

    if !verify_totp(&secret, &body.code, Utc::now()) {
        let next_attempts = mfa_attempts.saturating_add(1);
        if next_attempts >= max_failed_attempts {
            let locked_until = Utc::now() + Duration::minutes(lockout_duration_minutes as i64);
            sqlx::query(
                "UPDATE users
                 SET mfa_attempts = 0,
                     mfa_locked_until = $1,
                     updated_at = NOW()
                 WHERE id = $2",
            )
            .bind(locked_until)
            .bind(&claims.sub)
            .execute(&state.db)
            .await?;

            return Err(AppError::AccountLocked {
                error: "mfa_locked",
                message: format!(
                    "Too many invalid MFA attempts. Locked for {} minute{}.",
                    lockout_duration_minutes,
                    if lockout_duration_minutes == 1 {
                        ""
                    } else {
                        "s"
                    },
                ),
                retry_after: (locked_until - Utc::now()).num_seconds().max(1) as u64,
            });
        }

        sqlx::query(
            "UPDATE users
             SET mfa_attempts = $1,
                 mfa_locked_until = NULL,
                 updated_at = NOW()
             WHERE id = $2",
        )
        .bind(next_attempts)
        .bind(&claims.sub)
        .execute(&state.db)
        .await?;

        return Err(AppError::Unauthorized {
            error: "invalid_mfa_code",
            message: "Invalid MFA code".into(),
        });
    }

    sqlx::query(
        "UPDATE users
         SET mfa_enabled = TRUE,
             mfa_attempts = 0,
             mfa_locked_until = NULL,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(&claims.sub)
    .execute(&state.db)
    .await?;

    let mut updated = 0_u64;
    let refresh_candidates = extract_refresh_cookie_candidates(&headers, tenant_slug);
    let candidate_count = refresh_candidates.len();
    if !refresh_candidates.is_empty() {
        let now = Utc::now();
        let refresh_hashes: Vec<String> = refresh_candidates
            .iter()
            .map(|token| signing::hash_token(token))
            .collect();
        updated = sqlx::query(
            "UPDATE refresh_tokens
             SET mfa_verified = TRUE
             WHERE token_hash = ANY($1)
               AND user_id = $2
               AND revoked = FALSE
               AND expires_at > $3",
        )
        .bind(&refresh_hashes)
        .bind(&claims.sub)
        .bind(now)
        .execute(&state.db)
        .await?
        .rows_affected();
    }

    if updated == 0 {
        sqlx::query(
            "UPDATE refresh_tokens
             SET mfa_verified = TRUE
             WHERE user_id = $1
               AND ($2::text IS NULL OR tenant_id = $2)
               AND revoked = FALSE
               AND expires_at > $3",
        )
        .bind(&claims.sub)
        .bind(&tenant_id)
        .bind(Utc::now())
        .execute(&state.db)
        .await?;
        tracing::warn!(
            user_id = %claims.sub,
            tenant_id = ?tenant_id,
            candidate_count,
            "MFA verified, but no matching refresh cookie hash found; applied user/tenant fallback update"
        );
    } else {
        tracing::info!(
            user_id = %claims.sub,
            tenant_id = ?tenant_id,
            candidate_count,
            updated,
            "MFA verified and refresh token(s) upgraded from cookie candidates"
        );
    }

    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT r.name
         FROM roles r
         JOIN user_roles ur ON ur.role_id = r.id
         WHERE ur.user_id = $1",
    )
    .bind(&claims.sub)
    .fetch_all(&state.db)
    .await
    .unwrap_or_else(|_| claims.roles.clone());

    let (access_token, expires_at) = signing::create_access_token(
        signing::AccessTokenInput {
            user_id: &claims.sub,
            email: &email,
            name: name.as_deref(),
            tenant_id: tenant_id.as_deref(),
            roles: &roles,
            auth_method: &claims.auth_method,
            mfa_verified: true,
            cnf: None,
        },
        &state.config,
        &state.keys,
    )?;

    // Keep verification sticky across token rotation for a bounded time window.
    let mfa_expires_at = Utc::now() + Duration::hours(24);
    sqlx::query(
        "UPDATE refresh_tokens
         SET mfa_verified = TRUE
         WHERE user_id = $1
           AND ($2::text IS NULL OR tenant_id = $2)
           AND revoked = FALSE
           AND expires_at > $3
           AND expires_at <= $4",
    )
    .bind(&claims.sub)
    .bind(&tenant_id)
    .bind(Utc::now())
    .bind(mfa_expires_at)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "verified": true,
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": state.config.access_token_expiry,
        "expires_at": expires_at,
    })))
}
