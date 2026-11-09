//! Typed identifiers and opaque server values.
//!
//! Caller-supplied names are validated by their constructors against the Kubernetes naming
//! rules ([`NamespaceName::new`], [`ObjectName::new`]) before any request exists. Values decoded
//! from the server are taken as the server sent them (deserialization does not re-validate):
//! the API server is the authority for its own objects.
//!
//! Opaque server values ([`ResourceVersion`], [`ContinueToken`]) are passed back unmodified.
//! They are never parsed, compared numerically or synthesized by this SDK.

use std::fmt;

use concord::reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};

/// Why a caller-supplied name was refused. Never contains the name itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NameError {
    /// The name is empty.
    Empty,
    /// The name is longer than the rule allows (63 for a label, 253 for a subdomain).
    TooLong {
        /// The maximum length in bytes.
        max: usize,
    },
    /// The name contains a character outside lowercase `a-z`, `0-9`, `-` (and `.` for a
    /// subdomain), or does not start and end with an alphanumeric character.
    InvalidCharacters,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("name is empty"),
            Self::TooLong { max } => write!(f, "name is longer than {max} bytes"),
            Self::InvalidCharacters => f.write_str(
                "name must consist of lowercase alphanumerics and '-' (and '.' between labels), \
                 starting and ending with an alphanumeric",
            ),
        }
    }
}

impl std::error::Error for NameError {}

/// An RFC 1123 label: at most 63 bytes of `a-z`, `0-9` and `-`, starting and ending with an
/// alphanumeric (Kubernetes `IsDNS1123Label`).
fn check_dns1123_label(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > 63 {
        return Err(NameError::TooLong { max: 63 });
    }
    let bytes = name.as_bytes();
    let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    if !alnum(bytes[0]) || !alnum(bytes[bytes.len() - 1]) {
        return Err(NameError::InvalidCharacters);
    }
    if !bytes.iter().all(|&b| alnum(b) || b == b'-') {
        return Err(NameError::InvalidCharacters);
    }
    Ok(())
}

/// An RFC 1123 subdomain: at most 253 bytes, dot-separated RFC 1123 labels (Kubernetes
/// `IsDNS1123Subdomain`).
fn check_dns1123_subdomain(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > 253 {
        return Err(NameError::TooLong { max: 253 });
    }
    for label in name.split('.') {
        check_dns1123_label(label).map_err(|e| match e {
            NameError::TooLong { .. } | NameError::Empty => NameError::InvalidCharacters,
            other => other,
        })?;
    }
    Ok(())
}

/// A namespace name (an RFC 1123 label, as Kubernetes requires for namespaces).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NamespaceName(String);
serde_plain::derive_display_from_serialize!(NamespaceName);

