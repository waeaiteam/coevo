//! Real Ed25519 cryptography for the coevo control-plane.
//!
//! This module replaces the prior placeholder string-prefix "signatures" with
//! genuine Ed25519 signing/verification (RFC 8032, via `ed25519-dalek` v2).
//!
//! # Platform signing key
//!
//! The platform holds a single long-lived Ed25519 signing key used to sign
//! provenance envelopes, track outputs, and emergency-lease attestations. The
//! key is a raw 32-byte seed persisted at:
//!
//! ```text
//! $COEVO_SIGNING_KEY_PATH                       (if set)
//! else $COEVO_HOME/keys/platform_signing.key    (if COEVO_HOME set)
//! else <platform home>/.coevo/keys/platform_signing.key
//! ```
//!
//! On first use the key is generated from the OS CSPRNG and written with the
//! 32 raw seed bytes (no encoding). It is cached process-wide in a [`OnceLock`]
//! so repeated calls do not re-read the file.
//!
//! # Canonical bytes
//!
//! Structured payloads are signed over a deterministic byte encoding produced
//! by [`canonical_bytes`]: `serde_json` serialization with **lexicographically
//! sorted object keys** at every level, no insignificant whitespace, UTF-8.
//! Two values that are equal as JSON therefore always produce identical signing
//! bytes regardless of field declaration order.

use std::path::PathBuf;
use std::sync::OnceLock;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// Length of a raw Ed25519 seed / secret key, in bytes.
pub const SEED_LEN: usize = 32;
/// Length of an Ed25519 public key, in bytes.
pub const PUBLIC_KEY_LEN: usize = 32;
/// Length of an Ed25519 signature, in bytes.
pub const SIGNATURE_LEN: usize = 64;

/// Errors that can arise while loading keys or verifying signatures.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("failed to access signing key file at {path}: {source}")]
    KeyIo {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("signing key at {path} has invalid length: expected {SEED_LEN} bytes, found {found}")]
    BadKeyLength { path: String, found: usize },
    #[error("could not determine a home directory for the platform signing key")]
    NoHomeDir,
    #[error("signature is not valid hex: {0}")]
    SignatureNotHex(String),
    #[error("signature has invalid length: expected {SIGNATURE_LEN} bytes, found {found}")]
    BadSignatureLength { found: usize },
    #[error("public key is not valid hex: {0}")]
    PublicKeyNotHex(String),
    #[error("public key has invalid length: expected {PUBLIC_KEY_LEN} bytes, found {found}")]
    BadPublicKeyLength { found: usize },
    #[error("public key bytes are not a valid Ed25519 point")]
    InvalidPublicKey,
}

/// Process-wide cache of the loaded platform signing key.
static PLATFORM_KEY: OnceLock<SigningKey> = OnceLock::new();

/// Resolve the on-disk path of the platform signing key.
///
/// Precedence: `COEVO_SIGNING_KEY_PATH` > `$COEVO_HOME/keys/platform_signing.key`
/// > `<home>/.coevo/keys/platform_signing.key`.
pub fn platform_signing_key_path() -> Result<PathBuf, CryptoError> {
    if let Ok(explicit) = std::env::var("COEVO_SIGNING_KEY_PATH") {
        if !explicit.is_empty() {
            return Ok(PathBuf::from(explicit));
        }
    }
    let home = if let Ok(coevo_home) = std::env::var("COEVO_HOME") {
        if coevo_home.is_empty() {
            default_home()?
        } else {
            PathBuf::from(coevo_home)
        }
    } else {
        default_home()?
    };
    Ok(home.join("keys").join("platform_signing.key"))
}

/// The platform's default home directory (`%USERPROFILE%\.coevo` /
/// `$HOME/.coevo`) when `COEVO_HOME` is unset.
fn default_home() -> Result<PathBuf, CryptoError> {
    // USERPROFILE on Windows, HOME elsewhere.
    let base = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or(CryptoError::NoHomeDir)?;
    Ok(PathBuf::from(base).join(".coevo"))
}

/// Load the platform signing key from disk, generating and persisting a fresh
/// one if the file does not yet exist. The result is cached for the lifetime of
/// the process.
pub fn platform_signing_key() -> &'static SigningKey {
    PLATFORM_KEY.get_or_init(|| {
        load_or_create_signing_key().unwrap_or_else(|e| {
            panic!("fatal: could not initialize platform signing key: {e}");
        })
    })
}

