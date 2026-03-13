use axum::{
    extract::Path,
    http::HeaderMap,
    response::Response,
};
use rust_embed::Embed;

// Re-export asset types from forma-server so handler code using
// `crate::assets::AssetManifest`, `RouteAssets`, `WasmAssets` continues to work.
pub use forma_server::types::{AssetManifest, RouteAssets, WasmAssets};

/// Application-specific embedded asset directory.
/// This struct stays in the application — forma-server provides generic functions,
/// but the concrete `#[folder]` path is always application-specific.
#[derive(Embed)]
#[folder = "../../admin/dist/"]
pub struct AdminAssets;

/// Load a text asset from the embedded `admin/dist/` folder.
/// Panics at runtime if the asset is missing or not valid UTF-8.
pub fn asset(name: &str) -> String {
    forma_server::assets::asset::<AdminAssets>(name)
}

/// Load raw bytes from embedded assets. Returns None if not found.
pub fn asset_bytes(name: &str) -> Option<Vec<u8>> {
    forma_server::assets::asset_bytes::<AdminAssets>(name)
}

/// Load and parse the asset manifest from embedded admin/dist/manifest.json.
/// Panics if manifest is missing or malformed — this is a build-time contract.
pub fn load_manifest() -> AssetManifest {
    forma_server::assets::load_manifest::<AdminAssets>()
}

/// GET /_assets/{filename} — Serve embedded assets with content negotiation.
pub async fn serve_asset(path: Path<String>, headers: HeaderMap) -> Response {
    forma_server::assets::serve_asset::<AdminAssets>(path, headers).await
}

/// GET /sw.js — Serve the service worker with no-cache headers.
/// Browser spec requires stable URL and revalidation on each navigation.
pub async fn serve_sw() -> Response {
    forma_server::sw::serve_sw::<AdminAssets>().await
}
