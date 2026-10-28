use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect},
};
use base64::Engine as _;
use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use subtle::ConstantTimeEq;

use crate::error::AppError;
use crate::handlers::auth::{ensure_platform_account_and_owner_role, normalize_email};
use crate::jwt::signing;
use crate::middleware::tenant::TenantContext;
use crate::types::*;
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

/// Per-tenant OAuth credentials fetched from DB (google_client_id, google_client_secret, github_client_id, github_client_secret).
type TenantOAuthCreds = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Get OAuth credentials for a provider — prefers tenant-specific creds, falls back to global config.
fn get_oauth_creds(
    provider: &str,
    tenant: Option<&TenantContext>,
    state: &AppState,
) -> Result<(String, String), AppError> {
    match provider {
        "google" => {
            // Try tenant-specific first
            if let Some(t) = tenant {
                if let (Some(id), Some(secret)) = (&t.google_client_id, &t.google_client_secret) {
                    if !id.is_empty() && !secret.is_empty() {
                        return Ok((id.clone(), secret.clone()));
                    }
                }
            }
            // Fall back to global config
            match (
                &state.config.google_client_id,
                &state.config.google_client_secret,
            ) {
                (Some(id), Some(secret)) => Ok((id.clone(), secret.clone())),
                _ => Err(AppError::BadRequest {
                    error: "provider_error",
                    message: "Google OAuth not configured".into(),
                }),
            }
        }
        "github" => {
            if let Some(t) = tenant {
                if let (Some(id), Some(secret)) = (&t.github_client_id, &t.github_client_secret) {
                    if !id.is_empty() && !secret.is_empty() {
                        return Ok((id.clone(), secret.clone()));
                    }
                }
            }
            match (
                &state.config.github_client_id,
                &state.config.github_client_secret,
            ) {
                (Some(id), Some(secret)) => Ok((id.clone(), secret.clone())),
                _ => Err(AppError::BadRequest {
                    error: "provider_error",
                    message: "GitHub OAuth not configured".into(),
                }),
            }
        }
        _ => Err(AppError::BadRequest {
            error: "provider_error",
            message: format!("Unsupported provider: {}", provider),
        }),
    }
}