/// Load (or create) the platform signing key without populating the global
/// cache. Prefer [`platform_signing_key`] in normal use; this is exposed for
/// callers that need explicit error handling.
pub fn load_or_create_signing_key() -> Result<SigningKey, CryptoError> {
    let path = platform_signing_key_path()?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|source| CryptoError::KeyIo {
                path: parent.display().to_string(),
                source,
            })?;
        }
    }

    match std::fs::read(&path) {
        Ok(bytes) => {
            if bytes.len() != SEED_LEN {
                return Err(CryptoError::BadKeyLength {
                    path: path.display().to_string(),
                    found: bytes.len(),
                });
            }
            let mut seed = [0u8; SEED_LEN];
            seed.copy_from_slice(&bytes);
            Ok(SigningKey::from_bytes(&seed))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let key = generate_signing_key();
            persist_seed(&path, &key.to_bytes())?;
            Ok(key)
        }
        Err(source) => Err(CryptoError::KeyIo {
            path: path.display().to_string(),
            source,
        }),
    }
}

/// Generate a fresh Ed25519 signing key from the OS CSPRNG.
///
/// We fill a 32-byte seed via [`rand::RngCore`] and construct the key with
/// `SigningKey::from_bytes`, which avoids requiring ed25519-dalek's optional
/// `rand_core` feature while still using a cryptographically secure source.
pub fn generate_signing_key() -> SigningKey {
    use rand::RngCore;
    let mut seed = [0u8; SEED_LEN];
    rand::rngs::OsRng.fill_bytes(&mut seed);
    SigningKey::from_bytes(&seed)
}

/// Write the raw 32-byte seed to disk, restricting permissions where supported.
fn persist_seed(path: &PathBuf, seed: &[u8; SEED_LEN]) -> Result<(), CryptoError> {
    std::fs::write(path, seed).map_err(|source| CryptoError::KeyIo {
        path: path.display().to_string(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Owner read/write only.
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Sign `payload` with the platform signing key, returning a lowercase hex
/// encoding of the 64-byte signature.
pub fn sign(payload: &[u8]) -> String {
    let sig = platform_signing_key().sign(payload);
    hex::encode(sig.to_bytes())
}

/// The platform's public (verifying) key as lowercase hex.
pub fn platform_public_key_hex() -> String {
    hex::encode(platform_signing_key().verifying_key().to_bytes())
}

/// The platform's [`VerifyingKey`].
pub fn platform_verifying_key() -> VerifyingKey {
    platform_signing_key().verifying_key()
}

/// Verify a hex-encoded `signature` over `payload` against a hex-encoded
/// Ed25519 `public_key`. Returns `false` on any decoding/length/verification
/// failure (fail-closed); use [`verify_detailed`] when the reason matters.
pub fn verify(public_key: &str, payload: &[u8], signature: &str) -> bool {
    verify_detailed(public_key, payload, signature).is_ok()
}

/// Like [`verify`] but surfaces the precise failure reason.
pub fn verify_detailed(
    public_key: &str,
    payload: &[u8],
    signature: &str,
) -> Result<(), CryptoError> {
    let vk = parse_public_key(public_key)?;
    let sig = parse_signature(signature)?;
    vk.verify(payload, &sig)
        .map_err(|_| CryptoError::InvalidPublicKey)
}

/// Parse a lowercase/uppercase hex Ed25519 public key.
pub fn parse_public_key(public_key: &str) -> Result<VerifyingKey, CryptoError> {
    let raw = hex::decode(public_key).map_err(|e| CryptoError::PublicKeyNotHex(e.to_string()))?;
    if raw.len() != PUBLIC_KEY_LEN {
        return Err(CryptoError::BadPublicKeyLength { found: raw.len() });
    }
    let mut bytes = [0u8; PUBLIC_KEY_LEN];
    bytes.copy_from_slice(&raw);
    VerifyingKey::from_bytes(&bytes).map_err(|_| CryptoError::InvalidPublicKey)
}

/// Parse a hex Ed25519 signature.
fn parse_signature(signature: &str) -> Result<Signature, CryptoError> {
    let raw = hex::decode(signature).map_err(|e| CryptoError::SignatureNotHex(e.to_string()))?;
    if raw.len() != SIGNATURE_LEN {
        return Err(CryptoError::BadSignatureLength { found: raw.len() });
    }
    let mut bytes = [0u8; SIGNATURE_LEN];
    bytes.copy_from_slice(&raw);
    Ok(Signature::from_bytes(&bytes))
}

/// Deterministic canonical bytes for a structured JSON payload.
///
/// Object keys are sorted lexicographically at every level so that logically
/// equal values always serialize identically. The encoding is compact
/// (no insignificant whitespace) UTF-8 JSON.
pub fn canonical_bytes(value: &serde_json::Value) -> Vec<u8> {
    let canonical = canonicalize(value);
    // `serde_json::to_vec` on a Value with sorted maps is already deterministic;
    // sorting is enforced by `canonicalize` returning an ordered representation.
    serde_json::to_vec(&canonical).expect("canonical JSON serialization cannot fail")
}

/// Recursively rebuild a JSON value with object keys sorted.
fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            // BTreeMap iterates keys in sorted order; serde_json::Map preserves
            // insertion order, so collect through a BTreeMap to enforce sorting.
            let mut sorted = std::collections::BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), canonicalize(v));
            }
            let mut out = serde_json::Map::with_capacity(sorted.len());
            for (k, v) in sorted {
                out.insert(k, v);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonicalize).collect())
        }
        other => other.clone(),
    }
}

