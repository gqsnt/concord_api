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

struct MetadataFactory {
    content_type_calls: Arc<AtomicUsize>,
    create_calls: Arc<AtomicUsize>,
}

struct EmptyChunkFactory {
    polls: Arc<AtomicUsize>,
}

impl BodyFactory for EmptyChunkFactory {
    fn create(&self) -> Result<StreamBody, BodyError> {
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
    fn content_type(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static("application/x-factory"))
    }

    fn create(&self) -> Result<StreamBody, BodyError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(StreamBody::from_bytes(Bytes::from_static(b"factory")))
    }
}

impl BodyFactory for MetadataFactory {
    fn content_type(&self) -> Option<HeaderValue> {
        self.content_type_calls.fetch_add(1, Ordering::SeqCst);
        Some(HeaderValue::from_static("application/x-captured"))
    }

    fn create(&self) -> Result<StreamBody, BodyError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(StreamBody::from_bytes(Bytes::from_static(b"factory")))
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

fn metadata(body: &RequestBody) -> RequestBodyMetadata {
    body.structural_metadata().expect("body metadata")
}

#[test]
fn request_body_kinds_and_replayability_are_inferred() {
    let empty = RequestBody::empty();
    assert_eq!(empty.kind(), BodyKind::Empty);
    assert_eq!(metadata(&empty).length, LengthHint::Exact(0));
    assert_eq!(
        empty
            .into_prepared(NonZeroUsize::new(64).unwrap())
            .unwrap()
            .replayability(),
        Replayability::Reusable
    );

    let bytes = RequestBody::bytes(Bytes::from_static(b"bytes"));
    assert_eq!(bytes.kind(), BodyKind::Buffered);
    assert_eq!(metadata(&bytes).known_length(), Some(5));
    assert_eq!(
        bytes
            .into_prepared(NonZeroUsize::new(64).unwrap())
            .unwrap()
            .replayability(),
        Replayability::Reusable
    );

    let stream = RequestBody::stream(StreamBody::from_bytes(Bytes::from_static(b"bytes")));
    assert_eq!(stream.kind(), BodyKind::Stream);
    assert_eq!(
        stream
            .into_prepared(NonZeroUsize::new(64).unwrap())
            .unwrap()
            .replayability(),
        Replayability::OneShot
    );

    let calls = Arc::new(AtomicUsize::new(0));
    let factory = RequestBody::factory(Factory {
        calls: Arc::clone(&calls),
    });
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        metadata(&factory).content_type,
        RequestContentType::Exact(HeaderValue::from_static("application/x-factory"))
    );
    assert_eq!(
        factory
            .into_prepared(NonZeroUsize::new(64).unwrap())
            .unwrap()
            .replayability(),
        Replayability::Reconstructible
    );
}

#[test]
fn content_type_is_validated_and_factory_default_can_be_overridden() {
    let calls = Arc::new(AtomicUsize::new(0));
    let body = RequestBody::factory(Factory { calls })
        .content_type("application/x-override; version=1")
        .expect("valid media type");
    assert_eq!(
        metadata(&body).content_type,
        RequestContentType::Exact(HeaderValue::from_static(
            "application/x-override; version=1"
        ))
    );
    assert_eq!(
        RequestBody::empty()
            .content_type("not a media type")
            .expect_err("invalid media")
            .kind(),
        BodyErrorKind::InvalidContentType
    );
}

