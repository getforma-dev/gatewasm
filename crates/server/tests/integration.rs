use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use data_encoding::BASE32_NOPAD;
use ring::hmac;
use sqlx::postgres::PgPool;
use tower::ServiceExt;

use gatewasm_server::assets::{self as gw_assets};
use gatewasm_server::compliance::ComplianceConfig;
use gatewasm_server::config::AppConfig;
use gatewasm_server::db;
use gatewasm_server::handlers;
use gatewasm_server::jwt::keys::KeyPair;
use gatewasm_server::jwt::signing;
use gatewasm_server::middleware::tenant::TenantCache;
use gatewasm_server::AppState;

const TEST_PLATFORM_KEY: &str = "plat_test-admin-key-for-integration-tests";
const TEST_TENANT_SLUG: &str = "test-tenant";

/// Get the test database URL from environment or use default (docker-compose db-test on port 5433)
fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://gatewasm:gatewasm@localhost:5433/gatewasm_test".to_string())
}

/// Create a test app with a unique schema per test to avoid conflicts.
/// Each test gets its own PostgreSQL schema for isolation.
async fn test_app() -> axum::Router {
    test_app_with_pool().await.0
}

/// Create a test app with a custom compliance config.
async fn test_app_with_compliance(compliance: ComplianceConfig) -> (axum::Router, PgPool) {
    let database_url = test_database_url();

    let schema = format!("test_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

    let setup_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    sqlx::raw_sql(&format!("CREATE SCHEMA IF NOT EXISTS \"{}\"", schema))
        .execute(&setup_pool)
        .await
        .expect("Failed to create test schema");
    setup_pool.close().await;

    let schema_for_hook = schema.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |conn, _meta| {
            let schema = schema_for_hook.clone();
            Box::pin(async move {
                use sqlx::Executor;
                conn.execute(format!("SET search_path TO \"{}\"", schema).as_str())
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    db::run_migrations_on_pool(&pool)
        .await
        .expect("Failed to run migrations");

    create_test_tenant_in_db(&pool, TEST_TENANT_SLUG, "Test Tenant").await;

    let config = AppConfig {
        port: 3099,
        host: "127.0.0.1".to_string(),
        server_url: "https://auth.gatewasm.test".to_string(),
        database_url,
        ed25519_private_key_pem: None,
        jwt_issuer: "test".to_string(),
        jwt_audience: "test-app".to_string(),
        access_token_expiry: 900,
        refresh_token_expiry: 2592000,
        google_client_id: None,
        google_client_secret: None,
        github_client_id: None,
        github_client_secret: None,
        allowed_origins: vec![],
        csrf_secret: "test-csrf-secret-at-least-32-bytes-long".to_string(),
        mfa_encryption_key: "test-mfa-encryption-key".to_string(),
        mfa_encryption_key_previous: vec![],
        platform_admin_key: Some(TEST_PLATFORM_KEY.to_string()),
        platform_domain: Some("gatewasm.test".to_string()),
        platform_owner_emails: vec![],
        resend_api_key: None,
        email_from_address: "noreply@getforma.dev".to_string(),
        email_from_name: "Test Auth".to_string(),
        smtp_host: None,
        smtp_port: 587,
        smtp_username: None,
        smtp_password: None,
        smtp_from_email: None,
        smtp_from_name: "Test Auth".to_string(),
        smtp_starttls: true,
        compliance,
        ssf_signing_key_pem: None,
        maxmind_db_path: None,
        ws_ticket_ttl_secs: 30,
        suppress_email: true,
    };

    let keys = KeyPair::from_pem_or_generate(None);

    let (shutdown_tx, _) = tokio::sync::watch::channel(false);
    let state = AppState {
        db: pool.clone(),
        config,
        keys,
        tenant_cache: TenantCache::default(),
        asset_manifest: gw_assets::load_manifest(),
        render_modes: std::collections::HashMap::new(),
        ir_modules: std::collections::HashMap::new(),
        event_bus: gatewasm_server::ws::EventBus::new(),
        ticket_store: gatewasm_server::ws::ticket::TicketStore::new(30),
        connection_limits: gatewasm_server::ws::handler::ConnectionLimits::new(),
        shutdown_tx,
    };

    (handlers::create_router_without_rate_limit(state), pool)
}

/// Create a test app, returning both the router and the DB pool.
/// Automatically creates a test tenant in the DB.
async fn test_app_with_pool() -> (axum::Router, PgPool) {
    let database_url = test_database_url();

    // Create a unique schema name for this test to avoid conflicts
    let schema = format!("test_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

    // First, create the schema using a one-off connection
    let setup_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    sqlx::raw_sql(&format!("CREATE SCHEMA IF NOT EXISTS \"{}\"", schema))
        .execute(&setup_pool)
        .await
        .expect("Failed to create test schema");
    setup_pool.close().await;

    // Now create the main pool with after_connect that sets search_path on every connection
    let schema_for_hook = schema.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .after_connect(move |conn, _meta| {
            let schema = schema_for_hook.clone();
            Box::pin(async move {
                use sqlx::Executor;
                conn.execute(format!("SET search_path TO \"{}\"", schema).as_str())
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Run migrations in the isolated schema
    db::run_migrations_on_pool(&pool)
        .await
        .expect("Failed to run migrations");

    // Create a default test tenant
    create_test_tenant_in_db(&pool, TEST_TENANT_SLUG, "Test Tenant").await;

    let config = AppConfig {
        port: 3099,
        host: "127.0.0.1".to_string(),
        server_url: "https://auth.gatewasm.test".to_string(),
        database_url,
        ed25519_private_key_pem: None,
        jwt_issuer: "test".to_string(),
        jwt_audience: "test-app".to_string(),
        access_token_expiry: 900,
        refresh_token_expiry: 2592000,
        google_client_id: None,
        google_client_secret: None,
        github_client_id: None,
        github_client_secret: None,
        allowed_origins: vec![],
        csrf_secret: "test-csrf-secret-at-least-32-bytes-long".to_string(),
        mfa_encryption_key: "test-mfa-encryption-key".to_string(),
        mfa_encryption_key_previous: vec![],
        platform_admin_key: Some(TEST_PLATFORM_KEY.to_string()),
        platform_domain: Some("gatewasm.test".to_string()),
        platform_owner_emails: vec![],
        resend_api_key: None,
        email_from_address: "noreply@getforma.dev".to_string(),
        email_from_name: "Test Auth".to_string(),
        smtp_host: None,
        smtp_port: 587,
        smtp_username: None,
        smtp_password: None,
        smtp_from_email: None,
        smtp_from_name: "Test Auth".to_string(),
        smtp_starttls: true,
        compliance: ComplianceConfig::default(),
        ssf_signing_key_pem: None,
        maxmind_db_path: None,
        ws_ticket_ttl_secs: 30,
        suppress_email: true,
    };

    let keys = KeyPair::from_pem_or_generate(None);

    let (shutdown_tx, _) = tokio::sync::watch::channel(false);
    let state = AppState {
        db: pool.clone(),
        config,
        keys,
        tenant_cache: TenantCache::default(),
        asset_manifest: gw_assets::load_manifest(),
        render_modes: std::collections::HashMap::new(),
        ir_modules: std::collections::HashMap::new(),
        event_bus: gatewasm_server::ws::EventBus::new(),
        ticket_store: gatewasm_server::ws::ticket::TicketStore::new(30),
        connection_limits: gatewasm_server::ws::handler::ConnectionLimits::new(),
        shutdown_tx,
    };

    (handlers::create_router_without_rate_limit(state), pool)
}

/// Insert a test tenant directly into the DB.
async fn create_test_tenant_in_db(pool: &PgPool, slug: &str, name: &str) {
    let tenant_id = format!("ten_{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO tenants (id, name, slug, owner_email, allowed_providers, status) VALUES ($1, $2, $3, 'admin@test.com', '[\"email\",\"google\",\"github\"]'::jsonb, 'active') ON CONFLICT (slug) DO NOTHING",
    )
    .bind(&tenant_id)
    .bind(name)
    .bind(slug)
    .execute(pool)
    .await
    .expect("Failed to create test tenant");
}

/// Insert a test tenant with a known management API key. Returns (tenant_id, mgmt_key).
async fn create_test_tenant_with_mgmt_key(
    pool: &PgPool,
    slug: &str,
    name: &str,
) -> (String, String) {
    let tenant_id = format!("ten_{}", uuid::Uuid::new_v4());
    let mgmt_key = format!("mgmt_{}", uuid::Uuid::new_v4());
    let prefix = &mgmt_key[..mgmt_key.len().min(13)];
    let hash = signing::hash_token(&mgmt_key);

    sqlx::query(
        "INSERT INTO tenants (id, name, slug, owner_email, api_key_hash, api_key_prefix, allowed_providers, status) VALUES ($1, $2, $3, 'admin@test.com', $4, $5, '[\"email\"]'::jsonb, 'active')",
    )
    .bind(&tenant_id)
    .bind(name)
    .bind(slug)
    .bind(&hash)
    .bind(prefix)
    .execute(pool)
    .await
    .expect("Failed to create test tenant with mgmt key");

    // Create entitlements so webhooks/roles/orgs are enabled
    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations) \
         VALUES ($1, 'pro', 1000, 100, true, true, true) ON CONFLICT DO NOTHING",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("Failed to create test tenant entitlements");

    (tenant_id, mgmt_key)
}

async fn create_pending_tenant_invite(
    pool: &PgPool,
    tenant_id: &str,
    email: &str,
    role_id: &str,
) -> String {
    let invite_id = format!("tinv_{}", uuid::Uuid::new_v4());
    let invite_token = format!("tinv_{}", uuid::Uuid::new_v4());
    let token_hash = signing::hash_token(&invite_token);

    sqlx::query(
        "INSERT INTO tenant_user_invites
         (id, tenant_id, email, role_id, token_hash, status, invited_by_user_id, expires_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, 'pending', NULL, NOW() + INTERVAL '72 hours', NOW(), NOW())",
    )
    .bind(&invite_id)
    .bind(tenant_id)
    .bind(email)
    .bind(role_id)
    .bind(&token_hash)
    .execute(pool)
    .await
    .expect("Failed to create tenant invite");

    invite_token
}

// ── Request helpers ─────────────────────────────────────────────────────

fn json_request(method: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-tenant-slug", TEST_TENANT_SLUG)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn json_request_with_slug(
    method: &str,
    uri: &str,
    body: serde_json::Value,
    slug: &str,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-tenant-slug", slug)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn json_request_no_tenant(method: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn get_request(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn get_request_with_tenant(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("x-tenant-slug", TEST_TENANT_SLUG)
        .body(Body::empty())
        .unwrap()
}

fn auth_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {}", token))
        .header("x-tenant-slug", TEST_TENANT_SLUG)
        .body(Body::empty())
        .unwrap()
}

fn auth_delete(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {}", token))
        .header("x-tenant-slug", TEST_TENANT_SLUG)
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap()
}

fn platform_request(method: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", TEST_PLATFORM_KEY))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn platform_get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {}", TEST_PLATFORM_KEY))
        .body(Body::empty())
        .unwrap()
}

fn platform_delete(uri: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {}", TEST_PLATFORM_KEY))
        .body(Body::empty())
        .unwrap()
}

fn mgmt_get(uri: &str, mgmt_key: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap()
}

fn mgmt_request(method: &str, uri: &str, body: serde_json::Value, mgmt_key: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn mgmt_delete(uri: &str, mgmt_key: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Register a user and return (access_token, refresh_token, user_id).
async fn register_user(
    app: &axum::Router,
    email: &str,
    password: &str,
    name: Option<&str>,
) -> (String, String, String) {
    let mut body = serde_json::json!({
        "email": email,
        "password": password,
    });
    if let Some(n) = name {
        body["name"] = serde_json::Value::String(n.to_string());
    }
    let res = app
        .clone()
        .oneshot(json_request("POST", "/auth/register", body))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "register_user failed for {}",
        email
    );
    let body = body_json(res).await;
    (
        body["access_token"].as_str().unwrap().to_string(),
        body["refresh_token"].as_str().unwrap().to_string(),
        body["user"]["id"].as_str().unwrap().to_string(),
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Core Auth Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn health_check() {
    let app = test_app().await;
    let res = app.oneshot(get_request("/health")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["db_status"], "ok");
}

#[tokio::test]
async fn register_success() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "alice@example.com",
                "password": "Str0ngP@ss",
                "name": "Alice"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["user"]["email"], "alice@example.com");
    assert_eq!(body["user"]["name"], "Alice");
    assert_eq!(body["user"]["roles"][0], "user");
}

#[tokio::test]
async fn register_duplicate_409() {
    let app = test_app().await;
    let body = serde_json::json!({
        "email": "dup@example.com",
        "password": "Str0ngP@ss",
    });

    let res = app
        .clone()
        .oneshot(json_request("POST", "/auth/register", body.clone()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let res = app
        .oneshot(json_request("POST", "/auth/register", body))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let err = body_json(res).await;
    assert_eq!(err["error"], "user_exists");
}

#[tokio::test]
async fn register_weak_password_400() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "weak@example.com",
                "password": "short",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let err = body_json(res).await;
    assert_eq!(err["error"], "weak_password");
}

#[tokio::test]
async fn invite_only_register_requires_invite_and_accepts_valid_invite() {
    let (app, pool) = test_app_with_pool().await;

    let tenant_id: String = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = $1")
        .bind(TEST_TENANT_SLUG)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tenants SET signup_mode = 'invite_only' WHERE id = $1")
        .bind(&tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "invite-only@example.com",
                "password": "Str0ngP@ss",
                "name": "Invite Only",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let err = body_json(res).await;
    assert_eq!(err["error"], "invite_required");

    let invite_token =
        create_pending_tenant_invite(&pool, &tenant_id, "invite-only@example.com", "role_admin")
            .await;

    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "invite-only@example.com",
                "password": "Str0ngP@ss",
                "name": "Invite Only",
                "invite_token": invite_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let roles = body["user"]["roles"].as_array().unwrap();
    assert!(roles.iter().any(|r| r.as_str() == Some("admin")));

    let invite_status: (String, Option<String>) = sqlx::query_as(
        "SELECT status, accepted_by_user_id FROM tenant_user_invites WHERE tenant_id = $1 AND lower(email) = lower($2) LIMIT 1",
    )
    .bind(&tenant_id)
    .bind("invite-only@example.com")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(invite_status.0, "accepted");
    assert_eq!(
        invite_status.1.as_deref(),
        body["user"]["id"].as_str(),
        "invite should be accepted by the new user",
    );
}

#[tokio::test]
async fn login_success() {
    let app = test_app().await;
    register_user(&app, "login@example.com", "Str0ngP@ss", None).await;

    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "login@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["access_token"].is_string());
    assert_eq!(body["user"]["email"], "login@example.com");
}

#[tokio::test]
async fn login_with_invite_token_accepts_invite_and_assigns_role() {
    let (app, pool) = test_app_with_pool().await;
    register_user(&app, "login-invite@example.com", "Str0ngP@ss", None).await;

    let tenant_id: String = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = $1")
        .bind(TEST_TENANT_SLUG)
        .fetch_one(&pool)
        .await
        .unwrap();
    let invite_token =
        create_pending_tenant_invite(&pool, &tenant_id, "login-invite@example.com", "role_admin")
            .await;

    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "login-invite@example.com",
                "password": "Str0ngP@ss",
                "invite_token": invite_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let roles = body["user"]["roles"].as_array().unwrap();
    assert!(roles.iter().any(|r| r.as_str() == Some("admin")));

    let status: String = sqlx::query_scalar(
        "SELECT status FROM tenant_user_invites WHERE tenant_id = $1 AND lower(email) = lower($2) LIMIT 1",
    )
    .bind(&tenant_id)
    .bind("login-invite@example.com")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "accepted");
}

#[tokio::test]
async fn login_wrong_password_401() {
    let app = test_app().await;
    register_user(&app, "wrong@example.com", "Str0ngP@ss", None).await;

    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "wrong@example.com",
                "password": "WrongPassword1",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let err = body_json(res).await;
    assert_eq!(err["error"], "invalid_credentials");
}

#[tokio::test]
async fn login_nonexistent_user_401() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "nobody@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn token_refresh_flow() {
    let app = test_app().await;
    let (_, refresh_token, _) =
        register_user(&app, "refresh@example.com", "Str0ngP@ss", None).await;

    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert_ne!(body["refresh_token"].as_str().unwrap(), refresh_token);
}

#[tokio::test]
async fn refresh_token_rejects_cross_tenant_exchange() {
    let (app, pool) = test_app_with_pool().await;
    let other_slug = "other-tenant";
    create_test_tenant_in_db(&pool, other_slug, "Other Tenant").await;

    let (_, refresh_token, _) =
        register_user(&app, "cross-tenant-refresh@example.com", "Str0ngP@ss", None).await;

    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }),
            other_slug,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_grant");
}

#[tokio::test]
async fn refresh_token_replay_rejected() {
    let app = test_app().await;
    let (_, refresh_token, _) = register_user(&app, "replay@example.com", "Str0ngP@ss", None).await;

    // First refresh succeeds
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": &refresh_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Replaying the same old token should fail
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": &refresh_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn jwks_endpoint_returns_eddsa() {
    let app = test_app().await;
    let res = app.oneshot(get_request("/auth/jwks")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let key = &body["keys"][0];
    assert_eq!(key["kty"], "OKP");
    assert_eq!(key["crv"], "Ed25519");
    assert_eq!(key["alg"], "EdDSA");
    assert_eq!(key["use"], "sig");
    assert!(key["x"].is_string());
    assert!(key["kid"].is_string());
}

#[tokio::test]
async fn test_jwks_includes_kid_and_metadata() {
    let app = test_app().await;
    let res = app.oneshot(get_request("/auth/jwks")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify Cache-Control header
    let cache_control = res
        .headers()
        .get("cache-control")
        .expect("Cache-Control header missing")
        .to_str()
        .unwrap();
    assert_eq!(cache_control, "public, max-age=3600");

    let body = body_json(res).await;
    let key = &body["keys"][0];

    // kid must be a non-empty base64url string (11 chars for 8 bytes)
    let kid = key["kid"].as_str().expect("kid must be a string");
    assert!(!kid.is_empty(), "kid must be non-empty");
    assert!(kid.len() >= 8, "kid should be at least 8 chars (base64url of 8 bytes)");

    // Metadata fields
    assert_eq!(key["use"], "sig");
    assert_eq!(key["alg"], "EdDSA");
    assert_eq!(key["kty"], "OKP");
    assert_eq!(key["crv"], "Ed25519");
    assert!(key["x"].is_string());
}

#[tokio::test]
async fn test_jwks_kid_is_stable() {
    let app = test_app().await;

    // First call
    let app2 = app.clone();
    let res1 = app.oneshot(get_request("/auth/jwks")).await.unwrap();
    let body1 = body_json(res1).await;
    let kid1 = body1["keys"][0]["kid"].as_str().unwrap().to_string();

    // Second call on same app (cloned router)
    let res2 = app2.oneshot(get_request("/auth/jwks")).await.unwrap();
    let body2 = body_json(res2).await;
    let kid2 = body2["keys"][0]["kid"].as_str().unwrap().to_string();

    assert_eq!(kid1, kid2, "kid must be deterministic (same key => same kid)");
    assert!(!kid1.is_empty());
}

#[tokio::test]
async fn auth_middleware_rejects_no_token() {
    let app = test_app().await;
    let res = app
        .oneshot(get_request_with_tenant("/auth/me"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_middleware_rejects_bad_token() {
    let app = test_app().await;
    let res = app
        .oneshot(auth_get("/auth/me", "invalid-jwt-token"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_middleware_accepts_valid_token() {
    let app = test_app().await;
    let (access_token, _, _) = register_user(
        &app,
        "authed@example.com",
        "Str0ngP@ss",
        Some("Authed User"),
    )
    .await;

    let res = app
        .oneshot(auth_get("/auth/me", &access_token))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["email"], "authed@example.com");
    assert_eq!(body["name"], "Authed User");
    assert_eq!(body["roles"][0], "user");
}

#[tokio::test]
async fn delete_account() {
    let app = test_app().await;
    let (access_token, _, _) =
        register_user(&app, "delete-me@example.com", "Str0ngP@ss", None).await;

    let res = app
        .clone()
        .oneshot(auth_delete("/auth/account", &access_token))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Login should now fail
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "delete-me@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_clears_cookie() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/logout",
            serde_json::json!({ "refresh_token": null }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let set_cookie_headers: Vec<String> = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok().map(|v| v.to_string()))
        .collect();
    assert!(set_cookie_headers
        .iter()
        .any(|cookie| cookie.contains("Max-Age=0")));
    assert!(set_cookie_headers
        .iter()
        .any(|cookie| cookie.contains("refresh_token=")));
}

#[tokio::test]
async fn oauth_authorize_rejects_unknown_client() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/auth/authorize?client_id=unknown&response_type=code&state=abc&code_challenge=xyz&code_challenge_method=S256&provider=google")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_client");
}

#[tokio::test]
async fn oauth_authorize_uses_global_provider_callback_url() {
    let (app, pool) = test_app_with_pool().await;

    let tenant_id: String = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = $1")
        .bind(TEST_TENANT_SLUG)
        .fetch_one(&pool)
        .await
        .unwrap();

    // Seed tenant-level OAuth credentials so authorize() can build provider redirect.
    sqlx::query(
        "UPDATE tenants SET google_client_id = $1, google_client_secret = $2, allowed_providers = '[\"email\",\"google\"]'::jsonb WHERE id = $3",
    )
    .bind("test-google-client-id")
    .bind("test-google-client-secret")
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    // Register a tenant-scoped oauth client used by /auth/authorize validation.
    sqlx::query(
        "INSERT INTO oauth_clients (id, redirect_uri, name, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind("test-oauth-client")
    .bind("http://localhost:3099/platform/login")
    .bind("Test OAuth Client")
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .uri("/auth/authorize?client_id=test-oauth-client&redirect_uri=http%3A%2F%2Flocalhost%3A3099%2Fplatform%2Flogin&response_type=code&state=abc123&code_challenge=abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234&code_challenge_method=S256&provider=google")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = res
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(location.contains("accounts.google.com/o/oauth2/v2/auth"));
    assert!(
        location.contains("redirect_uri=https%3A%2F%2Fauth.gatewasm.test%2Fauth%2Fcallback%2Fgoogle")
    );
    assert!(!location.contains(
        "redirect_uri=https%3A%2F%2Fauth.gatewasm.test%2Ft%2Ftest-tenant%2Fauth%2Fcallback%2Fgoogle"
    ));
}

#[tokio::test]
async fn oauth_authorize_persists_invite_token_hash() {
    let (app, pool) = test_app_with_pool().await;

    let tenant_id: String = sqlx::query_scalar("SELECT id FROM tenants WHERE slug = $1")
        .bind(TEST_TENANT_SLUG)
        .fetch_one(&pool)
        .await
        .unwrap();

    sqlx::query(
        "UPDATE tenants SET google_client_id = $1, google_client_secret = $2, allowed_providers = '[\"email\",\"google\"]'::jsonb WHERE id = $3",
    )
    .bind("test-google-client-id")
    .bind("test-google-client-secret")
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO oauth_clients (id, redirect_uri, name, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind("test-oauth-client-invite")
    .bind("http://localhost:3099/platform/login")
    .bind("Test OAuth Client Invite")
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let invite_token = "tinv_test_invite_token";
    let state = "state_invite_hash_test";
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/auth/authorize?client_id=test-oauth-client-invite&redirect_uri=http%3A%2F%2Flocalhost%3A3099%2Fplatform%2Flogin&response_type=code&state={}&code_challenge=abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234&code_challenge_method=S256&provider=google&invite_token={}",
                    state,
                    invite_token
                ))
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);

    let stored_hash: Option<String> =
        sqlx::query_scalar("SELECT invite_token_hash FROM oauth_states WHERE state = $1")
            .bind(state)
            .fetch_optional(&pool)
            .await
            .unwrap();
    let expected_hash = signing::hash_token(invite_token);
    assert_eq!(stored_hash.as_deref(), Some(expected_hash.as_str()));
}

#[tokio::test]
async fn oauth_callback_route_is_global_not_tenant_scoped() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/auth/callback/google")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // If this route were still tenant-scoped, we'd get tenant_not_found (404)
    // from tenant middleware. Global callback should reach OAuth handler and
    // fail on missing state.
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_state");
}

#[tokio::test]
async fn register_sets_refresh_cookie() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "cookie@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let all_set_cookie: Vec<String> = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .collect();

    // The active cookie should use __Host- prefix in HTTPS mode with Path=/
    assert!(
        all_set_cookie.iter().any(|cookie| cookie.contains("__Host-gw_rt_test-tenant=")
            && cookie.contains("HttpOnly")
            && cookie.contains("SameSite=None")
            && cookie.contains("Secure")
            && cookie.contains("Path=/")
            && !cookie.contains("Max-Age=0")),
        "Expected __Host- prefixed active cookie, got {:?}",
        all_set_cookie
    );
}

#[tokio::test]
async fn register_sets_root_scoped_platform_refresh_cookie() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "platform-cookie@example.com",
                "password": "Str0ngP@ss",
            }),
            "platform",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let all_set_cookie: Vec<String> = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .collect();

    assert!(
        all_set_cookie
            .iter()
            .any(|cookie| cookie.contains("__Host-gw_rt_platform=")
                && cookie.contains("Path=/")
                && !cookie.contains("Max-Age=0")),
        "Expected active __Host- prefixed root-scoped gw_rt_platform cookie, got {:?}",
        all_set_cookie
    );
    assert!(
        all_set_cookie.iter().any(|cookie| {
            cookie.contains("gw_rt_platform=")
                && !cookie.starts_with("__Host-")
                && cookie.contains("Path=/t/platform")
                && cookie.contains("Max-Age=0")
        }),
        "Expected stale /t/platform cookie cleanup header, got {:?}",
        all_set_cookie
    );
}

#[tokio::test]
async fn host_prefix_cookie_in_https_mode() {
    // The test server uses https://auth.gatewasm.test, so cookies should use __Host- prefix.
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "host-prefix@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let all_set_cookie: Vec<String> = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .collect();

    // Active cookie must have __Host- prefix, Path=/, Secure, and no Domain
    let active = all_set_cookie
        .iter()
        .find(|c| c.contains("__Host-gw_rt_test-tenant=") && !c.contains("Max-Age=0"))
        .expect("Missing __Host- prefixed active cookie");
    assert!(active.contains("Secure"), "Active cookie must have Secure");
    assert!(active.contains("Path=/"), "Active cookie must have Path=/");
    assert!(
        !active.contains("Path=/t/test-tenant"),
        "__Host- cookies must use Path=/ not tenant-scoped path"
    );
    assert!(
        !active.contains("Domain="),
        "__Host- cookies must not include Domain"
    );

    // Should also have a clear header for the pre-__Host- era unprefixed cookie
    assert!(
        all_set_cookie.iter().any(|c| c.contains("gw_rt_test-tenant=")
            && !c.starts_with("__Host-")
            && c.contains("Max-Age=0")),
        "Expected pre-__Host- migration cleanup header, got {:?}",
        all_set_cookie
    );

    // Should also have clear headers for legacy refresh_token variants
    assert!(
        all_set_cookie
            .iter()
            .any(|c| c.starts_with("refresh_token=") && c.contains("Max-Age=0")),
        "Expected legacy refresh_token cleanup header, got {:?}",
        all_set_cookie
    );
    assert!(
        all_set_cookie
            .iter()
            .any(|c| c.starts_with("__Host-refresh_token=") && c.contains("Max-Age=0")),
        "Expected __Host-refresh_token cleanup header, got {:?}",
        all_set_cookie
    );
}

#[tokio::test]
async fn host_prefix_cookie_extract_prefers_host_over_legacy() {
    // When a browser sends both __Host- and legacy cookies, __Host- should be preferred.
    let (app, _pool) = test_app_with_pool().await;
    let (_, refresh_token, _) =
        register_user(&app, "host-extract@test.com", "Str0ngP@ss", None).await;

    // Send refresh request with __Host- prefixed cookie name (production format)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header(
                    "cookie",
                    format!("__Host-gw_rt_test-tenant={}", refresh_token),
                )
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
}

#[tokio::test]
async fn verify_email_endpoint_is_public_and_marks_user_verified() {
    let (app, pool) = test_app_with_pool().await;
    let email = "verifyme@example.com";

    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": email,
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let token: Option<String> =
        sqlx::query_scalar("SELECT verification_token FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&pool)
            .await
            .unwrap();
    let token = token.expect("verification token should be present after register");

    let verify_req = Request::builder()
        .uri(format!(
            "/auth/verify-email?token={}",
            urlencoding::encode(&token)
        ))
        .header("x-tenant-slug", TEST_TENANT_SLUG)
        .body(Body::empty())
        .unwrap();

    let verify_res = app.clone().oneshot(verify_req).await.unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let verified: bool = sqlx::query_scalar("SELECT email_verified FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(verified);
}

#[tokio::test]
async fn verified_email_is_reflected_in_login_me_and_refresh_responses() {
    let (app, pool) = test_app_with_pool().await;
    let email = "verified-flags@example.com";
    let password = "Str0ngP@ss";

    let register_res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": email,
                "password": password,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(register_res.status(), StatusCode::CREATED);

    let token: Option<String> =
        sqlx::query_scalar("SELECT verification_token FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&pool)
            .await
            .unwrap();
    let token = token.expect("verification token should be present after register");

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/auth/verify-email?token={}",
                    urlencoding::encode(&token)
                ))
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let login_res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": email,
                "password": password,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(login_res.status(), StatusCode::OK);
    let login_body = body_json(login_res).await;
    assert_eq!(login_body["user"]["email_verified"], true);

    let access_token = login_body["access_token"].as_str().unwrap();
    let refresh_token = login_body["refresh_token"].as_str().unwrap();

    let me_res = app
        .clone()
        .oneshot(auth_get("/auth/me", access_token))
        .await
        .unwrap();
    assert_eq!(me_res.status(), StatusCode::OK);
    let me_body = body_json(me_res).await;
    assert_eq!(me_body["email_verified"], true);

    let refresh_res = app
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(refresh_res.status(), StatusCode::OK);
    let refresh_body = body_json(refresh_res).await;
    assert_eq!(refresh_body["user"]["email_verified"], true);
}

#[tokio::test]
async fn refresh_token_from_cookie() {
    let (app, _pool) = test_app_with_pool().await;
    let (_, refresh_token, _) =
        register_user(&app, "cookie-refresh@test.com", "Str0ngP@ss", None).await;

    // Send refresh request with cookie header instead of body
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header("cookie", format!("refresh_token={}", refresh_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
}

#[tokio::test]
async fn refresh_cookie_flow_prefers_active_token_over_stale_duplicates() {
    let (app, _pool) = test_app_with_pool().await;
    let (_, refresh_token_v1, _) =
        register_user(&app, "cookie-dup-refresh@test.com", "Str0ngP@ss", None).await;

    // Rotate once so v1 is revoked and v2 is active.
    let rotate_v2 = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token_v1
            }),
            TEST_TENANT_SLUG,
        ))
        .await
        .unwrap();
    assert_eq!(rotate_v2.status(), StatusCode::OK);
    let rotate_v2_body = body_json(rotate_v2).await;
    let refresh_token_v2 = rotate_v2_body["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Browser can send duplicate cookie names during migration. Old token first must still work.
    let refresh_v3 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header(
                    "cookie",
                    format!(
                        "gw_rt_test-tenant={}; gw_rt_test-tenant={}",
                        refresh_token_v1, refresh_token_v2
                    ),
                )
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refresh_v3.status(), StatusCode::OK);
    let refresh_v3_body = body_json(refresh_v3).await;
    let refresh_token_v3 = refresh_v3_body["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Repeat with stale token first again (v2 now stale, v3 active).
    let refresh_v4 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header(
                    "cookie",
                    format!(
                        "gw_rt_test-tenant={}; gw_rt_test-tenant={}",
                        refresh_token_v2, refresh_token_v3
                    ),
                )
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refresh_v4.status(), StatusCode::OK);
}

#[tokio::test]
async fn stale_cookie_refresh_does_not_trigger_family_revoke() {
    let (app, _pool) = test_app_with_pool().await;
    let (_, refresh_token_v1, _) =
        register_user(&app, "cookie-stale-no-revoke@test.com", "Str0ngP@ss", None).await;

    let rotate_v2 = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token_v1
            }),
            TEST_TENANT_SLUG,
        ))
        .await
        .unwrap();
    assert_eq!(rotate_v2.status(), StatusCode::OK);
    let rotate_v2_body = body_json(rotate_v2).await;
    let refresh_token_v2 = rotate_v2_body["refresh_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Try only stale cookie token. This should fail, but must NOT revoke the active family.
    let stale_cookie_refresh = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header("cookie", format!("gw_rt_test-tenant={}", refresh_token_v1))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_cookie_refresh.status(), StatusCode::UNAUTHORIZED);
    let stale_body = body_json(stale_cookie_refresh).await;
    assert_ne!(stale_body["error"], "token_reuse_detected");

    // Active token should still be valid after stale-cookie failure.
    let active_refresh = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token_v2
            }),
            TEST_TENANT_SLUG,
        ))
        .await
        .unwrap();
    assert_eq!(active_refresh.status(), StatusCode::OK);
}