impl NamespaceName {
    /// Validates `name` as an RFC 1123 label (`team-a`, `kube-system`).
    ///
    /// # Errors
    ///
    /// [`NameError`] when it is empty, longer than 63 bytes or uses other characters.
    pub fn new(name: impl Into<String>) -> Result<Self, NameError> {
        let name = name.into();
        check_dns1123_label(&name)?;
        Ok(Self(name))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The name of a namespaced or cluster-scoped object (an RFC 1123 subdomain, the general
/// Kubernetes object-name rule; some kinds, such as Services, are stricter and the server
/// enforces that).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectName(String);
serde_plain::derive_display_from_serialize!(ObjectName);

impl ObjectName {
    /// Validates `name` as an RFC 1123 subdomain (`web-7d4f`, `app.config`).
    ///
    /// # Errors
    ///
    /// [`NameError`] when it is empty, longer than 253 bytes or not dot-separated labels.
    pub fn new(name: impl Into<String>) -> Result<Self, NameError> {
        let name = name.into();
        check_dns1123_subdomain(&name)?;
        Ok(Self(name))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An object's or a collection's `resourceVersion`: opaque, passed back unmodified, compared
/// only for equality (never ordered or parsed as a number).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceVersion(String);
serde_plain::derive_display_from_serialize!(ResourceVersion);

impl ResourceVersion {
    /// A resource version as received (from a list, an object or a bookmark).
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The opaque text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the server sent an empty value (treated as absent).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// An object's `metadata.uid`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Uid(String);

impl Uid {
    /// The UID text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A list `continue` token: opaque, server-defined, valid only for the identical query and for a
/// few minutes. Its `Debug` is redacted; the declared `continue` query value is `#[sensitive]`, so
/// Concord's own `Debug` and error output withhold it too. `Display` is the raw token (it is what
/// goes on the wire) and is therefore only used by the request builder.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[derive(concord::Debug)]
pub struct ContinueToken(#[sensitive] String);
serde_plain::derive_display_from_serialize!(ContinueToken);

impl ContinueToken {
    /// A token as received from `metadata.continue`. This SDK never fabricates one: tokens come
    /// from the server only (a resume checkpoint stores one it received).
    pub fn from_server(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw token. Read deliberately: do not log it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// A bearer token for `Authorization: Bearer <token>` (a service-account token, an OIDC ID
/// token). `Debug` is redacted and there is no `Display`; the header value it produces carries
/// the native sensitivity flag.
#[derive(Clone, concord::Debug)]
pub struct BearerToken(#[sensitive] String);

impl BearerToken {
    /// Wraps a token.
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// The `Authorization` header value, flagged sensitive.
    ///
    /// # Errors
    ///
    /// [`InvalidToken`] when the token contains bytes a header value cannot carry (a line
    /// break, NUL) or is empty.
    pub fn header_value(&self) -> Result<HeaderValue, InvalidToken> {
        if self.0.is_empty() {
            return Err(InvalidToken);
        }
        let mut value =
            HeaderValue::from_str(&format!("Bearer {}", self.0)).map_err(|_| InvalidToken)?;
        value.set_sensitive(true);
        Ok(value)
    }
}

/// A bearer token that cannot be sent as a header value (empty, or with control bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidToken;

impl fmt::Display for InvalidToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bearer token is empty or not a valid header value")
    }
}

impl std::error::Error for InvalidToken {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_names_follow_rfc1123_labels() {
        assert!(NamespaceName::new("team-a").is_ok());
        assert!(NamespaceName::new("a").is_ok());
        assert_eq!(NamespaceName::new(""), Err(NameError::Empty));
        assert_eq!(
            NamespaceName::new("Team"),
            Err(NameError::InvalidCharacters)
        );
        assert_eq!(NamespaceName::new("-a"), Err(NameError::InvalidCharacters));
        assert_eq!(NamespaceName::new("a-"), Err(NameError::InvalidCharacters));
        assert_eq!(NamespaceName::new("a.b"), Err(NameError::InvalidCharacters));
        assert_eq!(NamespaceName::new("a/b"), Err(NameError::InvalidCharacters));
        assert_eq!(
            NamespaceName::new("a".repeat(64)),
            Err(NameError::TooLong { max: 63 })
        );
        assert!(NamespaceName::new("a".repeat(63)).is_ok());
    }

    #[test]
    fn object_names_follow_rfc1123_subdomains() {
        assert!(ObjectName::new("web-7d4f9c-x2x").is_ok());
        assert!(ObjectName::new("app.config").is_ok());
        assert_eq!(ObjectName::new("a..b"), Err(NameError::InvalidCharacters));
        assert_eq!(ObjectName::new(".a"), Err(NameError::InvalidCharacters));
        assert_eq!(ObjectName::new(".."), Err(NameError::InvalidCharacters));
        assert_eq!(ObjectName::new("a_b"), Err(NameError::InvalidCharacters));
        let long = [
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(61),
        ]
        .join(".");
        assert_eq!(long.len(), 253);
        assert!(ObjectName::new(long.clone()).is_ok());
        assert_eq!(
            ObjectName::new(format!("{long}e")),
            Err(NameError::TooLong { max: 253 })
        );
    }

    #[test]
    fn secrets_are_redacted_in_debug() {
        let token = BearerToken::new("sekrit-token-123");
        assert_eq!(format!("{token:?}"), "BearerToken(<redacted>)");
        let value = token.header_value().unwrap();
        assert!(value.is_sensitive());
        assert_eq!(value, "Bearer sekrit-token-123");
        assert_eq!(BearerToken::new("").header_value(), Err(InvalidToken));
        assert_eq!(BearerToken::new("a\nb").header_value(), Err(InvalidToken));

        let cont = ContinueToken::from_server("eyJ2IjoibWV0YS5rOHMuaW8vdjEifQ");
        assert_eq!(format!("{cont:?}"), "ContinueToken(<redacted>)");
        assert!(!format!("{:?}", Some(cont)).contains("eyJ"));
    }
}
