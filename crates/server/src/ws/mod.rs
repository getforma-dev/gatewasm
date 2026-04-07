pub mod events;
pub mod handler;
pub mod ticket;

use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Clone, Debug, Serialize)]
pub struct WsEvent {
    pub tenant_id: String,
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