#[tokio::test]
async fn body_token_takes_precedence_over_cookie() {
    let (app, _pool) = test_app_with_pool().await;
    let (_, refresh_token, _) =
        register_user(&app, "body-precedence@test.com", "Str0ngP@ss", None).await;

    // Send valid token in body AND an invalid token in cookie — body should win
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header("cookie", "refresh_token=invalid-cookie-token")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token",
                        "refresh_token": refresh_token
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["access_token"].is_string());
}

#[tokio::test]
async fn logout_from_cookie() {
    let (app, _pool) = test_app_with_pool().await;
    let (_, refresh_token, _) =
        register_user(&app, "cookie-logout@test.com", "Str0ngP@ss", None).await;

    // Logout with cookie instead of body token
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header("cookie", format!("refresh_token={}", refresh_token))
                .body(Body::from(b"{}".to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // The revoked token should no longer work for refresh
    let res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header("cookie", format!("refresh_token={}", refresh_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unsupported_grant_type_400() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "client_credentials",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "unsupported_grant_type");
}

#[tokio::test]
async fn invalid_email_format_400() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "not-an-email",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_request");
}

#[tokio::test]
async fn request_id_header() {
    let app = test_app().await;

    let app = app.layer(axum::middleware::from_fn(
        gatewasm_server::middleware::request_id::add_request_id,
    ));

    let res = app.oneshot(get_request("/health")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().get("x-request-id").is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// Tenant Middleware Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn missing_tenant_header_returns_400() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request_no_tenant(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "no-tenant@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "missing_tenant");
}

#[tokio::test]
async fn unknown_tenant_slug_returns_404() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "ghost@example.com",
                "password": "Str0ngP@ss",
            }),
            "nonexistent-tenant",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = body_json(res).await;
    assert_eq!(body["error"], "tenant_not_found");
}

#[tokio::test]
async fn suspended_tenant_returns_403() {
    let (app, pool) = test_app_with_pool().await;

    // Create a suspended tenant
    let tenant_id = format!("ten_{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO tenants (id, name, slug, owner_email, status) VALUES ($1, 'Suspended Co', 'suspended-co', 'admin@sus.com', 'suspended')",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "user@sus.com",
                "password": "Str0ngP@ss",
            }),
            "suspended-co",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "tenant_suspended");
}

#[tokio::test]
async fn tenant_isolation_users_cannot_cross_tenants() {
    let (app, pool) = test_app_with_pool().await;

    // Create a second tenant
    create_test_tenant_in_db(&pool, "tenant-b", "Tenant B").await;

    // Register user in tenant A
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "shared@example.com",
                "password": "Str0ngP@ss",
            }),
            TEST_TENANT_SLUG,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Same email can register in tenant B (different tenant = different namespace)
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "shared@example.com",
                "password": "Str0ngP@ss",
            }),
            "tenant-b",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Login in tenant B should not return tenant A's user
    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "shared@example.com",
                "password": "Str0ngP@ss",
            }),
            "tenant-b",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    // Both tenants have the same email but different user IDs
    assert_eq!(body["user"]["email"], "shared@example.com");
}

// ═══════════════════════════════════════════════════════════════════════════
// Hardening Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn email_normalization_register_and_login() {
    let app = test_app().await;

    // Register with mixed-case email
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "  Alice@Example.COM  ",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["user"]["email"], "alice@example.com");

    // Login with different casing should work
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "ALICE@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn brute_force_lockout() {
    let app = test_app().await;
    register_user(&app, "lockme@example.com", "Str0ngP@ss", None).await;

    // 5 wrong password attempts
    for _ in 0..5 {
        let res = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/auth/login",
                serde_json::json!({
                    "email": "lockme@example.com",
                    "password": "WrongPassword1",
                }),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    // 6th attempt should be locked (429)
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "lockme@example.com",
                "password": "WrongPassword1",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = body_json(res).await;
    assert_eq!(body["error"], "account_locked");
    assert!(body["retry_after"].is_number());

    // Even correct password should fail while locked
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "lockme@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn refresh_token_family_revocation() {
    let app = test_app().await;
    let (_, old_token, _) = register_user(&app, "family@example.com", "Str0ngP@ss", None).await;

    // Rotate token (get new one)
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": &old_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let new_token = body["refresh_token"].as_str().unwrap().to_string();

    // Replay old token → should detect reuse and revoke family
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": &old_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert_eq!(body["error"], "token_reuse_detected");

    // The new token should also be revoked (family kill)
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": &new_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn forgot_password_always_200() {
    let app = test_app().await;

    // Non-existent email — should still return 200
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/forgot-password",
            serde_json::json!({ "email": "nobody@example.com" }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Register, then forgot-password for existing user
    register_user(&app, "reset@example.com", "Str0ngP@ss", None).await;

    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/forgot-password",
            serde_json::json!({ "email": "reset@example.com" }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn password_reset_flow() {
    let (app, pool) = test_app_with_pool().await;
    register_user(&app, "pwreset@example.com", "Str0ngP@ss", None).await;

    // Trigger forgot-password
    let _ = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/forgot-password",
            serde_json::json!({ "email": "pwreset@example.com" }),
        ))
        .await
        .unwrap();

    // Read the reset token directly from DB
    let token: String =
        sqlx::query_scalar("SELECT password_reset_token FROM users WHERE email = $1")
            .bind("pwreset@example.com")
            .fetch_one(&pool)
            .await
            .unwrap();

    // Reset with new password
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/reset-password",
            serde_json::json!({
                "token": token,
                "new_password": "NewStr0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Login with new password should work
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "pwreset@example.com",
                "password": "NewStr0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Login with old password should fail
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "pwreset@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn password_reset_invalid_token_400() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/reset-password",
            serde_json::json!({
                "token": "rst_fake-token",
                "new_password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_token");
}

#[tokio::test]
async fn login_history_endpoint() {
    let app = test_app().await;
    let (access_token, _, _) = register_user(&app, "history@example.com", "Str0ngP@ss", None).await;

    // Login to generate a login attempt record
    let _ = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "history@example.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();

    let res = app
        .oneshot(auth_get("/auth/login-history", &access_token))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["history"].is_array());
    assert!(!body["history"].as_array().unwrap().is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Platform Admin API Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn platform_auth_rejects_invalid_key() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/platform/tenants")
                .header("authorization", "Bearer plat_wrong-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn platform_auth_rejects_missing_key() {
    let app = test_app().await;
    let res = app.oneshot(get_request("/platform/tenants")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn platform_create_and_list_tenants() {
    let app = test_app().await;

    // Create a tenant via platform API
    let res = app
        .clone()
        .oneshot(platform_request(
            "POST",
            "/platform/tenants",
            serde_json::json!({
                "name": "Acme Corp",
                "slug": "acme",
                "owner_email": "admin@acme.com",
                "allowed_providers": ["email", "google"],
                "max_users": 500,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["tenant"]["name"], "Acme Corp");
    assert_eq!(body["tenant"]["slug"], "acme");
    assert_eq!(body["tenant"]["status"], "active");
    assert_eq!(body["tenant"]["max_users"], 500);
    assert!(body["api_key"].as_str().unwrap().starts_with("mgmt_"));

    // List tenants
    let res = app
        .oneshot(platform_get("/platform/tenants"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let tenants = body["tenants"].as_array().unwrap();
    // Should have at least the test tenant + acme
    assert!(tenants.len() >= 2);
    assert!(tenants.iter().any(|t| t["slug"] == "acme"));
}

#[tokio::test]
async fn platform_duplicate_slug_409() {
    let app = test_app().await;

    let body = serde_json::json!({
        "name": "First",
        "slug": "unique-slug",
        "owner_email": "a@b.com",
    });

    let res = app
        .clone()
        .oneshot(platform_request("POST", "/platform/tenants", body.clone()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Same slug again
    let res = app
        .oneshot(platform_request("POST", "/platform/tenants", body))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let err = body_json(res).await;
    assert_eq!(err["error"], "slug_taken");
}

#[tokio::test]
async fn platform_get_and_update_tenant() {
    let app = test_app().await;

    // Create
    let res = app
        .clone()
        .oneshot(platform_request(
            "POST",
            "/platform/tenants",
            serde_json::json!({
                "name": "UpdateMe Corp",
                "slug": "updateme",
                "owner_email": "admin@updateme.com",
            }),
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap().to_string();

    // Get tenant
    let res = app
        .clone()
        .oneshot(platform_get(&format!("/platform/tenants/{}", tenant_id)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["name"], "UpdateMe Corp");
    assert_eq!(body["user_count"], 0);

    // Update tenant
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/platform/tenants/{}", tenant_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", TEST_PLATFORM_KEY))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "name": "Updated Corp",
                        "max_users": 2000,
                        "allowed_providers": ["email", "google", "github"],
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["name"], "Updated Corp");
    assert_eq!(body["max_users"], 2000);
}

#[tokio::test]
async fn platform_delete_tenant_soft() {
    let app = test_app().await;

    let res = app
        .clone()
        .oneshot(platform_request(
            "POST",
            "/platform/tenants",
            serde_json::json!({
                "name": "DeleteMe",
                "slug": "deleteme",
                "owner_email": "bye@example.com",
            }),
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap().to_string();

    // Delete (soft)
    let res = app
        .clone()
        .oneshot(platform_delete(&format!("/platform/tenants/{}", tenant_id)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Get should show status = deleted
    let res = app
        .oneshot(platform_get(&format!("/platform/tenants/{}", tenant_id)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["status"], "deleted");
}

#[tokio::test]
async fn platform_rotate_api_key() {
    let app = test_app().await;

    let res = app
        .clone()
        .oneshot(platform_request(
            "POST",
            "/platform/tenants",
            serde_json::json!({
                "name": "RotateKey Corp",
                "slug": "rotatekey",
                "owner_email": "admin@rotate.com",
            }),
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap().to_string();
    let old_key = body["api_key"].as_str().unwrap().to_string();

    // Rotate
    let res = app
        .oneshot(platform_request(
            "POST",
            &format!("/platform/tenants/{}/rotate-key", tenant_id),
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let new_key = body["api_key"].as_str().unwrap().to_string();
    assert_ne!(old_key, new_key);
    assert!(new_key.starts_with("mgmt_"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Management API Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn manage_auth_rejects_invalid_key() {
    let app = test_app().await;
    let res = app
        .oneshot(mgmt_get("/manage/users", "mgmt_fake-key-12345"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn manage_auth_rejects_admin_jwt() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, _mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "mgmt-jwt", "Mgmt JWT").await;

    let register = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "admin@mgmt-jwt.com",
                "password": "Str0ngP@ss123",
                "name": "Mgmt Admin"
            }),
            "mgmt-jwt",
        ))
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::CREATED);
    let register_body = body_json(register).await;
    let user_id = register_body["user"]["id"].as_str().unwrap().to_string();

    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, 'role_admin') ON CONFLICT DO NOTHING")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    let login = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "admin@mgmt-jwt.com",
                "password": "Str0ngP@ss123"
            }),
            "mgmt-jwt",
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let login_body = body_json(login).await;
    let admin_jwt = login_body["access_token"].as_str().unwrap().to_string();

    let res = app
        .oneshot(
            Request::builder()
                .uri("/manage/users")
                .header("authorization", format!("Bearer {}", admin_jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_api_key");
}

#[tokio::test]
async fn manage_list_users() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "mgmt-test", "Mgmt Test").await;

    // Register two users in the managed tenant
    let _ = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "user1@mgmt.com",
                "password": "Str0ngP@ss",
                "name": "User One"
            }),
            "mgmt-test",
        ))
        .await
        .unwrap();
    let _ = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "user2@mgmt.com",
                "password": "Str0ngP@ss",
            }),
            "mgmt-test",
        ))
        .await
        .unwrap();

    // List users via management API
    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/users", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["total"], 2);
    assert_eq!(body["users"].as_array().unwrap().len(), 2);

    // Search by email
    let res = app
        .oneshot(mgmt_get("/manage/users?email=user1", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["users"].as_array().unwrap().len(), 1);
    assert_eq!(body["users"][0]["email"], "user1@mgmt.com");
}

#[tokio::test]
async fn platform_admin_api_requires_platform_owner_role() {
    let app = test_app().await;
    let (token, _user_id) =
        register_platform_user(&app, "non-owner@platform.test", "Password123!").await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/platform/admin/stats")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "forbidden");
}

#[tokio::test]
async fn platform_admin_api_allows_platform_owner_role() {
    let (app, pool) = test_app_with_pool().await;
    let (_token, user_id) =
        register_platform_user(&app, "owner@platform.test", "Password123!").await;

    sqlx::query("INSERT INTO user_roles (user_id, role_id, tenant_id) VALUES ($1, 'role_platform_owner', 'ten_platform') ON CONFLICT DO NOTHING")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    let owner_jwt =
        mint_platform_access_token_from_refresh(&app, &pool, &user_id, "sso", true).await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/platform/admin/stats")
                .header("authorization", format!("Bearer {}", owner_jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["customers"].is_number());
}

#[tokio::test]
async fn platform_admin_api_rejects_password_session_for_owner() {
    let (app, pool) = test_app_with_pool().await;
    let (_token, user_id) =
        register_platform_user(&app, "owner-password@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .unwrap();

    let login = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "owner-password@platform.test",
                "password": "Password123!"
            }),
            "platform",
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let body = body_json(login).await;
    let owner_jwt = body["access_token"].as_str().unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .uri("/platform/admin/stats")
                .header("authorization", format!("Bearer {}", owner_jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "owner_sso_required");
}

#[tokio::test]
async fn platform_admin_api_rejects_unverified_mfa_for_owner() {
    let (app, pool) = test_app_with_pool().await;
    let (_token, user_id) =
        register_platform_user(&app, "owner-nomfa@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .unwrap();

    let owner_jwt =
        mint_platform_access_token_from_refresh(&app, &pool, &user_id, "sso", false).await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/platform/admin/stats")
                .header("authorization", format!("Bearer {}", owner_jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "owner_mfa_required");
}

#[tokio::test]
async fn platform_admin_invites_require_platform_owner_role() {
    let app = test_app().await;
    let (token, _user_id) =
        register_platform_user(&app, "customer-admin@platform.test", "Password123!").await;

    // Invite endpoint moved to /platform/admin/team/invites (team routes).
    // A regular platform user (no platform_members row) gets 403 from
    // require_platform_member before ever reaching the handler.
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/platform/admin/team/invites")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "email": "cto@example.com" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn platform_owner_can_complete_mfa_and_access_admin_api() {
    let (app, pool) = test_app_with_pool().await;
    let (_seed_token, user_id) =
        register_platform_user(&app, "owner-mfa@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .unwrap();

    let owner_jwt =
        mint_platform_access_token_from_refresh(&app, &pool, &user_id, "sso", false).await;

    let status_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/mfa/status")
                .header("authorization", format!("Bearer {}", owner_jwt))
                .header("x-tenant-slug", "platform")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status_res.status(), StatusCode::OK);
    let status_body = body_json(status_res).await;
    assert_eq!(status_body["mfa_verified"], false);

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_jwt))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let qr_data_url = setup_body["otpauth_qr_data_url"].as_str().unwrap_or("");
    assert!(qr_data_url.starts_with("data:image/svg+xml;base64,"));
    let code = totp_from_base32(secret, chrono::Utc::now());

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_jwt))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);
    let verify_body = body_json(verify_res).await;
    let mfa_owner_jwt = verify_body["access_token"].as_str().unwrap();

    let admin_res = app
        .oneshot(
            Request::builder()
                .uri("/platform/admin/stats")
                .header("authorization", format!("Bearer {}", mfa_owner_jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_res.status(), StatusCode::OK);
}

#[tokio::test]
async fn platform_owner_mfa_verify_updates_active_cookie_when_duplicates_present() {
    let (app, pool) = test_app_with_pool().await;
    let (platform_token, user_id) =
        register_platform_user(&app, "owner-mfa-cookie@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .unwrap();

    let old_cookie_refresh = seed_platform_refresh_token(&pool, &user_id, "sso", false).await;
    let active_cookie_refresh = seed_platform_refresh_token(&pool, &user_id, "sso", false).await;

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", platform_token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let code = totp_from_base32(secret, chrono::Utc::now());

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", platform_token))
                .header("x-tenant-slug", "platform")
                .header(
                    "cookie",
                    format!(
                        "gw_rt_platform={}; gw_rt_platform={}",
                        old_cookie_refresh, active_cookie_refresh
                    ),
                )
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let active_hash = signing::hash_token(&active_cookie_refresh);
    let active_verified: bool = sqlx::query_scalar(
        "SELECT mfa_verified
         FROM refresh_tokens
         WHERE token_hash = $1
         LIMIT 1",
    )
    .bind(active_hash)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(active_verified);

    let dashboard = app
        .oneshot(
            Request::builder()
                .uri("/platform/admin/dashboard")
                .header(
                    "cookie",
                    format!("gw_rt_platform={}", active_cookie_refresh),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard.status(), StatusCode::OK);
}

#[tokio::test]
async fn mfa_setup_does_not_return_secret_after_mfa_enabled() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _user_id) =
        register_platform_user(&app, "mfa-redact@platform.test", "Password123!").await;

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let code = totp_from_base32(secret, chrono::Utc::now());

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let setup_again = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_again.status(), StatusCode::OK);
    let setup_again_body = body_json(setup_again).await;
    assert!(setup_again_body["secret"].is_null());
    assert!(setup_again_body["otpauth_uri"].is_null());
    assert_eq!(setup_again_body["already_configured"], true);
}

#[tokio::test]
async fn mfa_status_and_setup_recover_from_unreadable_secret() {
    let (app, pool) = test_app_with_pool().await;
    let (token, user_id) =
        register_platform_user(&app, "mfa-recover@platform.test", "Password123!").await;

    sqlx::query(
        "UPDATE users
         SET mfa_enabled = TRUE,
             mfa_secret = $1,
             updated_at = NOW()
         WHERE id = $2",
    )
    .bind("enc2:invalid:invalid")
    .bind(&user_id)
    .execute(&pool)
    .await
    .unwrap();

    let status_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/mfa/status")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status_res.status(), StatusCode::OK);
    let status_body = body_json(status_res).await;
    assert_eq!(status_body["mfa_enabled"], false);
    assert_eq!(status_body["recovery_required"], true);

    let setup_res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    assert!(setup_body["secret"].as_str().unwrap_or("").len() >= 16);
    assert_eq!(setup_body["recovered"], true);
}

#[tokio::test]
async fn mfa_rotate_requires_mfa_verified_session() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _user_id) =
        register_platform_user(&app, "mfa-rotate@platform.test", "Password123!").await;

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let code = totp_from_base32(secret, chrono::Utc::now());

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let rotate_res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": true })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate_res.status(), StatusCode::FORBIDDEN);
    let body = body_json(rotate_res).await;
    assert_eq!(body["error"], "mfa_reauth_required");
}

#[tokio::test]
async fn mfa_rotate_allows_owner_sso_recovery_for_non_mfa_session() {
    let (app, pool) = test_app_with_pool().await;
    let (_seed_token, user_id) =
        register_platform_user(&app, "mfa-recover-owner-sso@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .unwrap();

    let owner_sso_token =
        mint_platform_access_token_from_refresh(&app, &pool, &user_id, "sso", false).await;

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_sso_token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let code = totp_from_base32(secret, chrono::Utc::now());

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_sso_token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let rotate_res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_sso_token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "rotate": true,
                        "recovery_via_sso": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate_res.status(), StatusCode::OK);
    let rotate_body = body_json(rotate_res).await;
    assert_eq!(rotate_body["rotated"], true);
    assert!(rotate_body["secret"].as_str().unwrap_or("").len() >= 16);
}

#[tokio::test]
async fn mfa_rotate_sso_recovery_rejects_non_owner_sessions() {
    let (app, pool) = test_app_with_pool().await;
    let (_seed_token, user_id) = register_platform_user(
        &app,
        "mfa-recover-non-owner-sso@platform.test",
        "Password123!",
    )
    .await;

    let sso_token =
        mint_platform_access_token_from_refresh(&app, &pool, &user_id, "sso", false).await;

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", sso_token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let code = totp_from_base32(secret, chrono::Utc::now());

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", sso_token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let rotate_res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", sso_token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "rotate": true,
                        "recovery_via_sso": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate_res.status(), StatusCode::FORBIDDEN);
    let rotate_body = body_json(rotate_res).await;
    assert_eq!(rotate_body["error"], "mfa_sso_recovery_not_allowed");
}

#[tokio::test]
async fn mfa_rotate_allows_password_recovery_for_non_mfa_session() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _user_id) =
        register_platform_user(&app, "mfa-recover-password@platform.test", "Password123!").await;

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let code = totp_from_base32(secret, chrono::Utc::now());

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let rotate_recovery = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "rotate": true,
                        "recovery_password": "Password123!"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate_recovery.status(), StatusCode::OK);
    let rotate_body = body_json(rotate_recovery).await;
    assert_eq!(rotate_body["rotated"], true);
    assert!(rotate_body["secret"].as_str().unwrap_or("").len() >= 16);
}

#[tokio::test]
async fn mfa_rotate_recovery_rejects_invalid_password() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _user_id) = register_platform_user(
        &app,
        "mfa-recover-password-bad@platform.test",
        "Password123!",
    )
    .await;

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let code = totp_from_base32(secret, chrono::Utc::now());

    let verify_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_res.status(), StatusCode::OK);

    let rotate_recovery = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "rotate": true,
                        "recovery_password": "wrong-password"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate_recovery.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(rotate_recovery).await;
    assert_eq!(body["error"], "invalid_credentials");
}

#[tokio::test]
async fn mfa_verify_locks_after_repeated_invalid_codes() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _user_id) =
        register_platform_user(&app, "mfa-lock@platform.test", "Password123!").await;

    let setup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/setup")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "rotate": false })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_res.status(), StatusCode::OK);
    let setup_body = body_json(setup_res).await;
    let secret = setup_body["secret"].as_str().unwrap();
    let valid_code = totp_from_base32(secret, chrono::Utc::now());

    for _ in 0..4 {
        let bad_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/mfa/verify")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", token))
                    .header("x-tenant-slug", "platform")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({ "code": "000000" })).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_res.status(), StatusCode::UNAUTHORIZED);
        let body = body_json(bad_res).await;
        assert_eq!(body["error"], "invalid_mfa_code");
    }

    let lock_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": "000000" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(lock_res.status(), StatusCode::TOO_MANY_REQUESTS);
    let lock_body = body_json(lock_res).await;
    assert_eq!(lock_body["error"], "mfa_locked");
    assert!(lock_body["retry_after"].as_u64().unwrap_or(0) > 0);

    let still_locked = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/mfa/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .header("x-tenant-slug", "platform")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "code": valid_code })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(still_locked.status(), StatusCode::TOO_MANY_REQUESTS);
    let still_locked_body = body_json(still_locked).await;
    assert_eq!(still_locked_body["error"], "mfa_locked");
}

