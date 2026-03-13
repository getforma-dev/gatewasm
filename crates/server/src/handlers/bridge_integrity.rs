use axum::{http::header, response::IntoResponse, Json};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha384};

/// The bridge.js source, embedded at compile time.
const BRIDGE_JS: &str = include_str!("../../../../bridge/gate-bridge.min.js");

/// GET /bridge/integrity — returns the SRI hash, version, and usage snippet for bridge.js.
pub async fn bridge_integrity() -> impl IntoResponse {
    let mut hasher = Sha384::new();
    hasher.update(BRIDGE_JS.as_bytes());
    let hash = STANDARD.encode(hasher.finalize());
    let sri_hash = format!("sha384-{}", hash);

    let headers = [
        (header::CACHE_CONTROL, "public, max-age=3600".to_string()),
        (
            header::HeaderName::from_static("cross-origin-resource-policy"),
            "cross-origin".to_string(),
        ),
    ];

    (
        headers,
        Json(serde_json::json!({
            "sri_hash": sri_hash,
            "version": env!("CARGO_PKG_VERSION"),
            "usage": format!(
                "<script src=\"https://cdn.getforma.dev/bridge/gate-bridge.min.js\" integrity=\"{}\" crossorigin=\"anonymous\"></script>",
                sri_hash
            ),
        })),
    )
}
