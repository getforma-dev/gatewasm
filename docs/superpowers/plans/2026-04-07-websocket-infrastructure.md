# WebSocket Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real-time invalidation layer to fAuth using Axum-native WebSockets with Tokio broadcast channels, enabling live dashboard updates for team changes, tenant activity, and metrics.

**Architecture:** Single global `broadcast::channel` in `AppState`. Existing handlers emit fire-and-forget events after mutations. Clients authenticate via ticket-based handshake, subscribe to topics, and refetch data through existing HTTP APIs when invalidated. One new Rust crate (`dashmap`). One new TypeScript file (`createSocket.ts`).

**Tech Stack:** Rust/Axum (WS via tungstenite), Tokio broadcast channels, DashMap for ticket store, FormaJS signals for client state.

**Spec:** `docs/superpowers/specs/2026-04-07-websocket-infrastructure-design.md`

---

## File Map

### New Files

| File | Responsibility |
|------|---------------|
| `crates/server/src/ws/mod.rs` | `EventBus`, `WsEvent` structs, module re-exports |
| `crates/server/src/ws/ticket.rs` | `TicketStore`, `TicketInfo`, `POST /ws/ticket` handler |
| `crates/server/src/ws/handler.rs` | `GET /ws` upgrade, connection loop, topic filtering |
| `crates/server/src/ws/events.rs` | `emit_activity()`, `emit_team_changed()` helper functions |
| `admin/src/core/hooks/createSocket.ts` | Client-side WS hook with reconnect, debounce, subscribe API |

### Modified Files

| File | Change |
|------|--------|
| `crates/server/Cargo.toml` | Add `dashmap` dependency |
| `crates/server/src/lib.rs` | Add `pub mod ws;`, add `event_bus` and `ticket_store` to `AppState` |
| `crates/server/src/main.rs` | Construct `EventBus` + `TicketStore`, add to `AppState` |
| `crates/server/src/handlers/mod.rs` | Register `/ws/ticket` and `/ws` routes, add `pub mod ws` usage |
| `crates/server/src/handlers/auth.rs` | Add `emit_activity()` calls in `register` and `login` |
| `crates/server/src/handlers/owner/team.rs` | Add `emit_team_changed()` calls in `update_team_member` and `remove_team_member` |
| `crates/server/src/handlers/owner/invites.rs` | Add `emit_team_changed()` calls in `create_owner_invite`, `revoke_owner_invite`, `accept_owner_invite` |
| `crates/server/src/handlers/management.rs` | Add `emit_activity()` calls in `block_user`, `unblock_user`, `delete_user` |
| `crates/server/src/handlers/mfa.rs` | Add `emit_activity()` call in `verify` |
| `crates/server/src/handlers/webauthn.rs` | Add `emit_activity()` call in `register_complete` |
| `admin/src/core/hooks/index.ts` | Re-export `createSocket` and `Socket` type |
| `admin/src/platform/console/store.ts` | Add socket subscriptions for `team:changed`, `metrics:updated`, `tenant:activity` |
| `admin/src/platform/console/app.ts` | Add `socket.connect()` after auth init |
| `admin/src/tenant/admin/store.ts` | Add socket subscription for `tenant:activity` |
| `admin/src/tenant/admin/app.ts` | Add `socket.connect()` after auth init |

---

## Task 1: Add `dashmap` Dependency and `ws` Module Skeleton

**Files:**
- Modify: `crates/server/Cargo.toml:8-43`
- Create: `crates/server/src/ws/mod.rs`
- Modify: `crates/server/src/lib.rs:1-17`

- [ ] **Step 1: Add `dashmap` to Cargo.toml**

In `crates/server/Cargo.toml`, add after line 11 (after the `axum` dependency):

```toml
dashmap = "6"
```

- [ ] **Step 2: Create `ws/mod.rs` with EventBus and WsEvent**

Create `crates/server/src/ws/mod.rs`:

```rust
pub mod events;
pub mod handler;
pub mod ticket;

use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
pub struct WsEvent {
    pub tenant_id: Uuid,
    pub topic: String,
    pub hint: Option<String>,
    pub timestamp: i64,
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<WsEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self { sender }
    }

    pub fn send(&self, event: WsEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsEvent> {
        self.sender.subscribe()
    }
}
```

- [ ] **Step 3: Register the `ws` module in lib.rs**

In `crates/server/src/lib.rs`, add after line 16 (`pub mod types;`):

```rust
pub mod ws;
```

- [ ] **Step 4: Add `event_bus` and `ticket_store` to AppState**

In `crates/server/src/lib.rs`, add to the imports after line 26 (`use middleware::tenant::TenantCache;`):

```rust
use ws::EventBus;
use ws::ticket::TicketStore;
```

Add two fields to the `AppState` struct (after line 42, before the closing `}`):

```rust
    pub event_bus: EventBus,
    pub ticket_store: TicketStore,
```

