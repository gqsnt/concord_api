//! Response bodies of the listing and token endpoints, as sent on the wire.

use std::fmt;

use serde::Deserialize;

/// `GET /v2/_catalog`: `{"repositories": [<name>, ...]}`. A `null` list reads as empty.
#[derive(Debug, Deserialize)]
pub(crate) struct CatalogBody {
    #[serde(default)]
    pub(crate) repositories: Option<Vec<String>>,
}

/// `GET /v2/<name>/tags/list`: `{"name": <name>, "tags": [<tag>, ...]}`. A `null` list reads as
/// empty.
#[derive(Debug, Deserialize)]
pub(crate) struct TagsBody {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) tags: Option<Vec<String>>,
}

/// The token service's answer: `token` (or its OAuth 2.0 alias `access_token`; at least one is
/// present), `expires_in` (seconds, 60 when absent), `issued_at` (not read). A `refresh_token` is
/// never requested and is ignored. `Debug` withholds the token.
#[derive(Deserialize)]
pub(crate) struct TokenBody {
    #[serde(default)]
    pub(crate) token: Option<String>,
    #[serde(default)]
    pub(crate) access_token: Option<String>,
    #[serde(default)]
    pub(crate) expires_in: Option<u64>,
}

impl fmt::Debug for TokenBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenBody")
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_in", &self.expires_in)
            .finish()
    }
}