/// Rewritten for the RBAC rework: platform team invites now live under
/// `/platform/admin/team/invites` and grant per-tenant team roles (not
/// `role_platform_owner`).  The invite lifecycle is:
///   1. Account owner creates a tenant (also creates their `platform_members` row).
///   2. Owner creates a team invite with a per-tenant role + tenant_ids list.
///   3. Invitee accepts the invite via `/platform/admin/invites/accept`.
///   4. Invitee is now a `platform_member_tenants` member of the owner's tenant.
#[tokio::test]
async fn platform_owner_invite_lifecycle_grants_owner_role() {
    let (app, pool) = test_app_with_pool().await;
    let (owner_token, owner_user_id) =
        register_platform_user(&app, "owner-inviter@platform.test", "Password123!").await;
    let (invitee_token, invitee_user_id) =
        register_platform_user(&app, "cto-invitee@platform.test", "Password123!").await;

    // Owner creates a tenant via self-serve.  This also auto-creates the
    // self-referencing `platform_members` row required by team routes.
    let create_tenant_res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Invite Test Co", "slug": "invite-test-co" }),
            &owner_token,
        ))
        .await
        .unwrap();
    assert_eq!(create_tenant_res.status(), StatusCode::CREATED);
    let tenant_id = body_json(create_tenant_res).await["tenant"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Owner creates a team invite: `admin` role, scoped to the new tenant.
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/platform/admin/team/invites")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "email": "cto-invitee@platform.test",
                        "role": "admin",
                        "tenant_ids": [tenant_id],
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let create_body = body_json(create).await;
    let invite_token = create_body["invite_token"].as_str().unwrap().to_string();

    // Invitee accepts the invite.
    let accept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/platform/admin/invites/accept")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", invitee_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "token": invite_token })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accept.status(), StatusCode::OK);
    let accept_body = body_json(accept).await;
    assert_eq!(accept_body["accepted"], true);

    // Verify the invitee now has a platform_member_tenants row with `admin` role
    // for the owner's tenant.
    let member_tenant_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM platform_member_tenants pmt \
         JOIN platform_members pm ON pm.id = pmt.member_id \
         WHERE pm.user_id = $1 \
           AND pm.account_owner_user_id = $2 \
           AND pmt.tenant_id = $3 \
           AND pmt.role = 'admin'",
    )
    .bind(&invitee_user_id)
    .bind(&owner_user_id)
    .bind(&tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(member_tenant_count, 1, "Invitee should have admin role on the owner's tenant");
}

#[tokio::test]
async fn platform_owner_can_manage_customer_lifecycle_and_tenant_access() {
    let (app, pool) = test_app_with_pool().await;
    let (_seed_token, owner_user_id) =
        register_platform_user(&app, "owner-lifecycle@platform.test", "Password123!").await;
    let (customer_token, customer_user_id) =
        register_platform_user(&app, "customer-lifecycle@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&owner_user_id)
    .execute(&pool)
    .await
    .unwrap();

    let owner_token =
        mint_platform_access_token_from_refresh(&app, &pool, &owner_user_id, "sso", true).await;

    let slug = format!(
        "cust-life-{}",
        &uuid::Uuid::new_v4().to_string().replace('-', "")[..8]
    );
    let create_tenant = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/platform/self-serve/tenants")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", customer_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "name": "Customer Lifecycle App",
                        "slug": slug
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_tenant.status(), StatusCode::CREATED);

    let tenants_inventory = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/platform/admin/tenants?limit=1&offset=0")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tenants_inventory.status(), StatusCode::OK);
    let tenants_inventory_body = body_json(tenants_inventory).await;
    let has_customer_slug = tenants_inventory_body["tenants"]
        .as_array()
        .map(|rows| rows.iter().any(|row| row["slug"] == slug))
        .unwrap_or(false);
    assert!(has_customer_slug);
    assert_eq!(tenants_inventory_body["limit"], 1);
    assert_eq!(tenants_inventory_body["offset"], 0);
    assert!(tenants_inventory_body["total"].as_i64().unwrap_or(0) >= 1);

    let suspend = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!(
                    "/platform/admin/customers/{}/lifecycle",
                    customer_user_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "status": "suspended",
                        "category": "education",
                        "lifecycle_stage": "at_risk",
                        "apply_to_tenants": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(suspend.status(), StatusCode::OK);
    let suspend_body = body_json(suspend).await;
    assert_eq!(suspend_body["customer"]["status"], "suspended");
    assert_eq!(suspend_body["customer"]["category"], "education");

    let tenants = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/platform/admin/customers/{}/tenants",
                    customer_user_id
                ))
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tenants.status(), StatusCode::OK);
    let tenants_body = body_json(tenants).await;
    let tenant_id = tenants_body["tenants"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(tenants_body["tenants"][0]["status"], "suspended");

    let extend = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/platform/admin/customers/{}/extend-access",
                    customer_user_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "days": 21,
                        "tenant_ids": [tenant_id]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(extend.status(), StatusCode::OK);
    let extend_body = body_json(extend).await;
    assert!(extend_body["affected_tenants"].as_u64().unwrap_or(0) >= 1);

    let activate = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/platform/admin/customers/{}/tenants/{}/status",
                    customer_user_id, tenant_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "status": "active" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(activate.status(), StatusCode::OK);

    let delete_mismatch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/platform/admin/customers/{}/tenants/{}",
                    customer_user_id, tenant_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "confirm_slug": "wrong-slug"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_mismatch.status(), StatusCode::BAD_REQUEST);
    let delete_mismatch_body = body_json(delete_mismatch).await;
    assert_eq!(delete_mismatch_body["error"], "confirmation_mismatch");

    let wrong_customer_id = format!("usr_{}", uuid::Uuid::new_v4());
    let delete_wrong_customer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/platform/admin/customers/{}/tenants/{}",
                    wrong_customer_id, tenant_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "confirm_slug": slug
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_wrong_customer.status(), StatusCode::NOT_FOUND);

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/platform/admin/customers/{}/tenants/{}",
                    customer_user_id, tenant_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "confirm_slug": slug
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::OK);
    let delete_body = body_json(delete).await;
    assert_eq!(delete_body["status"], "deleted");

    let tenants_after_delete = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/platform/admin/customers/{}/tenants",
                    customer_user_id
                ))
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tenants_after_delete.status(), StatusCode::OK);
    let tenants_after_delete_body = body_json(tenants_after_delete).await;
    assert!(tenants_after_delete_body["tenants"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .all(|tenant| tenant["id"].as_str().unwrap_or_default() != tenant_id)
        })
        .unwrap_or(true));

    let tenant_status: String = sqlx::query_scalar("SELECT status FROM tenants WHERE id = $1")
        .bind(&tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tenant_status, "deleted");
}

#[tokio::test]
async fn platform_owner_can_record_and_list_payment_events() {
    let (app, pool) = test_app_with_pool().await;
    let (_seed_token, owner_user_id) =
        register_platform_user(&app, "owner-payments@platform.test", "Password123!").await;
    let (_customer_token, customer_user_id) =
        register_platform_user(&app, "customer-payments@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&owner_user_id)
    .execute(&pool)
    .await
    .unwrap();

    let owner_token =
        mint_platform_access_token_from_refresh(&app, &pool, &owner_user_id, "sso", true).await;

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/platform/admin/payments")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "customer_user_id": customer_user_id,
                        "amount_cents": 129900,
                        "currency": "usd",
                        "status": "paid",
                        "external_invoice_id": "inv_test_001",
                        "receipt_url": "https://example.com/receipt/1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let create_body = body_json(create).await;
    assert_eq!(create_body["payment"]["status"], "paid");

    // Regression guard: production may have NUMERIC for amount_cents due legacy schema drift.
    // Owner endpoints should still decode values safely.
    sqlx::query(
        "ALTER TABLE provider_billing_events
         ALTER COLUMN amount_cents TYPE NUMERIC USING amount_cents::NUMERIC",
    )
    .execute(&pool)
    .await
    .unwrap();

    let customers = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/platform/admin/customers?limit=150&offset=0&sort=created_desc")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customers.status(), StatusCode::OK);
    let customers_body = body_json(customers).await;
    let customer_exists = customers_body["customers"]
        .as_array()
        .map(|rows| rows.iter().any(|row| row["user_id"] == customer_user_id))
        .unwrap_or(false);
    assert!(customer_exists);

    let list = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/platform/admin/payments?customer_user_id={}",
                    customer_user_id
                ))
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = body_json(list).await;
    let payments = list_body["payments"].as_array().unwrap();
    assert!(!payments.is_empty());
    let max_amount = payments
        .iter()
        .filter_map(|p| p["amount_cents"].as_i64())
        .max()
        .unwrap_or(0);
    assert!(max_amount >= 129900);
}

#[tokio::test]
async fn platform_owner_customer_360_and_timeline_endpoints_return_audit_data() {
    let (app, pool) = test_app_with_pool().await;
    let (_seed_token, owner_user_id) =
        register_platform_user(&app, "owner-timeline@platform.test", "Password123!").await;
    let (customer_token, customer_user_id) =
        register_platform_user(&app, "customer-timeline@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&owner_user_id)
    .execute(&pool)
    .await
    .unwrap();

    let owner_token =
        mint_platform_access_token_from_refresh(&app, &pool, &owner_user_id, "sso", true).await;

    let slug = format!(
        "cust-timeline-{}",
        &uuid::Uuid::new_v4().to_string().replace('-', "")[..8]
    );
    let create_tenant = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/platform/self-serve/tenants")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", customer_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "name": "Customer Timeline App",
                        "slug": slug
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_tenant.status(), StatusCode::CREATED);

    let lifecycle_update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!(
                    "/platform/admin/customers/{}/lifecycle",
                    customer_user_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "status": "active",
                        "category": "agency",
                        "lifecycle_stage": "active",
                        "apply_to_tenants": false
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(lifecycle_update.status(), StatusCode::OK);

    let insights = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/platform/admin/customers/{}/insights",
                    customer_user_id
                ))
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(insights.status(), StatusCode::OK);
    let insights_body = body_json(insights).await;
    assert_eq!(insights_body["customer"]["user_id"], customer_user_id);
    assert!(insights_body["lifecycle_history"]
        .as_array()
        .map(|rows| !rows.is_empty())
        .unwrap_or(false));

    let timeline = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/platform/admin/timeline?limit=1&offset=0&actor={}",
                    owner_user_id
                ))
                .header("authorization", format!("Bearer {}", owner_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(timeline.status(), StatusCode::OK);
    let timeline_body = body_json(timeline).await;
    assert_eq!(timeline_body["limit"], 1);
    assert_eq!(timeline_body["offset"], 0);
    assert!(timeline_body["total"].as_i64().unwrap_or(0) >= 1);
    assert!(timeline_body["events"]
        .as_array()
        .map(|rows| !rows.is_empty())
        .unwrap_or(false));
}

#[tokio::test]
async fn manage_get_user_detail() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "detail-test", "Detail Test").await;

    // Register a user
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "detail@test.com",
                "password": "Str0ngP@ss",
                "name": "Detail User"
            }),
            "detail-test",
        ))
        .await
        .unwrap();
    let reg_body = body_json(res).await;
    let user_id = reg_body["user"]["id"].as_str().unwrap();

    // Get user via management API
    let res = app
        .oneshot(mgmt_get(&format!("/manage/users/{}", user_id), &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["email"], "detail@test.com");
    assert_eq!(body["name"], "Detail User");
    assert!(body["roles"].is_array());
    assert!(body["providers"].is_array());
    assert!(body["active_sessions"].is_number());
}

#[tokio::test]
async fn manage_block_and_unblock_user() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "block-test", "Block Test").await;

    // Register
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "blockme@test.com",
                "password": "Str0ngP@ss",
            }),
            "block-test",
        ))
        .await
        .unwrap();
    let reg_body = body_json(res).await;
    let user_id = reg_body["user"]["id"].as_str().unwrap().to_string();

    // Block user
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/users/{}/block", user_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["message"], "User has been blocked");

    // Login should now fail (account locked)
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "blockme@test.com",
                "password": "Str0ngP@ss",
            }),
            "block-test",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);

    // Unblock user
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/users/{}/unblock", user_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Login should work again
    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "blockme@test.com",
                "password": "Str0ngP@ss",
            }),
            "block-test",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn manage_delete_user() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "del-test", "Del Test").await;

    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "deletable@test.com",
                "password": "Str0ngP@ss",
            }),
            "del-test",
        ))
        .await
        .unwrap();
    let reg_body = body_json(res).await;
    let user_id = reg_body["user"]["id"].as_str().unwrap();

    // Delete via management API
    let res = app
        .clone()
        .oneshot(mgmt_delete(
            &format!("/manage/users/{}", user_id),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Login should fail
    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "deletable@test.com",
                "password": "Str0ngP@ss",
            }),
            "del-test",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn manage_revoke_user_sessions() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "revoke-test", "Revoke Test").await;

    // Register
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "revoke@test.com",
                "password": "Str0ngP@ss",
            }),
            "revoke-test",
        ))
        .await
        .unwrap();
    let reg_body = body_json(res).await;
    let user_id = reg_body["user"]["id"].as_str().unwrap().to_string();
    let refresh_token = reg_body["refresh_token"].as_str().unwrap().to_string();

    // Revoke all sessions
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/users/{}/revoke-sessions", user_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["revoked_count"].as_u64().unwrap() >= 1);

    // Refresh token should be invalid now
    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }),
            "revoke-test",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn manage_tenant_stats() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "stats-test", "Stats Test").await;

    // Register a user
    let _ = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "stat-user@test.com",
                "password": "Str0ngP@ss",
            }),
            "stats-test",
        ))
        .await
        .unwrap();

    // Get stats
    let res = app
        .oneshot(mgmt_get("/manage/stats", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["total_users"], 1);
    assert!(body["active_sessions"].is_number());
    assert!(body["providers"].is_object());
}

#[tokio::test]
async fn manage_audit_log() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "audit-test", "Audit Test").await;

    // Register and login to generate audit log entries
    let _ = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "audit@test.com",
                "password": "Str0ngP@ss",
            }),
            "audit-test",
        ))
        .await
        .unwrap();

    let _ = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "audit@test.com",
                "password": "Str0ngP@ss",
            }),
            "audit-test",
        ))
        .await
        .unwrap();

    // Failed login attempt
    let _ = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "audit@test.com",
                "password": "WrongPass1!",
            }),
            "audit-test",
        ))
        .await
        .unwrap();

    // Get audit log
    let res = app
        .oneshot(mgmt_get("/manage/audit-log", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let log = body["audit_log"].as_array().unwrap();
    assert!(log.len() >= 2); // at least the login success + failure
                             // Most recent should be the failed attempt
    assert!(!log[0]["success"].as_bool().unwrap());
}

#[tokio::test]
async fn manage_get_and_update_config() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "cfg-test", "Config Test").await;

    // Get config
    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/config", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["slug"], "cfg-test");
    assert_eq!(body["google_configured"], false);

    // Update config
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/manage/config")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", mgmt_key))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "allowed_providers": ["email", "google"],
                        "allowed_origins": ["https://app.example.com"],
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify config was updated (need fresh lookup — cache may still have old value)
    // Clear cache by waiting or checking DB directly
    let providers: serde_json::Value =
        sqlx::query_scalar("SELECT allowed_providers FROM tenants WHERE slug = 'cfg-test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let providers_arr: Vec<String> = serde_json::from_value(providers).unwrap();
    assert!(providers_arr.contains(&"google".to_string()));

    let origins: serde_json::Value =
        sqlx::query_scalar("SELECT allowed_origins FROM tenants WHERE slug = 'cfg-test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let origins_arr: Vec<String> = serde_json::from_value(origins).unwrap();
    assert!(origins_arr.contains(&"https://app.example.com".to_string()));
}

#[tokio::test]
async fn manage_user_not_found_in_other_tenant() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key_a) = create_test_tenant_with_mgmt_key(&pool, "iso-a", "Iso A").await;
    let (_, mgmt_key_b) = create_test_tenant_with_mgmt_key(&pool, "iso-b", "Iso B").await;

    // Register user in tenant A
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "private@test.com",
                "password": "Str0ngP@ss",
            }),
            "iso-a",
        ))
        .await
        .unwrap();
    let reg_body = body_json(res).await;
    let user_id = reg_body["user"]["id"].as_str().unwrap();

    // Tenant B should NOT be able to see tenant A's user
    let res = app
        .clone()
        .oneshot(mgmt_get(&format!("/manage/users/{}", user_id), &mgmt_key_b))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Tenant A should be able to see their own user
    let res = app
        .clone()
        .oneshot(mgmt_get(&format!("/manage/users/{}", user_id), &mgmt_key_a))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// ═══════════════════════════════════════════════════════════════════════════
