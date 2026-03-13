use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::handlers::auth::append_refresh_cookie_headers;
use crate::jwt::signing::{self, Claims};
use crate::middleware::tenant::TenantContext;
use crate::AppState;

/// Maximum number of credentials a user can register.
const MAX_CREDENTIALS_PER_USER: i64 = 10;

/// Challenge validity in minutes.
const CHALLENGE_TTL_MINUTES: i64 = 5;

/// Maximum number of refresh tokens per user (same as auth.rs).
const MAX_REFRESH_TOKENS_PER_USER: i64 = 10;

// ── Request/Response types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterBeginRequest {
    /// Optional friendly name for the credential (e.g., "YubiKey 5", "iPhone").
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterCompleteRequest {
    /// The challenge ID returned by register/begin.
    pub challenge_id: String,
    /// Base64url-encoded credential ID from navigator.credentials.create().
    pub credential_id: String,
    /// Base64url-encoded attestation object from the authenticator.
    pub attestation_object: String,
    /// Base64url-encoded client data JSON.
    pub client_data_json: String,
    /// Transport hints from the authenticator (e.g., ["usb", "ble", "nfc", "internal"]).
    #[serde(default)]
    pub transports: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginBeginRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginCompleteRequest {
    /// The challenge ID returned by login/begin.
    pub challenge_id: String,
    /// Base64url-encoded credential ID used for authentication.
    pub credential_id: String,
    /// Base64url-encoded authenticator data.
    pub authenticator_data: String,
    /// Base64url-encoded client data JSON.
    pub client_data_json: String,
    /// Base64url-encoded signature.
    pub signature: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameCredentialRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
struct CredentialResponse {
    id: String,
    name: String,
    created_at: String,
    last_used_at: Option<String>,
    is_synced: bool,
    transports: Vec<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Check that WebAuthn is enabled for the tenant, returning 403 if not.
fn require_webauthn_enabled(tenant: &TenantContext) -> Result<(), AppError> {
    if !tenant.security_config.webauthn_enabled {
        return Err(AppError::Forbidden {
            error: "webauthn_disabled",
            message: "WebAuthn is not enabled for this tenant".into(),
        });
    }
    Ok(())
}

/// Derive the RP ID from the server URL (hostname only, no port).
fn rp_id_from_server_url(server_url: &str) -> String {
    // Strip scheme
    let without_scheme = server_url
        .strip_prefix("https://")
        .or_else(|| server_url.strip_prefix("http://"))
        .unwrap_or(server_url);
    // Strip port and path
    without_scheme
        .split(':')
        .next()
        .unwrap_or(without_scheme)
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

/// Generate a random challenge (32 bytes) and return it base64url-encoded.
fn generate_challenge() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Clean up expired challenges for this tenant. Called during begin operations.
async fn cleanup_expired_challenges(db: &sqlx::PgPool, tenant_id: &str) {
    let _ = sqlx::query(
        "DELETE FROM webauthn_challenges WHERE tenant_id = $1 AND expires_at < NOW()",
    )
    .bind(tenant_id)
    .execute(db)
    .await;
}

/// Parse the CBOR attestation object to extract the public key and sign count.
/// Returns (public_key_bytes, sign_count, aaguid, is_synced).
///
/// The attestation object is CBOR-encoded with this structure:
/// { "fmt": "none"|"packed"|..., "attStmt": {...}, "authData": <bytes> }
///
/// authData layout:
///   [0..32]  = rpIdHash (SHA-256 of RP ID)
///   [32]     = flags
///   [33..37] = signCount (big-endian u32)
///   [37..53] = aaguid (16 bytes) — only if AT flag set
///   [53..55] = credentialIdLength (big-endian u16)
///   [55..55+L] = credentialId
///   [55+L..] = COSE public key (CBOR)
fn parse_attestation_object(
    attestation_object_b64: &str,
    expected_rp_id: &str,
    expected_origin: &str,
    client_data_json_b64: &str,
) -> Result<(Vec<u8>, u32, Option<String>, bool), AppError> {
    // Decode client data JSON and verify
    let client_data_bytes = URL_SAFE_NO_PAD
        .decode(client_data_json_b64)
        .map_err(|_| AppError::BadRequest {
            error: "invalid_client_data",
            message: "Invalid base64url encoding for client_data_json".into(),
        })?;
    let client_data: serde_json::Value =
        serde_json::from_slice(&client_data_bytes).map_err(|_| AppError::BadRequest {
            error: "invalid_client_data",
            message: "client_data_json is not valid JSON".into(),
        })?;

    // Verify type
    let cd_type = client_data["type"]
        .as_str()
        .unwrap_or("");
    if cd_type != "webauthn.create" {
        return Err(AppError::BadRequest {
            error: "invalid_ceremony_type",
            message: "client data type must be 'webauthn.create'".into(),
        });
    }

    // Verify origin
    let cd_origin = client_data["origin"].as_str().unwrap_or("");
    if cd_origin != expected_origin {
        return Err(AppError::BadRequest {
            error: "origin_mismatch",
            message: format!(
                "Origin mismatch: expected '{}', got '{}'",
                expected_origin, cd_origin
            ),
        });
    }

    // Decode attestation object (simple CBOR map parsing)
    let att_bytes =
        URL_SAFE_NO_PAD
            .decode(attestation_object_b64)
            .map_err(|_| AppError::BadRequest {
                error: "invalid_attestation",
                message: "Invalid base64url encoding for attestation_object".into(),
            })?;

    // We need to extract authData from the CBOR. For simplicity with "none" attestation,
    // we do a minimal CBOR parse to find the authData bytes.
    let auth_data = extract_auth_data_from_cbor(&att_bytes)?;

    // Verify rpIdHash
    if auth_data.len() < 37 {
        return Err(AppError::BadRequest {
            error: "invalid_auth_data",
            message: "authData too short".into(),
        });
    }

    let rp_id_hash = &auth_data[0..32];
    let expected_rp_id_hash = {
        use ring::digest;
        digest::digest(&digest::SHA256, expected_rp_id.as_bytes())
    };
    if rp_id_hash != expected_rp_id_hash.as_ref() {
        return Err(AppError::BadRequest {
            error: "rp_id_mismatch",
            message: "RP ID hash does not match".into(),
        });
    }

    let flags = auth_data[32];
    let _user_present = flags & 0x01 != 0;
    let attested_credential_data = flags & 0x40 != 0;
    let sign_count = u32::from_be_bytes([auth_data[33], auth_data[34], auth_data[35], auth_data[36]]);

    if !attested_credential_data {
        return Err(AppError::BadRequest {
            error: "no_credential_data",
            message: "Attestation does not contain credential data".into(),
        });
    }

    if auth_data.len() < 55 {
        return Err(AppError::BadRequest {
            error: "invalid_auth_data",
            message: "authData too short for attested credential data".into(),
        });
    }

    let aaguid = &auth_data[37..53];
    let aaguid_hex = hex::encode(aaguid);
    let cred_id_len = u16::from_be_bytes([auth_data[53], auth_data[54]]) as usize;

    if auth_data.len() < 55 + cred_id_len {
        return Err(AppError::BadRequest {
            error: "invalid_auth_data",
            message: "authData too short for credential ID".into(),
        });
    }

    // The COSE public key follows the credential ID
    let cose_key_start = 55 + cred_id_len;
    let cose_key_bytes = &auth_data[cose_key_start..];

    // Check if this is a synced credential (backup eligible flag, bit 3)
    let backup_eligible = flags & 0x08 != 0;

    Ok((
        cose_key_bytes.to_vec(),
        sign_count,
        Some(aaguid_hex),
        backup_eligible,
    ))
}

/// Minimal CBOR parser to extract the `authData` byte string from an attestation object.
/// The attestation object is a CBOR map with keys like "fmt", "attStmt", "authData".
fn extract_auth_data_from_cbor(data: &[u8]) -> Result<Vec<u8>, AppError> {
    let err = || AppError::BadRequest {
        error: "invalid_cbor",
        message: "Failed to parse attestation object CBOR".into(),
    };

    if data.is_empty() {
        return Err(err());
    }

    // First byte should be a CBOR map (major type 5)
    let major = data[0] >> 5;
    let additional = data[0] & 0x1f;
    if major != 5 {
        return Err(err());
    }

    let (map_len, mut pos) = decode_cbor_uint(additional, data, 1)?;

    for _ in 0..map_len {
        // Read key (should be a text string, major type 3)
        let (key, new_pos) = decode_cbor_text(data, pos)?;
        pos = new_pos;

        if key == "authData" {
            // Read value as a byte string (major type 2)
            let (bytes, _) = decode_cbor_bytes(data, pos)?;
            return Ok(bytes);
        } else {
            // Skip this value
            pos = skip_cbor_value(data, pos)?;
        }
    }

    Err(err())
}

/// Decode CBOR unsigned integer from additional info.
fn decode_cbor_uint(additional: u8, data: &[u8], pos: usize) -> Result<(u64, usize), AppError> {
    let err = || AppError::BadRequest {
        error: "invalid_cbor",
        message: "Failed to parse CBOR integer".into(),
    };

    if additional < 24 {
        Ok((additional as u64, pos))
    } else if additional == 24 {
        if pos >= data.len() {
            return Err(err());
        }
        Ok((data[pos] as u64, pos + 1))
    } else if additional == 25 {
        if pos + 2 > data.len() {
            return Err(err());
        }
        let val = u16::from_be_bytes([data[pos], data[pos + 1]]) as u64;
        Ok((val, pos + 2))
    } else if additional == 26 {
        if pos + 4 > data.len() {
            return Err(err());
        }
        let val =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as u64;
        Ok((val, pos + 4))
    } else if additional == 27 {
        if pos + 8 > data.len() {
            return Err(err());
        }
        let val = u64::from_be_bytes([
            data[pos],
            data[pos + 1],
            data[pos + 2],
            data[pos + 3],
            data[pos + 4],
            data[pos + 5],
            data[pos + 6],
            data[pos + 7],
        ]);
        Ok((val, pos + 8))
    } else {
        Err(err())
    }
}

/// Decode a CBOR text string at the given position.
fn decode_cbor_text(data: &[u8], pos: usize) -> Result<(String, usize), AppError> {
    let err = || AppError::BadRequest {
        error: "invalid_cbor",
        message: "Failed to parse CBOR text string".into(),
    };

    if pos >= data.len() {
        return Err(err());
    }

    let major = data[pos] >> 5;
    let additional = data[pos] & 0x1f;
    if major != 3 {
        return Err(err());
    }

    let (len, start) = decode_cbor_uint(additional, data, pos + 1)?;
    let len = len as usize;
    let end = start + len;
    if end > data.len() {
        return Err(err());
    }

    let text = std::str::from_utf8(&data[start..end])
        .map_err(|_| err())?
        .to_string();
    Ok((text, end))
}

/// Decode a CBOR byte string at the given position.
fn decode_cbor_bytes(data: &[u8], pos: usize) -> Result<(Vec<u8>, usize), AppError> {
    let err = || AppError::BadRequest {
        error: "invalid_cbor",
        message: "Failed to parse CBOR byte string".into(),
    };

    if pos >= data.len() {
        return Err(err());
    }

    let major = data[pos] >> 5;
    let additional = data[pos] & 0x1f;
    if major != 2 {
        return Err(err());
    }

    let (len, start) = decode_cbor_uint(additional, data, pos + 1)?;
    let len = len as usize;
    let end = start + len;
    if end > data.len() {
        return Err(err());
    }

    Ok((data[start..end].to_vec(), end))
}

/// Skip a CBOR value at the given position, returning the position after it.
fn skip_cbor_value(data: &[u8], pos: usize) -> Result<usize, AppError> {
    let err = || AppError::BadRequest {
        error: "invalid_cbor",
        message: "Failed to skip CBOR value".into(),
    };

    if pos >= data.len() {
        return Err(err());
    }

    let major = data[pos] >> 5;
    let additional = data[pos] & 0x1f;

    match major {
        // Unsigned integer, negative integer
        0 | 1 => {
            let (_, new_pos) = decode_cbor_uint(additional, data, pos + 1)?;
            Ok(new_pos)
        }
        // Byte string, text string
        2 | 3 => {
            let (len, start) = decode_cbor_uint(additional, data, pos + 1)?;
            Ok(start + len as usize)
        }
        // Array
        4 => {
            let (count, mut p) = decode_cbor_uint(additional, data, pos + 1)?;
            for _ in 0..count {
                p = skip_cbor_value(data, p)?;
            }
            Ok(p)
        }
        // Map
        5 => {
            let (count, mut p) = decode_cbor_uint(additional, data, pos + 1)?;
            for _ in 0..count {
                p = skip_cbor_value(data, p)?; // key
                p = skip_cbor_value(data, p)?; // value
            }
            Ok(p)
        }
        // Tag
        6 => {
            let (_, p) = decode_cbor_uint(additional, data, pos + 1)?;
            skip_cbor_value(data, p)
        }
        // Simple / float (major type 7)
        7 => {
            if additional < 24 {
                Ok(pos + 1)
            } else if additional == 24 {
                Ok(pos + 2)
            } else if additional == 25 {
                Ok(pos + 3)
            } else if additional == 26 {
                Ok(pos + 5)
            } else if additional == 27 {
                Ok(pos + 9)
            } else {
                Ok(pos + 1)
            }
        }
        _ => Err(err()),
    }
}

/// Verify an assertion signature for login.
/// Returns the new sign count on success.
fn verify_assertion(
    auth_data_b64: &str,
    client_data_json_b64: &str,
    signature_b64: &str,
    public_key: &[u8],
    expected_rp_id: &str,
    expected_origin: &str,
    expected_challenge: &str,
    stored_sign_count: i64,
) -> Result<u32, AppError> {
    // Decode authenticator data
    let auth_data =
        URL_SAFE_NO_PAD
            .decode(auth_data_b64)
            .map_err(|_| AppError::BadRequest {
                error: "invalid_authenticator_data",
                message: "Invalid base64url encoding for authenticator_data".into(),
            })?;

    // Decode and verify client data JSON
    let client_data_bytes = URL_SAFE_NO_PAD
        .decode(client_data_json_b64)
        .map_err(|_| AppError::BadRequest {
            error: "invalid_client_data",
            message: "Invalid base64url encoding for client_data_json".into(),
        })?;
    let client_data: serde_json::Value =
        serde_json::from_slice(&client_data_bytes).map_err(|_| AppError::BadRequest {
            error: "invalid_client_data",
            message: "client_data_json is not valid JSON".into(),
        })?;

    // Verify type
    let cd_type = client_data["type"].as_str().unwrap_or("");
    if cd_type != "webauthn.get" {
        return Err(AppError::BadRequest {
            error: "invalid_ceremony_type",
            message: "client data type must be 'webauthn.get'".into(),
        });
    }

    // Verify origin
    let cd_origin = client_data["origin"].as_str().unwrap_or("");
    if cd_origin != expected_origin {
        return Err(AppError::BadRequest {
            error: "origin_mismatch",
            message: format!(
                "Origin mismatch: expected '{}', got '{}'",
                expected_origin, cd_origin
            ),
        });
    }

    // Verify challenge
    let cd_challenge = client_data["challenge"].as_str().unwrap_or("");
    if cd_challenge != expected_challenge {
        return Err(AppError::BadRequest {
            error: "challenge_mismatch",
            message: "Challenge does not match".into(),
        });
    }

    // Verify rpIdHash
    if auth_data.len() < 37 {
        return Err(AppError::BadRequest {
            error: "invalid_auth_data",
            message: "authenticatorData too short".into(),
        });
    }
    let rp_id_hash = &auth_data[0..32];
    let expected_rp_id_hash = {
        use ring::digest;
        digest::digest(&digest::SHA256, expected_rp_id.as_bytes())
    };
    if rp_id_hash != expected_rp_id_hash.as_ref() {
        return Err(AppError::BadRequest {
            error: "rp_id_mismatch",
            message: "RP ID hash does not match".into(),
        });
    }

    // Check user present flag
    let flags = auth_data[32];
    let user_present = flags & 0x01 != 0;
    if !user_present {
        return Err(AppError::BadRequest {
            error: "user_not_present",
            message: "User presence flag not set".into(),
        });
    }

    let sign_count =
        u32::from_be_bytes([auth_data[33], auth_data[34], auth_data[35], auth_data[36]]);

    // Cloned credential detection:
    // If the presented sign count > 0 AND stored sign count > 0 AND presented <= stored,
    // the credential may have been cloned.
    if sign_count > 0 && stored_sign_count > 0 && sign_count as i64 <= stored_sign_count {
        return Err(AppError::Forbidden {
            error: "credential_cloned",
            message: "Credential counter indicates possible cloning. Please re-register your passkey.".into(),
        });
    }

    // Verify the signature: sign(authData || SHA-256(clientDataJSON))
    let client_data_hash = {
        use ring::digest;
        digest::digest(&digest::SHA256, &client_data_bytes)
    };

    let mut signed_data = Vec::with_capacity(auth_data.len() + 32);
    signed_data.extend_from_slice(&auth_data);
    signed_data.extend_from_slice(client_data_hash.as_ref());

    let signature =
        URL_SAFE_NO_PAD
            .decode(signature_b64)
            .map_err(|_| AppError::BadRequest {
                error: "invalid_signature",
                message: "Invalid base64url encoding for signature".into(),
            })?;

    // Try to verify with the stored COSE public key.
    // We support ES256 (ECDSA P-256) and Ed25519.
    verify_cose_signature(public_key, &signed_data, &signature)?;

    Ok(sign_count)
}

/// Verify a signature using a COSE public key.
/// Supports ES256 (ECDSA P-256 with SHA-256) and EdDSA (Ed25519).
fn verify_cose_signature(
    cose_key: &[u8],
    signed_data: &[u8],
    signature: &[u8],
) -> Result<(), AppError> {
    // Try to determine the algorithm from the COSE key.
    // COSE key is a CBOR map. Key identifier 3 = algorithm.
    // ES256 = -7, EdDSA = -8, RS256 = -257
    let alg = extract_cose_alg(cose_key).unwrap_or(-7); // default to ES256

    match alg {
        -7 => {
            // ES256: ECDSA P-256 with SHA-256
            // Extract x and y coordinates from COSE key (keys -2 and -3)
            let (x, y) = extract_cose_ec2_coords(cose_key)?;
            // Build uncompressed point: 0x04 || x || y
            let mut uncompressed = Vec::with_capacity(65);
            uncompressed.push(0x04);
            uncompressed.extend_from_slice(&x);
            uncompressed.extend_from_slice(&y);

            let public_key = ring::signature::UnparsedPublicKey::new(
                &ring::signature::ECDSA_P256_SHA256_ASN1,
                &uncompressed,
            );
            public_key
                .verify(signed_data, signature)
                .map_err(|_| AppError::Unauthorized {
                    error: "signature_invalid",
                    message: "WebAuthn signature verification failed".into(),
                })
        }
        -8 => {
            // EdDSA (Ed25519)
            let x = extract_cose_okp_x(cose_key)?;
            let public_key = ring::signature::UnparsedPublicKey::new(
                &ring::signature::ED25519,
                &x,
            );
            public_key
                .verify(signed_data, signature)
                .map_err(|_| AppError::Unauthorized {
                    error: "signature_invalid",
                    message: "WebAuthn signature verification failed".into(),
                })
        }
        _ => Err(AppError::BadRequest {
            error: "unsupported_algorithm",
            message: format!("Unsupported COSE algorithm: {}", alg),
        }),
    }
}

/// Extract the COSE algorithm identifier (key 3) from a CBOR map.
fn extract_cose_alg(data: &[u8]) -> Option<i64> {
    if data.is_empty() {
        return None;
    }
    let major = data[0] >> 5;
    let additional = data[0] & 0x1f;
    if major != 5 {
        return None;
    }

    let (map_len, mut pos) = decode_cbor_uint(additional, data, 1).ok()?;

    for _ in 0..map_len {
        let (key_val, key_is_neg, new_pos) = decode_cbor_int_key(data, pos).ok()?;
        pos = new_pos;

        // Key 3 = algorithm
        if !key_is_neg && key_val == 3 {
            let (val, is_neg, _) = decode_cbor_int_key(data, pos).ok()?;
            return if is_neg {
                Some(-(val as i64 + 1))
            } else {
                Some(val as i64)
            };
        }

        pos = skip_cbor_value(data, pos).ok()?;
    }

    None
}

/// Decode a CBOR integer key (unsigned or negative).
/// Returns (raw_value, is_negative, new_pos).
fn decode_cbor_int_key(
    data: &[u8],
    pos: usize,
) -> Result<(u64, bool, usize), AppError> {
    let err = || AppError::BadRequest {
        error: "invalid_cbor",
        message: "Failed to parse CBOR integer key".into(),
    };

    if pos >= data.len() {
        return Err(err());
    }

    let major = data[pos] >> 5;
    let additional = data[pos] & 0x1f;

    match major {
        0 => {
            let (val, new_pos) = decode_cbor_uint(additional, data, pos + 1)?;
            Ok((val, false, new_pos))
        }
        1 => {
            let (val, new_pos) = decode_cbor_uint(additional, data, pos + 1)?;
            Ok((val, true, new_pos))
        }
        _ => Err(err()),
    }
}

/// Extract x and y coordinates from a COSE EC2 key (keys -2 and -3).
fn extract_cose_ec2_coords(data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let err = || AppError::BadRequest {
        error: "invalid_cose_key",
        message: "Failed to extract EC2 coordinates from COSE key".into(),
    };

    if data.is_empty() {
        return Err(err());
    }

    let major = data[0] >> 5;
    let additional = data[0] & 0x1f;
    if major != 5 {
        return Err(err());
    }

    let (map_len, mut pos) = decode_cbor_uint(additional, data, 1).map_err(|_| err())?;

    let mut x: Option<Vec<u8>> = None;
    let mut y: Option<Vec<u8>> = None;

    for _ in 0..map_len {
        let (key_val, key_is_neg, new_pos) =
            decode_cbor_int_key(data, pos).map_err(|_| err())?;
        pos = new_pos;

        // -2 (x coord) is encoded as major=1, val=1 (meaning -(1+1) = -2)
        // -3 (y coord) is encoded as major=1, val=2 (meaning -(2+1) = -3)
        if key_is_neg && key_val == 1 {
            // x coordinate
            let (bytes, new_pos) = decode_cbor_bytes(data, pos).map_err(|_| err())?;
            x = Some(bytes);
            pos = new_pos;
        } else if key_is_neg && key_val == 2 {
            // y coordinate
            let (bytes, new_pos) = decode_cbor_bytes(data, pos).map_err(|_| err())?;
            y = Some(bytes);
            pos = new_pos;
        } else {
            pos = skip_cbor_value(data, pos).map_err(|_| err())?;
        }
    }

    match (x, y) {
        (Some(x_val), Some(y_val)) => Ok((x_val, y_val)),
        _ => Err(err()),
    }
}

/// Extract the x coordinate from a COSE OKP key (key -2).
fn extract_cose_okp_x(data: &[u8]) -> Result<Vec<u8>, AppError> {
    let err = || AppError::BadRequest {
        error: "invalid_cose_key",
        message: "Failed to extract OKP public key from COSE key".into(),
    };

    if data.is_empty() {
        return Err(err());
    }

    let major = data[0] >> 5;
    let additional = data[0] & 0x1f;
    if major != 5 {
        return Err(err());
    }

    let (map_len, mut pos) = decode_cbor_uint(additional, data, 1).map_err(|_| err())?;

    for _ in 0..map_len {
        let (key_val, key_is_neg, new_pos) =
            decode_cbor_int_key(data, pos).map_err(|_| err())?;
        pos = new_pos;

        if key_is_neg && key_val == 1 {
            // x coordinate (key -2)
            let (bytes, _) = decode_cbor_bytes(data, pos).map_err(|_| err())?;
            return Ok(bytes);
        } else {
            pos = skip_cbor_value(data, pos).map_err(|_| err())?;
        }
    }

    Err(err())
}

// ── Registration Handlers ─────────────────────────────────────────────────

/// POST /auth/webauthn/register/begin
///
/// Requires JWT auth. Generates a challenge and PublicKeyCredentialCreationOptions.
pub async fn register_begin(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<RegisterBeginRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_webauthn_enabled(&tenant)?;

    let user_id = &claims.sub;
    let tenant_id = &tenant.tenant_id;

    // Clean up expired challenges
    cleanup_expired_challenges(&state.db, tenant_id).await;

    // Check credential count limit
    let cred_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM webauthn_credentials WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    if cred_count >= MAX_CREDENTIALS_PER_USER {
        return Err(AppError::BadRequest {
            error: "too_many_credentials",
            message: format!(
                "Maximum of {} credentials per user",
                MAX_CREDENTIALS_PER_USER
            ),
        });
    }

    // Fetch existing credential IDs to exclude from registration
    let exclude_credentials: Vec<String> = sqlx::query_scalar(
        "SELECT encode(credential_id, 'base64') FROM webauthn_credentials WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let challenge = generate_challenge();
    let rp_id = rp_id_from_server_url(&state.config.server_url);
    let credential_name = body
        .name
        .unwrap_or_else(|| "Passkey".to_string());

    let challenge_id = format!("wc_{}", uuid::Uuid::new_v4());
    let expires_at = Utc::now() + Duration::minutes(CHALLENGE_TTL_MINUTES);

    // Store the challenge in the database
    let challenge_data = serde_json::json!({
        "challenge": challenge,
        "rp_id": rp_id,
        "user_id": user_id,
        "credential_name": credential_name,
    });

    sqlx::query(
        "INSERT INTO webauthn_challenges (id, user_id, tenant_id, challenge_data, ceremony_type, expires_at) VALUES ($1, $2, $3, $4, 'registration', $5)",
    )
    .bind(&challenge_id)
    .bind(user_id)
    .bind(tenant_id)
    .bind(&challenge_data)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    // Build PublicKeyCredentialCreationOptions
    let user_handle = URL_SAFE_NO_PAD.encode(user_id.as_bytes());

    let attestation = &tenant.security_config.webauthn_attestation;

    let exclude_list: Vec<serde_json::Value> = exclude_credentials
        .into_iter()
        .map(|cid| {
            serde_json::json!({
                "type": "public-key",
                "id": cid
            })
        })
        .collect();

    // Authenticator selection criteria
    let mut authenticator_selection = serde_json::json!({
        "residentKey": "preferred",
        "userVerification": "preferred",
    });
    if tenant.security_config.webauthn_require_device_bound {
        authenticator_selection["residentKey"] = serde_json::json!("required");
        authenticator_selection["authenticatorAttachment"] = serde_json::json!("platform");
    }

    let options = serde_json::json!({
        "challenge_id": challenge_id,
        "publicKey": {
            "challenge": challenge,
            "rp": {
                "name": tenant.name,
                "id": rp_id,
            },
            "user": {
                "id": user_handle,
                "name": claims.email,
                "displayName": claims.name.unwrap_or_else(|| claims.email.clone()),
            },
            "pubKeyCredParams": [
                { "type": "public-key", "alg": -7 },   // ES256
                { "type": "public-key", "alg": -257 },  // RS256
                { "type": "public-key", "alg": -8 },    // EdDSA
            ],
            "timeout": CHALLENGE_TTL_MINUTES as u64 * 60 * 1000, // milliseconds
            "excludeCredentials": exclude_list,
            "authenticatorSelection": authenticator_selection,
            "attestation": attestation,
        }
    });

    Ok((StatusCode::OK, Json(options)))
}

/// POST /auth/webauthn/register/complete
///
/// Requires JWT auth. Validates the attestation response and stores the credential.
pub async fn register_complete(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<RegisterCompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_webauthn_enabled(&tenant)?;

    let user_id = &claims.sub;
    let tenant_id = &tenant.tenant_id;

    // Fetch and validate the challenge
    let challenge_row = sqlx::query_as::<_, (String, String, serde_json::Value)>(
        "SELECT id, user_id, challenge_data FROM webauthn_challenges WHERE id = $1 AND tenant_id = $2 AND ceremony_type = 'registration' AND expires_at > NOW()",
    )
    .bind(&body.challenge_id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::BadRequest {
        error: "invalid_challenge",
        message: "Challenge not found or expired".into(),
    })?;

    let (challenge_id, challenge_user_id, challenge_data) = challenge_row;

    // Verify the challenge belongs to this user
    if challenge_user_id != *user_id {
        return Err(AppError::Forbidden {
            error: "challenge_user_mismatch",
            message: "Challenge does not belong to this user".into(),
        });
    }

    let expected_rp_id = challenge_data["rp_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let credential_name = challenge_data["credential_name"]
        .as_str()
        .unwrap_or("Passkey")
        .to_string();

    // Parse the attestation object and verify
    let (public_key, sign_count, aaguid, is_synced) = parse_attestation_object(
        &body.attestation_object,
        &expected_rp_id,
        &state.config.server_url,
        &body.client_data_json,
    )?;

    // Check device-bound requirement
    if tenant.security_config.webauthn_require_device_bound && is_synced {
        return Err(AppError::Forbidden {
            error: "synced_credential_not_allowed",
            message: "This tenant requires device-bound credentials (not synced passkeys)".into(),
        });
    }

    // Decode credential ID
    let credential_id_bytes =
        URL_SAFE_NO_PAD
            .decode(&body.credential_id)
            .map_err(|_| AppError::BadRequest {
                error: "invalid_credential_id",
                message: "Invalid base64url encoding for credential_id".into(),
            })?;

    // Check for duplicate credential ID
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM webauthn_credentials WHERE tenant_id = $1 AND credential_id = $2",
    )
    .bind(tenant_id)
    .bind(&credential_id_bytes)
    .fetch_optional(&state.db)
    .await?;

    if existing.is_some() {
        return Err(AppError::Conflict {
            error: "credential_exists",
            message: "This credential is already registered".into(),
        });
    }

    // Store the credential
    let cred_id = format!("wak_{}", uuid::Uuid::new_v4());
    let transports: Vec<String> = body.transports.clone();

    sqlx::query(
        "INSERT INTO webauthn_credentials (id, credential_id, public_key, sign_count, is_synced, transports, name, user_id, tenant_id, aaguid) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(&cred_id)
    .bind(&credential_id_bytes)
    .bind(&public_key)
    .bind(sign_count as i64)
    .bind(is_synced)
    .bind(&transports)
    .bind(&credential_name)
    .bind(user_id)
    .bind(tenant_id)
    .bind(&aaguid)
    .execute(&state.db)
    .await?;

    // Delete the challenge
    sqlx::query("DELETE FROM webauthn_challenges WHERE id = $1")
        .bind(&challenge_id)
        .execute(&state.db)
        .await?;

    tracing::info!(
        user_id = %user_id,
        tenant_id = %tenant_id,
        credential_id = %cred_id,
        "WebAuthn credential registered"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": cred_id,
            "name": credential_name,
            "is_synced": is_synced,
            "transports": transports,
        })),
    ))
}

// ── Authentication Handlers ───────────────────────────────────────────────

/// POST /auth/webauthn/login/begin
///
/// Public (no JWT required). Generates a challenge for authentication.
pub async fn login_begin(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Json(body): Json<LoginBeginRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_webauthn_enabled(&tenant)?;

    let tenant_id = &tenant.tenant_id;
    let email = body.email.trim().to_lowercase();

    // Clean up expired challenges
    cleanup_expired_challenges(&state.db, tenant_id).await;

    // Look up user by email in this tenant
    let user_row = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE email = $1 AND tenant_id = $2",
    )
    .bind(&email)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?;

    let user_id = match user_row {
        Some((uid,)) => uid,
        None => {
            // Don't reveal whether user exists — return a valid-looking challenge
            // that will fail at the complete step. This prevents user enumeration.
            let challenge = generate_challenge();
            let rp_id = rp_id_from_server_url(&state.config.server_url);
            let challenge_id = format!("wc_{}", uuid::Uuid::new_v4());
            return Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "challenge_id": challenge_id,
                    "publicKey": {
                        "challenge": challenge,
                        "rpId": rp_id,
                        "allowCredentials": [],
                        "timeout": CHALLENGE_TTL_MINUTES as u64 * 60 * 1000,
                        "userVerification": "preferred",
                    }
                })),
            ));
        }
    };

    // Fetch user's credentials
    let credentials: Vec<(Vec<u8>, Vec<String>)> = sqlx::query_as(
        "SELECT credential_id, transports FROM webauthn_credentials WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;

    if credentials.is_empty() {
        return Err(AppError::NotFound {
            error: "no_credentials",
            message: "No WebAuthn credentials found for this user".into(),
        });
    }

    let challenge = generate_challenge();
    let rp_id = rp_id_from_server_url(&state.config.server_url);
    let challenge_id = format!("wc_{}", uuid::Uuid::new_v4());
    let expires_at = Utc::now() + Duration::minutes(CHALLENGE_TTL_MINUTES);

    let challenge_data = serde_json::json!({
        "challenge": challenge,
        "rp_id": rp_id,
        "user_id": user_id,
        "email": email,
    });

    sqlx::query(
        "INSERT INTO webauthn_challenges (id, user_id, tenant_id, challenge_data, ceremony_type, expires_at) VALUES ($1, $2, $3, $4, 'authentication', $5)",
    )
    .bind(&challenge_id)
    .bind(&user_id)
    .bind(tenant_id)
    .bind(&challenge_data)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    let allow_credentials: Vec<serde_json::Value> = credentials
        .into_iter()
        .map(|(cid, transports)| {
            let cid_b64 = URL_SAFE_NO_PAD.encode(&cid);
            let mut entry = serde_json::json!({
                "type": "public-key",
                "id": cid_b64,
            });
            if !transports.is_empty() {
                entry["transports"] = serde_json::json!(transports);
            }
            entry
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "challenge_id": challenge_id,
            "publicKey": {
                "challenge": challenge,
                "rpId": rp_id,
                "allowCredentials": allow_credentials,
                "timeout": CHALLENGE_TTL_MINUTES as u64 * 60 * 1000,
                "userVerification": "preferred",
            }
        })),
    ))
}

