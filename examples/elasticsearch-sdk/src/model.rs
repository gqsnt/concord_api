//! Validated input values shared by the bulk and point-in-time workflows.

use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;

use concord::reqwest::header::HeaderValue;
use serde::{Serialize, Serializer};

use crate::error::{Error, Invalid};

/// The name of one index, data stream or alias.
///
/// This slice refuses an empty name, a name starting with `_` (the namespace of API endpoints
/// such as `_bulk` and `_pit`), and `,`, `*`, `/`, `\`, whitespace and control characters
/// (multi-target expressions and wildcards are outside the slice). Elasticsearch applies its own
/// naming rules on top.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct IndexName(Arc<str>);

impl IndexName {
    /// Validates `name`.
    pub fn new(name: &str) -> Result<Self, Error> {
        let refused = name.is_empty()
            || name.starts_with('_')
            || name.chars().any(|c| {
                matches!(c, ',' | '*' | '/' | '\\') || c.is_whitespace() || c.is_control()
            });
        if refused {
            return Err(Error::Invalid(Invalid::IndexName));
        }
        Ok(Self(name.into()))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IndexName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for IndexName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IndexName({:?})", &*self.0)
    }
}

impl Serialize for IndexName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// A document `_id` chosen by the caller (non-empty).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DocumentId(Arc<str>);

impl DocumentId {
    /// Validates `id`.
    pub fn new(id: &str) -> Result<Self, Error> {
        if id.is_empty() {
            return Err(Error::Invalid(Invalid::DocumentId));
        }
        Ok(Self(id.into()))
    }

    /// The id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DocumentId({:?})", &*self.0)
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for DocumentId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// An Elasticsearch API key, sent as `Authorization: ApiKey <key>` (the form the bulk API
/// reference's curl example uses). Pass the encoded key Elasticsearch issued; the SDK does not
/// encode it again. `Debug` never prints it.
#[derive(Clone)]
pub struct ApiKey(HeaderValue);

impl ApiKey {
    /// Validates the key (non-empty visible ASCII) and builds the sensitive header value with
    /// `concord_auth::authorization`.
    pub fn new(encoded: &str) -> Result<Self, Error> {
        concord_auth::authorization("ApiKey", encoded)
            .map(Self)
            .map_err(|_| Error::Invalid(Invalid::ApiKey))
    }

    pub(crate) fn into_header(self) -> HeaderValue {
        self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// The bulk API's `refresh` query parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    /// `true`: refresh the affected shards.
    True,
    /// `false`: do nothing with refreshes (Elasticsearch's default).
    False,
    /// `wait_for`: wait for a refresh to make the operations visible to search.
    WaitFor,
}

impl fmt::Display for Refresh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refresh::True => "true",
            Refresh::False => "false",
            Refresh::WaitFor => "wait_for",
        })
    }
}

/// How long a point in time stays open after each request that names it, in whole minutes
/// (`1m` is the form the pagination guide uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeepAlive(NonZeroU32);

impl KeepAlive {
    /// `n` minutes.
    pub fn minutes(n: NonZeroU32) -> Self {
        Self(n)
    }
}

impl fmt::Display for KeepAlive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}m", self.0)
    }
}

impl Serialize for KeepAlive {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
