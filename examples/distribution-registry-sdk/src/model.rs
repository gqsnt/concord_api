//! Response bodies of the listing and token endpoints, as sent on the wire.

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
#[derive(Deserialize, concord::Debug)]
pub(crate) struct TokenBody {
    #[serde(default)]
    #[sensitive]
    pub(crate) token: Option<String>,
    #[serde(default)]
    #[sensitive]
    pub(crate) access_token: Option<String>,
    #[serde(default)]
    pub(crate) expires_in: Option<u64>,
}

#[cfg(test)]
mod sdk_type_canaries {
    use super::*;

    #[test]
    fn token_service_aliases_are_withheld_in_both_debug_modes() {
        let mut c = concord_check_canary::Canaries::new("registry-token-body");
        let token = c.issue();
        let alias = c.issue();
        let body: TokenBody = serde_json::from_value(serde_json::json!({
            "token": token, "access_token": alias, "expires_in": 60
        }))
        .unwrap();
        assert_eq!(body.token.as_deref(), Some(token.as_str()));
        assert_eq!(body.access_token.as_deref(), Some(alias.as_str()));
        c.assert_debug_absent("TokenBody", &body);
    }
}
