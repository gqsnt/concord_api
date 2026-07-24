use std::collections::VecDeque;
use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::task::{Context, Poll};

use super::*;
use http_body::Body as _;

struct Factory {
    calls: Arc<AtomicUsize>,
}

struct MetadataFactory {
    content_type_calls: Arc<AtomicUsize>,
    create_calls: Arc<AtomicUsize>,
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
    StreamBody::from_stream(FrameStream {
        frames: frames
            .into_iter()
            .map(|frame| frame.map(Bytes::from_static))
            .collect(),
        polls: Arc::clone(polls),
    })
}

async fn consume(
    body: StreamBody,
    exact_length: Option<u64>,
    limit: usize,
) -> (Vec<Bytes>, Option<BodyErrorKind>) {
    let NativeBody::Stream {
        mut body, failure, ..
    } = stream_native(
        body,
        exact_length,
        NonZeroUsize::new(limit).expect("positive limit"),
    )
    else {
        panic!("stream native body expected");
    };
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
