//! Controlled mailbox verification, provider side (FORMA-247 mail follow-on,
//! plan steps 2-3): truthful delivery outcomes and a hashed, tenant/address
//! bound, atomic one-use challenge. Messages go only to an owned loopback SMTP
//! capture with `.test` addresses.
use super::*;

const PASSWORD: &str = "Str0ngP@ss";

/// Owned loopback SMTP sink: accepts every message and keeps its DATA section.
pub(super) async fn smtp_capture() -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let messages = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let capture = messages.clone();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let capture = capture.clone();
            tokio::spawn(async move {
                let mut stream = BufReader::new(stream);
                stream.get_mut().write_all(b"220 localhost SMTP\r\n").await.unwrap();
                let mut data = false;
                let mut message = String::new();
                loop {
                    let mut line = String::new();
                    if stream.read_line(&mut line).await.unwrap() == 0 {
                        break;
                    }
                    let answer = if data {
                        if line == ".\r\n" {
                            capture.lock().unwrap().push(std::mem::take(&mut message));
                            data = false;
                            "250 accepted\r\n"
                        } else {
                            message.push_str(&line);
                            continue;
                        }
                    } else if line.starts_with("DATA") {
                        data = true;
                        "354 send data\r\n"
                    } else if line.starts_with("QUIT") {
                        stream.get_mut().write_all(b"221 bye\r\n").await.unwrap();
                        break;
                    } else {
                        "250 localhost\r\n"
                    };
                    stream.get_mut().write_all(answer.as_bytes()).await.unwrap();
                }
            });
        }
    });
    (port, messages)
}

fn with_smtp(state: &mut AppState, port: u16) {
    state.config.suppress_email = false;
    state.config.smtp_host = Some("127.0.0.1".into());
    state.config.smtp_port = port;
    state.config.smtp_starttls = false;
    state.config.smtp_from_email = Some("sender@example.test".into());
}

