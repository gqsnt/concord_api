//! U15 / U32 (R07): editing the native request and finishing with the declared decoder.
//!
//! [`concord::Prepared::into_parts`] is a one-way door: it returns the application's
//! `reqwest::Client`, the native `reqwest::Request` and the endpoint's [`concord::Decoder`].
//! Anything reqwest can do to a request is available; afterwards
//! `decoder.finish(client.execute(request).await)` applies the declared status check and
//! representation and converts a native transport error into a Concord
//! [`concord::ErrorKind::Transport`] error. Concord's redacted `Debug` does not follow the native
//! request: the native `Debug` prints the full URL (sensitive query values and path segments
//! included) and every header value not flagged sensitive.

use std::time::Duration;

use concord::reqwest::header::{HeaderName, HeaderValue};

use crate::api::{Inventory, Item};

/// Inserts `extra.1` verbatim under the caller-supplied name `extra.0`, replacing that name
/// in the prepared request before execution.
///
/// Fetches one item with a per-request timeout and an extra header (for example a request id).
///
/// Use it when a single call needs transport settings or headers the declaration does not
/// carry. The timeout is reqwest's per-request timeout (it covers connecting, waiting for the
/// response and reading the body); when it fires, the result is an
/// [`concord::ErrorKind::Transport`] error whose [`concord::Error::transport`] reports
/// `is_timeout()`. Whether such a failure may be retried is the caller's decision. If the
/// extra header carries a secret, call `HeaderValue::set_sensitive(true)` yourself.
pub async fn item_with_timeout(
    api: &Inventory,
    id: u64,
    timeout: Duration,
    extra: (HeaderName, HeaderValue),
) -> Result<Item, concord::Error> {
    let (client, mut request, decoder) = api.item(id).prepare()?.into_parts();
    *request.timeout_mut() = Some(timeout);
    request.headers_mut().insert(extra.0, extra.1);
    decoder.finish(client.execute(request).await).await
}

/// One search filter: `f=<field>&op=<op>&v=<value>`.
#[derive(Debug, Clone, Copy)]
pub struct Filter<'a> {
    /// Field name.
    pub field: &'a str,
    /// Operator, e.g. `eq` or `gt`.
    pub op: &'a str,
    /// Operand.
    pub value: &'a str,
}

/// Searches with an interleaved filter sequence `f=a&op=eq&v=1&f=b&op=gt&v=2`.
///
/// Concord's query composition keeps one slot per key, so interleaving repeats *across* keys
/// cannot be declared (SEMANTICS §5). The application appends the pairs to the native URL after
/// the declared ones (`limit=10` stays first); `query_pairs_mut` applies the same
/// `application/x-www-form-urlencoded` encoding as Concord. Declared sensitive query keys are no
/// longer redacted in any native `Debug` output.
pub async fn search_filters(
    api: &Inventory,
    filters: &[Filter<'_>],
) -> Result<Vec<Item>, concord::Error> {
    let (client, mut request, decoder) = api.search().prepare()?.into_parts();
    if !filters.is_empty() {
        let mut pairs = request.url_mut().query_pairs_mut();
        for filter in filters {
            pairs
                .append_pair("f", filter.field)
                .append_pair("op", filter.op)
                .append_pair("v", filter.value);
        }
    }
    decoder.finish(client.execute(request).await).await
}
