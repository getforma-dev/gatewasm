use crate::types::*;
use crate::AppState;
use axum::{
    extract::State,
    http::header,
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

pub async fn jwks(State(state): State<AppState>) -> impl IntoResponse {
    let body = JwksResponse {
        keys: vec![JwkKey {
            kty: "OKP".to_string(),
            r#use: "sig".to_string(),
            kid: state.keys.kid.clone(),
            alg: "EdDSA".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some(URL_SAFE_NO_PAD.encode(&state.keys.public_key_bytes)),
        }],
    };

    (
        [(header::CACHE_CONTROL, "public, max-age=3600")],
        Json(body),
    )
}