/// POST /auth/webauthn/login/complete
///
/// Public (no JWT required). Validates the assertion and issues tokens.
pub async fn login_complete(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Json(body): Json<LoginCompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_webauthn_enabled(&tenant)?;

    let tenant_id = &tenant.tenant_id;

    // Fetch and validate the challenge
    let challenge_row = sqlx::query_as::<_, (String, String, serde_json::Value)>(
        "SELECT id, user_id, challenge_data FROM webauthn_challenges WHERE id = $1 AND tenant_id = $2 AND ceremony_type = 'authentication' AND expires_at > NOW()",
    )
    .bind(&body.challenge_id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::BadRequest {
        error: "invalid_challenge",
        message: "Challenge not found or expired".into(),
    })?;

    let (challenge_id, user_id, challenge_data) = challenge_row;
    let expected_challenge = challenge_data["challenge"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let expected_rp_id = challenge_data["rp_id"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // Decode credential ID from the request
    let credential_id_bytes =
        URL_SAFE_NO_PAD
            .decode(&body.credential_id)
            .map_err(|_| AppError::BadRequest {
                error: "invalid_credential_id",
                message: "Invalid base64url encoding for credential_id".into(),
            })?;

    // Look up the credential
    let cred_row = sqlx::query_as::<_, (String, Vec<u8>, i64)>(
        "SELECT id, public_key, sign_count FROM webauthn_credentials WHERE credential_id = $1 AND user_id = $2 AND tenant_id = $3",
    )
    .bind(&credential_id_bytes)
    .bind(&user_id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "credential_not_found",
        message: "WebAuthn credential not found".into(),
    })?;

    let (wak_id, public_key, stored_sign_count) = cred_row;

    // Verify the assertion
    let new_sign_count = verify_assertion(
        &body.authenticator_data,
        &body.client_data_json,
        &body.signature,
        &public_key,
        &expected_rp_id,
        &state.config.server_url,
        &expected_challenge,
        stored_sign_count,
    )?;

    // Update sign count and last_used_at
    sqlx::query(
        "UPDATE webauthn_credentials SET sign_count = $1, last_used_at = NOW() WHERE id = $2",
    )
    .bind(new_sign_count as i64)
    .bind(&wak_id)
    .execute(&state.db)
    .await?;

    // Delete the challenge
    sqlx::query("DELETE FROM webauthn_challenges WHERE id = $1")
        .bind(&challenge_id)
        .execute(&state.db)
        .await?;

    // Look up user details and issue tokens (same pattern as password login)
    let user_row = sqlx::query_as::<_, (String, String, Option<String>, bool)>(
        "SELECT id, email, name, COALESCE(email_verified, FALSE) FROM users WHERE id = $1 AND tenant_id = $2",
    )
    .bind(&user_id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound {
        error: "user_not_found",
        message: "User not found".into(),
    })?;

    let (db_user_id, db_email, db_name, email_verified) = user_row;

    // Check if user is blocked
    let is_blocked: bool = sqlx::query_scalar(
        "SELECT COALESCE(is_blocked, FALSE) FROM users WHERE id = $1",
    )
    .bind(&db_user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    if is_blocked {
        return Err(AppError::Forbidden {
            error: "user_blocked",
            message: "Your account has been blocked".into(),
        });
    }

    // Fetch roles
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT r.name FROM roles r JOIN user_roles ur ON ur.role_id = r.id WHERE ur.user_id = $1",
    )
    .bind(&db_user_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_else(|_| vec!["user".to_string()]);

    // Create tokens
    let (access_token, expires_at) = signing::create_access_token(
        signing::AccessTokenInput {
            user_id: &db_user_id,
            email: &db_email,
            name: db_name.as_deref(),
            tenant_id: Some(tenant_id),
            roles: &roles,
            auth_method: "webauthn",
            mfa_verified: true, // WebAuthn is a strong authenticator
            cnf: None,
        },
        &state.config,
        &state.keys,
    )?;

    let refresh_token = signing::create_refresh_token();
    let refresh_hash = signing::hash_token(&refresh_token);
    let refresh_expires =
        Utc::now() + chrono::Duration::seconds(state.config.refresh_token_expiry as i64);
    let family_id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, family_id, tenant_id, mfa_verified, auth_method, auth_provider, last_activity_at) VALUES ($1, $2, $3, $4, $5, $6, TRUE, 'webauthn', 'webauthn', NOW())",
    )
    .bind(format!("rt_{}", uuid::Uuid::new_v4()))
    .bind(&db_user_id)
    .bind(&refresh_hash)
    .bind(refresh_expires)
    .bind(&family_id)
    .bind(tenant_id)
    .execute(&state.db)
    .await?;

    crate::db::enforce_refresh_token_limit(&state.db, &db_user_id, MAX_REFRESH_TOKENS_PER_USER)
        .await;

    tracing::info!(
        user_id = %db_user_id,
        tenant_id = %tenant_id,
        auth_method = "webauthn",
        "WebAuthn login success"
    );

    // Dispatch webhook event
    super::webhook_engine::dispatch_event(
        &state.db,
        tenant_id,
        "user.logged_in",
        serde_json::json!({ "user_id": db_user_id, "email": db_email, "auth_method": "webauthn" }),
    )
    .await;

    let mut headers = HeaderMap::new();
    append_refresh_cookie_headers(
        &mut headers,
        &refresh_token,
        state.config.refresh_token_expiry,
        &state.config.server_url,
        Some(&tenant.slug),
    );

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": "Bearer",
            "expires_in": state.config.access_token_expiry,
            "expires_at": expires_at,
            "user": {
                "id": db_user_id,
                "email": db_email,
                "name": db_name,
                "avatar": null,
                "email_verified": email_verified,
                "roles": roles,
            }
        })),
    ))
}

