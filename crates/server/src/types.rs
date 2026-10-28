use serde::{Deserialize, Serialize};

// -- OAuth Provider Response Types --

#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    #[allow(dead_code)]
    pub token_type: String,
    #[allow(dead_code)]
    pub expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubTokenResponse {
    pub access_token: String,
    #[allow(dead_code)]
    pub token_type: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubUserInfo {
    pub id: u64,
    #[allow(dead_code)]
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubEmail {
    pub email: String,
    pub primary: bool,
    pub verified: bool,
}

// -- Request types --

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: Option<String>,
    pub invite_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub authorization_request: Option<String>,
    pub email: String,
    pub password: String,
    pub invite_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub code: Option<String>,
    pub code_verifier: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_id: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LogoutRequest {
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeQuery {
    pub client_id: String,
    pub redirect_uri: Option<String>,
    pub response_type: Option<String>,
    #[allow(dead_code)]
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub provider: Option<String>,
    pub invite_token: Option<String>,
    /// PAR request_uri — when present, stored parameters are used instead of query params.
    pub request_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailQuery {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

// -- Response types --

#[derive(Debug, Serialize)]
pub struct JwksResponse {
    pub keys: Vec<JwkKey>,
}

#[derive(Debug, Serialize)]
pub struct JwkKey {
    pub kty: String,
    pub r#use: String,
    pub kid: String,
    pub alg: String,
    /// Ed25519 curve (only for OKP keys)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crv: Option<String>,
    /// Ed25519 public key, base64url-encoded (only for OKP keys)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
}
