pub mod admin;
pub mod auth;
pub mod benchmark;
pub mod bridge_integrity;
pub mod compliance;
pub mod dashboard;
pub mod distribution;
pub mod docs;
pub mod fedcm;
pub mod geolocation;
pub mod hosted_code;
pub mod hosted_login;
pub mod hosted_page;
pub mod introspect;
pub mod introspection;
pub mod jwks;
pub mod management;
pub mod mfa;
pub mod mtls;
pub mod not_found;
pub mod oauth;
pub mod onboarding;
pub mod organizations;
pub mod owner;
pub mod par;
pub mod password_reset;
pub mod platform_dashboard;
pub mod roles;
pub mod ssf;
pub mod ssf_engine;
pub mod token;
pub mod webauthn;
pub mod webhook_engine;
pub mod webhooks;

use crate::compliance::enforcer::enforce_compliance;
use crate::middleware::admin::require_admin_role;
use crate::middleware::auth::require_auth;
use crate::middleware::entitlements::{
    require_custom_roles, require_organizations, require_webhooks,
};
use crate::middleware::management_auth::{require_management_auth, require_platform_admin};
use crate::middleware::platform_owner::require_platform_owner;
use crate::middleware::platform_team::{
    require_tenant_access, resolve_platform_member,
};
use crate::middleware::platform_user::require_platform_user;
use crate::middleware::rate_limit;
use crate::middleware::tenant::{extract_tenant, PathSlug};
use crate::AppState;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware,
    response::{IntoResponse, Redirect},
    routing::{delete, get, patch, post},
    Json, Router,
};
use tower::ServiceExt as _;

pub fn create_router(state: AppState) -> Router {
    create_router_inner(state, true)
}

pub fn create_router_without_rate_limit(state: AppState) -> Router {
    create_router_inner(state, false)
}

