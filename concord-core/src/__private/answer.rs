use std::fmt;
use std::future::Future;

use crate::answer::{Answer, Headers, KnownKind, KnownOutcome};
use crate::error::{Error, Fail, Kept, MismatchFailure};
use crate::read::{Incoming, Reader, ReaderFor};
use crate::transport::TryInfo;

/// The response evidence retained for an answer or a status arm.
pub fn answer_head(response: &reqwest::Response) -> (reqwest::Url, http::StatusCode, Headers) {
    (
        response.url().clone(),
        response.status(),
        Headers::from(response.headers().clone()),
    )
}

/// Finish a successful response using the evidence captured before reading it.
pub fn frame_answer<T, P>(
    url: reqwest::Url,
    status: http::StatusCode,
    headers: Headers,
    value: T,
) -> Result<Answer<T>, Error<P>> {
    Ok(Answer::new(status, headers, value).with_response_url(url))
}

/// The fallback for an endpoint without custom status arms.
pub trait Refusal<P>: Send {
    /// Read the refusal, creating any problem reader on this branch only.
    fn refuse(
        self,
        response: reqwest::Response,
        info: TryInfo,
    ) -> impl Future<Output = Error<P>> + Send;
}

/// Refuse a response without a typed problem.
#[derive(Debug)]
pub struct PlainRefusal;

impl<P: Send> Refusal<P> for PlainRefusal {
    async fn refuse(self, response: reqwest::Response, info: TryInfo) -> Error<P> {
        refused_plain(response, &info).await
    }
}

/// Refuse a response using a lazily constructed reader.
pub struct ProblemRefusal<F>(pub F);

impl<F> fmt::Debug for ProblemRefusal<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProblemRefusal").finish_non_exhaustive()
    }
}

impl<P: Send, R: Reader<P>, F: FnOnce() -> R + Send> Refusal<P> for ProblemRefusal<F> {
    async fn refuse(self, response: reqwest::Response, info: TryInfo) -> Error<P> {
        refused(&(self.0)(), response, &info).await
    }
}

/// Read and frame an ordinary successful response, or read the refusal.
pub async fn answer_read<T: Send, U, P: Send, R: ReaderFor<T, P>>(
    response: reqwest::Response,
    info: TryInfo,
    reader: impl FnOnce() -> R + Send,
    wrap: impl FnOnce(T) -> U + Send,
    refusal: impl Refusal<P>,
) -> Result<Answer<U>, Error<P>> {
    if !response.status().is_success() {
        return Err(refusal.refuse(response, info).await);
    }
    let (url, status, headers) = answer_head(&response);
    let value = read_body(&reader(), response, &info).await?;
    frame_answer(url, status, headers, wrap(value))
}

/// Frame a successful response from an endpoint that ignores the body.
pub async fn answer_empty<P: Send>(
    response: reqwest::Response,
    info: TryInfo,
    refusal: impl Refusal<P>,
) -> Result<Answer<()>, Error<P>> {
    if !response.status().is_success() {
        return Err(refusal.refuse(response, info).await);
    }
    let (url, status, headers) = answer_head(&response);
    frame_answer(url, status, headers, ())
}

/// Checks a reader at the expression that named it, inside a plain closure.
///
/// ```compile_fail
/// # async fn check<R: concord_core::Reader<String>>(value: Result<R, ()>) -> Result<(), ()> {
/// let _ = (|| concord_core::__private::reader::<String, _>(value?))();
/// # Ok(()) }
/// ```
///
/// ```compile_fail
/// # async fn check<R: concord_core::Reader<String>>(value: R) {
/// let _ = (|| concord_core::__private::reader::<String, _>(async { value }.await))();
/// # }
/// ```
pub fn reader<T, R: Reader<T>>(reader: R) -> CheckedReader<T, R> {
    CheckedReader(reader, std::marker::PhantomData)
}

/// Check the reader at its declaration and hide its concrete type from the response framing.
#[inline]
///
/// The client's problem type `P` is inferred from the call that reads the response, so that a
/// reader whose value holds the client's error is checked against the client's problem type.
pub fn reader_for_response<T, P>(value: impl ReaderFor<T, P>) -> impl ReaderFor<T, P> {
    value
}