- [ ] **Step 5: Verify it compiles (expect errors for missing files)**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | head -20`

Expected: Errors about missing `events`, `handler`, `ticket` modules — that's correct, we'll create them in the next tasks.

- [ ] **Step 6: Commit**

```bash
git add crates/server/Cargo.toml crates/server/src/lib.rs crates/server/src/ws/mod.rs
git commit -m "feat(ws): add dashmap dependency and ws module skeleton with EventBus"
```

---

## Task 2: Implement TicketStore and `/ws/ticket` Handler

**Files:**
- Create: `crates/server/src/ws/ticket.rs`

- [ ] **Step 1: Create `ws/ticket.rs`**

Create `crates/server/src/ws/ticket.rs`:

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{extract::State, response::IntoResponse, Json};
use dashmap::DashMap;
use rand::RngCore;
use uuid::Uuid;

use crate::error::AppError;
use crate::AppState;

const TICKET_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct TicketInfo {
    pub account_id: String,
    pub tenant_id: String,
    pub jwt_exp: i64,
    pub created_at: Instant,
}

#[derive(Clone)]
pub struct TicketStore {
    tickets: Arc<DashMap<String, TicketInfo>>,
}

impl TicketStore {
    pub fn new() -> Self {
        Self {
            tickets: Arc::new(DashMap::new()),
        }
    }

    pub fn create(&self, info: TicketInfo) -> String {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let ticket = hex::encode(bytes);
        self.tickets.insert(ticket.clone(), info);
        ticket
    }

    /// Look up and burn (single-use). Returns None if missing or expired.
    pub fn take(&self, ticket: &str) -> Option<TicketInfo> {
        self.tickets.remove(ticket).and_then(|(_, info)| {
            if info.created_at.elapsed() < TICKET_TTL {
                Some(info)
            } else {
                None
            }
        })
    }
}

/// POST /ws/ticket — Create a single-use WebSocket ticket.
/// Requires JWT authentication (reuses the auth middleware on the route).
pub async fn create_ticket(
    State(state): State<AppState>,
    req: axum::extract::Request,
) -> Result<impl IntoResponse, AppError> {
    // Extract identity from the JWT claims set by auth middleware.
    let claims = req
        .extensions()
        .get::<crate::jwt::signing::Claims>()
        .cloned()
        .ok_or(AppError::Unauthorized {
            error: "unauthorized",
            message: "Authentication required".into(),
        })?;

    let ticket = state.ticket_store.create(TicketInfo {
        account_id: claims.sub.clone(),
        tenant_id: claims.tenant_id.clone(),
        jwt_exp: claims.exp,
        created_at: Instant::now(),
    });

    Ok(Json(serde_json::json!({ "ticket": ticket })))
}
```

- [ ] **Step 2: Verify the file compiles in isolation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | head -20`

Expected: Errors about missing `handler` and `events` modules only. The `ticket` module should compile (verify no errors mentioning `ticket.rs`).

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/ws/ticket.rs
git commit -m "feat(ws): implement TicketStore and POST /ws/ticket handler"
```

---

## Task 3: Implement Emit Helper Functions

**Files:**
- Create: `crates/server/src/ws/events.rs`

- [ ] **Step 1: Create `ws/events.rs`**

Create `crates/server/src/ws/events.rs`:

```rust
use uuid::Uuid;

use super::{EventBus, WsEvent};

/// Emits both `tenant:activity` and `metrics:updated` in one call.
/// Called from auth handlers after login, signup, block, etc.
pub fn emit_activity(bus: &EventBus, tenant_id: &str, hint: &str) {
    let tid = Uuid::parse_str(tenant_id).unwrap_or_default();
    let now = chrono::Utc::now().timestamp_millis();
    bus.send(WsEvent {
        tenant_id: tid,
        topic: "tenant:activity".into(),
        hint: Some(hint.into()),
        timestamp: now,
    });
    bus.send(WsEvent {
        tenant_id: tid,
        topic: "metrics:updated".into(),
        hint: None,
        timestamp: now,
    });
}

/// Emits `team:changed`. Called from team/invite handlers.
pub fn emit_team_changed(bus: &EventBus, tenant_id: &str, hint: &str) {
    let tid = Uuid::parse_str(tenant_id).unwrap_or_default();
    let now = chrono::Utc::now().timestamp_millis();
    bus.send(WsEvent {
        tenant_id: tid,
        topic: "team:changed".into(),
        hint: Some(hint.into()),
        timestamp: now,
    });
}
```

Note: `tenant_id` is `&str` because the existing handlers use `String` tenant IDs (not `Uuid`). The parse handles the conversion.

- [ ] **Step 2: Verify compilation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | head -20`

Expected: Only error should be about missing `handler` module.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/ws/events.rs
git commit -m "feat(ws): add emit_activity() and emit_team_changed() helper functions"
```

---

## Task 4: Implement WS Upgrade Handler and Connection Loop

**Files:**
- Create: `crates/server/src/ws/handler.rs`

- [ ] **Step 1: Create `ws/handler.rs`**

Create `crates/server/src/ws/handler.rs`:

