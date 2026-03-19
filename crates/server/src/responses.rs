use serde::Serialize;

/// Generic message response (used by ~30 handlers).
///
/// # Example
/// ```ignore
/// Ok(Json(MessageResponse { message: "Webhook deleted" }))
/// ```
#[derive(Serialize)]
pub struct MessageResponse {
    pub message: &'static str,
}

/// Paginated list response.
///
/// Note: many existing handlers use custom key names for the items array
/// (e.g. `"users"`, `"tenants"`, `"invites"`). Use this struct for new
/// endpoints or when migrating existing ones that can adopt the `"items"` key.
#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Token response from login/register/refresh.
///
/// Covers the common access-token shape. Handlers that include a `user`
/// object or other extra fields should continue using `serde_json::json!()`
/// until a richer `AuthTokenResponse` struct is introduced.
#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}