fn create_router_inner(state: AppState, rate_limit_enabled: bool) -> Router {
    // ── Protected auth routes (require JWT + compliance enforcement) ──
    let protected = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/introspect", get(introspect::introspect))
        .route("/auth/account", delete(auth::delete_account))
        .route("/auth/login-history", get(auth::login_history))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            enforce_compliance,
        ))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // ── WebSocket ticket (auth-protected) ──────────────────────────
    let ws_ticket_route = Router::new()
        .route("/ws/ticket", post(crate::ws::ticket::create_ticket))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Resend-verification: auth-protected + rate-limited (reuse register_limiter: 3/10min)
    let resend_verification_route = Router::new()
        .route("/auth/resend-verification", post(auth::resend_verification))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));
    let resend_verification_route = if rate_limit_enabled {
        resend_verification_route.layer(rate_limit::register_limiter())
    } else {
        resend_verification_route
    };

    // MFA endpoints are auth-protected but exempt from compliance-middleware
    // lockouts so users can complete MFA enrollment/challenge.
    let mfa_verify_route = Router::new().route("/auth/mfa/verify", post(mfa::verify));
    let mfa_verify_route = if rate_limit_enabled {
        mfa_verify_route.layer(rate_limit::mfa_verify_limiter())
    } else {
        mfa_verify_route
    };

    let mfa_routes = Router::new()
        .route("/auth/mfa/status", get(mfa::status))
        .route("/auth/mfa/setup", post(mfa::setup))
        .merge(mfa_verify_route)
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // WebAuthn authenticated routes (registration + credential management)
    let webauthn_authed_routes = Router::new()
        .route(
            "/auth/webauthn/register/begin",
            post(webauthn::register_begin),
        )
        .route(
            "/auth/webauthn/register/complete",
            post(webauthn::register_complete),
        )
        .route(
            "/auth/webauthn/credentials",
            get(webauthn::list_credentials),
        )
        .route(
            "/auth/webauthn/credentials/{id}",
            patch(webauthn::rename_credential).delete(webauthn::delete_credential),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // WebAuthn public routes (login — no JWT required)
    let webauthn_login_routes = Router::new()
        .route(
            "/auth/webauthn/login/begin",
            post(webauthn::login_begin),
        )
        .route(
            "/auth/webauthn/login/complete",
            post(webauthn::login_complete),
        );
    let webauthn_login_routes = if rate_limit_enabled {
        webauthn_login_routes.layer(rate_limit::login_limiter())
    } else {
        webauthn_login_routes
    };

    // ── Public auth routes (with optional rate limiting) ────────────
    let register_routes = Router::new().route("/auth/register", post(auth::register));
    let login_routes = Router::new().route("/auth/login", post(auth::login));
    let authorize_routes = Router::new().route("/auth/authorize", get(oauth::authorize));
    let token_routes = Router::new().route("/auth/token", post(token::token_exchange));
    let password_reset_routes = Router::new()
        .route(
            "/auth/forgot-password",
            post(password_reset::forgot_password),
        )
        .route("/auth/reset-password", post(password_reset::reset_password));
    let verify_email_routes = Router::new().route("/auth/verify-email", get(auth::verify_email));

    let auth_routes = if rate_limit_enabled {
        Router::new()
            .merge(register_routes.layer(rate_limit::register_limiter()))
            .merge(login_routes.layer(rate_limit::login_limiter()))
            .merge(authorize_routes.layer(rate_limit::login_limiter()))
            .merge(token_routes.layer(rate_limit::token_limiter()))
            .merge(password_reset_routes.layer(rate_limit::register_limiter()))
            .merge(verify_email_routes.layer(rate_limit::register_limiter()))
    } else {
        Router::new()
            .merge(register_routes)
            .merge(login_routes)
            .merge(authorize_routes)
            .merge(token_routes)
            .merge(password_reset_routes)
            .merge(verify_email_routes)
    };

    // ── Tenant-scoped auth (all auth routes go through tenant middleware) ──
    let tenant_auth = auth_routes
        .route("/auth/logout", post(auth::logout))
        .route("/oauth/par", post(par::par_endpoint))
        .route("/login", get(hosted_login::login_page))
        .route("/dashboard", get(dashboard::dashboard_page))
        .route("/admin", get(dashboard::admin_console_page))
        .route("/sandbox", get(|| async { StatusCode::NOT_FOUND }))
        .route("/site", get(hosted_page::site_page))
        .merge(mfa_routes)
        .merge(webauthn_authed_routes)
        .merge(webauthn_login_routes)
        .merge(protected)
        .merge(resend_verification_route)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            extract_tenant,
        ));

    // ── Shared route builders for /manage/* and /admin/* ────────────
    // Both route trees expose the same user/config/invite/role/webhook/org
    // handlers with different prefixes and different auth middleware.

    /// Build the shared user + config + invite route tree under `prefix`.
    fn shared_base_routes(prefix: &str) -> Router<AppState> {
        Router::new()
            .route(
                &format!("{prefix}/users"),
                get(management::list_users).post(management::admin_create_user),
            )
            .route(&format!("{prefix}/users/{{id}}"), get(management::get_user))
            .route(
                &format!("{prefix}/users/{{id}}"),
                delete(management::delete_user),
            )
            .route(
                &format!("{prefix}/users/{{id}}/block"),
                post(management::block_user),
            )
            .route(
                &format!("{prefix}/users/{{id}}/unblock"),
                post(management::unblock_user),
            )
            .route(
                &format!("{prefix}/users/{{id}}/revoke-sessions"),
                post(management::revoke_user_sessions),
            )
            .route(
                &format!("{prefix}/users/{{id}}/roles"),
                post(roles::assign_role),
            )
            .route(
                &format!("{prefix}/users/{{id}}/roles/{{role_id}}"),
                delete(roles::remove_role),
            )
            .route(
                &format!("{prefix}/config"),
                get(management::get_config).patch(management::update_config),
            )
            .route(
                &format!("{prefix}/invites"),
                get(management::list_tenant_invites).post(management::create_tenant_invite),
            )
            .route(
                &format!("{prefix}/invites/{{id}}/revoke"),
                post(management::revoke_tenant_invite),
            )
    }

    fn shared_roles_routes(prefix: &str, state: &AppState) -> Router<AppState> {
        Router::new()
            .route(
                &format!("{prefix}/roles"),
                get(roles::list_roles).post(roles::create_role),
            )
            .route(
                &format!("{prefix}/roles/{{id}}"),
                patch(roles::update_role).delete(roles::delete_role),
            )
            .layer(middleware::from_fn_with_state(
                state.clone(),
                require_custom_roles,
            ))
    }

    fn shared_webhook_routes(prefix: &str, state: &AppState) -> Router<AppState> {
        Router::new()
            .route(
                &format!("{prefix}/webhooks"),
                get(webhooks::list_webhooks).post(webhooks::create_webhook),
            )
            .route(
                &format!("{prefix}/webhooks/{{id}}"),
                get(webhooks::get_webhook)
                    .patch(webhooks::update_webhook)
                    .delete(webhooks::delete_webhook),
            )
            .route(
                &format!("{prefix}/webhooks/{{id}}/deliveries"),
                get(webhooks::list_deliveries),
            )
            .route(
                &format!("{prefix}/webhooks/{{id}}/test"),
                post(webhooks::test_webhook),
            )
            .layer(middleware::from_fn_with_state(
                state.clone(),
                require_webhooks,
            ))
    }

    fn shared_org_routes(prefix: &str, state: &AppState) -> Router<AppState> {
        Router::new()
            .route(
                &format!("{prefix}/organizations"),
                get(organizations::list_organizations).post(organizations::create_organization),
            )
            .route(
                &format!("{prefix}/organizations/{{id}}"),
                get(organizations::get_organization)
                    .patch(organizations::update_organization)
                    .delete(organizations::delete_organization),
            )
            .route(
                &format!("{prefix}/organizations/{{id}}/members"),
                get(organizations::list_members).post(organizations::add_member),
            )
            .route(
                &format!("{prefix}/organizations/{{id}}/members/{{user_id}}"),
                delete(organizations::remove_member),
            )
            .layer(middleware::from_fn_with_state(
                state.clone(),
                require_organizations,
            ))
    }

    // ── Management API (authenticated with mgmt_ key) ──────────────
    let manage_routes = Router::new()
        .merge(shared_base_routes("/manage"))
        .route("/manage/oauth-clients", get(management::list_oauth_clients).post(management::register_oauth_client))
        .route("/manage/stats", get(management::tenant_stats))
        .route("/manage/audit-log", get(management::audit_log))
        .route(
            "/manage/tenants/{id}/security",
            patch(management::update_security_config),
        )
        .route(
            "/manage/certificates",
            get(mtls::list_certificates).post(mtls::register_certificate),
        )
        .route(
            "/manage/certificates/{fingerprint}",
            delete(mtls::revoke_certificate),
        )
        .merge(shared_roles_routes("/manage", &state))
        .merge(shared_webhook_routes("/manage", &state))
        .merge(shared_org_routes("/manage", &state))
        .merge(
            Router::new()
                .route(
                    "/manage/ssf/streams",
                    get(ssf::list_streams).post(ssf::create_stream),
                )
                .route(
                    "/manage/ssf/streams/{id}",
                    get(ssf::get_stream)
                        .patch(ssf::update_stream)
                        .delete(ssf::delete_stream),
                )
                .route(
                    "/manage/ssf/streams/{id}/verify",
                    post(ssf::verify_stream),
                )
                .route(
                    "/manage/ssf/streams/{id}/rotate-secret",
                    post(ssf::rotate_secret),
                )
                .route(
                    "/manage/ssf/streams/{id}/poll",
                    post(ssf::poll_stream),
                ),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_management_auth,
        ));

    // ── Platform Admin API (authenticated with plat_ key) ──────────
    let platform_routes = Router::new()
        .route("/platform/tenants", post(management::create_tenant))
        .route("/platform/tenants", get(management::list_tenants))
        .route("/platform/tenants/{id}", get(management::get_tenant))
        .route("/platform/tenants/{id}", patch(management::update_tenant))
        .route("/platform/tenants/{id}", delete(management::delete_tenant))
        .route(
            "/platform/tenants/{id}/rotate-key",
            post(management::rotate_api_key),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_platform_admin,
        ));

    // ── Self-Serve API (JWT auth for platform tenant users) ──────
    let self_serve_routes = Router::new()
        .route(
            "/platform/self-serve/check-slug/{slug}",
            get(onboarding::check_slug_availability),
        )
        .route(
            "/platform/self-serve/tenants",
            get(onboarding::list_my_tenants).post(onboarding::create_tenant),
        )
        .route(
            "/platform/self-serve/tenants/{id}",
            get(onboarding::get_my_tenant)
                .patch(onboarding::update_my_tenant)
                .delete(onboarding::delete_my_tenant),
        )
        .route(
            "/platform/self-serve/tenants/{id}/rotate-key",
            post(onboarding::rotate_my_tenant_key),
        )
        .route(
            "/platform/self-serve/tenants/{id}/stats",
            get(onboarding::tenant_stats),
        )
        .route(
            "/platform/self-serve/tenants/{id}/users",
            get(onboarding::list_tenant_users),
        )
        .route(
            "/platform/self-serve/tenants/{id}/users/{user_id}/block",
            post(onboarding::block_tenant_user),
        )
        .route(
            "/platform/self-serve/tenants/{id}/users/{user_id}/unblock",
            post(onboarding::unblock_tenant_user),
        )
        .route(
            "/platform/self-serve/tenants/{id}/users/{user_id}/revoke-sessions",
            post(onboarding::revoke_tenant_user_sessions),
        )
        .route(
            "/platform/self-serve/tenants/{id}/users/{user_id}/sessions",
            get(onboarding::list_tenant_user_sessions),
        )
        .route(
            "/platform/self-serve/tenants/{id}/audit-log",
            get(onboarding::list_tenant_audit_log),
        )
        .route(
            "/platform/self-serve/tenants/{id}/sessions",
            get(onboarding::list_tenant_sessions),
        )
        .route(
            "/platform/self-serve/tenants/{id}/passkeys",
            get(onboarding::list_tenant_passkeys),
        )
        .route(
            "/platform/self-serve/tenants/{id}/roles",
            get(onboarding::list_tenant_roles).post(onboarding::create_tenant_role),
        )
        .route(
            "/platform/self-serve/tenants/{id}/roles/{role_id}",
            patch(onboarding::update_tenant_role).delete(onboarding::delete_tenant_role),
        )
        .route(
            "/platform/self-serve/tenants/{id}/security-config",
            get(onboarding::get_tenant_security_config).patch(onboarding::update_tenant_security_config),
        )
        .route(
            "/platform/self-serve/tenants/{id}/hosted-page",
            get(onboarding::get_hosted_page).put(onboarding::update_hosted_page),
        )
        .route(
            "/platform/self-serve/tenants/{id}/invites",
            get(onboarding::list_tenant_invites_selfserve).post(onboarding::create_tenant_invite_selfserve),
        )
        .route(
            "/platform/self-serve/tenants/{id}/invites/{invite_id}/revoke",
            post(onboarding::revoke_tenant_invite_selfserve),
        )
        .route(
            "/platform/self-serve/tenants/{id}/organizations",
            get(onboarding::list_tenant_organizations).post(onboarding::create_tenant_organization),
        )
        .route(
            "/platform/self-serve/tenants/{id}/organizations/{org_id}",
            get(onboarding::get_tenant_organization).patch(onboarding::update_tenant_organization).delete(onboarding::delete_tenant_organization),
        )
        .route(
            "/platform/self-serve/tenants/{id}/organizations/{org_id}/members",
            get(onboarding::list_organization_members).post(onboarding::add_organization_member),
        )
        .route(
            "/platform/self-serve/tenants/{id}/organizations/{org_id}/members/{user_id}",
            delete(onboarding::remove_organization_member),
        )
        .route(
            "/platform/self-serve/tenants/{id}/users/{user_id}/detail",
            get(onboarding::get_tenant_user_detail),
        )
        .route(
            "/platform/self-serve/tenants/{id}/webhooks",
            get(onboarding::list_tenant_webhooks).post(onboarding::create_tenant_webhook),
        )
        .route(
            "/platform/self-serve/tenants/{id}/webhooks/{webhook_id}",
            get(onboarding::get_tenant_webhook).patch(onboarding::update_tenant_webhook).delete(onboarding::delete_tenant_webhook),
        )
        .route(
            "/platform/self-serve/tenants/{id}/webhooks/{webhook_id}/deliveries",
            get(onboarding::list_webhook_deliveries),
        )
        .route(
            "/platform/self-serve/tenants/{id}/webhooks/{webhook_id}/test",
            post(onboarding::test_tenant_webhook),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_tenant_access,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            resolve_platform_member,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_platform_user,
        ));

    // ── Admin API (JWT + admin role, tenant-scoped) ─────────────────
    let admin_routes = Router::new()
        .merge(shared_base_routes("/admin"))
        .route("/admin/stats", get(admin::stats))
        .route("/admin/audit-log", get(admin::audit_log))
        .merge(shared_roles_routes("/admin", &state))
        .merge(shared_webhook_routes("/admin", &state))
        .merge(shared_org_routes("/admin", &state))
        .layer(middleware::from_fn(require_admin_role))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            extract_tenant,
        ));

    // ── Provider Owner API (JWT + platform_owner role) ───────────────
    let owner_api_routes = Router::new()
        .route("/platform/admin/stats", get(owner::owner_stats))
        .route("/platform/admin/analytics", get(owner::owner_analytics))
        .route("/platform/admin/customers", get(owner::list_customers))
        .route("/platform/admin/tenants", get(owner::list_tenants))
        .route("/platform/admin/timeline", get(owner::owner_timeline))
        .route("/platform/admin/customers/{id}", get(owner::get_customer))
        .route(
            "/platform/admin/customers/{id}/insights",
            get(owner::customer_insights),
        )
        .route(
            "/platform/admin/customers/{id}/tenants",
            get(owner::list_customer_tenants),
        )
        .route(
            "/platform/admin/customers/{id}/lifecycle",
            patch(owner::update_customer_lifecycle),
        )
        .route(
            "/platform/admin/customers/{id}/extend-access",
            post(owner::extend_customer_access),
        )
        .route(
            "/platform/admin/customers/{id}/tenants/{tenant_id}/status",
            post(owner::update_customer_tenant_status),
        )
        .route(
            "/platform/admin/customers/{id}/tenants/{tenant_id}",
            delete(owner::delete_customer_tenant),
        )
        .route(
            "/platform/admin/payments",
            get(owner::list_payments).post(owner::create_payment),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_platform_owner))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // ── Platform Team API (JWT + account owner only) ──────────────────
    // Invite routes live here too — they're customer team management, not platform owner ops.
    let team_routes = Router::new()
        .route("/", axum::routing::get(owner::list_team_members))
        .route("/{id}", axum::routing::get(owner::get_team_member))
        .route("/{id}", axum::routing::patch(owner::update_team_member))
        .route("/{id}", axum::routing::delete(owner::remove_team_member))
        .route("/invites", axum::routing::get(owner::list_owner_invites).post(owner::create_owner_invite))
        .route("/invites/{id}/revoke", axum::routing::post(owner::revoke_owner_invite))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::platform_team::require_account_owner,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::platform_team::require_platform_member,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::auth::require_auth,
        ));

    // ── Provider owner invite acceptance (authenticated platform users) ───
    let owner_invite_accept_routes = Router::new()
        .route(
            "/platform/admin/invites/accept",
            post(owner::accept_owner_invite),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_platform_user,
        ));

    // ── Public invite resolution (no auth required) ──────────────
    let invite_resolve_routes = Router::new()
        .route(
            "/platform/admin/invites/resolve",
            get(owner::resolve_invite),
        );

    // ── Compliance API (require management auth + tenant) ──────────
    let compliance_routes = Router::new()
        .route(
            "/api/v1/compliance/status",
            get(compliance::compliance_status),
        )
        .route(
            "/api/v1/compliance/report",
            get(compliance::compliance_report),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_management_auth,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            extract_tenant,
        ));

    // ── Token Introspection (RFC 7662, management auth) ────────────
    let introspection_routes = Router::new()
        .route("/oauth/introspect", post(introspection::introspect))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_management_auth,
        ));

    // ── Internal docs (platform-admin only) ────────────────────────
    let internal_docs_routes = Router::new()
        .route("/internal/docs", get(docs::internal_docs_page))
        .route("/internal/openapi.json", get(docs::internal_openapi_json))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_platform_admin,
        ));

    // ── Global routes (no tenant middleware) ────────────────────────
    let main_router = Router::new()
        .merge(tenant_auth)
        .merge(manage_routes)
        .merge(platform_routes)
        .merge(self_serve_routes)
        .merge(admin_routes)
        .merge(owner_invite_accept_routes)
        .merge(invite_resolve_routes)
        .merge(owner_api_routes)
        .nest("/platform/admin/team", team_routes)
        .merge(compliance_routes)
        .merge(introspection_routes)
        .merge(internal_docs_routes)
        .route("/", get(|| async { Redirect::temporary("/t/forma/login") }))
        .route("/docs", get(docs::docs_page))
        .route("/openapi.json", get(docs::openapi_json))
        .route("/favicon.ico", get(favicon))
        .route("/.well-known/web-identity", get(fedcm::web_identity))
        .route("/fedcm/config.json", get(fedcm::config))
        .route("/fedcm/accounts", get(fedcm::accounts))
        .route("/fedcm/token", post(fedcm::token_assertion))
        .route("/auth/jwks", get(jwks::jwks))
        .route("/auth/callback/{provider}", get(oauth::callback))
        .merge(ws_ticket_route)
        .route("/ws", get(crate::ws::handler::ws_upgrade))
        .route("/health", get(health))
        .route("/bridge.js", get(serve_bridge_js))
        .route("/bridge/integrity", get(bridge_integrity::bridge_integrity))
        .route(
            "/get-started",
            get(|| async { Redirect::temporary("/platform/onboarding") }),
        )
        .route("/platform/login", get(onboarding::platform_login_page))
        .route("/platform/onboarding", get(onboarding::get_started_page))
        .route("/platform/benchmark", get(benchmark::benchmark_page))
        .route("/platform/api/benchmark", get(benchmark::benchmark_api))
        .route(
            "/platform/console",
            get(platform_dashboard::dashboard_page),
        )
        .route(
            "/platform/console/{slug}",
            get(platform_dashboard::dashboard_page),
        )
        .route("/platform/admin/dashboard", get(owner::dashboard_page))
        .route("/sw.js", get(crate::assets::serve_sw))
        .route("/_assets/{filename}", get(crate::assets::serve_asset))
        .fallback(not_found::not_found_page)
        .with_state(state.clone());

    // ── Slug-in-URL routing (/t/{slug}/...) ──────────────────────
    // When a request comes in for /t/{slug}/auth/login, Axum's Router::nest strips
    // the "/t" prefix, then the slug_dispatch fallback handler parses the slug from
    // the remaining path (/{slug}/auth/login), strips it, injects PathSlug into
    // extensions, and dispatches the rewritten request (/auth/login) to main_router.
    let main_for_slug = main_router.clone();
    let state_for_slug = state.clone();
    // The slices below are indexed from `find('/')` on a URI path. `/` is one
    // byte and the path always starts with one, so every index used here is a
    // char boundary.
    #[allow(clippy::string_slice)]
    let slug_handler = Router::new().fallback(move |mut req: Request| {
        let router = main_for_slug.clone();
        let state = state_for_slug.clone();
        async move {
            let path = req.uri().path().to_string();
            // After nest("/t") strips the prefix, path is "/{slug}/auth/login"
            if let Some(slash_pos) = path[1..].find('/') {
                let slug = &path[1..1 + slash_pos];
                let new_path = &path[1 + slash_pos..];
                req.extensions_mut().insert(PathSlug(slug.to_string()));
                let new_pq = match req.uri().query() {
                    Some(q) => format!("{}?{}", new_path, q),
                    None => new_path.to_string(),
                };
                let mut parts = req.uri().clone().into_parts();
                match new_pq.parse() {
                    Ok(pq) => {
                        parts.path_and_query = Some(pq);
                    }
                    Err(_) => return axum::http::StatusCode::BAD_REQUEST.into_response(),
                }
                match axum::http::Uri::from_parts(parts) {
                    Ok(uri) => {
                        *req.uri_mut() = uri;
                    }
                    Err(_) => return axum::http::StatusCode::BAD_REQUEST.into_response(),
                }
                router.oneshot(req).await.into_response()
            } else {
                not_found::page_response(&state, &format!("/t{}", path))
            }
        }
    });

    main_router
        .nest("/t", slug_handler)
        // Applied here rather than in main.rs so it is part of the router
        // itself: every construction of the app gets the floor, and the
        // integration harness — which builds the router directly — actually
        // exercises it. A security default that lives only in main.rs is the
        // same opt-in problem one level up.
        .layer(middleware::from_fn(crate::middleware::csp::default_csp))
}

