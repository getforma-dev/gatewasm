# WebSocket Infrastructure for fAuth

**Date:** 2026-04-07
**Branch:** `feat/platform-team-rbac` (extends current work)
**Status:** Approved — ready for implementation planning

## Overview

A real-time invalidation layer for the fAuth dashboard, built on Axum-native WebSockets with Tokio broadcast channels. WebSocket connections push lightweight invalidation signals ("something changed, refetch"), while existing HTTP endpoints remain the sole data transport and source of truth.

This differentiates fAuth from Auth0/Clerk's static dashboards by making the platform feel alive — live activity feeds, instant team updates, real-time metrics.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Transport | WebSocket (not SSE) | Bidirectional needed for future presence/collaboration features |
| Server implementation | Axum-native `axum::extract::ws` + Tokio broadcast | Zero new dependencies, single binary, single instance on EC2 t4g.small |
| Data model | WS is invalidation-only, HTTP is source of truth | Linear/Notion/Figma pattern. No event buffering, no replay, no sequence numbers. |
| Connection scope | One connection per plane | Maps to existing three-plane architecture (Operator / Org+Member / End User) |
| Auth | Ticket-based (`POST /ws/ticket` → single-use 30s ticket) | Keeps JWTs out of URLs/logs. Industry standard (Slack, Discord, Notion). Critical for HIPAA. |
| Reconnection | Auto-reconnect with exponential backoff + full refetch | No event replay. Client refetches current state via existing HTTP APIs on reconnect. |
| Message format | Flat JSON with topic + scope + optional hint | Tiny invalidation signals (~200 bytes), debuggable in browser devtools |
| Client hook | `createSocket()` in `src/core/hooks/` | Follows existing `createAuth()`, `createApi()`, `createToast()` pattern |
| Event bus | Single global `broadcast::channel(1024)` | Client-side filtering by tenant_id. Sharding by tenant is a one-line change later. |
| Multi-instance path | Swap Tokio broadcast for Redis pub/sub | WS handler, ticket auth, and client code don't change — only the event source changes. |

## MVP Scope

Three invalidation topics for the first implementation:

| Topic | Trigger | Value |
|-------|---------|-------|
| `team:changed` | Invite accepted/created/revoked, member removed/updated | Original motivating feature — pending-to-accepted flips live |
| `tenant:activity` | Login, signup, block/unblock, MFA, passkey registration | Live audit feed — the "wow" differentiator vs static dashboards |
| `metrics:updated` | Piggybacks on `tenant:activity` (same events) | Overview tab metrics tick live. Near-zero marginal cost. |

**Not in MVP (future, same infra):** Team presence, webhook delivery status, session revocation feedback, alert notifications.

---

## Server-Side Architecture

### EventBus in AppState

```rust
// ws/mod.rs
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct WsEvent {
    pub tenant_id: Uuid,
    pub topic: String,        // "team:changed", "tenant:activity", "metrics:updated"
    pub hint: Option<String>, // "invite_accepted", "login", "signup", etc.
    pub timestamp: i64,       // unix millis
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
        // Fire-and-forget. If no receivers, event is silently dropped.
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsEvent> {
        self.sender.subscribe()
    }
}
```

Added to `AppState`:

```rust
pub struct AppState {
    pub db: PgPool,
    // ...existing fields...
    pub event_bus: EventBus,
}
```

One global broadcast channel with client-side filtering. At <500 users and ~50 events/second, per-tenant sharding is unnecessary. Each WS handler filters by its own `tenant_id`.

### Emit Helper Functions

```rust
// ws/events.rs
use super::{EventBus, WsEvent};
use uuid::Uuid;

/// Emits both tenant:activity and metrics:updated in one call.
/// Called from auth handlers after login, signup, block, etc.
pub fn emit_activity(bus: &EventBus, tenant_id: Uuid, hint: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    bus.send(WsEvent {
        tenant_id,
        topic: "tenant:activity".into(),
        hint: Some(hint.into()),
        timestamp: now,
    });
    bus.send(WsEvent {
        tenant_id,
        topic: "metrics:updated".into(),
        hint: None,
        timestamp: now,
    });
}

/// Emits team:changed. Called from team/invite handlers.
pub fn emit_team_changed(bus: &EventBus, tenant_id: Uuid, hint: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    bus.send(WsEvent {
        tenant_id,
        topic: "team:changed".into(),
        hint: Some(hint.into()),
        timestamp: now,
    });
}
```

