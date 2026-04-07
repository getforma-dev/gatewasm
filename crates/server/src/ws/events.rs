use super::{EventBus, WsEvent};

/// Emits both `tenant:activity` and `metrics:updated` in one call.
/// Called from auth handlers after login, signup, block, etc.
pub fn emit_activity(bus: &EventBus, tenant_id: &str, hint: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    bus.send(WsEvent {
        tenant_id: tenant_id.to_string(),
        topic: "tenant:activity".into(),
        hint: Some(hint.into()),
        timestamp: now,
    });
    bus.send(WsEvent {
        tenant_id: tenant_id.to_string(),
        topic: "metrics:updated".into(),
        hint: None,
        timestamp: now,
    });
}

/// Emits `team:changed`. Called from team/invite handlers.
pub fn emit_team_changed(bus: &EventBus, tenant_id: &str, hint: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    bus.send(WsEvent {
        tenant_id: tenant_id.to_string(),
        topic: "team:changed".into(),
        hint: Some(hint.into()),
        timestamp: now,
    });
}
