pub mod assets;
pub mod compliance;
pub mod config;
pub mod crypto;
pub mod db;
pub mod email;
pub mod error;
pub mod frontend;
pub mod handlers;
pub mod html_template;
pub mod ir;
pub mod jwt;
pub mod middleware;
pub mod redirects;
pub mod responses;
pub mod security_config;
pub mod types;
pub mod ws;

use std::collections::HashMap;

use sqlx::postgres::PgPool;

use assets::AssetManifest;
use config::AppConfig;
use jwt::keys::KeyPair;
use middleware::tenant::TenantCache;
use ws::EventBus;
use ws::handler::ConnectionLimits;
use ws::ticket::TicketStore;

// Re-export RenderMode from the forma-server framework crate.
// All handler code continues to use `crate::RenderMode` unchanged.
pub use forma_server::RenderMode;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: AppConfig,
    pub keys: KeyPair,
    pub tenant_cache: TenantCache,
    pub asset_manifest: AssetManifest,
    /// Per-route render mode decisions (route pattern -> mode).
    pub render_modes: HashMap<String, RenderMode>,
    /// Pre-parsed IR modules keyed by route pattern.
    pub ir_modules: HashMap<String, ir::parser::IrModule>,
    pub event_bus: EventBus,
    pub ticket_store: TicketStore,
    pub connection_limits: ConnectionLimits,
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
}
