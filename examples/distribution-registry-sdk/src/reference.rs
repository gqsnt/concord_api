//! Validated inputs: repository names, tags, manifest references and page sizes.

use std::fmt;
use std::num::NonZeroU32;

use crate::digest::Digest;
use crate::error::InputError;

/// A repository name such as `library/ubuntu` or `acme/tools/app`.
///
/// The API spec's rule: one or more path components separated by `/`, each matching
/// `[a-z0-9]+(?:[._-][a-z0-9]+)*`, the whole name shorter than 256 characters. The name is sent
/// as **several** path segments (`/v2/acme/tools/app/...`), exactly as written.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RepositoryName(String);

impl RepositoryName {
    /// Validates `name`.
    ///
    /// # Errors
    ///
    /// [`InputError::RepositoryName`] when it breaks the rule above.
    pub fn new(name: &str) -> Result<Self, InputError> {
        if name.is_empty() || name.len() >= 256 || !name.split('/').all(valid_component) {
            return Err(InputError::RepositoryName);
        }
        Ok(Self(name.to_owned()))
    }

    /// The name as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `[a-z0-9]+(?:[._-][a-z0-9]+)*`: alphanumerics, single separators between them.
fn valid_component(component: &str) -> bool {
    let mut previous_separator = true; // a component cannot start with a separator
    for byte in component.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => previous_separator = false,
            b'.' | b'_' | b'-' if !previous_separator => previous_separator = true,
            _ => return false,
        }
    }
    !previous_separator
}

impl fmt::Display for RepositoryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RepositoryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RepositoryName({})", self.0)
    }
}

/// A tag such as `latest` or `v1.2.3`.
///
/// The API page gives no tag grammar. This SDK applies the Distribution reference grammar's tag
/// rule, `[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}`: it keeps a tag a single, unreserved path segment
/// that cannot be mistaken for a digest (no `:`).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Tag(String);

impl Tag {
    /// Validates `tag`.
    ///
    /// # Errors
    ///
    /// [`InputError::Tag`] when it breaks the rule above.
    pub fn new(tag: &str) -> Result<Self, InputError> {
        let mut bytes = tag.bytes();
        let first_ok = bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
        let rest_ok = bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'));
        if !first_ok || !rest_ok || tag.len() > 128 {
            return Err(InputError::Tag);
        }
        Ok(Self(tag.to_owned()))
    }

    /// The tag as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({})", self.0)
    }
}

/// The `<reference>` of `/v2/<name>/manifests/<reference>`: a tag or a digest.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Reference {
    /// A tag (mutable: the registry decides which manifest it names now).
    Tag(Tag),
    /// A digest (immutable: the content is verified against it).
    Digest(Digest),
}

impl Reference {
    /// A tag reference.
    ///
    /// # Errors
    ///
    /// [`InputError::Tag`].
    pub fn tag(tag: &str) -> Result<Self, InputError> {
        Tag::new(tag).map(Self::Tag)
    }

    /// A digest reference.
    pub fn digest(digest: Digest) -> Self {
        Self::Digest(digest)
    }

    /// The digest, for a digest reference.
    pub fn as_digest(&self) -> Option<&Digest> {
        match self {
            Self::Digest(digest) => Some(digest),
            Self::Tag(_) => None,
        }
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag(tag) => fmt::Display::fmt(tag, f),
            Self::Digest(digest) => fmt::Display::fmt(digest, f),
        }
    }
}

/// A page size, the `n` parameter of the catalog and tag listings: at least 1. The spec leaves
/// `n=0` undefined, and a registry refuses sizes over its own maximum
/// (`PAGINATION_NUMBER_INVALID`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageSize(NonZeroU32);

impl PageSize {
    /// `n` entries per page.
    ///
    /// # Errors
    ///
    /// [`InputError::PageSize`] for 0.
    pub fn new(n: u32) -> Result<Self, InputError> {
        NonZeroU32::new(n).map(Self).ok_or(InputError::PageSize)
    }

    /// The size.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for PageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_names_follow_the_api_rule() {
        for ok in ["a", "library/ubuntu", "acme/tools/app", "a.b_c-d/e1", "0/1"] {
            assert!(RepositoryName::new(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", "A", "a/", "/a", "a//b", "a..b", "a__b", "-a", "a-", "a b", "a%2fb", "a:b", "..",
        ] {
            assert!(RepositoryName::new(bad).is_err(), "{bad:?}");
        }
        assert!(RepositoryName::new(&"a".repeat(255)).is_ok());
        assert!(RepositoryName::new(&"a".repeat(256)).is_err());
    }

    #[test]
    fn tags_are_single_unreserved_segments() {
        for ok in ["latest", "v1.2.3", "_x", "A-b.c"] {
            assert!(Tag::new(ok).is_ok(), "{ok}");
        }
        for bad in ["", ".x", "-x", "a:b", "a/b", "a b", "ü"] {
            assert!(Tag::new(bad).is_err(), "{bad:?}");
        }
        assert!(Tag::new(&"a".repeat(128)).is_ok());
        assert!(Tag::new(&"a".repeat(129)).is_err());
    }
}
