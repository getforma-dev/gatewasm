use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{DecodingKey, EncodingKey};
use ring::signature::{Ed25519KeyPair as RingEd25519, KeyPair as RingKeyPair};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use jsonwebtoken::{
    errors::{Error, ErrorKind},
    Algorithm, Header,
};
use serde::Serialize;

pub trait RemoteSigner: Send + Sync {
    fn sign<'a>(&'a self, message: &'a [u8]) -> BoxFuture<'a, Result<Vec<u8>, String>>;
}

#[derive(Clone)]
pub struct PublicKey {
    pub kid: String,
    pub bytes: Vec<u8>,
    pub retire_after: Option<i64>,
}
impl PublicKey {
    pub fn new(bytes: Vec<u8>, retire_after: Option<i64>) -> Self {
        Self {
            kid: KeyPair::compute_kid(&bytes),
            bytes,
            retire_after,
        }
    }
}

#[derive(Clone)]
enum Signer {
    Local(Arc<EncodingKey>),
    Remote(Arc<dyn RemoteSigner>),
}

#[derive(Clone)]
pub struct KeyPair {
    pub kid: String,
    pub public_key_bytes: Vec<u8>,
    signer: Signer,
    verification_keys: Arc<Vec<PublicKey>>,
    ssf: Option<(Signer, PublicKey)>,
    max_access_lifetime: u64,
    manifest_guard: Option<(sqlx::PgPool, i64)>,
}

impl KeyPair {
    pub fn remote(
        signer: Arc<dyn RemoteSigner>,
        keys: Vec<PublicKey>,
        ssf_signer: Arc<dyn RemoteSigner>,
        ssf_key: PublicKey,
        max_access_lifetime: u64,
    ) -> Result<Self, String> {
        let mut ids = std::collections::HashSet::new();
        if keys.len() < 2
            || keys.len() > 10
            || max_access_lifetime == 0
            || keys.iter().chain(std::iter::once(&ssf_key)).any(|key| {
                key.bytes.len() != 32
                    || key.kid != Self::compute_kid(&key.bytes)
                    || !ids.insert(&key.kid)
            })
        {
            return Err(
                "Remote keyring needs distinct valid current, next and dedicated SSF keys".into(),
            );
        }
        let active = &keys[0];
        Ok(Self {
            kid: active.kid.clone(),
            public_key_bytes: active.bytes.clone(),
            signer: Signer::Remote(signer),
            verification_keys: Arc::new(keys),
            ssf: Some((Signer::Remote(ssf_signer), ssf_key)),
            max_access_lifetime,
            manifest_guard: None,
        })
    }

    pub fn with_manifest_guard(mut self, pool: sqlx::PgPool, version: i64) -> Self {
        self.manifest_guard = Some((pool, version));
        self
    }

    pub async fn serving_current_generation(&self) -> bool {
        match &self.manifest_guard {
            None => true,
            Some((pool, version)) => {
                sqlx::query_scalar::<_, i64>("SELECT MAX(version) FROM signing_keyring_versions")
                    .fetch_one(pool)
                    .await
                    .is_ok_and(|current| current == *version)
            }
        }
    }

    pub fn public_keys(&self, now: i64) -> Vec<PublicKey> {
        let mut keys: Vec<_> = self
            .verification_keys
            .iter()
            .filter(|key| key.retire_after.is_none_or(|t| t > now))
            .cloned()
            .collect();
        if let Some((_, key)) = &self.ssf {
            keys.push(key.clone());
        }
        keys
    }

    pub fn verification_key(&self, kid: &str, now: i64) -> Result<DecodingKey, Error> {
        self.verification_keys
            .iter()
            .find(|key| key.kid == kid && key.retire_after.is_none_or(|t| t > now))
            .map(|key| DecodingKey::from_ed_der(&key.bytes))
            .ok_or_else(|| ErrorKind::InvalidToken.into())
    }

    pub async fn sign_access<T: Serialize>(&self, claims: &T) -> Result<String, Error> {
        let value =
            serde_json::to_value(claims).map_err(|_| Error::from(ErrorKind::InvalidToken))?;
        let iat = value
            .get("iat")
            .and_then(|v| v.as_u64())
            .ok_or(ErrorKind::InvalidToken)?;
        let exp = value
            .get("exp")
            .and_then(|v| v.as_u64())
            .ok_or(ErrorKind::InvalidToken)?;
        if exp <= iat || exp - iat > self.max_access_lifetime {
            return Err(ErrorKind::InvalidToken.into());
        }
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(self.kid.clone());
        Self::sign_with(&self.signer, &self.public_key_bytes, &header, claims).await
    }

