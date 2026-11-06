//! The `Link` response header (RFC 8288), as GitLab sends it on paginated responses.
//!
//! GitLab's REST documentation shows `Link: <url>; rel="prev", <url>; rel="next", <url>;
//! rel="first", <url>; rel="last"` and says to use these links instead of generating URLs. This
//! module reads those four relations. The traversal follows `next` only; `last` is omitted by
//! GitLab for collections over 10,000 records and is never used to compute anything.
//!
//! Parsing is strict where a guess would end a traversal silently: a link-value that does not
//! start with `<`, an unterminated `<...>` or quoted string, a malformed parameter, a target that
//! does not resolve, or two different URLs for one relation are a [`LinkError`], not "no next
//! page". Unknown relations and parameters are ignored (RFC 8288 §3.3: only the first `rel`
//! counts). Relative targets resolve against the URL of the response that carried them; several
//! `Link` field lines are combined in order.

use std::fmt;

use concord::reqwest::Url;
use concord::reqwest::header::HeaderValue;
use concord_pagination::link as syntax;

/// The pagination relations of one response. `Debug` says which are present, never their URLs
/// (they carry the listing's filters, a search term for example).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Links {
    next: Option<Url>,
    prev: Option<Url>,
    first: Option<Url>,
    last: Option<Url>,
}

impl Links {
    /// `rel="next"`: the page after this one; `None` on the last page.
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

    /// `rel="last"`; GitLab omits it for collections over 10,000 records.
    pub fn last(&self) -> Option<&Url> {
        self.last.as_ref()
    }
}

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

/// Why a `Link` header could not be read. `Display` and `Debug` never contain the header text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkError {
    /// The field value is not visible ASCII.
    NotText,
    /// A link-value does not start with `<` (byte offset in the combined value).
    ExpectedTarget {
        /// Byte offset.
        at: usize,
    },
    /// A `<...>` target or a quoted string is not terminated.
    Unterminated {
        /// Byte offset where it started.
        at: usize,
    },
    /// A parameter is not `; name` or `; name=value`.
    Parameter {
        /// Byte offset.
        at: usize,
    },
    /// A target is not a valid URL reference.
    Url,
    /// One relation has two different targets.
    Conflict {
        /// The relation.
        rel: &'static str,
    },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Link header not understood: ")?;
        match self {
            Self::NotText => f.write_str("not visible ASCII"),
            Self::ExpectedTarget { at } => write!(f, "expected `<` at byte {at}"),
            Self::Unterminated { at } => write!(f, "unterminated target or string at byte {at}"),
            Self::Parameter { at } => write!(f, "malformed parameter at byte {at}"),
            Self::Url => f.write_str("a target is not a valid URL reference"),
            Self::Conflict { rel } => write!(f, "two different `{rel}` targets"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Reads every `Link` field line of one response, resolving relative targets against `base` (the
/// response's URL).
///
/// # Errors
///
/// A [`LinkError`]; see the [module documentation](self).
pub fn parse<'a>(
    values: impl IntoIterator<Item = &'a HeaderValue>,
    base: &Url,
) -> Result<Links, LinkError> {
    project(syntax::parse(
        values.into_iter().map(HeaderValue::as_bytes),
        base,
    )?)
}

/// Reads one (combined) `Link` field value. See [`parse`].
///
/// # Errors
///
/// A [`LinkError`].
pub fn parse_str(text: &str, base: &Url) -> Result<Links, LinkError> {
    project(syntax::parse([text.as_bytes()], base)?)
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
