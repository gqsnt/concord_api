//! The `Link` response header (RFC 8288), as GitHub uses it for pagination.
//!
//! GitHub sends `Link: <url>; rel="next", <url>; rel="last", ...` on paginated responses and omits
//! the header when everything fits on one page. Its pagination guide documents four relations:
//! `prev` is omitted on the first page, `last` is omitted **when it cannot be calculated**, and the
//! traversal ends when no `rel="next"` remains. This module extracts those four relations; the
//! traversal only ever follows `next` (see [`crate::paging`]). `last` is reported but never used to
//! compute page numbers.
//!
//! Parsing is strict where guessing would be dangerous: a link-value that does not start with
//! `<`, an unterminated `<...>` or quoted string, a URL that does not resolve, or two **different**
//! URLs for the same relation are [`LinkError`]s, not "no next page". Unknown relations and unknown
//! parameters are ignored, as RFC 8288 requires. Relative references are resolved against the
//! URL of the response that carried them. Several `Link` header lines are combined, in order.

use std::fmt;

use concord::reqwest::Url;
use concord::reqwest::header::HeaderValue;
use concord_pagination::link as syntax;

/// The pagination relations of one response.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Links {
    next: Option<Url>,
    prev: Option<Url>,
    first: Option<Url>,
    last: Option<Url>,
}

impl Links {
    /// `rel="next"`: the page to fetch after this one. `None` on the last page.
    pub fn next(&self) -> Option<&Url> {
        self.next.as_ref()
    }

    /// `rel="prev"`.
    pub fn prev(&self) -> Option<&Url> {
        self.prev.as_ref()
    }

    /// `rel="first"`.
    pub fn first(&self) -> Option<&Url> {
        self.first.as_ref()
    }

    /// `rel="last"`; GitHub omits it when it cannot be calculated.
    pub fn last(&self) -> Option<&Url> {
        self.last.as_ref()
    }

    /// Whether no pagination relation was present (no `Link` header, or none GitHub uses).
    pub fn is_empty(&self) -> bool {
        self.next.is_none() && self.prev.is_none() && self.first.is_none() && self.last.is_none()
    }
}

/// `Debug` says which relations are present, never their URLs (they carry filters and cursors).
impl fmt::Debug for Links {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Links")
            .field("next", &self.next.is_some())
            .field("prev", &self.prev.is_some())
            .field("first", &self.first.is_some())
            .field("last", &self.last.is_some())
            .finish()
    }
}

/// Why a `Link` header could not be read. `Display`/`Debug` never contain the header text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkError {
    /// The header value is not visible ASCII text.
    NotText,
    /// A link-value does not start with `<` (byte offset within the combined value).
    ExpectedTarget {
        /// Byte offset.
        at: usize,
    },
    /// A `<...>` target or a quoted string is not terminated.
    Unterminated {
        /// Byte offset where it started.
        at: usize,
    },
    /// A parameter is not `name` or `name=value`.
    Parameter {
        /// Byte offset.
        at: usize,
    },
    /// The target does not resolve to a URL.
    Url,
    /// The same relation is given two different URLs.
    Conflict {
        /// The relation.
        rel: &'static str,
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Link header not understood: ")?;
        match self {
            Self::NotText => f.write_str("not visible ASCII text"),
            Self::ExpectedTarget { at } => write!(f, "expected `<` at byte {at}"),
            Self::Unterminated { at } => write!(f, "unterminated target or string at byte {at}"),
            Self::Parameter { at } => write!(f, "malformed parameter at byte {at}"),
            Self::Url => f.write_str("a target is not a valid URL reference"),
            Self::Conflict { rel } => write!(f, "two different `{rel}` targets"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Parses every `Link` header value of one response, resolving relative targets against
/// `response_url`.
///
/// # Errors
///
/// A [`LinkError`]; see the [module documentation](self).
pub fn parse<'a>(
    values: impl IntoIterator<Item = &'a HeaderValue>,
    response_url: &Url,
) -> Result<Links, LinkError> {
    project(syntax::parse(
        values.into_iter().map(HeaderValue::as_bytes),
        response_url,
    )?)
}

/// Parses one (combined) `Link` field value. See [`parse`].
///
/// # Errors
///
/// A [`LinkError`].
pub fn parse_str(text: &str, response_url: &Url) -> Result<Links, LinkError> {
    project(syntax::parse([text.as_bytes()], response_url)?)
}

// Keep the SDK's public diagnostics and relation/ambiguity policy; grammar lives in pagination.
impl From<syntax::LinkError> for LinkError {
    fn from(error: syntax::LinkError) -> Self {
        match error {
            syntax::LinkError::NotText { .. } => Self::NotText,
            syntax::LinkError::ExpectedTarget { at } => Self::ExpectedTarget { at },
            syntax::LinkError::Unterminated { at } => Self::Unterminated { at },
            syntax::LinkError::Parameter { at } => Self::Parameter { at },
            syntax::LinkError::Url { .. } => Self::Url,
        }
    }
}

fn project(parsed: Vec<syntax::Link>) -> Result<Links, LinkError> {
    let mut links = Links::default();
    for link in parsed {
        for (rel, slot) in [
            ("next", &mut links.next),
            ("prev", &mut links.prev),
            ("first", &mut links.first),
            ("last", &mut links.last),
        ] {
            if link.has_relation(rel) {
                match slot {
                    Some(existing) if *existing != link.target => {
                        return Err(LinkError::Conflict { rel });
                    }
                    Some(_) => {}
                    None => *slot = Some(link.target.clone()),
                }
            }
        }
    }
    Ok(links)
}