/// Serves the fAuth bridge.js client library (~3.4KB minified).
async fn serve_bridge_js() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "application/javascript"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=3600"),
            (
                axum::http::header::HeaderName::from_static("cross-origin-resource-policy"),
                "cross-origin",
            ),
        ],
        include_str!("../../../../bridge/gate-bridge.min.js"),
    )
}

async fn favicon() -> impl IntoResponse {
    match crate::assets::asset_bytes("formajs-favicon.svg") {
        Some(bytes) => (
            [
                (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
                (header::CONTENT_TYPE, "image/svg+xml".to_string()),
            ],
            bytes,
        )
            .into_response(),
        None => (
            [(header::CACHE_CONTROL, "public, max-age=86400".to_string())],
            StatusCode::NO_CONTENT,
        )
            .into_response(),
    }
}

async fn health(State(state): State<AppState>) -> axum::response::Response {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1::int4")
        .fetch_one(&state.db)
        .await
        .is_ok();

    let signer_current = state.keys.serving_current_generation().await;
    (if db_ok && signer_current { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE }, Json(serde_json::json!({
        "signer_current": signer_current,
        "status": if db_ok && signer_current { "ok" } else { "degraded" },
        "db_status": if db_ok { "ok" } else { "error" },
        "ws_connections": state.connection_limits.active_count(),
        "source_revision": option_env!("GATEWASM_SOURCE_REVISION").unwrap_or("unrecorded"),
    }))).into_response()
}
