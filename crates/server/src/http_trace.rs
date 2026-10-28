//! Response logging deliberately excludes headers and redirect contents.
use axum::http::Response;
use std::time::Duration;

/// Shared by the production TraceLayer and its logging regression test.
/// Redirect paths, queries and fragments can all carry authentication capabilities.
pub fn log_response<B>(response: &Response<B>, latency: Duration, span: &tracing::Span) {
    let status = response.status().as_u16();
    let latency_ms = latency.as_millis() as u64;
    if (300..400).contains(&status) {
        tracing::warn!(parent: span, status, latency_ms, "HTTP redirect response");
    } else if status >= 500 {
        tracing::error!(parent: span, status, latency_ms, "HTTP server error response");
    } else if status >= 400 {
        tracing::warn!(parent: span, status, latency_ms, "HTTP client error response");
    } else {
        tracing::info!(parent: span, status, latency_ms, "HTTP response completed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{header::LOCATION, Request},
        response::Redirect,
        routing::get,
        Router,
    };
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };
    use tower::ServiceExt;
    use tower_http::trace::TraceLayer;

    #[derive(Clone)]
    struct CapturedLogs(Arc<Mutex<Vec<u8>>>);
    impl Write for CapturedLogs {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn redirect_response_logs_metadata_without_location_capabilities() {
        const LOCATION_VALUE: &str = "https://consumer.example.test/SECRET_PATH?code=SECRET_CODE&state=SECRET_STATE&authorization_request=SECRET_REQUEST#SECRET_FRAGMENT";
        let captured = CapturedLogs(Arc::new(Mutex::new(Vec::new())));
        let writer = captured.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let app = Router::new()
            .route("/synthetic/redirect", get(|| async { Redirect::temporary(LOCATION_VALUE) }))
            .layer(TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| tracing::info_span!("http.request", path = %request.uri().path()))
                .on_response(log_response));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/synthetic/redirect")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 307);
        assert_eq!(response.headers()[LOCATION], LOCATION_VALUE);
        let logs = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        for observable in [
            "HTTP redirect response",
            "status=307",
            "latency_ms=",
            "/synthetic/redirect",
        ] {
            assert!(
                logs.contains(observable),
                "Logging must remain enabled with response metadata"
            );
        }
        for forbidden in [
            "consumer.example.test",
            "SECRET_PATH",
            "SECRET_CODE",
            "SECRET_STATE",
            "SECRET_REQUEST",
            "SECRET_FRAGMENT",
            "location=",
        ] {
            assert!(
                !logs.contains(forbidden),
                "Redirect capability leaked through response logging"
            );
        }
    }
}
