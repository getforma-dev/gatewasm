use crate::error::AppError;

fn is_localhost_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("localhost") || lower.contains("127.0.0.1")
}

fn normalize_server_url(server_url: &str) -> String {
    server_url.trim_end_matches('/').to_string()
}

pub fn default_login_redirect(server_url: &str, slug: &str) -> String {
    format!("{}/t/{}/site", normalize_server_url(server_url), slug)
}

/// Resolve persisted tenant redirect URL into a safe absolute URL.
pub fn resolve_login_redirect(server_url: &str, slug: &str, configured: Option<&str>) -> String {
    let fallback = default_login_redirect(server_url, slug);
    let Some(raw) = configured else {
        return fallback;
    };
    let value = raw.trim();
    if value.is_empty() {
        return fallback;
    }

    if value.starts_with("http://") || value.starts_with("https://") {
        if !server_url.starts_with("http://") && is_localhost_url(value) {
            return fallback;
        }
        return value.to_string();
    }

    if value.starts_with('/') {
        return format!("{}{}", normalize_server_url(server_url), value);
    }

    fallback
}

/// Validate user-provided login redirect URL before storing.
///
/// Stores an absolute URL to avoid callback mismatch during OAuth code exchange.
pub fn validate_login_redirect_input(
    server_url: &str,
    _slug: &str,
    candidate: &str,
) -> Result<String, AppError> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest {
            error: "invalid_login_redirect_url",
            message: "Login redirect URL cannot be empty".into(),
        });
    }

    let is_prod_like = !server_url.starts_with("http://");

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        if is_prod_like && is_localhost_url(trimmed) {
            return Err(AppError::BadRequest {
                error: "invalid_login_redirect_url",
                message: "localhost redirect URLs are not allowed in production".into(),
            });
        }
        return Ok(trimmed.to_string());
    }

    if trimmed.starts_with('/') {
        return Ok(format!("{}{}", normalize_server_url(server_url), trimmed));
    }

    Err(AppError::BadRequest {
        error: "invalid_login_redirect_url",
        message: format!(
            "Login redirect URL '{}' is invalid. Use an absolute http(s) URL or absolute path.",
            trimmed
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_localhost_redirect_to_fallback_in_prod() {
        let resolved = resolve_login_redirect(
            "https://auth.getforma.dev",
            "acme",
            Some("http://localhost:3000/callback"),
        );
        assert_eq!(resolved, "https://auth.getforma.dev/t/acme/site");
    }

    #[test]
    fn preserves_localhost_redirect_in_local_dev() {
        let resolved = resolve_login_redirect(
            "http://localhost:3001",
            "acme",
            Some("http://localhost:3000/callback"),
        );
        assert_eq!(resolved, "http://localhost:3000/callback");
    }

    #[test]
    fn rejects_localhost_input_in_prod() {
        let err = validate_login_redirect_input(
            "https://auth.getforma.dev",
            "acme",
            "http://localhost:3000/callback",
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest { .. }));
    }
}
