use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{DecodingKey, EncodingKey};
use ring::signature::{Ed25519KeyPair as RingEd25519, KeyPair as RingKeyPair};
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Clone)]
pub struct KeyPair {
    pub kid: String,
    pub encoding_key: Arc<EncodingKey>,
    pub decoding_key: Arc<DecodingKey>,
    /// Raw 32-byte Ed25519 public key (for JWKS `x` field)
    pub public_key_bytes: Vec<u8>,
}

impl KeyPair {
    /// Compute a stable key ID from the raw 32-byte Ed25519 public key.
    ///
    /// kid = base64url(SHA-256(public_key_bytes)[0..8])  — 11 URL-safe characters.
    pub fn compute_kid(public_key_bytes: &[u8]) -> String {
        let hash = Sha256::digest(public_key_bytes);
        URL_SAFE_NO_PAD.encode(&hash[..8])
    }

    /// Load from a PEM-encoded Ed25519 private key, or auto-generate an ephemeral keypair.
    ///
    /// Ephemeral keys mean tokens won't survive a server restart — suitable for dev.
    /// For production, set the `ED25519_PRIVATE_KEY` env var with a PKCS#8 PEM key.
    pub fn from_pem_or_generate(pem: Option<&str>) -> Self {
        match pem {
            Some(pem_str) => Self::from_pem(pem_str),
            None => {
                tracing::warn!(
                    "No ED25519_PRIVATE_KEY set — generating ephemeral keypair (tokens won't survive restart)"
                );
                Self::generate()
            }
        }
    }

    fn from_pem(pem_str: &str) -> Self {
        let encoding_key =
            EncodingKey::from_ed_pem(pem_str.as_bytes()).expect("Invalid Ed25519 PEM private key");

        // Extract public key bytes from PEM for JWKS and decoding key.
        // ring only supports PKCS#8 v1, but OpenSSL 3.x generates v2.
        // Try ring first; on failure, extract the public key from the v2 DER structure.
        let pkcs8_der = pem::parse(pem_str).expect("Invalid PEM format");
        let public_key_bytes = match RingEd25519::from_pkcs8(pkcs8_der.contents()) {
            Ok(pair) => pair.public_key().as_ref().to_vec(),
            Err(_) => extract_ed25519_pubkey_from_pkcs8v2(pkcs8_der.contents())
                .expect("Ed25519 key is neither valid PKCS#8 v1 (ring) nor v2 (OpenSSL 3.x)"),
        };

        let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);
        let kid = Self::compute_kid(&public_key_bytes);

        Self {
            kid,
            encoding_key: Arc::new(encoding_key),
            decoding_key: Arc::new(decoding_key),
            public_key_bytes,
        }
    }

    fn generate() -> Self {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_doc = RingEd25519::generate_pkcs8(&rng).expect("Failed to generate Ed25519 key");
        let pair =
            RingEd25519::from_pkcs8(pkcs8_doc.as_ref()).expect("Failed to parse generated key");
        let public_key_bytes = pair.public_key().as_ref().to_vec();

        let encoding_key = EncodingKey::from_ed_der(pkcs8_doc.as_ref());
        let decoding_key = DecodingKey::from_ed_der(public_key_bytes.as_slice());
        let kid = Self::compute_kid(&public_key_bytes);

        Self {
            kid,
            encoding_key: Arc::new(encoding_key),
            decoding_key: Arc::new(decoding_key),
            public_key_bytes,
        }
    }
}

/// Extract the 32-byte Ed25519 public key from a PKCS#8 v2 DER structure.
///
/// PKCS#8 v2 (RFC 5958) appends the public key in a context-specific [1] tag:
///   SEQUENCE {
///     INTEGER 1
///     SEQUENCE { OID 1.3.101.112 }
///     OCTET STRING { OCTET STRING { 32-byte seed } }
///     [1] { BIT STRING { 0x00 + 32-byte public key } }
///   }
fn extract_ed25519_pubkey_from_pkcs8v2(der: &[u8]) -> Option<Vec<u8>> {
    // Scan for context-specific tag [1] (0xa1) containing a BIT STRING with the public key
    let mut i = 0;
    while i + 36 < der.len() {
        if der[i] == 0xa1 {
            let len = der[i + 1] as usize;
            if len >= 35 && i + 2 + len <= der.len() {
                // Expect BIT STRING tag (0x03)
                if der[i + 2] == 0x03 && der[i + 3] == 0x21 && der[i + 4] == 0x00 {
                    return Some(der[i + 5..i + 5 + 32].to_vec());
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_generates_v2_key() {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_doc = RingEd25519::generate_pkcs8(&rng).unwrap();
        let der = pkcs8_doc.as_ref();
        // ring 0.17 generates PKCS#8 v2 (83 bytes, version=1)
        assert_eq!(der.len(), 83);
        assert_eq!(der[4], 0x01); // version 1
                                  // Should round-trip
        let pair = RingEd25519::from_pkcs8(der).unwrap();
        assert_eq!(pair.public_key().as_ref().len(), 32);
    }

    #[test]
    fn test_generate_pem_for_secrets_manager() {
        // Utility: run with --nocapture to get a PEM key for AWS Secrets Manager
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_doc = RingEd25519::generate_pkcs8(&rng).unwrap();
        let pem_str = pem::encode(&pem::Pem::new("PRIVATE KEY", pkcs8_doc.as_ref().to_vec()));
        eprintln!("\n--- Ring-compatible Ed25519 PEM key ---\n{}", pem_str);
        // Verify it works with from_pem
        let kp = KeyPair::from_pem(&pem_str);
        assert_eq!(kp.public_key_bytes.len(), 32);
    }

    #[test]
    fn test_from_pem_with_ring_key() {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_doc = RingEd25519::generate_pkcs8(&rng).unwrap();
        let pem_str = pem::encode(&pem::Pem::new("PRIVATE KEY", pkcs8_doc.as_ref().to_vec()));
        let kp = KeyPair::from_pem(&pem_str);
        assert_eq!(kp.public_key_bytes.len(), 32);
    }
}
