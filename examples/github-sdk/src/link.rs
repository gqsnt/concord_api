//! GitHub's four pagination relations. Grammar still uses concord-pagination's RFC 8288 parser.
//! Relative targets resolve against the response URL; conflicts and malformed fields fail.
//! Debug reports presence, never targets. These exports preserve the SDK's public API.

use concord::reqwest::{Url, header::HeaderValue};
pub use concord_pagination::link::{
    PageLinkError as LinkError, PageLinks as Links, parse_page_links_str as parse_str,
};

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
    concord_pagination::link::parse_page_links(values, response_url)
}