pub async fn authorize(
    State(state): State<AppState>,
    Query(params): Query<AuthorizeQuery>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let tenant = req.extensions().get::<TenantContext>().cloned();
    let tenant_id = tenant.as_ref().map(|t| t.tenant_id.clone());

    // ── PAR request_uri handling ─────────────────────────────────────
    // If request_uri is present, look up the stored PAR record and use those
    // parameters instead of query params. The request_uri is single-use.
    let (client_id, redirect_uri_opt, response_type, state_param, code_challenge, code_challenge_method, provider, invite_token) =
        if let Some(ref request_uri) = params.request_uri {
            // Atomically consume the PAR request (single-use)
            let row = sqlx::query_as::<_, (String, serde_json::Value, String)>(
                "UPDATE par_requests SET consumed = TRUE \
                 WHERE request_uri = $1 AND consumed = FALSE AND expires_at > $2 \
                 RETURNING client_id, parameters, tenant_id",
            )
            .bind(request_uri)
            .bind(Utc::now())
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid, expired, or already-consumed request_uri".into(),
            })?;

            let (par_client_id, par_params, par_tenant_id) = row;

            // Verify client_id binding: the client_id in the query MUST match the one in PAR
            if params.client_id != par_client_id {
                return Err(AppError::BadRequest {
                    error: "invalid_request",
                    message: "client_id does not match the PAR request".into(),
                });
            }

            // Verify tenant_id binding
            if let Some(ref tid) = tenant_id {
                if *tid != par_tenant_id {
                    return Err(AppError::BadRequest {
                        error: "invalid_request",
                        message: "Tenant mismatch for PAR request".into(),
                    });
                }
            }

            // Extract stored parameters
            let p_client_id = par_params["client_id"].as_str().unwrap_or("").to_string();
            let p_redirect_uri = par_params["redirect_uri"].as_str().map(|s| s.to_string());
            let p_response_type = par_params["response_type"].as_str().unwrap_or("").to_string();
            let p_state = par_params["state"].as_str().unwrap_or("").to_string();
            let p_code_challenge = par_params["code_challenge"].as_str().unwrap_or("").to_string();
            let p_code_challenge_method = par_params["code_challenge_method"].as_str().unwrap_or("").to_string();
            let p_provider = par_params["provider"].as_str().unwrap_or("").to_string();
            let p_invite_token = par_params["invite_token"].as_str().map(|s| s.to_string());

            (p_client_id, p_redirect_uri, p_response_type, p_state, p_code_challenge, p_code_challenge_method, p_provider, p_invite_token)
        } else {
            // No request_uri — check if PAR is required for this tenant
            if let Some(ref t) = tenant {
                if t.security_config.par_required {
                    return Err(AppError::BadRequest {
                        error: "invalid_request",
                        message: "This tenant requires pushed authorization requests. Use the PAR endpoint first.".into(),
                    });
                }
            }

            // Use query params directly (existing flow)
            (
                params.client_id.clone(),
                params.redirect_uri.clone(),
                params.response_type.clone().unwrap_or_default(),
                params.state.clone().unwrap_or_default(),
                params.code_challenge.clone().unwrap_or_default(),
                params.code_challenge_method.clone().unwrap_or_default(),
                params.provider.clone().unwrap_or_default(),
                params.invite_token.clone(),
            )
        };

    let provider_str = provider.as_str();

    tracing::info!(
        "[authorize] provider={} client_id={} tenant_id={:?} par={}",
        provider_str,
        client_id,
        tenant_id,
        params.request_uri.is_some(),
    );

    // Check if tenant allows this provider
    if let Some(ref t) = tenant {
        if !t.allowed_providers.contains(&provider_str.to_string()) {
            return Err(AppError::BadRequest {
                error: "provider_not_allowed",
                message: format!("Provider '{}' is not enabled for this tenant", provider_str),
            });
        }
    }

    // Validate client_id against registered oauth_clients
    let registered_redirect = sqlx::query_scalar::<_, String>(
        "SELECT redirect_uri FROM oauth_clients WHERE id = $1 AND ($2::text IS NULL OR tenant_id = $2)",
    )
    .bind(&client_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| {
        tracing::warn!("[authorize] REJECTED: unknown client_id={}", client_id);
        AppError::BadRequest {
            error: "invalid_client",
            message: format!("Unknown client_id: {}", client_id),
        }
    })?;

    let client_redirect = redirect_uri_opt.as_deref().unwrap_or("");
    if client_redirect != registered_redirect {
        return Err(AppError::BadRequest {
            error: "invalid_redirect_uri",
            message: "redirect_uri does not match registered client".into(),
        });
    }

    if response_type != "code" {
        return Err(AppError::BadRequest {
            error: "unsupported_response_type",
            message: "Only response_type=code is supported".into(),
        });
    }
    if code_challenge_method != "S256" {
        return Err(AppError::BadRequest {
            error: "invalid_request",
            message: "Only code_challenge_method=S256 is supported".into(),
        });
    }

    if provider_str == "email" {
        let tenant = tenant.as_ref().ok_or(AppError::BadRequest { error: "tenant_required", message: "Hosted code sign-in requires a tenant.".into() })?;
        return super::hosted_code::begin(&state, tenant, &client_id, client_redirect, &state_param, &code_challenge).await;
    }
    let (provider_client_id, _) = get_oauth_creds(provider_str, tenant.as_ref(), &state)?;

    let expires_at = Utc::now() + Duration::minutes(10);

    let mut mac = HmacSha256::new_from_slice(state.config.csrf_secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(state_param.as_bytes());
    let hmac_tag =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    let invite_token_hash = invite_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(signing::hash_token);

    sqlx::query(
        "INSERT INTO oauth_states (state, code_challenge, provider, redirect_uri, client_id, expires_at, hmac_tag, tenant_id, invite_token_hash) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&state_param)
    .bind(&code_challenge)
    .bind(provider_str)
    .bind(client_redirect)
    .bind(&client_id)
    .bind(expires_at)
    .bind(&hmac_tag)
    .bind(&tenant_id)
    .bind(&invite_token_hash)
    .execute(&state.db)
    .await?;

    // Use a single stable OAuth callback URL for provider console configuration.
    // Tenant context is recovered from oauth_states. This avoids redirect_uri_mismatch
    // when provider apps are configured only for /auth/callback/{provider}.
    let server_callback = format!("{}/auth/callback/{}", state.config.server_url, provider_str);

    let url = match provider_str {
        "google" => format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&prompt=select_account",
            provider_client_id,
            urlencoding::encode(&server_callback),
            urlencoding::encode("openid email profile"),
            urlencoding::encode(&state_param),
        ),
        "github" => format!(
            "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope={}&state={}",
            provider_client_id,
            urlencoding::encode(&server_callback),
            urlencoding::encode("read:user user:email"),
            urlencoding::encode(&state_param),
        ),
        _ => unreachable!(),
    };

    Ok(Redirect::temporary(&url).into_response())
}

