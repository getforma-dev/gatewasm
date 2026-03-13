use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::AppState;

const CORS_METHODS: &str = "GET,POST,DELETE,PATCH,OPTIONS";
const CORS_HEADERS: &str = "content-type,authorization,accept,x-gatewasm-client,x-tenant-slug";
const CORS_MAX_AGE: &str = "86400";

/// Dynamic CORS middleware that checks both the global `ALLOWED_ORIGINS` env var
/// and per-tenant `allowed_origins` from the database.
///
/// Flow:
/// 1. Read the `Origin` header from the request.
/// 2. Check if it matches a global allowed origin (fast path, no DB).
/// 3. If not, extract the tenant slug from the URL path (`/t/{slug}/...`)
///    and check if the origin is in the tenant's `allowed_origins` column.
/// 4. If allowed, set CORS response headers with credentials support.
pub async fn dynamic_cors(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let is_preflight = req.method() == Method::OPTIONS;

    let allowed_origin = match &origin {
        Some(origin_str) => {
            // Fast path: check global ALLOWED_ORIGINS
            if state
                .config
                .allowed_origins
                .iter()
                .any(|o| o == origin_str)
            {
                Some(origin_str.clone())
            } else {
                // Slow path: check per-tenant allowed_origins in DB
                check_tenant_origin(&state, req.uri().path(), origin_str).await
            }
        }
        None => None,
    };

    if is_preflight {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::OK;
        if let Some(ref origin_val) = allowed_origin {
            add_cors_headers(response.headers_mut(), origin_val);
        }
        return response;
    }

    let mut response = next.run(req).await;
    if let Some(ref origin_val) = allowed_origin {
        add_cors_headers(response.headers_mut(), origin_val);
    }
    // Vary: Origin ensures caches don't serve wrong CORS headers
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("origin"));
    response
}

fn add_cors_headers(headers: &mut axum::http::HeaderMap, origin: &str) {
    if let Ok(val) = HeaderValue::from_str(origin) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(CORS_METHODS),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(CORS_HEADERS),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static(CORS_MAX_AGE),
    );
}

/// Extract tenant slug from path (`/t/acme/auth/token` → `acme`)
/// and check if the origin is in the tenant's `allowed_origins` JSON array.
async fn check_tenant_origin(state: &AppState, path: &str, origin: &str) -> Option<String> {
    let slug = extract_slug(path)?;

    let origins: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT allowed_origins FROM tenants WHERE slug = $1 AND status = 'active'",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let origins = origins?;
    if let Some(arr) = origins.as_array() {
        for v in arr {
            if v.as_str() == Some(origin) {
                return Some(origin.to_string());
            }
        }
    }

    None
}

fn extract_slug(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 3 && parts[1] == "t" && !parts[2].is_empty() {
        Some(parts[2].to_string())
    } else {
        None
    }
}
