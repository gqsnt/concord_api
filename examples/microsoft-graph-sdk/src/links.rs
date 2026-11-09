//! Opaque links bound to their tenant, principal, query and collection.

use concord::reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::{AuthContext, GraphError, LinkProblem};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Collection {
    UsersDelta,
    Groups,
    Members(String),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub context: AuthContext,
    pub base: String,
    pub collection: Collection,
    pub select: String,
    pub path: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LinkKind {
    Next,
    Delta,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredLink {
    pub version: u8,
    pub kind: LinkKind,
    pub binding: Binding,
    pub url: String,
}

impl StoredLink {
    pub(crate) fn parse(json: &str) -> Result<Self, GraphError> {
        // Storage is application-owned; bound parsing too, including malicious/corrupt files.
        if json.len() > 64 * 1024 {
            return Err(GraphError::link(LinkProblem::MalformedStorage));
        }
        let link: Self = serde_json::from_str(json)
            .map_err(|_| GraphError::link(LinkProblem::MalformedStorage))?;
        if link.version != 1 {
            return Err(GraphError::link(LinkProblem::MalformedStorage));
        }
        Ok(link)
    }

    pub(crate) fn check(&self, expected: &Binding, kind: LinkKind) -> Result<(), GraphError> {
        if self.binding != *expected || self.kind != kind || self.version != 1 {
            return Err(GraphError::link(LinkProblem::BindingMismatch));
        }
        validate_url(&self.url, expected)?;
        Ok(())
    }
}

/// The next page within one enumeration, never an incremental-sync checkpoint.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
pub struct NextLink(#[sensitive] pub(crate) StoredLink);

/// A completed users-delta round's checkpoint. Persist only after committing its changes.
/// Serialization contains the opaque URL and context, but never the bearer token.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
pub struct DeltaLink(#[sensitive] pub(crate) StoredLink);

macro_rules! link_methods {
    ($name:ident) => {
        impl $name {
            /// Versioned storage representation, unredacted. Store as sensitive application data.
            /// This is an integrity binding against accidental reuse, not a signed capability.
            pub fn to_json(&self) -> String {
                serde_json::to_string(&self.0)
                    .expect("stored link contains only JSON-compatible data")
            }

            /// Exact unredacted URL returned by Graph. Do not log it or rebuild its query.
            pub fn url(&self) -> &str {
                &self.0.url
            }
        }
    };
}
link_methods!(NextLink);
link_methods!(DeltaLink);

pub(crate) fn validate_url(raw: &str, binding: &Binding) -> Result<Url, GraphError> {
    let refused = || GraphError::link(LinkProblem::UnsafeDestination);
    // URL parsers may strip whitespace or normalize backslashes; do not accept those inputs.
    if raw.is_empty()
        || raw
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return Err(refused());
    }
    let url = Url::parse(raw).map_err(|_| refused())?;
    let base = Url::parse(&binding.base).map_err(|_| refused())?;
    let path_matches = url.path() == binding.path
        || (binding.collection == Collection::UsersDelta
            && url.path() == format!("{}/users/microsoft.graph.delta", base.path()));
    if url.origin() != base.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !path_matches
    {
        return Err(refused());
    }
    Ok(url)
}
