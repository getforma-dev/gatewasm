use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

#[derive(Debug)]
pub enum AppError {
    BadRequest {
        error: &'static str,
        message: String,
    },
    Unauthorized {
        error: &'static str,
        message: String,
    },
    Conflict {
        error: &'static str,
        message: String,
    },
    Forbidden {
        error: &'static str,
        message: String,
    },
    NotFound {
        error: &'static str,
        message: String,
    },
    InternalError {
        error: &'static str,
        message: String,
    },
    BadGateway {
        error: &'static str,
        message: String,
    },
    TooManyRequests,
    AccountLocked {
        error: &'static str,
        message: String,
        retry_after: u64,
    },
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_str, message) = match self {
            AppError::BadRequest { error, message } => (StatusCode::BAD_REQUEST, error, message),
            AppError::Unauthorized { error, message } => (StatusCode::UNAUTHORIZED, error, message),
            AppError::Conflict { error, message } => (StatusCode::CONFLICT, error, message),
            AppError::Forbidden { error, message } => (StatusCode::FORBIDDEN, error, message),
            AppError::NotFound { error, message } => (StatusCode::NOT_FOUND, error, message),
            AppError::InternalError { error, message } => {
                (StatusCode::INTERNAL_SERVER_ERROR, error, message)
            }
            AppError::BadGateway { error, message } => (StatusCode::BAD_GATEWAY, error, message),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                "Rate limit exceeded".to_string(),
            ),
            AppError::AccountLocked {
                error,
                message,
                retry_after,
            } => {
                let body = Json(serde_json::json!({
                    "error": error,
                    "message": message,
                    "status": 429,
                    "retry_after": retry_after,
                }));
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("Retry-After", retry_after.to_string())],
                    body,
                )
                    .into_response();
            }
        };

        (
            status,
            Json(serde_json::json!({
                "error": error_str,
                "message": message,
                "status": status.as_u16()
            })),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = %e, "Database error");
        AppError::InternalError {
            error: "server_error",
            message: "Internal server error".to_string(),
        }
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(_: jsonwebtoken::errors::Error) -> Self {
        AppError::InternalError {
            error: "server_error",
            message: "Failed to create token".to_string(),
        }
    }
}

impl From<argon2::password_hash::Error> for AppError {
    fn from(_: argon2::password_hash::Error) -> Self {
        AppError::InternalError {
            error: "server_error",
            message: "Password hashing failed".to_string(),
        }
    }
}
