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

use concord::reqwest::{Url, header::HeaderValue};
pub use concord_pagination::link::{
    PageLinkError as LinkError, PageLinks as Links, parse_page_links_str as parse_str,
};

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
    concord_pagination::link::parse_page_links(values, base)
}
