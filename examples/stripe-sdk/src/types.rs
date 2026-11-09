//! Small value types shared by the declaration, the models and the request bodies.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use concord::WireDisplay;
use serde::{Deserialize, Deserializer, Serialize};

/// The Stripe API version this SDK implements, sent as `Stripe-Version` on every request unless
/// the context sets another one. Recorded with its sources in `PROVIDER.md`.
pub const PINNED_API_VERSION: &str = "2026-10-28.endive";

/// Arbitrary key-value metadata attached to an object. Kept dynamic on purpose: its keys are the
/// application's, not part of Stripe's schema.
pub type Metadata = BTreeMap<String, String>;

/// A value the SDK refused before building a request. `Display` names the rule, never the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueError {
    /// A currency is not three lowercase ASCII letters.
    Currency,
    /// A page size outside 1–100.
    Limit,
    /// An expansion path that is empty, has an empty segment or more than four levels.
    ExpandPath,
    /// An API version that is empty or not a valid header value.
    ApiVersion,
    /// A secret key without an `sk_` or `rk_` prefix, or not a valid header value.
    SecretKey,
    /// An idempotency key that is empty, longer than 255 characters or not visible ASCII.
    IdempotencyKey,
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Currency => "currency must be three lowercase ASCII letters",
            Self::Limit => "list limit must be between 1 and 100",
            Self::ExpandPath => "expansion path must have 1-4 non-empty dot-separated segments",
            Self::ApiVersion => "API version must be non-empty visible ASCII",
            Self::SecretKey => "secret key must start with `sk_` or `rk_` and be visible ASCII",
            Self::IdempotencyKey => "idempotency key must be 1-255 visible ASCII characters",
        })
    }
}

impl std::error::Error for ValueError {}

/// A three-letter ISO currency code in lowercase, as Stripe writes it (`usd`, `eur`). Totals are
/// kept per currency; amounts in different currencies are never added.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, WireDisplay)]
#[serde(transparent)]
pub struct Currency(String);

impl Currency {
    /// Validates a currency code.
    ///
    /// # Errors
    ///
    /// [`ValueError::Currency`] unless the text is three lowercase ASCII letters.
    pub fn new(code: impl Into<String>) -> Result<Self, ValueError> {
        let code = code.into();
        if code.len() == 3 && code.bytes().all(|b| b.is_ascii_lowercase()) {
            Ok(Self(code))
        } else {
            Err(ValueError::Currency)
        }
    }

    /// The code, such as `usd`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Currency {
    type Err = ValueError;
    fn from_str(code: &str) -> Result<Self, ValueError> {
        Self::new(code)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Self::new(code).map_err(serde::de::Error::custom)
    }
}

/// A Unix timestamp in seconds, as Stripe writes every time (`created`, `period_start`).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, WireDisplay,
)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

/// A list page size, 1–100 (Stripe's default is 10 when `limit` is absent).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, WireDisplay)]
#[serde(transparent)]
pub struct Limit(u8);

impl Limit {
    /// Validates a page size.
    ///
    /// # Errors
    ///
    /// [`ValueError::Limit`] outside 1–100.
    pub fn new(limit: u8) -> Result<Self, ValueError> {
        if (1..=100).contains(&limit) {
            Ok(Self(limit))
        } else {
            Err(ValueError::Limit)
        }
    }

    /// The page size.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// One `expand[]` entry: a dot-separated path such as `customer`, `data.customer` (on a list) or
/// `parent.subscription_details.subscription`. Stripe documents a maximum depth of four levels.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, WireDisplay)]
#[serde(transparent)]
pub struct ExpandPath(String);

impl ExpandPath {
    /// Validates an expansion path.
    ///
    /// # Errors
    ///
    /// [`ValueError::ExpandPath`] when it is empty, has an empty segment, more than four
    /// segments, or a character outside `[a-z0-9_]`.
    pub fn new(path: impl Into<String>) -> Result<Self, ValueError> {
        let path = path.into();
        let segments: Vec<&str> = path.split('.').collect();
        let valid = !path.is_empty()
            && segments.len() <= 4
            && segments.iter().all(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            });
        if valid {
            Ok(Self(path))
        } else {
            Err(ValueError::ExpandPath)
        }
    }

    /// The path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A Stripe API version, such as [`PINNED_API_VERSION`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, WireDisplay)]
#[serde(transparent)]
pub struct ApiVersion(String);

impl ApiVersion {
    /// The version this SDK implements ([`PINNED_API_VERSION`]).
    pub fn pinned() -> Self {
        Self(PINNED_API_VERSION.to_owned())
    }

    /// Another version, for an application that has checked its models against it.
    ///
    /// # Errors
    ///
    /// [`ValueError::ApiVersion`] for an empty value or one that is not visible ASCII.
    pub fn new(version: impl Into<String>) -> Result<Self, ValueError> {
        let version = version.into();
        if !version.is_empty() && version.bytes().all(|b| b.is_ascii_graphic()) {
            Ok(Self(version))
        } else {
            Err(ValueError::ApiVersion)
        }
    }

    /// The version text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A Stripe secret (`sk_...`) or restricted (`rk_...`) API key. It has no `Display`; `Debug` is
/// redacted. It reaches the wire only inside the context's `Authorization` header value, which
/// is flagged sensitive.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretKey(String);

impl SecretKey {
    /// Validates a key's shape (not its validity: only Stripe knows that).
    ///
    /// # Errors
    ///
    /// [`ValueError::SecretKey`] unless it starts with `sk_` or `rk_` and is visible ASCII.
    pub fn new(key: impl Into<String>) -> Result<Self, ValueError> {
        let key = key.into();
        let prefixed = key.starts_with("sk_") || key.starts_with("rk_");
        if prefixed && key.len() > 3 && key.bytes().all(|b| b.is_ascii_graphic()) {
            Ok(Self(key))
        } else {
            Err(ValueError::SecretKey)
        }
    }

    /// Whether this is a test-mode or sandbox key (`sk_test_`/`rk_test_`). Live mutation gates
    /// use it as one of their conditions.
    pub fn is_test_mode(&self) -> bool {
        self.0.starts_with("sk_test_") || self.0.starts_with("rk_test_")
    }

    /// The key text, for building the `Authorization` header. Deliberately named.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(<redacted>)")
    }
}

/// An application-supplied `Idempotency-Key`. The SDK never generates one: the application owns
/// the key policy (which operation a key names, when a repeat is safe). `Debug` is redacted;
/// `Display` is the key text, which the declaration places in a header marked sensitive.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Validates a key: 1–255 visible ASCII characters (Stripe documents keys up to 255
    /// characters).
    ///
    /// # Errors
    ///
    /// [`ValueError::IdempotencyKey`] otherwise.
    pub fn new(key: impl Into<String>) -> Result<Self, ValueError> {
        let key = key.into();
        if !key.is_empty() && key.len() <= 255 && key.bytes().all(|b| b.is_ascii_graphic()) {
            Ok(Self(key))
        } else {
            Err(ValueError::IdempotencyKey)
        }
    }

    /// The key text. Deliberately named.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IdempotencyKey(<redacted>)")
    }
}