pub async fn callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(
        "[callback] provider={} params={:?}",
        provider,
        params.keys().collect::<Vec<_>>()
    );

    let returned_state = params.get("state").cloned().ok_or(AppError::BadRequest {
        error: "invalid_state",
        message: "Missing state parameter".into(),
    })?;

    // Look up stored OAuth state (including HMAC tag and tenant_id)
    let oauth_row = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT code_challenge, redirect_uri, client_id, hmac_tag, tenant_id, invite_token_hash FROM oauth_states WHERE state = $1 AND provider = $2 AND expires_at > $3",
    )
    .bind(&returned_state)
    .bind(&provider)
    .bind(Utc::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| {
        AppError::BadRequest {
            error: "invalid_state",
            message: "Invalid or expired OAuth state".into(),
        }
    })?;

    let (
        stored_code_challenge,
        client_redirect_uri,
        stored_client_id,
        stored_hmac,
        tenant_id,
        invite_token_hash,
    ) = oauth_row;

    // Verify CSRF HMAC tag
    if let Some(stored_hmac) = &stored_hmac {
        let mut mac = HmacSha256::new_from_slice(state.config.csrf_secret.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(returned_state.as_bytes());
        let computed_hmac =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        if computed_hmac.as_bytes().ct_eq(stored_hmac.as_bytes()).unwrap_u8() != 1 {
            return Err(AppError::BadRequest {
                error: "invalid_state",
                message: "CSRF verification failed".into(),
            });
        }
    }

    // Check for provider error
    if let Some(error) = params.get("error") {
        let desc = params
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| error.clone());
        if client_redirect_uri.is_empty() {
            return Err(AppError::BadRequest {
                error: "oauth_error",
                message: desc,
            });
        }
        let separator = if client_redirect_uri.contains('?') {
            "&"
        } else {
            "?"
        };
        return Ok(Redirect::temporary(&format!(
            "{}{}error={}&error_description={}&state={}",
            client_redirect_uri,
            separator,
            urlencoding::encode(error),
            urlencoding::encode(&desc),
            urlencoding::encode(&returned_state),
        ))
        .into_response());
    }

    let code = params.get("code").cloned().ok_or(AppError::BadRequest {
        error: "invalid_request",
        message: "Missing authorization code".into(),
    })?;

    // Must match authorize() callback exactly.
    let server_callback = format!("{}/auth/callback/{}", state.config.server_url, provider);
    let http_client = reqwest::Client::new();

    // Resolve OAuth credentials (tenant-specific or global) and tenant signup mode.
    let (tenant_ctx, tenant_signup_mode) = if let Some(ref tid) = tenant_id {
        let row = sqlx::query_as::<
            _,
            (
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                String,
            ),
        >(
            "SELECT google_client_id, google_client_secret, github_client_id, github_client_secret, COALESCE(signup_mode, 'open') FROM tenants WHERE id = $1",
        )
        .bind(tid)
        .fetch_optional(&state.db)
        .await?;

        if let Some((google_id, google_secret, github_id, github_secret, signup_mode)) = row {
            (
                Some((google_id, google_secret, github_id, github_secret)),
                signup_mode,
            )
        } else {
            (None, "open".to_string())
        }
    } else {
        (None, "open".to_string())
    };

    let (provider_user_id, user_email, user_name, user_avatar) = match provider.as_str() {
        "google" => {
            let (client_id, client_secret) = resolve_provider_creds("google", &tenant_ctx, &state)?;
            fetch_google_user(
                &client_id,
                &client_secret,
                &http_client,
                &code,
                &server_callback,
            )
            .await?
        }
        "github" => {
            let (client_id, client_secret) = resolve_provider_creds("github", &tenant_ctx, &state)?;
            fetch_github_user(&client_id, &client_secret, &http_client, &code).await?
        }
        _ => {
            return Err(AppError::BadRequest {
                error: "provider_error",
                message: format!("Unsupported provider: {}", provider),
            });
        }
    };

    let user_email = normalize_email(&user_email);

    let internal_code = format!("gc_{}", uuid::Uuid::new_v4());
    let code_expires = Utc::now() + Duration::minutes(5);

    let mut tx = state.db.begin().await?;

    // Find or create user (tenant-scoped)
    let existing_uid = sqlx::query_scalar::<_, String>(
        "SELECT user_id FROM credentials WHERE provider = $1 AND provider_user_id = $2 AND ($3::text IS NULL OR tenant_id = $3)",
    )
    .bind(&provider)
    .bind(&provider_user_id)
    .bind(&tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let existing_by_email = if existing_uid.is_none() {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM users WHERE email = $1 AND ($2::text IS NULL OR tenant_id = $2)",
        )
        .bind(&user_email)
        .bind(&tenant_id)
        .fetch_optional(&mut *tx)
        .await?
    } else {
        None
    };

    let is_new_user = existing_uid.is_none() && existing_by_email.is_none();
    if is_new_user && tenant_signup_mode == "invite_only" && invite_token_hash.is_none() {
        return Err(AppError::BadRequest {
            error: "invite_required",
            message: "This tenant is invite-only. Use an invitation link to create an account."
                .into(),
        });
    }

    let mut invite_id_to_accept: Option<String> = None;
    let mut invite_role_to_assign: Option<String> = None;
    let mut invite_accepted_by_user_id: Option<String> = None;

    if let Some(ref token_hash) = invite_token_hash {
        let tid = tenant_id.as_deref().ok_or(AppError::BadRequest {
            error: "invalid_invite_token",
            message: "Invite links are only valid for tenant-scoped login".into(),
        })?;
        let invite = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                chrono::DateTime<Utc>,
                Option<String>,
            ),
        >(
            "SELECT id, email, role_id, status, expires_at, accepted_by_user_id
             FROM tenant_user_invites
             WHERE tenant_id = $1
               AND token_hash = $2
             LIMIT 1
             FOR UPDATE",
        )
        .bind(tid)
        .bind(token_hash)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::BadRequest {
            error: "invalid_invite_token",
            message: "Invite not found".into(),
        })?;

        let (invite_id, invite_email, role_id, status, expires_at, accepted_by) = invite;
        if normalize_email(&invite_email) != user_email {
            return Err(AppError::Forbidden {
                error: "invite_email_mismatch",
                message: "Invite email does not match the OAuth account".into(),
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
                "UPDATE tenant_user_invites
                 SET status = 'expired', updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(&invite_id)
            .execute(&mut *tx)
            .await;
            return Err(AppError::BadRequest {
                error: "invite_expired",
                message: "This invite has expired".into(),
            });
        }

        if status == "accepted" {
            invite_accepted_by_user_id = accepted_by;
        } else if status == "pending" {
            invite_id_to_accept = Some(invite_id);
        } else {
            return Err(AppError::BadRequest {
                error: "invalid_invite_state",
                message: "Invite is not in a usable state".into(),
            });
        }

        invite_role_to_assign = Some(role_id);
    }

    let mut needs_credential_insert = false;
    let user_id = if let Some(uid) = existing_uid {
        sqlx::query(
            "UPDATE users SET name = COALESCE($1, name), avatar = COALESCE($2, avatar), updated_at = NOW() WHERE id = $3",
        )
        .bind(&user_name)
        .bind(&user_avatar)
        .bind(&uid)
        .execute(&mut *tx)
        .await?;
        uid
    } else {
        needs_credential_insert = true;
        if let Some(existing_uid) = existing_by_email {
            // Auto-link: Google and GitHub both verify email, so it's safe to
            // add the OAuth credential to an existing account with the same email.
            // Update name/avatar if the existing user doesn't have them.
            sqlx::query(
                "UPDATE users SET name = COALESCE(NULLIF(name, ''), $1), avatar = COALESCE(avatar, $2), updated_at = NOW() WHERE id = $3",
            )
            .bind(&user_name)
            .bind(&user_avatar)
            .bind(&existing_uid)
            .execute(&mut *tx)
            .await?;
            existing_uid
        } else {
            let new_uid = format!("usr_{}", uuid::Uuid::new_v4());
            sqlx::query(
                "INSERT INTO users (id, email, name, avatar, email_verified, tenant_id) VALUES ($1, $2, $3, $4, TRUE, $5)",
            )
            .bind(&new_uid)
            .bind(&user_email)
            .bind(&user_name)
            .bind(&user_avatar)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;

            let role_id = invite_role_to_assign.as_deref().unwrap_or("role_user");
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, tenant_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(&new_uid)
            .bind(role_id)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;

            new_uid
        }
    };

    if needs_credential_insert {
        sqlx::query(
            "INSERT INTO credentials (id, user_id, provider, provider_user_id, tenant_id) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(format!("cred_{}", uuid::Uuid::new_v4()))
        .bind(&user_id)
        .bind(&provider)
        .bind(&provider_user_id)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }

    if !is_new_user {
        if let Some(role_id) = invite_role_to_assign.as_deref() {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, tenant_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(&user_id)
            .bind(role_id)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    if let Some(invite_id) = invite_id_to_accept.as_deref() {
        let accepted = sqlx::query(
            "UPDATE tenant_user_invites
             SET status = 'accepted', accepted_by_user_id = $1, accepted_at = NOW(), updated_at = NOW()
             WHERE id = $2
               AND status = 'pending'",
        )
        .bind(&user_id)
        .bind(invite_id)
        .execute(&mut *tx)
        .await?;
        if accepted.rows_affected() == 0 {
            return Err(AppError::Conflict {
                error: "invite_conflict",
                message: "Invite could not be accepted; please request a new invite".into(),
            });
        }
    } else if let Some(accepted_by) = invite_accepted_by_user_id.as_deref() {
        if accepted_by != user_id.as_str() {
            return Err(AppError::Forbidden {
                error: "invite_already_used",
                message: "This invite was already accepted by another account".into(),
            });
        }
    }

    sqlx::query(
        "INSERT INTO auth_codes (code, user_id, code_challenge, redirect_uri, client_id, provider, expires_at, tenant_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&internal_code)
    .bind(&user_id)
    .bind(&stored_code_challenge)
    .bind(&client_redirect_uri)
    .bind(&stored_client_id)
    .bind(&provider)
    .bind(code_expires)
    .bind(&tenant_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM oauth_states WHERE state = $1")
        .bind(&returned_state)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    if let Err(e) =
        ensure_platform_account_and_owner_role(&state, &user_id, &user_email, tenant_id.as_deref())
            .await
    {
        tracing::warn!(
            user_id = %user_id,
            email = %user_email,
            error = %e,
            "Failed to ensure platform account/owner role after OAuth callback"
        );
    }

    let redirect_url = if client_redirect_uri.is_empty() {
        format!(
            "/?code={}&state={}",
            urlencoding::encode(&internal_code),
            urlencoding::encode(&returned_state)
        )
    } else {
        let separator = if client_redirect_uri.contains('?') {
            "&"
        } else {
            "?"
        };
        format!(
            "{}{}code={}&state={}",
            client_redirect_uri,
            separator,
            urlencoding::encode(&internal_code),
            urlencoding::encode(&returned_state)
        )
    };

    Ok(Redirect::temporary(&redirect_url).into_response())
}

/// Resolve provider credentials from tenant DB row or global config.
fn resolve_provider_creds(
    provider: &str,
    tenant_row: &Option<TenantOAuthCreds>,
    state: &AppState,
) -> Result<(String, String), AppError> {
    if let Some((g_id, g_secret, gh_id, gh_secret)) = tenant_row {
        match provider {
            "google" => {
                if let (Some(id), Some(secret)) = (g_id, g_secret) {
                    if !id.is_empty() && !secret.is_empty() {
                        return Ok((id.clone(), secret.clone()));
                    }
                }
            }
            "github" => {
                if let (Some(id), Some(secret)) = (gh_id, gh_secret) {
                    if !id.is_empty() && !secret.is_empty() {
                        return Ok((id.clone(), secret.clone()));
                    }
                }
            }
            _ => {}
        }
    }

    // Fall back to global
    match provider {
        "google" => match (
            &state.config.google_client_id,
            &state.config.google_client_secret,
        ) {
            (Some(id), Some(secret)) => Ok((id.clone(), secret.clone())),
            _ => Err(AppError::InternalError {
                error: "server_error",
                message: "Google OAuth not configured".into(),
            }),
        },
        "github" => match (
            &state.config.github_client_id,
            &state.config.github_client_secret,
        ) {
            (Some(id), Some(secret)) => Ok((id.clone(), secret.clone())),
            _ => Err(AppError::InternalError {
                error: "server_error",
                message: "GitHub OAuth not configured".into(),
            }),
        },
        _ => Err(AppError::BadRequest {
            error: "provider_error",
            message: format!("Unsupported provider: {}", provider),
        }),
    }
}

async fn fetch_google_user(
    client_id: &str,
    client_secret: &str,
    http: &reqwest::Client,
    code: &str,
    redirect_uri: &str,
) -> Result<(String, String, Option<String>, Option<String>), AppError> {
    let token_res = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Google token exchange failed");
            AppError::BadGateway {
                error: "provider_error",
                message: "Failed to exchange code with Google".into(),
            }
        })?;

    if !token_res.status().is_success() {
        let body = token_res.text().await.unwrap_or_default();
        tracing::error!(body = %body, "Google token exchange error response");
        return Err(AppError::BadGateway {
            error: "provider_error",
            message: "Google rejected the authorization code".into(),
        });
    }

    let google_tokens: GoogleTokenResponse = token_res.json().await.map_err(|e| {
        tracing::error!(error = %e, "Failed to parse Google token response");
        AppError::BadGateway {
            error: "provider_error",
            message: "Invalid response from Google".into(),
        }
    })?;

    // Always use Google userinfo API for user data.
    // The id_token contains user claims but verifying its RS256 signature
    // requires fetching Google's JWKS and rotating keys — not worth the
    // complexity when the userinfo endpoint is one HTTP call away and
    // already authenticated via the access_token we just obtained.
    let user_info: GoogleUserInfo = http
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&google_tokens.access_token)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Google userinfo failed");
            AppError::BadGateway {
                error: "provider_error",
                message: "Failed to fetch user info from Google".into(),
            }
        })?
        .json()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Google userinfo parse failed");
            AppError::BadGateway {
                error: "provider_error",
                message: "Failed to get user info from Google".into(),
            }
        })?;

    Ok((
        user_info.id,
        user_info.email,
        user_info.name,
        user_info.picture,
    ))
}

