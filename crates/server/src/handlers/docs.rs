use axum::{
    http::StatusCode,
    response::{Html, IntoResponse},
};
use serde_json::{Map, Value};

const OPENAPI_SPEC: &str = include_str!("../../openapi.json");
const PUBLIC_TAGS: &[&str] = &[
    "Health",
    "Auth",
    "OAuth",
    "User",
    "Management",
    "Roles",
    "Webhooks",
    "Organizations",
    "Admin",
    "Self-Serve",
];
const INTERNAL_TAGS: &[&str] = &["Platform", "Compliance"];
const HTTP_METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

fn swagger_ui_html(title: &str, spec_url: &str) -> Html<String> {
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.18.2/swagger-ui.css" />
  <style>
    body {{ margin: 0; background: #fafafa; }}
    .swagger-ui .topbar {{ display: none; }}
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5.18.2/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({{
      url: "{spec_url}",
      dom_id: '#swagger-ui',
      deepLinking: true,
      presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
      layout: "BaseLayout",
    }});
  </script>
</body>
</html>"#,
        title = title,
        spec_url = spec_url,
    );
    Html(html)
}

fn should_keep_public_operation(op: &Value) -> bool {
    let Some(tags) = op.get("tags").and_then(Value::as_array) else {
        // Default deny: operations without explicit public tags are not exported publicly.
        return false;
    };

    let op_tags: Vec<&str> = tags.iter().filter_map(Value::as_str).collect();
    let has_public = op_tags.iter().any(|t| PUBLIC_TAGS.contains(t));
    let has_internal = op_tags.iter().any(|t| INTERNAL_TAGS.contains(t));
    has_public && !has_internal
}

fn build_public_spec() -> Result<String, serde_json::Error> {
    let mut spec: Value = serde_json::from_str(OPENAPI_SPEC)?;

    if let Some(tags) = spec.get_mut("tags").and_then(Value::as_array_mut) {
        tags.retain(|tag| {
            tag.get("name")
                .and_then(Value::as_str)
                .map(|name| PUBLIC_TAGS.contains(&name))
                .unwrap_or(false)
        });
    }

    let mut public_paths = Map::new();
    let source_paths = spec
        .get("paths")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for (path, path_item) in source_paths {
        let Some(path_obj) = path_item.as_object() else {
            continue;
        };

        let mut filtered_path = Map::new();
        for (key, val) in path_obj {
            if HTTP_METHODS.contains(&key.as_str()) {
                if should_keep_public_operation(val) {
                    filtered_path.insert(key.clone(), val.clone());
                }
            } else {
                // Preserve path-level metadata (e.g., shared parameters) for remaining operations.
                filtered_path.insert(key.clone(), val.clone());
            }
        }

        let has_ops = filtered_path
            .keys()
            .any(|k| HTTP_METHODS.contains(&k.as_str()));
        if has_ops {
            public_paths.insert(path, Value::Object(filtered_path));
        }
    }

    if let Some(paths) = spec.get_mut("paths") {
        *paths = Value::Object(public_paths);
    }

    serde_json::to_string_pretty(&spec)
}

/// GET /docs — Public API reference (customer-facing).
pub async fn docs_page() -> impl IntoResponse {
    swagger_ui_html("fAuth Public API Reference", "/openapi.json")
}

/// GET /openapi.json — Public OpenAPI spec (internal paths removed).
pub async fn openapi_json() -> impl IntoResponse {
    match build_public_spec() {
        Ok(spec) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            spec,
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to build public OpenAPI spec");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                r#"{"error":"server_error","message":"failed to build public spec"}"#.to_string(),
            )
                .into_response()
        }
    }
}

/// GET /internal/docs — Internal API reference (Forma-only).
pub async fn internal_docs_page() -> impl IntoResponse {
    swagger_ui_html("fAuth Internal API Reference", "/internal/openapi.json")
}

/// GET /internal/openapi.json — Full OpenAPI spec (platform-admin protected).
pub async fn internal_openapi_json() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        OPENAPI_SPEC,
    )
}
