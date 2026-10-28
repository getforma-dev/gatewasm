use crate::types::*;
use crate::AppState;
use axum::{extract::State, http::header, response::IntoResponse, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

pub async fn jwks(State(state): State<AppState>) -> axum::response::Response {
    let keys = state
        .keys
        .access_public_keys(chrono::Utc::now().timestamp());
    public_key_response(&state, keys).await
}

/// SET verification uses a separate public key set; these keys must never be
/// offered to consumers resolving access JWT signatures at /auth/jwks.
pub async fn security_events(State(state): State<AppState>) -> axum::response::Response {
    let keys = state.keys.security_event_public_keys();
    public_key_response(&state, keys).await
}

async fn public_key_response(
    state: &AppState,
    keys: Vec<crate::jwt::keys::PublicKey>,
) -> axum::response::Response {
    if !state.keys.serving_current_generation().await {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            [(header::CACHE_CONTROL, "no-store")],
            Json(serde_json::json!({"error":"stale_signing_generation"})),
        )
            .into_response();
    }
    let body = JwksResponse {
        keys: keys
            .into_iter()
            .map(|key| JwkKey {
                kty: "OKP".to_string(),
                r#use: "sig".to_string(),
                kid: key.kid,
                alg: "EdDSA".to_string(),
                crv: Some("Ed25519".to_string()),
                x: Some(URL_SAFE_NO_PAD.encode(&key.bytes)),
            })
            .collect(),
    };

    (
        [(header::CACHE_CONTROL, "public, max-age=3600")],
        Json(body),
    )
        .into_response()
}