/// Check the reader for the type declared in a problem statement.
#[inline]
pub fn reader_for_problem<T, R: Reader<T>>(value: R) -> impl Reader<T> {
    reader(value)
}

/// Carries the read type through inference, and cannot implement `Try`.
///
/// ```compile_fail
/// use concord_core::read::{Incoming, ReadError, Reader};
/// struct Local;
/// impl Reader<Local> for Result<(), ()> {
///     async fn read(&self, _: Incoming) -> Result<Local, ReadError> { Ok(Local) }
/// }
/// fn check(value: Result<Result<(), ()>, ()>) {
///     let _ = (|| concord_core::__private::reader::<Local, _>(value?))();
/// }
/// ```
pub struct CheckedReader<T, R>(R, std::marker::PhantomData<fn() -> T>);
impl<T, R> fmt::Debug for CheckedReader<T, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CheckedReader(..)")
    }
}
impl<T, R: Reader<T>> Reader<T> for CheckedReader<T, R> {
    fn read(
        &self,
        body: crate::read::Incoming,
    ) -> impl Future<Output = Result<T, crate::read::ReadError>> + Send {
        self.0.read(body)
    }
}

/// Reads a success body or an arm's bound body. A reader failure on a non-2xx
/// is Refused with no typed problem; on a 2xx it keeps its ordinary kind.
/// Failures returned by the arm expression itself still go through [`arm`].
// PANIC-SAFETY: The explicit returned future states the hidden/public Send contract; no panic lint is relaxed.
#[allow(clippy::manual_async_fn)] // The hidden interface explicitly promises Send.
pub fn read_body<'a, T, P, R>(
    reader: &'a R,
    response: impl Into<Incoming> + Send + 'a,
    info: &'a TryInfo,
) -> impl Future<Output = Result<T, Error<P>>> + Send + 'a
where
    R: ReaderFor<T, P>,
    T: Send + 'a,
    P: Send + 'a,
{
    async move {
        let mut incoming = response.into().with_info(info);
        let status = incoming.status();
        let headers = incoming.headers().clone();
        let clock = incoming.clock();
        let evidence = (!status.is_success()).then(|| incoming.keep_refusal());
        match reader.read_for(incoming).await {
            Ok(value) => Ok(value),
            Err(error) => Err(match evidence {
                Some(evidence) => {
                    crate::read::finish_refusal(&evidence, status, &headers, info, clock).await;
                    error.into_refused(
                        status,
                        headers,
                        info,
                        &evidence
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner),
                    )
                }
                None => error.into_error(status, headers, info),
            }),
        }
    }
}

/// The Refused error for a status that no arm takes. The problem is tried from the body with its reader.
// PANIC-SAFETY: The explicit returned future states the hidden/public Send contract; no panic lint is relaxed.
#[allow(clippy::manual_async_fn)] // The hidden interface explicitly promises Send.
pub fn refused<'a, P, R>(
    problem: &'a R,
    response: impl Into<Incoming> + Send + 'a,
    info: &'a TryInfo,
) -> impl Future<Output = Error<P>> + Send + 'a
where
    R: Reader<P>,
    P: Send + 'a,
{
    async move {
        let incoming = response.into().with_info(info);
        if info.unclaimed_conditional(incoming.status()) {
            return Error::refused(
                info.endpoint(),
                info.number(),
                incoming.status(),
                incoming.headers().clone(),
                None,
            );
        }
        let (status, headers, body, kept) = evidence(incoming, KnownKind::Refused).await;
        let decoded = if kept == Kept::Complete {
            let mut response = http::Response::new(body.clone());
            *response.status_mut() = status;
            *response.headers_mut() = headers.as_map().clone();
            problem
                .read(Incoming::new(response.into()).with_info(info))
                .await
                .ok()
        } else {
            None
        };
        Error::refused(info.endpoint(), info.number(), status, headers, decoded)
            .with_body(&body, kept)
    }
}

