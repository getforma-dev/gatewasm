use axum::{
    extract::{FromRequest, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    Json,
};
use chrono::{Duration, Utc};
use serde::Deserialize;

use sqlx::Row;

use crate::error::AppError;
use crate::frontend::FrontendSnapshot;
use crate::handlers::management::is_valid_css_color;
use crate::html_template::{render_page, PageConfig, PageOutput};
use crate::ir;
use crate::jwt::signing;
use crate::middleware::platform_user::PlatformUser;
use crate::redirects::{
    default_login_redirect, resolve_login_redirect, validate_login_redirect_input,
};
use crate::{AppState, RenderMode};

/// Slugs reserved for system routes — cannot be used as tenant slugs.
pub const RESERVED_SLUGS: &[&str] = &[
    "platform",
    "api",
    "admin",
    "manage",
    "auth",
    "health",
    "docs",
    "www",
    "app",
    "dashboard",
    "billing",
    "support",
    "login",
    "register",
    "signup",
    "get-started",
    "bridge",
    "openapi",
    "t",
    "static",
    "assets",
];

const PLATFORM_LOGIN_CLIENT_ID: &str = "platform-login";
const PLATFORM_ONBOARDING_CLIENT_ID: &str = "platform-onboarding";

/// Slot ID constants — must match compiler output order.
const SLOT_OAUTH_PROVIDERS: u16 = 0;

/// Populate onboarding slot data from IR module defaults + server-side overrides.
fn populate_onboarding_slots(state: &AppState, ir_module: &ir::parser::IrModule) -> ir::slot::SlotData {
    let mut slots = ir::slot::SlotData::new_from_defaults(&ir_module.slots);

    let google = state.config.google_client_id.is_some()
        && state.config.google_client_secret.is_some();
    let github = state.config.github_client_id.is_some()
        && state.config.github_client_secret.is_some();

    let mut providers = Vec::new();
    if google {
        providers.push(ir::slot::SlotValue::Object(vec![
            ("provider".into(), ir::slot::SlotValue::Text("google".into())),
            ("label".into(), ir::slot::SlotValue::Text("Continue with Google".into())),
        ]));
    }
    if github {
        providers.push(ir::slot::SlotValue::Object(vec![
            ("provider".into(), ir::slot::SlotValue::Text("github".into())),
            ("label".into(), ir::slot::SlotValue::Text("Continue with GitHub".into())),
        ]));
    }
    slots.set(SLOT_OAUTH_PROVIDERS, ir::slot::SlotValue::Array(providers));

    slots
}

fn render_get_started_page_html(state: &AppState) -> PageOutput {
    let frontend = FrontendSnapshot::capture(state);
    let google_enabled = state.config.google_client_id.is_some()
        && state.config.google_client_secret.is_some();
    let github_enabled = state.config.github_client_id.is_some()
        && state.config.github_client_secret.is_some();

    let config_script = format!(
        "window.__FORMA_SERVER_URL__=\"{}\";window.__PLATFORM_CONFIG__={};",
        state.config.server_url,
        serde_json::json!({
            "google_enabled": google_enabled,
            "github_enabled": github_enabled,
            "client_id": PLATFORM_ONBOARDING_CLIENT_ID,
        }),
    );

    let route_pattern = "/platform/onboarding";
    let render_mode = frontend.render_mode(route_pattern);
    let ir_module = frontend.ir_module(route_pattern);

    // Populate slot data from IR module defaults + server overrides
    let slots = if render_mode == RenderMode::Phase2SsrReconcile {
        ir_module.map(|m| populate_onboarding_slots(state, m))
    } else {
        None
    };

    render_page(&PageConfig {
        title: "Get Started — Forma Auth",
        route_pattern,
        manifest: frontend.manifest(),
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;--grid-opacity:0.15;--glow-size:1100px;--glow-intensity:0.10;}"),
        body_prefix: None,
        render_mode,
        ir_module,
        slots: slots.as_ref(),
    })
}

fn render_platform_login_page_html(state: &AppState) -> PageOutput {
    let frontend = FrontendSnapshot::capture(state);
    let google_enabled = state.config.google_client_id.is_some()
        && state.config.google_client_secret.is_some();
    let github_enabled = state.config.github_client_id.is_some()
        && state.config.github_client_secret.is_some();

    let config_script = format!(
        "window.__FORMA_SERVER_URL__=\"{}\";window.__PLATFORM_CONFIG__={};",
        state.config.server_url,
        serde_json::json!({
            "google_enabled": google_enabled,
            "github_enabled": github_enabled,
            "client_id": PLATFORM_LOGIN_CLIENT_ID,
        }),
    );

    let route_pattern = "/platform/login";
    let render_mode = frontend.render_mode(route_pattern);
    let ir_module = frontend.ir_module(route_pattern);

    // Populate slot data from IR module defaults + server overrides
    let slots = if render_mode == RenderMode::Phase2SsrReconcile {
        ir_module.map(|m| populate_onboarding_slots(state, m))
    } else {
        None
    };

    render_page(&PageConfig {
        title: "Platform Login — Forma Auth",
        route_pattern,
        manifest: frontend.manifest(),
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some("body{background:#0a0a0c;color:#e8e6e3;--grid-opacity:0.25;--glow-size:900px;--glow-intensity:0.07;}"),
        body_prefix: None,
        render_mode,
        ir_module,
        slots: slots.as_ref(),
    })
}

pub fn is_reserved_slug(slug: &str) -> bool {
    RESERVED_SLUGS.contains(&slug)
}

async fn ensure_platform_oauth_clients(state: &AppState) -> Result<(), AppError> {
    let login_redirect = format!("{}/platform/login", state.config.server_url);
    let onboarding_redirect = format!("{}/platform/onboarding", state.config.server_url);

    sqlx::query(
        "INSERT INTO oauth_clients (id, redirect_uri, name, tenant_id)
         VALUES ($1, $2, $3, 'ten_platform')
         ON CONFLICT (id) DO UPDATE SET redirect_uri = EXCLUDED.redirect_uri, tenant_id = 'ten_platform'",
    )
    .bind(PLATFORM_LOGIN_CLIENT_ID)
    .bind(login_redirect)
    .bind("Platform Login")
    .execute(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO oauth_clients (id, redirect_uri, name, tenant_id)
         VALUES ($1, $2, $3, 'ten_platform')
         ON CONFLICT (id) DO UPDATE SET redirect_uri = EXCLUDED.redirect_uri, tenant_id = 'ten_platform'",
    )
    .bind(PLATFORM_ONBOARDING_CLIENT_ID)
    .bind(onboarding_redirect)
    .bind("Platform Onboarding")
    .execute(&state.db)
    .await?;

    Ok(())
}

fn validate_slug(slug: &str) -> Result<String, AppError> {
    let slug = slug.trim().to_lowercase();
    if slug.is_empty()
        || slug.len() > 63
        || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AppError::BadRequest {
            error: "invalid_slug",
            message: "Slug must be 1-63 characters, lowercase alphanumeric or hyphens".into(),
        });
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(AppError::BadRequest {
            error: "invalid_slug",
            message: "Slug cannot start or end with a hyphen".into(),
        });
    }
    if is_reserved_slug(&slug) {
        return Err(AppError::BadRequest {
            error: "reserved_slug",
            message: format!("The slug '{}' is reserved and cannot be used", slug),
        });
    }
    Ok(slug)
}

// ── Self-Serve API ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub allowed_providers: Option<Vec<String>>,
    pub branding_logo_url: Option<String>,
    pub branding_primary_color: Option<String>,
    pub branding_app_name: Option<String>,
    pub login_redirect_url: Option<String>,
    pub allowed_origins: Option<Vec<String>>,
    pub onboarding_completed: Option<bool>,
    pub onboarding_step: Option<i16>,
    pub preferred_tab: Option<String>,
    /// Alias: frontend sends `primary_color`, maps to `branding_primary_color`
    pub primary_color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteTenantRequest {
    pub confirm_slug: String,
}

#[derive(Debug, Deserialize)]
pub struct TenantStatsQuery {
    pub window: Option<String>,
}

fn normalize_tenant_stats_window(raw: Option<&str>) -> &'static str {
    match raw {
        Some("24h") => "24h",
        Some("30d") => "30d",
        _ => "7d",
    }
}

