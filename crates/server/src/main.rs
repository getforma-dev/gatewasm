use std::{net::SocketAddr, time::Duration};

use axum::{
    body::Body,
    http::{
        header::{LOCATION, ORIGIN, USER_AGENT},
        Request, Response,
    },
};

use gatewasm_server::compliance;
use gatewasm_server::config::AppConfig;
use gatewasm_server::ir;
use gatewasm_server::jwt::keys::KeyPair;
use gatewasm_server::middleware::tenant::TenantCache;
use gatewasm_server::ws::{handler::ConnectionLimits, EventBus};
use gatewasm_server::ws::ticket::TicketStore;
use gatewasm_server::{assets, db, handlers, middleware, AppState};
use sqlx::postgres::PgPool;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let _ = dotenvy::dotenv();

    let config = AppConfig::from_env();
    tracing::info!(host = %config.host, port = %config.port, "Starting fAuth server");
    tracing::info!(compliance_mode = %config.compliance.mode, "Compliance mode");

    // ── Compliance Startup Validation ────────────────────────────────
    if let Some(spec) = compliance::load_spec(&config.compliance.mode) {
        let results = compliance::validator::validate_compliance(&config.compliance, &spec);
        if !compliance::validator::print_and_check(&results, &spec) {
            std::process::exit(1);
        }
    }

    // Database
    let pool = db::create_pool(&config.database_url)
        .await
        .expect("Failed to create database pool");
    tracing::info!("Database connected");

    db::cleanup_expired(&pool).await;
    bootstrap_platform_plane(&pool, &config).await;

    // JWT keys (EdDSA / Ed25519)
    let keys = KeyPair::from_pem_or_generate(config.ed25519_private_key_pem.as_deref());

    // Asset manifest (content-hashed filenames)
    let asset_manifest = assets::load_manifest();
    tracing::info!(
        build_hash = %asset_manifest.build_hash,
        assets = asset_manifest.assets.len(),
        routes = asset_manifest.routes.len(),
        source = if assets::use_filesystem_assets() { "filesystem" } else { "embedded" },
        "Loaded asset manifest"
    );

    // Load IR modules from the same asset source as the manifest so SSR,
    // the IR walker, and hydrated bundles stay aligned in development.
    let (render_modes, ir_modules) = ir::load_ir_modules(&asset_manifest);

    let (shutdown_tx, _) = tokio::sync::watch::channel(false);

    let state = AppState {
        db: pool,
        config: config.clone(),
        keys,
        tenant_cache: TenantCache::default(),
        asset_manifest,
        render_modes,
        ir_modules,
        event_bus: EventBus::new(),
        ticket_store: TicketStore::new(config.ws_ticket_ttl_secs),
        connection_limits: ConnectionLimits::new(),
        shutdown_tx,
    };

    // Clone pool/keys/cache for background workers before state is moved
    let retry_pool = state.db.clone();
    let bg_tenant_cache = state.tenant_cache.clone();
    let ssf_pool = state.db.clone();
    let ssf_keys = state.keys.clone();
    let shutdown_tx_for_signal = state.shutdown_tx.clone();
    let bg_ticket_store = state.ticket_store.clone();

    // Router
    let trace_layer = tower_http::trace::TraceLayer::new_for_http()
        .make_span_with(|request: &Request<Body>| {
            let request_id = request
                .headers()
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let path = request.uri().path();
            tracing::info_span!(
                "http.request",
                request_id = %request_id,
                method = %request.method(),
                path = %path,
                plane = %auth_plane_for_path(path),
            )
        })
        .on_request(|request: &Request<Body>, _span: &tracing::Span| {
            let request_id = request
                .headers()
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let user_agent = request
                .headers()
                .get(USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let origin = request
                .headers()
                .get(ORIGIN)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            tracing::info!(
                request_id = %request_id,
                user_agent = %user_agent,
                origin = %origin,
                "HTTP request started"
            );
        })
        .on_response(
            |response: &Response<Body>, latency: Duration, span: &tracing::Span| {
                let status = response.status().as_u16();
                let latency_ms = latency.as_millis() as u64;
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("");
                if (300..400).contains(&status) {
                    tracing::warn!(
                        parent: span,
                        status,
                        latency_ms,
                        location = %location,
                        "HTTP redirect response"
                    );
                } else if status >= 500 {
                    tracing::error!(
                        parent: span,
                        status,
                        latency_ms,
                        "HTTP server error response"
                    );
                } else if status >= 400 {
                    tracing::warn!(
                        parent: span,
                        status,
                        latency_ms,
                        "HTTP client error response"
                    );
                } else {
                    tracing::info!(
                        parent: span,
                        status,
                        latency_ms,
                        "HTTP response completed"
                    );
                }
            },
        )
        .on_failure(
            |error: tower_http::classify::ServerErrorsFailureClass,
             latency: Duration,
             span: &tracing::Span| {
                tracing::error!(
                    parent: span,
                    error = ?error,
                    latency_ms = latency.as_millis() as u64,
                    "HTTP request failed"
                );
            },
        );

    let cors_state = state.clone();
    let app = handlers::create_router(state)
        .layer(axum::middleware::from_fn_with_state(
            cors_state,
            middleware::cors::dynamic_cors,
        ))
        .layer(trace_layer)
        .layer(axum::middleware::from_fn(
            middleware::request_id::add_request_id,
        ));

    // Background webhook retry loop + DPoP jti cleanup + PAR cleanup
    tokio::spawn(async move {
        let mut tick: u64 = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            handlers::webhook_engine::process_retries(&retry_pool).await;
            tick += 1;
            // Clean up expired DPoP jti entries + PAR requests + tenant cache every ~5 minutes (10 ticks * 30s)
            if tick % 10 == 0 {
                middleware::dpop::cleanup_expired_jti(&retry_pool).await;
                handlers::par::cleanup_expired_par(&retry_pool).await;
                bg_tenant_cache.evict_expired();
                bg_ticket_store.cleanup_expired();
                db::cleanup_stale_refresh_tokens(&retry_pool).await;
            }
        }
    });

    // Background SSF event engine (LISTEN/NOTIFY + 1s fallback poll)
    tokio::spawn(async move {
        handlers::ssf_engine::run_ssf_worker(ssf_pool, ssf_keys).await;
    });

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid address");

    tracing::info!(%addr, "fAuth server listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|_| panic!("Failed to bind to {} — is the port already in use?", addr));

    // Graceful shutdown
    // `into_make_service_with_connect_info` injects ConnectInfo<SocketAddr> into each
    // request so the rate limiter can extract the client's peer IP address.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        let _ = shutdown_tx_for_signal.send(true);
    })
    .await
    .expect("Server exited with error");
}