// End-to-end Multi-Tenant Flow
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn full_tenant_lifecycle() {
    let app = test_app().await;

    // 1. Platform admin creates a tenant
    let res = app
        .clone()
        .oneshot(platform_request(
            "POST",
            "/platform/tenants",
            serde_json::json!({
                "name": "Lifecycle Corp",
                "slug": "lifecycle",
                "owner_email": "admin@lifecycle.com",
                "allowed_providers": ["email"],
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap().to_string();
    let mgmt_key = body["api_key"].as_str().unwrap().to_string();

    // 2. User registers via tenant's auth endpoint
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "user@lifecycle.com",
                "password": "Str0ngP@ss",
                "name": "Lifecycle User"
            }),
            "lifecycle",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 3. Management API can see the user
    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/users", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["users"][0]["email"], "user@lifecycle.com");

    // 4. Check stats
    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/stats", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["total_users"], 1);

    // 5. Platform admin can see tenant details with user count
    let res = app
        .oneshot(platform_get(&format!("/platform/tenants/{}", tenant_id)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["user_count"], 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// Compliance Engine Tests
// ═══════════════════════════════════════════════════════════════════════════

use gatewasm_server::compliance::{self, ComplianceMode};

/// Helper: create a HIPAA compliance config with audit_log_enabled=true
fn hipaa_compliance_config() -> ComplianceConfig {
    let mut config = ComplianceConfig {
        mode: ComplianceMode::HIPAA,
        ..Default::default()
    };
    compliance::apply_compliance_overrides(&mut config);
    config
}

/// Helper: create a standard compliance config with audit_log_enabled=true
fn standard_audit_config() -> ComplianceConfig {
    ComplianceConfig {
        audit_log_enabled: true,
        ..Default::default()
    }
}

// ── Startup Validator Tests ─────────────────────────────────────────

#[tokio::test]
async fn compliance_hipaa_config_overrides() {
    let config = hipaa_compliance_config();
    assert_eq!(config.session_timeout_minutes, 15);
    assert!(config.mfa_required);
    assert!(config.audit_log_enabled);
    assert_ne!(config.jwt_algorithm, "HS256");
    assert!(config.max_failed_logins <= 5);
}

#[tokio::test]
async fn compliance_hipaa_validation_passes_with_correct_config() {
    let config = hipaa_compliance_config();
    let spec = compliance::load_spec(&ComplianceMode::HIPAA).unwrap();
    let results = compliance::validator::validate_compliance(&config, &spec);
    let required_failures: Vec<_> = results
        .iter()
        .filter(|r| r.required && r.status == compliance::validator::ControlStatus::Fail)
        .collect();
    assert!(
        required_failures.is_empty(),
        "Required controls failing: {:?}",
        required_failures
            .iter()
            .map(|r| format!("{}: {:?}", r.control_id, r.failures))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn compliance_hs256_fails_hipaa_validation() {
    let mut config = hipaa_compliance_config();
    config.jwt_algorithm = "HS256".to_string();
    let spec = compliance::load_spec(&ComplianceMode::HIPAA).unwrap();
    let results = compliance::validator::validate_compliance(&config, &spec);
    let ac5 = results
        .iter()
        .find(|r| r.control_id == "HIPAA-AC-5")
        .unwrap();
    assert_eq!(ac5.status, compliance::validator::ControlStatus::Fail);
}

#[tokio::test]
async fn compliance_standard_mode_no_spec() {
    let spec = compliance::load_spec(&ComplianceMode::Standard);
    assert!(spec.is_none());
}

// ── HIPAA-AC-1: Access Control ──────────────────────────────────────

#[tokio::test]
async fn hipaa_ac1_unauthenticated_request_denied() {
    let app = test_app().await;
    let res = app
        .oneshot(get_request_with_tenant("/auth/me"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn hipaa_ac1_correct_role_allowed() {
    let app = test_app().await;
    let (token, _, _) = register_user(&app, "role@test.com", "Str0ngP@ss", None).await;
    let res = app
        .clone()
        .oneshot(auth_get("/auth/me", &token))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(!body["roles"].as_array().unwrap().is_empty());
}

// ── HIPAA-AC-2: Unique User Identification ──────────────────────────

#[tokio::test]
async fn hipaa_ac2_users_have_unique_ids() {
    let app = test_app().await;
    let (_, _, id1) = register_user(&app, "unique1@test.com", "Str0ngP@ss", None).await;
    let (_, _, id2) = register_user(&app, "unique2@test.com", "Str0ngP@ss", None).await;
    let (_, _, id3) = register_user(&app, "unique3@test.com", "Str0ngP@ss", None).await;
    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
    assert!(id1.starts_with("usr_"));
    assert!(id2.starts_with("usr_"));
}

#[tokio::test]
async fn hipaa_ac2_audit_entries_include_user_id() {
    let (app, pool) = test_app_with_compliance(standard_audit_config()).await;

    let (_, _, user_id) = register_user(&app, "audit@test.com", "Str0ngP@ss", None).await;

    // Check audit_log for user.register event with user_id
    let row = sqlx::query_as::<_, (String, Option<String>, String)>(
        "SELECT action, user_id, result FROM audit_log WHERE action = 'user.register' ORDER BY timestamp DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(row.is_some(), "No audit log entry found for user.register");
    let (action, audit_user_id, result) = row.unwrap();
    assert_eq!(action, "user.register");
    assert_eq!(audit_user_id.as_deref(), Some(user_id.as_str()));
    assert_eq!(result, "success");
}

// ── HIPAA-AC-3: Emergency Access ────────────────────────────────────

#[tokio::test]
async fn hipaa_ac3_emergency_access_endpoint_exists() {
    let (app, pool) = test_app_with_pool().await;

    let slug = format!("emergency-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, &slug, "Emergency Test").await;

    // Register a user in this tenant
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "emergency@test.com",
                "password": "Str0ngP@ss",
            }),
            &slug,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Management API can access user data (emergency access)
    let res = app
        .oneshot(mgmt_get("/manage/users", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["total"], 1);
}

// ── HIPAA-AC-4: Automatic Logoff ────────────────────────────────────

#[tokio::test]
async fn hipaa_ac4_timeout_cannot_exceed_15_minutes() {
    let mut config = ComplianceConfig {
        mode: ComplianceMode::HIPAA,
        session_timeout_minutes: 60,
        ..Default::default()
    };
    compliance::apply_compliance_overrides(&mut config);
    assert!(
        config.session_timeout_minutes <= 15,
        "HIPAA mode must enforce session timeout <= 15 minutes, got {}",
        config.session_timeout_minutes
    );
}

// ── HIPAA-AC-5: Encryption ──────────────────────────────────────────

#[tokio::test]
async fn hipaa_ac5_jwt_not_hs256() {
    let app = test_app().await;
    let (token, _, _) = register_user(&app, "crypto@test.com", "Str0ngP@ss", None).await;

    // Decode JWT header (base64url decode the first segment)
    let parts: Vec<&str> = token.split('.').collect();
    assert!(parts.len() >= 2);
    let header_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0])
        .unwrap();
    let header: serde_json::Value = serde_json::from_slice(&header_bytes).unwrap();
    let alg = header["alg"].as_str().unwrap();
    assert_ne!(alg, "HS256", "JWT must not use HS256");
    assert_eq!(alg, "EdDSA", "JWT should use EdDSA");
}

#[tokio::test]
async fn hipaa_ac5_passwords_hashed_argon2id() {
    let (app, pool) = test_app_with_pool().await;
    register_user(&app, "hash@test.com", "Str0ngP@ss", None).await;

    let hash = sqlx::query_scalar::<_, Option<String>>(
        "SELECT hashed_password FROM credentials WHERE provider = 'email' LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(hash.is_some());
    assert!(
        hash.unwrap().starts_with("$argon2id$"),
        "Password must be hashed with Argon2id"
    );
}

// ── HIPAA-AUDIT: Audit Controls ─────────────────────────────────────

#[tokio::test]
async fn hipaa_audit_all_auth_events_logged() {
    let (app, pool) = test_app_with_compliance(standard_audit_config()).await;

    // Register
    let (token, refresh, _) = register_user(&app, "audit-all@test.com", "Str0ngP@ss", None).await;

    // Login
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "audit-all@test.com",
                "password": "Str0ngP@ss",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let login_body = body_json(res).await;
    let login_refresh = login_body["refresh_token"].as_str().unwrap().to_string();

    // Token refresh
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": login_refresh,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Logout
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "refresh_token": refresh })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Verify audit log has entries for all events
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        count >= 4,
        "Expected at least 4 audit events (register, login, refresh, logout), got {}",
        count
    );

    // Verify specific actions exist
    let actions: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT action FROM audit_log ORDER BY action")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(actions.contains(&"user.register".to_string()));
    assert!(actions.contains(&"user.login".to_string()));
    assert!(actions.contains(&"user.token_refresh".to_string()));
    assert!(actions.contains(&"user.logout".to_string()));
}

#[tokio::test]
async fn hipaa_audit_entries_have_required_fields() {
    let (app, pool) = test_app_with_compliance(standard_audit_config()).await;
    register_user(&app, "fields@test.com", "Str0ngP@ss", None).await;

    let row = sqlx::query_as::<_, (String, chrono::DateTime<chrono::Utc>, String, Option<String>, String, String, String, String)>(
        "SELECT event_id, timestamp, tenant_id, user_id, action, resource, ip_address, result FROM audit_log LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (event_id, _timestamp, tenant_id, user_id, action, resource, ip_address, result) = row;
    assert!(event_id.starts_with("evt_"));
    assert!(!tenant_id.is_empty());
    assert!(user_id.is_some());
    assert!(!action.is_empty());
    assert!(!resource.is_empty());
    assert!(!ip_address.is_empty());
    assert!(!result.is_empty());
}

#[tokio::test]
async fn hipaa_audit_login_failure_logged() {
    let (app, pool) = test_app_with_compliance(standard_audit_config()).await;
    register_user(&app, "fail@test.com", "Str0ngP@ss", None).await;

    // Attempt login with wrong password
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "fail@test.com",
                "password": "WrongPassword1",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Check audit log for failure
    let row = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT action, result, failure_reason FROM audit_log WHERE action = 'user.login_failed' LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(row.is_some(), "Login failure not logged to audit_log");
    let (action, result, failure_reason) = row.unwrap();
    assert_eq!(action, "user.login_failed");
    assert_eq!(result, "failure");
    assert_eq!(failure_reason.as_deref(), Some("wrong_password"));
}

// ── HIPAA-INT: Integrity ────────────────────────────────────────────

#[tokio::test]
async fn hipaa_int_tampered_jwt_rejected() {
    let app = test_app().await;
    let (token, _, _) = register_user(&app, "tamper@test.com", "Str0ngP@ss", None).await;

    // Tamper with the JWT payload (change a character in the middle)
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let mut tampered_payload = parts[1].to_string();
    // Flip a character to corrupt the payload
    let bytes = unsafe { tampered_payload.as_bytes_mut() };
    if !bytes.is_empty() {
        bytes[0] = if bytes[0] == b'a' { b'b' } else { b'a' };
    }
    let tampered_token = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

    let res = app
        .oneshot(auth_get("/auth/me", &tampered_token))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn hipaa_int_refresh_token_rotated() {
    let app = test_app().await;
    let (_, refresh1, _) = register_user(&app, "rotate@test.com", "Str0ngP@ss", None).await;

    // Use refresh token to get a new one
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh1,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let refresh2 = body["refresh_token"].as_str().unwrap();

    // Old and new tokens must be different (rotation)
    assert_ne!(refresh1, refresh2);
}

#[tokio::test]
async fn hipaa_int_replay_detected_family_revoked() {
    let app = test_app().await;
    let (_, refresh1, _) = register_user(&app, "replay@test.com", "Str0ngP@ss", None).await;

    // Use refresh token (it gets revoked)
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh1.clone(),
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Attempt to reuse the old (revoked) refresh token
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh1,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert_eq!(body["error"], "token_reuse_detected");
}

// ── HIPAA-AUTH: Person or Entity Authentication ─────────────────────

#[tokio::test]
async fn hipaa_auth_weak_password_rejected() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "weak@test.com",
                "password": "abc",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "weak_password");
}

#[tokio::test]
async fn hipaa_auth_identity_verified_before_access() {
    let app = test_app().await;

    // No auth header -> 401
    let res = app
        .clone()
        .oneshot(get_request_with_tenant("/auth/me"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Invalid token -> 401
    let res = app
        .oneshot(auth_get("/auth/me", "invalid-token"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── HIPAA-TX: Transmission Security ─────────────────────────────────

#[tokio::test]
async fn hipaa_tx_secure_cookies_set() {
    let app = test_app().await;
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "cookies@test.com",
                "password": "Str0ngP@ss",
                "name": "Cookie Test"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Check Set-Cookie header has Secure and HttpOnly flags
    let cookie = res
        .headers()
        .get("set-cookie")
        .expect("Missing Set-Cookie header")
        .to_str()
        .unwrap();
    assert!(
        cookie.contains("HttpOnly"),
        "Cookie must have HttpOnly flag"
    );
    // HTTPS mode: SameSite=None + Secure (no Partitioned — breaks cross-site redirect flow)
    assert!(
        cookie.contains("SameSite=None"),
        "Cookie must have SameSite=None in HTTPS mode"
    );
    assert!(
        cookie.contains("Secure"),
        "Cookie must have Secure flag in HTTPS mode"
    );
}

// ── Compliance API Endpoint Tests ───────────────────────────────────

#[tokio::test]
async fn compliance_status_endpoint() {
    let compliance = hipaa_compliance_config();
    let (app, pool) = test_app_with_compliance(compliance).await;

    let slug = format!("comp-status-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, &slug, "Compliance Status Test").await;

    // Register a user in the tenant to have some data
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "comp@test.com",
                "password": "Str0ngP@ss",
            }),
            &slug,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Call compliance status endpoint
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/compliance/status")
                .header("authorization", format!("Bearer {}", mgmt_key))
                .header("x-tenant-slug", slug.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    assert_eq!(body["mode"], "hipaa");
    assert_eq!(body["framework"], "HIPAA");
    assert!(body["controls"].as_array().unwrap().len() >= 9);
    assert!(body["summary"]["total_controls"].as_u64().unwrap() >= 9);
}

#[tokio::test]
async fn compliance_report_endpoint_markdown() {
    let compliance = hipaa_compliance_config();
    let (app, pool) = test_app_with_compliance(compliance).await;

    let slug = format!("comp-report-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, &slug, "Compliance Report Test").await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/compliance/report")
                .header("authorization", format!("Bearer {}", mgmt_key))
                .header("x-tenant-slug", slug.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/markdown"));

    let bytes = axum::body::to_bytes(res.into_body(), 1_000_000)
        .await
        .unwrap();
    let report = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(report.contains("HIPAA Compliance Report"));
    assert!(report.contains("HIPAA-AC-1"));
    assert!(report.contains("HIPAA-AUDIT"));
}

#[tokio::test]
async fn compliance_report_endpoint_json() {
    let compliance = hipaa_compliance_config();
    let (app, pool) = test_app_with_compliance(compliance).await;

    let slug = format!("comp-json-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, &slug, "Compliance JSON Test").await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/compliance/report?format=json")
                .header("authorization", format!("Bearer {}", mgmt_key))
                .header("x-tenant-slug", slug.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["mode"], "hipaa");
    assert!(body["controls"].as_array().is_some());
}

#[tokio::test]
async fn compliance_standard_mode_returns_error() {
    // Standard mode has no compliance spec -> should return 400
    let (app, pool) = test_app_with_pool().await;

    let slug = format!("comp-std-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, &slug, "Standard Mode Test").await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/compliance/status")
                .header("authorization", format!("Bearer {}", mgmt_key))
                .header("x-tenant-slug", slug.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ── Token Replay Audit Logging ──────────────────────────────────────

#[tokio::test]
async fn compliance_token_replay_creates_audit_entry() {
    let (app, pool) = test_app_with_compliance(standard_audit_config()).await;

    let (_, refresh, _) = register_user(&app, "replay-audit@test.com", "Str0ngP@ss", None).await;

    // Use the refresh token
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh.clone(),
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Replay the old token (should fail)
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Check audit log for token revocation event
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT action, result FROM audit_log WHERE action = 'user.token_revoked' LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(row.is_some(), "Token replay should be logged to audit_log");
    let (action, result) = row.unwrap();
    assert_eq!(action, "user.token_revoked");
    assert_eq!(result, "failure");
}

// ══════════════════════════════════════════════════════════════════════════
// ██ SLUG-IN-URL ROUTING TESTS
// ══════════════════════════════════════════════════════════════════════════

/// Helper: build a JSON request using slug-in-URL (no X-Tenant-Slug header).
fn slug_json_request(
    method: &str,
    slug: &str,
    path: &str,
    body: serde_json::Value,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(format!("/t/{}{}", slug, path))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

/// Helper: build an authenticated GET using slug-in-URL (no X-Tenant-Slug header).
fn slug_auth_get(slug: &str, path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/t/{}{}", slug, path))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn test_slug_in_url_register() {
    let app = test_app().await;
    let res = app
        .oneshot(slug_json_request(
            "POST",
            TEST_TENANT_SLUG,
            "/auth/register",
            serde_json::json!({
                "email": "slug-register@test.com",
                "password": "SecurePass123!",
                "name": "Slug User",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert!(body.get("access_token").is_some());
    assert!(body.get("refresh_token").is_some());
}

#[tokio::test]
async fn test_slug_in_url_login() {
    let app = test_app().await;

    // Register first via slug URL
    let _ = app
        .clone()
        .oneshot(slug_json_request(
            "POST",
            TEST_TENANT_SLUG,
            "/auth/register",
            serde_json::json!({
                "email": "slug-login@test.com",
                "password": "SecurePass123!",
                "name": "Slug Login User",
            }),
        ))
        .await
        .unwrap();

    // Login via slug URL
    let res = app
        .oneshot(slug_json_request(
            "POST",
            TEST_TENANT_SLUG,
            "/auth/login",
            serde_json::json!({
                "email": "slug-login@test.com",
                "password": "SecurePass123!",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.get("access_token").is_some());
}

#[tokio::test]
async fn test_slug_in_url_me() {
    let app = test_app().await;

    // Register via slug URL
    let res = app
        .clone()
        .oneshot(slug_json_request(
            "POST",
            TEST_TENANT_SLUG,
            "/auth/register",
            serde_json::json!({
                "email": "slug-me@test.com",
                "password": "SecurePass123!",
                "name": "Slug Me User",
            }),
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    let token = body["access_token"].as_str().unwrap();

    // GET /auth/me via slug URL
    let res = app
        .oneshot(slug_auth_get(TEST_TENANT_SLUG, "/auth/me", token))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["email"], "slug-me@test.com");
}

#[tokio::test]
async fn test_slug_in_url_nonexistent_tenant() {
    let app = test_app().await;
    let res = app
        .oneshot(slug_json_request(
            "POST",
            "nonexistent-tenant",
            "/auth/register",
            serde_json::json!({
                "email": "nobody@test.com",
                "password": "SecurePass123!",
                "name": "Nobody",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = body_json(res).await;
    assert_eq!(body["error"], "tenant_not_found");
}

// ══════════════════════════════════════════════════════════════════════════
// ██ HOSTED LOGIN PAGE TESTS
// ══════════════════════════════════════════════════════════════════════════

/// Helper: extract response body as String.
async fn body_string(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn test_hosted_login_page_renders() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/t/{}/login", TEST_TENANT_SLUG))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/html"));
    let body = body_string(res).await;
    assert!(body.contains("<!DOCTYPE html>"), "Should return HTML page");
    assert!(body.contains("script type=\"module\""), "Should include module script");
    assert!(
        body.contains("window.__TENANT__="),
        "Should inject tenant config via window.__TENANT__"
    );
}

#[tokio::test]
async fn test_hosted_login_page_contains_tenant_config() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/t/{}/login", TEST_TENANT_SLUG))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;
    // Config should include slug and providers
    assert!(
        body.contains(TEST_TENANT_SLUG),
        "Config should include tenant slug"
    );
    assert!(
        body.contains("email"),
        "Config should include email provider"
    );
    assert!(
        body.contains("google"),
        "Config should include google provider"
    );
    assert!(body.contains("window.__TENANT__"), "Should have tenant config injection");
}

#[tokio::test]
async fn test_hosted_login_does_not_redirect_with_cross_tenant_cookie() {
    let (app, pool) = test_app_with_pool().await;
    let other_slug = "cookie-scope-tenant";
    create_test_tenant_in_db(&pool, other_slug, "Cookie Scope Tenant").await;

    let (_, refresh_token, _) =
        register_user(&app, "cookie-scope@example.com", "Str0ngP@ss", None).await;

    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/t/{}/login", other_slug))
                .header("cookie", format!("refresh_token={}", refresh_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;
    assert!(body.contains("<div id=\"app\">"));
}

#[tokio::test]
async fn test_hosted_login_nonexistent_tenant() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/t/nonexistent/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Should fail with 404 (tenant not found)
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_bridge_js_endpoint() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bridge.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert_eq!(content_type, "application/javascript");
    let cache_control = res
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(cache_control, "public, max-age=3600");
    let body = body_string(res).await;
    assert!(
        body.contains("GateAuth"),
        "Should contain bridge.js content"
    );
}

#[tokio::test]
async fn test_hosted_login_via_header() {
    // The /login route should also work via X-Tenant-Slug header (backward compat)
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/login")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;
    assert!(body.contains("<!DOCTYPE html>"));
}

// ── Admin Create User tests ──────────────────────────────────────────────

#[tokio::test]
async fn admin_create_user_success() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "admin-create", "Admin Create").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({
                "email": "new@test.com",
                "password": "Str0ngP@ss1",
                "name": "New User"
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["user"]["email"], "new@test.com");
    assert_eq!(body["user"]["name"], "New User");
    assert_eq!(body["user"]["email_verified"], false);
    assert_eq!(body["user"]["roles"][0], "user");
    assert!(body["user"]["id"].as_str().unwrap().starts_with("usr_"));
    // No tokens in response
    assert!(body.get("access_token").is_none());
}

#[tokio::test]
async fn admin_create_user_with_admin_role() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "admin-role-create", "Admin Role").await;

    let res = app
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({
                "email": "admin@test.com",
                "password": "Str0ngP@ss1",
                "role": "admin"
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["user"]["roles"][0], "admin");
}

#[tokio::test]
async fn admin_create_user_duplicate_409() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "admin-dup", "Admin Dup").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({"email": "dup@test.com", "password": "Str0ngP@ss1"}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let res = app
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({"email": "dup@test.com", "password": "Str0ngP@ss1"}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"], "user_exists");
}

#[tokio::test]
async fn admin_create_user_weak_password_400() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "admin-weak", "Admin Weak").await;

    let res = app
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({"email": "weak@test.com", "password": "short"}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "weak_password");
}

#[tokio::test]
async fn admin_create_user_invalid_role_400() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "admin-badrole", "Admin BadRole").await;

    let res = app
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({"email": "r@test.com", "password": "Str0ngP@ss1", "role": "superadmin"}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_role");
}

#[tokio::test]
async fn admin_create_user_can_then_login() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "admin-login-after", "Login After").await;

    // Admin creates user
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({"email": "invited@test.com", "password": "Str0ngP@ss1", "name": "Invited"}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // User can login with those credentials
    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({"email": "invited@test.com", "password": "Str0ngP@ss1"}),
            "admin-login-after",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["access_token"].is_string());
    assert_eq!(body["user"]["email"], "invited@test.com");
}

// ── Docs endpoint tests ─────────────────────────────────────────────────

#[tokio::test]
async fn docs_page_returns_swagger_ui() {
    let app = test_app().await;
    let res = app
        .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;
    assert!(body.contains("swagger-ui"));
    assert!(body.contains("fAuth"));
    assert!(body.contains("url: \"/openapi.json\"") || body.contains("url: '/openapi.json'"));
    assert!(!body.contains("/platform/tenants"));
    assert!(!body.contains("/api/v1/compliance/status"));
}

#[tokio::test]
async fn openapi_json_returns_spec() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["openapi"], "3.0.3");
    assert!(body["paths"].is_object());
    assert!(body["paths"]["/platform/tenants"].is_null());
    assert!(body["paths"]["/api/v1/compliance/status"].is_null());
    assert!(body["paths"]["/platform/self-serve/tenants"].is_object());
    assert!(body["paths"]["/manage/organizations"].is_object());
}

#[tokio::test]
async fn internal_openapi_requires_platform_key() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/internal/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn internal_openapi_with_platform_key_returns_full_spec() {
    let app = test_app().await;
    let res = app
        .oneshot(platform_get("/internal/openapi.json"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["paths"]["/platform/tenants"].is_object());
    assert!(body["paths"]["/api/v1/compliance/status"].is_object());
}

// ═══════════════════════════════════════════════════════════════════════════
// Self-Serve Onboarding Tests
// ═══════════════════════════════════════════════════════════════════════════

/// Helper: register a user on the "platform" tenant and return (access_token, user_id).
async fn register_platform_user(
    app: &axum::Router,
    email: &str,
    password: &str,
) -> (String, String) {
    let body = serde_json::json!({
        "email": email,
        "password": password,
        "name": "Platform User",
    });
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            body,
            "platform",
        ))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "register platform user failed for {}",
        email
    );
    let body = body_json(res).await;
    (
        body["access_token"].as_str().unwrap().to_string(),
        body["user"]["id"].as_str().unwrap().to_string(),
    )
}

/// Helper: seed a platform refresh token row and return the raw refresh token value.
async fn seed_platform_refresh_token(
    pool: &PgPool,
    user_id: &str,
    auth_method: &str,
    mfa_verified: bool,
) -> String {
    let raw_refresh = format!("rt_seed_{}", uuid::Uuid::new_v4());
    let refresh_hash = signing::hash_token(&raw_refresh);
    let family_id = format!("fam_{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, family_id, tenant_id, mfa_verified, auth_method, auth_provider)
         VALUES ($1, $2, $3, $4, $5, 'ten_platform', $6, $7, 'google')",
    )
    .bind(format!("rt_{}", uuid::Uuid::new_v4()))
    .bind(user_id)
    .bind(refresh_hash)
    .bind(chrono::Utc::now() + chrono::Duration::days(7))
    .bind(family_id)
    .bind(mfa_verified)
    .bind(auth_method)
    .execute(pool)
    .await
    .unwrap();

    raw_refresh
}

/// Helper: mint a platform access token from a seeded refresh token with auth context.
async fn mint_platform_access_token_from_refresh(
    app: &axum::Router,
    pool: &PgPool,
    user_id: &str,
    auth_method: &str,
    mfa_verified: bool,
) -> String {
    let raw_refresh = seed_platform_refresh_token(pool, user_id, auth_method, mfa_verified).await;

    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": raw_refresh
            }),
            "platform",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    body["access_token"].as_str().unwrap().to_string()
}

fn totp_from_base32(secret_b32: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let secret = BASE32_NOPAD.decode(secret_b32.as_bytes()).unwrap();
    let mut msg = [0u8; 8];
    let counter = now.timestamp().div_euclid(30) as u64;
    for i in 0..8 {
        msg[7 - i] = ((counter >> (i * 8)) & 0xff) as u8;
    }
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, &secret);
    let tag = hmac::sign(&key, &msg);
    let bytes = tag.as_ref();
    let offset = (bytes[19] & 0x0f) as usize;
    let binary = ((u32::from(bytes[offset]) & 0x7f) << 24)
        | (u32::from(bytes[offset + 1]) << 16)
        | (u32::from(bytes[offset + 2]) << 8)
        | u32::from(bytes[offset + 3]);
    format!("{:06}", binary % 1_000_000)
}

/// Helper: make a self-serve API request with JWT auth.
fn self_serve_request(
    method: &str,
    uri: &str,
    body: serde_json::Value,
    token: &str,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn self_serve_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn self_serve_post(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from("{}"))
        .unwrap()
}

fn self_serve_delete(uri: &str, body: serde_json::Value, token: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

#[tokio::test]
async fn platform_tenant_seed_exists() {
    let (_app, pool) = test_app_with_pool().await;

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tenants WHERE slug = 'platform')")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(exists, "Platform tenant should be seeded by migration 007");
}

#[tokio::test]
async fn self_serve_signup_creates_tenant() {
    let (app, _pool) = test_app_with_pool().await;

    // 1. Register on platform tenant
    let (token, _user_id) = register_platform_user(&app, "dev@example.com", "Str0ngP@ss123").await;

    // 2. Create a tenant via self-serve API
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "My App", "slug": "my-app" }),
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["tenant"]["slug"], "my-app");
    assert_eq!(body["tenant"]["plan"], "trial");
    assert!(body["api_key"].as_str().unwrap().starts_with("mgmt_"));
    assert!(body["tenant"]["trial_ends_at"].is_string());
}

#[tokio::test]
async fn self_serve_requires_platform_jwt() {
    let (app, _pool) = test_app_with_pool().await;

    // Register on a regular tenant (not platform)
    let (token, _rt, _uid) =
        register_user(&app, "user@test.com", "Str0ngP@ss123", Some("User")).await;

    // Try self-serve with non-platform JWT
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Hack", "slug": "hack" }),
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "not_platform_user");
}

#[tokio::test]
async fn self_serve_rejects_reserved_slugs() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "dev2@example.com", "Str0ngP@ss123").await;

    for slug in &["platform", "admin", "api", "auth", "health", "docs"] {
        let res = app
            .clone()
            .oneshot(self_serve_request(
                "POST",
                "/platform/self-serve/tenants",
                serde_json::json!({ "name": "Reserved", "slug": slug }),
                &token,
            ))
            .await
            .unwrap();

        assert_eq!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "Slug '{}' should be rejected",
            slug
        );
        let body = body_json(res).await;
        assert_eq!(body["error"], "reserved_slug");
    }
}

#[tokio::test]
async fn self_serve_rejects_duplicate_slug() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "dev3@example.com", "Str0ngP@ss123").await;

    // Create first tenant
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "First", "slug": "unique-slug" }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Try duplicate
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Second", "slug": "unique-slug" }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"], "slug_taken");
}

#[tokio::test]
async fn self_serve_sets_trial_defaults() {
    let (app, pool) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "dev4@example.com", "Str0ngP@ss123").await;

    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Trial App", "slug": "trial-app" }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap();

    // Verify DB values
    let (plan, max_users, trial_ends): (String, i32, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT COALESCE(plan, 'trial'), max_users, trial_ends_at FROM tenants WHERE id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(plan, "trial");
    assert_eq!(max_users, 100);
    assert!(trial_ends.is_some(), "trial_ends_at should be set");

    // Should be roughly 14 days from now
    let days_left = (trial_ends.unwrap() - chrono::Utc::now()).num_days();
    assert!(
        (13..=14).contains(&days_left),
        "Trial should be ~14 days, got {}",
        days_left
    );
}

#[tokio::test]
async fn self_serve_get_tenant_normalizes_localhost_redirect_in_prod() {
    let (app, pool) = test_app_with_pool().await;
    let (token, _) =
        register_platform_user(&app, "redirect-owner@example.com", "Str0ngP@ss123").await;

    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Redirect App", "slug": "redirect-app" }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap().to_string();

    // Simulate pre-fix legacy data with localhost redirect.
    sqlx::query(
        "UPDATE tenants SET login_redirect_url = 'http://localhost:3000/callback' WHERE id = $1",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let res = app
        .clone()
        .oneshot(self_serve_get(
            &format!("/platform/self-serve/tenants/{}", tenant_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    // login_redirect_url is a top-level field on the tenant response (not nested
    // under branding — branding only holds logo_url, primary_color, app_name).
    let redirect = body["login_redirect_url"].as_str().unwrap();
    // In HTTPS/prod mode, localhost redirects are normalized to the /site fallback
    // (default_login_redirect produces "{server_url}/t/{slug}/site").
    assert_eq!(redirect, "https://auth.gatewasm.test/t/redirect-app/site");
}

#[tokio::test]
async fn self_serve_list_my_tenants() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "dev5@example.com", "Str0ngP@ss123").await;

    // Create two tenants
    for slug in &["list-test-1", "list-test-2"] {
        let res = app
            .clone()
            .oneshot(self_serve_request(
                "POST",
                "/platform/self-serve/tenants",
                serde_json::json!({ "name": format!("App {}", slug), "slug": slug }),
                &token,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    // List tenants
    let res = app
        .clone()
        .oneshot(self_serve_get("/platform/self-serve/tenants", &token))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let tenants = body["tenants"].as_array().unwrap();
    assert_eq!(tenants.len(), 2);
}

#[tokio::test]
async fn self_serve_cant_manage_others_tenant() {
    let (app, _pool) = test_app_with_pool().await;

    // User A creates a tenant
    let (token_a, _) = register_platform_user(&app, "user-a@example.com", "Str0ngP@ss123").await;
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "A's App", "slug": "a-app" }),
            &token_a,
        ))
        .await
        .unwrap();
    let body_a = body_json(res).await;
    let tenant_id = body_a["tenant"]["id"].as_str().unwrap();

    // User B tries to access A's tenant
    let (token_b, _) = register_platform_user(&app, "user-b@example.com", "Str0ngP@ss123").await;
    let res = app
        .clone()
        .oneshot(self_serve_get(
            &format!("/platform/self-serve/tenants/{}", tenant_id),
            &token_b,
        ))
        .await
        .unwrap();
    // After RBAC rework, accessing another user's tenant returns 403 (Forbidden)
    // instead of 404. Both are valid — the key guarantee is it's not 200.
    assert!(
        res.status() == StatusCode::NOT_FOUND || res.status() == StatusCode::FORBIDDEN,
        "User B should not see A's tenant, got {}",
        res.status(),
    );
}

#[tokio::test]
async fn self_serve_rotate_key() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "dev6@example.com", "Str0ngP@ss123").await;

    // Create tenant
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Key App", "slug": "key-app" }),
            &token,
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap();
    let original_key = body["api_key"].as_str().unwrap().to_string();

    // Rotate key
    let res = app
        .clone()
        .oneshot(self_serve_post(
            &format!("/platform/self-serve/tenants/{}/rotate-key", tenant_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let new_key = body["api_key"].as_str().unwrap();
    assert!(new_key.starts_with("mgmt_"));
    assert_ne!(new_key, original_key, "New key should differ from original");
}

#[tokio::test]
async fn self_serve_delete_tenant_requires_exact_slug_confirmation() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _) =
        register_platform_user(&app, "delete-owner1@example.com", "Str0ngP@ss123").await;

    let create_res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Delete Me", "slug": "delete-me-1" }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);
    let create_body = body_json(create_res).await;
    let tenant_id = create_body["tenant"]["id"].as_str().unwrap();

    let delete_res = app
        .clone()
        .oneshot(self_serve_delete(
            &format!("/platform/self-serve/tenants/{}", tenant_id),
            serde_json::json!({ "confirm_slug": "wrong-slug" }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(delete_res.status(), StatusCode::BAD_REQUEST);
    let delete_body = body_json(delete_res).await;
    assert_eq!(delete_body["error"], "confirmation_mismatch");

    // Tenant should still exist.
    let get_res = app
        .oneshot(self_serve_get(
            &format!("/platform/self-serve/tenants/{}", tenant_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(get_res.status(), StatusCode::OK);
}

#[tokio::test]
async fn self_serve_delete_tenant_soft_deletes_and_hides_it() {
    let (app, pool) = test_app_with_pool().await;
    let (token, _) =
        register_platform_user(&app, "delete-owner2@example.com", "Str0ngP@ss123").await;

    let create_res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Delete Me Too", "slug": "delete-me-2" }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);
    let create_body = body_json(create_res).await;
    let tenant_id = create_body["tenant"]["id"].as_str().unwrap().to_string();
    let slug = create_body["tenant"]["slug"].as_str().unwrap().to_string();

    let delete_res = app
        .clone()
        .oneshot(self_serve_delete(
            &format!("/platform/self-serve/tenants/{}", tenant_id),
            serde_json::json!({ "confirm_slug": slug }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(delete_res.status(), StatusCode::OK);
    let delete_body = body_json(delete_res).await;
    assert_eq!(delete_body["status"], "deleted");

    // Not accessible anymore to owner via self-serve detail endpoint.
    let get_res = app
        .clone()
        .oneshot(self_serve_get(
            &format!("/platform/self-serve/tenants/{}", tenant_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(get_res.status(), StatusCode::NOT_FOUND);

    // Not listed anymore.
    let list_res = app
        .clone()
        .oneshot(self_serve_get("/platform/self-serve/tenants", &token))
        .await
        .unwrap();
    assert_eq!(list_res.status(), StatusCode::OK);
    let list_body = body_json(list_res).await;
    let tenants = list_body["tenants"].as_array().unwrap();
    assert!(
        tenants
            .iter()
            .all(|t| t["id"].as_str().unwrap_or_default() != tenant_id),
        "Deleted tenant should not be listed in self-serve tenants"
    );

    // Verify soft-delete in DB.
    let status: String = sqlx::query_scalar("SELECT status FROM tenants WHERE id = $1")
        .bind(&tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "deleted");
}

#[tokio::test]
async fn trial_expired_tenant_blocked() {
    let (app, pool) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "dev7@example.com", "Str0ngP@ss123").await;

    // Create tenant
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Expired App", "slug": "expired-app" }),
            &token,
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap();

    // Manually set trial_ends_at to the past
    sqlx::query("UPDATE tenants SET trial_ends_at = NOW() - INTERVAL '1 day' WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    // Try to access the expired tenant's login page
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/login")
                .header("x-tenant-slug", "expired-app")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "trial_expired");
}

#[tokio::test]
async fn trial_active_tenant_allowed() {
    let (app, _pool) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "dev8@example.com", "Str0ngP@ss123").await;

    // Create tenant (has 14-day trial)
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Active App", "slug": "active-app" }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Access the active trial tenant's registration — should work
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "enduser@example.com",
                "password": "Str0ngP@ss123",
                "name": "End User"
            }),
            "active-app",
        ))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "Active trial tenant should allow registration"
    );
}

#[tokio::test]
async fn max_users_enforced_on_register() {
    let (app, pool) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "dev9@example.com", "Str0ngP@ss123").await;

    // Create tenant with max_users = 100 (default trial)
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({ "name": "Limit App", "slug": "limit-app" }),
            &token,
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    let tenant_id = body["tenant"]["id"].as_str().unwrap();

    // Set max_users to 2 (1 auto-admin created at tenant creation + 1 more allowed)
    sqlx::query("UPDATE tenants SET max_users = 2 WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tenant_entitlements SET max_users = 2 WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    // Register first user — should succeed (auto-admin + this = 2, within limit)
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "first@limit.com",
                "password": "Str0ngP@ss123",
                "name": "First"
            }),
            "limit-app",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Register second user — should be blocked (3 would exceed max_users=2)
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "second@limit.com",
                "password": "Str0ngP@ss123",
                "name": "Second"
            }),
            "limit-app",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "user_limit_reached");
}

