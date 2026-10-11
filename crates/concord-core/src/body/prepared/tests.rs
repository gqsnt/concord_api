use std::collections::VecDeque;
use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use super::*;
use crate::limits::{MAX_EMPTY_REQUEST_CHUNKS, MAX_EMPTY_REQUEST_CHUNKS_PER_POLL};
use http_body::Body as _;

struct Factory {
    calls: Arc<AtomicUsize>,
}

struct EmptyChunkFactory {
    polls: Arc<AtomicUsize>,
}

impl BodyFactory for EmptyChunkFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        Ok(stream_bytes(
            [
                Ok(Bytes::new()),
                Ok(Bytes::from_static(b"factory")),
                Ok(Bytes::new()),
            ],
            &self.polls,
        )
        .exact_length(7))
    }
}

#[derive(Default)]
struct WakeCounter {
    wakes: AtomicUsize,
}

impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(feature = "multipart")]
struct MultipartFactoryProbe {
    calls: Arc<AtomicUsize>,
}

impl BodyFactory for Factory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(RequestStream::from_bytes(Bytes::from_static(b"factory")))
    }
}

#[cfg(feature = "multipart")]
impl MultipartFactory for MultipartFactoryProbe {
    fn create(&self) -> Result<MultipartBody, BodyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        MultipartBody::new().part(
            "factory-field",
            crate::body::MultipartPart::text("factory-value")?,
        )
    }
}

fn limit(bytes: usize) -> NonZeroUsize {
    NonZeroUsize::new(bytes).expect("positive limit")
}

#[test]
fn prepared_body_kinds_lengths_and_replayability_are_core_facts() {
    let empty = PreparedBody::empty();
    assert_eq!(empty.kind(), BodyKind::Empty);
    assert_eq!(empty.length_hint(), LengthHint::Exact(0));
    assert_eq!(empty.media_type(), None);
    assert!(empty.is_replayable());
    assert_eq!(
        empty.into_managed(limit(64)).unwrap().replayability(),
        Replayability::Reusable
    );

    let bytes = PreparedBody::bytes(Bytes::from_static(b"bytes"));
    assert_eq!(bytes.kind(), BodyKind::Buffered);
    assert_eq!(bytes.length_hint(), LengthHint::Exact(5));
    assert!(bytes.is_replayable());
    assert_eq!(
        bytes.into_managed(limit(64)).unwrap().replayability(),
        Replayability::Reusable
    );

    let stream = PreparedBody::stream(RequestStream::from_bytes(Bytes::from_static(b"bytes")));
    assert_eq!(stream.kind(), BodyKind::Stream);
    assert_eq!(stream.length_hint(), LengthHint::Exact(5));
    assert!(!stream.is_replayable());
    assert_eq!(
        stream.into_managed(limit(64)).unwrap().replayability(),
        Replayability::OneShot
    );

    let minimum = PreparedBody::stream(
        RequestStream::from_stream(FrameStream {
            frames: VecDeque::new(),
            polls: Arc::new(AtomicUsize::new(0)),
        })
        .at_least(3),
    );
    assert_eq!(minimum.length_hint(), LengthHint::AtLeast(3));

    let calls = Arc::new(AtomicUsize::new(0));
    let factory = PreparedBody::factory(Factory {
        calls: Arc::clone(&calls),
    })
    .content_type("application/x-factory")
    .expect("factory media type");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory.length_hint(), LengthHint::Unknown);
    assert_eq!(factory.media_type(), Some("application/x-factory"));
    assert!(factory.is_replayable());
    let managed = factory.into_managed(limit(64)).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(managed.replayability(), Replayability::Reconstructible);
    assert_eq!(
        managed.content_type(),
        Some(&HeaderValue::from_static("application/x-factory"))
    );
}