### Emit Points in Existing Handlers

**`team:changed`** — from team/invite handlers in `handlers/team.rs`:

| Handler | Hint |
|---------|------|
| `accept_invite` | `"invite_accepted"` |
| `create_invite` | `"invite_created"` |
| `revoke_invite` | `"invite_revoked"` |
| `remove_member` | `"member_removed"` |
| `update_member_role` | `"member_updated"` |

**`tenant:activity`** + **`metrics:updated`** — from auth handlers:

| Handler | File | Hint |
|---------|------|------|
| `login` | `handlers/auth.rs` | `"login"` |
| `register` | `handlers/auth.rs` | `"signup"` |
| `block_end_user` | `handlers/manage_users.rs` | `"end_user_blocked"` |
| `unblock_end_user` | `handlers/manage_users.rs` | `"end_user_unblocked"` |
| `delete_end_user` | `handlers/manage_users.rs` | `"end_user_deleted"` |
| `mfa_verified` | `handlers/mfa.rs` | `"mfa_verified"` |
| `webauthn_register` | `handlers/webauthn.rs` | `"passkey_registered"` |

Each handler gains one line: `emit_activity(&state.event_bus, tenant_id, "login");`

### Ticket Auth — `POST /ws/ticket`

```rust
// ws/ticket.rs
use dashmap::DashMap;
use uuid::Uuid;
use std::time::{Duration, Instant};
use rand::RngCore;

pub struct TicketInfo {
    pub account_id: Uuid,
    pub tenant_id: Uuid,
    pub plane: Plane,          // Operator / Member / EndUser
    pub jwt_exp: i64,          // original JWT exp claim (unix timestamp)
    pub created_at: Instant,
}

pub struct TicketStore {
    tickets: DashMap<String, TicketInfo>,
}

const TICKET_TTL: Duration = Duration::from_secs(30);

impl TicketStore {
    pub fn new() -> Self {
        Self { tickets: DashMap::new() }
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
                None // expired — lazy expiry
            }
        })
    }
}
```

**Route:** `POST /ws/ticket` — behind existing JWT auth middleware.

**Handler:** Extracts `account_id`, `tenant_id`, `plane` from JWT claims, records `exp`, creates ticket, returns `{ "ticket": "hex64chars" }`.

### WS Upgrade Handler — `GET /ws?ticket=<ticket>`