#[tokio::test]
async fn reserved_slug_blocked_on_platform_create() {
    let app = test_app().await;

    let res = app
        .clone()
        .oneshot(platform_request(
            "POST",
            "/platform/tenants",
            serde_json::json!({
                "name": "Reserved",
                "slug": "platform",
                "owner_email": "admin@test.com"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "reserved_slug");
}

#[tokio::test]
async fn get_started_redirects_to_platform_login() {
    let app = test_app().await;
    let res = app.oneshot(get_request("/get-started")).await.unwrap();
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = res
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(location, "/platform/onboarding");
}

#[tokio::test]
async fn platform_login_page_returns_html() {
    let app = test_app().await;
    let res = app.oneshot(get_request("/platform/login")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1_000_000)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Platform Login"));
    assert!(body.contains("platform-login"));
    assert!(body.contains("script type=\"module\""));
    assert!(!body.contains("window.prompt("));
    // owner_only / ownerOnly were removed in the URL architecture cleanup
    assert!(!body.contains("owner_only"));
    assert!(!body.contains("ownerOnly"));
}

#[tokio::test]
async fn platform_onboarding_page_returns_html() {
    let app = test_app().await;
    let res = app
        .oneshot(get_request("/platform/onboarding"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1_000_000)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Get Started"));
    assert!(body.contains("platform"));
}

#[tokio::test]
async fn platform_console_page_returns_html() {
    let app = test_app().await;
    let res = app
        .oneshot(get_request("/platform/console"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1_000_000)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Platform Console"));
    assert!(body.contains("platform-console"));
}

#[tokio::test]
async fn platform_admin_dashboard_page_redirects_without_owner_session_cookie() {
    let app = test_app().await;
    let res = app
        .oneshot(get_request("/platform/admin/dashboard"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = res
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(location, "/platform/login");
}

#[tokio::test]
async fn platform_admin_dashboard_page_returns_html_with_owner_session_cookie() {
    let (app, pool) = test_app_with_pool().await;
    let (_seed_token, owner_user_id) =
        register_platform_user(&app, "owner-dashboard@platform.test", "Password123!").await;

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, tenant_id)
         VALUES ($1, 'role_platform_owner', 'ten_platform')
         ON CONFLICT DO NOTHING",
    )
    .bind(&owner_user_id)
    .execute(&pool)
    .await
    .unwrap();

    let owner_refresh = seed_platform_refresh_token(&pool, &owner_user_id, "sso", true).await;
    let refresh_hash = signing::hash_token(&owner_refresh);
    let valid_session_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM refresh_tokens rt
         JOIN users u ON u.id = rt.user_id
         JOIN user_roles ur ON ur.user_id = rt.user_id
         WHERE rt.token_hash = $1
           AND rt.tenant_id = 'ten_platform'
           AND rt.revoked = FALSE
           AND rt.auth_method = 'sso'
           AND rt.mfa_verified = TRUE
           AND u.tenant_id = 'ten_platform'
           AND ur.tenant_id = 'ten_platform'
           AND ur.role_id = 'role_platform_owner'",
    )
    .bind(refresh_hash)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(valid_session_count, 1);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/platform/admin/dashboard")
                .header(
                    "cookie",
                    format!(
                        "gw_rt_platform={}; refresh_token={}",
                        owner_refresh, owner_refresh
                    ),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1_000_000)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("Admin Dashboard"));
    assert!(body.contains("admin-dashboard"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase 2: Per-Tenant Stats Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn self_serve_tenant_stats() {
    let (app, _pool) = test_app_with_pool().await;

    // Register platform user + create a tenant
    let (token, _) = register_platform_user(&app, "stats-owner@test.com", "Password123!").await;

    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({
                "name": "Stats Test",
                "slug": "stats-test",
            }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let create_body = body_json(res).await;
    let tenant_id = create_body["tenant"]["id"].as_str().unwrap().to_string();

    // Register a user on the new tenant to have something in stats
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "enduser@test.com",
                "password": "Password123!",
                "name": "End User",
            }),
            "stats-test",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Call stats endpoint
    let res = app
        .clone()
        .oneshot(self_serve_get(
            &format!("/platform/self-serve/tenants/{}/stats", tenant_id),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let stats = body_json(res).await;
    assert_eq!(stats["tenant_id"], tenant_id);
    assert!(stats["total_users"].as_i64().unwrap() >= 1);
    assert!(stats["providers"].is_object());
}

#[tokio::test]
async fn self_serve_tenant_stats_unauthorized() {
    let app = test_app().await;
    // No auth header
    let res = app
        .oneshot(
            Request::builder()
                .uri("/platform/self-serve/tenants/ten_fake/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn self_serve_tenant_stats_not_owner() {
    let (app, _) = test_app_with_pool().await;

    // User A creates a tenant
    let (token_a, _) = register_platform_user(&app, "owner-a@test.com", "Password123!").await;
    let res = app
        .clone()
        .oneshot(self_serve_request(
            "POST",
            "/platform/self-serve/tenants",
            serde_json::json!({
                "name": "Owner A Tenant",
                "slug": "owner-a-tenant",
            }),
            &token_a,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let tenant_id = body_json(res).await["tenant"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // User B tries to get stats for User A's tenant.
    // After the RBAC rework the middleware returns 403 Forbidden (not 404) when
    // the caller is authenticated but not the tenant owner / assigned member.
    let (token_b, _) = register_platform_user(&app, "owner-b@test.com", "Password123!").await;
    let res = app
        .clone()
        .oneshot(self_serve_get(
            &format!("/platform/self-serve/tenants/{}/stats", tenant_id),
            &token_b,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn self_serve_tenant_stats_nonexistent() {
    let (app, _) = test_app_with_pool().await;
    let (token, _) = register_platform_user(&app, "stats-noent@test.com", "Password123!").await;

    let res = app
        .clone()
        .oneshot(self_serve_get(
            "/platform/self-serve/tenants/ten_nonexistent/stats",
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase 3A: Custom Roles Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn roles_list_returns_system_roles() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "roles-list", "Roles List").await;

    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/roles", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let roles = body["roles"].as_array().unwrap();
    let names: Vec<&str> = roles.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"user"), "should contain system 'user' role");
    assert!(
        names.contains(&"admin"),
        "should contain system 'admin' role"
    );
}

#[tokio::test]
async fn roles_create_custom_role() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-create", "Roles Create").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "editor",
                "description": "Can edit content",
                "permissions": ["user:read", "user:write"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["role"]["name"], "editor");
    assert_eq!(body["role"]["is_system"], false);
    assert!(body["role"]["id"].as_str().unwrap().starts_with("role_"));
}

#[tokio::test]
async fn roles_create_reserved_name_fails() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-reserved", "Roles Reserved").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "admin",
                "permissions": ["user:read"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(res).await["error"], "reserved_name");
}

#[tokio::test]
async fn roles_create_duplicate_name_fails() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "roles-dup", "Roles Dup").await;

    let body = serde_json::json!({"name": "viewer", "permissions": ["user:read"]});
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            body.clone(),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let res = app
        .clone()
        .oneshot(mgmt_request("POST", "/manage/roles", body, &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn roles_update_custom_role() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-update", "Roles Update").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "editor",
                "permissions": ["user:read"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let role_id = body_json(res).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/roles/{}", role_id),
            serde_json::json!({
                "description": "Updated description",
                "permissions": ["user:read", "user:write", "audit:read"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn roles_update_system_role_fails() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-sysupd", "Roles SysUpd").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            "/manage/roles/role_admin",
            serde_json::json!({
                "description": "hacked",
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn roles_delete_custom_role() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "roles-del", "Roles Del").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "temp-role",
                "permissions": ["user:read"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let role_id = body_json(res).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = app
        .clone()
        .oneshot(mgmt_delete(
            &format!("/manage/roles/{}", role_id),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify it's gone
    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/roles", &mgmt_key))
        .await
        .unwrap();
    let roles = body_json(res).await;
    let names: Vec<&str> = roles["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"temp-role"));
}

#[tokio::test]
async fn roles_delete_system_role_fails() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-sysdel", "Roles SysDel").await;

    let res = app
        .clone()
        .oneshot(mgmt_delete("/manage/roles/role_admin", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn roles_assign_to_user() {
    let (app, pool) = test_app_with_pool().await;
    let (_tid, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-assign", "Roles Assign").await;

    // Create a custom role
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "editor",
                "permissions": ["user:read", "user:write"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let role_id = body_json(res).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Register a user on this tenant
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "roleuser@test.com",
                "password": "Password123!",
                "name": "Role User",
            }),
            "roles-assign",
        ))
        .await
        .unwrap();
    let user_id = body_json(res).await["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Assign the role
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/users/{}/roles", user_id),
            serde_json::json!({
                "role_id": role_id,
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify via get user
    let res = app
        .clone()
        .oneshot(mgmt_get(&format!("/manage/users/{}", user_id), &mgmt_key))
        .await
        .unwrap();
    let body = body_json(res).await;
    let roles: Vec<&str> = body["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(roles.contains(&"editor"));
    assert!(roles.contains(&"user"));
}

#[tokio::test]
async fn roles_remove_from_user() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-remove", "Roles Remove").await;

    // Create custom role
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "temp",
                "permissions": ["user:read"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let role_id = body_json(res).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Register user
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "rmrole@test.com", "password": "Password123!", "name": "RM"
            }),
            "roles-remove",
        ))
        .await
        .unwrap();
    let user_id = body_json(res).await["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Assign then remove
    app.clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/users/{}/roles", user_id),
            serde_json::json!({
                "role_id": role_id,
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(mgmt_delete(
            &format!("/manage/users/{}/roles/{}", user_id, role_id),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn roles_in_jwt_after_assign() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "roles-jwt", "Roles JWT").await;

    // Create custom role
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "editor",
                "permissions": ["user:read"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let role_id = body_json(res).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Register user
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "jwtuser@test.com", "password": "Password123!", "name": "JWT User"
            }),
            "roles-jwt",
        ))
        .await
        .unwrap();
    let reg = body_json(res).await;
    let user_id = reg["user"]["id"].as_str().unwrap().to_string();

    // Assign custom role
    app.clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/users/{}/roles", user_id),
            serde_json::json!({
                "role_id": role_id,
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();

    // Login to get fresh JWT with custom role
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/login",
            serde_json::json!({
                "email": "jwtuser@test.com", "password": "Password123!"
            }),
            "roles-jwt",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let login_body = body_json(res).await;
    let roles: Vec<&str> = login_body["user"]["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        roles.contains(&"editor"),
        "JWT should contain custom role 'editor'"
    );
    assert!(
        roles.contains(&"user"),
        "JWT should still contain system 'user' role"
    );
}

#[tokio::test]
async fn roles_permissions_in_me() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "roles-me", "Roles Me").await;

    // Create role with specific permissions
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "auditor",
                "permissions": ["audit:read", "user:read"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let role_id = body_json(res).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Register user + assign role
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "meperms@test.com", "password": "Password123!", "name": "Perm User"
            }),
            "roles-me",
        ))
        .await
        .unwrap();
    let reg = body_json(res).await;
    let user_id = reg["user"]["id"].as_str().unwrap().to_string();
    let access_token = reg["access_token"].as_str().unwrap().to_string();

    app.clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/users/{}/roles", user_id),
            serde_json::json!({
                "role_id": role_id,
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();

    // Call /auth/me with the user's token
    let res = app
        .clone()
        .oneshot(auth_get("/auth/me", &access_token))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let perms: Vec<&str> = body["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        perms.contains(&"audit:read"),
        "should have audit:read permission"
    );
    assert!(
        perms.contains(&"user:read"),
        "should have user:read permission"
    );
}

#[tokio::test]
async fn roles_invalid_permissions_rejected() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-badperm", "Roles BadPerm").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "bad-role",
                "permissions": ["totally_fake_permission"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(res).await["error"], "invalid_permission");
}

#[tokio::test]
async fn roles_custom_permissions_accepted() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-custperm", "Roles CustPerm").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "custom-role",
                "permissions": ["custom:billing:read", "custom:reports:write"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn roles_scoped_to_tenant() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key_a) = create_test_tenant_with_mgmt_key(&pool, "roles-scope-a", "Scope A").await;
    let (_, mgmt_key_b) = create_test_tenant_with_mgmt_key(&pool, "roles-scope-b", "Scope B").await;

    // Create role in tenant A
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "reviewer",
                "permissions": ["user:read"],
            }),
            &mgmt_key_a,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let role_id = body_json(res).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Tenant B should NOT see tenant A's custom role (only system roles)
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            "/manage/roles",
            serde_json::json!({}),
            &mgmt_key_b,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let roles = body_json(res).await;
    let role_names: Vec<&str> = roles["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(
        !role_names.contains(&"reviewer"),
        "Tenant B should not see tenant A's custom role"
    );

    // Tenant B cannot update tenant A's role
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/roles/{}", role_id),
            serde_json::json!({
                "name": "hacked"
            }),
            &mgmt_key_b,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Tenant B cannot delete tenant A's role
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "DELETE",
            &format!("/manage/roles/{}", role_id),
            serde_json::json!({}),
            &mgmt_key_b,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_create_user_with_custom_role() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "roles-admcreate", "Roles AdminCreate").await;

    // Create custom role
    app.clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "moderator",
                "permissions": ["user:read", "user:write"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();

    // Create user with custom role name
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({
                "email": "mod@test.com",
                "password": "Password123!",
                "name": "Mod User",
                "role": "moderator",
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let roles: Vec<&str> = body["user"]["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(roles.contains(&"moderator"));
}

// ════════════════════════════════════════════════════════════════════════
// Phase 3B — Webhooks
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn webhook_create() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "wh-create", "WH Create").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://example.com/webhook",
                "events": ["user.registered", "user.logged_in"],
                "description": "Test webhook",
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert!(body["webhook"]["id"].as_str().unwrap().starts_with("wh_"));
    assert!(body["webhook"]["secret"]
        .as_str()
        .unwrap()
        .starts_with("whsec_"));
    assert_eq!(body["webhook"]["url"], "https://example.com/webhook");
    assert_eq!(body["webhook"]["active"], true);
}

#[tokio::test]
async fn webhook_create_invalid_events() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "wh-inval", "WH Invalid").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://example.com/webhook",
                "events": ["user.registered", "totally.bogus"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(res).await["error"], "invalid_event");
}

#[tokio::test]
async fn webhook_list() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "wh-list", "WH List").await;

    // Create two webhooks
    app.clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://a.com/wh", "events": ["user.registered"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://b.com/wh", "events": ["user.logged_in"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            "/manage/webhooks",
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["webhooks"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn webhook_get_masked_secret() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "wh-get", "WH Get").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://example.com/wh", "events": ["user.registered"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let create_body = body_json(res).await;
    let wh_id = create_body["webhook"]["id"].as_str().unwrap();
    let full_secret = create_body["webhook"]["secret"].as_str().unwrap();

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            &format!("/manage/webhooks/{}", wh_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let masked = body["webhook"]["secret"].as_str().unwrap();
    // Secret should be masked (not equal to original)
    assert_ne!(masked, full_secret);
    assert!(masked.contains("..."));
}

#[tokio::test]
async fn webhook_update() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "wh-update", "WH Update").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://example.com/wh", "events": ["user.registered"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let wh_id = body_json(res).await["webhook"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Update URL and active status
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/webhooks/{}", wh_id),
            serde_json::json!({
                "url": "https://new-url.com/wh",
                "active": false,
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify update
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            &format!("/manage/webhooks/{}", wh_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    assert_eq!(body["webhook"]["url"], "https://new-url.com/wh");
    assert_eq!(body["webhook"]["active"], false);
}

#[tokio::test]
async fn webhook_delete() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "wh-delete", "WH Delete").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://example.com/wh", "events": ["user.registered"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let wh_id = body_json(res).await["webhook"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "DELETE",
            &format!("/manage/webhooks/{}", wh_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Should be gone
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            &format!("/manage/webhooks/{}", wh_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn webhook_deliveries_empty() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "wh-deliv", "WH Deliveries").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://example.com/wh", "events": ["user.registered"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let wh_id = body_json(res).await["webhook"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            &format!("/manage/webhooks/{}/deliveries", wh_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["deliveries"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn webhook_test_delivery() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "wh-test", "WH Test").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://httpbin.org/status/200",
                "events": ["user.registered"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let wh_id = body_json(res).await["webhook"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/webhooks/{}/test", wh_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body["delivery_id"].as_str().unwrap().starts_with("whd_"));
    // Status is either "delivered" or "failed" (depending on network), both are valid
    assert!(["delivered", "failed"].contains(&body["status"].as_str().unwrap()));

    // Check that a delivery record was created
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            &format!("/manage/webhooks/{}/deliveries", wh_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["deliveries"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn webhook_scoped_to_tenant() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key_a) = create_test_tenant_with_mgmt_key(&pool, "wh-scope-a", "WH Scope A").await;
    let (_, mgmt_key_b) = create_test_tenant_with_mgmt_key(&pool, "wh-scope-b", "WH Scope B").await;

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://example.com/wh", "events": ["user.registered"],
            }),
            &mgmt_key_a,
        ))
        .await
        .unwrap();
    let wh_id = body_json(res).await["webhook"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Tenant B can't see tenant A's webhook
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            &format!("/manage/webhooks/{}", wh_id),
            serde_json::json!({}),
            &mgmt_key_b,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Tenant B can't delete tenant A's webhook
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "DELETE",
            &format!("/manage/webhooks/{}", wh_id),
            serde_json::json!({}),
            &mgmt_key_b,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn webhook_unauthorized() {
    let (app, _pool) = test_app_with_pool().await;

    // No auth header
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/manage/webhooks")
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn webhook_dispatch_creates_delivery() {
    let (app, pool) = test_app_with_pool().await;
    let (tid, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "wh-dispatch", "WH Dispatch").await;

    // Insert the webhook directly into the DB so we can use 127.0.0.1:9 (the
    // "discard" port).  Port 9 returns ECONNREFUSED immediately, so the delivery
    // engine records the failure row within milliseconds.  We bypass the API
    // because the private-URL guard in POST /manage/webhooks now rejects
    // 127.0.0.1 — but the guard is for external callers, not internal test setup.
    let wh_id = format!("wh_{}", uuid::Uuid::new_v4());
    let secret = "whsec_test_secret";
    sqlx::query(
        "INSERT INTO webhooks (id, tenant_id, url, secret, events, active, description) \
         VALUES ($1, $2, 'http://127.0.0.1:9/webhook', $3, '[\"user.registered\"]'::jsonb, TRUE, '')",
    )
    .bind(&wh_id)
    .bind(&tid)
    .bind(secret)
    .execute(&pool)
    .await
    .expect("Failed to insert test webhook");

    // Register a user (which should trigger webhook dispatch)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/register")
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wh-dispatch")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "email": "webhookuser@test.com",
                        "password": "Password123!",
                        "name": "Webhook User"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Poll because dispatch is async and runs in a spawned task.
    let mut body = serde_json::json!({ "deliveries": [] });
    for _ in 0..25 {
        let res = app
            .clone()
            .oneshot(mgmt_request(
                "GET",
                &format!("/manage/webhooks/{}/deliveries", wh_id),
                serde_json::json!({}),
                &mgmt_key,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        body = body_json(res).await;
        if body["deliveries"]
            .as_array()
            .map(|d| !d.is_empty())
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let deliveries = body["deliveries"].as_array().unwrap();
    assert!(
        !deliveries.is_empty(),
        "Expected at least 1 delivery record, got {}",
        deliveries.len()
    );
    assert_eq!(deliveries[0]["event_type"], "user.registered");
}

#[tokio::test]
async fn webhook_inactive_not_dispatched() {
    let (app, pool) = test_app_with_pool().await;
    let (_tid, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "wh-inactive", "WH Inactive").await;

    // Create webhook and deactivate it
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://httpbin.org/post",
                "events": ["user.registered"],
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let wh_id = body_json(res).await["webhook"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/webhooks/{}", wh_id),
            serde_json::json!({
                "active": false,
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();

    // Register a user
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/register")
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wh-inactive")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "email": "inactive-wh@test.com",
                        "password": "Password123!",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // No deliveries should exist (webhook was inactive)
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "GET",
            &format!("/manage/webhooks/{}/deliveries", wh_id),
            serde_json::json!({}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    let body = body_json(res).await;
    assert_eq!(body["deliveries"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn tenant_admin_console_route_returns_html() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/t/{}/admin", TEST_TENANT_SLUG))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;
    assert!(body.contains("Admin Console"));
}

#[tokio::test]
async fn tenant_dashboard_and_admin_include_login_redirect_url_in_config() {
    let (app, pool) = test_app_with_pool().await;
    let login_redirect = "https://customer.example.com/auth/callback";

    sqlx::query("UPDATE tenants SET login_redirect_url = $1 WHERE slug = $2")
        .bind(login_redirect)
        .bind(TEST_TENANT_SLUG)
        .execute(&pool)
        .await
        .unwrap();

    let dashboard_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/t/{}/dashboard", TEST_TENANT_SLUG))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard_res.status(), StatusCode::OK);
    let dashboard_body = body_string(dashboard_res).await;
    assert!(
        dashboard_body.contains(&format!(r#""login_redirect_url":"{}""#, login_redirect))
            || dashboard_body.contains(&format!(r#""loginRedirectUrl":"{}""#, login_redirect)),
        "dashboard config should include login_redirect_url"
    );

    let admin_res = app
        .oneshot(
            Request::builder()
                .uri(format!("/t/{}/admin", TEST_TENANT_SLUG))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_res.status(), StatusCode::OK);
    let admin_body = body_string(admin_res).await;
    assert!(
        admin_body.contains(&format!(r#""login_redirect_url":"{}""#, login_redirect))
            || admin_body.contains(&format!(r#""loginRedirectUrl":"{}""#, login_redirect)),
        "admin config should include loginRedirectUrl"
    );
}

#[tokio::test]
async fn admin_create_user_respects_entitlement_max_users() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "ent-users", "Ent Users").await;

    sqlx::query(
        "INSERT INTO tenant_entitlements \
         (tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations) \
         VALUES ($1, 'trial', 1, 3, TRUE, TRUE, TRUE) \
         ON CONFLICT (tenant_id) DO UPDATE SET max_users = EXCLUDED.max_users",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let first = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({"email": "first@ent-users.test", "password": "Str0ngP@ss1"}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = app
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({"email": "second@ent-users.test", "password": "Str0ngP@ss1"}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::FORBIDDEN);
    let body = body_json(second).await;
    assert_eq!(body["error"], "user_limit_reached");
}

#[tokio::test]
async fn custom_roles_blocked_when_entitlement_disabled() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "ent-roles", "Ent Roles").await;

    sqlx::query(
        "INSERT INTO tenant_entitlements \
         (tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations) \
         VALUES ($1, 'trial', 100, 3, FALSE, TRUE, TRUE) \
         ON CONFLICT (tenant_id) DO UPDATE SET allow_custom_roles = EXCLUDED.allow_custom_roles",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let res = app
        .oneshot(mgmt_request(
            "POST",
            "/manage/roles",
            serde_json::json!({
                "name": "reviewer",
                "description": "Can review content",
                "permissions": ["user:read"]
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "feature_not_enabled");
}

#[tokio::test]
async fn webhooks_blocked_when_entitlement_disabled() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) = create_test_tenant_with_mgmt_key(&pool, "ent-wh", "Ent WH").await;

    sqlx::query(
        "INSERT INTO tenant_entitlements \
         (tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations) \
         VALUES ($1, 'trial', 100, 3, TRUE, FALSE, TRUE) \
         ON CONFLICT (tenant_id) DO UPDATE SET allow_webhooks = EXCLUDED.allow_webhooks",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let res = app
        .oneshot(mgmt_request(
            "POST",
            "/manage/webhooks",
            serde_json::json!({
                "url": "https://example.com/webhook",
                "events": ["user.registered"]
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "feature_not_enabled");
}

#[tokio::test]
async fn organizations_crud_and_member_flow() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "org-flow", "Org Flow").await;

    sqlx::query(
        "INSERT INTO tenant_entitlements \
         (tenant_id, plan, max_users, max_organizations, allow_custom_roles, allow_webhooks, allow_organizations) \
         VALUES ($1, 'trial', 100, 3, TRUE, TRUE, TRUE) \
         ON CONFLICT (tenant_id) DO NOTHING",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let reg = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "org-member@test.com",
                "password": "Str0ngP@ss1",
                "name": "Org Member"
            }),
            "org-flow",
        ))
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);
    let reg_body = body_json(reg).await;
    let user_id = reg_body["user"]["id"].as_str().unwrap().to_string();

    let created = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/organizations",
            serde_json::json!({
                "name": "Acme North",
                "slug": "acme-north",
                "metadata": {"region": "north-america"}
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body = body_json(created).await;
    let org_id = created_body["organization"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add_member_res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            &format!("/manage/organizations/{}/members", org_id),
            serde_json::json!({"user_id": user_id, "role": "admin"}),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(add_member_res.status(), StatusCode::CREATED);

    let members = app
        .clone()
        .oneshot(mgmt_get(
            &format!("/manage/organizations/{}/members", org_id),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(members.status(), StatusCode::OK);
    let members_body = body_json(members).await;
    assert_eq!(members_body["members"].as_array().unwrap().len(), 1);

    let remove_member_res = app
        .clone()
        .oneshot(mgmt_delete(
            &format!("/manage/organizations/{}/members/{}", org_id, user_id),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(remove_member_res.status(), StatusCode::NO_CONTENT);

    let delete_org = app
        .oneshot(mgmt_delete(
            &format!("/manage/organizations/{}", org_id),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(delete_org.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_tenant_security_config_defaults() {
    let app = test_app().await;

    // GET /platform/tenants should include security_config with foundation defaults
    let res = app
        .oneshot(platform_get("/platform/tenants"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    let tenants = body["tenants"].as_array().unwrap();
    assert!(!tenants.is_empty(), "Should have at least the test tenant");

    let tenant = &tenants[0];
    let sc = &tenant["security_config"];

    // Foundation defaults (all ON)
    assert_eq!(sc["idle_timeout_minutes"], 30);
    assert_eq!(sc["host_prefix_cookies"], true);
    assert_eq!(sc["csp_enabled"], true);
    assert_eq!(sc["client_fingerprint"], true);

    // Pro defaults (all OFF)
    assert_eq!(sc["webauthn_enabled"], false);
    assert_eq!(sc["webauthn_attestation"], "none");
    assert_eq!(sc["webauthn_require_device_bound"], false);
    assert_eq!(sc["fedcm_enabled"], false);

    // Enterprise defaults (all OFF)
    assert_eq!(sc["dpop_required"], false);
    assert_eq!(sc["par_required"], false);
    assert_eq!(sc["mtls_enabled"], false);
    assert_eq!(sc["introspection_enabled"], false);

    // Compliance defaults (all OFF)
    assert_eq!(sc["caep_enabled"], false);
    assert_eq!(sc["impossible_travel_enabled"], false);
    assert_eq!(sc["impossible_travel_speed_threshold_kmh"], 800);
}

#[tokio::test]
async fn test_manage_config_includes_security_config() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "sec-config-tenant", "Sec Config Tenant").await;

    let res = app
        .oneshot(mgmt_get("/manage/config", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    // security_config should be present with foundation defaults
    let sc = &body["security_config"];
    assert_eq!(sc["idle_timeout_minutes"], 30);
    assert_eq!(sc["host_prefix_cookies"], true);
    assert_eq!(sc["csp_enabled"], true);
    assert_eq!(sc["client_fingerprint"], true);
    assert_eq!(sc["webauthn_enabled"], false);
    assert_eq!(sc["dpop_required"], false);
}

#[tokio::test]
async fn test_update_tenant_security_config() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "sec-update-tenant", "Sec Update Tenant").await;

    // PATCH /manage/tenants/{id}/security with { "idle_timeout_minutes": 15 }
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({ "idle_timeout_minutes": 15 }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    let sc = &body["security_config"];
    // Updated value
    assert_eq!(sc["idle_timeout_minutes"], 15);
    // Foundation defaults preserved for unset fields
    assert_eq!(sc["host_prefix_cookies"], true);
    assert_eq!(sc["csp_enabled"], true);
    assert_eq!(sc["client_fingerprint"], true);
    // Pro/Enterprise/Compliance defaults preserved
    assert_eq!(sc["webauthn_enabled"], false);
    assert_eq!(sc["dpop_required"], false);
    assert_eq!(sc["caep_enabled"], false);
}

#[tokio::test]
async fn test_security_config_plan_gating_rejects_enterprise_on_trial() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "sec-gate-trial", "Sec Gate Trial").await;

    // Ensure tenant_entitlements has plan='trial'
    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users) VALUES ($1, 'trial', 100) ON CONFLICT (tenant_id) DO UPDATE SET plan = 'trial'",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to set trial plan");

    // PATCH with { "dpop_required": true } on trial plan -> should get 403
    let res = app
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({ "dpop_required": true }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    let msg = body["message"].as_str().unwrap();
    assert!(
        msg.contains("dpop_required requires enterprise plan or above"),
        "Error message should mention enterprise plan requirement, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_security_config_plan_gating_allows_on_correct_plan() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "sec-gate-ent", "Sec Gate Enterprise").await;

    // Update tenant to 'enterprise' plan
    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users) VALUES ($1, 'enterprise', 1000) ON CONFLICT (tenant_id) DO UPDATE SET plan = 'enterprise'",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to set enterprise plan");

    // PATCH with { "dpop_required": true } on enterprise plan -> should succeed
    let res = app
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({ "dpop_required": true }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    let sc = &body["security_config"];
    assert_eq!(sc["dpop_required"], true);
    assert_eq!(body["plan"], "enterprise");
    // Foundation defaults still preserved
    assert_eq!(sc["idle_timeout_minutes"], 30);
    assert_eq!(sc["host_prefix_cookies"], true);
}


#[tokio::test]
async fn test_pro_tenant_cannot_enable_caep() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "plan-gate-pro-caep", "Pro CAEP Tenant").await;

    // Set plan to 'pro'
    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users) VALUES ($1, 'pro', 500) ON CONFLICT (tenant_id) DO UPDATE SET plan = 'pro'",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to set pro plan");

    // Attempt to enable caep_enabled on pro plan -> 403
    let res = app
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({ "caep_enabled": true }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    let msg = body["message"].as_str().unwrap();
    assert!(
        msg.contains("caep_enabled requires compliance plan"),
        "Expected compliance plan violation for caep_enabled, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_enterprise_tenant_can_enable_dpop_but_not_caep() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "plan-gate-ent-mixed", "Enterprise Mixed Tenant")
            .await;

    // Set plan to 'enterprise'
    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users) VALUES ($1, 'enterprise', 1000) ON CONFLICT (tenant_id) DO UPDATE SET plan = 'enterprise'",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to set enterprise plan");

    // dpop_required should succeed on enterprise
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({ "dpop_required": true }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["security_config"]["dpop_required"], true);
    assert_eq!(body["plan"], "enterprise");

    // caep_enabled should be rejected on enterprise (requires compliance)
    let res = app
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({ "caep_enabled": true }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    let msg = body["message"].as_str().unwrap();
    assert!(
        msg.contains("caep_enabled requires compliance plan"),
        "Expected compliance plan violation for caep_enabled on enterprise, got: {}",
        msg
    );
}

#[tokio::test]
async fn test_compliance_tenant_can_enable_everything() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "plan-gate-compliance-all", "Compliance All Tenant")
            .await;

    // Set plan to 'compliance'
    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users) VALUES ($1, 'compliance', 5000) ON CONFLICT (tenant_id) DO UPDATE SET plan = 'compliance'",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to set compliance plan");

    // Enable everything in one PATCH — should all succeed
    let res = app
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({
                "webauthn_enabled": true,
                "fedcm_enabled": true,
                "dpop_required": true,
                "par_required": true,
                "mtls_enabled": true,
                "introspection_enabled": true,
                "caep_enabled": true,
                "impossible_travel_enabled": true,
                "idle_timeout_minutes": 10
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let sc = &body["security_config"];
    assert_eq!(body["plan"], "compliance");

    // Verify all features enabled
    assert_eq!(sc["webauthn_enabled"], true);
    assert_eq!(sc["fedcm_enabled"], true);
    assert_eq!(sc["dpop_required"], true);
    assert_eq!(sc["par_required"], true);
    assert_eq!(sc["mtls_enabled"], true);
    assert_eq!(sc["introspection_enabled"], true);
    assert_eq!(sc["caep_enabled"], true);
    assert_eq!(sc["impossible_travel_enabled"], true);
    assert_eq!(sc["idle_timeout_minutes"], 10);

    // Foundation defaults still preserved
    assert_eq!(sc["host_prefix_cookies"], true);
    assert_eq!(sc["csp_enabled"], true);
    assert_eq!(sc["client_fingerprint"], true);
}

#[tokio::test]
async fn test_idle_timeout_with_custom_config() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "idle-timeout-custom", "Idle Timeout Custom Tenant")
            .await;

    // Set a custom idle timeout via PATCH
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({ "idle_timeout_minutes": 5 }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["security_config"]["idle_timeout_minutes"], 5);

    // Verify the config was persisted by reading it back via GET /manage/config
    let res = app
        .oneshot(mgmt_get("/manage/config", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(
        body["security_config"]["idle_timeout_minutes"], 5,
        "Custom idle timeout should persist and be returned in config"
    );
}

#[tokio::test]
async fn test_security_config_partial_update() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "partial-update-sec", "Partial Update Tenant")
            .await;

    // Set plan to 'enterprise' so we can enable enterprise features
    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users) VALUES ($1, 'enterprise', 1000) ON CONFLICT (tenant_id) DO UPDATE SET plan = 'enterprise'",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to set enterprise plan");

    // First PATCH: set idle_timeout_minutes=10 and dpop_required=true
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({
                "idle_timeout_minutes": 10,
                "dpop_required": true
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["security_config"]["idle_timeout_minutes"], 10);
    assert_eq!(body["security_config"]["dpop_required"], true);
    assert_eq!(body["security_config"]["par_required"], false);

    // Second PATCH: only update par_required=true — other fields must remain unchanged
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "PATCH",
            &format!("/manage/tenants/{}/security", tenant_id),
            serde_json::json!({ "par_required": true }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let sc = &body["security_config"];

    // par_required was updated
    assert_eq!(sc["par_required"], true, "par_required should be updated to true");

    // Previously set fields must remain unchanged
    assert_eq!(
        sc["idle_timeout_minutes"], 10,
        "idle_timeout_minutes should remain 10 after partial update"
    );
    assert_eq!(
        sc["dpop_required"], true,
        "dpop_required should remain true after partial update"
    );

    // Foundation defaults still intact
    assert_eq!(sc["host_prefix_cookies"], true);
    assert_eq!(sc["csp_enabled"], true);
    assert_eq!(sc["client_fingerprint"], true);

    // Unset fields still at defaults
    assert_eq!(sc["webauthn_enabled"], false);
    assert_eq!(sc["caep_enabled"], false);
    assert_eq!(sc["mtls_enabled"], false);
}

// ── CSP Header Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_csp_header_on_login_page() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/t/{}/login", TEST_TENANT_SLUG))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let csp = res
        .headers()
        .get("content-security-policy")
        .expect("Login page must have CSP header")
        .to_str()
        .unwrap();

    // Verify strict directives
    assert!(
        csp.contains("default-src 'none'"),
        "CSP must have default-src 'none'"
    );
    assert!(
        csp.contains("script-src 'nonce-"),
        "CSP must have nonce-based script-src"
    );
    // style-src: nonce + self (for personality CSS inline styles)
    assert!(
        csp.contains("style-src 'nonce-"),
        "CSP style-src must include nonce for inline styles"
    );
    assert!(
        csp.contains("'self'"),
        "CSP style-src must include 'self' for stylesheets"
    );
    assert!(
        !csp.contains("unsafe-inline"),
        "CSP must NOT allow unsafe-inline anywhere"
    );
    assert!(
        csp.contains("frame-ancestors 'none'"),
        "CSP must prevent framing"
    );
    assert!(
        csp.contains("base-uri 'none'"),
        "CSP must prevent base tag hijacking"
    );
    assert!(
        !csp.contains("unsafe-eval"),
        "CSP must NOT allow unsafe-eval"
    );

    // Clone CSP before consuming res
    let csp = csp.to_string();

    // Verify the nonce is in the HTML body
    let body = body_string(res).await;
    // Extract nonce from CSP header: script-src 'nonce-XXXX'
    let nonce_start = csp.find("script-src 'nonce-").unwrap() + "script-src 'nonce-".len();
    let nonce_end = csp[nonce_start..].find('\'').unwrap() + nonce_start;
    let nonce = &csp[nonce_start..nonce_end];
    assert!(
        body.contains(&format!("nonce=\"{}\"", nonce)),
        "HTML must contain the nonce from the CSP header on script/style tags"
    );
}

#[tokio::test]
async fn test_csp_nonce_unique_per_request() {
    let (app, _pool) = test_app_with_pool().await;
    let uri = format!("/t/{}/login", TEST_TENANT_SLUG);

    let res1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csp1 = res1
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let res2 = app
        .oneshot(
            Request::builder()
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csp2 = res2
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Nonces must differ between requests
    assert_ne!(csp1, csp2, "CSP nonces must be unique per request");
}

#[tokio::test]
async fn test_csp_header_on_dashboard_page() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/t/{}/dashboard", TEST_TENANT_SLUG))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let csp = res
        .headers()
        .get("content-security-policy")
        .expect("Dashboard page must have CSP header")
        .to_str()
        .unwrap();

    assert!(
        csp.contains("default-src 'none'"),
        "Dashboard CSP must have default-src 'none'"
    );
    assert!(
        csp.contains("script-src 'nonce-"),
        "Dashboard CSP must have nonce-based script-src"
    );
    // No unsafe-inline anywhere — all styles moved to CSS classes
    assert!(
        !csp.contains("unsafe-inline"),
        "Dashboard CSP must NOT allow unsafe-inline"
    );

    // Clone CSP before consuming res
    let csp = csp.to_string();

    // Verify nonce appears in the HTML
    let body = body_string(res).await;
    let nonce_start = csp.find("script-src 'nonce-").unwrap() + "script-src 'nonce-".len();
    let nonce_end = csp[nonce_start..].find('\'').unwrap() + nonce_start;
    let nonce = &csp[nonce_start..nonce_end];
    assert!(
        body.contains(&format!("nonce=\"{}\"", nonce)),
        "Dashboard HTML must contain the nonce from the CSP header"
    );
}

// ── Idle session timeout tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_idle_session_timeout_rejects_stale_token() {
    let (app, pool) = test_app_with_pool().await;
    let (_, refresh_token, _) =
        register_user(&app, "idle-timeout@example.com", "Str0ngP@ss", None).await;

    // Backdate last_activity_at to 31 minutes ago (default idle timeout is 30 min)
    let token_hash = signing::hash_token(&refresh_token);
    sqlx::query(
        "UPDATE refresh_tokens SET last_activity_at = NOW() - INTERVAL '31 minutes' WHERE token_hash = $1",
    )
    .bind(&token_hash)
    .execute(&pool)
    .await
    .expect("Failed to backdate last_activity_at");

    // Attempt refresh — should fail with 401 session_expired
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert_eq!(body["error"], "session_expired");
    assert_eq!(body["message"], "Session expired due to inactivity");
}

#[tokio::test]
async fn test_idle_session_timeout_updates_activity() {
    let (app, pool) = test_app_with_pool().await;
    let (_, refresh_token, _) =
        register_user(&app, "idle-activity@example.com", "Str0ngP@ss", None).await;

    // Refresh immediately — should succeed and update last_activity_at
    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let new_refresh_token = body["refresh_token"].as_str().unwrap();

    // Verify last_activity_at on the NEW token is recent (within 5 seconds)
    let new_hash = signing::hash_token(new_refresh_token);
    let last_activity: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT last_activity_at FROM refresh_tokens WHERE token_hash = $1",
    )
    .bind(&new_hash)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch last_activity_at for new token");

    let age = chrono::Utc::now() - last_activity;
    assert!(
        age.num_seconds() < 5,
        "last_activity_at on new token should be within 5 seconds of now, but was {} seconds ago",
        age.num_seconds()
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Client Fingerprint Bridging Tests
// ═══════════════════════════════════════════════════════════════════════════

/// Build a register request with custom User-Agent and X-Forwarded-For headers.
fn register_request_with_fingerprint(
    email: &str,
    password: &str,
    user_agent: &str,
    forwarded_for: &str,
) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/auth/register")
        .header("content-type", "application/json")
        .header("x-tenant-slug", TEST_TENANT_SLUG)
        .header("user-agent", user_agent)
        .header("x-forwarded-for", forwarded_for)
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": email,
                "password": password,
            }))
            .unwrap(),
        ))
        .unwrap()
}

/// Build a refresh token request with custom User-Agent and X-Forwarded-For headers.
fn refresh_request_with_fingerprint(
    refresh_token: &str,
    user_agent: &str,
    forwarded_for: &str,
) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/auth/token")
        .header("content-type", "application/json")
        .header("x-tenant-slug", TEST_TENANT_SLUG)
        .header("user-agent", user_agent)
        .header("x-forwarded-for", forwarded_for)
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }))
            .unwrap(),
        ))
        .unwrap()
}

#[tokio::test]
async fn test_client_fingerprint_stored_on_register() {
    let (app, pool) = test_app_with_pool().await;

    let res = app
        .clone()
        .oneshot(register_request_with_fingerprint(
            "fp-reg@example.com",
            "Str0ngP@ss",
            "Mozilla/5.0 Chrome",
            "192.168.1.100",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let refresh_token = body["refresh_token"].as_str().unwrap();
    let hash = signing::hash_token(refresh_token);

    let fp: Option<String> =
        sqlx::query_scalar("SELECT client_fingerprint FROM refresh_tokens WHERE token_hash = $1")
            .bind(&hash)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(fp.is_some(), "client_fingerprint should be stored on register");
    assert_eq!(fp.unwrap().len(), 64, "fingerprint should be 64-char hex SHA-256");
}

#[tokio::test]
async fn test_client_fingerprint_same_returns_zero_risk() {
    let (app, pool) = test_app_with_pool().await;

    // Register with fingerprint A
    let res = app
        .clone()
        .oneshot(register_request_with_fingerprint(
            "fp-same@example.com",
            "Str0ngP@ss",
            "Mozilla/5.0 Chrome",
            "192.168.1.100",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    // Refresh with same fingerprint (same /24 subnet)
    let res = app
        .clone()
        .oneshot(refresh_request_with_fingerprint(
            &refresh_token,
            "Mozilla/5.0 Chrome",
            "192.168.1.200", // same /24 as .100
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let new_refresh = body["refresh_token"].as_str().unwrap();
    let new_hash = signing::hash_token(new_refresh);

    let risk: i32 =
        sqlx::query_scalar("SELECT risk_score FROM refresh_tokens WHERE token_hash = $1")
            .bind(&new_hash)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(risk, 0, "risk_score should remain 0 for same fingerprint");
}

#[tokio::test]
async fn test_client_fingerprint_drift_increments_risk_score() {
    let (app, pool) = test_app_with_pool().await;

    // Register with fingerprint A (Chrome + 192.168.1.x)
    let res = app
        .clone()
        .oneshot(register_request_with_fingerprint(
            "fp-drift@example.com",
            "Str0ngP@ss",
            "Mozilla/5.0 Chrome",
            "192.168.1.100",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let refresh_token = body["refresh_token"].as_str().unwrap().to_string();

    // Refresh with different fingerprint (different UA + different subnet)
    let res = app
        .clone()
        .oneshot(refresh_request_with_fingerprint(
            &refresh_token,
            "curl/7.88.1",
            "10.0.0.50",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "First drift should succeed");
    let body = body_json(res).await;
    let new_refresh = body["refresh_token"].as_str().unwrap();
    let new_hash = signing::hash_token(new_refresh);

    let risk: i32 =
        sqlx::query_scalar("SELECT risk_score FROM refresh_tokens WHERE token_hash = $1")
            .bind(&new_hash)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        risk > 0,
        "risk_score should be > 0 after fingerprint drift, got {}",
        risk
    );
}

#[tokio::test]
async fn test_client_fingerprint_family_revoked_at_threshold() {
    let (app, pool) = test_app_with_pool().await;

    // Register with fingerprint A
    let res = app
        .clone()
        .oneshot(register_request_with_fingerprint(
            "fp-revoke@example.com",
            "Str0ngP@ss",
            "Mozilla/5.0 Chrome",
            "192.168.1.100",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let mut current_refresh = body["refresh_token"].as_str().unwrap().to_string();

    // Each refresh uses a different fingerprint to increment risk_score.
    // We need 3 drifts to reach risk_score >= 3 (at which point the family is revoked).
    let agents_and_ips = [
        ("curl/7.88.1", "10.0.0.1"),
        ("wget/1.21", "172.16.0.1"),
        ("HTTPie/3.2", "203.0.113.1"),
    ];

    for (i, (ua, ip)) in agents_and_ips.iter().enumerate() {
        let res = app
            .clone()
            .oneshot(refresh_request_with_fingerprint(&current_refresh, ua, ip))
            .await
            .unwrap();

        if i < 2 {
            // First two drifts should succeed (risk_score goes 1, 2)
            assert_eq!(
                res.status(),
                StatusCode::OK,
                "Drift {} should succeed (risk_score {})",
                i + 1,
                i + 1
            );
            let body = body_json(res).await;
            current_refresh = body["refresh_token"].as_str().unwrap().to_string();
        } else {
            // Third drift: risk_score reaches 3 → family revoked → 401
            assert_eq!(
                res.status(),
                StatusCode::UNAUTHORIZED,
                "Drift {} should be rejected (risk_score >= 3)",
                i + 1
            );
            let body = body_json(res).await;
            assert_eq!(body["error"], "token_risk_exceeded");
        }
    }

    // Verify the entire family is revoked in the DB
    let non_revoked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens rt
         JOIN users u ON u.id = rt.user_id
         WHERE u.email = 'fp-revoke@example.com' AND rt.revoked = FALSE",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        non_revoked, 0,
        "All tokens in the family should be revoked after risk threshold"
    );
}

// ── Bridge Integrity Endpoint ───────────────────────────────────────

#[tokio::test]
async fn test_bridge_integrity_endpoint() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bridge/integrity")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // Verify CORP header
    let corp = res
        .headers()
        .get("cross-origin-resource-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(corp, "cross-origin");

    // Verify Cache-Control header
    let cache_control = res
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(cache_control, "public, max-age=3600");

    let body = body_json(res).await;
    let sri_hash = body["sri_hash"].as_str().unwrap();
    assert!(
        sri_hash.starts_with("sha384-"),
        "SRI hash should start with sha384-"
    );
    assert!(body["version"].is_string(), "version should be a string");
    let usage = body["usage"].as_str().unwrap();
    assert!(
        usage.contains("integrity="),
        "usage snippet should contain integrity attribute"
    );
    assert!(
        usage.contains(sri_hash),
        "usage snippet should embed the SRI hash"
    );
}

#[tokio::test]
async fn test_bridge_js_has_corp_header() {
    let app = test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/bridge.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let corp = res
        .headers()
        .get("cross-origin-resource-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(
        corp, "cross-origin",
        "bridge.js should have CORP header set to cross-origin"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// WebAuthn / Passkey Tests
// ═══════════════════════════════════════════════════════════════════════════

/// Helper: create a test tenant with WebAuthn enabled and return (tenant_id, mgmt_key).
async fn create_webauthn_enabled_tenant(pool: &PgPool, slug: &str) -> (String, String) {
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(pool, slug, &format!("WebAuthn {}", slug)).await;

    // Enable WebAuthn + set plan to 'pro' (required for WebAuthn)
    sqlx::query(
        "UPDATE tenants SET security_config = '{\"webauthn_enabled\": true}'::jsonb WHERE id = $1",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("Failed to enable webauthn");

    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users) VALUES ($1, 'pro', 1000) ON CONFLICT (tenant_id) DO UPDATE SET plan = 'pro'",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("Failed to set pro plan");

    (tenant_id, mgmt_key)
}

#[tokio::test]
async fn test_webauthn_disabled_returns_403() {
    let (app, _pool) = test_app_with_pool().await;

    // Register a user (test-tenant does NOT have webauthn enabled)
    let (access_token, _, _) =
        register_user(&app, "webauthn-off@example.com", "Str0ngP@ss", Some("Test")).await;

    // Try register/begin — should get 403
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/webauthn/register/begin")
                .header("content-type", "application/json")
                .header("x-tenant-slug", TEST_TENANT_SLUG)
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::from(serde_json::to_vec(&serde_json::json!({})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "webauthn_disabled");
}

#[tokio::test]
async fn test_webauthn_register_begin_returns_options() {
    let (app, pool) = test_app_with_pool().await;

    let (_tenant_id, _mgmt_key) =
        create_webauthn_enabled_tenant(&pool, "wa-reg-begin").await;

    // Register a user on the webauthn-enabled tenant
    let res = app
        .clone()
        .oneshot(
            json_request_with_slug(
                "POST",
                "/auth/register",
                serde_json::json!({
                    "email": "alice-wa@example.com",
                    "password": "Str0ngP@ss",
                    "name": "Alice WA"
                }),
                "wa-reg-begin",
            ),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let reg_body = body_json(res).await;
    let access_token = reg_body["access_token"].as_str().unwrap();

    // Call register/begin
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/webauthn/register/begin")
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wa-reg-begin")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "name": "My YubiKey" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    // Verify the response structure
    assert!(body["challenge_id"].is_string(), "Should have challenge_id");
    let pk = &body["publicKey"];
    assert!(pk["challenge"].is_string(), "Should have challenge");
    assert!(pk["rp"]["name"].is_string(), "Should have rp.name");
    assert!(pk["rp"]["id"].is_string(), "Should have rp.id");
    assert_eq!(
        pk["rp"]["id"].as_str().unwrap(),
        "auth.gatewasm.test",
        "RP ID should be derived from server_url"
    );
    assert!(pk["user"]["id"].is_string(), "Should have user.id");
    assert_eq!(pk["user"]["name"], "alice-wa@example.com");
    assert!(
        pk["pubKeyCredParams"].is_array(),
        "Should have pubKeyCredParams"
    );
    assert!(
        pk["excludeCredentials"].is_array(),
        "Should have excludeCredentials"
    );

    // Verify the challenge was stored in DB
    let challenge_id = body["challenge_id"].as_str().unwrap();
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT ceremony_type FROM webauthn_challenges WHERE id = $1",
    )
    .bind(challenge_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(stored, Some("registration".to_string()));
}

#[tokio::test]
async fn test_webauthn_login_begin_requires_email() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, _mgmt_key) =
        create_webauthn_enabled_tenant(&pool, "wa-login-email").await;

    // Call login/begin without registering any credentials — should get no_credentials
    // First register a user
    let _res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "bob-wa@example.com",
                "password": "Str0ngP@ss",
            }),
            "wa-login-email",
        ))
        .await
        .unwrap();

    // Call login/begin — user exists but has no webauthn credentials
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/webauthn/login/begin")
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wa-login-email")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "email": "bob-wa@example.com" }))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = body_json(res).await;
    assert_eq!(body["error"], "no_credentials");
}

#[tokio::test]
async fn test_webauthn_login_begin_nonexistent_user_no_enumeration() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, _mgmt_key) =
        create_webauthn_enabled_tenant(&pool, "wa-login-noenum").await;

    // Call login/begin for a user that doesn't exist — should return 200
    // with a dummy challenge to prevent user enumeration
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/webauthn/login/begin")
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wa-login-noenum")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "email": "nonexistent@example.com" }))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    // Should still have a challenge (to prevent enumeration)
    assert!(body["publicKey"]["challenge"].is_string());
    // But allowCredentials should be empty
    assert_eq!(body["publicKey"]["allowCredentials"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_webauthn_credentials_crud() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, _mgmt_key) =
        create_webauthn_enabled_tenant(&pool, "wa-crud").await;

    // Register a user
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "carol-wa@example.com",
                "password": "Str0ngP@ss",
                "name": "Carol WA"
            }),
            "wa-crud",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let reg_body = body_json(res).await;
    let access_token = reg_body["access_token"].as_str().unwrap().to_string();
    let user_id = reg_body["user"]["id"].as_str().unwrap().to_string();

    // List credentials — should be empty
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/webauthn/credentials")
                .header("x-tenant-slug", "wa-crud")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["credentials"].as_array().unwrap().len(), 0);

    // Directly insert a fake credential into the DB for CRUD testing
    let cred_id = format!("wak_{}", uuid::Uuid::new_v4());
    let fake_credential_id = vec![1u8, 2, 3, 4, 5];
    let fake_public_key = vec![10u8, 20, 30, 40, 50];
    sqlx::query(
        "INSERT INTO webauthn_credentials (id, credential_id, public_key, sign_count, is_synced, transports, name, user_id, tenant_id) VALUES ($1, $2, $3, 0, FALSE, ARRAY['usb'], 'Test Key', $4, $5)",
    )
    .bind(&cred_id)
    .bind(&fake_credential_id)
    .bind(&fake_public_key)
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to insert fake credential");

    // List credentials — should have one
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/webauthn/credentials")
                .header("x-tenant-slug", "wa-crud")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let creds = body["credentials"].as_array().unwrap();
    assert_eq!(creds.len(), 1);
    assert_eq!(creds[0]["id"], cred_id);
    assert_eq!(creds[0]["name"], "Test Key");
    assert_eq!(creds[0]["is_synced"], false);
    assert_eq!(creds[0]["transports"][0], "usb");

    // Rename the credential
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/auth/webauthn/credentials/{}", cred_id))
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wa-crud")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "name": "My YubiKey 5" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["name"], "My YubiKey 5");

    // Verify rename persisted
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/webauthn/credentials")
                .header("x-tenant-slug", "wa-crud")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["credentials"][0]["name"], "My YubiKey 5");

    // Delete the credential
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/auth/webauthn/credentials/{}", cred_id))
                .header("x-tenant-slug", "wa-crud")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Verify deletion
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/webauthn/credentials")
                .header("x-tenant-slug", "wa-crud")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["credentials"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_webauthn_delete_nonexistent_credential_returns_404() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, _mgmt_key) =
        create_webauthn_enabled_tenant(&pool, "wa-del-404").await;

    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "del404-wa@example.com",
                "password": "Str0ngP@ss",
            }),
            "wa-del-404",
        ))
        .await
        .unwrap();
    let reg_body = body_json(res).await;
    let access_token = reg_body["access_token"].as_str().unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/auth/webauthn/credentials/wak_nonexistent")
                .header("x-tenant-slug", "wa-del-404")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = body_json(res).await;
    assert_eq!(body["error"], "credential_not_found");
}

#[tokio::test]
async fn test_webauthn_register_begin_unauthenticated_returns_401() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, _mgmt_key) =
        create_webauthn_enabled_tenant(&pool, "wa-noauth").await;

    // Call register/begin without JWT — should get 401
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/webauthn/register/begin")
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wa-noauth")
                .body(Body::from(serde_json::to_vec(&serde_json::json!({})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_webauthn_register_complete_invalid_challenge_returns_400() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, _mgmt_key) =
        create_webauthn_enabled_tenant(&pool, "wa-bad-chal").await;

    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "badchal@example.com",
                "password": "Str0ngP@ss",
            }),
            "wa-bad-chal",
        ))
        .await
        .unwrap();
    let reg_body = body_json(res).await;
    let access_token = reg_body["access_token"].as_str().unwrap();

    // Call register/complete with a bogus challenge_id
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/webauthn/register/complete")
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wa-bad-chal")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "challenge_id": "wc_nonexistent",
                        "credential_id": "AAAA",
                        "attestation_object": "AAAA",
                        "client_data_json": "AAAA",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_challenge");
}

#[tokio::test]
async fn test_webauthn_rename_credential_validates_name_length() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, _mgmt_key) =
        create_webauthn_enabled_tenant(&pool, "wa-rename-val").await;

    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "rename-val@example.com",
                "password": "Str0ngP@ss",
            }),
            "wa-rename-val",
        ))
        .await
        .unwrap();
    let reg_body = body_json(res).await;
    let access_token = reg_body["access_token"].as_str().unwrap().to_string();
    let user_id = reg_body["user"]["id"].as_str().unwrap().to_string();

    // Insert a credential
    let cred_id = format!("wak_{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO webauthn_credentials (id, credential_id, public_key, sign_count, is_synced, transports, name, user_id, tenant_id) VALUES ($1, $2, $3, 0, FALSE, '{}', 'Test Key', $4, $5)",
    )
    .bind(&cred_id)
    .bind(vec![99u8, 100])
    .bind(vec![10u8, 20])
    .bind(&user_id)
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    // Try to rename with an empty name
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/auth/webauthn/credentials/{}", cred_id))
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wa-rename-val")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "name": "  " })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_name");

    // Try to rename with a name that's too long (>100 chars)
    let long_name = "a".repeat(101);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/auth/webauthn/credentials/{}", cred_id))
                .header("content-type", "application/json")
                .header("x-tenant-slug", "wa-rename-val")
                .header("authorization", format!("Bearer {}", access_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({ "name": long_name })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ═══════════════════════════════════════════════════════════════════════════
// FedCM Identity Provider Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_fedcm_well_known() {
    let app = test_app().await;

    let res = app
        .oneshot(get_request("/.well-known/web-identity"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(res).await;
    let provider_urls = body["provider_urls"].as_array().unwrap();
    assert_eq!(provider_urls.len(), 1);
    assert!(
        provider_urls[0]
            .as_str()
            .unwrap()
            .ends_with("/fedcm/config.json"),
        "provider_urls should end with /fedcm/config.json, got: {}",
        provider_urls[0]
    );
}

#[tokio::test]
async fn test_fedcm_config() {
    let app = test_app().await;

    let res = app
        .oneshot(get_request("/fedcm/config.json"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(res).await;
    assert_eq!(body["accounts_endpoint"], "/fedcm/accounts");
    assert_eq!(body["id_assertion_endpoint"], "/fedcm/token");
    assert_eq!(body["login_url"], "/login");
    assert!(body["branding"]["background_color"].is_string());
    assert!(body["branding"]["color"].is_string());

    let icons = body["branding"]["icons"].as_array().unwrap();
    assert!(!icons.is_empty());
    assert!(icons[0]["url"].as_str().unwrap().contains("/favicon.ico"));
    assert_eq!(icons[0]["size"], 32);
}

#[tokio::test]
async fn test_fedcm_accounts_no_session() {
    let app = test_app().await;

    // Request without any cookies — should return empty accounts
    let res = app
        .oneshot(get_request("/fedcm/accounts"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(res).await;
    let accounts = body["accounts"].as_array().unwrap();
    assert!(
        accounts.is_empty(),
        "Expected empty accounts when no session cookie is present"
    );
}

#[tokio::test]
async fn test_fedcm_accounts_with_session() {
    let (app, pool) = test_app_with_pool().await;

    // Register a user (this creates a refresh token in the DB)
    let (_, refresh_token, user_id) =
        register_user(&app, "fedcm-user@example.com", "Str0ngP@ss", Some("Jane Doe")).await;

    // Now call /fedcm/accounts with the refresh token as a cookie.
    // The test server uses HTTPS URLs (auth.gatewasm.test), so the cookie name
    // follows __Host-gw_rt_test-tenant pattern, but since FedCM endpoints scan
    // for any gw_rt_* cookie, we can use the tenant-scoped cookie name directly.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/fedcm/accounts")
                .header("cookie", format!("gw_rt_test-tenant={}", refresh_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(res).await;
    let accounts = body["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0]["id"], user_id);
    assert_eq!(accounts[0]["email"], "fedcm-user@example.com");
    assert_eq!(accounts[0]["name"], "Jane Doe");
    assert_eq!(accounts[0]["given_name"], "Jane");
    assert!(accounts[0]["approved_clients"].as_array().unwrap().is_empty());

    // Now insert an approved client and verify it shows up
    sqlx::query(
        "INSERT INTO fedcm_approved_clients (id, user_id, tenant_id, client_id) VALUES ($1, $2, (SELECT tenant_id FROM refresh_tokens WHERE user_id = $2 LIMIT 1), $3)",
    )
    .bind(format!("fca_{}", uuid::Uuid::new_v4()))
    .bind(&user_id)
    .bind("https://example-rp.com")
    .execute(&pool)
    .await
    .unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/fedcm/accounts")
                .header("cookie", format!("gw_rt_test-tenant={}", refresh_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(res).await;
    let approved = body["accounts"][0]["approved_clients"]
        .as_array()
        .unwrap();
    assert_eq!(approved.len(), 1);
    assert_eq!(approved[0], "https://example-rp.com");
}

#[tokio::test]
async fn test_fedcm_token_no_session() {
    let app = test_app().await;

    // POST /fedcm/token without cookies — should fail
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/fedcm/token")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "account_id": "usr_fake",
                        "client_id": "https://example.com",
                        "nonce": "abc123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_fedcm_token_with_session() {
    let (app, pool) = test_app_with_pool().await;

    // Register a user
    let (_, refresh_token, user_id) =
        register_user(&app, "fedcm-token@example.com", "Str0ngP@ss", Some("Token User")).await;

    // POST /fedcm/token with a valid session cookie
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/fedcm/token")
                .header("content-type", "application/json")
                .header("cookie", format!("gw_rt_test-tenant={}", refresh_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "account_id": user_id,
                        "client_id": "https://example-rp.com",
                        "nonce": "nonce123"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(res).await;
    assert!(body["token"].is_string(), "Expected a token in the response");

    // Verify the client was recorded in fedcm_approved_clients
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fedcm_approved_clients WHERE user_id = $1 AND client_id = $2",
    )
    .bind(&user_id)
    .bind("https://example-rp.com")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "Client approval should be recorded");

    // Calling again should succeed (ON CONFLICT DO NOTHING) — no duplicate error
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/fedcm/token")
                .header("content-type", "application/json")
                .header("cookie", format!("gw_rt_test-tenant={}", refresh_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "account_id": user_id,
                        "client_id": "https://example-rp.com",
                        "nonce": "nonce456"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Count should still be 1 (idempotent)
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fedcm_approved_clients WHERE user_id = $1 AND client_id = $2",
    )
    .bind(&user_id)
    .bind("https://example-rp.com")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "Client approval should be idempotent");
}

#[tokio::test]
async fn test_fedcm_token_account_mismatch() {
    let (app, _pool) = test_app_with_pool().await;

    // Register a user
    let (_, refresh_token, _user_id) =
        register_user(&app, "fedcm-mismatch@example.com", "Str0ngP@ss", None).await;

    // POST /fedcm/token with a mismatched account_id
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/fedcm/token")
                .header("content-type", "application/json")
                .header("cookie", format!("gw_rt_test-tenant={}", refresh_token))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "account_id": "usr_wrong_user_id",
                        "client_id": "https://example-rp.com",
                        "nonce": "abc"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let body = body_json(res).await;
    assert_eq!(body["error"], "account_mismatch");
}

// ═══════════════════════════════════════════════════════════════════════════
// DPoP (Sender-Constrained Tokens) Tests
// ═══════════════════════════════════════════════════════════════════════════

/// Helper: create a tenant with dpop_required=true on enterprise plan.
async fn create_dpop_required_tenant(pool: &PgPool, slug: &str) -> (String, String) {
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(pool, slug, &format!("DPoP {}", slug)).await;

    sqlx::query(
        "UPDATE tenants SET security_config = '{\"dpop_required\": true}'::jsonb WHERE id = $1",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("Failed to enable dpop_required");

    sqlx::query(
        "INSERT INTO tenant_entitlements (tenant_id, plan, max_users) VALUES ($1, 'enterprise', 1000) ON CONFLICT (tenant_id) DO UPDATE SET plan = 'enterprise'",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("Failed to set enterprise plan");

    (tenant_id, mgmt_key)
}

/// Helper: generate an ES256 DPoP proof JWT using ring.
fn generate_dpop_proof(method: &str, uri: &str) -> String {
    use ring::rand::SystemRandom;
    use ring::signature::{self as ring_sig, EcdsaKeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};

    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
        .expect("Failed to generate ECDSA key pair");
    let key_pair =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .expect("Failed to parse ECDSA key pair");

    // Extract public key (uncompressed point: 0x04 || x || y)
    let pub_key_bytes = ring_sig::KeyPair::public_key(&key_pair).as_ref();
    assert_eq!(pub_key_bytes.len(), 65); // 1 + 32 + 32
    let x_bytes = &pub_key_bytes[1..33];
    let y_bytes = &pub_key_bytes[33..65];

    let x_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(x_bytes);
    let y_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(y_bytes);

    let jti = uuid::Uuid::new_v4().to_string();
    let iat = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let header = serde_json::json!({
        "typ": "dpop+jwt",
        "alg": "ES256",
        "jwk": {
            "kty": "EC",
            "crv": "P-256",
            "x": x_b64,
            "y": y_b64
        }
    });

    let payload = serde_json::json!({
        "jti": jti,
        "htm": method,
        "htu": uri,
        "iat": iat
    });

    let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&header).unwrap());
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&payload).unwrap());

    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let sig = key_pair
        .sign(&rng, signing_input.as_bytes())
        .expect("Failed to sign DPoP proof");
    let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sig.as_ref());

    format!("{}.{}.{}", header_b64, payload_b64, sig_b64)
}

#[tokio::test]
async fn test_dpop_required_rejects_without_proof() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "dpop-req-no-proof";
    let (_tenant_id, _mgmt_key) = create_dpop_required_tenant(&pool, slug).await;

    // Register a user on this tenant
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "dpop-user@example.com",
                "password": "Str0ngP@ss"
            }),
            slug,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let reg_body = body_json(res).await;
    let refresh_token = reg_body["refresh_token"].as_str().unwrap().to_string();

    // Attempt token refresh WITHOUT DPoP header -> should fail
    let res = app
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }),
            slug,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_dpop_proof");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("DPoP proof required"),
        "Expected 'DPoP proof required' message, got: {}",
        body["message"]
    );
}

#[tokio::test]
async fn test_dpop_optional_works_without_proof() {
    let (app, _pool) = test_app_with_pool().await;

    // Default tenant has dpop_required=false
    let (_, refresh_token, _) =
        register_user(&app, "dpop-optional@example.com", "Str0ngP@ss", None).await;

    let res = app
        .oneshot(json_request(
            "POST",
            "/auth/token",
            serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["token_type"], "Bearer");
    assert!(body["access_token"].is_string());
}

#[tokio::test]
async fn test_dpop_invalid_proof_rejected() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "dpop-invalid-proof";
    let (_tenant_id, _mgmt_key) = create_dpop_required_tenant(&pool, slug).await;

    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "dpop-invalid@example.com",
                "password": "Str0ngP@ss"
            }),
            slug,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let reg_body = body_json(res).await;
    let refresh_token = reg_body["refresh_token"].as_str().unwrap().to_string();

    // Send garbage in DPoP header
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", slug)
                .header("DPoP", "not.a.valid-jwt")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token",
                        "refresh_token": refresh_token,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_dpop_proof");
}

#[tokio::test]
async fn test_dpop_valid_proof_returns_dpop_token_type() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "dpop-valid-proof";
    let (_tenant_id, _mgmt_key) = create_dpop_required_tenant(&pool, slug).await;

    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "dpop-valid@example.com",
                "password": "Str0ngP@ss"
            }),
            slug,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let reg_body = body_json(res).await;
    let refresh_token = reg_body["refresh_token"].as_str().unwrap().to_string();

    // Generate a valid DPoP proof
    let dpop_proof =
        generate_dpop_proof("POST", "https://auth.gatewasm.test/auth/token");

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", slug)
                .header("DPoP", &dpop_proof)
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token",
                        "refresh_token": refresh_token,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["token_type"], "DPoP");
    assert!(body["access_token"].is_string());
}

#[tokio::test]
async fn test_dpop_replay_rejected() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "dpop-replay";
    let (_tenant_id, _mgmt_key) = create_dpop_required_tenant(&pool, slug).await;

    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "dpop-replay@example.com",
                "password": "Str0ngP@ss"
            }),
            slug,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let reg_body = body_json(res).await;
    let refresh1 = reg_body["refresh_token"].as_str().unwrap().to_string();

    // Use a valid proof
    let dpop_proof =
        generate_dpop_proof("POST", "https://auth.gatewasm.test/auth/token");

    // First request should succeed
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", slug)
                .header("DPoP", &dpop_proof)
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token",
                        "refresh_token": refresh1,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let refresh2 = body["refresh_token"].as_str().unwrap().to_string();

    // Second request with SAME proof (same jti) should fail
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .header("x-tenant-slug", slug)
                .header("DPoP", &dpop_proof)
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "grant_type": "refresh_token",
                        "refresh_token": refresh2,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "invalid_dpop_proof");
    assert!(
        body["message"].as_str().unwrap().contains("replay"),
        "Expected replay error, got: {}",
        body["message"]
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// PAR (Pushed Authorization Requests) Tests
// ═══════════════════════════════════════════════════════════════════════════

/// Helper: create a tenant with an oauth_client for PAR tests. Returns (tenant_id, mgmt_key, oauth_client_id).
async fn create_par_test_tenant(
    pool: &PgPool,
    slug: &str,
) -> (String, String, String) {
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(pool, slug, &format!("PAR {}", slug)).await;

    // Enable Google provider
    sqlx::query(
        "UPDATE tenants SET google_client_id = 'test-google-id', google_client_secret = 'test-google-secret', allowed_providers = '[\"email\",\"google\"]'::jsonb WHERE id = $1",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("Failed to set google creds");

    // Create an oauth_client for this tenant
    let oauth_client_id = format!("par-client-{}", slug);
    sqlx::query(
        "INSERT INTO oauth_clients (id, redirect_uri, name, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(&oauth_client_id)
    .bind("http://localhost:3099/callback")
    .bind("PAR Test Client")
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("Failed to create oauth_client");

    (tenant_id, mgmt_key, oauth_client_id)
}

/// Build a standard PAR form body.
fn par_form_body(client_id: &str) -> String {
    format!(
        "client_id={}&redirect_uri={}&response_type=code&scope=openid&state=par-state-123&code_challenge=abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234&code_challenge_method=S256&provider=google",
        urlencoding::encode(client_id),
        urlencoding::encode("http://localhost:3099/callback"),
    )
}

#[tokio::test]
async fn test_par_creates_request_uri() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "par-create";
    let (_tenant_id, _mgmt_key, oauth_client_id) = create_par_test_tenant(&pool, slug).await;

    let body = par_form_body(&oauth_client_id);
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/par")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("x-tenant-slug", slug)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let json = body_json(res).await;
    let request_uri = json["request_uri"].as_str().unwrap();
    assert!(
        request_uri.starts_with("urn:ietf:params:oauth:request_uri:"),
        "request_uri should have correct prefix, got: {}",
        request_uri
    );
    assert_eq!(json["expires_in"], 60);
}

#[tokio::test]
async fn test_par_invalid_client_id_rejected() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "par-bad-client";
    let (_tenant_id, _mgmt_key, _oauth_client_id) = create_par_test_tenant(&pool, slug).await;

    let body = par_form_body("nonexistent-client-id");
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/par")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("x-tenant-slug", slug)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = body_json(res).await;
    assert_eq!(json["error"], "invalid_client");
}

#[tokio::test]
async fn test_par_request_uri_authorize_flow() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "par-auth-flow";
    let (_tenant_id, _mgmt_key, oauth_client_id) = create_par_test_tenant(&pool, slug).await;

    // Step 1: Create PAR
    let body = par_form_body(&oauth_client_id);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/par")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("x-tenant-slug", slug)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let json = body_json(res).await;
    let request_uri = json["request_uri"].as_str().unwrap().to_string();

    // Step 2: Use request_uri at /auth/authorize
    let authorize_uri = format!(
        "/auth/authorize?client_id={}&request_uri={}",
        urlencoding::encode(&oauth_client_id),
        urlencoding::encode(&request_uri),
    );
    let res = app
        .oneshot(
            Request::builder()
                .uri(&authorize_uri)
                .header("x-tenant-slug", slug)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to Google OAuth
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = res
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        location.contains("accounts.google.com"),
        "Should redirect to Google, got: {}",
        location
    );
}

#[tokio::test]
async fn test_par_request_uri_single_use() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "par-single-use";
    let (_tenant_id, _mgmt_key, oauth_client_id) = create_par_test_tenant(&pool, slug).await;

    // Create PAR
    let body = par_form_body(&oauth_client_id);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/par")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("x-tenant-slug", slug)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let json = body_json(res).await;
    let request_uri = json["request_uri"].as_str().unwrap().to_string();

    let authorize_uri = format!(
        "/auth/authorize?client_id={}&request_uri={}",
        urlencoding::encode(&oauth_client_id),
        urlencoding::encode(&request_uri),
    );

    // First use — should succeed (302)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&authorize_uri)
                .header("x-tenant-slug", slug)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);

    // Second use — should fail (consumed)
    let res = app
        .oneshot(
            Request::builder()
                .uri(&authorize_uri)
                .header("x-tenant-slug", slug)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = body_json(res).await;
    assert_eq!(json["error"], "invalid_request");
    assert!(
        json["message"]
            .as_str()
            .unwrap()
            .contains("consumed"),
        "Expected consumed error, got: {}",
        json["message"]
    );
}

#[tokio::test]
async fn test_par_expired_request_uri_rejected() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "par-expired";
    let (_tenant_id, _mgmt_key, oauth_client_id) = create_par_test_tenant(&pool, slug).await;

    // Create PAR
    let body = par_form_body(&oauth_client_id);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/par")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("x-tenant-slug", slug)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let json = body_json(res).await;
    let request_uri = json["request_uri"].as_str().unwrap().to_string();

    // Manually expire it in the DB
    sqlx::query("UPDATE par_requests SET expires_at = NOW() - INTERVAL '1 minute' WHERE request_uri = $1")
        .bind(&request_uri)
        .execute(&pool)
        .await
        .expect("Failed to expire PAR request");

    // Try to use — should fail
    let authorize_uri = format!(
        "/auth/authorize?client_id={}&request_uri={}",
        urlencoding::encode(&oauth_client_id),
        urlencoding::encode(&request_uri),
    );
    let res = app
        .oneshot(
            Request::builder()
                .uri(&authorize_uri)
                .header("x-tenant-slug", slug)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = body_json(res).await;
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn test_par_required_rejects_direct_authorize() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "par-required";
    let (tenant_id, _mgmt_key, oauth_client_id) = create_par_test_tenant(&pool, slug).await;

    // Enable par_required on this tenant's security config
    sqlx::query(
        "UPDATE tenants SET security_config = '{\"par_required\": true}'::jsonb WHERE id = $1",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to enable par_required");

    // Invalidate tenant cache so the new config is picked up
    // (Not needed since each test gets its own schema, but explicit)

    // Direct /auth/authorize without request_uri — should be rejected
    let authorize_uri = format!(
        "/auth/authorize?client_id={}&redirect_uri={}&response_type=code&state=abc&code_challenge=abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234&code_challenge_method=S256&provider=google",
        urlencoding::encode(&oauth_client_id),
        urlencoding::encode("http://localhost:3099/callback"),
    );
    let res = app
        .oneshot(
            Request::builder()
                .uri(&authorize_uri)
                .header("x-tenant-slug", slug)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = body_json(res).await;
    assert_eq!(json["error"], "invalid_request");
    assert!(
        json["message"]
            .as_str()
            .unwrap()
            .contains("pushed authorization requests"),
        "Expected PAR required message, got: {}",
        json["message"]
    );
}

#[tokio::test]
async fn test_par_client_id_mismatch_rejected() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "par-mismatch";
    let (_tenant_id, _mgmt_key, oauth_client_id) = create_par_test_tenant(&pool, slug).await;

    // Create a second oauth_client
    sqlx::query(
        "INSERT INTO oauth_clients (id, redirect_uri, name, tenant_id) VALUES ($1, $2, $3, (SELECT id FROM tenants WHERE slug = $4))",
    )
    .bind("par-client-other")
    .bind("http://localhost:3099/other")
    .bind("Other Client")
    .bind(slug)
    .execute(&pool)
    .await
    .expect("Failed to create second oauth_client");

    // Create PAR with original client
    let body = par_form_body(&oauth_client_id);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/par")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("x-tenant-slug", slug)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let json = body_json(res).await;
    let request_uri = json["request_uri"].as_str().unwrap().to_string();

    // Try to use request_uri with a different client_id
    let authorize_uri = format!(
        "/auth/authorize?client_id=par-client-other&request_uri={}",
        urlencoding::encode(&request_uri),
    );
    let res = app
        .oneshot(
            Request::builder()
                .uri(&authorize_uri)
                .header("x-tenant-slug", slug)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = body_json(res).await;
    assert_eq!(json["error"], "invalid_request");
    assert!(
        json["message"]
            .as_str()
            .unwrap()
            .contains("client_id"),
        "Expected client_id mismatch error, got: {}",
        json["message"]
    );
}

// ── mTLS Certificate Management ─────────────────────────────────────────

#[tokio::test]
async fn mtls_register_certificate() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "mtls-reg", "mTLS Register").await;

    // Enable mTLS for the tenant
    sqlx::query("UPDATE tenants SET security_config = '{\"mtls_enabled\": true}'::jsonb, plan = 'enterprise' WHERE id = $1")
        .bind(&tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/certificates",
            serde_json::json!({
                "fingerprint": "sha256:a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
                "client_id": "my-service",
                "name": "Production Service Cert",
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert!(body["certificate"]["id"].as_str().unwrap().starts_with("cert_"));
    assert_eq!(
        body["certificate"]["fingerprint"],
        "sha256:a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
    );
    assert_eq!(body["certificate"]["client_id"], "my-service");
    assert_eq!(body["certificate"]["name"], "Production Service Cert");
}

#[tokio::test]
async fn mtls_list_certificates() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "mtls-list", "mTLS List").await;

    // Enable mTLS
    sqlx::query("UPDATE tenants SET security_config = '{\"mtls_enabled\": true}'::jsonb, plan = 'enterprise' WHERE id = $1")
        .bind(&tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    // Register a certificate
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/certificates",
            serde_json::json!({
                "fingerprint": "sha256:aaaa1111bbbb2222cccc3333dddd4444eeee5555ffff6666aaaa1111bbbb2222",
                "client_id": "svc-alpha",
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // List certificates
    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/certificates", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let certs = body["certificates"].as_array().unwrap();
    assert_eq!(certs.len(), 1);
    assert_eq!(
        certs[0]["fingerprint"],
        "sha256:aaaa1111bbbb2222cccc3333dddd4444eeee5555ffff6666aaaa1111bbbb2222"
    );
    assert_eq!(certs[0]["client_id"], "svc-alpha");
}

#[tokio::test]
async fn mtls_revoke_certificate() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "mtls-revoke", "mTLS Revoke").await;

    // Enable mTLS
    sqlx::query("UPDATE tenants SET security_config = '{\"mtls_enabled\": true}'::jsonb, plan = 'enterprise' WHERE id = $1")
        .bind(&tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    let fp = "sha256:deadbeef1234567890abcdef1234567890abcdef1234567890abcdef12345678";

    // Register
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/certificates",
            serde_json::json!({
                "fingerprint": fp,
                "client_id": "svc-beta",
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Revoke via DELETE
    let encoded_fp = urlencoding::encode(fp);
    let res = app
        .clone()
        .oneshot(mgmt_delete(
            &format!("/manage/certificates/{}", encoded_fp),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["message"], "Certificate revoked");

    // List should now be empty (revoked certs are excluded)
    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/certificates", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let certs = body["certificates"].as_array().unwrap();
    assert_eq!(certs.len(), 0);

    // Verify the record still exists in DB with revoked_at set
    let row = sqlx::query("SELECT revoked_at FROM client_certificates WHERE tenant_id = $1 AND fingerprint = $2")
        .bind(&tenant_id)
        .bind(fp)
        .fetch_one(&pool)
        .await
        .unwrap();
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::Row::get(&row, "revoked_at");
    assert!(revoked_at.is_some(), "revoked_at should be set after revocation");
}

#[tokio::test]
async fn mtls_disabled_returns_403() {
    let (app, pool) = test_app_with_pool().await;
    let (_, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "mtls-off", "mTLS Disabled").await;

    // mTLS is NOT enabled (default) — all certificate operations should return 403

    // POST /manage/certificates
    let res = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/certificates",
            serde_json::json!({
                "fingerprint": "sha256:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
                "client_id": "svc-blocked",
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "mtls_not_enabled");

    // GET /manage/certificates
    let res = app
        .clone()
        .oneshot(mgmt_get("/manage/certificates", &mgmt_key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // DELETE /manage/certificates/{fp}
    let res = app
        .clone()
        .oneshot(mgmt_delete(
            &format!(
                "/manage/certificates/{}",
                urlencoding::encode("sha256:abcdef")
            ),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// ═══════════════════════════════════════════════════════════════════════════
// Token Introspection (RFC 7662)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_token_introspection_active() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "introspect-active", "Introspect Active").await;

    // Enable introspection + enterprise plan
    sqlx::query(
        "UPDATE tenants SET security_config = '{\"introspection_enabled\": true}'::jsonb, plan = 'enterprise' WHERE id = $1",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    // Register a user on that tenant
    let res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "alice@introspect.test",
                "password": "StrongP@ss1!"
            }),
            "introspect-active",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let reg_body = body_json(res).await;
    let access_token = reg_body["access_token"].as_str().unwrap();

    // Introspect the token using the management key
    let form_body = format!("token={}", access_token);
    let req = Request::builder()
        .method("POST")
        .uri("/oauth/introspect")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(form_body))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    assert_eq!(body["active"], true);
    assert_eq!(body["sub"], reg_body["user"]["id"]);
    assert!(body["exp"].is_number());
    assert!(body["iat"].is_number());
    assert_eq!(body["iss"], "test");
    assert_eq!(body["aud"], tenant_id);
    assert_eq!(body["scope"], "openid profile email");
    assert_eq!(body["token_type"], "Bearer");
}

#[tokio::test]
async fn test_token_introspection_expired() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "introspect-expired", "Introspect Expired").await;

    // Enable introspection + enterprise plan
    sqlx::query(
        "UPDATE tenants SET security_config = '{\"introspection_enabled\": true}'::jsonb, plan = 'enterprise' WHERE id = $1",
    )
    .bind(&tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    // Send a bogus/expired token
    let form_body = "token=invalid.jwt.token";
    let req = Request::builder()
        .method("POST")
        .uri("/oauth/introspect")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(form_body))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    assert_eq!(body["active"], false);
    // Must NOT reveal why the token is inactive
    assert!(body.get("sub").is_none());
    assert!(body.get("error").is_none());
}

#[tokio::test]
async fn test_token_introspection_disabled() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "introspect-disabled", "Introspect Disabled")
            .await;

    // Do NOT enable introspection (default is false)

    let form_body = "token=some.fake.token";
    let req = Request::builder()
        .method("POST")
        .uri("/oauth/introspect")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(form_body))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "introspection_not_enabled");
}

#[tokio::test]
async fn test_token_introspection_no_auth() {
    let app = test_app().await;

    // No Authorization header at all
    let form_body = "token=some.fake.token";
    let req = Request::builder()
        .method("POST")
        .uri("/oauth/introspect")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(form_body))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── SSF Stream Management Tests ─────────────────────────────────────────

/// Helper: create a tenant with caep_enabled + compliance plan for SSF tests.
async fn create_ssf_test_tenant(pool: &PgPool, slug: &str) -> (String, String) {
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(pool, slug, &format!("SSF {}", slug)).await;

    sqlx::query(
        "UPDATE tenants SET security_config = '{\"caep_enabled\": true}'::jsonb, plan = 'compliance' WHERE id = $1",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("Failed to enable caep for SSF test tenant");

    (tenant_id, mgmt_key)
}

#[tokio::test]
async fn test_ssf_create_stream() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-create").await;

    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked", "credential-change"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body = body_json(res).await;
    let stream = &body["stream"];

    // Should have an ID (UUID format)
    assert!(stream["id"].as_str().unwrap().len() > 10);
    // Secret should be visible (shown once) and start with ssf_
    let secret = stream["shared_secret"].as_str().unwrap();
    assert!(secret.starts_with("ssf_"), "Secret should start with ssf_");
    assert!(secret.len() > 10, "Secret should be a full secret, not masked");
    // Check other fields
    assert_eq!(stream["delivery_method"], "push");
    assert_eq!(stream["status"], "enabled");
    assert_eq!(stream["receiver_url"], "https://example.com/ssf/events");
    assert_eq!(stream["consecutive_failures"], 0);
}

#[tokio::test]
async fn test_ssf_list_streams() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-list").await;

    // Create a stream first
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Now list
    let req = Request::builder()
        .method("GET")
        .uri("/manage/ssf/streams")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_json(res).await;
    let streams = body["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 1);

    // Secret should be masked in list response
    let masked = streams[0]["shared_secret"].as_str().unwrap();
    assert!(masked.ends_with("..."), "Secret should be masked in list");
    assert!(masked.len() < 20, "Masked secret should be short");
}

#[tokio::test]
async fn test_ssf_stream_crud() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-crud").await;

    // 1. Create
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let stream_id = body["stream"]["id"].as_str().unwrap().to_string();

    // 2. Get
    let req = Request::builder()
        .method("GET")
        .uri(format!("/manage/ssf/streams/{}", stream_id))
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["stream"]["status"], "enabled");
    assert_eq!(body["stream"]["has_previous_secret"], false);

    // 3. Patch — update status to paused
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/manage/ssf/streams/{}", stream_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "status": "paused",
                "event_types": ["session-revoked", "credential-change"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify the update took effect
    let req = Request::builder()
        .method("GET")
        .uri(format!("/manage/ssf/streams/{}", stream_id))
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["stream"]["status"], "paused");
    let event_types = body["stream"]["event_types"].as_array().unwrap();
    assert_eq!(event_types.len(), 2);

    // 4. Delete
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/manage/ssf/streams/{}", stream_id))
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify deleted — GET should 404
    let req = Request::builder()
        .method("GET")
        .uri(format!("/manage/ssf/streams/{}", stream_id))
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_ssf_rotate_secret() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-rotate").await;

    // Create a stream
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let stream_id = body["stream"]["id"].as_str().unwrap().to_string();
    let original_secret = body["stream"]["shared_secret"].as_str().unwrap().to_string();

    // Rotate secret
    let req = Request::builder()
        .method("POST")
        .uri(format!("/manage/ssf/streams/{}/rotate-secret", stream_id))
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;

    // New secret should be different and start with ssf_
    let new_secret = body["shared_secret"].as_str().unwrap();
    assert!(new_secret.starts_with("ssf_"));
    assert_ne!(new_secret, original_secret);

    // Previous secret should be masked
    let prev_masked = body["previous_shared_secret"].as_str().unwrap();
    assert!(prev_masked.ends_with("..."));

    // Grace period should be 48h
    assert_eq!(body["grace_period_hours"], 48);

    // Verify via GET that has_previous_secret is now true
    let req = Request::builder()
        .method("GET")
        .uri(format!("/manage/ssf/streams/{}", stream_id))
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["stream"]["has_previous_secret"], true);
    assert!(body["stream"]["secret_rotated_at"].as_str().is_some());
}

#[tokio::test]
async fn test_ssf_disabled_returns_403() {
    let (app, pool) = test_app_with_pool().await;
    // Create a tenant WITHOUT caep_enabled (default is false)
    let (_tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "ssf-disabled", "SSF Disabled").await;

    // Try to create a stream — should get 403
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["error"], "caep_not_enabled");

    // Try to list streams — should also get 403
    let req = Request::builder()
        .method("GET")
        .uri("/manage/ssf/streams")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_ssf_verify_stream() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-verify").await;

    // Create a stream
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let stream_id = body["stream"]["id"].as_str().unwrap().to_string();

    // Send verification event
    let req = Request::builder()
        .method("POST")
        .uri(format!("/manage/ssf/streams/{}/verify", stream_id))
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["message"], "Verification event queued");
    assert!(body["sequence_number"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_ssf_create_poll_stream() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-poll").await;

    // Create a poll stream (no receiver_url needed)
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "delivery_method": "poll",
                "event_types": ["session-revoked", "credential-change"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["stream"]["delivery_method"], "poll");
    assert!(body["stream"]["receiver_url"].is_null());
}

#[tokio::test]
async fn test_ssf_create_push_requires_receiver_url() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-no-url").await;

    // Try to create a push stream without receiver_url
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "missing_receiver_url");
}