```rust
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::IntoResponse,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::time::Instant;

use crate::AppState;

const VALID_TOPICS: &[&str] = &["team:changed", "tenant:activity", "metrics:updated"];
const PING_INTERVAL: Duration = Duration::from_secs(30);
const MAX_INVALID_MESSAGES: u8 = 5;

#[derive(Deserialize)]
pub struct WsParams {
    ticket: String,
}

/// Connection limit tracker.
#[derive(Clone)]
pub struct ConnectionLimits {
    per_tenant: Arc<DashMap<String, AtomicUsize>>,
    global: Arc<AtomicUsize>,
}

impl ConnectionLimits {
    pub fn new() -> Self {
        Self {
            per_tenant: Arc::new(DashMap::new()),
            global: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn try_acquire(&self, tenant_id: &str) -> bool {
        let global = self.global.fetch_add(1, Ordering::Relaxed);
        if global >= 500 {
            self.global.fetch_sub(1, Ordering::Relaxed);
            return false;
        }

        let entry = self.per_tenant.entry(tenant_id.to_string()).or_insert_with(|| AtomicUsize::new(0));
        let count = entry.value().fetch_add(1, Ordering::Relaxed);
        if count >= 50 {
            entry.value().fetch_sub(1, Ordering::Relaxed);
            self.global.fetch_sub(1, Ordering::Relaxed);
            return false;
        }

        true
    }

    pub fn release(&self, tenant_id: &str) {
        self.global.fetch_sub(1, Ordering::Relaxed);
        if let Some(entry) = self.per_tenant.get(tenant_id) {
            entry.value().fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub fn active_count(&self) -> usize {
        self.global.load(Ordering::Relaxed)
    }
}

/// GET /ws?ticket=<ticket> — Upgrade to WebSocket.
pub async fn ws_upgrade(
    State(state): State<AppState>,
    Query(params): Query<WsParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let info = match state.ticket_store.take(&params.ticket) {
        Some(info) => info,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    if !state.connection_limits.try_acquire(&info.tenant_id) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    let tenant_id = info.tenant_id.clone();
    tracing::info!(
        tenant_id = %tenant_id,
        account_id = %info.account_id,
        "WebSocket connection upgrading"
    );

    ws.on_upgrade(move |socket| handle_connection(socket, state, info))
        .into_response()
}

async fn handle_connection(
    socket: WebSocket,
    state: AppState,
    info: super::ticket::TicketInfo,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut bus_rx = state.event_bus.subscribe();
    let mut subscribed_topics: HashSet<String> = HashSet::new();
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    let mut invalid_msg_count: u8 = 0;

    // Kill connection when the original JWT would have expired.
    let secs_until_expiry = (info.jwt_exp - chrono::Utc::now().timestamp()).max(0) as u64;
    let token_expiry = tokio::time::sleep_until(Instant::now() + Duration::from_secs(secs_until_expiry));
    tokio::pin!(token_expiry);

    loop {
        tokio::select! {
            result = bus_rx.recv() => {
                match result {
                    Ok(event) => {
                        if event.tenant_id.to_string() == info.tenant_id
                            && subscribed_topics.contains(&event.topic)
                        {
                            let msg = serde_json::json!({
                                "type": "event",
                                "topic": event.topic,
                                "tenant_id": event.tenant_id,
                                "hint": event.hint,
                            });
                            if ws_tx.send(Message::Text(msg.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(tenant_id = %info.tenant_id, lagged = n, "WS receiver lagged");
                        // Continue — client will get the next events
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match parse_client_message(&text) {
                            Some(ClientMsg::Subscribe(topics)) => {
                                for t in topics {
                                    if VALID_TOPICS.contains(&t.as_str()) {
                                        subscribed_topics.insert(t);
                                    }
                                }
                            }
                            Some(ClientMsg::Unsubscribe(topics)) => {
                                for t in topics {
                                    subscribed_topics.remove(&t);
                                }
                            }
                            Some(ClientMsg::Ping) => {
                                let _ = ws_tx.send(Message::Text(r#"{"type":"pong"}"#.into())).await;
                            }
                            None => {
                                invalid_msg_count += 1;
                                if invalid_msg_count >= MAX_INVALID_MESSAGES {
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // Ignore binary, ping, pong frames
                }
            }

            _ = ping_interval.tick() => {
                if ws_tx.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }

            _ = &mut token_expiry => {
                let _ = ws_tx.send(Message::Text(r#"{"type":"auth_expired"}"#.into())).await;
                break;
            }
        }
    }

    state.connection_limits.release(&info.tenant_id);
    tracing::info!(
        tenant_id = %info.tenant_id,
        account_id = %info.account_id,
        "WebSocket connection closed"
    );
}

// ── Client message parsing ───────────────────────────────────────────────

enum ClientMsg {
    Subscribe(Vec<String>),
    Unsubscribe(Vec<String>),
    Ping,
}

fn parse_client_message(text: &str) -> Option<ClientMsg> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let msg_type = v.get("type")?.as_str()?;
    match msg_type {
        "subscribe" => {
            let topics = v.get("topics")?
                .as_array()?
                .iter()
                .filter_map(|t| t.as_str().map(String::from))
                .collect();
            Some(ClientMsg::Subscribe(topics))
        }
        "unsubscribe" => {
            let topics = v.get("topics")?
                .as_array()?
                .iter()
                .filter_map(|t| t.as_str().map(String::from))
                .collect();
            Some(ClientMsg::Unsubscribe(topics))
        }
        "ping" => Some(ClientMsg::Ping),
        _ => None,
    }
}
```

- [ ] **Step 2: Add `futures-util` dependency if not already present**

Check if `futures-util` is in Cargo.toml. If not, add it:

```toml
futures-util = "0.3"
```

This is needed for `SinkExt` and `StreamExt` on the split WebSocket.

