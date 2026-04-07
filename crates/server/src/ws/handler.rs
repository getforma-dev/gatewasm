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

        let entry = self
            .per_tenant
            .entry(tenant_id.to_string())
            .or_insert_with(|| AtomicUsize::new(0));
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
    let mut shutdown_rx = state.shutdown_tx.subscribe();

    // Kill connection when the original JWT would have expired.
    let now_ts = chrono::Utc::now().timestamp() as u64;
    let secs_until_expiry = info.jwt_exp.saturating_sub(now_ts);
    let token_expiry = tokio::time::sleep_until(Instant::now() + Duration::from_secs(secs_until_expiry));
    tokio::pin!(token_expiry);

    loop {
        tokio::select! {
            result = bus_rx.recv() => {
                match result {
                    Ok(event) => {
                        if event.tenant_id == info.tenant_id
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
                    _ => {}
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

            _ = shutdown_rx.changed() => {
                let _ = ws_tx.send(Message::Text(r#"{"type":"server_shutdown"}"#.into())).await;
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
            let topics = v
                .get("topics")?
                .as_array()?
                .iter()
                .filter_map(|t| t.as_str().map(String::from))
                .collect();
            Some(ClientMsg::Subscribe(topics))
        }
        "unsubscribe" => {
            let topics = v
                .get("topics")?
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