// ═══════════════════════════════════════════════════════════════════════════
// SSF Event Engine Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_ssf_event_written_to_outbox() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "ssf-engine-outbox";
    let (tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, slug).await;

    // Create an SSF stream subscribing to session-revoked
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Register a user under this tenant
    let reg_res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "ssf-outbox-user@test.com",
                "password": "Str0ngP@ss!123",
            }),
            slug,
        ))
        .await
        .unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);
    let reg_body = body_json(reg_res).await;
    let access_token = reg_body["access_token"].as_str().unwrap().to_string();
    let refresh_token = reg_body["refresh_token"].as_str().unwrap().to_string();

    // Logout (this should emit an SSF event)
    let logout_req = Request::builder()
        .method("POST")
        .uri("/auth/logout")
        .header("content-type", "application/json")
        .header("x-tenant-slug", slug)
        .header("authorization", format!("Bearer {}", access_token))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "refresh_token": refresh_token,
            }))
            .unwrap(),
        ))
        .unwrap();

    let logout_res = app.clone().oneshot(logout_req).await.unwrap();
    assert_eq!(logout_res.status(), StatusCode::NO_CONTENT);

    // Check outbox for the session-revoked event
    let outbox_rows = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT id, event_type, set_token, status FROM ssf_outbox \
         WHERE tenant_id = $1 AND event_type = 'session-revoked' \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(&tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(outbox_rows.len(), 1, "Expected 1 SSF outbox entry for session-revoked");
    let (outbox_id, event_type, set_token, status) = &outbox_rows[0];
    assert_eq!(event_type, "session-revoked");
    assert_eq!(status, "pending");
    assert!(!set_token.is_empty(), "SET token should not be empty");
    assert!(*outbox_id > 0);
}