#[test]
fn content_type_is_validated_once_and_never_overwritten() {
    let body = PreparedBody::bytes(Bytes::from_static(b"x"))
        .content_type("application/x-override; version=1")
        .expect("valid media type");
    assert_eq!(body.media_type(), Some("application/x-override; version=1"));
    let body = body
        .content_type("application/x-override; version=1")
        .expect("identical repeat is harmless");
    let error = body
        .content_type("application/x-other")
        .expect_err("conflicting overwrite");
    assert_eq!(error.kind(), BodyErrorKind::InvalidContentType);

    for invalid in [
        "not a media type",
        "*/*",
        "application/*",
        "text/plain\r\nx-injected: yes",
        "",
    ] {
        assert_eq!(
            PreparedBody::empty()
                .content_type(invalid)
                .expect_err("invalid media")
                .kind(),
            BodyErrorKind::InvalidContentType
        );
    }
    let exact = format!(
        "application/{}",
        "a".repeat(MAX_REQUEST_CONTENT_TYPE_BYTES - "application/".len())
    );
    PreparedBody::empty()
        .content_type(&exact)
        .expect("exact content-type limit");
    let excessive = format!("{exact}a");
    assert_eq!(
        PreparedBody::empty()
            .content_type(&excessive)
            .expect_err("content-type first excess")
            .kind(),
        BodyErrorKind::InvalidContentType
    );
}

#[test]
fn known_lengths_are_checked_against_the_limit_before_materialization() {
    let error = PreparedBody::bytes(Bytes::from_static(b"12345"))
        .into_managed(limit(4))
        .err()
        .expect("exact length over limit");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);

    let polls = Arc::new(AtomicUsize::new(0));
    let error = PreparedBody::stream(
        RequestStream::from_stream(FrameStream {
            frames: VecDeque::new(),
            polls: Arc::clone(&polls),
        })
        .at_least(10),
    )
    .into_managed(limit(4))
    .err()
    .expect("minimum over limit");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert_eq!(polls.load(Ordering::SeqCst), 0, "no producer polling");

    let error = PreparedBody::stream(
        RequestStream::from_bytes(Bytes::from_static(b"four")).exact_length(3),
    )
    .into_managed(limit(64))
    .err()
    .expect("conflicting exact length");
    assert_eq!(error.kind(), BodyErrorKind::InvalidLength);

    let error =
        PreparedBody::stream(RequestStream::from_bytes(Bytes::from_static(b"four")).at_least(5))
            .into_managed(limit(64))
            .err()
            .expect("minimum above exact length");
    assert_eq!(error.kind(), BodyErrorKind::InvalidLength);
}

#[test]
fn debug_is_structural_and_omits_body_values() {
    let body = PreparedBody::bytes(Bytes::from_static(b"prepared-body-secret-3c1f"))
        .content_type("application/x-secret-media")
        .expect("media type");
    let debug = format!("{body:?}");
    assert!(!debug.contains("prepared-body-secret-3c1f"));
    assert!(!debug.contains("application/x-secret-media"));
    assert!(debug.contains("Buffered"));
}

