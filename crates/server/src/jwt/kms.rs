//! Non-exportable Ed25519 signing. The manifest is public, versioned rollout state.
//! AWS_KMS private material is never requested, loaded, or used as a fallback.
use super::keys::{KeyPair, PublicKey, RemoteSigner};
use aws_sdk_kms::{
    primitives::Blob,
    types::{KeyManagerType, KeySpec, KeyUsageType, MessageType, OriginType, SigningAlgorithmSpec},
};
use chrono::Utc;
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{collections::HashSet, sync::Arc, time::Duration};

pub fn parse_required(value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err("FAUTH_REQUIRE_KMS must be exactly true or false".into()),
    }
}

/// Select signing custody once, before production config guards or database work.
/// A supplied manifest always selects KMS; invalid input never selects a local key.
pub enum SigningMode {
    Local,
    Kms(String),
}

impl SigningMode {
    pub fn from_env() -> Result<Self, String> {
        Self::select(
            std::env::var("FAUTH_REQUIRE_KMS"),
            std::env::var("FAUTH_KMS_KEYRING_JSON"),
        )
    }

    fn select(
        required: Result<String, std::env::VarError>,
        manifest: Result<String, std::env::VarError>,
    ) -> Result<Self, String> {
        let required = match required {
            Ok(value) => parse_required(Some(&value))?,
            Err(std::env::VarError::NotPresent) => false,
            Err(_) => return Err("Invalid FAUTH_REQUIRE_KMS encoding".into()),
        };
        match manifest {
            Ok(json) => {
                serde_json::from_str::<Manifest>(&json)
                    .map_err(|_| "Invalid FAUTH_KMS_KEYRING_JSON")?;
                Ok(Self::Kms(json))
            }
            Err(std::env::VarError::NotPresent) if !required => Ok(Self::Local),
            Err(_) => Err("Required KMS keyring configuration is missing or invalid".into()),
        }
    }

    pub fn validate_private_material(
        &self,
        production: bool,
        access_pem_present: bool,
        ssf_pem_present: bool,
    ) -> Result<(), String> {
        match self {
            Self::Kms(_) if access_pem_present || ssf_pem_present => {
                Err("KMS mode refuses process-readable signing key configuration".into())
            }
            Self::Local if production && !access_pem_present => {
                Err("ED25519_PRIVATE_KEY must be set in production when KMS is not selected".into())
            }
            _ => Ok(()),
        }
    }
}

