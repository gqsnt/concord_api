#[cfg(doc)]
use super::Prepared;
use super::{Call, RequestSecrets};
use crate::answer::Answer;
use crate::error::{BuildFailure, Error, MismatchFailure};
use crate::page::{IntoNext, PageSource, Pages};
use crate::{Headers, Sending};
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::task::{Context, Poll};

/// `.pages()`: the call, and its pager. For each page, clone and [`Call::prepare`]
/// once, check that preparation's origin and request identity, then [`Prepared::send`].
/// Pin the first finished request's origin after `edit_request`, before middleware;
/// check later finished requests against it before request comparison. The separate
/// `at_url` check runs during construction, before edits, against the origin the call
/// builds from its base (or `with_base`). Stream bodies never compare as the same request.
/// On the next call returned by the pager, calls [`Call::answered_from`] with the
/// URL of the answer the pager saw, before that call's preparation. This supplies
/// the base for a relative `at_url` before its origin check and caller edits.
/// This needs neither `request()` inspection (which refuses one-shot bodies) nor
/// a subsequent `send_full()` (which would build a second request).
pub fn pages<C, N>(
    first: C,
    pager: impl FnMut(C, &Answer<C::Value>) -> N + Send + 'static,
) -> Pages<C::Value, C::Problem>
where
    C: Call,
    N: IntoNext<C>,
{
    Pages::new(Machine {
        state: State::Ready(first),
        pager,
        origin: None,
        last: None,
    })
}

struct Sent {
    // A streaming body cannot be cloned or compared. Its absence here always
    // means different, without inspecting or consuming any of its bytes.
    request: Option<reqwest::Request>,
    secrets: RequestSecrets,
}

struct Last {
    sent: Sent,
    status: http::StatusCode,
    headers: Headers,
    tries: u32,
}

// The whole machine is already boxed. Keep its in-flight request in that one
// allocation rather than allocating a second box on every page transition.
// PANIC-SAFETY: Keeping page state inline avoids an allocation on every state transition; no panic lint is relaxed.
#[allow(clippy::large_enum_variant)]
enum State<C: Call> {
    Ready(C),
    Flying {
        call: C,
        sent: Sent,
        endpoint: &'static str,
        tries: Arc<AtomicU32>,
        exchange: Sending<Answer<C::Value>, C::Problem>,
    },
    Failed(Error<C::Problem>),
    Over,
}

struct Machine<C: Call, F> {
    state: State<C>,
    pager: F,
    origin: Option<reqwest::Url>,
    last: Option<Last>,
}

impl<C, F, N> PageSource<C::Value, C::Problem> for Machine<C, F>
where
    C: Call,
    F: FnMut(C, &Answer<C::Value>) -> N + Send,
    N: IntoNext<C>,
{
    fn poll_page(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<C::Value, Error<C::Problem>>>> {
        loop {
            match std::mem::replace(&mut self.state, State::Over) {
                State::Ready(call) => {
                    let mut prepared = match call.clone().prepare() {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            self.state = State::Over;
                            return Poll::Ready(Some(Err(error)));
                        }
                    };
                    let endpoint = prepared.run.endpoint();
                    if self
                        .origin
                        .as_ref()
                        .is_some_and(|url| url.origin() != prepared.request.url().origin())
                    {
                        self.state = State::Over;
                        return Poll::Ready(Some(Err(Error::build(
                            endpoint,
                            BuildFailure::Origin,
                        ))));
                    }
                    self.origin
                        .get_or_insert_with(|| prepared.request.url().clone());
                    let sent = Sent {
                        request: prepared.request.try_clone(),
                        secrets: prepared.secrets.clone(),
                    };
                    if let Some(last) = &self.last {
                        if let (Some(left), Some(right)) = (&last.sent.request, &sent.request) {
                            let query: Vec<_> = last
                                .sent
                                .secrets
                                .query
                                .iter()
                                .chain(&sent.secrets.query)
                                .copied()
                                .collect();
                            if same_page_request(
                                left,
                                right,
                                &query,
                                &[],
                                last.sent.secrets.url.as_ref(),
                                sent.secrets.url.as_ref(),
                            ) {
                                let error = Error::mismatch(
                                    endpoint,
                                    last.tries,
                                    last.status,
                                    last.headers.clone(),
                                    MismatchFailure::NextPage,
                                );
                                self.state = State::Over;
                                return Poll::Ready(Some(Err(error)));
                            }
                        }
                    }
                    let tries = Arc::new(AtomicU32::new(0));
                    let count = tries.clone();
                    let read = prepared.one_try;
                    prepared.one_try = Box::new(move |response, info| {
                        count.store(info.number(), Ordering::Relaxed);
                        read(response, info)
                    });
                    self.state = State::Flying {
                        call,
                        sent,
                        endpoint,
                        tries,
                        exchange: prepared.send(),
                    };
                }
                State::Flying {
                    call,
                    sent,
                    endpoint,
                    tries,
                    mut exchange,
                } => {
                    let answer = match Pin::new(&mut exchange).poll(cx) {
                        Poll::Pending => {
                            self.state = State::Flying {
                                call,
                                sent,
                                endpoint,
                                tries,
                                exchange,
                            };
                            return Poll::Pending;
                        }
                        Poll::Ready(Err(error)) => return Poll::Ready(Some(Err(error))),
                        Poll::Ready(Ok(answer)) => answer,
                    };
                    let tries = tries.load(Ordering::Relaxed);
                    self.last = Some(Last {
                        sent,
                        status: answer.status(),
                        headers: answer.headers().clone(),
                        tries,
                    });
                    self.state = match (self.pager)(call.clone(), &answer).into_next() {
                        Ok(Some(next)) => State::Ready(match &answer.response_url {
                            Some(url) => next.answered_from(url.clone()),
                            None => next,
                        }),
                        Ok(None) => State::Over,
                        Err(fail) => State::Failed(fail.into_error(
                            endpoint,
                            tries,
                            answer.status(),
                            answer.headers().clone(),
                            true,
                        )),
                    };
                    return Poll::Ready(Some(Ok(answer.into_value())));
                }
                State::Failed(error) => return Poll::Ready(Some(Err(error))),
                State::Over => return Poll::Ready(None),
            }
        }
    }
}