/// POST /platform/self-serve/tenants — Create a new tenant (authenticated platform user).
pub async fn create_tenant(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    let body: CreateTenantRequest = axum::Json::<CreateTenantRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body. Required: name, slug".into(),
        })?
        .0;

    let slug = validate_slug(&body.slug)?;

    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_name",
            message: "Tenant name is required".into(),
        });
    }

    let tenant_id = format!("ten_{}", uuid::Uuid::new_v4());
    let api_key = format!("mgmt_{}", uuid::Uuid::new_v4());
    let api_key_prefix = &api_key[..api_key.len().min(13)];
    let api_key_hash = signing::hash_token(&api_key);
    let trial_ends_at = Utc::now() + Duration::days(14);

    let default_login_redirect = default_login_redirect(&state.config.server_url, &slug);

    // Copy password hash from platform tenant credentials (read before transaction)
    let platform_hash = sqlx::query_scalar::<_, String>(
        "SELECT hashed_password FROM credentials WHERE user_id = $1 AND provider = 'email' LIMIT 1",
    )
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let owner_name = user.name.as_deref().unwrap_or("");
    let admin_user_id = format!("usr_{}", uuid::Uuid::new_v4());
    let admin_cred_id = format!("cred_{}", uuid::Uuid::new_v4());

    // ── Atomic transaction: tenant + entitlements + admin user ────────
    let mut tx = state.db.begin().await?;

    // Atomic slug uniqueness: INSERT with ON CONFLICT to avoid SELECT-then-INSERT race
    let result = sqlx::query(
        "INSERT INTO tenants (id, name, slug, owner_email, api_key_hash, api_key_prefix, \
         allowed_providers, max_users, plan, trial_ends_at, owner_user_id, login_redirect_url) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
         ON CONFLICT (slug) DO NOTHING",
    )
    .bind(&tenant_id)
    .bind(body.name.trim())
    .bind(&slug)
    .bind(&user.email)
    .bind(&api_key_hash)
    .bind(api_key_prefix)
    .bind(serde_json::json!(["email"]))
    .bind(100_i32) // trial limit
    .bind("trial")
    .bind(trial_ends_at)
    .bind(&user.user_id)
    .bind(&default_login_redirect)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict {
            error: "slug_taken",
            message: format!("Tenant slug '{}' is already in use", slug),
        });
    }

    sqlx::query(
        "INSERT INTO tenant_entitlements \
         (tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations) \
         VALUES ($1, 'trial', 100, 3, TRUE, TRUE, TRUE) \
         ON CONFLICT (tenant_id) DO NOTHING",
    )
    .bind(&tenant_id)
    .execute(&mut *tx)
    .await?;

    // ── Auto-create owner as admin on new tenant ─────────────
    sqlx::query(
        "INSERT INTO users (id, email, name, email_verified, tenant_id) VALUES ($1, $2, $3, TRUE, $4)",
    )
    .bind(&admin_user_id)
    .bind(&user.email)
    .bind(owner_name)
    .bind(&tenant_id)
    .execute(&mut *tx)
    .await?;

    if let Some(ref hash) = platform_hash {
        sqlx::query(
            "INSERT INTO credentials (id, user_id, provider, provider_user_id, hashed_password, tenant_id) \
             VALUES ($1, $2, 'email', $3, $4, $5)",
        )
        .bind(&admin_cred_id)
        .bind(&admin_user_id)
        .bind(&user.email)
        .bind(hash)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id) VALUES ($1, 'role_admin', $2) ON CONFLICT DO NOTHING",
    )
    .bind(&admin_user_id)
    .bind(&tenant_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let row = sqlx::query(
        "SELECT created_at, onboarding_completed, onboarding_step, preferred_tab, branding_primary_color \
         FROM tenants WHERE id = $1",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(
        tenant_id = %tenant_id,
        slug = %slug,
        owner = %user.email,
        owner_user_id = %user.user_id,
        admin_user_id = %admin_user_id,
        "Self-serve tenant created with owner as admin"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "tenant": {
                "id": tenant_id,
                "name": body.name.trim(),
                "slug": slug,
                "owner_email": user.email,
                "allowed_providers": ["email"],
                "max_users": 100,
                "plan": "trial",
                "trial_ends_at": trial_ends_at.to_rfc3339(),
                "status": "active",
                "onboarding_completed": row.get::<bool, _>("onboarding_completed"),
                "onboarding_step": row.get::<i16, _>("onboarding_step"),
                "preferred_tab": row.get::<Option<String>, _>("preferred_tab"),
                "primary_color": row.get::<Option<String>, _>("branding_primary_color"),
                "login_redirect_url": default_login_redirect,
                "created_at": row.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
            },
            "api_key": api_key,
            "dashboard_url": format!("/t/{}/site", slug),
            "admin_url": format!("/t/{}/admin", slug),
            "note": "Save this API key in backend secrets only — it will not be shown again. You are already admin on this tenant — use the admin URL for tenant management."
        })),
    ))
}

/// GET /platform/self-serve/tenants — List tenants owned by the authenticated user.
pub async fn list_my_tenants(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    let rows = sqlx::query(
        "SELECT id, name, slug, status, allowed_providers, max_users, \
         COALESCE(plan, 'trial') AS plan, trial_ends_at, created_at, \
         onboarding_completed, onboarding_step, preferred_tab, \
         branding_primary_color, login_redirect_url \
         FROM tenants WHERE owner_user_id = $1 AND status != 'deleted' \
         ORDER BY created_at DESC",
    )
    .bind(&user.user_id)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            let slug: String = row.get("slug");
            let redirect_url: Option<String> = row.get("login_redirect_url");
            let resolved_redirect =
                resolve_login_redirect(&state.config.server_url, &slug, redirect_url.as_deref());
            serde_json::json!({
                "id": row.get::<String, _>("id"),
                "name": row.get::<String, _>("name"),
                "slug": slug,
                "status": row.get::<String, _>("status"),
                "allowed_providers": row.get::<serde_json::Value, _>("allowed_providers"),
                "max_users": row.get::<i32, _>("max_users"),
                "plan": row.get::<String, _>("plan"),
                "trial_ends_at": row.get::<Option<chrono::DateTime<Utc>>, _>("trial_ends_at").map(|t| t.to_rfc3339()),
                "created_at": row.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
                "onboarding_completed": row.get::<bool, _>("onboarding_completed"),
                "onboarding_step": row.get::<i16, _>("onboarding_step"),
                "preferred_tab": row.get::<Option<String>, _>("preferred_tab"),
                "primary_color": row.get::<Option<String>, _>("branding_primary_color"),
                "login_redirect_url": resolved_redirect,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "tenants": items,
    })))
}

/// GET /platform/self-serve/tenants/{id} — Get a tenant owned by the authenticated user.
pub async fn get_my_tenant(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    let row = sqlx::query(
        "SELECT id, name, slug, owner_email, status, allowed_providers, allowed_origins, \
         max_users, COALESCE(plan, 'trial') AS plan, trial_ends_at, \
         branding_logo_url, branding_primary_color, branding_app_name, \
         login_redirect_url, \
         onboarding_completed, onboarding_step, preferred_tab, \
         created_at, updated_at \
         FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found or you don't have access".into(),
    })?;

    let id: String = row.get("id");
    let slug: String = row.get("slug");
    let redirect_url: Option<String> = row.get("login_redirect_url");
    let resolved_login_redirect =
        resolve_login_redirect(&state.config.server_url, &slug, redirect_url.as_deref());

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    Ok(Json(serde_json::json!({
        "id": id,
        "name": row.get::<String, _>("name"),
        "slug": slug,
        "owner_email": row.get::<String, _>("owner_email"),
        "status": row.get::<String, _>("status"),
        "allowed_providers": row.get::<serde_json::Value, _>("allowed_providers"),
        "allowed_origins": row.get::<serde_json::Value, _>("allowed_origins"),
        "max_users": row.get::<i32, _>("max_users"),
        "user_count": user_count,
        "plan": row.get::<String, _>("plan"),
        "trial_ends_at": row.get::<Option<chrono::DateTime<Utc>>, _>("trial_ends_at").map(|t| t.to_rfc3339()),
        "primary_color": row.get::<Option<String>, _>("branding_primary_color"),
        "login_redirect_url": resolved_login_redirect,
        "branding": {
            "logo_url": row.get::<Option<String>, _>("branding_logo_url"),
            "primary_color": row.get::<Option<String>, _>("branding_primary_color"),
            "app_name": row.get::<Option<String>, _>("branding_app_name"),
        },
        "onboarding_completed": row.get::<bool, _>("onboarding_completed"),
        "onboarding_step": row.get::<i16, _>("onboarding_step"),
        "preferred_tab": row.get::<Option<String>, _>("preferred_tab"),
        "created_at": row.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
        "updated_at": row.get::<chrono::DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })))
}