    pub async fn sign_security_event<T: Serialize>(&self, claims: &T) -> Result<String, Error> {
        // SSF 1.0 section4.1.7 forbids exp. Its dedicated key is retained independently.
        if serde_json::to_value(claims)
            .map_err(|_| Error::from(ErrorKind::InvalidToken))?
            .get("exp")
            .is_some()
        {
            return Err(ErrorKind::InvalidToken.into());
        }
        let (signer, kid, bytes) = match &self.ssf {
            Some((signer, key)) => (signer, key.kid.as_str(), key.bytes.as_slice()),
            None => (
                &self.signer,
                self.kid.as_str(),
                self.public_key_bytes.as_slice(),
            ),
        };
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(kid.into());
        header.typ = Some("secevent+jwt".into());
        Self::sign_with(signer, bytes, &header, claims).await
    }

    async fn sign_with<T: Serialize>(
        signer: &Signer,
        public: &[u8],
        header: &Header,
        claims: &T,
    ) -> Result<String, Error> {
        match signer {
            Signer::Local(key) => jsonwebtoken::encode(header, claims, key),
            Signer::Remote(remote) => {
                let header = URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(header).map_err(|_| Error::from(ErrorKind::InvalidToken))?,
                );
                let payload = URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(claims).map_err(|_| Error::from(ErrorKind::InvalidToken))?,
                );
                let input = format!("{header}.{payload}");
                if input.len() > 4096 {
                    return Err(ErrorKind::InvalidToken.into());
                }
                let signature = remote.sign(input.as_bytes()).await.map_err(|_| {
                    tracing::error!("KMS signing refused; no local fallback");
                    Error::from(ErrorKind::InvalidToken)
                })?;
                if signature.len() != 64
                    || ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
                        .verify(input.as_bytes(), &signature)
                        .is_err()
                {
                    return Err(ErrorKind::InvalidSignature.into());
                }
                Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature)))
            }
        }
    }

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
    /// Legacy/development only: this exposes private key material to the process.
    /// Real-organization custody requires `FAUTH_REQUIRE_KMS=true` and a KMS manifest.
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

        let kid = Self::compute_kid(&public_key_bytes);

        Self {
            kid,
            signer: Signer::Local(Arc::new(encoding_key)),
            verification_keys: Arc::new(vec![PublicKey::new(public_key_bytes.clone(), None)]),
            public_key_bytes,
            ssf: None,
            max_access_lifetime: u64::MAX,
            manifest_guard: None,
        }
    }

    fn generate() -> Self {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_doc = RingEd25519::generate_pkcs8(&rng).expect("Failed to generate Ed25519 key");
        let pair =
            RingEd25519::from_pkcs8(pkcs8_doc.as_ref()).expect("Failed to parse generated key");
        let public_key_bytes = pair.public_key().as_ref().to_vec();

        let encoding_key = EncodingKey::from_ed_der(pkcs8_doc.as_ref());
        let kid = Self::compute_kid(&public_key_bytes);

        Self {
            kid,
            signer: Signer::Local(Arc::new(encoding_key)),
            verification_keys: Arc::new(vec![PublicKey::new(public_key_bytes.clone(), None)]),
            public_key_bytes,
            ssf: None,
            max_access_lifetime: u64::MAX,
            manifest_guard: None,
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
        // Synthetic key round-trip only; private material must never enter test logs.
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_doc = RingEd25519::generate_pkcs8(&rng).unwrap();
        let pem_str = pem::encode(&pem::Pem::new("PRIVATE KEY", pkcs8_doc.as_ref().to_vec()));

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
    struct FakeRemote {
        pair: RingEd25519,
        mode: std::sync::atomic::AtomicUsize,
        calls: std::sync::atomic::AtomicUsize,
    }
    impl FakeRemote {
        fn new() -> Arc<Self> {
            let doc = RingEd25519::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
            Arc::new(Self {
                pair: RingEd25519::from_pkcs8(doc.as_ref()).unwrap(),
                mode: 0.into(),
                calls: 0.into(),
            })
        }
        fn public(&self, retire_after: Option<i64>) -> PublicKey {
            PublicKey::new(self.pair.public_key().as_ref().to_vec(), retire_after)
        }
    }
    impl RemoteSigner for FakeRemote {
        fn sign<'a>(&'a self, message: &'a [u8]) -> BoxFuture<'a, Result<Vec<u8>, String>> {
            Box::pin(async move {
                use std::sync::atomic::Ordering::SeqCst;
                self.calls.fetch_add(1, SeqCst);
                match self.mode.load(SeqCst) {
                    1 => Err("injected unavailable or denied".into()),
                    2 => Ok(vec![0; 64]),
                    3 => Ok(vec![0; 65]),
                    _ => Ok(self.pair.sign(message).as_ref().to_vec()),
                }
            })
        }
    }
    #[tokio::test]
    async fn remote_signing_fail_closed_limits_and_recovery() {
        use std::sync::atomic::Ordering::SeqCst;
        let access = FakeRemote::new();
        let next = FakeRemote::new();
        let ssf = FakeRemote::new();
        let keys = KeyPair::remote(
            access.clone(),
            vec![access.public(None), next.public(None)],
            ssf.clone(),
            ssf.public(None),
            900,
        )
        .unwrap();
        let now = chrono::Utc::now().timestamp();
        let claims = serde_json::json!({"iat":now,"exp":now+900,"sub":"synthetic"});
        let token = keys.sign_access(&claims).await.unwrap();
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.alg, Algorithm::EdDSA);
        assert_eq!(header.typ.as_deref(), Some("JWT"));
        assert_eq!(header.kid, Some(keys.kid.clone()));
        let mut validation = jsonwebtoken::Validation::new(Algorithm::EdDSA);
        validation.validate_aud = false;
        jsonwebtoken::decode::<serde_json::Value>(
            &token,
            &keys.verification_key(&keys.kid, now).unwrap(),
            &validation,
        )
        .unwrap();
        for mode in [1, 2, 3] {
            access.mode.store(mode, SeqCst);
            assert!(keys.sign_access(&claims).await.is_err());
        }
        access.mode.store(0, SeqCst);
        assert!(keys.sign_access(&claims).await.is_ok());
        let before = access.calls.load(SeqCst);
        assert!(keys
            .sign_access(&serde_json::json!({"iat":now,"exp":now+901}))
            .await
            .is_err());
        assert!(keys
            .sign_access(&serde_json::json!({"iat":now,"exp":now+900,"oversized":"x".repeat(4096)}))
            .await
            .is_err());
        assert_eq!(
            access.calls.load(SeqCst),
            before,
            "invalid or oversized requests must not reach KMS"
        );
        assert!(keys.verification_key("unknown-kid", now).is_err());
    }
    #[tokio::test]
    async fn rotation_overlap_and_ssf_history_are_independent() {
        let old = FakeRemote::new();
        let active = FakeRemote::new();
        let next = FakeRemote::new();
        let ssf = FakeRemote::new();
        let now = chrono::Utc::now().timestamp();
        let retire = now + 960;
        let keys = KeyPair::remote(
            active.clone(),
            vec![
                active.public(None),
                next.public(None),
                old.public(Some(retire)),
            ],
            ssf.clone(),
            ssf.public(None),
            900,
        )
        .unwrap();
        let old_id = old.public(None).kid;
        let ssf_id = ssf.public(None).kid;
        assert!(keys.verification_key(&old_id, retire - 1).is_ok());
        assert!(keys.verification_key(&old_id, retire).is_err());
        assert!(keys.public_keys(retire - 1).iter().any(|k| k.kid == old_id));
        assert!(!keys.public_keys(retire).iter().any(|k| k.kid == old_id));
        assert!(keys
            .public_keys(retire + 1000000)
            .iter()
            .any(|k| k.kid == ssf_id));
        assert!(
            keys.verification_key(&ssf_id, now).is_err(),
            "SET signing key cannot verify access JWTs"
        );
        let claims = serde_json::json!({"iat":now,"jti":"historical-event","events":{"https://schemas.openid.net/secevent/ssf/event-type/verification":{}}});
        let token = keys.sign_security_event(&claims).await.unwrap();
        let parts: Vec<_> = token.split('.').collect();
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.typ.as_deref(), Some("secevent+jwt"));
        assert_eq!(header.kid, Some(ssf_id));
        let payload: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert!(payload.get("exp").is_none());
        ring::signature::UnparsedPublicKey::new(
            &ring::signature::ED25519,
            ssf.pair.public_key().as_ref(),
        )
        .verify(
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &URL_SAFE_NO_PAD.decode(parts[2]).unwrap(),
        )
        .unwrap();
        assert!(keys
            .sign_security_event(&serde_json::json!({"iat":now,"exp":now+900}))
            .await
            .is_err());
    }
}