/// Wait for the `n`th captured message (bounded).
async fn message(messages: &std::sync::Arc<std::sync::Mutex<Vec<String>>>, n: usize) -> String {
    for _ in 0..100 {
        if let Some(m) = messages.lock().unwrap().get(n) {
            return m.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("verification message {n} was not captured");
}

/// The raw challenge from the message's verification link (quoted-printable
/// soft breaks and `=3D` decoded).
fn link_token(message: &str, slug: &str) -> String {
    let decoded = message.replace("=\r\n", "").replace("=3D", "=");
    let marker = format!("https://auth.gatewasm.test/t/{slug}/auth/verify-email?token=");
    let start = decoded.find(&marker).expect("message carries the tenant verification link") + marker.len();
    decoded[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

async fn register(app: &axum::Router, email: &str) -> String {
    let res = app
        .clone()
        .oneshot(json_request("POST", "/auth/register", serde_json::json!({"email": email, "password": PASSWORD})))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    body_json(res).await["access_token"].as_str().unwrap().to_string()
}

fn confirm(slug: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/t/{slug}/auth/verify-email"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(format!("token={}", urlencoding::encode(token))))
        .unwrap()
}

async fn resend(app: &axum::Router, access: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/t/{TEST_TENANT_SLUG}/auth/resend-verification"))
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn verified(pool: &PgPool, email: &str) -> bool {
    sqlx::query_scalar("SELECT COALESCE(email_verified, FALSE) FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Bounded wait until the async delivery outcome is recorded.
async fn delivery(pool: &PgPool, email: &str) -> Option<String> {
    for _ in 0..100 {
        let outcome: Option<String> =
            sqlx::query_scalar("SELECT verification_delivery FROM users WHERE email = $1")
                .bind(email)
                .fetch_one(pool)
                .await
                .unwrap();
        if outcome.as_deref().is_some_and(|o| o != "pending") {
            return outcome;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    None
}

/// Store a known challenge for refusal cases where a captured link is not the
/// subject (expiry, address change, races). Never used to set the flag itself.
async fn seed_challenge(pool: &PgPool, email: &str, raw: &str, expires_in_secs: f64) {
    sqlx::query(
        "UPDATE users SET verification_token = $1, verification_email = email, \
         verification_token_expires = clock_timestamp() + make_interval(secs => $2) WHERE email = $3",
    )
    .bind(signing::hash_token(raw))
    .bind(expires_in_secs)
    .bind(email)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn real_message_link_is_hashed_scanner_safe_tenant_bound_and_one_use() {
    let (port, messages) = smtp_capture().await;
    let (mut state, pool) = test_state_with_keys(None).await;
    with_smtp(&mut state, port);
    create_test_tenant_in_db(&pool, "other-tenant", "Other Tenant").await;
    let app = handlers::create_router_without_rate_limit(state);
    let email = "maya@example.test";
    register(&app, email).await;

    let mail = message(&messages, 0).await;
    assert!(mail.contains("To: maya@example.test"), "exact invited recipient");
    assert!(mail.contains("sender@example.test"), "configured sender");
    assert!(mail.contains("Verify your Test Tenant account"), "branded tenant subject");
    let token = link_token(&mail, TEST_TENANT_SLUG);
    assert!(token.starts_with("vt_") && token.len() > 40);
    let (stored, bound): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT verification_token, verification_email FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.as_deref(), Some(signing::hash_token(&token).as_str()), "only the hash is stored");
    assert_eq!(bound.as_deref(), Some(email));
    assert_eq!(delivery(&pool, email).await.as_deref(), Some("provider_accepted"));

    // Opening the link (or a scanner prefetching it) changes nothing.
    let page = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/t/{TEST_TENANT_SLUG}/auth/verify-email?token={}", urlencoding::encode(&token)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(page.headers()["cache-control"], "no-store");
    assert_eq!(page.headers()["referrer-policy"], "no-referrer");
    let csp = page.headers()["content-security-policy"].to_str().unwrap().to_string();
    assert!(csp.contains("form-action 'self'") && !csp.contains("script-src"));
    let html = String::from_utf8(axum::body::to_bytes(page.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    assert!(html.contains(r#"action="/t/test-tenant/auth/verify-email""#), "POST target has no query string");
    assert!(!verified(&pool, email).await);

    // Another tenant's endpoint cannot consume this tenant's challenge.
    let wrong = app.clone().oneshot(confirm("other-tenant", &token)).await.unwrap();
    assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
    assert!(!verified(&pool, email).await);

    let ok = app.clone().oneshot(confirm(TEST_TENANT_SLUG, &token)).await.unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(ok.headers()["cache-control"], "no-store");
    let html = String::from_utf8(axum::body::to_bytes(ok.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    assert!(!html.contains("/platform/login") && !html.contains("http-equiv"), "no redirect or session resumption");
    assert!(verified(&pool, email).await);
    let cleared: Option<String> = sqlx::query_scalar("SELECT verification_token FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(cleared.is_none());

    let replay = app.clone().oneshot(confirm(TEST_TENANT_SLUG, &token)).await.unwrap();
    assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn expired_changed_address_raw_hash_and_concurrent_consumes_are_refused() {
    let (state, pool) = test_state_with_keys(None).await;
    let app = handlers::create_router_without_rate_limit(state);
    let email = "jordan@example.test";
    register(&app, email).await;

    seed_challenge(&pool, email, "vt_expired", -1.0).await;
    assert_eq!(app.clone().oneshot(confirm(TEST_TENANT_SLUG, "vt_expired")).await.unwrap().status(), StatusCode::BAD_REQUEST);

    // Presenting the stored hash itself is not the capability.
    seed_challenge(&pool, email, "vt_secret", 600.0).await;
    let hash = signing::hash_token("vt_secret");
    assert_eq!(app.clone().oneshot(confirm(TEST_TENANT_SLUG, &hash)).await.unwrap().status(), StatusCode::BAD_REQUEST);

    seed_challenge(&pool, email, "vt_moved", 600.0).await;
    sqlx::query("UPDATE users SET email = 'jordan.new@example.test' WHERE email = $1")
        .bind(email)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(app.clone().oneshot(confirm(TEST_TENANT_SLUG, "vt_moved")).await.unwrap().status(), StatusCode::BAD_REQUEST);
    assert!(!verified(&pool, "jordan.new@example.test").await, "a challenge issued to another address never verifies");
    sqlx::query("UPDATE users SET email = $1 WHERE email = 'jordan.new@example.test'")
        .bind(email)
        .execute(&pool)
        .await
        .unwrap();

    seed_challenge(&pool, email, "vt_race", 600.0).await;
    let attempts = (0..8).map(|_| app.clone().oneshot(confirm(TEST_TENANT_SLUG, "vt_race")));
    let statuses: Vec<StatusCode> = futures_util::future::join_all(attempts)
        .await
        .into_iter()
        .map(|r| r.unwrap().status())
        .collect();
    assert_eq!(statuses.iter().filter(|s| **s == StatusCode::OK).count(), 1, "{statuses:?}");
    assert_eq!(statuses.iter().filter(|s| **s == StatusCode::BAD_REQUEST).count(), 7);
    assert!(verified(&pool, email).await);
}

#[tokio::test]
async fn expiry_during_row_lock_wait_refuses_verification() {
    let (state, pool) = test_state_with_keys(None).await;
    let app = handlers::create_router_without_rate_limit(state);
    let email = "sarah@example.test";
    register(&app, email).await;
    seed_challenge(&pool, email, "vt_slow", 1.0).await;

    let mut lock = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE email = $1 FOR UPDATE")
        .bind(email)
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    let waiting = tokio::spawn({
        let app = app.clone();
        async move { app.oneshot(confirm(TEST_TENANT_SLUG, "vt_slow")).await.unwrap().status() }
    });
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    assert!(!waiting.is_finished(), "consumer must be blocked on the row lock");
    lock.commit().await.unwrap();
    assert_eq!(waiting.await.unwrap(), StatusCode::BAD_REQUEST);
    assert!(!verified(&pool, email).await);
}

#[tokio::test]
async fn suppressed_or_unconfigured_mail_is_recorded_and_never_reported_sent() {
    let (state, pool) = test_state_with_keys(None).await;
    let app = handlers::create_router_without_rate_limit(state.clone());
    let email = "suppressed@example.test";
    let access = register(&app, email).await;
    assert_eq!(delivery(&pool, email).await.as_deref(), Some("suppressed"));
    let before: Option<String> = sqlx::query_scalar("SELECT verification_token FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(&pool)
        .await
        .unwrap();

    let res = resend(&app, &access).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(res).await["error"], "email_unavailable");
    let after: Option<String> = sqlx::query_scalar("SELECT verification_token FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after, "nothing rotated when nothing can be sent");

    // Unconfigured (not suppressed) is reported as such too.
    let mut unconfigured = state.clone();
    unconfigured.config.suppress_email = false;
    let app = handlers::create_router_without_rate_limit(unconfigured);
    let email = "unconfigured@example.test";
    register(&app, email).await;
    assert_eq!(delivery(&pool, email).await.as_deref(), Some("unconfigured"));

    // A legacy invite with a configured transport but SUPPRESS_EMAIL set is not "sent".
    let (port, messages) = smtp_capture().await;
    let mut suppressed = state;
    with_smtp(&mut suppressed, port);
    suppressed.config.suppress_email = true;
    let app = handlers::create_router_without_rate_limit(suppressed);
    let (_, key) = create_test_tenant_with_mgmt_key(&pool, "mail-suppressed", "Mail Suppressed").await;
    let res = app
        .oneshot(mgmt_request("POST", "/manage/invites", serde_json::json!({"email":"legacy@example.test","role":"user"}), &key))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(body_json(res).await["email_delivery"], "suppressed");
    assert!(messages.lock().unwrap().is_empty());
}

#[tokio::test]
async fn resend_rotates_cools_down_atomically_and_reports_provider_failure() {
    let (port, messages) = smtp_capture().await;
    let (mut state, pool) = test_state_with_keys(None).await;
    with_smtp(&mut state, port);
    let app = handlers::create_router_without_rate_limit(state.clone());
    let email = "resend@example.test";
    let access = register(&app, email).await;
    let first = link_token(&message(&messages, 0).await, TEST_TENANT_SLUG);
    assert_eq!(delivery(&pool, email).await.as_deref(), Some("provider_accepted"));

    let res = resend(&app, &access).await;
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body_json(res).await["error"], "resend_cooldown");

    sqlx::query("UPDATE users SET verification_delivery_at = clock_timestamp() - INTERVAL '2 minutes' WHERE email = $1")
        .bind(email)
        .execute(&pool)
        .await
        .unwrap();
    let racing = (0..4).map(|_| resend(&app, &access));
    let statuses: Vec<StatusCode> = futures_util::future::join_all(racing).await.into_iter().map(|r| r.status()).collect();
    assert_eq!(statuses.iter().filter(|s| **s == StatusCode::ACCEPTED).count(), 1, "{statuses:?}");
    assert_eq!(statuses.iter().filter(|s| **s == StatusCode::TOO_MANY_REQUESTS).count(), 3);
    let second = link_token(&message(&messages, 1).await, TEST_TENANT_SLUG);
    assert_ne!(first, second);
    assert_eq!(messages.lock().unwrap().len(), 2, "exactly one resend message");

    assert_eq!(app.clone().oneshot(confirm(TEST_TENANT_SLUG, &first)).await.unwrap().status(), StatusCode::BAD_REQUEST, "rotated link is dead");
    assert_eq!(app.clone().oneshot(confirm(TEST_TENANT_SLUG, &second)).await.unwrap().status(), StatusCode::OK);
    let res = resend(&app, &access).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["status"], "already_verified");

    // Provider refusal: a closed loopback port. Reported as failure and recorded.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    let mut failing = state;
    failing.config.smtp_port = closed_port;
    let app = handlers::create_router_without_rate_limit(failing);
    let email = "refused@example.test";
    let access = register(&app, email).await;
    assert_eq!(delivery(&pool, email).await.as_deref(), Some("failed"));
    sqlx::query("UPDATE users SET verification_delivery_at = clock_timestamp() - INTERVAL '2 minutes' WHERE email = $1")
        .bind(email)
        .execute(&pool)
        .await
        .unwrap();
    let res = resend(&app, &access).await;
    assert_eq!(res.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(body_json(res).await["error"], "email_failed");
    assert_eq!(delivery(&pool, email).await.as_deref(), Some("failed"));
    assert!(!verified(&pool, email).await);
}

#[tokio::test]
async fn verified_page_offers_only_the_clients_current_same_origin_entry() {
    const CALLBACK: &str = "https://beta.example.test/auth/callback";
    let (state, pool) = test_state_with_keys(None).await;
    let app = handlers::create_router_without_rate_limit(state);
    let (_, key) = create_test_tenant_with_mgmt_key(&pool, "entry-tenant", "Entry Tenant").await;
    let register_client = |entry: Option<&str>| {
        let mut body = serde_json::json!({"client_id":"entry-beta","name":"Beta","redirect_uri":CALLBACK});
        if let Some(entry) = entry {
            body["return_entry_url"] = entry.into();
        }
        app.clone().oneshot(mgmt_request("POST", "/manage/oauth-clients", body, &key))
    };
    for bad in [
        "https://evil.example.test/signin/h1",
        "http://beta.example.test/signin/h1",
        "https://beta.example.test/signin/h1?return_to=x",
        "https://beta.example.test/signin/h1#frag",
        "https://user@beta.example.test/signin/h1",
    ] {
        assert_eq!(register_client(Some(bad)).await.unwrap().status(), StatusCode::BAD_REQUEST, "{bad}");
    }
    let created = register_client(Some("https://beta.example.test/signin/h1")).await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(body_json(created).await["return_entry_url"], "https://beta.example.test/signin/h1");
    let rotated = register_client(Some("https://beta.example.test/signin/h2")).await.unwrap();
    assert_eq!(rotated.status(), StatusCode::OK);
    assert_eq!(body_json(rotated).await["return_entry_url"], "https://beta.example.test/signin/h2");
    let unchanged = register_client(None).await.unwrap();
    assert_eq!(body_json(unchanged).await["return_entry_url"], "https://beta.example.test/signin/h2");

    // Accounts associated with the client (the hosted signup path that sets this
    // association is covered in hosted_signup.rs).
    async fn account(app: &axum::Router, pool: &PgPool, email: &str, raw: &str) {
        let res = app
            .clone()
            .oneshot(json_request_with_slug("POST", "/auth/register", serde_json::json!({"email": email, "password": PASSWORD}), "entry-tenant"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        seed_challenge(pool, email, raw, 600.0).await;
        sqlx::query("UPDATE users SET verification_client_id = 'entry-beta' WHERE email = $1")
            .bind(email)
            .execute(pool)
            .await
            .unwrap();
    }
    async fn verified_html(app: &axum::Router, raw: &str) -> String {
        let res = app.clone().oneshot(confirm("entry-tenant", raw)).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        String::from_utf8(axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap()
    }

    account(&app, &pool, "one@example.test", "vt_entry_one").await;
    let html = verified_html(&app, "vt_entry_one").await;
    assert!(html.contains(r#"href="https://beta.example.test/signin/h2""#), "current entry offered");
    assert!(!html.contains("vt_entry_one") && !html.contains("token"), "no capability travels with the link");
    let association: Option<String> = sqlx::query_scalar("SELECT verification_client_id FROM users WHERE email = 'one@example.test'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(association.is_none(), "association cleared once used");

    // An entry that no longer matches the client's callback origin is not offered.
    account(&app, &pool, "two@example.test", "vt_entry_two").await;
    sqlx::query("UPDATE oauth_clients SET return_entry_url = 'https://evil.example.test/signin' WHERE id = 'entry-beta'")
        .execute(&pool)
        .await
        .unwrap();
    let html = verified_html(&app, "vt_entry_two").await;
    assert!(!html.contains("href=") && html.contains("Return to Entry Tenant"));

    // Another tenant's client is never offered, even if an association pointed at it.
    let (_, other_key) = create_test_tenant_with_mgmt_key(&pool, "other-entry", "Other Entry").await;
    let other = app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/oauth-clients",
            serde_json::json!({"client_id":"other-beta","name":"Other","redirect_uri":"https://other.example.test/auth/callback","return_entry_url":"https://other.example.test/signin/x"}),
            &other_key,
        ))
        .await
        .unwrap();
    assert_eq!(other.status(), StatusCode::CREATED);
    account(&app, &pool, "cross@example.test", "vt_entry_cross").await;
    sqlx::query("UPDATE users SET verification_client_id = 'other-beta' WHERE email = 'cross@example.test'")
        .execute(&pool)
        .await
        .unwrap();
    let html = verified_html(&app, "vt_entry_cross").await;
    assert!(!html.contains("other.example.test") && !html.contains("href="), "cross-tenant client never offered");

    // The stored entry is the parser's normalized form.
    let normalized = register_client(Some("HTTPS://BETA.example.test:443/signin/h3")).await.unwrap();
    assert_eq!(body_json(normalized).await["return_entry_url"], "https://beta.example.test/signin/h3");

    // A removed client yields the safe no-link page.
    account(&app, &pool, "three@example.test", "vt_entry_three").await;
    sqlx::query("DELETE FROM oauth_clients WHERE id = 'entry-beta'").execute(&pool).await.unwrap();
    let html = verified_html(&app, "vt_entry_three").await;
    assert!(!html.contains("href=") && html.contains("Return to Entry Tenant"));
}

#[tokio::test]
async fn cooldown_starts_at_issuance_and_a_racing_verification_wins() {
    // An SMTP endpoint that accepts the connection and never answers keeps the
    // signup send in flight; the cooldown must already apply.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let silent_port = silent.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            let (stream, _) = silent.accept().await.unwrap();
            held.push(stream);
        }
    });
    let (mut state, pool) = test_state_with_keys(None).await;
    with_smtp(&mut state, silent_port);
    let app = handlers::create_router_without_rate_limit(state.clone());
    let email = "inflight@example.test";
    let access = register(&app, email).await;
    let res = resend(&app, &access).await;
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body_json(res).await["error"], "resend_cooldown");

    // A resend blocked behind a verification that commits reports already_verified.
    let (port, _messages) = smtp_capture().await;
    let mut state = state;
    state.config.smtp_port = port;
    let app = handlers::create_router_without_rate_limit(state);
    let email = "racing@example.test";
    let access = register(&app, email).await;
    sqlx::query("UPDATE users SET verification_delivery_at = clock_timestamp() - INTERVAL '2 minutes' WHERE email = $1")
        .bind(email)
        .execute(&pool)
        .await
        .unwrap();
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE email = $1 FOR UPDATE")
        .bind(email)
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    let waiting = tokio::spawn({
        let app = app.clone();
        async move { resend(&app, &access).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    sqlx::query("UPDATE users SET email_verified = TRUE, verification_token = NULL WHERE email = $1")
        .bind(email)
        .execute(&mut *lock)
        .await
        .unwrap();
    lock.commit().await.unwrap();
    let res = waiting.await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["status"], "already_verified");
}