/// PATCH /platform/self-serve/tenants/{id} — Update a tenant owned by the authenticated user.
pub async fn update_my_tenant(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    // Verify ownership
    let existing_tenant = sqlx::query(
        "SELECT slug, login_redirect_url FROM tenants \
         WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found or you don't have access".into(),
    })?;
    let current_slug: String = existing_tenant.get("slug");
    let current_login_redirect: Option<String> = existing_tenant.get("login_redirect_url");

    let body: UpdateTenantRequest = axum::Json::<UpdateTenantRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    if let Some(ref name) = body.name {
        if name.trim().is_empty() {
            return Err(AppError::BadRequest {
                error: "invalid_name",
                message: "Tenant name is required".into(),
            });
        }
    }

    let requested_slug = match body.slug.as_ref() {
        Some(slug) => Some(validate_slug(slug)?),
        None => None,
    };
    let effective_slug = requested_slug.as_deref().unwrap_or(&current_slug);

    let effective_color = body.branding_primary_color.as_ref().or(body.primary_color.as_ref());
    if let Some(v) = effective_color {
        if !is_valid_css_color(v) {
            return Err(AppError::BadRequest {
                error: "invalid_color",
                message: "primary_color must be a valid CSS hex color (e.g. #abc, #aabbcc)".into(),
            });
        }
    }

    let validated_login_redirect = match body.login_redirect_url.as_ref() {
        Some(v) => Some(validate_login_redirect_input(
            &state.config.server_url,
            effective_slug,
            v,
        )?),
        None => None,
    };

    let derived_login_redirect = match requested_slug.as_ref() {
        Some(next_slug) if next_slug != &current_slug => {
            let previous_default = default_login_redirect(&state.config.server_url, &current_slug);
            let current_resolved_redirect = resolve_login_redirect(
                &state.config.server_url,
                &current_slug,
                current_login_redirect.as_deref(),
            );
            if current_resolved_redirect == previous_default {
                Some(default_login_redirect(&state.config.server_url, next_slug))
            } else {
                None
            }
        }
        _ => None,
    };

    let mut tx = state.db.begin().await?;

    if let Some(ref next_slug) = requested_slug {
        if next_slug != &current_slug {
            sqlx::query("UPDATE tenants SET slug = $1, updated_at = NOW() WHERE id = $2")
                .bind(next_slug)
                .bind(&tenant_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    if let sqlx::Error::Database(ref db_err) = e {
                        if db_err.is_unique_violation() {
                            return AppError::Conflict {
                                error: "slug_taken",
                                message: format!("Tenant slug '{}' is already in use", next_slug),
                            };
                        }
                    }
                    AppError::from(e)
                })?;
        }
    }

    if let Some(ref name) = body.name {
        sqlx::query("UPDATE tenants SET name = $1, updated_at = NOW() WHERE id = $2")
            .bind(name.trim())
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(ref providers) = body.allowed_providers {
        let json = serde_json::to_value(providers).unwrap_or_default();
        sqlx::query("UPDATE tenants SET allowed_providers = $1, updated_at = NOW() WHERE id = $2")
            .bind(&json)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(ref origins) = body.allowed_origins {
        let json = serde_json::to_value(origins).unwrap_or_default();
        sqlx::query("UPDATE tenants SET allowed_origins = $1, updated_at = NOW() WHERE id = $2")
            .bind(&json)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(ref v) = body.branding_logo_url {
        sqlx::query("UPDATE tenants SET branding_logo_url = $1, updated_at = NOW() WHERE id = $2")
            .bind(v)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = effective_color {
        sqlx::query(
            "UPDATE tenants SET branding_primary_color = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(ref v) = body.branding_app_name {
        sqlx::query("UPDATE tenants SET branding_app_name = $1, updated_at = NOW() WHERE id = $2")
            .bind(v)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(ref validated) = validated_login_redirect.or(derived_login_redirect) {
        sqlx::query("UPDATE tenants SET login_redirect_url = $1, updated_at = NOW() WHERE id = $2")
            .bind(validated)
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(v) = body.onboarding_completed {
        sqlx::query(
            "UPDATE tenants SET onboarding_completed = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(v) = body.onboarding_step {
        sqlx::query(
            "UPDATE tenants SET onboarding_step = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(ref v) = body.preferred_tab {
        sqlx::query(
            "UPDATE tenants SET preferred_tab = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(v)
        .bind(&tenant_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    state.tenant_cache.invalidate(&current_slug);
    tracing::info!(tenant_id = %tenant_id, "Self-serve tenant config updated");

    // Return the full updated tenant so the frontend can refresh state
    let row = sqlx::query(
        "SELECT id, name, slug, owner_email, status, allowed_providers, allowed_origins, \
         max_users, COALESCE(plan, 'trial') AS plan, trial_ends_at, \
         branding_logo_url, branding_primary_color, branding_app_name, \
         login_redirect_url, \
         onboarding_completed, onboarding_step, preferred_tab, \
         created_at, updated_at \
         FROM tenants WHERE id = $1",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    let updated_slug: String = row.get("slug");
    let redirect_url: Option<String> = row.get("login_redirect_url");
    let resolved_redirect =
        resolve_login_redirect(&state.config.server_url, &updated_slug, redirect_url.as_deref());

    if updated_slug != current_slug {
        state.tenant_cache.invalidate(&updated_slug);
    }

    Ok(Json(serde_json::json!({
        "id": row.get::<String, _>("id"),
        "name": row.get::<String, _>("name"),
        "slug": updated_slug,
        "owner_email": row.get::<String, _>("owner_email"),
        "status": row.get::<String, _>("status"),
        "allowed_providers": row.get::<serde_json::Value, _>("allowed_providers"),
        "allowed_origins": row.get::<serde_json::Value, _>("allowed_origins"),
        "max_users": row.get::<i32, _>("max_users"),
        "plan": row.get::<String, _>("plan"),
        "trial_ends_at": row.get::<Option<chrono::DateTime<Utc>>, _>("trial_ends_at").map(|t| t.to_rfc3339()),
        "primary_color": row.get::<Option<String>, _>("branding_primary_color"),
        "login_redirect_url": resolved_redirect,
        "onboarding_completed": row.get::<bool, _>("onboarding_completed"),
        "onboarding_step": row.get::<i16, _>("onboarding_step"),
        "preferred_tab": row.get::<Option<String>, _>("preferred_tab"),
        "created_at": row.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
        "updated_at": row.get::<chrono::DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })))
}

/// DELETE /platform/self-serve/tenants/{id} — Soft-delete a tenant owned by the authenticated user.
/// Requires `confirm_slug` in request body as an explicit destructive-action confirmation.
pub async fn delete_my_tenant(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    let body: DeleteTenantRequest = axum::Json::<DeleteTenantRequest>::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_request",
            message: "Invalid request body".into(),
        })?
        .0;

    let slug = sqlx::query_scalar::<_, String>(
        "SELECT slug FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found or you don't have access".into(),
    })?;

    let confirm_slug = body.confirm_slug.trim();
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

    // Revoke all active sessions for the deleted tenant
    sqlx::query("UPDATE refresh_tokens SET revoked = TRUE WHERE tenant_id = $1 AND revoked = FALSE")
        .bind(&tenant_id)
        .execute(&state.db)
        .await
        .ok();

    state.tenant_cache.invalidate(&slug);
    tracing::warn!(
        tenant_id = %tenant_id,
        owner_user_id = %user.user_id,
        slug = %slug,
        "Self-serve tenant soft-deleted"
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "message": "Tenant deleted",
            "tenant_id": tenant_id,
            "slug": slug,
            "status": "deleted"
        })),
    ))
}

/// POST /platform/self-serve/tenants/{id}/rotate-key — Rotate management API key.
pub async fn rotate_my_tenant_key(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    // Verify ownership
    let exists = sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_none() {
        return Err(AppError::NotFound {
            error: "tenant_not_found",
            message: "Tenant not found or you don't have access".into(),
        });
    }

    let new_key = format!("mgmt_{}", uuid::Uuid::new_v4());
    let prefix = &new_key[..new_key.len().min(13)];
    let hash = signing::hash_token(&new_key);

    sqlx::query("UPDATE tenants SET api_key_hash = $1, api_key_prefix = $2, updated_at = NOW() WHERE id = $3")
        .bind(&hash)
        .bind(prefix)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    tracing::info!(tenant_id = %tenant_id, "Self-serve API key rotated");

    Ok(Json(serde_json::json!({
        "api_key": new_key,
        "note": "Save this API key in backend secrets only — it will not be shown again. The old key is now invalid."
    })))
}

/// GET /platform/self-serve/tenants/{id}/stats — Per-tenant stats for the dashboard.
pub async fn tenant_stats(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Query(query): Query<TenantStatsQuery>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    // Verify ownership
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found or you don't have access".into(),
    })?;

    // Gather stats (same queries as admin.rs::stats + management.rs::tenant_stats)
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id = $1")
        .bind(&tenant_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let verified_users: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE tenant_id = $1 AND email_verified = TRUE",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let blocked_users: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE tenant_id = $1 AND locked_until > NOW()",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let active_sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE tenant_id = $1 AND revoked = FALSE AND expires_at > NOW()"
    ).bind(&tenant_id).fetch_one(&state.db).await.unwrap_or(0);

    let logins_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM login_attempts WHERE user_id IN \
         (SELECT id FROM users WHERE tenant_id = $1) AND success = TRUE \
         AND created_at > NOW() - INTERVAL '24 hours'",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let failed_logins_24h: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM login_attempts WHERE user_id IN \
         (SELECT id FROM users WHERE tenant_id = $1) AND success = FALSE \
         AND created_at > NOW() - INTERVAL '24 hours'",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Provider breakdown
    let provider_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT c.provider, COUNT(*) FROM credentials c \
         JOIN users u ON u.id = c.user_id WHERE u.tenant_id = $1 GROUP BY c.provider",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut providers = serde_json::Map::new();
    for (provider, count) in provider_rows {
        providers.insert(provider, serde_json::json!(count));
    }

    // Latest user (for console overview activity)
    let latest_user = if total_users > 0 {
        sqlx::query_as::<_, (String, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT u.email, \
             COALESCE((SELECT c.provider FROM credentials c WHERE c.user_id = u.id AND c.tenant_id = u.tenant_id ORDER BY c.created_at DESC LIMIT 1), 'email') as provider, \
             u.created_at \
             FROM users u WHERE u.tenant_id = $1 ORDER BY u.created_at DESC LIMIT 1"
        )
        .bind(&tenant_id)
        .fetch_optional(&state.db)
        .await?
        .map(|(email, provider, created_at)| serde_json::json!({
            "email": email,
            "provider": provider,
            "created_at": created_at,
        }))
    } else {
        None
    };

    let selected_window = normalize_tenant_stats_window(query.window.as_deref());
    let series_start = match selected_window {
        "24h" => Utc::now() - Duration::hours(23),
        "30d" => Utc::now() - Duration::days(29),
        _ => Utc::now() - Duration::days(6),
    };

    let series_rows = if selected_window == "24h" {
        sqlx::query(
            "WITH buckets AS (
                SELECT generate_series(
                    date_trunc('hour', $2::timestamptz),
                    date_trunc('hour', NOW()),
                    INTERVAL '1 hour'
                ) AS bucket
            ),
            logins AS (
                SELECT date_trunc('hour', la.created_at) AS bucket,
                       COUNT(*) FILTER (WHERE la.success = TRUE) AS logins,
                       COUNT(*) FILTER (WHERE la.success = FALSE) AS failed_logins
                  FROM login_attempts la
                 WHERE la.tenant_id = $1
                   AND la.created_at >= $2
                 GROUP BY 1
            ),
            sessions AS (
                SELECT date_trunc('hour', rt.created_at) AS bucket,
                       COUNT(*) AS sessions_started
                  FROM refresh_tokens rt
                 WHERE rt.tenant_id = $1
                   AND rt.created_at >= $2
                 GROUP BY 1
            )
            SELECT b.bucket,
                   to_char(b.bucket, 'HH24:00') AS label,
                   COALESCE(l.logins, 0)::BIGINT AS logins,
                   COALESCE(l.failed_logins, 0)::BIGINT AS failed_logins,
                   COALESCE(s.sessions_started, 0)::BIGINT AS sessions_started
              FROM buckets b
              LEFT JOIN logins l ON l.bucket = b.bucket
              LEFT JOIN sessions s ON s.bucket = b.bucket
             ORDER BY b.bucket ASC"
        )
        .bind(&tenant_id)
        .bind(series_start)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query(
            "WITH buckets AS (
                SELECT generate_series(
                    date_trunc('day', $2::timestamptz),
                    date_trunc('day', NOW()),
                    INTERVAL '1 day'
                ) AS bucket
            ),
            logins AS (
                SELECT date_trunc('day', la.created_at) AS bucket,
                       COUNT(*) FILTER (WHERE la.success = TRUE) AS logins,
                       COUNT(*) FILTER (WHERE la.success = FALSE) AS failed_logins
                  FROM login_attempts la
                 WHERE la.tenant_id = $1
                   AND la.created_at >= $2
                 GROUP BY 1
            ),
            sessions AS (
                SELECT date_trunc('day', rt.created_at) AS bucket,
                       COUNT(*) AS sessions_started
                  FROM refresh_tokens rt
                 WHERE rt.tenant_id = $1
                   AND rt.created_at >= $2
                 GROUP BY 1
            )
            SELECT b.bucket,
                   to_char(b.bucket, 'Mon DD') AS label,
                   COALESCE(l.logins, 0)::BIGINT AS logins,
                   COALESCE(l.failed_logins, 0)::BIGINT AS failed_logins,
                   COALESCE(s.sessions_started, 0)::BIGINT AS sessions_started
              FROM buckets b
              LEFT JOIN logins l ON l.bucket = b.bucket
              LEFT JOIN sessions s ON s.bucket = b.bucket
             ORDER BY b.bucket ASC"
        )
        .bind(&tenant_id)
        .bind(series_start)
        .fetch_all(&state.db)
        .await?
    };

    let mut logins_in_window = 0_i64;
    let mut failed_logins_in_window = 0_i64;
    let series: Vec<serde_json::Value> = series_rows
        .into_iter()
        .map(|row| {
            let logins: i64 = row.get("logins");
            let failed_logins: i64 = row.get("failed_logins");
            let sessions_started: i64 = row.get("sessions_started");
            logins_in_window += logins;
            failed_logins_in_window += failed_logins;

            serde_json::json!({
                "bucket_start": row.get::<chrono::DateTime<chrono::Utc>, _>("bucket"),
                "label": row.get::<String, _>("label"),
                "logins": logins,
                "failed_logins": failed_logins,
                "sessions_started": sessions_started,
            })
        })
        .collect();

    let activity_rows = sqlx::query(
        "SELECT id, kind, email, provider, description, created_at
           FROM (
                SELECT CONCAT('user:', u.id) AS id,
                       'user_created'::TEXT AS kind,
                       u.email AS email,
                       COALESCE(cp.provider, 'email') AS provider,
                       'Joined the tenant workspace.'::TEXT AS description,
                       u.created_at AS created_at
                  FROM users u
                  LEFT JOIN LATERAL (
                      SELECT c.provider
                        FROM credentials c
                       WHERE c.user_id = u.id
                         AND c.tenant_id = u.tenant_id
                       ORDER BY c.created_at DESC
                       LIMIT 1
                  ) cp ON TRUE
                 WHERE u.tenant_id = $1
                   AND u.created_at >= $2

                UNION ALL

                SELECT CONCAT('login:', la.id) AS id,
                       CASE WHEN la.success THEN 'login_succeeded' ELSE 'login_failed' END AS kind,
                       COALESCE(u.email, la.email) AS email,
                       COALESCE(cp.provider, 'email') AS provider,
                       CASE
                           WHEN la.success THEN 'Authenticated through the tenant login flow.'
                           ELSE COALESCE(la.failure_reason, 'Sign-in failed before a session was created.')
                       END AS description,
                       la.created_at AS created_at
                  FROM login_attempts la
                  LEFT JOIN users u ON u.id = la.user_id
                  LEFT JOIN LATERAL (
                      SELECT c.provider
                        FROM credentials c
                       WHERE c.user_id = la.user_id
                         AND c.tenant_id = $1
                       ORDER BY c.created_at DESC
                       LIMIT 1
                  ) cp ON TRUE
                 WHERE la.tenant_id = $1
                   AND la.created_at >= $2

                UNION ALL

                SELECT CONCAT('session:', rt.id) AS id,
                       'session_started'::TEXT AS kind,
                       COALESCE(u.email, 'Unknown user') AS email,
                       COALESCE(cp.provider, rt.auth_provider, 'email') AS provider,
                       'A browser session was issued for this tenant.'::TEXT AS description,
                       rt.created_at AS created_at
                  FROM refresh_tokens rt
                  LEFT JOIN users u ON u.id = rt.user_id
                  LEFT JOIN LATERAL (
                      SELECT c.provider
                        FROM credentials c
                       WHERE c.user_id = rt.user_id
                         AND c.tenant_id = $1
                       ORDER BY c.created_at DESC
                       LIMIT 1
                  ) cp ON TRUE
                 WHERE rt.tenant_id = $1
                   AND rt.created_at >= $2
            ) activity
          ORDER BY created_at DESC
          LIMIT 8"
    )
    .bind(&tenant_id)
    .bind(series_start)
    .fetch_all(&state.db)
    .await?;

    let activity: Vec<serde_json::Value> = activity_rows
        .into_iter()
        .map(|row| serde_json::json!({
            "id": row.get::<String, _>("id"),
            "kind": row.get::<String, _>("kind"),
            "email": row.get::<String, _>("email"),
            "provider": row.get::<String, _>("provider"),
            "description": row.get::<String, _>("description"),
            "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        }))
        .collect();

    Ok(Json(serde_json::json!({
        "tenant_id": tenant_id,
        "total_users": total_users,
        "verified_users": verified_users,
        "blocked_users": blocked_users,
        "active_sessions": active_sessions,
        "logins_24h": logins_24h,
        "failed_logins_24h": failed_logins_24h,
        "window": selected_window,
        "logins_in_window": logins_in_window,
        "failed_logins_in_window": failed_logins_in_window,
        "series": series,
        "activity": activity,
        "providers": providers,
        "latest_user": latest_user,
    })))
}

// ── Self-Serve User Management ──────────────────────────────────────────

/// GET /platform/self-serve/tenants/{id}/users — List users for a tenant owned by the authenticated user.
pub async fn list_tenant_users(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    // Verify ownership
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found or you don't have access".into(),
    })?;

    let rows = sqlx::query(
        "SELECT u.id, u.email, u.name, u.email_verified, u.created_at, u.updated_at, \
         COALESCE(u.locked_until > NOW(), FALSE) AS is_locked, \
         (SELECT array_agg(r.name) FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = u.id AND ur.tenant_id = $1) AS roles, \
         (SELECT c.provider FROM credentials c WHERE c.user_id = u.id AND c.tenant_id = $1 LIMIT 1) AS provider, \
         (SELECT COUNT(*)::BIGINT FROM refresh_tokens rt WHERE rt.user_id = u.id AND rt.tenant_id = $1 AND rt.revoked = FALSE AND rt.expires_at > NOW()) AS active_sessions \
         FROM users u WHERE u.tenant_id = $1 \
         ORDER BY u.created_at DESC LIMIT 100",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let users: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let roles: Option<Vec<String>> = r.get("roles");
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "email": r.get::<String, _>("email"),
                "name": r.get::<Option<String>, _>("name"),
                "email_verified": r.get::<bool, _>("email_verified"),
                "is_locked": r.get::<bool, _>("is_locked"),
                "provider": r.get::<Option<String>, _>("provider"),
                "roles": roles.unwrap_or_default(),
                "active_sessions": r.get::<i64, _>("active_sessions"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(
        serde_json::json!({ "users": users, "total": users.len() }),
    ))
}

/// POST /platform/self-serve/tenants/{id}/users/{user_id}/block
pub async fn block_tenant_user(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let result = sqlx::query(
        "UPDATE users SET locked_until = '2099-12-31T23:59:59Z', updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: "User not found".into(),
        });
    }

    Ok(Json(serde_json::json!({ "status": "blocked" })))
}

/// POST /platform/self-serve/tenants/{id}/users/{user_id}/unblock
pub async fn unblock_tenant_user(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let result = sqlx::query(
        "UPDATE users SET locked_until = NULL, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "user_not_found",
            message: "User not found".into(),
        });
    }

    Ok(Json(serde_json::json!({ "status": "unblocked" })))
}