async fn bootstrap_platform_plane(pool: &PgPool, config: &AppConfig) {
    let mut providers = vec!["email".to_string()];
    if config.google_client_id.is_some() && config.google_client_secret.is_some() {
        providers.push("google".to_string());
    }
    if config.github_client_id.is_some() && config.github_client_secret.is_some() {
        providers.push("github".to_string());
    }

    let providers_json =
        serde_json::to_value(&providers).unwrap_or_else(|_| serde_json::json!(["email"]));
    if let Err(e) = sqlx::query(
        "UPDATE tenants SET allowed_providers = $1, updated_at = NOW() WHERE id = 'ten_platform'",
    )
    .bind(providers_json)
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "Failed to update platform tenant provider set");
    } else {
        tracing::info!(providers = ?providers, "Platform tenant auth providers synced");
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO platform_accounts (user_id, status, created_at, updated_at)
         SELECT u.id, 'active', NOW(), NOW()
         FROM users u
         WHERE u.tenant_id = 'ten_platform'
         ON CONFLICT (user_id) DO NOTHING",
    )
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "Failed to backfill platform_accounts");
    }

    for email in &config.platform_owner_emails {
        match sqlx::query_scalar::<_, String>(
            "SELECT id FROM users WHERE tenant_id = 'ten_platform' AND lower(email) = lower($1) LIMIT 1",
        )
        .bind(email)
        .fetch_optional(pool)
        .await
        {
            Ok(Some(user_id)) => {
                if let Err(e) = sqlx::query(
                    "INSERT INTO user_roles (user_id, role_id, tenant_id)
                     VALUES ($1, 'role_platform_owner', 'ten_platform')
                     ON CONFLICT DO NOTHING",
                )
                .bind(&user_id)
                .execute(pool)
                .await
                {
                    tracing::warn!(email = %email, error = %e, "Failed to assign platform_owner role");
                } else {
                    tracing::info!(email = %email, user_id = %user_id, "Platform owner role ensured");
                }
            }
            Ok(None) => {
                tracing::warn!(email = %email, "Configured platform owner email has no platform account yet");
            }
            Err(e) => {
                tracing::warn!(email = %email, error = %e, "Failed to resolve configured platform owner email");
            }
        }
    }

    // ── Backfill: ensure platform_members row for users who own tenants ──
    // Each tenant owner gets a self-referencing row (account_owner_user_id = user_id)
    // indicating they are the account owner of their own team.
    let tenant_owners = sqlx::query_as::<_, (String,)>(
        "SELECT DISTINCT owner_user_id FROM tenants
         WHERE owner_user_id IS NOT NULL AND status != 'deleted' AND id != 'ten_platform'"
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (owner_user_id,) in &tenant_owners {
        sqlx::query(
            "INSERT INTO platform_members (id, account_owner_user_id, user_id, status, created_at, updated_at)
             VALUES ($1, $2, $2, 'active', NOW(), NOW())
             ON CONFLICT (account_owner_user_id, user_id) DO NOTHING"
        )
        .bind(format!("pmem_{}", uuid::Uuid::new_v4()))
        .bind(owner_user_id)
        .execute(pool)
        .await
        .ok();
    }

    if !tenant_owners.is_empty() {
        tracing::info!(count = tenant_owners.len(), "platform_members: backfilled tenant owners");
    }
}

fn auth_plane_for_path(path: &str) -> &'static str {
    if path.starts_with("/platform/admin") {
        "owner"
    } else if path.starts_with("/platform") {
        "platform"
    } else if path.starts_with("/t/") {
        "tenant"
    } else if path.starts_with("/manage") {
        "manage_api"
    } else if path.starts_with("/internal") {
        "internal"
    } else {
        "public"
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received, draining connections...");
}