```rust
// ws/handler.rs (pseudocode structure)

async fn ws_upgrade(
    State(state): State<AppState>,
    Query(params): Query<WsParams>,   // { ticket: String }
    ws: WebSocketUpgrade,
) -> Response {
    // 1. Look up and burn ticket
    let info = match state.ticket_store.take(&params.ticket) {
        Some(info) => info,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    // 2. Check connection limits
    if !state.connection_limits.try_acquire(&info) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    // 3. Upgrade
    ws.on_upgrade(move |socket| handle_connection(socket, state, info))
}

async fn handle_connection(socket: WebSocket, state: AppState, info: TicketInfo) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut bus_rx = state.event_bus.subscribe();
    let mut subscribed_topics: HashSet<String> = HashSet::new();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    let mut invalid_msg_count: u8 = 0;

    // Timer: kill connection when original JWT expires
    let token_expiry = tokio::time::sleep_until(
        Instant::now() + Duration::from_secs(
            (info.jwt_exp - chrono::Utc::now().timestamp()).max(0) as u64
        )
    );
    tokio::pin!(token_expiry);

    loop {
        tokio::select! {
            // Server → Client: forward matching events
            Ok(event) = bus_rx.recv() => {
                if event.tenant_id == info.tenant_id
                    && subscribed_topics.contains(&event.topic)
                {
                    let msg = serde_json::json!({
                        "type": "event",
                        "topic": event.topic,
                        "tenant_id": event.tenant_id,
                        "hint": event.hint,
                    });
                    if ws_tx.send(Message::Text(msg.to_string())).await.is_err() {
                        break; // client disconnected
                    }
                }
            }

            // Client → Server: subscribe/unsubscribe/ping
            Some(Ok(msg)) = ws_rx.next() => {
                match parse_client_message(msg) {
                    Some(ClientMsg::Subscribe(topics)) => {
                        for t in topics {
                            if VALID_TOPICS.contains(&t.as_str()) {
                                subscribed_topics.insert(t);
                            }
                        }
                    }
                    Some(ClientMsg::Unsubscribe(topics)) => {
                        for t in topics { subscribed_topics.remove(&t); }
                    }
                    Some(ClientMsg::Ping) => {
                        let _ = ws_tx.send(Message::Text(r#"{"type":"pong"}"#.into())).await;
                    }
                    None => {
                        invalid_msg_count += 1;
                        if invalid_msg_count >= 5 { break; }
                    }
                }
            }

            // Keepalive ping every 30s
            _ = ping_interval.tick() => {
                if ws_tx.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }

            // JWT expiry — kill connection
            _ = &mut token_expiry => {
                let msg = r#"{"type":"auth_expired"}"#;
                let _ = ws_tx.send(Message::Text(msg.into())).await;
                break;
            }
        }
    }

    // Cleanup
    state.connection_limits.release(&info);
    // tracing::info! for connection close
}

const VALID_TOPICS: &[&str] = &["team:changed", "tenant:activity", "metrics:updated"];
```

### Route Registration

```rust
// In handlers/mod.rs
.route("/ws/ticket", post(ws::ticket::create_ticket))  // behind auth middleware
.route("/ws", get(ws::handler::ws_upgrade))             // ticket-based auth (no middleware)
```

### New Server Files

```
crates/server/src/ws/
├── mod.rs          — EventBus, WsEvent, re-exports
├── ticket.rs       — TicketStore, TicketInfo, POST /ws/ticket handler
├── handler.rs      — GET /ws upgrade, connection loop, filtering
└── events.rs       — emit_activity(), emit_team_changed()
```

---

## Client-Side Architecture

### `createSocket()` Hook

**File:** `admin/src/core/hooks/createSocket.ts` (~150 lines)

```typescript
interface Socket {
    connect(ticketUrl: string): void;
    disconnect(): void;
    subscribe(topic: string, callback: (hint?: string) => void): () => void;
    state: () => 'connecting' | 'connected' | 'disconnected' | 'reconnecting';
}
```

### Connection Lifecycle

```
connect(ticketUrl)
  → POST ticketUrl (with Authorization header via createApi)
  → receive { ticket }
  → new WebSocket(wsUrl + "?ticket=" + ticket)
  → state = 'connecting'
  → onopen: state = 'connected', send subscribe for all registered topics, reset backoff
  → onmessage: parse JSON, route to callbacks by topic (debounced 500ms per-topic)
  → onclose/onerror: state = 'reconnecting', exponential backoff (1s→2s→4s→8s→16s→30s cap, ±25% jitter)
  → on auth_expired: auth.silentRefresh() → new ticket → reconnect (no backoff)
  → disconnect(): state = 'disconnected', close socket, no reconnect
```

**Subscribe-before-connect:** Subscriptions are stored in a `Map<string, Set<callback>>`. On connect and reconnect, the hook sends a `subscribe` message with all current topics. Store code calls `subscribe()` during initialization without waiting for connection.

**Debouncing:** Each subscription callback is wrapped in a 500ms trailing debounce (per-topic). A burst of 50 login events produces one `loadAuditLog()` call, not 50.

**Backoff jitter:** `delay * (0.75 + Math.random() * 0.5)` prevents thundering herd after deploys.

### Wire Protocol

**Client → Server:**
```json
{ "type": "subscribe", "topics": ["team:changed", "metrics:updated"] }
{ "type": "unsubscribe", "topics": ["metrics:updated"] }
{ "type": "ping" }
```