/// POST /platform/self-serve/tenants/{id}/users/{user_id}/revoke-sessions
pub async fn revoke_tenant_user_sessions(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let revoked = sqlx::query(
        "UPDATE refresh_tokens SET revoked = TRUE WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "revoked": revoked.rows_affected() })))
}

/// GET /platform/self-serve/tenants/{id}/users/{user_id}/sessions
pub async fn list_tenant_user_sessions(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    // Verify user belongs to tenant
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM users WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "user_not_found",
        message: "User not found in this tenant".into(),
    })?;

    let rows = sqlx::query(
        "SELECT id, created_at, expires_at, auth_method, auth_provider, \
         last_activity_at, login_ip, mfa_verified \
         FROM refresh_tokens \
         WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE AND expires_at > NOW() \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let sessions: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
                "expires_at": r.get::<chrono::DateTime<Utc>, _>("expires_at").to_rfc3339(),
                "auth_method": r.get::<String, _>("auth_method"),
                "auth_provider": r.get::<Option<String>, _>("auth_provider"),
                "last_activity_at": r.get::<Option<chrono::DateTime<Utc>>, _>("last_activity_at").map(|d| d.to_rfc3339()),
                "login_ip": r.get::<Option<String>, _>("login_ip"),
                "mfa_verified": r.get::<bool, _>("mfa_verified"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

/// GET /platform/self-serve/tenants/{id}/audit-log
pub async fn list_tenant_audit_log(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let limit: i64 = 100;
    let offset: i64 = 0;

    let rows = sqlx::query(
        "SELECT la.id, la.email, la.user_id, la.success, la.ip_address, la.user_agent, \
         la.failure_reason, la.created_at \
         FROM login_attempts la \
         WHERE la.tenant_id = $1 \
         ORDER BY la.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(&tenant_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let entries: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "email": r.get::<String, _>("email"),
                "user_id": r.get::<Option<String>, _>("user_id"),
                "success": r.get::<bool, _>("success"),
                "ip_address": r.get::<Option<String>, _>("ip_address"),
                "user_agent": r.get::<Option<String>, _>("user_agent"),
                "failure_reason": r.get::<Option<String>, _>("failure_reason"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "audit_log": entries })))
}