- [ ] **Step 3: Verify full compilation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | tail -5`

Expected: Compilation should succeed (all four `ws` submodules now exist). If there are type mismatches with axum's `Message` type, adjust — axum 0.8 may use `String` vs `Utf8Bytes` for `Message::Text`. Fix any such issues.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/ws/handler.rs crates/server/Cargo.toml
git commit -m "feat(ws): implement WS upgrade handler with connection loop and topic filtering"
```

---

## Task 5: Wire Up AppState and Routes

**Files:**
- Modify: `crates/server/src/main.rs:63-71`
- Modify: `crates/server/src/handlers/mod.rs:60-681`

- [ ] **Step 1: Construct EventBus, TicketStore, and ConnectionLimits in main.rs**

In `crates/server/src/main.rs`, add to the imports (around line 16):

```rust
use gatewasm_server::ws::{EventBus, handler::ConnectionLimits};
use gatewasm_server::ws::ticket::TicketStore;
```

Then in the `AppState` construction block (lines 63-71), add the new fields:

```rust
    let state = AppState {
        db: pool,
        config: config.clone(),
        keys,
        tenant_cache: TenantCache::default(),
        asset_manifest,
        render_modes,
        ir_modules,
        event_bus: EventBus::new(),
        ticket_store: TicketStore::new(),
        connection_limits: ConnectionLimits::new(),
    };
```

This also requires adding `connection_limits` to the `AppState` struct in `lib.rs`:

```rust
    pub connection_limits: crate::ws::handler::ConnectionLimits,
```

- [ ] **Step 2: Register WS routes in handlers/mod.rs**

In `crates/server/src/handlers/mod.rs`, add two routes. `/ws/ticket` needs JWT auth middleware; `/ws` does not (ticket-based auth).

Create a protected route group for the ticket endpoint. Add this near the other route definitions (around line 68, after the existing `let protected = Router::new()` block):

```rust
    // ── WebSocket ticket (auth-protected) ──────────────────────────
    let ws_ticket_route = Router::new()
        .route("/ws/ticket", post(crate::ws::ticket::create_ticket))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));
```

Then merge `ws_ticket_route` into `main_router` (near line 640, alongside the other `.merge()` calls):

```rust
        .merge(ws_ticket_route)
```

Add the unprotected WS upgrade route directly to `main_router` (near line 657, before `.route("/health", get(health))`):

```rust
        .route("/ws", get(crate::ws::handler::ws_upgrade))
```

- [ ] **Step 3: Verify compilation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | tail -5`

Expected: Clean compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/main.rs crates/server/src/lib.rs crates/server/src/handlers/mod.rs
git commit -m "feat(ws): wire EventBus, TicketStore, and WS routes into AppState and router"
```

---

## Task 6: Add Health Check WS Connection Count

**Files:**
- Modify: `crates/server/src/handlers/mod.rs:829-839`

- [ ] **Step 1: Add `ws_connections` to the health endpoint**

In `crates/server/src/handlers/mod.rs`, update the `health` function (lines 829-839):

Replace:
```rust
    Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "db_status": if db_ok { "ok" } else { "error" },
    }))
```

With:
```rust
    Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "db_status": if db_ok { "ok" } else { "error" },
        "ws_connections": state.connection_limits.active_count(),
    }))
```

- [ ] **Step 2: Verify compilation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | tail -5`

Expected: Clean compilation.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/handlers/mod.rs
git commit -m "feat(ws): expose ws_connections count in /health endpoint"
```

---

## Task 7: Add Emit Calls to Auth Handlers

**Files:**
- Modify: `crates/server/src/handlers/auth.rs:826-827` (register)
- Modify: `crates/server/src/handlers/auth.rs:1442-1443` (login)

- [ ] **Step 1: Add emit to `register` handler**

In `crates/server/src/handlers/auth.rs`, find the line just before the final `Ok((` in the `register` function (around line 826, after `append_refresh_cookie_headers`).

Add before `Ok((`:

```rust
    crate::ws::events::emit_activity(&state.event_bus, &tenant_id, "signup");
```