#[test]
fn factory_content_type_is_captured_before_metadata_without_materialization() {
    let content_type_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let body = RequestBody::factory(MetadataFactory {
        content_type_calls: Arc::clone(&content_type_calls),
        create_calls: Arc::clone(&create_calls),
    });
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);

    for _ in 0..2 {
        assert_eq!(
            metadata(&body).content_type,
            RequestContentType::Exact(HeaderValue::from_static("application/x-captured"))
        );
    }
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "multipart")]
#[test]
fn multipart_replayability_and_factory_freshness_are_internal() {
    let reusable = RequestBody::multipart(
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
    );
    assert_eq!(reusable.kind(), BodyKind::Multipart);
    assert_eq!(
        metadata(&reusable).content_type,
        RequestContentType::Generated
    );
    assert!(
        reusable
            .content_type("multipart/form-data; boundary=application-owned")
            .is_err()
    );

    let mut reusable_prepared = RequestBody::multipart(
        MultipartBody::new()
            .part(
                "text",
                crate::body::MultipartPart::text("fresh recipe").expect("bounded multipart text"),
            )
            .expect("text part"),
    )
    .into_prepared(NonZeroUsize::new(4096).expect("positive limit"))
    .expect("reusable multipart materialization");
    assert_eq!(reusable_prepared.replayability(), Replayability::Reusable);
    let _ = reusable_prepared
        .prepare_native(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("first reusable native recipe");
    reusable_prepared
        .reconstruct_for_next_attempt(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("reusable next attempt");
    let _ = reusable_prepared
        .prepare_native(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("second reusable native recipe");

    let one_shot = RequestBody::multipart(
        MultipartBody::new()
            .part(
                "stream",
                crate::body::MultipartPart::stream(StreamBody::from_bytes(Bytes::from_static(
                    b"multipart-stream-secret",
                ))),
            )
            .expect("stream part"),
    );
    let mut one_shot = one_shot
        .into_prepared(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("one-shot multipart materialization");
    assert!(
        one_shot
            .reconstruct_for_next_attempt(NonZeroUsize::new(4096).expect("positive limit"))
            .is_err()
    );

    let calls = Arc::new(AtomicUsize::new(0));
    let factory = RequestBody::multipart_factory(MultipartFactoryProbe {
        calls: Arc::clone(&calls),
    });
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let mut prepared = factory
        .into_prepared(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("multipart factory materialization");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepared.replayability(), Replayability::Reconstructible);
    let _ = prepared
        .prepare_native(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("current multipart attempt");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    prepared
        .reconstruct_for_next_attempt(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("next multipart attempt");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn reqwest_multipart_serialization_uses_the_managed_aggregate_counter() {
    let first_polls = Arc::new(AtomicUsize::new(0));
    let form = MultipartBody::new()
        .part(
            "field",
            crate::body::MultipartPart::stream(stream([Ok(&b"x"[..])], &first_polls)),
        )
        .expect("multipart recipe")
        .into_form(NonZeroUsize::new(4096).expect("positive limit"))
        .expect("native multipart recipe");
    let mut serialized = Box::pin(form.into_stream());
    let mut serialized_frames = Vec::new();
    loop {
        match poll_fn(|context| serialized.as_mut().poll_next(context)).await {
            Some(Ok(bytes)) => serialized_frames.push(bytes),
            Some(Err(error)) => {
                drop(error);
                panic!("reference serialization failed")
            }
            None => break,
        }
    }
    assert!(serialized_frames.len() >= 2);
    let crossing_limit = serialized_frames[0]
        .len()
        .checked_add(serialized_frames[1].len())
        .and_then(|length| length.checked_sub(1))
        .expect("multipart frame lengths");
    let limit = NonZeroUsize::new(crossing_limit).expect("positive crossing limit");

    let second_polls = Arc::new(AtomicUsize::new(0));
    let form = MultipartBody::new()
        .part(
            "field",
            crate::body::MultipartPart::stream(stream([Ok(&b"x"[..])], &second_polls)),
        )
        .expect("multipart recipe")
        .into_form(limit)
        .expect("native multipart recipe");
    let serialized = reqwest::Body::wrap_stream(form.into_stream());
    let (mut managed, failure) = wrap_multipart_native_body(serialized, None, limit);
    let mut accepted = Vec::new();
    let native_error = loop {
        match poll_fn(|context| Pin::new(&mut managed).poll_frame(context)).await {
            Some(Ok(frame)) => {
                if let Ok(bytes) = frame.into_data() {
                    accepted.extend_from_slice(&bytes);
                }
            }
            Some(Err(error)) => {
                drop(error);
                break true;
            }
            None => break false,
        }
    };
    assert!(
        native_error,
        "multipart framing must exceed the small limit"
    );
    assert_eq!(accepted.len(), serialized_frames[0].len());
    assert!(accepted.len() < limit.get());
    assert_eq!(failure.kind(), Some(BodyErrorKind::LimitExceeded));
    assert!(1 < limit.get(), "the one-byte payload itself fits");
}

#[test]
fn factory_materialization_and_explicit_reconstruction_are_attempt_scoped() {
    let calls = Arc::new(AtomicUsize::new(0));
    let body = RequestBody::factory(Factory {
        calls: Arc::clone(&calls),
    });
    let mut prepared = body
        .into_prepared(NonZeroUsize::new(64).unwrap())
        .expect("initial factory body");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepared.replayability(), Replayability::Reconstructible);
    let _ = prepared
        .prepare_native(NonZeroUsize::new(64).unwrap())
        .expect("first native body");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    prepared
        .reconstruct_for_next_attempt(NonZeroUsize::new(64).unwrap())
        .expect("next Concord-attempt body");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
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
) -> StreamBody {
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
) -> StreamBody {
    StreamBody::from_stream(FrameStream {
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
    body: StreamBody,
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
    body: StreamBody,
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
    let (accepted, failure) = consume(StreamBody::from_bytes(Bytes::new()), Some(0), 64).await;

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
    let body = StreamBody::from_stream(DroppableEmptyStream {
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
    let mut prepared = RequestBody::factory(EmptyChunkFactory {
        polls: Arc::clone(&polls),
    })
    .into_prepared(limit)
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
async fn multipart_or_native_managed_stream_uses_same_contract() {
    let polls = Arc::new(AtomicUsize::new(0));
    let source = FrameStream {
        frames: [
            Ok(Bytes::new()),
            Ok(Bytes::from_static(b"native")),
            Ok(Bytes::new()),
        ]
        .into_iter()
        .collect(),
        polls: Arc::clone(&polls),
    };
    let native = reqwest::Body::wrap_stream(source);
    let (managed, failure) = wrap_multipart_native_body(
        native,
        Some(6),
        NonZeroUsize::new(64).expect("positive limit"),
    );

    let (accepted, kind) = consume_native_stream(managed, failure).await;
    assert_eq!(accepted, [Bytes::from_static(b"native")]);
    assert_eq!(kind, None);
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
    let body = StreamBody::from_stream(PendingStream {
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