/// The Refused error of a client with no `problem` line.
// PANIC-SAFETY: The explicit returned future states the hidden/public Send contract; no panic lint is relaxed.
#[allow(clippy::manual_async_fn)] // The hidden interface explicitly promises Send.
pub fn refused_plain<'a, P: Send + 'a>(
    response: impl Into<Incoming> + Send + 'a,
    info: &'a TryInfo,
) -> impl Future<Output = Error<P>> + Send + 'a {
    async move {
        let incoming = response.into().with_info(info);
        if info.unclaimed_conditional(incoming.status()) {
            return Error::refused(
                info.endpoint(),
                info.number(),
                incoming.status(),
                incoming.headers().clone(),
                None,
            );
        }
        let (status, headers, body, kept) = evidence(incoming, KnownKind::Refused).await;
        Error::refused(info.endpoint(), info.number(), status, headers, None).with_body(&body, kept)
    }
}

/// A successful status that no arm takes, when the call has no simple success rule.
/// Keeps status, headers and at most 64 KiB of the body in a Mismatch.
// PANIC-SAFETY: The explicit returned future states the hidden/public Send contract; no panic lint is relaxed.
#[allow(clippy::manual_async_fn)] // The hidden interface explicitly promises Send.
pub fn no_arm<'a, P: Send + 'a>(
    response: impl Into<Incoming> + Send + 'a,
    info: &'a TryInfo,
) -> impl Future<Output = Error<P>> + Send + 'a {
    async move {
        let (status, headers, body, kept) =
            evidence(response.into().with_info(info), KnownKind::NoArm).await;
        Error::mismatch(
            info.endpoint(),
            info.number(),
            status,
            headers,
            MismatchFailure::NoArm,
        )
        .with_body(&body, kept)
    }
}

/// What an arm gave, as the result of the try.
pub fn arm<A, P>(
    result: Result<A, Fail<P>>,
    status: http::StatusCode,
    headers: &Headers,
    info: &TryInfo,
) -> Result<A, Error<P>> {
    result.map_err(|failure| {
        failure.into_error(
            info.endpoint(),
            info.number(),
            status,
            headers.clone(),
            false,
        )
    })
}

// The known result is recorded before the first body poll, then after each piece.
// A total-clock cancellation can therefore recover it without polling this future again.
async fn evidence(
    incoming: Incoming,
    kind: KnownKind,
) -> (http::StatusCode, Headers, bytes::Bytes, Kept) {
    let status = incoming.status();
    let headers = incoming.headers().clone();
    let clock = incoming.clock();
    let mut body = bytes::BytesMut::new();
    let record = |body: &[u8], kept| {
        if let Some(clock) = &clock {
            clock.record(KnownOutcome {
                kind,
                status,
                headers: &headers,
                body,
                kept,
            });
        }
    };
    record(&body, Kept::Interrupted);
    let mut stream = incoming.stream();
    let kept = loop {
        match stream.try_next().await {
            Ok(Some(piece)) => {
                // PANIC-SAFETY: 64 KiB is representable on the supported 32/64-bit targets; subtraction is saturating.
                let available = (64 * crate::KIB).saturating_sub(body.len());
                body.extend_from_slice(piece.get(..piece.len().min(available)).unwrap_or_default());
                if piece.len() > available {
                    break Kept::Cut;
                }
                record(&body, Kept::Interrupted);
            }
            Ok(None) => break Kept::Complete,
            Err(_) => break Kept::Interrupted,
        }
    };
    record(&body, kept);
    (status, headers, body.freeze(), kept)
}

pub use crate::read::details as cut_details;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c5_6_short_details_does_not_retain_unbounded_allocation() {
        let mut text = String::with_capacity(7_000_000);
        text.push_str("small");
        let text = cut_details(text);
        assert_eq!(text, "small");
        assert!(text.capacity() < 2048, "retained {} bytes", text.capacity());
    }
    #[test]
    fn c5_6_details_are_cut_once_even_at_a_multibyte_boundary() {
        for lead in 1018..=1026 {
            let text = "a".repeat(lead) + "🦀" + &"b".repeat(100);
            let once = cut_details(text);
            assert_eq!(once.matches("… [cut]").count(), 1);
            assert_eq!(cut_details(once.clone()), once);
        }
    }

    #[test]
    fn c5_6_details_and_their_marker_fit_together_in_one_kib() {
        for text in ["a".repeat(1025), "🦀".repeat(1000)] {
            let kept = cut_details(text);
            assert!(kept.len() <= 1024);
            assert!(kept.ends_with(" … [cut]"));
            assert!(1024 - kept.len() < 4);
            assert_eq!(cut_details(kept.clone()), kept);
        }
    }
}