#[tokio::test]
async fn test_ssf_outbox_event_has_valid_set_token() {
    let (app, pool) = test_app_with_pool().await;
    let slug = "ssf-engine-set";
    let (tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, slug).await;

    // Create an SSF stream subscribing to session-revoked
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Register a user under this tenant
    let reg_res = app
        .clone()
        .oneshot(json_request_with_slug(
            "POST",
            "/auth/register",
            serde_json::json!({
                "email": "ssf-set-user@test.com",
                "password": "Str0ngP@ss!123",
            }),
            slug,
        ))
        .await
        .unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);
    let reg_body = body_json(reg_res).await;
    let access_token = reg_body["access_token"].as_str().unwrap().to_string();
    let refresh_token = reg_body["refresh_token"].as_str().unwrap().to_string();
    let user_id = reg_body["user"]["id"].as_str().unwrap().to_string();

    // Logout
    let logout_req = Request::builder()
        .method("POST")
        .uri("/auth/logout")
        .header("content-type", "application/json")
        .header("x-tenant-slug", slug)
        .header("authorization", format!("Bearer {}", access_token))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "refresh_token": refresh_token,
            }))
            .unwrap(),
        ))
        .unwrap();

    let logout_res = app.clone().oneshot(logout_req).await.unwrap();
    assert_eq!(logout_res.status(), StatusCode::NO_CONTENT);

    // Fetch the SET token from outbox
    let set_token: String = sqlx::query_scalar(
        "SELECT set_token FROM ssf_outbox \
         WHERE tenant_id = $1 AND event_type = 'session-revoked' \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(&tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // The SET token is a JWT — it has 3 dot-separated parts
    let parts: Vec<&str> = set_token.split('.').collect();
    assert_eq!(parts.len(), 3, "SET token should be a JWT with 3 parts");

    // Decode the header (base64url)
    let header_json: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[0])
            .expect("Invalid base64 in SET header"),
    )
    .expect("Invalid JSON in SET header");
    assert_eq!(header_json["alg"], "EdDSA");
    assert_eq!(header_json["typ"], "secevent+jwt");
    assert!(header_json["kid"].is_string());

    // Decode the payload
    let payload_json: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("Invalid base64 in SET payload"),
    )
    .expect("Invalid JSON in SET payload");

    // Verify claims
    assert_eq!(payload_json["iss"], tenant_id);
    assert!(payload_json["iat"].is_number());
    assert!(payload_json["jti"].is_string());
    assert!(payload_json["seq_num"].is_number());

    // Verify events structure
    let events = &payload_json["events"];
    let event_key = "urn:ietf:params:ssf:event-type:session-revoked";
    assert!(events[event_key].is_object(), "events should contain session-revoked URI key");
    assert_eq!(events[event_key]["subject"]["user_id"], user_id);
    assert_eq!(events[event_key]["subject"]["email"], "ssf-set-user@test.com");
}