async fn fetch_github_user(
    client_id: &str,
    client_secret: &str,
    http: &reqwest::Client,
    code: &str,
) -> Result<(String, String, Option<String>, Option<String>), AppError> {
    let token_res = http
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": client_id,
            "client_secret": client_secret,
            "code": code,
        }))
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "GitHub token exchange failed");
            AppError::BadGateway {
                error: "provider_error",
                message: "Failed to exchange code with GitHub".into(),
            }
        })?;

    if !token_res.status().is_success() {
        return Err(AppError::BadGateway {
            error: "provider_error",
            message: "GitHub rejected the authorization code".into(),
        });
    }

    let github_tokens: GitHubTokenResponse =
        token_res.json().await.map_err(|_| AppError::BadGateway {
            error: "provider_error",
            message: "Invalid response from GitHub".into(),
        })?;

    let user_info: GitHubUserInfo = http
        .get("https://api.github.com/user")
        .bearer_auth(&github_tokens.access_token)
        .header("User-Agent", "fAuth-Server")
        .send()
        .await
        .map_err(|_| AppError::BadGateway {
            error: "provider_error",
            message: "Failed to fetch user info from GitHub".into(),
        })?
        .json()
        .await
        .map_err(|_| AppError::BadGateway {
            error: "provider_error",
            message: "Failed to get user info from GitHub".into(),
        })?;

    let email = if let Some(email) = user_info.email {
        email
    } else {
        let emails_res = http
            .get("https://api.github.com/user/emails")
            .bearer_auth(&github_tokens.access_token)
            .header("User-Agent", "fAuth-Server")
            .send()
            .await;

        let emails: Vec<GitHubEmail> = match emails_res {
            Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
            _ => vec![],
        };

        emails
            .iter()
            .find(|e| e.primary && e.verified)
            .map(|e| e.email.clone())
            .ok_or(AppError::BadRequest {
                error: "email_required",
                message: "Could not retrieve a verified email from GitHub.".into(),
            })?
    };

    Ok((
        user_info.id.to_string(),
        email,
        user_info.name,
        user_info.avatar_url,
    ))
}