/// Convenience: sign the canonical bytes of a structured payload.
pub fn sign_canonical(value: &serde_json::Value) -> String {
    sign(&canonical_bytes(value))
}

/// Convenience: verify a signature over the canonical bytes of a payload.
pub fn verify_canonical(public_key: &str, value: &serde_json::Value, signature: &str) -> bool {
    verify(public_key, &canonical_bytes(value), signature)
}

/// Lowercase hex SHA-256 of arbitrary bytes. Shared content-hash helper so
/// signers and verifiers agree on how "content" is digested.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize tests that mutate process-wide env / key files.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_key<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("coevo-crypto-test-{}", uuid_like()));
        let key_path = dir.join("platform_signing.key");
        std::fs::create_dir_all(&dir).unwrap();
        let prev = std::env::var_os("COEVO_SIGNING_KEY_PATH");
        std::env::set_var("COEVO_SIGNING_KEY_PATH", &key_path);
        let result = f();
        match prev {
            Some(v) => std::env::set_var("COEVO_SIGNING_KEY_PATH", v),
            None => std::env::remove_var("COEVO_SIGNING_KEY_PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    // Tiny unique suffix without pulling uuid into a doctest path.
    fn uuid_like() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{nanos}-{:?}", std::thread::current().id()).replace(['(', ')', ' '], "")
    }

    #[test]
    fn load_or_create_is_stable_and_roundtrips() {
        with_temp_key(|| {
            let k1 = load_or_create_signing_key().unwrap();
            // Second load must read the same persisted seed.
            let k2 = load_or_create_signing_key().unwrap();
            assert_eq!(k1.to_bytes(), k2.to_bytes());

            let pk = hex::encode(k1.verifying_key().to_bytes());
            let payload = b"the quick brown fox";
            let sig = hex::encode(k1.sign(payload).to_bytes());
            assert!(verify(&pk, payload, &sig));
        });
    }

    #[test]
    fn verify_rejects_tampered_payload_and_garbage() {
        with_temp_key(|| {
            let key = load_or_create_signing_key().unwrap();
            let pk = hex::encode(key.verifying_key().to_bytes());
            let sig = hex::encode(key.sign(b"original").to_bytes());

            assert!(verify(&pk, b"original", &sig));
            assert!(!verify(&pk, b"tampered", &sig));
            assert!(!verify(&pk, b"original", "not-hex"));
            assert!(!verify(&pk, b"original", &"00".repeat(SIGNATURE_LEN)));
            assert!(!verify("deadbeef", b"original", &sig));
        });
    }

    #[test]
    fn canonical_bytes_is_key_order_independent() {
        let a = serde_json::json!({"b": 1, "a": {"y": 2, "x": 3}});
        let b = serde_json::json!({"a": {"x": 3, "y": 2}, "b": 1});
        assert_eq!(canonical_bytes(&a), canonical_bytes(&b));
        // And actually sorted: "a" precedes "b".
        let s = String::from_utf8(canonical_bytes(&a)).unwrap();
        assert!(s.starts_with("{\"a\":"));
    }
}