#[cfg(feature = "multipart")]
#[test]
fn multipart_boundary_is_generated_once_and_replays_are_mechanical() {
    let before = crate::body::generated_boundaries_on_this_thread();
    let reusable = PreparedBody::multipart(
        MultipartBody::new()
            .part(
                "text",
                crate::body::MultipartPart::text("multipart-secret-text")
                    .expect("bounded multipart text"),
            )
            .expect("text part")
            .part(
                "bytes",
                crate::body::MultipartPart::bytes(Bytes::from_static(b"multipart-secret-bytes"))
                    .expect("bounded multipart bytes"),
            )
            .expect("bytes part"),
    )
    .expect("prepared multipart");
    assert_eq!(
        crate::body::generated_boundaries_on_this_thread(),
        before + 1
    );
    assert_eq!(reusable.kind(), BodyKind::Multipart);
    assert!(reusable.is_replayable());
    let media = reusable
        .media_type()
        .expect("generated media type")
        .to_owned();
    assert!(media.starts_with("multipart/form-data; boundary=concord-"));
    assert!(matches!(reusable.length_hint(), LengthHint::Exact(_)));
    assert!(!format!("{reusable:?}").contains("multipart-secret-text"));
    let reusable = reusable
        .content_type(&media)
        .expect("identical generated media type is harmless");
    let error = PreparedBody::multipart(MultipartBody::new())
        .expect("empty multipart")
        .content_type("multipart/form-data; boundary=application-owned")
        .expect_err("conflicting multipart media type");
    assert_eq!(error.kind(), BodyErrorKind::InvalidContentType);

    let generated = crate::body::generated_boundaries_on_this_thread();
    let mut managed = reusable
        .into_managed(limit(4096))
        .expect("reusable multipart materialization");
    assert_eq!(managed.replayability(), Replayability::Reusable);
    assert_eq!(
        managed.content_type().and_then(|value| value.to_str().ok()),
        Some(media.as_str())
    );
    let _ = managed.prepare_native(limit(4096)).expect("first attempt");
    managed
        .reconstruct_for_next_attempt(limit(4096))
        .expect("reusable next attempt");
    let _ = managed.prepare_native(limit(4096)).expect("second attempt");
    assert_eq!(
        crate::body::generated_boundaries_on_this_thread(),
        generated,
        "replays reuse the retained boundary"
    );

    let one_shot = PreparedBody::multipart(
        MultipartBody::new()
            .part(
                "stream",
                crate::body::MultipartPart::stream(RequestStream::from_bytes(Bytes::from_static(
                    b"multipart-stream-secret",
                ))),
            )
            .expect("stream part"),
    )
    .expect("prepared one-shot multipart");
    assert!(!one_shot.is_replayable());
    let mut one_shot = one_shot
        .into_managed(limit(4096))
        .expect("one-shot multipart materialization");
    assert_eq!(one_shot.replayability(), Replayability::OneShot);
    assert!(one_shot.reconstruct_for_next_attempt(limit(4096)).is_err());

    let calls = Arc::new(AtomicUsize::new(0));
    let factory = PreparedBody::multipart_factory(MultipartFactoryProbe {
        calls: Arc::clone(&calls),
    })
    .expect("multipart factory");
    let factory_media = factory.media_type().expect("factory media").to_owned();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let mut prepared = factory
        .into_managed(limit(4096))
        .expect("multipart factory materialization");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepared.replayability(), Replayability::Reconstructible);
    let _ = prepared
        .prepare_native(limit(4096))
        .expect("current multipart attempt");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    prepared
        .reconstruct_for_next_attempt(limit(4096))
        .expect("next multipart attempt");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        prepared
            .content_type()
            .and_then(|value| value.to_str().ok()),
        Some(factory_media.as_str())
    );
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_serialization_uses_the_managed_aggregate_counter() {
    let polls = Arc::new(AtomicUsize::new(0));
    let body = PreparedBody::multipart(
        MultipartBody::new()
            .part(
                "field",
                crate::body::MultipartPart::stream(stream([Ok(&b"x"[..])], &polls)),
            )
            .expect("multipart recipe"),
    )
    .expect("prepared multipart");
    let LengthHint::AtLeast(minimum) = body.length_hint() else {
        panic!("a streaming part has an unknown exact length")
    };
    // The one-byte payload fits, but the framing crosses a limit just below
    // the framed minimum; the managed stream fails at the first excess.
    let small = limit(usize::try_from(minimum).expect("minimum") - 1);
    let mut managed = PreparedBody::multipart(
        MultipartBody::new()
            .part(
                "field",
                crate::body::MultipartPart::stream(stream([Ok(&b"x"[..])], &polls)),
            )
            .expect("multipart recipe"),
    )
    .expect("prepared multipart")
    .into_managed(limit(4096))
    .expect("materialized multipart");
    let NativeBody::Stream { body, failure, .. } = managed
        .prepare_native(small)
        .expect("native multipart body")
    else {
        panic!("multipart serializes as a managed stream");
    };
    let (accepted, kind) = consume_native_stream(body, failure).await;
    let accepted_len: usize = accepted.iter().map(Bytes::len).sum();
    assert!(accepted_len < small.get());
    assert_eq!(kind, Some(BodyErrorKind::LimitExceeded));
}

#[test]
fn factory_materialization_and_explicit_reconstruction_are_attempt_scoped() {
    let calls = Arc::new(AtomicUsize::new(0));
    let body = PreparedBody::factory(Factory {
        calls: Arc::clone(&calls),
    });
    let mut prepared = body.into_managed(limit(64)).expect("initial factory body");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepared.replayability(), Replayability::Reconstructible);
    let _ = prepared
        .prepare_native(limit(64))
        .expect("first native body");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    prepared
        .reconstruct_for_next_attempt(limit(64))
        .expect("next Concord-attempt body");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn dishonest_minimum_is_detected_against_observed_production() {
    let polls = Arc::new(AtomicUsize::new(0));
    let mut managed = PreparedBody::stream(stream([Ok(&b"ab"[..])], &polls).at_least(3))
        .into_managed(limit(64))
        .expect("minimum below limit");
    assert_eq!(managed.length_hint(), LengthHint::AtLeast(3));
    let NativeBody::Stream { body, failure, .. } =
        managed.prepare_native(limit(64)).expect("native body")
    else {
        panic!("stream native body expected");
    };
    let (accepted, kind) = consume_native_stream(body, failure).await;
    assert_eq!(accepted, [Bytes::from_static(b"ab")]);
    assert_eq!(kind, Some(BodyErrorKind::InvalidLength));
}