**Server → Client:**
```json
{ "type": "event", "topic": "team:changed", "tenant_id": "uuid", "hint": "invite_accepted" }
{ "type": "pong" }
{ "type": "auth_expired" }
{ "type": "server_shutdown" }
```

### Store Integration

**Platform Console (`src/platform/console/store.ts`):**
```
subscribe("team:changed")     → loadTeamMembers(), loadPendingInvites()
subscribe("metrics:updated")  → loadOverviewMetrics() (if current view is overview)
subscribe("tenant:activity")  → loadAuditLog() (if current view is audit), loadSessions() (if sessions)
```

**Tenant Admin (`src/tenant/admin/store.ts`):**
```
subscribe("tenant:activity")  → loadStats() (if stats view), loadEndUsers() (if users view)
```

**Tenant Dashboard:** No WS subscriptions in MVP.

### New Client Files

```
admin/src/core/hooks/createSocket.ts   — the hook (~150 lines, single file)
```

---

## Security & Compliance

### Connection Limits

| Scope | Limit | Enforcement |
|-------|-------|-------------|
| Per tenant | 50 | `AtomicUsize` in `DashMap`, checked at upgrade |
| Per account (all tenants) | 100 | Same mechanism |
| Global | 500 | Same mechanism, hard ceiling |

Reject with `429 Too Many Requests` before upgrade. Decremented on connection drop.

### Compliance Tiers

| Tier | Behavior |
|------|----------|
| **Standard** | Token expiry kills connection. Client auto-reconnects via refresh. |
| **SOC2** | Standard + audit_log entries for WS connect/disconnect events. |
| **HIPAA** | SOC2 + strict expiry (no grace period). WS carries zero PII/PHI — invalidation signals only. All sensitive data fetched over HTTPS. |

### TLS

- Production: `wss://` only. TLS terminated at EC2 or load balancer.
- Development: `ws://localhost:3001/ws` is acceptable.
- `createSocket()` infers protocol from `serverUrl` (`http` → `ws`, `https` → `wss`).

### Rate Limiting & Abuse

- `/ws/ticket` inherits existing rate limiter (same as `/auth/token`).
- Invalid WS messages: connection closed after 5 consecutive invalid messages.
- Topics validated against `VALID_TOPICS` whitelist server-side.

### Graceful Shutdown

1. Stop accepting new connections
2. Send `{ "type": "server_shutdown" }` to all clients
3. Close all connections with code `1001` (Going Away)
4. Clients enter reconnect backoff
5. Integrates with Axum's existing `graceful_shutdown` signal

---

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Server crash/restart | All WS connections drop | Auto-reconnect + full refetch. Dashboard works without WS. |
| Slow client (lagged) | `broadcast::Lagged` error | Client reconnects, refetches. 1024-buffer won't lag at this volume. |
| Ticket endpoint down | Can't establish WS | Dashboard works — no live updates. All data via HTTP. |
| Memory pressure | Connection exhaustion | 500-conn cap = ~10MB. Postgres pool is bigger. |
| Malicious client | CPU waste | Close after 5 invalid msgs. Rate limit tickets. Topic whitelist. |

---

## Future Topics (Not MVP)

These plug into the same infrastructure without protocol changes:

| Topic | When | Trigger |
|-------|------|---------|
| `presence:updated` | Team presence feature | Operator opens/closes tenant view |
| `webhook:delivery` | Webhook monitoring feature | Webhook delivery succeeds/fails |
| `session:revoked` | Session management feature | End user session blocked/revoked |
| `alert:security` | Security alerting feature | Brute force detected, suspicious login |

Each is a new topic string + new emit points in handlers + new `subscribe()` calls in stores. No protocol changes, no new infrastructure.

---

## Dependencies

**New Rust crates:**
- `dashmap` — concurrent hashmap for ticket store and connection counters

**Existing (already in Cargo.toml):**
- `axum` 0.8 — includes WS support via tungstenite
- `tokio` — broadcast channels, timers, select
- `serde_json` — message serialization
- `rand` — ticket generation
- `hex` — ticket encoding
- `chrono` — timestamps

**Frontend:** No new dependencies. Pure TypeScript, uses browser `WebSocket` API.