#[cfg(test)]
mod c5_tests {
    use super::*;
    use crate::error::Kind;
    use crate::read::{ByteStream, Place, ReadError};
    use futures_util::FutureExt;
    use std::error::Error as _;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct Binary(usize);
    impl Reader<bytes::Bytes> for Binary {
        async fn read(&self, incoming: Incoming) -> Result<bytes::Bytes, ReadError> {
            assert_eq!(incoming.status(), http::StatusCode::OK);
            incoming.bytes(self.0).await
        }
    }
    struct Stream;
    impl Reader<ByteStream> for Stream {
        async fn read(&self, incoming: Incoming) -> Result<ByteStream, ReadError> {
            Ok(incoming.stream())
        }
    }
    struct NotTheType;
    impl Reader<()> for NotTheType {
        async fn read(&self, incoming: Incoming) -> Result<(), ReadError> {
            let body = incoming.bytes(100 * crate::KIB).await?;
            Err(ReadError::not_the_type(body)
                .at(Place::root()
                    .field("items")
                    .key()
                    .variant()
                    .index(3)
                    .field("kind"))
                .with_details("PRIVATE_SENTINEL".repeat(1000))
                .label_was_not_expected())
        }
    }
    fn ready<F: Future>(future: F) -> F::Output {
        future
            .now_or_never()
            .expect("an in-memory response never waits")
    }
    fn info() -> TryInfo {
        TryInfo::new("tasks.get", 3)
    }

    #[test]
    fn c5_4_exact_limit_and_empty_body_are_values() {
        for body in ["", "abc"] {
            let value: Result<_, Error> = ready(read_body(
                &Binary(body.len()),
                crate::reply(200, body),
                &info(),
            ));
            assert_eq!(value.unwrap(), body);
        }
    }

    #[test]
    fn c5_11_a_reader_gets_the_status_headers_and_content_type() {
        struct Head;
        impl Reader<()> for Head {
            async fn read(&self, incoming: Incoming) -> Result<(), ReadError> {
                assert_eq!(incoming.status(), http::StatusCode::CREATED);
                assert_eq!(incoming.headers().get("x-info"), Some("yes"));
                assert_eq!(incoming.content_type(), Some("application/custom"));
                Ok(())
            }
        }
        let response = crate::reply_with(
            201,
            &[("x-info", "yes"), ("content-type", "application/custom")],
            "ignored",
        );
        let value: Result<(), Error> = ready(read_body(&Head, response, &info()));
        value.unwrap();
    }

    #[test]
    /// Also proves C5.11.
    fn c5_6_decode_keeps_only_bounded_evidence_and_structured_causes() {
        let error: Error = ready(read_body(
            &NotTheType,
            crate::reply(200, "PRIVATE_SENTINEL".repeat(5000)),
            &info(),
        ))
        .unwrap_err();
        assert_eq!(error.reason(), None);
        assert_eq!(error.kind(), Kind::Mismatch);
        assert_eq!(error.body().unwrap().len(), 64 * crate::KIB);
        assert_eq!(error.kept(), Some(Kept::Cut));
        assert!(error.problem().is_none());
        assert!(error.to_string().contains("items[key][variant][3].kind"));
        assert!(!error.to_string().contains("content type"));
        assert!(error.details().to_string().contains("PRIVATE_SENTINEL"));
        let mut cause: Option<&dyn std::error::Error> = Some(&error);
        while let Some(source) = cause {
            assert!(!format!("{source} {source:?} {source:#?}").contains("PRIVATE_SENTINEL"));
            cause = source.source();
        }
    }

