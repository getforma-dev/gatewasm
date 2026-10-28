use super::*;
use sha2::{Digest, Sha256};

const CALLBACK: &str = "https://beta.example.test/auth/callback";
const CLIENT: &str = "invited-beta";
const SLUG: &str = "invited";
const EMAIL: &str = "member@example.test";
const PASSWORD: &str = "SyntheticPassword123!";

struct Fixture {
    app: axum::Router,
    pool: PgPool,
    tenant: String,
    key: String,
}
impl Fixture {
    async fn new() -> Self {
        let (mut state, pool) = test_state_with_keys(None).await;
        state.config.compliance.audit_log_enabled = true;
        let app = handlers::create_router_without_rate_limit(state);
        let (tenant, key) = create_test_tenant_with_mgmt_key(&pool, SLUG, "Invited beta").await;
        sqlx::query("UPDATE tenants SET signup_mode='invite_only' WHERE id=$1")
            .bind(&tenant)
            .execute(&pool)
            .await
            .unwrap();
        let r = app
            .clone()
            .oneshot(mgmt_request(
                "POST",
                "/manage/oauth-clients",
                serde_json::json!({"client_id":CLIENT,"name":"Beta","redirect_uri":CALLBACK}),
                &key,
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);
        Self {
            app,
            pool,
            tenant,
            key,
        }
    }
    async fn invite(&self, email: &str, role: &str) -> serde_json::Value {
        let r = self
            .app
            .clone()
            .oneshot(mgmt_request(
                "POST",
                "/manage/invites",
                serde_json::json!({"email":email,"role":role,"delivery_mode":"server_only"}),
                &self.key,
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED);
        let v = body_json(r).await;
        assert!(v.get("invite_url").is_none());
        assert_eq!(v["email_delivery"], "suppressed");
        v
    }
    async fn par(&self, token: Option<&str>) -> axum::response::Response {
        let mut params = vec![
            ("client_id", CLIENT.to_string()),
            ("redirect_uri", CALLBACK.to_string()),
            ("response_type", "code".to_string()),
            ("provider", "email".to_string()),
            ("state", "backend-state".to_string()),
            ("code_challenge_method", "S256".to_string()),
            (
                "code_challenge",
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(Sha256::digest("v".repeat(43).as_bytes())),
            ),
        ];
        if let Some(token) = token {
            params.push(("invite_token", token.into()));
        }
        self.app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/t/{SLUG}/oauth/par"))
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(serde_urlencoded::to_string(params).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    async fn begin_uri(&self, uri: &str) -> axum::response::Response {
        self.app
            .clone()
            .oneshot(get_request(&format!(
                "/t/{SLUG}/auth/authorize?client_id={CLIENT}&request_uri={}",
                urlencoding::encode(uri)
            )))
            .await
            .unwrap()
    }
    async fn begin(&self, token: Option<&str>) -> (String, String) {
        let r = self.par(token).await;
        assert_eq!(r.status(), StatusCode::CREATED);
        assert_eq!(r.headers()["cache-control"], "no-store");
        let uri = body_json(r).await["request_uri"]
            .as_str()
            .unwrap()
            .to_string();
        let r = self.begin_uri(&uri).await;
        assert_eq!(r.status(), StatusCode::TEMPORARY_REDIRECT);
        let cookie = r.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let id = r.headers()["location"]
            .to_str()
            .unwrap()
            .split("authorization_request=")
            .nth(1)
            .unwrap()
            .to_string();
        (id, cookie)
    }
    fn body(id: &str, email: &str) -> serde_json::Value {
        serde_json::json!({"authorization_request":id,"email":email,"password":PASSWORD,"name":"Invited Member"})
    }
    fn req(path: &str, body: serde_json::Value, cookie: Option<&str>) -> Request<Body> {
        let mut r = json_request_no_tenant("POST", path, body);
        if let Some(cookie) = cookie {
            r.headers_mut().insert("cookie", cookie.parse().unwrap());
        }
        r
    }
    async fn register(
        &self,
        id: &str,
        cookie: Option<&str>,
        email: &str,
    ) -> axum::response::Response {
        self.app
            .clone()
            .oneshot(Self::req(
                &format!("/t/{SLUG}/auth/register"),
                Self::body(id, email),
                cookie,
            ))
            .await
            .unwrap()
    }
    async fn counts(&self) -> (i64, i64, i64) {
        let u = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id=$1")
            .bind(&self.tenant)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        let r = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE tenant_id=$1")
            .bind(&self.tenant)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        let c = sqlx::query_scalar("SELECT COUNT(*) FROM auth_codes WHERE tenant_id=$1")
            .bind(&self.tenant)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        (u, r, c)
    }
    async fn exchange(&self, code: &str, verifier: &str) -> axum::response::Response {
        self.app.clone().oneshot(json_request_no_tenant("POST",&format!("/t/{SLUG}/auth/token"),serde_json::json!({"grant_type":"authorization_code","code":code,"code_verifier":verifier,"client_id":CLIENT,"redirect_uri":CALLBACK}))).await.unwrap()
    }
}

#[tokio::test]
async fn invited_registration_is_code_only_bound_and_verifiable() {
    let f = Fixture::new().await;
    let invite = f.invite(EMAIL, "user").await;
    let token = invite["invite_token"].as_str().unwrap();
    let (id, cookie) = f.begin(Some(token)).await;
    let params: serde_json::Value =
        sqlx::query_scalar("SELECT parameters FROM par_requests LIMIT 1")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(!params.to_string().contains(token));
    assert_eq!(params["invite_token_hash"], signing::hash_token(token));
    let mut request = Fixture::body(&id, EMAIL);
    request["role"] = "admin".into();
    request["tenant_id"] = "ten_platform".into();
    let r = f
        .app
        .clone()
        .oneshot(Fixture::req(
            &format!("/t/{SLUG}/auth/register"),
            request,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    assert!(r.headers().get("set-cookie").is_none());
    assert_eq!(r.headers()["cache-control"], "no-store");
    let result = body_json(r).await;
    assert_eq!(result.as_object().unwrap().len(), 1);
    assert!(result.get("redirect_uri").is_some());
    assert_eq!(f.counts().await, (1, 0, 1));
    let redirect = url::Url::parse(result["redirect_uri"].as_str().unwrap()).unwrap();
    assert_eq!(
        format!(
            "{}://{}{}",
            redirect.scheme(),
            redirect.host_str().unwrap(),
            redirect.path()
        ),
        CALLBACK
    );
    let q: std::collections::HashMap<_, _> = redirect.query_pairs().into_owned().collect();
    assert_eq!(q["state"], "backend-state");
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT r.name FROM user_roles ur JOIN roles r ON r.id=ur.role_id WHERE ur.tenant_id=$1",
    )
    .bind(&f.tenant)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(roles, vec!["user"]);
    let accepted: String = sqlx::query_scalar("SELECT status FROM tenant_user_invites WHERE id=$1")
        .bind(invite["invite"]["id"].as_str().unwrap())
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(accepted, "accepted");
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE tenant_id=$1 AND action='user.register'",
    )
    .bind(&f.tenant)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(audits, 1);
    assert_eq!(
        f.exchange(&q["code"], "wrong").await.status(),
        StatusCode::BAD_REQUEST
    );
    let exchange = f.exchange(&q["code"], &"v".repeat(43)).await;
    assert_eq!(exchange.status(), StatusCode::OK);
    let issued = body_json(exchange).await;
    assert_eq!(issued["user"]["email_verified"], false);
    let jwks = body_json(
        f.app
            .clone()
            .oneshot(get_request("/auth/jwks"))
            .await
            .unwrap(),
    )
    .await;
    let jwt = issued["access_token"].as_str().unwrap();
    let header = jsonwebtoken::decode_header(jwt).unwrap();
    let key = jwks["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kid"].as_str() == header.kid.as_deref())
        .unwrap();
    let key = jsonwebtoken::DecodingKey::from_ed_components(key["x"].as_str().unwrap()).unwrap();
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_issuer(&["test"]);
    validation.set_audience(&["test-app"]);
    let claims = jsonwebtoken::decode::<serde_json::Value>(jwt, &key, &validation)
        .unwrap()
        .claims;
    assert_eq!(claims["tenant_id"], f.tenant);
    assert_eq!(claims["roles"], serde_json::json!(["user"]));
    assert!(claims.get("email_verified").is_none());
    assert_eq!(
        f.exchange(&q["code"], &"v".repeat(43)).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.register(&id, Some(&cookie), EMAIL).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn refused_registration_leaves_no_identity_or_code() {
    let f = Fixture::new().await;
    let invite = f.invite(EMAIL, "user").await;
    let (id, cookie) = f.begin(invite["invite_token"].as_str()).await;
    for supplied in [
        None,
        Some("fauth_authorization=short"),
        Some("fauth_authorization=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
    ] {
        assert_eq!(
            f.register(&id, supplied, EMAIL).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        f.register("", Some(&cookie), EMAIL).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.register(&id, Some(&cookie), "wrong@example.test")
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    for (field, value) in [
        ("password", "weak"),
        ("invite_token", invite["invite_token"].as_str().unwrap()),
    ] {
        let mut b = Fixture::body(&id, EMAIL);
        b[field] = value.into();
        assert_eq!(
            f.app
                .clone()
                .oneshot(Fixture::req(
                    &format!("/t/{SLUG}/auth/register"),
                    b,
                    Some(&cookie)
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    create_test_tenant_in_db(&f.pool, "other", "Other").await;
    let r = f
        .app
        .clone()
        .oneshot(Fixture::req(
            "/t/other/auth/register",
            Fixture::body(&id, EMAIL),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    sqlx::query("UPDATE tenants SET security_config=jsonb_set(security_config,'{mfa_policy}','\"required\"') WHERE id=$1").bind(&f.tenant).execute(&f.pool).await.unwrap();
    assert_eq!(
        f.register(&id, Some(&cookie), EMAIL).await.status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE tenants SET security_config='{}' WHERE id=$1")
        .bind(&f.tenant)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE hosted_authorization_requests SET expires_at=NOW()-INTERVAL '1 second' WHERE id=$1",
    )
    .bind(&id)
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        f.register(&id, Some(&cookie), EMAIL).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.counts().await, (0, 0, 0));
    let status: String = sqlx::query_scalar("SELECT status FROM tenant_user_invites WHERE id=$1")
        .bind(invite["invite"]["id"].as_str().unwrap())
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");
}

#[tokio::test]
async fn invitation_is_required_even_on_open_tenant_and_rechecked_after_begin() {
    let f = Fixture::new().await;
    sqlx::query("UPDATE tenants SET signup_mode='open' WHERE id=$1")
        .bind(&f.tenant)
        .execute(&f.pool)
        .await
        .unwrap();
    let (id, cookie) = f.begin(None).await;
    assert_eq!(
        f.register(&id, Some(&cookie), EMAIL).await.status(),
        StatusCode::BAD_REQUEST
    );
    for failure in [
        "revoked",
        "expired",
        "accepted",
        "foreign-role",
        "privileged-role",
    ] {
        let invite = f.invite(EMAIL, "user").await;
        let (id, cookie) = f.begin(invite["invite_token"].as_str()).await;
        if failure == "expired" {
            sqlx::query(
                "UPDATE tenant_user_invites SET expires_at=NOW()-INTERVAL '1 second' WHERE id=$1",
            )
            .bind(invite["invite"]["id"].as_str().unwrap())
            .execute(&f.pool)
            .await
            .unwrap();
        } else if failure == "privileged-role" {
            sqlx::query("UPDATE tenant_user_invites SET role_id='role_platform_owner' WHERE id=$1")
                .bind(invite["invite"]["id"].as_str().unwrap())
                .execute(&f.pool)
                .await
                .unwrap();
        } else if failure == "foreign-role" {
            sqlx::query("INSERT INTO roles(id,name,tenant_id) VALUES ('foreign-role','admin','other-tenant')").execute(&f.pool).await.unwrap();
            sqlx::query("UPDATE tenant_user_invites SET role_id='foreign-role' WHERE id=$1")
                .bind(invite["invite"]["id"].as_str().unwrap())
                .execute(&f.pool)
                .await
                .unwrap();
        } else {
            sqlx::query("UPDATE tenant_user_invites SET status=$1 WHERE id=$2")
                .bind(failure)
                .bind(invite["invite"]["id"].as_str().unwrap())
                .execute(&f.pool)
                .await
                .unwrap();
        }
        assert_eq!(
            f.register(&id, Some(&cookie), EMAIL).await.status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            f.par(invite["invite_token"].as_str()).await.status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(f.counts().await, (0, 0, 0));
    }
}

#[tokio::test]
async fn concurrent_registration_and_capacity_are_serialized() {
    let f = Fixture::new().await;
    let invite = f.invite(EMAIL, "user").await;
    let (id, cookie) = f.begin(invite["invite_token"].as_str()).await;
    let (a, b) = tokio::join!(
        f.register(&id, Some(&cookie), EMAIL),
        f.register(&id, Some(&cookie), EMAIL)
    );
    let mut statuses = [a.status().as_u16(), b.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, [201, 400]);
    assert_eq!(f.counts().await, (1, 0, 1));
    let f = Fixture::new().await;
    sqlx::query("UPDATE tenant_entitlements SET max_users=1 WHERE tenant_id=$1")
        .bind(&f.tenant)
        .execute(&f.pool)
        .await
        .unwrap();
    let a = f.invite("a@example.test", "user").await;
    let b = f.invite("b@example.test", "user").await;
    let (aid, ac) = f.begin(a["invite_token"].as_str()).await;
    let (bid, bc) = f.begin(b["invite_token"].as_str()).await;
    let (a, b) = tokio::join!(
        f.register(&aid, Some(&ac), "a@example.test"),
        f.register(&bid, Some(&bc), "b@example.test")
    );
    let mut statuses = [a.status().as_u16(), b.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, [201, 403]);
    assert_eq!(f.counts().await, (1, 0, 1));
}

#[tokio::test]
async fn duplicate_can_sign_in_and_invite_login_is_atomic() {
    let f = Fixture::new().await;
    let r = f
        .app
        .clone()
        .oneshot(mgmt_request(
            "POST",
            "/manage/users",
            serde_json::json!({"email":EMAIL,"password":PASSWORD,"role":"user"}),
            &f.key,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let user = body_json(r).await["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let invite = f.invite(EMAIL, "admin").await;
    let (id, cookie) = f.begin(invite["invite_token"].as_str()).await;
    assert_eq!(
        f.register(&id, Some(&cookie), EMAIL).await.status(),
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE users SET login_attempts=2 WHERE id=$1")
        .bind(&user)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tenant_user_invites SET status='revoked' WHERE id=$1")
        .bind(invite["invite"]["id"].as_str().unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    let r = f
        .app
        .clone()
        .oneshot(Fixture::req(
            &format!("/t/{SLUG}/auth/login"),
            Fixture::body(&id, EMAIL),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let attempts: i32 = sqlx::query_scalar("SELECT login_attempts FROM users WHERE id=$1")
        .bind(&user)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(attempts, 2);
    let admin: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_roles WHERE user_id=$1 AND role_id='role_admin'",
    )
    .bind(&user)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(admin, 0);
    sqlx::query("UPDATE tenant_user_invites SET status='pending' WHERE id=$1")
        .bind(invite["invite"]["id"].as_str().unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    let mut raw = Fixture::body(&id, EMAIL);
    raw["invite_token"] = invite["invite_token"].clone();
    assert_eq!(
        f.app
            .clone()
            .oneshot(Fixture::req(
                &format!("/t/{SLUG}/auth/login"),
                raw,
                Some(&cookie)
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let r = f
        .app
        .clone()
        .oneshot(Fixture::req(
            &format!("/t/{SLUG}/auth/login"),
            Fixture::body(&id, EMAIL),
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(r.headers().get("set-cookie").is_none());
    assert_eq!(body_json(r).await.as_object().unwrap().len(), 1);
    let admin: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_roles WHERE user_id=$1 AND role_id='role_admin'",
    )
    .bind(&user)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(admin, 1);
    assert_eq!(f.counts().await, (1, 0, 1));
}

#[tokio::test]
async fn par_binding_cap_and_rollback_preserve_usable_requests() {
    let f = Fixture::new().await;
    let invite = f.invite(EMAIL, "user").await;
    let par = f.par(invite["invite_token"].as_str()).await;
    let uri = body_json(par).await["request_uri"]
        .as_str()
        .unwrap()
        .to_string();
    create_test_tenant_in_db(&f.pool, "other", "Other").await;
    for (slug, client) in [(SLUG, "wrong"), ("other", CLIENT)] {
        let r = f
            .app
            .clone()
            .oneshot(get_request(&format!(
                "/t/{slug}/auth/authorize?client_id={client}&request_uri={}",
                urlencoding::encode(&uri)
            )))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }
    sqlx::query("INSERT INTO hosted_authorization_requests(id,tenant_id,client_id,redirect_uri,client_state,code_challenge,browser_hash,expires_at) SELECT 'flood-'||n,$1,$2,$3,'state','challenge','hash',NOW()+INTERVAL '1 minute' FROM generate_series(1,1000)n").bind(&f.tenant).bind(CLIENT).bind(CALLBACK).execute(&f.pool).await.unwrap();
    assert_eq!(
        f.begin_uri(&uri).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let consumed: bool =
        sqlx::query_scalar("SELECT consumed FROM par_requests WHERE request_uri=$1")
            .bind(&uri)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(!consumed);
    sqlx::query("DELETE FROM hosted_authorization_requests WHERE id LIKE 'flood-%'")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        f.begin_uri(&uri).await.status(),
        StatusCode::TEMPORARY_REDIRECT
    );
    assert_eq!(f.begin_uri(&uri).await.status(), StatusCode::BAD_REQUEST);
    sqlx::query("INSERT INTO par_requests(request_uri,client_id,parameters,tenant_id,expires_at) SELECT 'flood-'||n,$1,'{}',$2,NOW()+INTERVAL '1 minute' FROM generate_series(1,1000)n").bind(CLIENT).bind(&f.tenant).execute(&f.pool).await.unwrap();
    assert_eq!(
        f.par(invite["invite_token"].as_str()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(f.counts().await, (0, 0, 0));
}

#[tokio::test]
async fn server_only_invitation_suppresses_real_transport_and_validates_before_writes() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let messages = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let capture = messages.clone();
    let smtp = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let capture = capture.clone();
            tokio::spawn(async move {
                let mut stream = BufReader::new(stream);
                stream
                    .get_mut()
                    .write_all(b"220 localhost SMTP\r\n")
                    .await
                    .unwrap();
                let mut data = false;
                let mut message = String::new();
                loop {
                    let mut line = String::new();
                    if stream.read_line(&mut line).await.unwrap() == 0 {
                        break;
                    }
                    let answer = if data {
                        if line == ".\r\n" {
                            capture.lock().unwrap().push(message.clone());
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
    let (mut state, pool) = test_state_with_keys(None).await;
    state.config.suppress_email = false;
    state.config.smtp_host = Some("127.0.0.1".into());
    state.config.smtp_port = port;
    state.config.smtp_starttls = false;
    state.config.smtp_from_email = Some("sender@example.test".into());
    let app = handlers::create_router_without_rate_limit(state);
    let (tenant, key) = create_test_tenant_with_mgmt_key(&pool, "mail-test", "Mail Test").await;
    let call = |body| {
        app.clone()
            .oneshot(mgmt_request("POST", "/manage/invites", body, &key))
    };
    let r = call(serde_json::json!({"email":EMAIL,"role":"user","delivery_mode":"server_only"}))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let only = body_json(r).await;
    assert!(only.get("invite_url").is_none());
    assert_eq!(only["email_delivery"], "suppressed");
    assert!(messages.lock().unwrap().is_empty());
    for body in [
        serde_json::json!({"email":EMAIL,"role":"user","delivery_mode":"typo"}),
        serde_json::json!({"email":EMAIL,"delivery_mode":"server_only"}),
        serde_json::json!({"email":EMAIL,"role":"platform_owner","delivery_mode":"server_only"}),
    ] {
        assert_eq!(call(body).await.unwrap().status(), StatusCode::BAD_REQUEST);
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenant_user_invites WHERE tenant_id=$1 AND status='pending'",
    )
    .bind(&tenant)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let r = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        call(serde_json::json!({"email":"legacy@example.test","role":"user"})),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let legacy = body_json(r).await;
    assert_eq!(legacy["email_delivery"], "sent");
    assert!(legacy["invite_url"].is_string());
    assert_eq!(messages.lock().unwrap().len(), 1);
    assert!(messages.lock().unwrap()[0].contains(legacy["invite_token"].as_str().unwrap()));
    smtp.abort();
}

#[tokio::test]
async fn expiry_during_row_lock_wait_cannot_create_identity_or_hosted_request() {
    for boundary in ["par", "request", "invitation"] {
        let f = Fixture::new().await;
        let invite = f.invite(EMAIL, "user").await;
        let invite_id = invite["invite"]["id"].as_str().unwrap();
        let (id, cookie) = if boundary == "par" {
            let p = f.par(invite["invite_token"].as_str()).await;
            (
                body_json(p).await["request_uri"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                String::new(),
            )
        } else {
            f.begin(invite["invite_token"].as_str()).await
        };
        let expires = chrono::Utc::now() + chrono::Duration::seconds(2);
        let (table, column, value) = match boundary {
            "par" => ("par_requests", "request_uri", id.as_str()),
            "request" => ("hosted_authorization_requests", "id", id.as_str()),
            _ => ("tenant_user_invites", "id", invite_id),
        };
        sqlx::query(&format!(
            "UPDATE {table} SET expires_at=$1 WHERE {column}=$2"
        ))
        .bind(expires)
        .bind(value)
        .execute(&f.pool)
        .await
        .unwrap();
        let mut blocker = f.pool.begin().await.unwrap();
        let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *blocker)
            .await
            .unwrap();
        if boundary == "invitation" {
            sqlx::query("SELECT id FROM tenant_user_invites WHERE id=$1 FOR UPDATE")
                .bind(invite_id)
                .execute(&mut *blocker)
                .await
                .unwrap();
        } else {
            sqlx::query("SELECT id FROM tenants WHERE id=$1 FOR UPDATE")
                .bind(&f.tenant)
                .execute(&mut *blocker)
                .await
                .unwrap();
        }
        let app = f.app.clone();
        let request = if boundary == "par" {
            get_request(&format!(
                "/t/{SLUG}/auth/authorize?client_id={CLIENT}&request_uri={}",
                urlencoding::encode(&id)
            ))
        } else {
            Fixture::req(
                &format!("/t/{SLUG}/auth/register"),
                Fixture::body(&id, EMAIL),
                Some(&cookie),
            )
        };
        let pending = tokio::spawn(async move { app.oneshot(request).await.unwrap() });
        tokio::time::timeout(std::time::Duration::from_secs(1),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))").bind(blocker_pid).fetch_one(&f.pool).await.unwrap();
            if blocked {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }}).await.expect("request must be waiting on our lock before expiry");
        assert!(chrono::Utc::now() < expires);
        let delay = (expires - chrono::Utc::now()).to_std().unwrap()
            + std::time::Duration::from_millis(100);
        tokio::time::sleep(delay).await;
        blocker.commit().await.unwrap();
        assert_eq!(pending.await.unwrap().status(), StatusCode::BAD_REQUEST);
        assert_eq!(f.counts().await, (0, 0, 0));
        if boundary == "par" {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM hosted_authorization_requests WHERE tenant_id=$1",
            )
            .bind(&f.tenant)
            .fetch_one(&f.pool)
            .await
            .unwrap();
            assert_eq!(count, 0);
        }
    }
}