struct FrameStream {
    frames: VecDeque<Result<Bytes, io::Error>>,
    polls: Arc<AtomicUsize>,
}

impl Stream for FrameStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(self.frames.pop_front())
    }
}

fn stream(
    frames: impl IntoIterator<Item = Result<&'static [u8], io::Error>>,
    polls: &Arc<AtomicUsize>,
) -> RequestStream {
    stream_bytes(
        frames
            .into_iter()
            .map(|frame| frame.map(Bytes::from_static)),
        polls,
    )
}

fn stream_bytes(
    frames: impl IntoIterator<Item = Result<Bytes, io::Error>>,
    polls: &Arc<AtomicUsize>,
) -> RequestStream {
    RequestStream::from_stream(FrameStream {
        frames: frames.into_iter().collect(),
        polls: Arc::clone(polls),
    })
}

fn empty_request_chunks(count: usize) -> Vec<Result<Bytes, io::Error>> {
    std::iter::repeat_with(|| Ok(Bytes::new()))
        .take(count)
        .collect()
}

fn direct_managed(
    body: RequestStream,
    exact_length: Option<u64>,
    limit: usize,
) -> (ManagedRequestStream, BodyFailureState) {
    let failure = BodyFailureState::default();
    let managed = managed_request_stream(
        body.into_source(),
        exact_length,
        NonZeroUsize::new(limit).expect("positive limit"),
        failure.clone(),
    );
    (managed, failure)
}

async fn next_managed(stream: &mut ManagedRequestStream) -> Option<Result<Bytes, BodyError>> {
    poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}

async fn consume_native_stream(
    mut body: reqwest::Body,
    failure: BodyFailureState,
) -> (Vec<Bytes>, Option<BodyErrorKind>) {
    let mut accepted = Vec::new();
    loop {
        match poll_fn(|context| Pin::new(&mut body).poll_frame(context)).await {
            Some(Ok(frame)) => {
                if let Ok(bytes) = frame.into_data() {
                    accepted.push(bytes);
                }
            }
            Some(Err(error)) => {
                drop(error);
                return (accepted, failure.kind());
            }
            None => return (accepted, failure.kind()),
        }
    }
}

async fn consume(
    body: RequestStream,
    exact_length: Option<u64>,
    limit: usize,
) -> (Vec<Bytes>, Option<BodyErrorKind>) {
    let NativeBody::Stream { body, failure, .. } = stream_native(
        body,
        exact_length,
        NonZeroUsize::new(limit).expect("positive limit"),
    ) else {
        panic!("stream native body expected");
    };
    consume_native_stream(body, failure).await
}