/// GET /platform/self-serve/tenants/{id}/sessions
pub async fn list_tenant_sessions(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let rows = sqlx::query(
        "SELECT rt.id, rt.user_id, u.email, rt.created_at, rt.expires_at, \
         rt.auth_method, rt.auth_provider, rt.last_activity_at, rt.login_ip, rt.mfa_verified \
         FROM refresh_tokens rt \
         JOIN users u ON u.id = rt.user_id \
         WHERE rt.tenant_id = $1 AND rt.revoked = FALSE AND rt.expires_at > NOW() \
         ORDER BY rt.created_at DESC LIMIT 100",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let sessions: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "user_id": r.get::<String, _>("user_id"),
                "email": r.get::<String, _>("email"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
                "expires_at": r.get::<chrono::DateTime<Utc>, _>("expires_at").to_rfc3339(),
                "auth_method": r.get::<String, _>("auth_method"),
                "auth_provider": r.get::<Option<String>, _>("auth_provider"),
                "last_activity_at": r.get::<Option<chrono::DateTime<Utc>>, _>("last_activity_at").map(|d| d.to_rfc3339()),
                "login_ip": r.get::<Option<String>, _>("login_ip"),
                "mfa_verified": r.get::<bool, _>("mfa_verified"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

/// GET /platform/self-serve/tenants/{id}/passkeys
pub async fn list_tenant_passkeys(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let rows = sqlx::query(
        "SELECT wc.id, wc.name, wc.is_synced, wc.user_id, u.email, \
         wc.created_at, wc.last_used_at, wc.aaguid \
         FROM webauthn_credentials wc \
         JOIN users u ON u.id = wc.user_id \
         WHERE wc.tenant_id = $1 \
         ORDER BY wc.created_at DESC LIMIT 100",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let passkeys: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "name": r.get::<String, _>("name"),
                "is_synced": r.get::<bool, _>("is_synced"),
                "user_id": r.get::<String, _>("user_id"),
                "email": r.get::<String, _>("email"),
                "aaguid": r.get::<Option<String>, _>("aaguid"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
                "last_used_at": r.get::<Option<chrono::DateTime<Utc>>, _>("last_used_at").map(|d| d.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "passkeys": passkeys })))
}

// ── Role CRUD ─────────────────────────────────────────────────────────

/// GET /platform/self-serve/tenants/{id}/roles
pub async fn list_tenant_roles(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let rows = sqlx::query(
        "SELECT id, name, description, is_system, permissions_jsonb, tenant_id, created_at \
         FROM roles WHERE tenant_id IS NULL OR tenant_id = $1 \
         ORDER BY is_system DESC, name",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let roles: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let perms: Option<serde_json::Value> = r.get("permissions_jsonb");
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "name": r.get::<String, _>("name"),
                "description": r.get::<Option<String>, _>("description"),
                "is_system": r.get::<bool, _>("is_system"),
                "permissions": perms.unwrap_or(serde_json::json!([])),
                "tenant_id": r.get::<Option<String>, _>("tenant_id"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "roles": roles })))
}

/// POST /platform/self-serve/tenants/{id}/roles
pub async fn create_tenant_role(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let body: serde_json::Value = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_body",
            message: "Invalid JSON body".into(),
        })?
        .0;

    let name = body["name"].as_str().ok_or(AppError::BadRequest {
        error: "missing_name",
        message: "Role name is required".into(),
    })?;

    if name.is_empty() || name.len() > 64 {
        return Err(AppError::BadRequest {
            error: "invalid_name",
            message: "Role name must be 1-64 characters".into(),
        });
    }

    if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ') {
        return Err(AppError::BadRequest {
            error: "invalid_name",
            message: "Role name may only contain letters, numbers, spaces, hyphens, and underscores".into(),
        });
    }

    let description = body["description"].as_str().map(String::from);
    let permissions = body.get("permissions").cloned().unwrap_or(serde_json::json!([]));
    let role_id = format!("role_{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO roles (id, name, description, permissions_jsonb, tenant_id, is_system, created_at) \
         VALUES ($1, $2, $3, $4, $5, FALSE, NOW())",
    )
    .bind(&role_id)
    .bind(name)
    .bind(&description)
    .bind(&permissions)
    .bind(&tenant_id)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": role_id,
            "name": name,
            "description": description,
            "permissions": permissions,
            "is_system": false,
            "tenant_id": tenant_id,
        })),
    ))
}

