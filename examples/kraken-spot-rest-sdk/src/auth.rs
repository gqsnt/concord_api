//! Credentials and a process-local, shared nonce allocator.

use crate::LocalError;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use concord::reqwest::header::HeaderValue;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256, Sha512};
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// A shared counter seeded with the last nonce already used for this API key.
///
/// Clones allocate distinct, strictly increasing values, including across threads.
/// This establishes allocation order only, never network arrival order. Share one
/// source per key; persist its high-water mark across restarts and coordinate other
/// processes yourself. Cancellation consumes an allocation. Exhaustion never wraps.
#[derive(Clone, concord::Debug)]
pub struct Nonces(#[sensitive] Arc<AtomicU64>);

impl Nonces {
    /// Seeds the counter; the first allocation is `last_used + 1`.
    pub fn new(last_used: u64) -> Self {
        Self(Arc::new(AtomicU64::new(last_used)))
    }

    /// Allocates once, or refuses if the unsigned 64-bit space is exhausted.
    pub fn next(&self) -> Result<u64, LocalError> {
        self.0
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map(|n| n + 1)
            .map_err(|_| LocalError::NonceExhausted)
    }
}

/// Validated API key and decoded secret. Cloning shares the secret allocation.
/// Debug and Display withhold both values; memory is not zeroized.
#[derive(Clone, concord::Debug)]
pub struct Credentials(#[sensitive] Arc<Keys>);
struct Keys {
    key: HeaderValue,
    secret: Vec<u8>,
}

impl Credentials {
    /// Requires a nonempty ASCII token key and a nonempty standard padded Base64 secret.
    pub fn new(key: &str, secret_base64: &str) -> Result<Self, LocalError> {
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(LocalError::InvalidKey);
        }
        let mut key = HeaderValue::from_str(key).map_err(|_| LocalError::InvalidKey)?;
        key.set_sensitive(true);
        let secret = STANDARD
            .decode(secret_base64)
            .map_err(|_| LocalError::InvalidSecret)?;
        if secret.is_empty() {
            return Err(LocalError::InvalidSecret);
        }
        Ok(Self(Arc::new(Keys { key, secret })))
    }

    /// Computes API-Sign without performing I/O. `encoded` must be the exact body
    /// bytes and `path` the exact URI path (without origin or query).
    ///
    /// Formula: Base64(HMAC-SHA512(secret, path || SHA256(decimal nonce || body))).
    /// The returned header is flagged sensitive. This primitive declares no trading endpoint.
    pub fn signature(&self, path: &str, nonce: u64, encoded: &[u8]) -> HeaderValue {
        let mut hash = Sha256::new();
        hash.update(nonce.to_string().as_bytes());
        hash.update(encoded);
        let mut mac = Hmac::<Sha512>::new_from_slice(&self.0.secret)
            .expect("HMAC accepts keys of any length");
        mac.update(path.as_bytes());
        mac.update(&hash.finalize());
        let signature = STANDARD.encode(mac.finalize().into_bytes());
        let mut header = HeaderValue::from_str(&signature).expect("Base64 is a valid header");
        header.set_sensitive(true);
        header
    }

    pub(crate) fn key(&self) -> HeaderValue {
        self.0.key.clone()
    }
}

impl fmt::Display for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