#[tokio::test]
async fn managed_stream_withholds_exact_and_limit_excess_frames() {
    let success_polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) = consume(
        stream([Ok(&b"ab"[..]), Ok(&b"cd"[..])], &success_polls),
        Some(4),
        4,
    )
    .await;
    assert_eq!(accepted.concat(), b"abcd");
    assert_eq!(failure, None);
    assert_eq!(success_polls.load(Ordering::SeqCst), 3);

    let overflow_polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) = consume(
        stream([Ok(&b"abc"[..]), Ok(&b"de"[..])], &overflow_polls),
        Some(4),
        64,
    )
    .await;
    assert_eq!(accepted.concat(), b"abc");
    assert_eq!(failure, Some(BodyErrorKind::InvalidLength));
    assert_eq!(overflow_polls.load(Ordering::SeqCst), 2);

    let limit_polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) = consume(
        stream([Ok(&b"abc"[..]), Ok(&b"de"[..])], &limit_polls),
        None,
        4,
    )
    .await;
    assert_eq!(accepted.concat(), b"abc");
    assert_eq!(failure, Some(BodyErrorKind::LimitExceeded));
    assert_eq!(limit_polls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn managed_stream_detects_underflow_and_sanitizes_producer_failure() {
    let underflow_polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) =
        consume(stream([Ok(&b"abc"[..])], &underflow_polls), Some(4), 64).await;
    assert_eq!(accepted.concat(), b"abc");
    assert_eq!(failure, Some(BodyErrorKind::InvalidLength));
    assert_eq!(underflow_polls.load(Ordering::SeqCst), 2);

    let producer_polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) = consume(
        stream(
            [
                Ok(&b"accepted"[..]),
                Err(io::Error::other("producer-secret-881d")),
            ],
            &producer_polls,
        ),
        None,
        64,
    )
    .await;
    assert_eq!(accepted.concat(), b"accepted");
    assert_eq!(failure, Some(BodyErrorKind::Producer));
    assert_eq!(producer_polls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn request_empty_chunk_is_not_forwarded() {
    let polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) =
        consume(stream([Ok(&b""[..]), Ok(&b"data"[..])], &polls), None, 64).await;

    assert_eq!(accepted, [Bytes::from_static(b"data")]);
    assert_eq!(failure, None);
    assert_eq!(polls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn request_empty_chunks_before_data_are_suppressed() {
    let polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) = consume(
        stream(
            [
                Ok(&b""[..]),
                Ok(&b""[..]),
                Ok(&b""[..]),
                Ok(&b"payload"[..]),
            ],
            &polls,
        ),
        None,
        64,
    )
    .await;

    assert_eq!(accepted, [Bytes::from_static(b"payload")]);
    assert_eq!(failure, None);
}

#[tokio::test]
async fn request_empty_chunks_after_data_are_suppressed() {
    let polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) = consume(
        stream(
            [
                Ok(&b"first"[..]),
                Ok(&b""[..]),
                Ok(&b""[..]),
                Ok(&b"second"[..]),
                Ok(&b""[..]),
            ],
            &polls,
        ),
        None,
        64,
    )
    .await;

    assert_eq!(
        accepted,
        [Bytes::from_static(b"first"), Bytes::from_static(b"second")]
    );
    assert_eq!(failure, None);
}

#[test]
fn request_per_poll_budget_returns_pending() {
    let polls = Arc::new(AtomicUsize::new(0));
    let mut chunks = empty_request_chunks(MAX_EMPTY_REQUEST_CHUNKS_PER_POLL);
    chunks.push(Ok(Bytes::from_static(b"deferred")));
    let (mut managed, _) = direct_managed(stream_bytes(chunks, &polls), None, 64);
    let wakes = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut context = Context::from_waker(&waker);

    assert!(matches!(
        Pin::new(&mut managed).poll_next(&mut context),
        Poll::Pending
    ));
    assert_eq!(
        polls.load(Ordering::SeqCst),
        MAX_EMPTY_REQUEST_CHUNKS_PER_POLL
    );
}

#[test]
fn request_per_poll_budget_wakes_task() {
    let polls = Arc::new(AtomicUsize::new(0));
    let mut chunks = empty_request_chunks(MAX_EMPTY_REQUEST_CHUNKS_PER_POLL);
    chunks.push(Ok(Bytes::from_static(b"deferred")));
    let (mut managed, _) = direct_managed(stream_bytes(chunks, &polls), None, 64);
    let wakes = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut context = Context::from_waker(&waker);

    assert!(matches!(
        Pin::new(&mut managed).poll_next(&mut context),
        Poll::Pending
    ));
    assert_eq!(wakes.wakes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn request_exact_empty_limit_succeeds() {
    let polls = Arc::new(AtomicUsize::new(0));
    let mut chunks = empty_request_chunks(MAX_EMPTY_REQUEST_CHUNKS);
    chunks.push(Ok(Bytes::from_static(b"data")));
    let (accepted, failure) = consume(stream_bytes(chunks, &polls), Some(4), 64).await;

    assert_eq!(accepted, [Bytes::from_static(b"data")]);
    assert_eq!(failure, None);
    assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_REQUEST_CHUNKS + 2);
}

#[tokio::test]
async fn request_first_excess_is_terminal() {
    let polls = Arc::new(AtomicUsize::new(0));
    let (mut managed, failure) = direct_managed(
        stream_bytes(empty_request_chunks(MAX_EMPTY_REQUEST_CHUNKS + 1), &polls),
        None,
        64,
    );

    let error = next_managed(&mut managed)
        .await
        .expect("one terminal body failure")
        .expect_err("empty request chunk first excess");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert_eq!(failure.kind(), Some(BodyErrorKind::LimitExceeded));
    assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_REQUEST_CHUNKS + 1);
    assert!(next_managed(&mut managed).await.is_none());
}