/// PATCH /platform/self-serve/tenants/{id}/roles/{role_id}
pub async fn update_tenant_role(
    State(state): State<AppState>,
    Path((tenant_id, role_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    // Verify role exists in this tenant and is custom (not system)
    let is_system: bool = sqlx::query_scalar(
        "SELECT is_system FROM roles WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&role_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "role_not_found",
        message: "Role not found".into(),
    })?;

    if is_system {
        return Err(AppError::BadRequest {
            error: "system_role",
            message: "Cannot modify system roles".into(),
        });
    }

    let body: serde_json::Value = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_body",
            message: "Invalid JSON body".into(),
        })?
        .0;

    if let Some(name) = body["name"].as_str() {
        sqlx::query("UPDATE roles SET name = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(name)
            .bind(&role_id)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(desc) = body.get("description") {
        let desc_str = desc.as_str().map(String::from);
        sqlx::query("UPDATE roles SET description = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(&desc_str)
            .bind(&role_id)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(perms) = body.get("permissions") {
        sqlx::query("UPDATE roles SET permissions_jsonb = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(perms)
            .bind(&role_id)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(serde_json::json!({ "message": "Role updated" })))
}

/// DELETE /platform/self-serve/tenants/{id}/roles/{role_id}
pub async fn delete_tenant_role(
    State(state): State<AppState>,
    Path((tenant_id, role_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    // Verify role is custom
    let is_system: bool = sqlx::query_scalar(
        "SELECT is_system FROM roles WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&role_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "role_not_found",
        message: "Role not found in this tenant".into(),
    })?;

    if is_system {
        return Err(AppError::BadRequest {
            error: "system_role",
            message: "Cannot delete system roles".into(),
        });
    }

    sqlx::query("DELETE FROM user_roles WHERE role_id = $1 AND tenant_id = $2")
        .bind(&role_id)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    sqlx::query("DELETE FROM roles WHERE id = $1 AND tenant_id = $2")
        .bind(&role_id)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "message": "Role deleted" })))
}

// ── Hosted Page Management ─────────────────────────────────────────────

/// GET /platform/self-serve/tenants/{id}/hosted-page
pub async fn get_hosted_page(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    // Verify ownership
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let row = sqlx::query(
        "SELECT headline, body, cta_text, cta_link, custom_html, updated_at \
         FROM tenant_hosted_pages WHERE tenant_id = $1",
    )
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?;

    match row {
        Some(r) => Ok(Json(serde_json::json!({
            "headline": r.get::<String, _>("headline"),
            "body": r.get::<String, _>("body"),
            "cta_text": r.get::<String, _>("cta_text"),
            "cta_link": r.get::<String, _>("cta_link"),
            "custom_html": r.get::<Option<String>, _>("custom_html"),
            "updated_at": r.get::<chrono::DateTime<Utc>, _>("updated_at").to_rfc3339(),
        }))),
        None => Ok(Json(serde_json::json!({
            "headline": "Welcome",
            "body": "",
            "cta_text": "",
            "cta_link": "",
            "custom_html": null,
            "updated_at": null,
        }))),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateHostedPageRequest {
    pub headline: Option<String>,
    pub body: Option<String>,
    pub cta_text: Option<String>,
    pub cta_link: Option<String>,
    pub custom_html: Option<String>,
}

/// PUT /platform/self-serve/tenants/{id}/hosted-page
pub async fn update_hosted_page(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let body: UpdateHostedPageRequest =
        axum::Json::<UpdateHostedPageRequest>::from_request(req, &state)
            .await
            .map_err(|_| AppError::BadRequest {
                error: "invalid_request",
                message: "Invalid request body".into(),
            })?
            .0;

    // Validate field lengths
    if body.headline.as_deref().map_or(false, |s| s.len() > 200) {
        return Err(AppError::BadRequest {
            error: "headline_too_long",
            message: "Headline must be 200 characters or fewer".into(),
        });
    }
    if body.body.as_deref().map_or(false, |s| s.len() > 5000) {
        return Err(AppError::BadRequest {
            error: "body_too_long",
            message: "Body must be 5000 characters or fewer".into(),
        });
    }
    if body.cta_text.as_deref().map_or(false, |s| s.len() > 500) {
        return Err(AppError::BadRequest {
            error: "cta_text_too_long",
            message: "CTA text must be 500 characters or fewer".into(),
        });
    }
    if body.custom_html.as_deref().map_or(false, |s| s.len() > 50_000) {
        return Err(AppError::BadRequest {
            error: "custom_html_too_long",
            message: "Custom HTML must be 50000 characters or fewer".into(),
        });
    }

    // Validate cta_link URL scheme to prevent javascript: XSS
    if let Some(ref link) = body.cta_link {
        let trimmed = link.trim();
        if !trimmed.is_empty()
            && !trimmed.starts_with("http://")
            && !trimmed.starts_with("https://")
            && !trimmed.starts_with('/')
        {
            return Err(AppError::BadRequest {
                error: "invalid_cta_link",
                message: "CTA link must be an http(s) URL or absolute path".into(),
            });
        }
    }

    sqlx::query(
        "INSERT INTO tenant_hosted_pages (tenant_id, headline, body, cta_text, cta_link, custom_html) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (tenant_id) DO UPDATE SET \
           headline = COALESCE($2, tenant_hosted_pages.headline), \
           body = COALESCE($3, tenant_hosted_pages.body), \
           cta_text = COALESCE($4, tenant_hosted_pages.cta_text), \
           cta_link = COALESCE($5, tenant_hosted_pages.cta_link), \
           custom_html = COALESCE($6, tenant_hosted_pages.custom_html), \
           updated_at = NOW()",
    )
    .bind(&tenant_id)
    .bind(&body.headline.unwrap_or_else(|| "Welcome".to_string()))
    .bind(&body.body.unwrap_or_default())
    .bind(&body.cta_text.unwrap_or_default())
    .bind(&body.cta_link.unwrap_or_default())
    .bind(&body.custom_html)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "status": "saved" })))
}

// ── Security Config ─────────────────────────────────────────────────────

/// GET /platform/self-serve/tenants/{id}/security-config
pub async fn get_tenant_security_config(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let row: (serde_json::Value, String) = sqlx::query_as(
        "SELECT COALESCE(t.security_config, '{}'::jsonb), COALESCE(te.plan, 'trial') \
         FROM tenants t \
         LEFT JOIN tenant_entitlements te ON te.tenant_id = t.id \
         WHERE t.id = $1",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    let config = crate::security_config::TenantSecurityConfig::from_json(&row.0);

    Ok(Json(serde_json::json!({
        "security_config": config,
        "plan": row.1,
    })))
}

/// PATCH /platform/self-serve/tenants/{id}/security-config
pub async fn update_tenant_security_config(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let body: serde_json::Value = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_body",
            message: "Invalid JSON body".into(),
        })?
        .0;

    if !body.is_object() {
        return Err(AppError::BadRequest {
            error: "invalid_body",
            message: "Request body must be a JSON object".into(),
        });
    }

    // Load current config
    let current_json: serde_json::Value = sqlx::query_scalar(
        "SELECT COALESCE(security_config, '{}'::jsonb) FROM tenants WHERE id = $1",
    )
    .bind(&tenant_id)
    .fetch_one(&state.db)
    .await?;

    // Merge patch into current
    let mut merged = current_json.clone();
    if let (Some(base), Some(patch)) = (merged.as_object_mut(), body.as_object()) {
        for (k, v) in patch {
            base.insert(k.clone(), v.clone());
        }
    }

    // Validate
    let config = crate::security_config::TenantSecurityConfig::from_json(&merged);

    let plan: String = sqlx::query_scalar(
        "SELECT COALESCE(plan, 'trial') FROM tenant_entitlements WHERE tenant_id = $1",
    )
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or_else(|| "trial".to_string());

    let violations = config.validate_for_plan(&plan);
    if !violations.is_empty() {
        return Err(AppError::Forbidden {
            error: "plan_violation",
            message: format!("Security config violates plan restrictions: {}", violations.join(", ")),
        });
    }

    // Save
    sqlx::query("UPDATE tenants SET security_config = $1, updated_at = NOW() WHERE id = $2")
        .bind(&merged)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({
        "security_config": config,
        "plan": plan,
    })))
}