    #[test]
    fn c5_11_wrong_content_type_keeps_no_body_and_no_reason() {
        struct Strict;
        impl Reader<()> for Strict {
            async fn read(&self, _: Incoming) -> Result<(), ReadError> {
                Err(ReadError::wrong_content_type())
            }
        }
        let error: Error =
            ready(read_body(&Strict, crate::reply(200, "untouched"), &info())).unwrap_err();
        assert_eq!(error.kept(), None);
        assert_eq!(error.body(), None);
        assert_eq!(error.reason(), None);
        assert!(error.to_string().contains("content type is not the one"));
        assert!(error.source().is_some());
    }

    #[test]
    fn c5_8_arm_problem_and_mismatch_keep_the_head_without_bytes() {
        let status = http::StatusCode::OK;
        let headers = Headers::from(
            crate::reply_with(200, &[("x-id", "one")], "")
                .headers()
                .clone(),
        );
        let error =
            arm::<(), _>(Err(Fail::from("problem")), status, &headers, &info()).unwrap_err();
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.problem(), Some(&"problem"));
        assert_eq!(error.status(), Some(status));
        assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
        assert_eq!(error.body(), None);
        assert_eq!(error.kept(), None);
        let error: Error = arm::<(), _>(
            Err(crate::mismatch("PRIVATE_SENTINEL")),
            status,
            &headers,
            &info(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), Kind::Mismatch);
        assert_eq!(error.reason(), Some("PRIVATE_SENTINEL"));
        assert_eq!(error.source().map(ToString::to_string), None);
        assert!(!error.to_string().contains("PRIVATE_SENTINEL"));
        assert_eq!(arm::<_, ()>(Ok(7), status, &headers, &info()).unwrap(), 7);
    }

    #[test]
    fn c5_8_a_read_error_of_another_response_brings_none_of_its_context() {
        fn broken(status: u16) -> reqwest::Response {
            let body = futures_util::stream::iter([Err::<bytes::Bytes, _>(std::io::Error::other(
                "PRIVATE_SENTINEL",
            ))]);
            crate::reply_with(
                status,
                &[("x-other", "yes")],
                reqwest::Body::wrap_stream(body),
            )
        }
        // A whole body that broke off, and a stream whose error a reader forwarded with `?`.
        let whole = || ready(Incoming::new(broken(418)).bytes(64)).unwrap_err();
        let forwarded = || {
            ready(async {
                let mut stream = Incoming::new(broken(418)).stream();
                stream.try_next().await.map(|_| ()).map_err(ReadError::from)
            })
            .unwrap_err()
        };
        let headers = Headers::from(
            crate::reply_with(200, &[("x-id", "one")], "")
                .headers()
                .clone(),
        );
        for (status, next_page, text) in [
            (200, false, "mismatch: status 200, the status arm rejected the response, in tasks.get, after 3 attempts"),
            (409, false, "mismatch: status 409, the status arm rejected the response, in tasks.get, after 3 attempts"),
            (200, true, "mismatch: status 200, the next page cannot be requested, in tasks.get, after 3 attempts"),
        ] {
            for failure in [whole(), forwarded()] {
                let error: Error = Fail::from_read(failure).into_error(
                    "tasks.get",
                    3,
                    http::StatusCode::from_u16(status).unwrap(),
                    headers.clone(),
                    next_page,
                );
                assert_eq!(error.kind(), Kind::Mismatch);
                assert_eq!(error.to_string(), text);
                assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
                assert_eq!(error.headers().unwrap().get("x-other"), None);
                assert_eq!(error.reason(), None);
                assert_eq!(error.body(), None);
                assert!(error.source().is_none());
            }
        }
        // Through the arm of a call, the same.
        let error: Error = arm::<(), _>(
            Err(Fail::from_read(whole())),
            http::StatusCode::OK,
            &headers,
            &info(),
        )
        .unwrap_err();
        assert_eq!(error.endpoint(), "tasks.get");
        assert_eq!(error.tries(), 3);
        assert!(error.is_status(200));
    }

    #[test]
    fn c5_8_a_decoder_error_returned_by_an_arm_is_the_error_of_the_reader() {
        for status in [200, 409] {
            let response = || crate::reply_with(status, &[("x-id", "one")], "PRIVATE_SENTINEL");
            let headers = Headers::from(response().headers().clone());
            let from_reader: Error =
                ready(read_body(&NotTheType, response(), &info())).unwrap_err();
            let failure = ready(NotTheType.read(Incoming::new(response()))).unwrap_err();
            let from_arm: Error = arm::<(), _>(
                Err(Fail::from_read(failure)),
                http::StatusCode::from_u16(status).unwrap(),
                &headers,
                &info(),
            )
            .unwrap_err();
            assert_eq!(from_arm.kind(), from_reader.kind());
            assert_eq!(from_arm.to_string(), from_reader.to_string());
            assert_eq!(
                from_arm.details().to_string(),
                from_reader
                    .details()
                    .to_string()
                    .replace("\nbody (", "\ndecoder input (")
            );
            assert_eq!(from_arm.body(), Some(b"PRIVATE_SENTINEL".as_slice()));
            assert_eq!(from_arm.kept(), Some(Kept::Complete));
            assert_eq!(from_arm.reason(), None);
            assert_eq!(from_arm.tries(), 3);
            assert_eq!(
                from_arm.source().map(ToString::to_string),
                from_reader.source().map(ToString::to_string)
            );
        }
    }

    #[test]
    fn c5_6_problem_is_tried_only_on_complete_refusals_and_failure_is_ignored() {
        struct Problem;
        impl Reader<String> for Problem {
            async fn read(&self, body: Incoming) -> Result<String, ReadError> {
                assert_eq!(body.status(), http::StatusCode::BAD_REQUEST);
                assert_eq!(body.headers().get("x-id"), Some("one"));
                assert_eq!(body.content_type(), Some("text/plain"));
                Ok(String::from_utf8(body.bytes(64 * crate::KIB).await?.to_vec()).unwrap())
            }
        }
        let response = crate::reply_with(
            400,
            &[("x-id", "one"), ("content-type", "text/plain")],
            "problem",
        );
        let error = ready(refused(&Problem, response, &info()));
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.problem().map(String::as_str), Some("problem"));
        assert_eq!(error.body(), Some(b"problem".as_slice()));
        assert_eq!(error.kept(), Some(Kept::Complete));
        let error = ready(refused(&NotTheType, crate::reply(400, "bad"), &info()));
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.problem(), None);
        assert_eq!(error.kept(), Some(Kept::Complete));
        assert!(error.source().is_none());
    }

    #[test]
    fn c5_2_no_arm_is_a_mismatch_and_keeps_refusal_evidence() {
        let error: Error = ready(no_arm(
            crate::reply_with(204, &[("x-id", "one")], ""),
            &info(),
        ));
        assert_eq!(error.kind(), Kind::Mismatch);
        assert_eq!(error.kept(), Some(Kept::Complete));
        assert_eq!(error.body(), Some(b"".as_slice()));
        assert!(error.to_string().contains("no arm matches this status"));
        assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
        assert!(error.problem().is_none());
    }

    #[test]
    fn c5_10_stream_success_ends_at_the_head_then_yields_the_bytes() {
        let polls = Arc::new(AtomicUsize::new(0));
        let counted = polls.clone();
        let body = futures_util::stream::once(async move {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"ok"))
        });
        let response: reqwest::Response =
            http::Response::new(reqwest::Body::wrap_stream(body)).into();
        let mut stream = ready(read_body::<_, (), _>(&Stream, response, &info())).unwrap();
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert_eq!(stream.status(), http::StatusCode::OK);
        assert_eq!(ready(stream.try_next()).unwrap().unwrap(), "ok");
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(ready(stream.try_next()).unwrap().is_none());
        assert!(ready(stream.try_next()).unwrap().is_none());
    }

    #[test]
    fn c3_10_read_response_has_zero_tries_and_no_clock() {
        let error: Error = ready(read_body(
            &NotTheType,
            crate::reply(200, "bad"),
            &TryInfo::new("tasks.get", 0),
        ))
        .unwrap_err();
        assert_eq!(error.tries(), 0);
        assert_eq!(error.endpoint(), "tasks.get");
    }
}