#[tokio::test]
async fn request_first_excess_error_is_redacted() {
    const PRODUCER_SENTINEL: &str = "empty-request-producer-secret-509d";
    let polls = Arc::new(AtomicUsize::new(0));
    let (mut managed, _) = direct_managed(
        stream_bytes(empty_request_chunks(MAX_EMPTY_REQUEST_CHUNKS + 1), &polls),
        None,
        64,
    );

    let error = next_managed(&mut managed)
        .await
        .expect("one terminal body failure")
        .expect_err("empty request chunk first excess");
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains(&(MAX_EMPTY_REQUEST_CHUNKS + 1).to_string()));
    assert!(!rendered.contains(PRODUCER_SENTINEL));
    assert!(!rendered.contains("empty chunk"));
}

#[tokio::test]
async fn request_total_count_is_not_reset_by_nonempty_data() {
    let polls = Arc::new(AtomicUsize::new(0));
    let mut chunks = empty_request_chunks(MAX_EMPTY_REQUEST_CHUNKS / 2);
    chunks.push(Ok(Bytes::from_static(b"progress")));
    chunks.extend(empty_request_chunks(MAX_EMPTY_REQUEST_CHUNKS / 2 + 1));
    let (mut managed, _) = direct_managed(stream_bytes(chunks, &polls), None, 64);

    assert_eq!(
        next_managed(&mut managed)
            .await
            .expect("nonempty request data")
            .expect("successful request data"),
        Bytes::from_static(b"progress")
    );
    let error = next_managed(&mut managed)
        .await
        .expect("one terminal body failure")
        .expect_err("response-wide empty count");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
}

#[tokio::test]
async fn request_nonempty_byte_limit_is_unchanged() {
    let polls = Arc::new(AtomicUsize::new(0));
    let (mut managed, _) =
        direct_managed(stream([Ok(&b"abc"[..]), Ok(&b"de"[..])], &polls), None, 4);

    assert_eq!(
        next_managed(&mut managed)
            .await
            .expect("accepted prefix")
            .expect("successful prefix"),
        Bytes::from_static(b"abc")
    );
    let error = next_managed(&mut managed)
        .await
        .expect("one terminal body failure")
        .expect_err("first byte excess");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
}

#[tokio::test]
async fn request_exact_zero_with_empty_chunks_and_eof_succeeds() {
    let (accepted, failure) = consume(RequestStream::from_bytes(Bytes::new()), Some(0), 64).await;

    assert!(accepted.is_empty());
    assert_eq!(failure, None);
}

#[tokio::test]
async fn request_positive_exact_length_with_only_empty_chunks_fails_at_eof() {
    let polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) =
        consume(stream([Ok(&b""[..]), Ok(&b""[..])], &polls), Some(1), 64).await;

    assert!(accepted.is_empty());
    assert_eq!(failure, Some(BodyErrorKind::InvalidLength));
}

#[tokio::test]
async fn request_empty_first_excess_precedes_exact_length_eof_error() {
    let polls = Arc::new(AtomicUsize::new(0));
    let (accepted, failure) = consume(
        stream_bytes(empty_request_chunks(MAX_EMPTY_REQUEST_CHUNKS + 1), &polls),
        Some(1),
        64,
    )
    .await;

    assert!(accepted.is_empty());
    assert_eq!(failure, Some(BodyErrorKind::LimitExceeded));
    assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_REQUEST_CHUNKS + 1);
}

#[tokio::test]
async fn request_upstream_error_after_empty_chunks_is_reduced_safely() {
    const ERROR_SENTINEL: &str = "request-upstream-secret-202a";
    let polls = Arc::new(AtomicUsize::new(0));
    let (mut managed, _) = direct_managed(
        stream_bytes(
            [
                Ok(Bytes::new()),
                Ok(Bytes::new()),
                Err(io::Error::other(ERROR_SENTINEL)),
            ],
            &polls,
        ),
        None,
        64,
    );

    let error = next_managed(&mut managed)
        .await
        .expect("one terminal body failure")
        .expect_err("safe upstream reduction");
    assert_eq!(error.kind(), BodyErrorKind::Producer);
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains(ERROR_SENTINEL));
}

struct DroppableEmptyStream {
    remaining: usize,
    polls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
}

