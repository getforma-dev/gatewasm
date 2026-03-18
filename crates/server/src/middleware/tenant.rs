use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Utc};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::error::AppError;
use crate::redirects::resolve_login_redirect;
use crate::security_config::TenantSecurityConfig;
use crate::AppState;

/// Slug extracted from the URL path `/t/{slug}/...`, stored in request extensions
/// by the pre-routing `rewrite_tenant_path` middleware.
#[derive(Debug, Clone)]
pub struct PathSlug(pub String);

/// Tenant context extracted from the request and injected into extensions.
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: String,
    pub slug: String,
    pub name: String,
    pub allowed_providers: Vec<String>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub allowed_origins: Vec<String>,
    pub status: String,
    pub signup_mode: String,
    // Plan / trial
    pub plan: String,
    pub trial_ends_at: Option<DateTime<Utc>>,
    pub owner_user_id: Option<String>,
    // Branding / hosted login
    pub branding_logo_url: Option<String>,
    pub branding_primary_color: String,
    pub branding_app_name: Option<String>,
    pub branding_custom_css: Option<String>,
    pub login_redirect_url: String,
    // Security config (from JSONB column)
    pub security_config: TenantSecurityConfig,
}

impl TenantContext {
    /// Construct a `TenantContext` from a database row and the platform server URL.
    ///
    /// Both the slug-based lookup (extract_tenant) and the API-key lookup
    /// (management_auth) share the same field mapping, so this avoids duplication.
    pub fn from_db_row(row: &sqlx::postgres::PgRow, server_url: &str) -> Self {
        let providers_json: serde_json::Value = row.get("allowed_providers");
        let origins_json: serde_json::Value = row.get("allowed_origins");
        let allowed_providers: Vec<String> =
            serde_json::from_value(providers_json).unwrap_or_default();
        let allowed_origins: Vec<String> =
            serde_json::from_value(origins_json).unwrap_or_default();

        let slug: String = row.get("slug");
        let login_redirect_url: Option<String> = row.get("login_redirect_url");
        let resolved_login_redirect =
            resolve_login_redirect(server_url, &slug, login_redirect_url.as_deref());

        let security_config_json: serde_json::Value = row.get("security_config");
        let security_config = TenantSecurityConfig::from_json(&security_config_json);

        Self {
            tenant_id: row.get("id"),
            slug,
            name: row.get("name"),
            allowed_providers,
            google_client_id: row.get("google_client_id"),
            google_client_secret: row.get("google_client_secret"),
            github_client_id: row.get("github_client_id"),
            github_client_secret: row.get("github_client_secret"),
            allowed_origins,
            status: row.get("status"),
            signup_mode: row
                .get::<Option<String>, _>("signup_mode")
                .unwrap_or_else(|| "open".to_string()),
            plan: row
                .get::<Option<String>, _>("plan")
                .unwrap_or_else(|| "trial".to_string()),
            trial_ends_at: row.get("trial_ends_at"),
            owner_user_id: row.get("owner_user_id"),
            branding_logo_url: row.get("branding_logo_url"),
            branding_primary_color: row
                .get::<Option<String>, _>("branding_primary_color")
                .unwrap_or_else(|| "#6366f1".to_string()),
            branding_app_name: row.get("branding_app_name"),
            branding_custom_css: row.get("branding_custom_css"),
            login_redirect_url: resolved_login_redirect,
            security_config,
        }
    }
}

/// Simple in-memory tenant cache (slug → TenantContext).
/// In production, use something like moka or Redis. This is good enough for v1.
#[derive(Clone, Default)]
pub struct TenantCache {
    inner: Arc<RwLock<HashMap<String, (TenantContext, std::time::Instant)>>>,
}

impl TenantCache {
    const TTL_SECS: u64 = 300; // 5-minute cache

    pub fn get(&self, slug: &str) -> Option<TenantContext> {
        let map = self.inner.read().ok()?;
        let (ctx, inserted_at) = map.get(slug)?;
        if inserted_at.elapsed().as_secs() < Self::TTL_SECS {
            Some(ctx.clone())
        } else {
            None
        }
    }