// ── Credential Management Handlers ────────────────────────────────────────

/// GET /auth/webauthn/credentials
///
/// Requires JWT auth. Lists the authenticated user's WebAuthn credentials.
pub async fn list_credentials(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    require_webauthn_enabled(&tenant)?;

    let rows: Vec<(String, String, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>, bool, Vec<String>)> = sqlx::query_as(
        "SELECT id, name, created_at, last_used_at, is_synced, transports FROM webauthn_credentials WHERE user_id = $1 AND tenant_id = $2 ORDER BY created_at ASC",
    )
    .bind(&claims.sub)
    .bind(&tenant.tenant_id)
    .fetch_all(&state.db)
    .await?;

    let credentials: Vec<CredentialResponse> = rows
        .into_iter()
        .map(|(id, name, created_at, last_used_at, is_synced, transports)| CredentialResponse {
            id,
            name,
            created_at: created_at.to_rfc3339(),
            last_used_at: last_used_at.map(|t| t.to_rfc3339()),
            is_synced,
            transports,
        })
        .collect();

    Ok((StatusCode::OK, Json(serde_json::json!({ "credentials": credentials }))))
}

/// PATCH /auth/webauthn/credentials/:id
///
/// Requires JWT auth. Renames a credential.
pub async fn rename_credential(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Extension(claims): Extension<Claims>,
    Path(credential_id): Path<String>,
    Json(body): Json<RenameCredentialRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_webauthn_enabled(&tenant)?;

    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::BadRequest {
            error: "invalid_name",
            message: "Credential name must be 1-100 characters".into(),
        });
    }

    let result = sqlx::query(
        "UPDATE webauthn_credentials SET name = $1 WHERE id = $2 AND user_id = $3 AND tenant_id = $4",
    )
    .bind(&name)
    .bind(&credential_id)
    .bind(&claims.sub)
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "credential_not_found",
            message: "WebAuthn credential not found".into(),
        });
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "id": credential_id,
            "name": name,
        })),
    ))
}

/// DELETE /auth/webauthn/credentials/:id
///
/// Requires JWT auth. Deletes a credential.
pub async fn delete_credential(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Extension(claims): Extension<Claims>,
    Path(credential_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    require_webauthn_enabled(&tenant)?;

    let result = sqlx::query(
        "DELETE FROM webauthn_credentials WHERE id = $1 AND user_id = $2 AND tenant_id = $3",
    )
    .bind(&credential_id)
    .bind(&claims.sub)
    .bind(&tenant.tenant_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            error: "credential_not_found",
            message: "WebAuthn credential not found".into(),
        });
    }

    tracing::info!(
        user_id = %claims.sub,
        credential_id = %credential_id,
        "WebAuthn credential deleted"
    );

    Ok(StatusCode::NO_CONTENT)
}
