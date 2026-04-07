use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{extract::State, response::IntoResponse, Json};
use dashmap::DashMap;
use rand::RngCore;

use crate::error::AppError;
use crate::AppState;

const TICKET_TTL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct TicketInfo {
    pub account_id: String,
    pub tenant_id: String,
    pub jwt_exp: u64,
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
        rand::thread_rng().fill_bytes(&mut bytes);
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
        tenant_id: claims.tenant_id.clone().unwrap_or_default(),
        jwt_exp: claims.exp,
        created_at: Instant::now(),
    });

    Ok(Json(serde_json::json!({ "ticket": ticket })))
}