    pub fn set(&self, slug: String, ctx: TenantContext) {
        if let Ok(mut map) = self.inner.write() {
            map.insert(slug, (ctx, std::time::Instant::now()));
        }
    }

    /// Remove a tenant from cache (e.g., after config update).
    pub fn invalidate(&self, slug: &str) {
        if let Ok(mut map) = self.inner.write() {
            map.remove(slug);
        }
    }
}

/// Middleware that extracts the tenant from the URL path, header, or subdomain.
///
/// Extraction priority:
/// 1. `PathSlug` from extensions (set by `rewrite_tenant_path` for `/t/{slug}/...` URLs)
/// 2. `X-Tenant-Slug` header (for API clients / testing)
/// 3. Subdomain from `Host` header (e.g. `acme.gatewasm.io` → slug = `acme`)
pub async fn extract_tenant(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // 1. Try PathSlug from extensions (set by rewrite_tenant_path)
    let slug = req.extensions().get::<PathSlug>().map(|s| s.0.clone());

    // 2. Fall back to X-Tenant-Slug header
    let slug = slug.or_else(|| {
        req.headers()
            .get("x-tenant-slug")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });

    // 3. Fall back to subdomain extraction from Host header
    let slug = slug.or_else(|| {
        let raw_host = req.headers().get("host")?.to_str().ok()?;
        let platform_domain = state.config.platform_domain.as_deref()?;
        // Strip port if present (e.g., "acme.gatewasm.io:3001" → "acme.gatewasm.io")
        let host = raw_host.split(':').next().unwrap_or(raw_host);
        // e.g., host = "acme.gatewasm.io", platform_domain = "gatewasm.io"
        if host.ends_with(platform_domain) && host.len() > platform_domain.len() + 1 {
            let prefix = &host[..host.len() - platform_domain.len() - 1];
            // Only take the first subdomain segment (no dots)
            if !prefix.contains('.') && !prefix.is_empty() {
                return Some(prefix.to_string());
            }
        }
        None
    });

    let slug = slug.ok_or(AppError::BadRequest {
        error: "missing_tenant",
        message: "Could not determine tenant. Use /t/{slug}/... URL, X-Tenant-Slug header, or a tenant subdomain.".into(),
    })?;

    // Look up tenant (cache → DB)
    let ctx = if let Some(cached) = state.tenant_cache.get(&slug) {
        cached
    } else {
        let row = sqlx::query(
            "SELECT id, name, slug, allowed_providers, \
             google_client_id, google_client_secret, github_client_id, github_client_secret, \
             allowed_origins, status, signup_mode, \
             branding_logo_url, branding_primary_color, \
             branding_app_name, branding_custom_css, \
             login_redirect_url, \
             plan, trial_ends_at, owner_user_id, \
             COALESCE(security_config, '{}') as security_config \
             FROM tenants WHERE slug = $1",
        )
        .bind(&slug)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to look up tenant");
            AppError::InternalError {
                error: "server_error",
                message: "Failed to look up tenant".into(),
            }
        })?
        .ok_or(AppError::NotFound {
            error: "tenant_not_found",
            message: format!("No tenant with slug '{}'", slug),
        })?;

        let ctx = TenantContext::from_db_row(&row, &state.config.server_url);

        state.tenant_cache.set(slug.clone(), ctx.clone());
        ctx
    };

    // Check tenant is active
    if ctx.status != "active" {
        return Err(AppError::Forbidden {
            error: "tenant_suspended",
            message: "This tenant has been suspended".into(),
        });
    }

    // Check trial expiry
    if ctx.plan == "trial" {
        if let Some(trial_ends) = ctx.trial_ends_at {
            if trial_ends < Utc::now() {
                return Err(AppError::Forbidden {
                    error: "trial_expired",
                    message: "Trial has expired. Visit /get-started to upgrade.".into(),
                });
            }
        }
    }

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}