pub const JWKS_CACHE_SECONDS: i64 = 3600;
pub const CLOCK_SKEW_SECONDS: i64 = 60;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreviousKey {
    pub key_id: String,
    // None keeps a rolling deployment's old key published indefinitely.
    pub last_issued_at: Option<i64>,
    pub retire_after: Option<i64>,
    pub drain_receipt: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicationReceipt {
    pub next_key_id: String,
    pub all_replicas_serving_since: i64,
    pub evidence: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: i64,
    pub max_access_lifetime: u64,
    pub publication: Option<PublicationReceipt>,
    pub active: String,
    pub next: String,
    pub ssf: String,
    #[serde(default)]
    pub previous: Vec<PreviousKey>,
}

impl Manifest {
    pub fn validate(&self, max_access_lifetime: u64, now: i64) -> Result<(), String> {
        if self.version < 1 || self.previous.len() > 8 {
            return Err("Invalid keyring version or previous-key bound".into());
        }
        if max_access_lifetime == 0 || max_access_lifetime > self.max_access_lifetime {
            return Err("Configured access TTL exceeds the persisted generation cap".into());
        }
        let lifetime =
            i64::try_from(self.max_access_lifetime).map_err(|_| "Token lifetime overflow")?;
        if let Some(receipt) = &self.publication {
            if receipt.next_key_id != self.next
                || receipt.all_replicas_serving_since < 0
                || receipt.all_replicas_serving_since > now
                || receipt.evidence.trim().is_empty()
                || receipt.evidence.len() > 2048
            {
                return Err("Invalid next-key publication receipt".into());
            }
        }
        let mut ids = HashSet::new();
        for id in std::iter::once(&self.active)
            .chain(std::iter::once(&self.next))
            .chain(std::iter::once(&self.ssf))
            .chain(self.previous.iter().map(|p| &p.key_id))
        {
            if !id.starts_with("arn:aws:kms:") || !id.contains(":key/") || !ids.insert(id) {
                return Err("Keyring requires distinct immutable KMS key ARNs".into());
            }
        }
        for key in &self.previous {
            match (key.last_issued_at, key.retire_after, key.drain_receipt.as_deref()) {
                (None, None, None) => {},
                (Some(last), Some(retire), Some(receipt)) if !receipt.trim().is_empty() && last <= now && last >= 0
                    && retire >= last.checked_add(lifetime).and_then(|n| n.checked_add(CLOCK_SKEW_SECONDS)).ok_or("Retirement overflow")? => {},
                _ => return Err("Previous key needs a drain receipt and full signed-token lifetime plus skew, or indefinite retention".into()),
            }
        }
        Ok(())
    }
}

fn validate_transition(
    old: &Manifest,
    new: &Manifest,
    published_since: i64,
    now: i64,
) -> Result<(), String> {
    if old.max_access_lifetime != new.max_access_lifetime {
        return Err(
            "The generation lifetime cap cannot change during this keyring lifecycle".into(),
        );
    }
    if new.ssf != old.ssf {
        return Err("Dedicated SSF key lifetime needs a separate reviewed migration".into());
    }
    if new.version != old.version + 1 {
        return Err("Keyring versions must advance by one".into());
    }
    if new.active != old.active {
        let published = old
            .publication
            .as_ref()
            .ok_or("Promotion needs a witnessed fleet publication receipt")?;
        if new.active != old.next
            || now < published.all_replicas_serving_since + JWKS_CACHE_SECONDS
            || new.publication.is_some()
        {
            return Err(
                "Promotion requires the prior next key and a full JWKS cache publication interval"
                    .into(),
            );
        }
        let retiring = new
            .previous
            .iter()
            .find(|p| p.key_id == old.active)
            .ok_or("Promotion must retain the old active key")?;
        if retiring.retire_after.is_some() {
            return Err(
                "First promotion must retain old key indefinitely until drain is witnessed".into(),
            );
        }
    }
    if new.active == old.active {
        if new.next != old.next && new.publication.is_some() {
            return Err("A replacement next key needs a new witnessed publication window".into());
        }
        if let Some(receipt) = &new.publication {
            if old
                .publication
                .as_ref()
                .is_some_and(|old_receipt| old_receipt != receipt)
                || (old.publication.is_none()
                    && receipt.all_replicas_serving_since < published_since)
            {
                return Err(
                    "Publication receipt cannot predate its deployed generation or be rewritten"
                        .into(),
                );
            }
        }
    }
    for key in &new.previous {
        let retained = old
            .previous
            .iter()
            .any(|old_key| old_key.key_id == key.key_id);
        let demoted = new.active != old.active && key.key_id == old.active;
        if !(retained || demoted) {
            return Err(
                "Previous keys must be retained history or the active key being promoted".into(),
            );
        }
    }
    for previous in &old.previous {
        let retained = new.previous.iter().find(|p| p.key_id == previous.key_id);
        if previous.retire_after.is_some_and(|t| t <= now)
            && retained.is_some_and(|key| key != previous)
        {
            return Err("Expired keys cannot be republished or have retirement rewritten".into());
        }
        match retained {
            None if previous.retire_after.is_some_and(|t| t <= now) => {}
            Some(next)
                if previous.retire_after.is_none()
                    || next.retire_after.is_none()
                    || next.retire_after >= previous.retire_after => {}
            _ => {
                return Err("A previous key cannot be removed early or shorten its overlap".into())
            }
        }
    }
    Ok(())
}

pub async fn record_manifest(
    pool: &PgPool,
    manifest: &Manifest,
    max_access_lifetime: u64,
) -> Result<(), String> {
    let now = Utc::now().timestamp();
    manifest.validate(max_access_lifetime, now)?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "Cannot open keyring version transaction")?;
    sqlx::query("SELECT pg_advisory_xact_lock(248)")
        .execute(&mut *tx)
        .await
        .map_err(|_| "Cannot lock keyring versions")?;
    let latest: Option<(i64, serde_json::Value, chrono::DateTime<Utc>)> = sqlx::query_as("SELECT version,manifest,activated_at FROM signing_keyring_versions ORDER BY version DESC LIMIT 1")
        .fetch_optional(&mut *tx).await.map_err(|_| "Cannot read keyring version")?;
    if let Some((version, value, activated)) = latest {
        let old: Manifest =
            serde_json::from_value(value).map_err(|_| "Stored keyring manifest is malformed")?;
        if version == manifest.version && old == *manifest {
            return Ok(());
        }
        if manifest.next != old.next {
            let history: Vec<serde_json::Value> =
                sqlx::query_scalar("SELECT manifest FROM signing_keyring_versions")
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|_| "Cannot read keyring history")?;
            for value in history {
                let historic: Manifest = serde_json::from_value(value)
                    .map_err(|_| "Stored keyring history is malformed")?;
                if [&historic.active, &historic.next, &historic.ssf].contains(&&manifest.next)
                    || historic
                        .previous
                        .iter()
                        .any(|key| key.key_id == manifest.next)
                {
                    return Err("Replacement next key must be fresh; keyring history forbids retired-key reuse".into());
                }
            }
        }
        // DB activation is only a lower bound. Promotion also needs an explicit
        // receipt witnessing next-key publication across every serving replica.
        validate_transition(&old, manifest, activated.timestamp(), now)?;
        for key in &manifest.previous {
            if let Some(last) = key.last_issued_at {
                let previously_indefinite = old
                    .previous
                    .iter()
                    .any(|p| p.key_id == key.key_id && p.last_issued_at.is_none());
                if previously_indefinite && last < activated.timestamp() {
                    return Err("Drain timestamp predates promotion".into());
                }
            }
        }
    } else if manifest.version != 1
        || !manifest.previous.is_empty()
        || manifest.publication.is_some()
    {
        return Err("Initial KMS manifest must be version one without previous keys".into());
    }
    sqlx::query("INSERT INTO signing_keyring_versions (version,manifest) VALUES ($1,$2)")
        .bind(manifest.version)
        .bind(serde_json::to_value(manifest).map_err(|_| "Cannot serialize keyring")?)
        .execute(&mut *tx)
        .await
        .map_err(|_| "Cannot persist keyring version")?;
    tx.commit()
        .await
        .map_err(|_| "Cannot commit keyring version".into())
}

