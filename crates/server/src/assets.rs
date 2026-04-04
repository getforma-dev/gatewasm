use axum::{
    extract::Path,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;
use std::{
    fs,
    path::{Path as FsPath, PathBuf},
};

// Re-export asset types from forma-server so handler code using
// `crate::assets::AssetManifest`, `RouteAssets`, `WasmAssets` continues to work.
pub use forma_server::types::{AssetManifest, RouteAssets, WasmAssets};

/// Application-specific embedded asset directory.
/// This struct stays in the application — forma-server provides generic functions,
/// but the concrete `#[folder]` path is always application-specific.
#[derive(Embed)]
#[folder = "../../admin/dist/"]
pub struct AdminAssets;

const DEV_ASSET_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../admin/dist");
const DEV_SW_SCRIPT: &str = r#"self.addEventListener('install', (event) => {
  event.waitUntil(self.skipWaiting());
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    const keys = await caches.keys();
    await Promise.all(keys.map((key) => caches.delete(key)));
    await self.clients.claim();
    await self.registration.unregister();
  })());
});
"#;

pub fn use_filesystem_assets() -> bool {
    cfg!(debug_assertions) && FsPath::new(DEV_ASSET_DIR).exists()
}

fn filesystem_asset_path(name: &str) -> PathBuf {
    FsPath::new(DEV_ASSET_DIR).join(name)
}

fn filesystem_asset_bytes(name: &str) -> Option<Vec<u8>> {
    fs::read(filesystem_asset_path(name)).ok()
}

fn asset_response(filename: &str, data: Vec<u8>, encoding: Option<&str>, cache_control: &str) -> Response {
    let mime = mime_for(filename);
    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache_control)
        .header("vary", "accept-encoding")
        .header("x-content-type-options", "nosniff");

    if let Some(enc) = encoding {
        builder = builder.header(header::CONTENT_ENCODING, enc);
    }

    builder.body(axum::body::Body::from(data)).unwrap()
}

fn mime_for(filename: &str) -> &'static str {
    if filename.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if filename.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if filename.ends_with(".woff2") {
        "font/woff2"
    } else if filename.ends_with(".wasm") {
        "application/wasm"
    } else if filename.ends_with(".json") {
        "application/json"
    } else if filename.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if filename.ends_with(".svg") {
        "image/svg+xml"
    } else if filename.ends_with(".png") {
        "image/png"
    } else {
        "application/octet-stream"
    }
}

/// Load a text asset from the embedded `admin/dist/` folder.
/// Panics at runtime if the asset is missing or not valid UTF-8.
pub fn asset(name: &str) -> String {
    if use_filesystem_assets() {
        if let Some(data) = filesystem_asset_bytes(name) {
            return String::from_utf8(data).unwrap_or_else(|_| panic!("Non-UTF8 asset: {name}"));
        }
    }
    forma_server::assets::asset::<AdminAssets>(name)
}

/// Load raw bytes from embedded assets. Returns None if not found.
pub fn asset_bytes(name: &str) -> Option<Vec<u8>> {
    if use_filesystem_assets() {
        if let Some(data) = filesystem_asset_bytes(name) {
            return Some(data);
        }
    }
    forma_server::assets::asset_bytes::<AdminAssets>(name)
}

/// Load and parse the asset manifest from embedded admin/dist/manifest.json.
/// Panics if manifest is missing or malformed — this is a build-time contract.
pub fn load_manifest() -> AssetManifest {
    if use_filesystem_assets() {
        if let Some(data) = filesystem_asset_bytes("manifest.json") {
            return serde_json::from_slice(&data).expect("Failed to parse manifest.json");
        }
    }
    forma_server::assets::load_manifest::<AdminAssets>()
}

/// GET /_assets/{filename} — Serve embedded assets with content negotiation.
pub async fn serve_asset(path: Path<String>, headers: HeaderMap) -> Response {
    if use_filesystem_assets() {
        let filename = path.0;
        let accept = headers
            .get(header::ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let (data, encoding) = if accept.contains("br") {
            if let Some(br) = filesystem_asset_bytes(&format!("{filename}.br")) {
                (br, Some("br"))
            } else if let Some(raw) = filesystem_asset_bytes(&filename) {
                (raw, None)
            } else {
                return StatusCode::NOT_FOUND.into_response();
            }
        } else if accept.contains("gzip") {
            if let Some(gz) = filesystem_asset_bytes(&format!("{filename}.gz")) {
                (gz, Some("gzip"))
            } else if let Some(raw) = filesystem_asset_bytes(&filename) {
                (raw, None)
            } else {
                return StatusCode::NOT_FOUND.into_response();
            }
        } else if let Some(raw) = filesystem_asset_bytes(&filename) {
            (raw, None)
        } else {
            return StatusCode::NOT_FOUND.into_response();
        };

        return asset_response(&filename, data, encoding, "no-cache, no-store, must-revalidate");
    }

    forma_server::assets::serve_asset::<AdminAssets>(path, headers).await
}

/// GET /sw.js — Serve the service worker with no-cache headers.
/// Browser spec requires stable URL and revalidation on each navigation.
pub async fn serve_sw() -> Response {
    if use_filesystem_assets() {
        return Response::builder()
            .header(
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            )
            .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
            .header("service-worker-allowed", "/")
            .body(axum::body::Body::from(DEV_SW_SCRIPT))
            .unwrap();
    }

    forma_server::sw::serve_sw::<AdminAssets>().await
}