// ── SSF Poll Endpoint Tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_ssf_poll_and_ack() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-poll-ack").await;

    // Create a poll stream
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "delivery_method": "poll",
                "event_types": ["session-revoked", "credential-change"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let stream_id = body["stream"]["id"].as_str().unwrap().to_string();
    let stream_uuid: uuid::Uuid = stream_id.parse().unwrap();

    // Insert 3 outbox events directly with known sequence numbers
    for seq in 1..=3i64 {
        let set_token = format!("eyJ0eXAiOiJzZWNldmVudCtqd3QifQ.event_payload_{}", seq);
        sqlx::query(
            "INSERT INTO ssf_outbox (tenant_id, stream_id, sequence_number, event_type, set_token, status) \
             VALUES ($1, $2, $3, $4, $5, 'pending')",
        )
        .bind(&tenant_id)
        .bind(stream_uuid)
        .bind(seq)
        .bind("session-revoked")
        .bind(&set_token)
        .execute(&pool)
        .await
        .expect("Failed to insert outbox event");
    }

    // Poll with ack=0 — should get all 3 events
    let req = Request::builder()
        .method("POST")
        .uri(format!("/manage/ssf/streams/{}/poll", stream_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "ack": 0,
                "max_events": 100
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let sets = body["sets"].as_array().unwrap();
    assert_eq!(sets.len(), 3, "Expected 3 events with ack=0");
    assert_eq!(body["more_available"], false);

    // Verify ordering — seq 1 first, seq 3 last
    assert!(sets[0].as_str().unwrap().contains("event_payload_1"));
    assert!(sets[2].as_str().unwrap().contains("event_payload_3"));

    // Poll with ack=1 — should get events 2 and 3
    let req = Request::builder()
        .method("POST")
        .uri(format!("/manage/ssf/streams/{}/poll", stream_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "ack": 1,
                "max_events": 100
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let sets = body["sets"].as_array().unwrap();
    assert_eq!(sets.len(), 2, "Expected 2 events after ack=1");
    assert!(sets[0].as_str().unwrap().contains("event_payload_2"));

    // Poll again without advancing ack — should still get 2 and 3 (ack is persisted)
    let req = Request::builder()
        .method("POST")
        .uri(format!("/manage/ssf/streams/{}/poll", stream_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "ack": 0,
                "max_events": 100
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let sets = body["sets"].as_array().unwrap();
    assert_eq!(sets.len(), 2, "ack=0 should not regress; last_ack_sequence=1 persisted");

    // Poll with ack=3 — should get 0 events (all acknowledged)
    let req = Request::builder()
        .method("POST")
        .uri(format!("/manage/ssf/streams/{}/poll", stream_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "ack": 3,
                "max_events": 100
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let sets = body["sets"].as_array().unwrap();
    assert_eq!(sets.len(), 0, "Expected 0 events after ack=3");
    assert_eq!(body["more_available"], false);

    // Verify last_ack_sequence was updated in DB
    let last_ack: i64 = sqlx::query_scalar(
        "SELECT last_ack_sequence FROM ssf_streams WHERE id = $1",
    )
    .bind(stream_uuid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(last_ack, 3, "last_ack_sequence should be 3");
}

#[tokio::test]
async fn test_ssf_poll_push_stream_rejected() {
    let (app, pool) = test_app_with_pool().await;
    let (_tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-poll-push").await;

    // Create a push stream
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/events",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    let stream_id = body["stream"]["id"].as_str().unwrap().to_string();

    // Try to poll a push stream — should get 400
    let req = Request::builder()
        .method("POST")
        .uri(format!("/manage/ssf/streams/{}/poll", stream_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "ack": 0,
                "max_events": 100
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["error"], "not_poll_stream");
}

// ── Impossible Travel Detection ─────────────────────────────────────────

#[tokio::test]
async fn test_impossible_travel_stores_login_location() {
    let (app, pool) = test_app_with_pool().await;

    // Register a user
    let reg_res = app
        .clone()
        .oneshot(slug_json_request(
            "POST",
            TEST_TENANT_SLUG,
            "/auth/register",
            serde_json::json!({
                "email": "travel-test@test.com",
                "password": "SecurePass123!",
                "name": "Travel Test User",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);
    let reg_body = body_json(reg_res).await;
    let user_id = reg_body["user"]["id"].as_str().unwrap().to_string();

    // Login — the handler will call classify_ip and geolocate_ip on the request IP.
    // Since test requests come from 127.0.0.1 (or 0.0.0.0 in X-Forwarded-For fallback),
    // geolocate_ip will return None. But login_ip and login_ip_type should still be stored.
    let login_res = app
        .clone()
        .oneshot(slug_json_request(
            "POST",
            TEST_TENANT_SLUG,
            "/auth/login",
            serde_json::json!({
                "email": "travel-test@test.com",
                "password": "SecurePass123!",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(login_res.status(), StatusCode::OK);

    // Verify the refresh token row has login_ip and login_ip_type set.
    // The login_lat/login_lon will be NULL because the stub geolocate_ip doesn't know the test IP.
    let row = sqlx::query_as::<_, (Option<String>, Option<String>, Option<f64>, Option<f64>)>(
        "SELECT login_ip, login_ip_type, login_lat, login_lon \
         FROM refresh_tokens WHERE user_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&user_id)
    .fetch_one(&pool)
    .await
    .expect("Should find refresh token with login location columns");

    let (login_ip, login_ip_type, login_lat, login_lon) = row;

    // login_ip should be set (the extracted client IP — defaults to "0.0.0.0" in tests)
    assert!(login_ip.is_some(), "login_ip should be stored on refresh token");
    let ip = login_ip.unwrap();
    assert!(!ip.is_empty(), "login_ip should not be empty");

    // login_ip_type should be "residential" (default from classify_ip stub)
    assert_eq!(
        login_ip_type.as_deref(),
        Some("residential"),
        "login_ip_type should be 'residential' from stub"
    );

    // login_lat/lon will be None for unknown test IPs — verify they are NULL
    assert!(login_lat.is_none(), "login_lat should be NULL for unknown test IP");
    assert!(login_lon.is_none(), "login_lon should be NULL for unknown test IP");
}

#[tokio::test]
async fn test_impossible_travel_manual_location_query() {
    // This test manually inserts a previous login location and verifies the
    // impossible travel detection logic stores the current location fields.
    let (app, pool) = test_app_with_pool().await;

    // Register + login to create a user with a refresh token
    let reg_res = app
        .clone()
        .oneshot(slug_json_request(
            "POST",
            TEST_TENANT_SLUG,
            "/auth/register",
            serde_json::json!({
                "email": "travel-manual@test.com",
                "password": "SecurePass123!",
                "name": "Travel Manual User",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);
    let reg_body = body_json(reg_res).await;
    let user_id = reg_body["user"]["id"].as_str().unwrap().to_string();

    // Manually update the existing refresh token to simulate a previous login from NYC
    sqlx::query(
        "UPDATE refresh_tokens SET login_lat = 40.7128, login_lon = -74.0060, \
         login_ip = '203.0.113.1', login_ip_type = 'residential' \
         WHERE user_id = $1",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .expect("Should update refresh token with NYC location");

    // Login again — new refresh token will be created
    let login_res = app
        .clone()
        .oneshot(slug_json_request(
            "POST",
            TEST_TENANT_SLUG,
            "/auth/login",
            serde_json::json!({
                "email": "travel-manual@test.com",
                "password": "SecurePass123!",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(login_res.status(), StatusCode::OK);

    // Verify new refresh token has login_ip set but lat/lon NULL (unknown test IP)
    let rows = sqlx::query_as::<_, (Option<String>, Option<String>, Option<f64>, Option<f64>)>(
        "SELECT login_ip, login_ip_type, login_lat, login_lon \
         FROM refresh_tokens WHERE user_id = $1 \
         ORDER BY created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&pool)
    .await
    .expect("Should find refresh tokens");

    // Should have at least 2 refresh tokens (register + login)
    assert!(rows.len() >= 2, "Should have at least 2 refresh tokens");

    // Most recent token (from login) should have login_ip set
    let (newest_ip, newest_ip_type, _newest_lat, _newest_lon) = &rows[0];
    assert!(newest_ip.is_some(), "Newest refresh token should have login_ip");
    assert_eq!(
        newest_ip_type.as_deref(),
        Some("residential"),
        "Newest refresh token should have login_ip_type"
    );

    // Previous token (from register, manually updated) should have NYC coordinates
    let (_prev_ip, _prev_ip_type, prev_lat, prev_lon) = &rows[1];
    assert!(
        (prev_lat.unwrap_or(0.0) - 40.7128).abs() < 0.01,
        "Previous token should have NYC latitude"
    );
    assert!(
        (prev_lon.unwrap_or(0.0) - (-74.0060)).abs() < 0.01,
        "Previous token should have NYC longitude"
    );
}

// ── SSF Audit Logging Tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_ssf_audit_logging() {
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) = create_ssf_test_tenant(&pool, "ssf-audit").await;

    // Create a stream — should generate ssf.stream.created audit entry
    let req = Request::builder()
        .method("POST")
        .uri("/manage/ssf/streams")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "receiver_url": "https://example.com/ssf/audit-test",
                "delivery_method": "push",
                "event_types": ["session-revoked"]
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body = body_json(res).await;
    let stream_id = body["stream"]["id"].as_str().unwrap().to_string();

    // Verify ssf.stream.created audit log entry exists
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE tenant_id = $1 AND action = 'ssf.stream.created'",
    )
    .bind(&tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Should query audit_log");

    assert_eq!(
        audit_count, 1,
        "Should have exactly 1 ssf.stream.created audit entry"
    );

    // Verify the resource field contains the stream ID
    let resource: String = sqlx::query_scalar(
        "SELECT resource FROM audit_log WHERE tenant_id = $1 AND action = 'ssf.stream.created'",
    )
    .bind(&tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Should query audit_log resource");

    assert!(
        resource.contains(&stream_id),
        "Audit resource should contain stream ID, got: {}",
        resource
    );

    // Update the stream status to paused then back to enabled — should generate ssf.stream.resumed
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/manage/ssf/streams/{}", stream_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "status": "paused"
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/manage/ssf/streams/{}", stream_id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", mgmt_key))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "status": "enabled"
            }))
            .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify ssf.stream.resumed audit log entry exists
    let resumed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE tenant_id = $1 AND action = 'ssf.stream.resumed'",
    )
    .bind(&tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Should query audit_log");

    assert_eq!(
        resumed_count, 1,
        "Should have exactly 1 ssf.stream.resumed audit entry"
    );
}

// In debug/test builds the asset handler reads from the filesystem `admin/dist/`
// directory (if it exists) and returns `no-cache` headers for hot-reload convenience.
// The immutable-cache behavior only applies to the embedded release build.
// Skip this test in debug builds where filesystem assets take precedence.
#[tokio::test]
#[cfg_attr(debug_assertions, ignore = "filesystem assets use no-cache headers in debug builds")]
async fn test_asset_route_serves_with_immutable_headers() {
    let (app, _pool) = test_app_with_pool().await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/_assets/manifest.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("cache-control").unwrap(),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        resp.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    let vary = resp.headers().get("vary").unwrap().to_str().unwrap();
    assert!(
        vary.eq_ignore_ascii_case("Accept-Encoding"),
        "vary header should be Accept-Encoding (case-insensitive), got: {vary}"
    );
}

#[tokio::test]
async fn test_asset_route_returns_404_for_missing() {
    let (app, _pool) = test_app_with_pool().await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/_assets/does-not-exist.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── Security regressions ────────────────────────────────────────────────
//
// These cover defects that were live in the tree: an unauthenticated route
// that enumerated every tenant, and a `<style>` escape that matched only two
// literal spellings of the closing tag.

#[tokio::test]
async fn dev_clear_cache_route_is_gone() {
    // The handler ran `SELECT slug FROM tenants` and emitted one Set-Cookie
    // per tenant, disclosing the full tenant list to any anonymous caller and
    // force-logging-out every session. It shipped in the release binary with
    // no auth layer and no debug_assertions guard.
    let app = test_app().await;
    let res = app
        .oneshot(get_request("/dev/clear-cache"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn hosted_login_drops_branding_that_would_escape_the_style_block() {
    // Written straight to the column, bypassing the write-side validator, so
    // this covers rows that predate it and any future write path that forgets
    // to validate.
    //
    // This asserts the *render-side* filtering: the values never reach the
    // page. It deliberately does not double as the escaper's test — with the
    // filter in place nothing hostile gets that far, so it cannot fail if the
    // escaper regresses. `css_text_safe_blocks_every_end_tag_spelling` in
    // html_template.rs covers the escaper itself, and does fail without it.
    let (app, pool) = test_app_with_pool().await;
    sqlx::query(
        "UPDATE tenants SET branding_bg_color = $1, branding_card_color = $2, \
         branding_custom_css = $3 WHERE slug = $4",
    )
    .bind("#000}</StYlE ><h1 id=pwned>phish</h1><style>")
    .bind("#111}</style\n><img src=x>")
    .bind("color:red}</STYLE><form action=https://evil.example>")
    .bind(TEST_TENANT_SLUG)
    .execute(&pool)
    .await
    .unwrap();

    let res = app
        .oneshot(get_request(&format!("/t/{}/login", TEST_TENANT_SLUG)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;

    // The page emits exactly one <style> element, so exactly one closing tag.
    assert_eq!(
        body.to_lowercase().matches("</style").count(),
        1,
        "branding closed the <style> element"
    );
    assert!(!body.contains("id=pwned"), "injected markup reached the page");
    assert!(
        !body.contains("evil.example"),
        "injected form action reached the page"
    );
}

#[tokio::test]
async fn hosted_login_survives_non_ascii_branding_css() {
    // `sanitize_custom_css` aborted the process on any non-ASCII input, so a
    // tenant storing an accented value took the platform down on render.
    let (app, pool) = test_app_with_pool().await;
    sqlx::query("UPDATE tenants SET branding_custom_css = $1 WHERE slug = $2")
        .bind("color:#fff; font-family:Ünsere Schrift; letter-spacing:é")
        .bind(TEST_TENANT_SLUG)
        .execute(&pool)
        .await
        .unwrap();

    let res = app
        .oneshot(get_request(&format!("/t/{}/login", TEST_TENANT_SLUG)))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn benchmark_query_params_cannot_break_out_of_the_config_script() {
    // /platform/benchmark has no auth layer and interpolated serde_json output
    // straight into a <script>. serde_json escapes `"` and `\` but not `<`, so
    // a query param could close the element, or open a comment and put the
    // tokenizer into script-data-escaped state where the real </script> stops
    // closing it.
    let app = test_app().await;
    let res = app
        .oneshot(get_request(
            "/platform/benchmark?sort=%3C/script%3E%3Ch1%20id=pwned%3Ex&dir=%3C!--%3Cscript%3E&search=%3C/SCRIPT%3E",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_string(res).await;

    // Isolate the config script's contents. Asserting over the whole document
    // would be wrong: Forma's SSR island markers are legitimate `<!--` comments.
    let marker = body
        .find("window.__FORMA_SERVER_URL__")
        .expect("benchmark page must emit a config script");
    let open_end = body[..marker].rfind('>').expect("script tag open") + 1;
    let close = body[open_end..]
        .find("</script>")
        .expect("config script must be closed")
        + open_end;
    let script = &body[open_end..close];

    // `json_for_script` escapes every `<`, so the script body has none.
    assert!(
        !script.contains('<'),
        "unescaped `<` inside the config script: {script:?}"
    );
    // The payload survived, escaped — encoded rather than silently dropped.
    assert!(
        script.contains("\\u003C"),
        "expected the injected `<` to be escaped: {script:?}"
    );
    // And the document's script elements still balance.
    assert_eq!(
        body.matches("<script").count(),
        body.matches("</script>").count(),
        "unbalanced <script> elements"
    );
}

#[tokio::test]
async fn manage_config_sanitizes_branding_custom_css_on_write() {
    // This path stored the value raw while the self-serve path sanitized it —
    // the same field, the same render sink, two write paths with different
    // care. The validator existed and was simply never called here, so this
    // asserts the WIRING, which a unit test on the sanitizer cannot.
    let (app, pool) = test_app_with_pool().await;
    let (tenant_id, mgmt_key) =
        create_test_tenant_with_mgmt_key(&pool, "mgmt-css", "Mgmt CSS").await;

    let res = app
        .oneshot(mgmt_request(
            "PATCH",
            "/manage/config",
            serde_json::json!({
                "branding_custom_css": "color:#fff;}</StYlE><script>alert(1)</script>;background:url(https://evil.example)"
            }),
            &mgmt_key,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let stored: Option<String> =
        sqlx::query_scalar("SELECT branding_custom_css FROM tenants WHERE id = $1")
            .bind(&tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let stored = stored.unwrap_or_default();
    for forbidden in ['<', '>', '{', '}', '\\'] {
        assert!(
            !stored.contains(forbidden),
            "management write path stored {forbidden:?}: {stored:?}"
        );
    }
    assert!(
        !stored.to_ascii_lowercase().contains("url"),
        "management write path stored a fetching notation: {stored:?}"
    );
}