impl Stream for DroppableEmptyStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        if self.remaining == 0 {
            Poll::Ready(None)
        } else {
            self.remaining -= 1;
            Poll::Ready(Some(Ok(Bytes::new())))
        }
    }
}

impl Drop for DroppableEmptyStream {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn request_source_is_dropped_after_first_excess() {
    let polls = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let body = RequestStream::from_stream(DroppableEmptyStream {
        remaining: MAX_EMPTY_REQUEST_CHUNKS + 1,
        polls: Arc::clone(&polls),
        dropped: Arc::clone(&dropped),
    });
    let (mut managed, _) = direct_managed(body, None, 64);

    let error = next_managed(&mut managed)
        .await
        .expect("one terminal body failure")
        .expect_err("empty request chunk first excess");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_REQUEST_CHUNKS + 1);
}

#[tokio::test]
async fn body_factory_stream_uses_same_empty_chunk_contract() {
    let polls = Arc::new(AtomicUsize::new(0));
    let limit = NonZeroUsize::new(64).expect("positive limit");
    let mut prepared = PreparedBody::factory(EmptyChunkFactory {
        polls: Arc::clone(&polls),
    })
    .into_managed(limit)
    .expect("factory body");
    let NativeBody::Stream { body, failure, .. } = prepared
        .prepare_native(limit)
        .expect("managed factory body")
    else {
        panic!("stream native body expected");
    };

    let (accepted, kind) = consume_native_stream(body, failure).await;
    assert_eq!(accepted, [Bytes::from_static(b"factory")]);
    assert_eq!(kind, None);
    assert_eq!(polls.load(Ordering::SeqCst), 4);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_serialized_stream_uses_same_empty_chunk_contract() {
    let polls = Arc::new(AtomicUsize::new(0));
    let part = RequestStream::from_stream(FrameStream {
        frames: [
            Ok(Bytes::new()),
            Ok(Bytes::from_static(b"native")),
            Ok(Bytes::new()),
        ]
        .into_iter()
        .collect(),
        polls: Arc::clone(&polls),
    })
    .exact_length(6);
    let mut managed = PreparedBody::multipart(
        MultipartBody::new()
            .part("field", crate::body::MultipartPart::stream(part))
            .expect("multipart recipe"),
    )
    .expect("prepared multipart")
    .into_managed(NonZeroUsize::new(4096).expect("positive limit"))
    .expect("materialized multipart");
    let exact = managed.known_length().expect("exact framed length");
    let NativeBody::Stream {
        body,
        failure,
        known_length,
    } = managed
        .prepare_native(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("native multipart")
    else {
        panic!("multipart serializes as a managed stream");
    };
    assert_eq!(known_length, Some(exact));

    let (accepted, kind) = consume_native_stream(body, failure).await;
    assert_eq!(kind, None);
    let joined = accepted.concat();
    assert_eq!(joined.len() as u64, exact);
    assert!(accepted.iter().all(|chunk| !chunk.is_empty()));
    assert!(joined.windows(6).any(|window| window == b"native"));
    assert_eq!(polls.load(Ordering::SeqCst), 4);
}

struct PendingStream {
    polls: Arc<AtomicUsize>,
    dropped: Arc<std::sync::atomic::AtomicBool>,
    reached: Arc<tokio::sync::Notify>,
}

impl Stream for PendingStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        self.reached.notify_one();
        Poll::Pending
    }
}

impl Drop for PendingStream {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn dropping_native_consumption_drops_source_and_stops_polling() {
    let polls = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reached = Arc::new(tokio::sync::Notify::new());
    let body = RequestStream::from_stream(PendingStream {
        polls: Arc::clone(&polls),
        dropped: Arc::clone(&dropped),
        reached: Arc::clone(&reached),
    });
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let NativeBody::Stream { mut body, .. } =
        stream_native(body, None, NonZeroUsize::new(64).expect("positive limit"))
    else {
        panic!("stream native body expected");
    };
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let task = tokio::spawn(async move {
        let _ = poll_fn(|context| Pin::new(&mut body).poll_frame(context)).await;
    });
    reached.notified().await;
    let polls_at_cancellation = polls.load(Ordering::SeqCst);
    task.abort();
    let join = task.await.expect_err("native body task must be cancelled");
    assert!(join.is_cancelled());
    assert!(dropped.load(Ordering::SeqCst));
    tokio::task::yield_now().await;
    assert_eq!(polls.load(Ordering::SeqCst), polls_at_cancellation);
}