The `tenant_id` variable is already in scope (it's used throughout the handler).

- [ ] **Step 2: Add emit to `login` handler**

In `crates/server/src/handlers/auth.rs`, find the line just before the final `Ok((` in the `login` function (around line 1442, after `append_refresh_cookie_headers`).

Add before `Ok((`:

```rust
    crate::ws::events::emit_activity(&state.event_bus, &tenant_id, "login");
```

- [ ] **Step 3: Verify compilation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | tail -5`

Expected: Clean compilation. If `tenant_id` is a different type than `&str`, adjust the emit call (e.g., `&tenant_id` or `tenant_id.as_str()`).

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/handlers/auth.rs
git commit -m "feat(ws): emit tenant:activity on login and signup"
```

---

## Task 8: Add Emit Calls to Team and Invite Handlers

**Files:**
- Modify: `crates/server/src/handlers/owner/team.rs:319-321` (update_team_member)
- Modify: `crates/server/src/handlers/owner/team.rs:369-371` (remove_team_member)
- Modify: `crates/server/src/handlers/owner/invites.rs:402-406` (create_owner_invite)
- Modify: `crates/server/src/handlers/owner/invites.rs:435-440` (revoke_owner_invite)
- Modify: `crates/server/src/handlers/owner/invites.rs:622-630` (accept_owner_invite)

- [ ] **Step 1: Add emit to `update_team_member`**

In `crates/server/src/handlers/owner/team.rs`, before `Ok(Json(serde_json::json!({ "success": true })))` at line 321, add:

```rust
    crate::ws::events::emit_team_changed(&state.event_bus, "ten_platform", "member_updated");
```

Note: Team operations are platform-scoped, so the tenant_id is `"ten_platform"`. The `state` is already available via the `State(state)` extractor.

- [ ] **Step 2: Add emit to `remove_team_member`**

In the same file, before `Ok(Json(serde_json::json!({ "success": true })))` at line 371, add:

```rust
    crate::ws::events::emit_team_changed(&state.event_bus, "ten_platform", "member_removed");
```

- [ ] **Step 3: Add emit to `create_owner_invite`**

In `crates/server/src/handlers/owner/invites.rs`, before the final `Ok((StatusCode::CREATED, ...))` response (around line 402), add:

```rust
    crate::ws::events::emit_team_changed(&state.event_bus, "ten_platform", "invite_created");
```

- [ ] **Step 4: Add emit to `revoke_owner_invite`**

In the same file, before `Ok(Json(serde_json::json!({...})))` at line 435, add:

```rust
    crate::ws::events::emit_team_changed(&state.event_bus, "ten_platform", "invite_revoked");
```

- [ ] **Step 5: Add emit to `accept_owner_invite`**

In the same file, after `tx.commit().await?;` at line 622 and before the final `Ok(Json(...))`, add:

```rust
    crate::ws::events::emit_team_changed(&state.event_bus, "ten_platform", "invite_accepted");
```

- [ ] **Step 6: Verify compilation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | tail -5`

Expected: Clean compilation.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/handlers/owner/team.rs crates/server/src/handlers/owner/invites.rs
git commit -m "feat(ws): emit team:changed on invite and member mutations"
```

---

## Task 9: Add Emit Calls to Management, MFA, and WebAuthn Handlers

**Files:**
- Modify: `crates/server/src/handlers/management.rs:778-781` (delete_user)
- Modify: `crates/server/src/handlers/management.rs:816-818` (block_user)
- Modify: `crates/server/src/handlers/management.rs:846-848` (unblock_user)
- Modify: `crates/server/src/handlers/mfa.rs:714-716` (verify)
- Modify: `crates/server/src/handlers/webauthn.rs:1056-1058` (register_complete)

- [ ] **Step 1: Add emit to `delete_user`**

In `crates/server/src/handlers/management.rs`, after the webhook dispatch (line 778) and before `Ok(StatusCode::NO_CONTENT)` at line 781, add:

```rust
    crate::ws::events::emit_activity(&state.event_bus, &tenant.tenant_id, "end_user_deleted");
```

- [ ] **Step 2: Add emit to `block_user`**

Before `Ok(Json(...))` at line 818, add:

```rust
    crate::ws::events::emit_activity(&state.event_bus, &tenant.tenant_id, "end_user_blocked");
```

- [ ] **Step 3: Add emit to `unblock_user`**

Before `Ok(Json(...))` at line 848, add:

```rust
    crate::ws::events::emit_activity(&state.event_bus, &tenant.tenant_id, "end_user_unblocked");
```

- [ ] **Step 4: Add emit to MFA `verify`**

In `crates/server/src/handlers/mfa.rs`, before `Ok(Json(...))` at line 716, add:

```rust
    crate::ws::events::emit_activity(&state.event_bus, &tenant_id, "mfa_verified");
```

- [ ] **Step 5: Add emit to WebAuthn `register_complete`**

In `crates/server/src/handlers/webauthn.rs`, before `Ok((StatusCode::CREATED, ...))` at line 1058, add:

```rust
    crate::ws::events::emit_activity(&state.event_bus, &tenant_id, "passkey_registered");
```

- [ ] **Step 6: Verify full server compilation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | tail -5`

Expected: Clean compilation.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/handlers/management.rs crates/server/src/handlers/mfa.rs crates/server/src/handlers/webauthn.rs
git commit -m "feat(ws): emit tenant:activity on block/unblock/delete, MFA verify, and passkey registration"
```

---

## Task 10: Implement `createSocket()` Client Hook

**Files:**
- Create: `admin/src/core/hooks/createSocket.ts`

- [ ] **Step 1: Create `createSocket.ts`**

Create `admin/src/core/hooks/createSocket.ts`:

```typescript
import { createSignal } from 'formajs';
import type { Auth } from './createAuth';

export type SocketState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export interface Socket {
  connect(ticketUrl: string): void;
  disconnect(): void;
  subscribe(topic: string, callback: (hint?: string) => void): () => void;
  state: () => SocketState;
}

const BACKOFF_BASE = 1000;
const BACKOFF_CAP = 30000;
const BACKOFF_FACTOR = 2;

function debounce(fn: (hint?: string) => void, ms: number): (hint?: string) => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  return (hint?: string) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => fn(hint), ms);
  };
}

export function createSocket(auth: Auth): Socket {
  const [state, setState] = createSignal<SocketState>('disconnected');
  const subscribers = new Map<string, Set<(hint?: string) => void>>();
  const debouncedCallbacks = new WeakMap<Function, (hint?: string) => void>();

  let ws: WebSocket | null = null;
  let ticketUrl = '';
  let attempt = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let intentionalClose = false;

  function getWsUrl(base: string): string {
    return base.replace(/^http/, 'ws');
  }

  function allTopics(): string[] {
    return Array.from(subscribers.keys());
  }

  function sendSubscribe(): void {
    const topics = allTopics();
    if (ws && ws.readyState === WebSocket.OPEN && topics.length > 0) {
      ws.send(JSON.stringify({ type: 'subscribe', topics }));
    }
  }

  async function openConnection(): Promise<void> {
    setState('connecting');

    // Get ticket via authenticated HTTP request
    let ticket: string;
    try {
      const res = await fetch(ticketUrl, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(auth.accessToken() ? { Authorization: 'Bearer ' + auth.accessToken() } : {}),
        },
        credentials: 'include',
      });

      if (res.status === 401) {
        // Token expired — refresh and retry once
        try {
          await auth.silentRefresh();
        } catch {
          setState('disconnected');
          return;
        }
        const retry = await fetch(ticketUrl, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            Authorization: 'Bearer ' + auth.accessToken(),
          },
          credentials: 'include',
        });
        if (!retry.ok) {
          scheduleReconnect();
          return;
        }
        const data = await retry.json();
        ticket = data.ticket;
      } else if (!res.ok) {
        scheduleReconnect();
        return;
      } else {
        const data = await res.json();
        ticket = data.ticket;
      }
    } catch {
      scheduleReconnect();
      return;
    }

    // Build WS URL from ticket URL origin
    const url = new URL(ticketUrl);
    const wsBase = getWsUrl(url.origin);
    ws = new WebSocket(wsBase + '/ws?ticket=' + ticket);

    ws.onopen = () => {
      setState('connected');
      attempt = 0;
      sendSubscribe();
    };

    ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data);
        if (msg.type === 'event') {
          const callbacks = subscribers.get(msg.topic);
          if (callbacks) {
            for (const cb of callbacks) {
              const debounced = debouncedCallbacks.get(cb);
              if (debounced) debounced(msg.hint ?? undefined);
            }
          }
        } else if (msg.type === 'auth_expired') {
          // Token expired server-side — refresh and reconnect cleanly
          if (ws) ws.close();
          auth.silentRefresh()
            .then(() => openConnection())
            .catch(() => setState('disconnected'));
          return;
        } else if (msg.type === 'server_shutdown') {
          // Server shutting down — reconnect with backoff
          // (onclose will fire and trigger reconnect)
        }
      } catch {
        // Ignore unparseable messages
      }
    };

    ws.onclose = () => {
      if (!intentionalClose) {
        scheduleReconnect();
      }
    };

    ws.onerror = () => {
      // onerror is always followed by onclose — let onclose handle reconnect
    };
  }

  function scheduleReconnect(): void {
    setState('reconnecting');
    const delay = Math.min(BACKOFF_BASE * (BACKOFF_FACTOR ** attempt), BACKOFF_CAP);
    const jitter = delay * (0.75 + Math.random() * 0.5);
    attempt++;
    reconnectTimer = setTimeout(() => openConnection(), jitter);
  }

  function connect(url: string): void {
    ticketUrl = url;
    intentionalClose = false;
    attempt = 0;
    openConnection();
  }

  function disconnect(): void {
    intentionalClose = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws) {
      ws.close();
      ws = null;
    }
    setState('disconnected');
  }

  function subscribe(topic: string, callback: (hint?: string) => void): () => void {
    if (!subscribers.has(topic)) {
      subscribers.set(topic, new Set());
    }
    const debounced = debounce(callback, 500);
    debouncedCallbacks.set(callback, debounced);
    subscribers.get(topic)!.add(callback);

    // If already connected, send subscribe for the new topic
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'subscribe', topics: [topic] }));
    }

    // Return unsubscribe function
    return () => {
      const set = subscribers.get(topic);
      if (set) {
        set.delete(callback);
        debouncedCallbacks.delete(callback);
        if (set.size === 0) {
          subscribers.delete(topic);
          if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: 'unsubscribe', topics: [topic] }));
          }
        }
      }
    };
  }

  return { connect, disconnect, subscribe, state };
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npx tsc --noEmit 2>&1 | tail -10`

Expected: No errors related to `createSocket.ts`.

- [ ] **Step 3: Commit**

```bash
git add admin/src/core/hooks/createSocket.ts
git commit -m "feat(ws): implement createSocket() client hook with reconnect and debounce"
```

---

## Task 11: Export `createSocket` from Hooks Index

**Files:**
- Modify: `admin/src/core/hooks/index.ts`

- [ ] **Step 1: Add exports**

In `admin/src/core/hooks/index.ts`, add after line 9 (`export { createKeyboard } from './createKeyboard';`):

```typescript
export { createSocket } from './createSocket';
export type { Socket } from './createSocket';
```

- [ ] **Step 2: Commit**

```bash
git add admin/src/core/hooks/index.ts
git commit -m "feat(ws): export createSocket from hooks index"
```

---

## Task 12: Wire Socket into Platform Console

**Files:**
- Modify: `admin/src/platform/console/store.ts:1-12` (imports + socket init)
- Modify: `admin/src/platform/console/store.ts:1495` (connect after auth)
- Modify: `admin/src/platform/console/store.ts:1502-1508` (disconnect on signout)

- [ ] **Step 1: Add socket to store module singletons**

In `admin/src/platform/console/store.ts`, add import after line 4:

```typescript
import { createSocket } from '../../core/hooks/createSocket';
```

After line 12 (`export const toast = createToast();`), add:

```typescript
const socket = createSocket(auth);
export { socket };
```

- [ ] **Step 2: Add socket subscriptions**

After the socket creation (the lines you just added), add the subscription wiring:

```typescript
// ── WebSocket invalidation subscriptions ────────────────────────────────

socket.subscribe('team:changed', () => {
  loadTeamMembers();
  loadTeamInvites();
});

socket.subscribe('metrics:updated', () => {
  const t = selectedTenant();
  if (t && view() === 'overview') {
    loadTenantDetail(t.id);
  }
});

socket.subscribe('tenant:activity', () => {
  const t = selectedTenant();
  if (!t) return;
  if (view() === 'audit-log') {
    loadAuditLog(t.id);
  }
  if (view() === 'sessions') {
    loadAllSessions(t.id);
  }
});
```

Note: `loadTeamInvites`, `loadAuditLog(tenantId)`, `loadAllSessions(tenantId)`, `loadTenantDetail(tenantId)`, `selectedTenant`, and `view` are all already defined in this store module. Since subscriptions reference functions that are defined later in the same module, these subscriptions should be placed after all the function definitions but before `init()`. Alternatively, since the callbacks are only invoked when a WS event arrives (which happens after init), placing them at module top is fine — the functions will be hoisted by the time they're called.

- [ ] **Step 3: Connect socket after auth succeeds**

In the `init()` function (around line 1495), add `socket.connect()` right after `auth.startAutoRefresh();`:

Find the two places where `auth.startAutoRefresh()` is called (lines 1485 and 1495) and add after each:

```typescript
    socket.connect(serverUrl + '/ws/ticket');
```

- [ ] **Step 4: Disconnect on signout**

In the `signout()` function (around line 1502), add `socket.disconnect()` before `auth.stopAutoRefresh()`:

```typescript
export async function signout(): Promise<void> {
  clearOverviewRefresh();
  socket.disconnect();
  auth.stopAutoRefresh();
```

- [ ] **Step 5: Verify TypeScript compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npx tsc --noEmit 2>&1 | tail -10`

Expected: Clean compilation.

- [ ] **Step 6: Commit**

```bash
git add admin/src/platform/console/store.ts
git commit -m "feat(ws): wire socket subscriptions into platform console store"
```

---

## Task 13: Wire Socket into Tenant Admin

**Files:**
- Modify: `admin/src/tenant/admin/app.ts:9-20` (imports + socket init)
- Modify: `admin/src/tenant/admin/app.ts:27-28` (connect after auth)
- Modify: `admin/src/tenant/admin/store.ts:66` (add socket subscriptions)

- [ ] **Step 1: Add socket to tenant admin app.ts**

In `admin/src/tenant/admin/app.ts`, add import after line 11 (`import { createToast } from '../../core/hooks/createToast';`):

```typescript
import { createSocket } from '../../core/hooks/createSocket';
```

After line 19 (`const toast = createToast();`), add:

```typescript
const socket = createSocket(auth);
```

Pass `socket` to the store factory. Update line 20:

```typescript
const store = createTenantAdminStore(api, toast, socket);
```

- [ ] **Step 2: Connect socket after auth**

In the `boot()` function, after `auth.startAutoRefresh();` (line 27), add:

```typescript
    socket.connect(window.location.origin + '/ws/ticket');
```

- [ ] **Step 3: Add socket subscriptions to tenant admin store**

In `admin/src/tenant/admin/store.ts`, update the factory signature at line 66:

```typescript
export function createTenantAdminStore(api: Api, toast: ToastController, socket: Socket) {
```

Add the import for `Socket` type at the top of the file:

```typescript
import type { Socket } from '../../core/hooks/createSocket';
```

Inside the factory, after the data loader functions (after `loadAuditLog` around line 165), add:

```typescript
  // ── WebSocket invalidation subscriptions ──────────────────────────────

  socket.subscribe('tenant:activity', () => {
    if (store.view === 'stats') {
      loadStats();
    }
    if (store.view === 'users') {
      loadUsers();
    }
    if (store.view === 'audit') {
      loadAuditLog();
    }
  });
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npx tsc --noEmit 2>&1 | tail -10`

Expected: Clean compilation.

- [ ] **Step 5: Commit**

```bash
git add admin/src/tenant/admin/app.ts admin/src/tenant/admin/store.ts
git commit -m "feat(ws): wire socket subscriptions into tenant admin store"
```

---

## Task 14: Graceful Shutdown Integration

**Files:**
- Modify: `crates/server/src/main.rs:184-204` (add shutdown broadcast for WS connections)

- [ ] **Step 1: Add shutdown notification to WS connections**

The current `shutdown_signal()` function handles SIGTERM/Ctrl+C. To send `server_shutdown` to all WS clients before closing, we need to broadcast a shutdown signal through the event bus.

In `crates/server/src/main.rs`, after the server's `axum::serve` call completes (after line 225), the process exits. But Axum's graceful shutdown already drains active connections. For WS connections, we need to signal them.

The simplest approach: add a special "shutdown" topic that the WS handler listens for. But cleaner: use a `tokio::sync::watch` channel for shutdown signaling.

Add a shutdown watch channel in `AppState`. In `crates/server/src/lib.rs`, add:

```rust
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
```

In `main.rs`, create the channel and add it to state:

```rust
    let (shutdown_tx, _) = tokio::sync::watch::channel(false);
```

Add `shutdown_tx` to the `AppState` construction:

```rust
        shutdown_tx,
```

Then, in the shutdown flow, after `shutdown_signal()` fires but before the server fully stops, send the shutdown signal:

Replace the graceful shutdown block (lines 216-225):

```rust
    // Graceful shutdown
    let shutdown_tx_for_signal = state_for_shutdown.shutdown_tx.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        // Signal all WS connections to close
        let _ = shutdown_tx_for_signal.send(true);
    })
    .await
    .expect("Server exited with error");
