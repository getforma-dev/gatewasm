use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::AppConfig;
use crate::jwt::keys::KeyPair;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub iss: String,
    pub aud: String,
    pub exp: u64,
    pub iat: u64,
    pub roles: Vec<String>,
    #[serde(default = "default_auth_method")]
    pub auth_method: String,
    #[serde(default)]
    pub mfa_verified: bool,
    /// DPoP confirmation claim (RFC 9449). Contains `{ "jkt": "<thumbprint>" }`
    /// when the access token is sender-constrained via DPoP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cnf: Option<serde_json::Value>,
}

fn default_auth_method() -> String {
    "password".to_string()
}

pub struct AccessTokenInput<'a> {
    pub user_id: &'a str,
    pub email: &'a str,
    pub name: Option<&'a str>,
    pub tenant_id: Option<&'a str>,
    pub roles: &'a [String],
    pub auth_method: &'a str,
    pub mfa_verified: bool,
    /// DPoP confirmation claim. Set to `Some(json!({"jkt": "<thumbprint>"}))` for
    /// sender-constrained tokens.
    pub cnf: Option<serde_json::Value>,
}

pub fn create_access_token(
    input: AccessTokenInput<'_>,
    config: &AppConfig,
    keys: &KeyPair,
) -> Result<(String, u64), jsonwebtoken::errors::Error> {
    let now = Utc::now().timestamp() as u64;
    let exp = now + config.access_token_expiry;

    let claims = Claims {
        sub: input.user_id.to_string(),
        email: input.email.to_string(),
        name: input.name.map(|s| s.to_string()),
        tenant_id: input.tenant_id.map(|s| s.to_string()),
        iss: config.jwt_issuer.clone(),
        aud: config.jwt_audience.clone(),
        exp,
        iat: now,
        roles: input.roles.to_vec(),
        auth_method: input.auth_method.to_string(),
        mfa_verified: input.mfa_verified,
        cnf: input.cnf,
    };

    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(keys.kid.clone());

    let token = encode(&header, &claims, &keys.encoding_key)?;
    Ok((token, exp))
}

pub fn create_refresh_token() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

pub fn verify_access_token(
    token: &str,
    config: &AppConfig,
    keys: &KeyPair,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_issuer(&[&config.jwt_issuer]);
    validation.set_audience(&[&config.jwt_audience]);

    let data = decode::<Claims>(token, &keys.decoding_key, &validation)?;
    Ok(data.claims)
}