struct KmsSigner {
    client: aws_sdk_kms::Client,
    key_id: String,
    pool: PgPool,
    version: i64,
}
impl RemoteSigner for KmsSigner {
    fn sign<'a>(&'a self, message: &'a [u8]) -> BoxFuture<'a, Result<Vec<u8>, String>> {
        Box::pin(async move {
            if message.len() > 4096 {
                return Err("KMS Ed25519 RAW input exceeds 4096 bytes".into());
            }
            let current: i64 =
                sqlx::query_scalar("SELECT MAX(version) FROM signing_keyring_versions")
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|_| "Cannot verify active signer version")?;
            if current != self.version {
                return Err("Signer belongs to a superseded deployment".into());
            }
            let output = self
                .client
                .sign()
                .key_id(&self.key_id)
                .message(Blob::new(message))
                .message_type(MessageType::Raw)
                .signing_algorithm(SigningAlgorithmSpec::Ed25519Sha512)
                .send()
                .await
                .map_err(|_| "KMS signing denied or unavailable")?;
            if output.key_id.as_deref() != Some(self.key_id.as_str())
                || output.signing_algorithm != Some(SigningAlgorithmSpec::Ed25519Sha512)
            {
                return Err("KMS signing response identity or algorithm differs".into());
            }
            Ok(output
                .signature
                .ok_or("KMS response lacks signature")?
                .into_inner())
        })
    }
}

