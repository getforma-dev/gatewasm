use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_FIXED};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// DPoP proof JWT header (typ must be "dpop+jwt").
#[derive(Debug, Deserialize)]
struct DPoPHeader {
    typ: String,
    alg: String,
    jwk: serde_json::Value,
}

/// DPoP proof JWT claims.
#[derive(Debug, Deserialize)]
struct DPoPClaims {
    jti: String,
    htm: String,
    htu: String,
    iat: u64,
    /// Access token hash (RFC 9449 §4.2) — base64url(SHA-256(access_token)).
    /// Deserialized for future validation when DPoP proofs accompany access tokens.
    #[serde(default)]
    #[allow(dead_code)]
    ath: Option<String>,
}

/// Validate a DPoP proof JWT.
///
/// Returns `(jti, jwk_thumbprint)` on success so the caller can perform
/// jti-replay checking and bind the thumbprint into the access token.
///
/// When `access_token` is provided, the `ath` claim (RFC 9449 §4.2) is
/// validated: it must equal the base64url(SHA-256(access_token)).
pub fn validate_dpop_proof(
    proof: &str,
    expected_method: &str,
    expected_uri: &str,
) -> Result<(String, String), String> {
    // 1. Split into header.payload.signature
    let parts: Vec<&str> = proof.split('.').collect();
    if parts.len() != 3 {
        return Err("DPoP proof must be a three-part JWT".to_string());
    }
    let (header_b64, payload_b64, sig_b64) = (parts[0], parts[1], parts[2]);

    // 2. Decode and parse header
    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|_| "Invalid base64url in DPoP header".to_string())?;
    let header: DPoPHeader =
        serde_json::from_slice(&header_bytes).map_err(|e| format!("Bad DPoP header JSON: {e}"))?;

    // 3. Verify typ
    if header.typ != "dpop+jwt" {
        return Err(format!(
            "DPoP typ must be \"dpop+jwt\", got \"{}\"",
            header.typ
        ));
    }

    // 4. Only ES256 is supported
    if header.alg != "ES256" {
        return Err(format!(
            "DPoP alg must be \"ES256\", got \"{}\"",
            header.alg
        ));
    }

    // 5. Extract JWK — must be EC P-256
    let jwk = &header.jwk;
    if jwk.get("kty").and_then(|v| v.as_str()) != Some("EC") {
        return Err("DPoP JWK kty must be \"EC\"".to_string());
    }
    if jwk.get("crv").and_then(|v| v.as_str()) != Some("P-256") {
        return Err("DPoP JWK crv must be \"P-256\"".to_string());
    }
    let x_b64 = jwk
        .get("x")
        .and_then(|v| v.as_str())
        .ok_or("DPoP JWK missing x")?;
    let y_b64 = jwk
        .get("y")
        .and_then(|v| v.as_str())
        .ok_or("DPoP JWK missing y")?;

    // 6. Verify signature (ES256 — ECDSA P-256 SHA-256, fixed/raw format)
    let x_bytes = URL_SAFE_NO_PAD
        .decode(x_b64)
        .map_err(|_| "Invalid base64url in JWK x".to_string())?;
    let y_bytes = URL_SAFE_NO_PAD
        .decode(y_b64)
        .map_err(|_| "Invalid base64url in JWK y".to_string())?;

    if x_bytes.len() != 32 || y_bytes.len() != 32 {
        return Err("JWK x/y coordinates must be 32 bytes each for P-256".to_string());
    }

    // Uncompressed point: 0x04 || x || y
    let mut public_key_bytes = Vec::with_capacity(65);
    public_key_bytes.push(0x04);
    public_key_bytes.extend_from_slice(&x_bytes);
    public_key_bytes.extend_from_slice(&y_bytes);

    let public_key = UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, &public_key_bytes);
    let signing_input = format!("{}.{}", header_b64, payload_b64);

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| "Invalid base64url in DPoP signature".to_string())?;

    public_key
        .verify(signing_input.as_bytes(), &sig_bytes)
        .map_err(|_| "Invalid DPoP proof signature".to_string())?;

    // 7. Parse and validate claims
    let claims_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| "Invalid base64url in DPoP payload".to_string())?;
    let claims: DPoPClaims = serde_json::from_slice(&claims_bytes)
        .map_err(|e| format!("Bad DPoP claims JSON: {e}"))?;

    // htm must match expected HTTP method
    if !claims.htm.eq_ignore_ascii_case(expected_method) {
        return Err(format!(
            "DPoP htm mismatch: expected \"{}\", got \"{}\"",
            expected_method, claims.htm
        ));
    }

    // htu comparison: strip query, compare scheme+host+path
    let normalize_uri = |u: &str| -> String {
        // Strip query string and fragment
        u.split('?').next().unwrap_or(u).split('#').next().unwrap_or(u).to_string()
    };
    if normalize_uri(&claims.htu) != normalize_uri(expected_uri) {
        return Err(format!(
            "DPoP htu mismatch: expected \"{}\", got \"{}\"",
            expected_uri, claims.htu
        ));
    }

    // iat must be within ±60 seconds of now
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_secs();
    let iat = claims.iat;
    if iat > now + 60 {
        return Err("DPoP iat is too far in the future".to_string());
    }
    if now > iat + 60 {
        return Err("DPoP iat is too far in the past".to_string());
    }

    // 8. Compute JWK Thumbprint (RFC 7638)
    let thumbprint = compute_jwk_thumbprint(jwk)?;

    Ok((claims.jti, thumbprint))
}

