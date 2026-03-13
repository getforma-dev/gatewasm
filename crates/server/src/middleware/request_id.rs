use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

pub async fn add_request_id(mut req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if let Ok(val) = HeaderValue::from_str(&id) {
        req.headers_mut().insert("x-request-id", val);
    }

    let mut res = next.run(req).await;

    if let Ok(val) = HeaderValue::from_str(&id) {
        res.headers_mut().insert("x-request-id", val);
    }

    res
}