fn ed25519_spki(bytes: &[u8]) -> Result<Vec<u8>, String> {
    const PREFIX: &[u8] = &[
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    if bytes.len() != 44 || !bytes.starts_with(PREFIX) {
        return Err("KMS public key is not canonical Ed25519 SPKI".into());
    }
    Ok(bytes[12..].to_vec())
}

pub async fn load(json: &str, pool: &PgPool, max_access_lifetime: u64) -> Result<KeyPair, String> {
    let manifest: Manifest =
        serde_json::from_str(json).map_err(|_| "Invalid FAUTH_KMS_KEYRING_JSON")?;
    manifest.validate(max_access_lifetime, Utc::now().timestamp())?;
    let sdk = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .timeout_config(
            aws_config::timeout::TimeoutConfig::builder()
                .operation_timeout(Duration::from_secs(15))
                .build(),
        )
        .load()
        .await;
    let client = aws_sdk_kms::Client::new(&sdk);
    let mut public_keys = Vec::new();
    let mut ssf_public_key = None;
    let mut kids = HashSet::new();
    for (id, retire_after) in std::iter::once((&manifest.active, None))
        .chain(std::iter::once((&manifest.next, None)))
        .chain(std::iter::once((&manifest.ssf, None)))
        .chain(
            manifest
                .previous
                .iter()
                .filter(|p| p.retire_after.is_none_or(|t| t > Utc::now().timestamp()))
                .map(|p| (&p.key_id, p.retire_after)),
        )
    {
        let description = client
            .describe_key()
            .key_id(id)
            .send()
            .await
            .map_err(|_| "KMS DescribeKey denied or unavailable")?;
        let metadata = description.key_metadata.ok_or("Missing KMS metadata")?;
        if metadata.arn.as_deref() != Some(id)
            || metadata.origin != Some(OriginType::AwsKms)
            || metadata.key_manager != Some(KeyManagerType::Customer)
            || !metadata.enabled
            || metadata.key_usage != Some(KeyUsageType::SignVerify)
            || metadata.key_spec != Some(KeySpec::EccNistEdwards25519)
        {
            return Err(
                "KMS key is not an enabled AWS-generated customer Ed25519 signing key".into(),
            );
        }
        let result = client
            .get_public_key()
            .key_id(id)
            .send()
            .await
            .map_err(|_| "KMS GetPublicKey denied or unavailable")?;
        if result.key_id.as_deref() != Some(id)
            || result.key_spec != Some(KeySpec::EccNistEdwards25519)
            || result.key_usage != Some(KeyUsageType::SignVerify)
            || !result
                .signing_algorithms()
                .contains(&SigningAlgorithmSpec::Ed25519Sha512)
        {
            return Err("KMS public-key contract differs".into());
        }
        let bytes = ed25519_spki(result.public_key.ok_or("Missing public key")?.as_ref())?;
        let key = PublicKey::new(bytes, retire_after);
        if !kids.insert(key.kid.clone()) {
            return Err("Duplicate public key IDs in KMS keyring".into());
        }
        if id == &manifest.ssf {
            ssf_public_key = Some(key);
        } else {
            public_keys.push(key);
        }
    }
    record_manifest(pool, &manifest, max_access_lifetime).await?;
    let signer = Arc::new(KmsSigner {
        client: client.clone(),
        key_id: manifest.active,
        pool: pool.clone(),
        version: manifest.version,
    });
    let ssf_signer = Arc::new(KmsSigner {
        client,
        key_id: manifest.ssf,
        pool: pool.clone(),
        version: manifest.version,
    });
    Ok(KeyPair::remote(
        signer,
        public_keys,
        ssf_signer,
        ssf_public_key.ok_or("Missing SSF public key")?,
        manifest.max_access_lifetime,
    )?
    .with_manifest_guard(pool.clone(), manifest.version))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arn(id: &str) -> String {
        format!("arn:aws:kms:us-east-1:123456789012:key/{id}")
    }
    fn initial() -> Manifest {
        Manifest {
            version: 1,
            max_access_lifetime: 900,
            publication: None,
            active: arn("a"),
            next: arn("b"),
            ssf: arn("s"),
            previous: vec![],
        }
    }
    #[test]
    fn manifest_refuses_duplicates_short_overlap_and_partial_retirement() {
        let mut m = initial();
        m.next = m.active.clone();
        assert!(m.validate(900, 1000).is_err());
        let mut m = initial();
        m.previous.push(PreviousKey {
            key_id: arn("c"),
            last_issued_at: Some(1),
            retire_after: Some(901),
            drain_receipt: Some("drained".into()),
        });
        assert!(m.validate(900, 1000).is_err());
        m.previous[0].retire_after = Some(1 + 900 + CLOCK_SKEW_SECONDS);
        assert!(m.validate(900, 1000).is_ok());
        m.previous[0].drain_receipt = None;
        assert!(m.validate(900, 1000).is_err());
    }
    #[test]
    fn promotion_requires_prepublished_next_and_old_key_overlap() {
        let mut old = initial();
        old.publication = Some(PublicationReceipt {
            next_key_id: arn("b"),
            all_replicas_serving_since: 1000,
            evidence: "fleet-jwks-receipt".into(),
        });
        let mut new = Manifest {
            version: 2,
            max_access_lifetime: 900,
            publication: None,
            active: arn("b"),
            next: arn("c"),
            ssf: arn("s"),
            previous: vec![PreviousKey {
                key_id: arn("a"),
                last_issued_at: None,
                retire_after: None,
                drain_receipt: None,
            }],
        };
        assert!(validate_transition(&old, &new, 1000, 4599).is_err());
        assert!(validate_transition(&old, &new, 1000, 4600).is_ok());
        new.previous.clear();
        assert!(validate_transition(&old, &new, 1000, 4600).is_err());
        new.active = arn("d");
        assert!(validate_transition(&old, &new, 1000, 4600).is_err());
    }
    #[test]
    fn spki_requires_exact_algorithm_and_shape() {
        let mut bytes = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        bytes.extend_from_slice(&[1; 32]);
        assert_eq!(ed25519_spki(&bytes).unwrap(), vec![1; 32]);
        bytes[8] = 0x71;
        assert!(ed25519_spki(&bytes).is_err());
    }
    #[test]
    fn custody_config_must_be_explicit_and_exact() {
        assert_eq!(parse_required(None), Ok(false));
        assert_eq!(parse_required(Some("true")), Ok(true));
        assert_eq!(parse_required(Some("false")), Ok(false));
        for invalid in ["True", "treu", "", "0", " true"] {
            assert!(parse_required(Some(invalid)).is_err());
        }
    }
    #[test]
    fn custody_selection_and_public_host_guards_share_one_mode() {
        use std::env::VarError::NotPresent;
        let manifest = serde_json::to_string(&initial()).unwrap();
        for required in [Err(NotPresent), Ok("false".into()), Ok("true".into())] {
            let mode = SigningMode::select(required, Ok(manifest.clone())).unwrap();
            assert!(matches!(mode, SigningMode::Kms(_)));
            assert!(mode.validate_private_material(true, false, false).is_ok());
            assert!(mode.validate_private_material(true, true, false).is_err());
            assert!(mode.validate_private_material(true, false, true).is_err());
        }
        for required in [Err(NotPresent), Ok("false".into())] {
            let mode = SigningMode::select(required, Err(NotPresent)).unwrap();
            assert!(matches!(mode, SigningMode::Local));
            assert!(mode.validate_private_material(true, false, false).is_err());
            assert!(mode.validate_private_material(true, true, false).is_ok());
            assert!(mode.validate_private_material(false, false, false).is_ok());
        }
        assert!(SigningMode::select(Ok("true".into()), Err(NotPresent)).is_err());
        for invalid in ["True", "false ", "", "treu"] {
            assert!(SigningMode::select(Ok(invalid.into()), Ok(manifest.clone())).is_err());
        }
        for invalid in ["", "{}", "null", "not-json"] {
            assert!(SigningMode::select(Ok("false".into()), Ok(invalid.into())).is_err());
        }
        let bad_encoding = || std::env::VarError::NotUnicode(std::ffi::OsString::from("invalid"));
        assert!(SigningMode::select(Err(bad_encoding()), Ok(manifest)).is_err());
        assert!(SigningMode::select(Ok("false".into()), Err(bad_encoding())).is_err());
    }
    #[test]
    fn lifetime_cap_survives_config_decrease_and_cannot_shrink() {
        let mut old = initial();
        old.max_access_lifetime = 3600;
        assert!(old.validate(900, 1000).is_ok());
        assert!(old.validate(3601, 1000).is_err());
        let mut new = old.clone();
        new.version = 2;
        new.max_access_lifetime = 900;
        assert!(validate_transition(&old, &new, 900, 1000).is_err());
        old.previous.push(PreviousKey {
            key_id: arn("c"),
            last_issued_at: Some(1),
            retire_after: Some(961),
            drain_receipt: Some("drained".into()),
        });
        assert!(
            old.validate(900, 1000).is_err(),
            "a smaller runtime TTL must not shrink historical overlap"
        );
    }
    #[test]
    fn publication_receipt_must_follow_fleet_deployment_and_survive_later_versions() {
        let old = initial();
        let mut new = old.clone();
        new.version = 2;
        new.publication = Some(PublicationReceipt {
            next_key_id: new.next.clone(),
            all_replicas_serving_since: 999,
            evidence: "public-jwks-fleet-proof".into(),
        });
        assert!(validate_transition(&old, &new, 1000, 2000).is_err());
        new.publication.as_mut().unwrap().all_replicas_serving_since = 2001;
        assert!(new.validate(900, 2000).is_err());
        new.publication.as_mut().unwrap().all_replicas_serving_since = 1500;
        assert!(new.validate(900, 2000).is_ok());
        assert!(validate_transition(&old, &new, 1000, 2000).is_ok());
        let mut unchanged = new.clone();
        unchanged.version = 3;
        assert!(validate_transition(&new, &unchanged, 2000, 3000).is_ok());
        unchanged
            .publication
            .as_mut()
            .unwrap()
            .all_replicas_serving_since = 2500;
        assert!(validate_transition(&new, &unchanged, 2000, 3000).is_err());
        let mut promoted = old.clone();
        promoted.version = 2;
        promoted.active = old.next.clone();
        promoted.next = arn("c");
        promoted.previous.push(PreviousKey {
            key_id: old.active.clone(),
            last_issued_at: None,
            retire_after: None,
            drain_receipt: None,
        });
        assert!(
            validate_transition(&old, &promoted, 0, 9999).is_err(),
            "DB age alone never proves fleet publication"
        );
    }

    #[test]
    fn expired_previous_keys_cannot_be_resurrected_or_added_arbitrarily() {
        let mut old = initial();
        old.previous.push(PreviousKey {
            key_id: arn("c"),
            last_issued_at: Some(0),
            retire_after: Some(960),
            drain_receipt: Some("drained".into()),
        });
        let mut new = old.clone();
        new.version = 2;
        new.previous[0].last_issued_at = None;
        new.previous[0].retire_after = None;
        new.previous[0].drain_receipt = None;
        assert!(validate_transition(&old, &new, 1000, 2000).is_err());
        new.previous.clear();
        assert!(validate_transition(&old, &new, 1000, 2000).is_ok());
        let mut readded = new.clone();
        readded.version = 3;
        readded.previous = old.previous.clone();
        assert!(validate_transition(&new, &readded, 1000, 2000).is_err());
    }
}