// ── Tenant Invite Proxy Endpoints ───────────────────────────────────────

/// GET /platform/self-serve/tenants/{id}/invites
pub async fn list_tenant_invites_selfserve(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    ).bind(&tenant_id).bind(&platform_user.user_id)
    .fetch_optional(&state.db).await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    // Auto-expire stale invites
    sqlx::query(
        "UPDATE tenant_user_invites SET status = 'expired', updated_at = NOW() \
         WHERE tenant_id = $1 AND status = 'pending' AND expires_at < NOW()",
    ).bind(&tenant_id).execute(&state.db).await.ok();

    let rows = sqlx::query(
        "SELECT tui.id, tui.email, tui.role_id, r.name AS role_name, tui.status, \
         tui.expires_at, tui.created_at \
         FROM tenant_user_invites tui \
         LEFT JOIN roles r ON r.id = tui.role_id \
         WHERE tui.tenant_id = $1 \
         ORDER BY tui.created_at DESC LIMIT 50",
    ).bind(&tenant_id).fetch_all(&state.db).await?;

    let invites: Vec<serde_json::Value> = rows.iter().map(|r| {
        serde_json::json!({
            "id": r.get::<String, _>("id"),
            "email": r.get::<String, _>("email"),
            "role_id": r.get::<String, _>("role_id"),
            "role_name": r.get::<Option<String>, _>("role_name"),
            "status": r.get::<String, _>("status"),
            "expires_at": r.get::<chrono::DateTime<Utc>, _>("expires_at").to_rfc3339(),
            "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
        })
    }).collect();

    Ok(Json(serde_json::json!({ "invites": invites })))
}

/// POST /platform/self-serve/tenants/{id}/invites
pub async fn create_tenant_invite_selfserve(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    ).bind(&tenant_id).bind(&platform_user.user_id)
    .fetch_optional(&state.db).await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    let body: serde_json::Value = axum::Json::from_request(req, &state)
        .await.map_err(|_| AppError::BadRequest { error: "invalid_body", message: "Invalid JSON".into() })?.0;

    let email = body["email"].as_str().ok_or(AppError::BadRequest {
        error: "missing_email", message: "Email is required".into(),
    })?.to_lowercase().trim().to_string();

    let role_id = body["role"].as_str().unwrap_or("role_admin").to_string();
    let invite_id = format!("tinv_{}", uuid::Uuid::new_v4());

    // Revoke previous pending invites for same email
    sqlx::query(
        "UPDATE tenant_user_invites SET status = 'revoked', updated_at = NOW() \
         WHERE tenant_id = $1 AND lower(email) = lower($2) AND status = 'pending'",
    ).bind(&tenant_id).bind(&email).execute(&state.db).await.ok();

    let expires_at = Utc::now() + chrono::Duration::hours(72);

    sqlx::query(
        "INSERT INTO tenant_user_invites \
         (id, tenant_id, email, role_id, token_hash, status, invited_by_user_id, expires_at, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7, NOW(), NOW())",
    )
    .bind(&invite_id)
    .bind(&tenant_id)
    .bind(&email)
    .bind(&role_id)
    .bind(&format!("hash_{}", uuid::Uuid::new_v4())) // placeholder hash
    .bind(&platform_user.user_id)
    .bind(expires_at)
    .execute(&state.db).await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "invite": { "id": invite_id, "email": email, "role_id": role_id, "status": "pending", "expires_at": expires_at.to_rfc3339() },
        })),
    ))
}

/// POST /platform/self-serve/tenants/{id}/invites/{invite_id}/revoke
pub async fn revoke_tenant_invite_selfserve(
    State(state): State<AppState>,
    Path((tenant_id, invite_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    ).bind(&tenant_id).bind(&platform_user.user_id)
    .fetch_optional(&state.db).await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    let result = sqlx::query(
        "UPDATE tenant_user_invites SET status = 'revoked', updated_at = NOW() \
         WHERE id = $1 AND tenant_id = $2 AND status = 'pending'",
    ).bind(&invite_id).bind(&tenant_id).execute(&state.db).await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound { error: "invite_not_found", message: "Invite not found or not pending".into() });
    }

    Ok(Json(serde_json::json!({ "success": true, "invite_id": invite_id })))
}

// ── Get Started Page ────────────────────────────────────────────────────

/// GET /get-started — Self-serve onboarding page.
/// Step 1: Register/login on platform tenant
/// Step 2: Create a tenant
/// Step 3: Get API key + next steps
pub async fn get_started_page(State(state): State<AppState>) -> impl IntoResponse {
    if let Err(e) = ensure_platform_oauth_clients(&state).await {
        tracing::warn!(error = ?e, "Failed to ensure platform OAuth clients for onboarding");
    }

    let page = render_get_started_page_html(&state);
    (
        [(axum::http::header::HeaderName::from_static("content-security-policy"), page.csp)],
        Html(page.html),
    ).into_response()
}

// ── Organization Management ──────────────────────────────────────────

/// GET /platform/self-serve/tenants/{id}/organizations
pub async fn list_tenant_organizations(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let rows = sqlx::query(
        "SELECT id, name, slug, metadata, created_at, updated_at \
         FROM organizations WHERE tenant_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let orgs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<String, _>("id"),
                "name": r.get::<String, _>("name"),
                "slug": r.get::<String, _>("slug"),
                "metadata": r.get::<Option<serde_json::Value>, _>("metadata"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
                "updated_at": r.get::<chrono::DateTime<Utc>, _>("updated_at").to_rfc3339(),
            })
        })
        .collect();

    let total = orgs.len();
    Ok(Json(serde_json::json!({ "organizations": orgs, "total": total })))
}

/// POST /platform/self-serve/tenants/{id}/organizations
pub async fn create_tenant_organization(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let body: serde_json::Value = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_body",
            message: "Invalid JSON body".into(),
        })?
        .0;

    let name = body["name"].as_str().ok_or(AppError::BadRequest {
        error: "missing_name",
        message: "Organization name is required".into(),
    })?;

    if name.is_empty() || name.len() > 120 {
        return Err(AppError::BadRequest {
            error: "invalid_name",
            message: "Organization name must be 1-120 characters".into(),
        });
    }

    let slug = body["slug"].as_str().ok_or(AppError::BadRequest {
        error: "missing_slug",
        message: "Organization slug is required".into(),
    })?;

    let slug = slug.trim().to_lowercase();
    if slug.is_empty()
        || slug.len() > 64
        || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AppError::BadRequest {
            error: "invalid_slug",
            message: "Slug must be 1-64 characters, lowercase alphanumeric or hyphens".into(),
        });
    }

    // Check slug uniqueness within tenant
    let slug_exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM organizations WHERE tenant_id = $1 AND slug = $2",
    )
    .bind(&tenant_id)
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;

    if slug_exists.is_some() {
        return Err(AppError::Conflict {
            error: "slug_taken",
            message: format!("Organization slug '{}' is already in use for this tenant", slug),
        });
    }

    let org_id = format!("org_{}", uuid::Uuid::new_v4());
    let metadata = body.get("metadata").cloned();

    sqlx::query(
        "INSERT INTO organizations (id, tenant_id, name, slug, metadata) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&org_id)
    .bind(&tenant_id)
    .bind(name)
    .bind(&slug)
    .bind(&metadata)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": org_id,
            "name": name,
            "slug": slug,
            "metadata": metadata,
            "tenant_id": tenant_id,
        })),
    ))
}