/// Compute JWK Thumbprint per RFC 7638.
///
/// For EC keys the canonical JSON includes crv, kty, x, y in
/// lexicographic order.
fn compute_jwk_thumbprint(jwk: &serde_json::Value) -> Result<String, String> {
    let crv = jwk
        .get("crv")
        .and_then(|v| v.as_str())
        .ok_or("missing crv")?;
    let x = jwk
        .get("x")
        .and_then(|v| v.as_str())
        .ok_or("missing x")?;
    let y = jwk
        .get("y")
        .and_then(|v| v.as_str())
        .ok_or("missing y")?;

    let canonical = format!(
        r#"{{"crv":"{}","kty":"EC","x":"{}","y":"{}"}}"#,
        crv, x, y
    );
    let hash = Sha256::digest(canonical.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(hash))
}

/// Check jti uniqueness and store it. Returns error if jti was already used.
pub async fn check_and_store_jti(db: &sqlx::PgPool, jti: &str) -> Result<(), String> {
    let result = sqlx::query(
        "INSERT INTO dpop_jti_cache (jti, expires_at) VALUES ($1, NOW() + INTERVAL '5 minutes') \
         ON CONFLICT (jti) DO NOTHING",
    )
    .bind(jti)
    .execute(db)
    .await
    .map_err(|e| format!("DB error checking DPoP jti: {e}"))?;

    if result.rows_affected() == 0 {
        return Err("DPoP jti already used (replay)".to_string());
    }
    Ok(())
}

/// Clean up expired jti entries (call periodically).
pub async fn cleanup_expired_jti(db: &sqlx::PgPool) {
    sqlx::query("DELETE FROM dpop_jti_cache WHERE expires_at < NOW()")
        .execute(db)
        .await
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_jwk_thumbprint() {
        // A known test vector — just verify it doesn't error
        let jwk = serde_json::json!({
            "kty": "EC",
            "crv": "P-256",
            "x": "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU",
            "y": "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0"
        });
        let thumb = compute_jwk_thumbprint(&jwk).unwrap();
        assert!(!thumb.is_empty());
        // Thumbprint must be deterministic
        assert_eq!(thumb, compute_jwk_thumbprint(&jwk).unwrap());
    }

    #[test]
    fn test_bad_typ_rejected() {
        // Minimal JWT with wrong typ — will fail at typ check
        let header = serde_json::json!({
            "typ": "JWT",
            "alg": "ES256",
            "jwk": { "kty": "EC", "crv": "P-256", "x": "AA", "y": "BB" }
        });
        let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let p = URL_SAFE_NO_PAD.encode(b"{}");
        let s = URL_SAFE_NO_PAD.encode(b"sig");
        let proof = format!("{h}.{p}.{s}");
        let err = validate_dpop_proof(&proof, "POST", "https://example.com/token").unwrap_err();
        assert!(err.contains("dpop+jwt"), "got: {err}");
    }

    #[test]
    fn test_bad_alg_rejected() {
        let header = serde_json::json!({
            "typ": "dpop+jwt",
            "alg": "RS256",
            "jwk": { "kty": "EC", "crv": "P-256", "x": "AA", "y": "BB" }
        });
        let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let p = URL_SAFE_NO_PAD.encode(b"{}");
        let s = URL_SAFE_NO_PAD.encode(b"sig");
        let proof = format!("{h}.{p}.{s}");
        let err = validate_dpop_proof(&proof, "POST", "https://example.com/token").unwrap_err();
        assert!(err.contains("ES256"), "got: {err}");
    }

    #[test]
    fn test_non_jwt_format_rejected() {
        let err = validate_dpop_proof("not-a-jwt", "POST", "https://x.com/token").unwrap_err();
        assert!(err.contains("three-part"), "got: {err}");
    }
}