pub use crate::page::fragmentless as without_fragment;

/// Byte ranges written by secrets in the built URL's host and encoded path.
/// Keep one instance per request: rotated secrets may have different lengths.
/// Ranges must be ordered, non-overlapping, and lie on UTF-8 boundaries.
#[derive(Clone, Debug, Default)]
pub struct SecretUrl {
    /// Ranges relative to `url.host_str()`.
    pub host: Vec<std::ops::Range<usize>>,
    /// Ranges relative to `url.path()`.
    pub path: Vec<std::ops::Range<usize>>,
}

/// Compare requests as built, before middleware: ignore fragments and secret values.
/// The metadata names whole secret query pairs or headers containing only secrets.
/// For a header mixing public and secret values, mark each secret `HeaderValue`
/// sensitive; the remaining values are compared in their original order.
/// Host and path ranges come from the builder for each request; only those bytes
/// are omitted, so public text beside a secret is still compared. The paging origin
/// restriction is checked independently, before this comparison.
/// `None` for a request's URL marks means an edit changed that URL: its marks,
/// including secret query pairs, are no longer known. Compare that URL as it is,
/// without its fragment, independently of whether the other request kept its marks.
/// Stream bodies cannot be compared as bytes and are never treated as equal here.
pub fn same_page_request(
    left: &reqwest::Request,
    right: &reqwest::Request,
    secret_query: &[&str],
    secret_headers: &[&str],
    left_url_secrets: Option<&SecretUrl>,
    right_url_secrets: Option<&SecretUrl>,
) -> bool {
    fn url_without_secrets(url: &reqwest::Url, keys: &[&str]) -> reqwest::Url {
        let mut clean = url.clone();
        clean.set_fragment(None);
        if let Some(raw) = url.query() {
            // Decode each key independently: empty query components must not
            // shift the correspondence between raw bytes and parsed pairs.
            let mut one_pair = url.clone();
            let mut removed = false;
            let kept: Vec<_> = raw
                .split('&')
                .filter(|part| {
                    one_pair.set_query(Some(part));
                    let secret = one_pair
                        .query_pairs()
                        .next()
                        .is_some_and(|(key, _)| keys.contains(&key.as_ref()));
                    removed |= secret;
                    !secret
                })
                .collect();
            if removed {
                let query = kept
                    .iter()
                    .any(|part| !part.is_empty())
                    .then(|| kept.join("&"));
                clean.set_query(query.as_deref());
            }
        }
        clean
    }
    fn without_pieces(text: &str, ranges: &[std::ops::Range<usize>]) -> Option<String> {
        let mut out = String::new();
        let mut end = 0;
        for range in ranges {
            if range.start < end || range.start > range.end {
                return None;
            }
            out.push_str(text.get(end..range.start)?);
            text.get(range.clone())?;
            end = range.end;
        }
        out.push_str(text.get(end..)?);
        Some(out)
    }
    let url_identity = |url: &reqwest::Url, secrets: Option<&SecretUrl>| {
        let host = without_pieces(
            url.host_str().unwrap_or(""),
            secrets.map_or(&[], |s| s.host.as_slice()),
        )?;
        let path = without_pieces(url.path(), secrets.map_or(&[], |s| s.path.as_slice()))?;
        let mut clean =
            url_without_secrets(url, if secrets.is_some() { secret_query } else { &[] });
        // Compare the rest of the URL separately, allowing even a whole host label
        // to be secret without trying to parse the redacted host as a real URL.
        clean.set_host(Some("comparison.invalid")).ok()?;
        clean.set_path("/");
        Some((clean, host, path))
    };
    fn body_bytes(request: &reqwest::Request) -> Option<&[u8]> {
        match request.body() {
            None => Some(&[]),
            Some(body) => body.as_bytes(),
        }
    }
    let headers = |request: &reqwest::Request| {
        let mut kept = http::HeaderMap::new();
        for (name, value) in request.headers() {
            if !value.is_sensitive()
                && !secret_headers
                    .iter()
                    .any(|secret| name.as_str().eq_ignore_ascii_case(secret))
            {
                kept.append(name.clone(), value.clone());
            }
        }
        kept
    };
    left.method() == right.method()
        && matches!((url_identity(left.url(), left_url_secrets), url_identity(right.url(), right_url_secrets)),
            (Some(a), Some(b)) if a == b)
        && matches!((body_bytes(left), body_bytes(right)), (Some(a), Some(b)) if a == b)
        && headers(left) == headers(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c8_6_next_url_fragments_are_dropped_without_changing_other_bytes() {
        assert_eq!(
            without_fragment("https://example.com/a?b=%23c#part"),
            "https://example.com/a?b=%23c"
        );
        assert_eq!(without_fragment("/a#one#two"), "/a");
        assert_eq!(without_fragment("/a?b=c"), "/a?b=c");
        assert_eq!(without_fragment("/a#"), "/a");
    }

    fn request(url: &str) -> reqwest::Request {
        reqwest::Request::new(http::Method::GET, url.parse().unwrap())
    }

    #[test]
    fn c8_4_page_comparison_checks_method_url_body_and_continuation_headers() {
        let a = request("https://example.test/a?page=1#old");
        let mut b = request("https://example.test/a?page=1#new");
        assert!(same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        *b.method_mut() = http::Method::POST;
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        *b.method_mut() = http::Method::GET;
        *b.url_mut() = "https://example.test/a?page=2".parse().unwrap();
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        *b.url_mut() = a.url().clone();
        b.headers_mut()
            .insert("x-continuation", "next-page".parse().unwrap());
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        b.headers_mut().clear();
        *b.body_mut() = Some("changed".into());
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        *b.body_mut() = Some("".into());
        assert!(same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        let mut a = a;
        *a.body_mut() = Some("same".into());
        *b.body_mut() = Some("same".into());
        assert!(same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
    }

    #[test]
    fn c8_4_page_comparison_ignores_all_secret_pairs_and_headers() {
        let mut a = request("https://example.test/a?token=old&page=1&token=twice#old");
        let mut b = request("https://example.test/a?token=new&page=1#new");
        a.headers_mut()
            .insert("authorization", "old-key".parse().unwrap());
        b.headers_mut()
            .insert("authorization", "new-key".parse().unwrap());
        assert!(same_page_request(
            &a,
            &b,
            &["token"],
            &["authorization"],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &["authorization"],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        assert!(!same_page_request(
            &a,
            &b,
            &["token"],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        a.headers_mut()
            .get_mut("authorization")
            .unwrap()
            .set_sensitive(true);
        // The public value on b still participates, even though a's value is secret.
        assert!(!same_page_request(
            &a,
            &b,
            &["token"],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        b.headers_mut()
            .get_mut("authorization")
            .unwrap()
            .set_sensitive(true);
        assert!(same_page_request(
            &a,
            &b,
            &["token"],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        b.headers_mut()
            .insert("x-continuation", "changed".parse().unwrap());
        assert!(!same_page_request(
            &a,
            &b,
            &["token"],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
    }

    #[test]
    fn c8_4_page_comparison_keeps_nonsecret_url_bytes_and_repeated_header_values() {
        let a = request("https://example.test/a?%74oken=x&key=%41");
        let b = request("https://example.test/a?key=%41&token=y&token=z");
        assert!(same_page_request(
            &a,
            &b,
            &["token"],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        let c = request("https://example.test/a?key=A");
        assert!(!same_page_request(
            &a,
            &c,
            &["token"],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        let mut a = request("https://example.test/a");
        let mut b = request("https://example.test/a");
        a.headers_mut().append("x-page", "a".parse().unwrap());
        a.headers_mut().append("x-page", "b".parse().unwrap());
        b.headers_mut().append("x-page", "a".parse().unwrap());
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        b.headers_mut().append("x-page", "b".parse().unwrap());
        assert!(same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
    }

    // Regression probes from experience/challenge/code-b.md.
    #[test]
    fn c8_4_page_comparison_does_not_discard_public_values_of_repeated_sensitive_header() {
        let mut a = request("https://x.test/");
        let mut b = request("https://x.test/");
        for (request, plain) in [(&mut a, "page=1"), (&mut b, "page=2")] {
            let mut secret = http::HeaderValue::from_static("credential");
            secret.set_sensitive(true);
            request.headers_mut().append("x-state", secret);
            request
                .headers_mut()
                .append("x-state", plain.parse().unwrap());
        }
        assert!(
            !same_page_request(
                &a,
                &b,
                &[],
                &[],
                Some(&SecretUrl::default()),
                Some(&SecretUrl::default())
            ),
            "public continuation value changed"
        );
        let mut secret = http::HeaderValue::from_static("rotated credential");
        secret.set_sensitive(true);
        b.headers_mut().remove("x-state");
        b.headers_mut().append("X-State", "page=1".parse().unwrap());
        b.headers_mut().append("X-State", secret);
        assert!(same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        a.headers_mut().append("x-state", "page=2".parse().unwrap());
        b.headers_mut().append("x-state", "page=2".parse().unwrap());
        assert!(same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        b.headers_mut().remove("x-state");
        b.headers_mut().append("x-state", "page=2".parse().unwrap());
        b.headers_mut().append("x-state", "page=1".parse().unwrap());
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
    }

    #[test]
    fn c8_4_page_comparison_handles_empty_query_components_without_hiding_values() {
        let a = request("https://example.test/a?&token=old&&page=1");
        let b = request("https://example.test/a?&token=new&&page=1");
        assert!(same_page_request(
            &a,
            &b,
            &["token"],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        let a = request("https://example.test/a?");
        let b = request("https://example.test/a");
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        assert!(!same_page_request(
            &a,
            &b,
            &["token"],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
    }

    #[test]
    fn c8_4_page_comparison_does_not_treat_unreadable_stream_bodies_as_equal() {
        let mut a = request("https://example.test/a");
        let mut b = request("https://example.test/a");
        for request in [&mut a, &mut b] {
            *request.body_mut() = Some(reqwest::Body::wrap_stream(futures_util::stream::empty::<
                Result<bytes::Bytes, std::io::Error>,
            >()));
        }
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
    }

    #[test]
    // These fixtures intentionally hold one secret byte range, not its individual offsets.
    #[allow(clippy::single_range_in_vec_init)]
    fn c8_4_a_rotating_path_token_is_not_page_progress() {
        // holes3 finding 2: the same Telegram poll after replacing its path credential.
        let a = request("https://api.telegram.org/botOLD/getUpdates");
        let b = request("https://api.telegram.org/botLONGER/getUpdates");
        let left = SecretUrl {
            path: vec![4..7],
            ..SecretUrl::default()
        };
        let right = SecretUrl {
            path: vec![4..10],
            ..SecretUrl::default()
        };
        assert!(same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&left),
            Some(&right)
        ));
        assert!(!same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&SecretUrl::default()),
            Some(&SecretUrl::default())
        ));
        let c = request("https://api.telegram.org/botLONGER/getOther");
        assert!(!same_page_request(
            &a,
            &c,
            &[],
            &[],
            Some(&left),
            Some(&right)
        ));
    }

    #[test]
    // These fixtures intentionally hold one secret byte range, not its individual offsets.
    #[allow(clippy::single_range_in_vec_init)]
    fn c8_4_secret_host_pieces_are_omitted_while_adjacent_public_text_is_compared() {
        let a = request("https://old.tenant.example/publicOLD/end?page=1");
        let b = request("https://new-secret.tenant.example/publicLONGER/end?page=1");
        let left = SecretUrl {
            host: vec![0..3],
            path: vec![7..10],
        };
        let right = SecretUrl {
            host: vec![0..10],
            path: vec![7..13],
        };
        assert!(same_page_request(
            &a,
            &b,
            &[],
            &[],
            Some(&left),
            Some(&right)
        ));
        let c = request("https://new-secret.other.example/publicLONGER/end?page=1");
        assert!(!same_page_request(
            &a,
            &c,
            &[],
            &[],
            Some(&left),
            Some(&right)
        ));
        let c = request("https://new-secret.tenant.example/publicLONGER/end?page=2");
        assert!(!same_page_request(
            &a,
            &c,
            &[],
            &[],
            Some(&left),
            Some(&right)
        ));
    }

    #[test]
    // This fixture intentionally holds one invalid byte range, not its individual offsets.
    #[allow(clippy::single_range_in_vec_init)]
    fn c8_4_invalid_secret_url_ranges_cannot_make_requests_equal() {
        let a = request("https://example.test/a");
        let bad = SecretUrl {
            path: vec![2..99],
            ..SecretUrl::default()
        };
        assert!(!same_page_request(&a, &a, &[], &[], Some(&bad), Some(&bad)));
    }

    #[test]
    // This fixture intentionally holds one secret byte range, not its individual offsets.
    #[allow(clippy::single_range_in_vec_init)]
    fn c8_4_edited_urls_with_unknown_marks_are_compared_as_they_are() {
        let a = request("https://example.test/botOLD/poll?token=old&page=1#old");
        let b = request("https://example.test/botNEW/poll?token=new&page=1#new");
        let marks = SecretUrl {
            path: vec![4..7],
            ..SecretUrl::default()
        };
        assert!(same_page_request(
            &a,
            &b,
            &["token"],
            &[],
            Some(&marks),
            Some(&marks)
        ));
        // Unknown marks on either request must not hide edited URL text.
        for (left, right) in [(None, None), (Some(&marks), None), (None, Some(&marks))] {
            assert!(!same_page_request(&a, &b, &["token"], &[], left, right));
        }
        // The unmarked URL already has the marked request's redacted spelling.
        let redacted = request("https://example.test/bot/poll?page=1#different");
        assert!(same_page_request(
            &a,
            &redacted,
            &["token"],
            &[],
            Some(&marks),
            None
        ));
        assert!(same_page_request(
            &redacted,
            &a,
            &["token"],
            &[],
            None,
            Some(&marks)
        ));
        let c = request("https://example.test/botOLD/poll?token=old&page=1#new");
        assert!(same_page_request(&a, &c, &["token"], &[], None, None));
        let query_changed = request("https://example.test/botOLD/poll?token=new&page=1");
        assert!(!same_page_request(
            &a,
            &query_changed,
            &["token"],
            &[],
            None,
            None
        ));
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;
    use crate::__private::{
        AdaptValue, Core, Draft, Idempotent, Options, Piece, Prepared, Run, ValueRef,
    };
    use crate::error::{Fail, Kind};
    use crate::page::Call as _;
    use crate::read::{Incoming, ReadError, Reader};
    use crate::transport::{Transport, TransportError, TryInfo};
    use crate::{Body, Secret};
    use concord_testkit::Script;
    use futures_util::{FutureExt, StreamExt};
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    #[derive(Clone)]
    struct ScriptTransport(Script);
    impl Transport for ScriptTransport {
        async fn send(
            &self,
            request: reqwest::Request,
            info: TryInfo,
        ) -> Result<reqwest::Response, TransportError> {
            concord_core::Transport::send(
                &self.0,
                request,
                concord_core::transport::TryInfo::new(info.endpoint(), info.number()),
            )
            .await
            .map_err(|_| TransportError::connect())
        }
    }

    #[derive(Clone, Copy)]
    enum Placement {
        None,
        Header,
        Path,
        Query,
        Host,
        Body,
    }

    #[derive(Clone)]
    struct List {
        core: Core<String>,
        options: Options<String>,
        at: u32,
        host: String,
        key: Secret,
        placement: Placement,
        body: Option<Body>,
        fresh: bool,
        builds: Arc<AtomicUsize>,
    }
    impl List {
        fn new(script: &Script) -> Self {
            Self {
                core: Core::default().with_transport(ScriptTransport(script.clone())),
                options: Options::default(),
                at: 0,
                host: "api".into(),
                key: Secret::from("old"),
                placement: Placement::None,
                body: None,
                fresh: false,
                builds: Arc::new(AtomicUsize::new(0)),
            }
        }
        fn draft(&self) -> Draft {
            self.builds.fetch_add(1, Ordering::SeqCst);
            let mut draft = Draft::new(http::Method::GET);
            if matches!(self.placement, Placement::Host) {
                draft.base(&[
                    Piece::Lit("https://"),
                    Piece::Val("key", &(&ValueRef(&self.key)).adapt()),
                    Piece::Lit(".test/root/"),
                ]);
            } else {
                draft.base(&[
                    Piece::Lit("https://"),
                    Piece::Val("host", &self.host),
                    Piece::Lit(".test/root/"),
                ]);
            }
            draft.segment(&[Piece::Lit("items")]);
            draft.query("at", &[Piece::Val("at", &self.at)]);
            match self.placement {
                Placement::Header => draft.header(
                    "authorization",
                    &[Piece::Val("key", &(&ValueRef(&self.key)).adapt())],
                ),
                Placement::Path => {
                    draft.segment(&[Piece::Val("key", &(&ValueRef(&self.key)).adapt())])
                }
                Placement::Query => {
                    draft.query("key", &[Piece::Val("key", &(&ValueRef(&self.key)).adapt())])
                }
                Placement::Body => {
                    draft.body(Body::new(self.key.expose().to_string(), "text/plain"))
                }
                Placement::None | Placement::Host => (),
            }
            if let Some(body) = &self.body {
                draft.body(body.clone());
            }
            if self.fresh {
                draft.body(Body::stream(futures_util::stream::iter([Ok::<
                    _,
                    std::io::Error,
                >(
                    bytes::Bytes::from_static(b"input"),
                )])));
            }
            draft
        }
    }
    impl crate::page::Call for List {
        type Value = Vec<u32>;
        type Problem = String;
        fn at_url(mut self, url: &str) -> Self {
            self.options.at_url(without_fragment(url));
            self
        }
    }
    struct Numbers;
    impl Reader<Vec<u32>> for Numbers {
        async fn read(&self, incoming: Incoming) -> Result<Vec<u32>, ReadError> {
            Ok(incoming
                .bytes(100)
                .await?
                .iter()
                .map(|&n| u32::from(n))
                .collect())
        }
    }
    impl Call for List {
        fn request(&self) -> Result<reqwest::Request, Error<String>> {
            self.draft().finish(&self.core, &self.options, "list")
        }
        fn answered_from(mut self, url: reqwest::Url) -> Self {
            self.options.answered_from(url);
            self
        }
        fn prepare(self) -> Result<Prepared<Vec<u32>, String>, Error<String>> {
            let run = Run::start(&self.core, &self.options, "list");
            self.draft().prepare(
                run,
                self.core,
                self.options,
                Idempotent::ByMethod,
                |response, info| async move {
                    let status = response.status();
                    let headers = response.headers().clone().into();
                    if !status.is_success() {
                        return Err(crate::__private::refused_plain(response, &info).await);
                    }
                    let value = crate::__private::read_body(&Numbers, response, &info).await?;
                    Ok(Answer::new(status, headers, value))
                },
            )
        }
        fn edit_request(
            mut self,
            edit: impl Fn(&mut reqwest::Request) + Send + Sync + 'static,
        ) -> Self {
            self.options.edit_request(edit);
            self
        }
    }
    fn next(mut call: List, answer: &Answer<Vec<u32>>) -> Option<List> {
        if answer.value().is_empty() {
            None
        } else {
            call.at += 1;
            Some(call)
        }
    }
    async fn over(pages: &mut Pages<Vec<u32>, String>) {
        assert!(pages.try_next().await.unwrap().is_none());
        assert!(pages.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_ready_builds_once_on_demand_and_gives_the_page_before_the_end() {
        let script = Script::new();
        script.answer(200).body("a");
        script.answer(200).body("");
        let call = List::new(&script);
        let builds = call.builds.clone();
        let edits = Arc::new(AtomicUsize::new(0));
        let count = edits.clone();
        let call = call.edit_request(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
        });
        let mut pages = pages(call, next);
        assert_eq!(builds.load(Ordering::SeqCst), 0);
        assert!(script.requests().is_empty());
        assert_eq!(pages.try_next().await.unwrap(), Some(vec![97]));
        assert_eq!(builds.load(Ordering::SeqCst), 1);
        assert_eq!(script.requests().len(), 1);
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert_eq!(pages.next().await.unwrap().unwrap(), Vec::<u32>::new());
        assert_eq!(builds.load(Ordering::SeqCst), 2);
        assert_eq!(edits.load(Ordering::SeqCst), 2);
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_same_request_and_pager_failures_arrive_once_after_the_page() {
        for case in 0..3 {
            let script = Script::new();
            script.answer(201).header("x-head", "kept").body("a");
            let mut pages = pages(
                List::new(&script),
                move |call, _| -> Result<Option<List>, Fail<String>> {
                    match case {
                        0 => Ok(Some(call)),
                        1 => Err(crate::mismatch("missing cursor")),
                        _ => Err(Fail::from("problem".to_owned())),
                    }
                },
            );
            assert_eq!(pages.try_next().await.unwrap(), Some(vec![97]));
            let error = pages.try_next().await.unwrap_err();
            assert_eq!(
                error.kind(),
                if case == 2 {
                    Kind::Refused
                } else {
                    Kind::Mismatch
                }
            );
            assert_eq!(error.status(), Some(http::StatusCode::CREATED));
            assert_eq!(error.headers().unwrap().get("x-head"), Some("kept"));
            assert_eq!(error.tries(), 1);
            if case == 1 {
                assert_eq!(error.reason(), Some("missing cursor"));
            }
            if case == 2 {
                assert_eq!(error.problem().map(String::as_str), Some("problem"));
            }
            assert_eq!(script.requests().len(), 1);
            over(&mut pages).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_secret_rotation_is_read_at_the_next_build_and_is_not_progress() {
        for placement in [
            Placement::Header,
            Placement::Path,
            Placement::Query,
            Placement::Host,
            Placement::Body,
        ] {
            let script = Script::new();
            script.answer(200).body("a");
            script.answer(200).body("b");
            let mut call = List::new(&script);
            call.placement = placement;
            let key = call.key.clone();
            let mut pages = pages(call, |call, _| Some(call));
            assert!(pages.try_next().await.unwrap().is_some());
            key.replace("new-longer");
            match placement {
                Placement::Body => {
                    assert_eq!(pages.try_next().await.unwrap(), Some(vec![98]));
                    assert_eq!(script.requests()[1].body(), Some(b"new-longer".as_slice()));
                    assert_eq!(pages.try_next().await.unwrap_err().kind(), Kind::Mismatch);
                }
                _ => {
                    assert_eq!(
                        pages.try_next().await.unwrap_err().kind(),
                        if matches!(placement, Placement::Host) {
                            Kind::Build
                        } else {
                            Kind::Mismatch
                        }
                    );
                    assert_eq!(script.requests().len(), 1);
                }
            }
            over(&mut pages).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_url_edits_discard_secret_marks() {
        for placement in [Placement::Path, Placement::Query] {
            let script = Script::new();
            script.answer(200).body("a");
            script.answer(200).body("b");
            let mut call = List::new(&script);
            call.placement = placement;
            let key = call.key.clone();
            let call = call.edit_request(|request| {
                request.url_mut().set_fragment(Some("edited"));
            });
            let mut pages = pages(call, |call, _| Some(call));
            assert!(pages.try_next().await.unwrap().is_some());
            key.replace("new");
            assert_eq!(pages.try_next().await.unwrap(), Some(vec![98]));
            assert_eq!(pages.try_next().await.unwrap_err().kind(), Kind::Mismatch);
            over(&mut pages).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_public_headers_advance_and_sensitive_edits_do_not() {
        for sensitive in [false, true] {
            let script = Script::new();
            script.answer(200).body("a");
            script.answer(200).body("b");
            let edits = Arc::new(AtomicUsize::new(0));
            let count = edits.clone();
            let call = List::new(&script).edit_request(move |request| {
                let mut value = count
                    .fetch_add(1, Ordering::SeqCst)
                    .to_string()
                    .parse::<http::HeaderValue>()
                    .unwrap();
                value.set_sensitive(sensitive);
                request.headers_mut().append("x-continuation", value);
            });
            let mut pages = pages(call, |call, answer| {
                if answer.value() == &[98] {
                    None
                } else {
                    Some(call)
                }
            });
            assert!(pages.try_next().await.unwrap().is_some());
            if sensitive {
                assert_eq!(pages.try_next().await.unwrap_err().kind(), Kind::Mismatch);
            } else {
                assert_eq!(pages.try_next().await.unwrap(), Some(vec![98]));
            }
            over(&mut pages).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_origin_is_the_first_finished_request_and_is_checked_before_identity() {
        for edited in [false, true] {
            let script = Script::new();
            script.answer(200).body("a");
            script.answer(200).body("");
            let mut call = List::new(&script);
            if edited {
                call = call.edit_request(|request| {
                    request.url_mut().set_host(Some("edited.test")).unwrap();
                });
            }
            let mut pages = pages(call, move |mut call, answer| {
                if answer.value().is_empty() {
                    None
                } else {
                    call.host = "other".into();
                    call.at += 1;
                    Some(call)
                }
            });
            assert!(pages.try_next().await.unwrap().is_some());
            if edited {
                assert_eq!(pages.try_next().await.unwrap(), Some(vec![]));
                assert!(script
                    .requests()
                    .iter()
                    .all(|r| r.url().starts_with("https://edited.test/")));
            } else {
                let error = pages.try_next().await.unwrap_err();
                assert_eq!(error.kind(), Kind::Build);
                assert!(error.to_string().contains("another origin"));
                assert_eq!(script.requests().len(), 1);
            }
            over(&mut pages).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_6_at_url_is_checked_before_edits_on_first_and_later_pages() {
        for first in [false, true] {
            let script = Script::new();
            script.answer(200).body("a");
            let mut call = List::new(&script).edit_request(|request| {
                request.url_mut().set_host(Some("edited.test")).unwrap();
            });
            if first {
                call = call.at_url("https://other.test/items");
            }
            let mut pages = pages(call, |call, _| {
                Some(call.at_url("https://other.test/items"))
            });
            if !first {
                assert!(pages.try_next().await.unwrap().is_some());
            }
            assert_eq!(pages.try_next().await.unwrap_err().kind(), Kind::Build);
            assert_eq!(script.requests().len(), usize::from(!first));
            over(&mut pages).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_6_answer_context_is_recorded_on_the_call_the_pager_returned() {
        let script = Script::new();
        script.answer(200).body("a");
        script.answer(200).body("");
        let first = List::new(&script).at_url("/different/deep/current");
        let replacement = List::new(&script);
        let mut pages = pages(first, move |_, answer| {
            if answer.value().is_empty() {
                None
            } else {
                Some(replacement.clone().at_url("../next?x=1#fragment"))
            }
        });
        assert!(pages.try_next().await.unwrap().is_some());
        assert!(pages.try_next().await.unwrap().is_some());
        assert_eq!(
            script.requests()[1].url(),
            "https://api.test/different/next?x=1"
        );
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_6_callers_relative_url_and_next_url_secret_collisions() {
        let script = Script::new();
        script.answer(200).body("a");
        script.answer(200).body("");
        let mut call = List::new(&script);
        call.placement = Placement::Query;
        let mut pages = pages(
            call.at_url("../first?key=stale&key=twice&public=1#fragment"),
            |call, answer| {
                if answer.value().is_empty() {
                    None
                } else {
                    Some(call.at_url("next?key=old&key=again&public=2#fragment"))
                }
            },
        );
        assert!(pages.try_next().await.unwrap().is_some());
        assert!(pages.try_next().await.unwrap().is_some());
        let requests = script.requests();
        assert_eq!(requests[0].url(), "https://api.test/first?public=1&key=old");
        assert_eq!(requests[1].url(), "https://api.test/next?public=2&key=old");
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_a_next_call_that_does_not_build_ends_the_pages() {
        for url in ["http://[", "https://other.test/"] {
            let script = Script::new();
            script.answer(200).body("a");
            let mut pages = pages(List::new(&script), move |call, _| Some(call.at_url(url)));
            assert!(pages.try_next().await.unwrap().is_some());
            assert_eq!(pages.try_next().await.unwrap_err().kind(), Kind::Build);
            assert_eq!(script.requests().len(), 1);
            over(&mut pages).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_shared_one_shot_and_composed_bodies_fail_before_the_second_send() {
        for composed in [false, true] {
            let script = Script::new();
            script.answer(200).read_upload().body("a");
            let stream = Body::stream(futures_util::stream::iter([Ok::<_, std::io::Error>(
                bytes::Bytes::from_static(b"input"),
            )]));
            let mut call = List::new(&script);
            call.body = Some(if composed {
                Body::concat([Body::new("prefix", "text/plain"), stream])
            } else {
                stream
            });
            let mut pages = pages(call, next);
            assert!(pages.try_next().await.unwrap().is_some());
            let error = pages.try_next().await.unwrap_err();
            assert_eq!(error.kind(), Kind::Build);
            assert!(error.to_string().contains("already sent"));
            assert_eq!(script.requests().len(), 1);
            over(&mut pages).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_fresh_streams_are_never_the_same_request() {
        let script = Script::new();
        script.answer(200).read_upload().body("a");
        script.answer(200).read_upload().body("");
        let mut call = List::new(&script);
        call.fresh = true;
        let mut pages = pages(call, |call, answer| {
            if answer.value().is_empty() {
                None
            } else {
                Some(call)
            }
        });
        assert!(pages.try_next().await.unwrap().is_some());
        assert_eq!(pages.try_next().await.unwrap(), Some(vec![]));
        assert_eq!(script.requests().len(), 2);
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_5_dropped_next_resumes_the_same_exchange_through_either_interface() {
        let script = Script::new();
        script.answer(200).after(Duration::from_secs(3)).body("a");
        script.answer(200).body("");
        let mut pages = pages(List::new(&script), next);
        assert!(pages.try_next().now_or_never().is_none());
        assert_eq!(script.requests().len(), 1);
        assert_eq!(pages.next().await.unwrap().unwrap(), vec![97]);
        assert_eq!(pages.try_next().await.unwrap(), Some(vec![]));
        assert_eq!(script.requests().len(), 2);
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_5_clocks_keep_running_in_flight_but_do_not_count_time_between_pages() {
        let script = Script::new();
        script.never();
        let mut call = List::new(&script);
        call.options.with_timeout(Duration::from_secs(2));
        let mut pages = pages(call, next);
        assert!(pages.try_next().now_or_never().is_none());
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(
            pages.try_next().await.unwrap_err().class(),
            Some(crate::error::Class::Timeout)
        );
        assert_eq!(script.requests().len(), 1);
        over(&mut pages).await;
        let script = Script::new();
        script.answer(200).body("a");
        script.answer(200).body("");
        let mut call = List::new(&script);
        call.options.with_timeout(Duration::from_secs(2));
        let mut pages = super::pages(call, next);
        assert!(pages.try_next().await.unwrap().is_some());
        tokio::time::advance(Duration::from_secs(500)).await;
        assert!(pages.try_next().await.unwrap().is_some());
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_7_items_skip_empty_pages_preserve_order_and_deliver_one_error() {
        let script = Script::new();
        script.answer(200).body("ab");
        script.answer(200).body("");
        script.answer(200).body("c");
        let mut items = pages(
            List::new(&script),
            |mut call, _| -> Result<Option<List>, Fail<String>> {
                if call.at == 2 {
                    Err(crate::mismatch("missing cursor"))
                } else {
                    call.at += 1;
                    Ok(Some(call))
                }
            },
        )
        .items();
        assert_eq!(items.try_next().await.unwrap(), Some(97));
        assert_eq!(script.requests().len(), 1);
        assert_eq!(items.next().await.unwrap().unwrap(), 98);
        assert_eq!(script.requests().len(), 1);
        assert_eq!(items.try_next().await.unwrap(), Some(99));
        assert_eq!(
            items.next().await.unwrap().unwrap_err().kind(),
            Kind::Mismatch
        );
        assert!(items.try_next().await.unwrap().is_none());
        assert!(items.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn c8_4_middleware_changes_do_not_make_a_repeated_request_progress() {
        struct Change(AtomicUsize);
        impl crate::Middleware for Change {
            async fn handle(
                &self,
                mut request: reqwest::Request,
                info: TryInfo,
                next: crate::Next<'_>,
            ) -> Result<reqwest::Response, TransportError> {
                request
                    .url_mut()
                    .set_query(Some(&self.0.fetch_add(1, Ordering::SeqCst).to_string()));
                next.send(request, info).await
            }
        }
        let script = Script::new();
        script.answer(200).body("a");
        let mut call = List::new(&script);
        call.core = call.core.with_middleware(Change(AtomicUsize::new(0)));
        let mut pages = pages(call, |call, _| Some(call));
        assert!(pages.try_next().await.unwrap().is_some());
        assert_eq!(pages.try_next().await.unwrap_err().kind(), Kind::Mismatch);
        assert_eq!(script.requests().len(), 1);
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_5_each_page_uses_its_retry_policy_and_preserves_successful_try_count() {
        let script = Script::new();
        script.answer(503);
        script.answer(200).body("a");
        script.answer(503);
        script.answer(200).body("b");
        let mut call = List::new(&script);
        call.options
            .with_retry(crate::Retry::none().tries(2).when(|_| Some(Duration::ZERO)));
        let mut pages = pages(call, |mut call, _| -> Result<Option<List>, Fail<String>> {
            if call.at == 1 {
                Err(crate::mismatch("bad next"))
            } else {
                call.at += 1;
                Ok(Some(call))
            }
        });
        assert_eq!(pages.try_next().await.unwrap(), Some(vec![97]));
        assert_eq!(pages.try_next().await.unwrap(), Some(vec![98]));
        assert_eq!(pages.try_next().await.unwrap_err().tries(), 2);
        assert_eq!(
            script
                .requests()
                .iter()
                .map(|r| r.info().number())
                .collect::<Vec<_>>(),
            [1, 2, 1, 2]
        );
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_5_a_page_already_ready_is_given_after_an_unpolled_deadline() {
        let script = Script::new();
        script.answer(200).after(Duration::from_secs(1)).body("a");
        let mut call = List::new(&script);
        call.options.with_timeout(Duration::from_secs(2));
        let mut pages = pages(call, |_, _| None);
        assert!(pages.next().now_or_never().is_none());
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(pages.try_next().await.unwrap(), Some(vec![97]));
        assert_eq!(script.requests().len(), 1);
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_5_an_expired_preparation_is_not_sent_and_a_call_error_ends_the_pages() {
        let script = Script::new();
        let mut call = List::new(&script);
        call.options.with_timeout(Duration::ZERO);
        let mut pages = pages(call, next);
        assert!(pages.try_next().await.unwrap_err().is_timeout());
        assert!(script.requests().is_empty());
        over(&mut pages).await;
        let script = Script::new();
        script.answer(403).body("refusal");
        let mut pages = super::pages(List::new(&script), next);
        assert_eq!(pages.try_next().await.unwrap_err().kind(), Kind::Refused);
        assert_eq!(script.requests().len(), 1);
        over(&mut pages).await;
    }

    #[tokio::test(start_paused = true)]
    async fn c8_7_dropped_item_future_resumes_without_requiring_sync_items() {
        let script = Script::new();
        script.answer(200).after(Duration::from_secs(1)).body("ab");
        let pages = pages(List::new(&script), |_, _| None);
        let mut items = pages.items();
        assert!(items.try_next().now_or_never().is_none());
        assert_eq!(items.next().await.unwrap().unwrap(), 97);
        assert_eq!(items.try_next().await.unwrap(), Some(98));
        assert!(items.try_next().await.unwrap().is_none());
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Pages<std::cell::Cell<u32>, String>>();
        send_sync::<crate::Items<std::cell::Cell<u32>, String>>();
        assert_eq!(script.requests().len(), 1);
    }
}