/// GET /platform/self-serve/tenants/{id}/organizations/{org_id}
pub async fn get_tenant_organization(
    State(state): State<AppState>,
    Path((tenant_id, org_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let row = sqlx::query(
        "SELECT id, name, slug, metadata, created_at, updated_at \
         FROM organizations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&org_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "organization_not_found",
        message: "Organization not found".into(),
    })?;

    Ok(Json(serde_json::json!({
        "id": row.get::<String, _>("id"),
        "name": row.get::<String, _>("name"),
        "slug": row.get::<String, _>("slug"),
        "metadata": row.get::<Option<serde_json::Value>, _>("metadata"),
        "created_at": row.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
        "updated_at": row.get::<chrono::DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })))
}

/// PATCH /platform/self-serve/tenants/{id}/organizations/{org_id}
pub async fn update_tenant_organization(
    State(state): State<AppState>,
    Path((tenant_id, org_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    // Verify org exists in this tenant
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM organizations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&org_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "organization_not_found",
        message: "Organization not found".into(),
    })?;

    let body: serde_json::Value = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_body",
            message: "Invalid JSON body".into(),
        })?
        .0;

    if let Some(name) = body["name"].as_str() {
        if name.is_empty() || name.len() > 120 {
            return Err(AppError::BadRequest {
                error: "invalid_name",
                message: "Organization name must be 1-120 characters".into(),
            });
        }
        sqlx::query("UPDATE organizations SET name = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3")
            .bind(name)
            .bind(&org_id)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    if let Some(slug) = body["slug"].as_str() {
        let slug = slug.trim().to_lowercase();
        if slug.is_empty()
            || slug.len() > 64
            || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(AppError::BadRequest {
                error: "invalid_slug",
                message: "Slug must be 1-64 characters, lowercase alphanumeric or hyphens".into(),
            });
        }
        sqlx::query("UPDATE organizations SET slug = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3")
            .bind(&slug)
            .bind(&org_id)
            .bind(&tenant_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                if let sqlx::Error::Database(ref db_err) = e {
                    if db_err.is_unique_violation() {
                        return AppError::Conflict {
                            error: "slug_taken",
                            message: format!("Organization slug '{}' is already in use", slug),
                        };
                    }
                }
                AppError::from(e)
            })?;
    }

    if let Some(metadata) = body.get("metadata") {
        sqlx::query("UPDATE organizations SET metadata = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3")
            .bind(metadata)
            .bind(&org_id)
            .bind(&tenant_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(serde_json::json!({ "message": "Organization updated" })))
}

/// DELETE /platform/self-serve/tenants/{id}/organizations/{org_id}
pub async fn delete_tenant_organization(
    State(state): State<AppState>,
    Path((tenant_id, org_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    // Delete members first, then the organization
    sqlx::query("DELETE FROM organization_members WHERE organization_id = $1 AND tenant_id = $2")
        .bind(&org_id)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    let result = sqlx::query("DELETE FROM organizations WHERE id = $1 AND tenant_id = $2")
        .bind(&org_id)
        .bind(&tenant_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "organization_not_found",
            message: "Organization not found".into(),
        });
    }

    Ok(Json(serde_json::json!({ "message": "Organization deleted" })))
}

/// GET /platform/self-serve/tenants/{id}/organizations/{org_id}/members
pub async fn list_organization_members(
    State(state): State<AppState>,
    Path((tenant_id, org_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let rows = sqlx::query(
        "SELECT m.user_id, m.role, m.created_at, u.email, u.name \
         FROM organization_members m \
         JOIN users u ON u.id = m.user_id \
         WHERE m.organization_id = $1 AND m.tenant_id = $2 \
         ORDER BY m.created_at DESC",
    )
    .bind(&org_id)
    .bind(&tenant_id)
    .fetch_all(&state.db)
    .await?;

    let members: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "user_id": r.get::<String, _>("user_id"),
                "email": r.get::<String, _>("email"),
                "name": r.get::<Option<String>, _>("name"),
                "role": r.get::<String, _>("role"),
                "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "members": members })))
}

/// POST /platform/self-serve/tenants/{id}/organizations/{org_id}/members
pub async fn add_organization_member(
    State(state): State<AppState>,
    Path((tenant_id, org_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let body: serde_json::Value = axum::Json::from_request(req, &state)
        .await
        .map_err(|_| AppError::BadRequest {
            error: "invalid_body",
            message: "Invalid JSON body".into(),
        })?
        .0;

    let user_id = body["user_id"].as_str().ok_or(AppError::BadRequest {
        error: "missing_user_id",
        message: "user_id is required".into(),
    })?;

    let role = body["role"].as_str().unwrap_or("member");

    // Verify user exists in tenant
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM users WHERE id = $1 AND tenant_id = $2",
    )
    .bind(user_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "user_not_found",
        message: "User not found in this tenant".into(),
    })?;

    // Verify organization exists in tenant
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM organizations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&org_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "organization_not_found",
        message: "Organization not found".into(),
    })?;

    sqlx::query(
        "INSERT INTO organization_members (organization_id, user_id, tenant_id, role) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (organization_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(&org_id)
    .bind(user_id)
    .bind(&tenant_id)
    .bind(role)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "message": "Member added",
            "organization_id": org_id,
            "user_id": user_id,
            "role": role,
        })),
    ))
}

/// DELETE /platform/self-serve/tenants/{id}/organizations/{org_id}/members/{user_id}
pub async fn remove_organization_member(
    State(state): State<AppState>,
    Path((tenant_id, org_id, user_id)): Path<(String, String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req
        .extensions()
        .get::<PlatformUser>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Not authenticated".into(),
        })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    )
    .bind(&tenant_id)
    .bind(&platform_user.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "tenant_not_found",
        message: "Tenant not found".into(),
    })?;

    let result = sqlx::query(
        "DELETE FROM organization_members \
         WHERE organization_id = $1 AND user_id = $2 AND tenant_id = $3",
    )
    .bind(&org_id)
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "member_not_found",
            message: "Member not found in this organization".into(),
        });
    }

    Ok(Json(serde_json::json!({ "message": "Member removed" })))
}

/// GET /platform/self-serve/tenants/{id}/users/{user_id}/detail
pub async fn get_tenant_user_detail(
    State(state): State<AppState>,
    Path((tenant_id, user_id)): Path<(String, String)>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    let platform_user = req.extensions().get::<PlatformUser>().cloned()
        .ok_or(AppError::Unauthorized { error: "unauthorized", message: "Not authenticated".into() })?;

    sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = $1 AND owner_user_id = $2 AND status != 'deleted'",
    ).bind(&tenant_id).bind(&platform_user.user_id)
    .fetch_optional(&state.db).await?
    .ok_or(AppError::NotFound { error: "tenant_not_found", message: "Tenant not found".into() })?;

    let user = sqlx::query(
        "SELECT id, email, name, email_verified, COALESCE(login_attempts, 0) AS login_attempts, \
         locked_until, created_at \
         FROM users WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .fetch_optional(&state.db).await?
    .ok_or(AppError::NotFound { error: "user_not_found", message: "User not found".into() })?;

    // Get roles, permissions, providers, active sessions in one query
    let detail: (serde_json::Value, serde_json::Value, serde_json::Value, i64) = sqlx::query_as(
        "SELECT \
           COALESCE((SELECT jsonb_agg(r.name) FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1), '[]'::jsonb), \
           COALESCE((SELECT jsonb_agg(DISTINCT val) FROM (SELECT jsonb_array_elements_text(r.permissions_jsonb) AS val FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1) sub), '[]'::jsonb), \
           COALESCE((SELECT jsonb_agg(provider) FROM credentials WHERE user_id = $1), '[]'::jsonb), \
           (SELECT COUNT(*) FROM refresh_tokens WHERE user_id = $1 AND tenant_id = $2 AND revoked = FALSE AND expires_at > NOW())",
    )
    .bind(&user_id)
    .bind(&tenant_id)
    .fetch_one(&state.db).await?;

    Ok(Json(serde_json::json!({
        "id": user.get::<String, _>("id"),
        "email": user.get::<String, _>("email"),
        "name": user.get::<Option<String>, _>("name"),
        "email_verified": user.get::<bool, _>("email_verified"),
        "login_attempts": user.get::<i32, _>("login_attempts"),
        "locked_until": user.get::<Option<chrono::DateTime<Utc>>, _>("locked_until").map(|d| d.to_rfc3339()),
        "roles": detail.0,
        "permissions": detail.1,
        "providers": detail.2,
        "active_sessions": detail.3,
        "created_at": user.get::<chrono::DateTime<Utc>, _>("created_at").to_rfc3339(),
    })))
}

/// GET /platform/login — Returning platform users sign in here.
/// After authentication they always land in /platform/console, where
/// tenant creation and setup now live.
pub async fn platform_login_page(State(state): State<AppState>) -> impl IntoResponse {
    if let Err(e) = ensure_platform_oauth_clients(&state).await {
        tracing::warn!(error = ?e, "Failed to ensure platform OAuth clients for login");
    }

    let page = render_platform_login_page_html(&state);
    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
            (axum::http::header::HeaderName::from_static("content-security-policy"), page.csp),
        ],
        Html(page.html),
    )
}
