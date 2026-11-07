//! Explicit, immutable context for one member's listing.

use std::fmt;

use concord::reqwest::header::HeaderValue;
use serde::Serialize;

/// An invalid local input. The rejected value is never included in diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    /// A namespace ID must match Dropbox's `[-_0-9a-zA-Z:]+` schema.
    NamespaceId,
    /// A member ID must be nonempty ASCII with no whitespace or control characters.
    MemberId,
    /// Page size must be in the inclusive range 1–2000.
    PageSize,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Dropbox input: {self:?}")
    }
}

impl std::error::Error for InputError {}

/// An opaque namespace identifier, kept as text without integer conversion.
///
/// It is sent in the `Dropbox-API-Path-Root` header, which the declaration marks sensitive, so
/// `Debug` withholds it (as Concord withholds the header); [`as_str`](Self::as_str) returns it.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct NamespaceId(String);

impl fmt::Debug for NamespaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NamespaceId(<redacted>)")
    }
}

impl NamespaceId {
    /// Validate Dropbox's `[-_0-9a-zA-Z:]+` schema without normalizing the value.
    pub fn new(value: impl Into<String>) -> Result<Self, InputError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':'))
        {
            return Err(InputError::NamespaceId);
        }
        Ok(Self(value))
    }

    /// The namespace ID as supplied.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The three documented `Dropbox-API-Path-Root` modes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = ".tag", rename_all = "snake_case")]
pub enum PathRoot {
    /// Root relative paths at the selected member's home namespace.
    Home,
    /// Require this to be the selected member's current root; mismatch is HTTP 422.
    Root {
        /// Expected root namespace.
        root: NamespaceId,
    },
    /// Root at an accessible namespace, without asserting it is the member's current root.
    #[serde(rename = "namespace_id")]
    Namespace {
        /// Explicit namespace to select.
        namespace_id: NamespaceId,
    },
}

impl PathRoot {
    /// Construct validated `root` mode.
    pub fn root(value: impl Into<String>) -> Result<Self, InputError> {
        Ok(Self::Root {
            root: NamespaceId::new(value)?,
        })
    }

    /// Construct validated `namespace_id` mode.
    pub fn namespace(value: impl Into<String>) -> Result<Self, InputError> {
        Ok(Self::Namespace {
            namespace_id: NamespaceId::new(value)?,
        })
    }

    pub(crate) fn header(&self) -> HeaderValue {
        // This enum contains only static tags and validated ASCII identifiers. Serialization
        // is infallible and the resulting compact JSON is always valid ASCII header text.
        let json = serde_json::to_string(self).expect("PathRoot contains only strings");
        HeaderValue::from_str(&json).expect("PathRoot serializes to ASCII without controls")
    }
}

/// An owned member/root snapshot. No client-wide selected user or root exists.
/// Derive a new selection with [`Self::with_member`] and [`Self::with_path_root`].
/// Previously created requests, traversals and cursors retain their own snapshots.
#[derive(Clone, PartialEq, Eq)]
pub struct RequestContext {
    member: String,
    root: PathRoot,
}

impl RequestContext {
    /// Select a team member ID (normally `dbmid:...`) and an explicit path root.
    /// Validates header syntax; Dropbox validates membership and access rights.
    pub fn new(member: impl Into<String>, root: PathRoot) -> Result<Self, InputError> {
        let member = member.into();
        if member.is_empty()
            || !member.bytes().all(|b| b.is_ascii_graphic())
            || HeaderValue::from_str(&member).is_err()
        {
            return Err(InputError::MemberId);
        }
        Ok(Self { member, root })
    }

    /// Return a context selecting another member, preserving the current root.
    /// Uses the same validation as [`Self::new`]; no request is sent.
    /// Start a new listing with this context; existing cursors keep their original member.
    pub fn with_member(self, member: impl Into<String>) -> Result<Self, InputError> {
        Self::new(member, self.root)
    }

    /// Return a context selecting another root, preserving the current member.
    /// Start a new listing with this context; existing cursors keep their original root.
    #[must_use]
    pub fn with_path_root(mut self, root: PathRoot) -> Self {
        self.root = root;
        self
    }

    /// Selected member ID, for explicit inspection.
    pub fn member_id(&self) -> &str {
        &self.member
    }

    /// Selected root mode.
    pub fn path_root(&self) -> &PathRoot {
        &self.root
    }
}

impl fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RequestContext { member: <redacted>, root: <redacted> }")
    }
}