```

Wait — `state` is moved into the router at this point. Clone the shutdown_tx before the router is built. Around line 73 (where other clones happen):

```rust
    let state_shutdown_tx = state.shutdown_tx.clone();
```

Then use `state_shutdown_tx` in the shutdown closure.

- [ ] **Step 2: Update WS handler to listen for shutdown**

In `crates/server/src/ws/handler.rs`, in the `handle_connection` function, add a shutdown watch receiver:

```rust
    let mut shutdown_rx = state.shutdown_tx.subscribe();
```

Add a new arm to the `tokio::select!` loop:

```rust
            _ = shutdown_rx.changed() => {
                let _ = ws_tx.send(Message::Text(r#"{"type":"server_shutdown"}"#.into())).await;
                break;
            }
```

- [ ] **Step 3: Verify compilation**

Run: `cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo check 2>&1 | tail -5`

Expected: Clean compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/lib.rs crates/server/src/main.rs crates/server/src/ws/handler.rs
git commit -m "feat(ws): graceful shutdown sends server_shutdown to all WS clients"
```

---

## Task 15: Manual Integration Test

**Files:** None (verification only)

- [ ] **Step 1: Build the server**

```bash
cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm && cargo build 2>&1 | tail -5
```

Expected: Clean build.

- [ ] **Step 2: Build the frontend**

```bash
cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npm run build 2>&1 | tail -10
```

Expected: Clean build.

- [ ] **Step 3: TypeScript type check**

```bash
cd /Users/victorvillacis/dev/forma/getforma-dev/gatewasm/admin && npx tsc --noEmit 2>&1 | tail -10
```

Expected: No errors.

- [ ] **Step 4: Start the server and test WS connection**

Start the dev server:
```bash
npm run dev
```

In a separate terminal, test the WS flow:

1. Log in to the platform console in the browser
2. Open browser devtools → Network → WS tab
3. Verify you see a WebSocket connection to `/ws`
4. Verify the connection stays open and responds to ping frames
5. In another tab, trigger a team invite — verify the first tab receives a `team:changed` event
6. Verify the team members list refreshes automatically

- [ ] **Step 5: Test reconnection**

1. Kill the server (Ctrl+C)
2. Watch the browser devtools — the WS should show as closed
3. Restart the server
4. Verify the client reconnects automatically (new WS connection appears)
5. Verify the client refetches current data on reconnect

- [ ] **Step 6: Test auth expiry**

1. Set `ACCESS_TOKEN_EXPIRY=30` in `.env` (30 second tokens for testing)
2. Restart server
3. Connect to console, open WS devtools
4. Wait 30 seconds — verify the server sends `auth_expired`
5. Verify the client reconnects with a fresh token
6. Reset `ACCESS_TOKEN_EXPIRY` to normal value

- [ ] **Step 7: Commit any fixes discovered during testing**

```bash
git add -A
git commit -m "fix(ws): integration test fixes"
```

Only if fixes were needed. Skip if everything works.

---

## Summary

| Task | Description | Files Created | Files Modified |
|------|-------------|---------------|----------------|
| 1 | Module skeleton + dashmap dep | `ws/mod.rs` | `Cargo.toml`, `lib.rs` |
| 2 | TicketStore + `/ws/ticket` handler | `ws/ticket.rs` | — |
| 3 | Emit helper functions | `ws/events.rs` | — |
| 4 | WS upgrade handler + connection loop | `ws/handler.rs` | `Cargo.toml` (futures-util) |
| 5 | Wire AppState + routes | — | `main.rs`, `lib.rs`, `handlers/mod.rs` |
| 6 | Health check WS count | — | `handlers/mod.rs` |
| 7 | Auth handler emits | — | `auth.rs` |
| 8 | Team/invite handler emits | — | `owner/team.rs`, `owner/invites.rs` |
| 9 | Management/MFA/WebAuthn emits | — | `management.rs`, `mfa.rs`, `webauthn.rs` |
| 10 | `createSocket()` hook | `createSocket.ts` | — |
| 11 | Hooks index export | — | `index.ts` |
| 12 | Platform console wiring | — | `console/store.ts` |
| 13 | Tenant admin wiring | — | `tenant/admin/app.ts`, `tenant/admin/store.ts` |
| 14 | Graceful shutdown | — | `lib.rs`, `main.rs`, `ws/handler.rs` |
| 15 | Manual integration test | — | — |
