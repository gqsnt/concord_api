//! F1: is one canonical by-value conversion (`EncodedBody`) a simpler request-format contract
//! than today's `Encode` (required `encode`, defaulted `encode_body`)?
//!
//! Compiled comparison. The alternative is a successor trait with a single required method and
//! a blanket implementation over every existing `Encode` format (the `IntoIterator`-over-
//! `Iterator` coherence pattern), so existing formats would keep working while a runtime-media
//! format implements one method. Both arms implement the same runtime-media format (a tagged
//! upload whose media type is chosen per body) and the same static format, and are held to the
//! same observable contract: exact bytes and media, no media for an absent body, a one-shot
//! non-`Clone` source moved, not buffered or polled while preparing.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::bytes::Bytes;
use concord::reqwest::header::HeaderValue;
use concord::{CodecError, Encode, EncodedBody};

/// A body whose media type is known only per value (a dialect, a boundary).
pub struct Tagged {
    media: &'static str,
    bytes: Bytes,
}

/// A one-shot, non-`Clone` source: counts how often it is polled.
pub struct OneShot {
    polls: Arc<AtomicUsize>,
    data: Vec<Bytes>,
}

// ---- Arm A: today's contract -------------------------------------------------------------

/// The runtime-media format, current `Encode`: `encode` must refuse, `encode_body` does the work.
pub struct TaggedFormat;

impl Encode<Tagged> for TaggedFormat {
    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(_: Tagged) -> Result<concord::reqwest::Body, CodecError> {
        Err(CodecError::new(
            "a tagged body has a runtime media type; it is encoded with `encode_body`",
        ))
    }

    fn encode_body(value: Tagged) -> Result<EncodedBody, CodecError> {
        let media = HeaderValue::from_static(value.media);
        Ok(EncodedBody::with_content_type(value.bytes.into(), media))
    }
}

/// A static format (the shape of every SDK-authored format in the corpus): one method.
pub struct Upper;

impl Encode<String> for Upper {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: String) -> Result<concord::reqwest::Body, CodecError> {
        Ok(value.to_uppercase().into())
    }
}

/// A lazy one-shot format: moves the source into a streaming body; preparing polls nothing.
pub struct Lazy;

impl Encode<OneShot> for Lazy {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: OneShot) -> Result<concord::reqwest::Body, CodecError> {
        let OneShot { polls, data } = value;
        Ok(concord::reqwest::Body::wrap_stream(Counted {
            polls,
            data: data.into_iter(),
        }))
    }
}

/// A source stream that counts its polls (preparing a request must not poll it).
pub struct Counted {
    polls: Arc<AtomicUsize>,
    data: std::vec::IntoIter<Bytes>,
}

impl futures_core::Stream for Counted {
    type Item = Result<Bytes, std::io::Error>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        std::task::Poll::Ready(self.data.next().map(Ok))
    }
}

// ---- Arm B: the single-entry successor ---------------------------------------------------

/// The alternative: one required conversion to `EncodedBody`.
pub trait EncodeOnce<T> {
    /// As `Encode::CONTENT_TYPE`.
    const CONTENT_TYPE: Option<&'static str>;
    /// The only conversion.
    fn encode_once(value: T) -> Result<EncodedBody, CodecError>;
}

/// Every existing format keeps working (through its `encode_body`, which defaults to `encode`).
impl<F: Encode<T>, T> EncodeOnce<T> for F {
    const CONTENT_TYPE: Option<&'static str> = <F as Encode<T>>::CONTENT_TYPE;
    fn encode_once(value: T) -> Result<EncodedBody, CodecError> {
        F::encode_body(value)
    }
}

/// The same runtime-media format written against the successor: one method, no refusal. It
/// compiles next to the blanket implementation because `TaggedOnce` implements no `Encode`.
pub struct TaggedOnce;

impl EncodeOnce<Tagged> for TaggedOnce {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode_once(value: Tagged) -> Result<EncodedBody, CodecError> {
        let media = HeaderValue::from_static(value.media);
        Ok(EncodedBody::with_content_type(value.bytes.into(), media))
    }
}

fn tagged() -> Tagged {
    Tagged {
        media: "text/csv; header=present",
        bytes: Bytes::from_static(b"a,b\n1,2\n"),
    }
}

#[test]
fn both_arms_produce_the_same_body_and_runtime_media() {
    let (a_body, a_media) = TaggedFormat::encode_body(tagged()).unwrap().into_parts();
    let (b_body, b_media) = TaggedOnce::encode_once(tagged()).unwrap().into_parts();
    assert_eq!(a_media, b_media);
    assert_eq!(a_body.as_bytes(), b_body.as_bytes());
    assert_eq!(a_media.unwrap(), "text/csv; header=present");
}

#[test]
fn the_current_arm_cannot_lose_the_runtime_media_through_encode() {
    // Code that calls the primary method directly is refused rather than given a body without
    // its media type: the invariant the two-method shape protects at the cost of a refusal.
    assert!(TaggedFormat::encode(tagged()).is_err());
}

#[test]
fn existing_formats_reach_the_successor_through_the_blanket() {
    let (body, media) = <Upper as EncodeOnce<String>>::encode_once("hi".into())
        .unwrap()
        .into_parts();
    assert_eq!(body.as_bytes(), Some(b"HI".as_slice()));
    assert_eq!(media, None, "static media stays the format's CONTENT_TYPE");
    assert_eq!(
        <Upper as EncodeOnce<String>>::CONTENT_TYPE,
        Some("text/plain; charset=utf-8")
    );
}

concord::api! {
    /// The generated path: a declared body of each kind.
    pub client Uploads() {
        base "http://uploads.invalid";

        POST tagged(body item: Tagged) { path "tagged"; request TaggedFormat; }
        POST note(body item?: String) { path "note"; request Upper; }
        POST raw(body item: OneShot) { path "raw"; request Lazy; }
    }
}

#[test]
fn the_generated_path_keeps_runtime_media_absent_bodies_and_one_shot_sources() {
    let api = Uploads::new(concord::reqwest::Client::new());
    let tagged = api.tagged(tagged()).prepare().unwrap();
    assert_eq!(
        tagged.request().headers()["content-type"],
        "text/csv; header=present"
    );
    let absent = api.note().prepare().unwrap();
    assert!(absent.request().headers().get("content-type").is_none());
    assert!(absent.request().body().is_none());
    let polls = Arc::new(AtomicUsize::new(0));
    let raw = api
        .raw(OneShot {
            polls: polls.clone(),
            data: vec![Bytes::from_static(b"x")],
        })
        .prepare()
        .unwrap();
    let _ = format!("{raw:?}");
    assert_eq!(polls.load(Ordering::SeqCst), 0, "preparing polls nothing");
    assert!(
        raw.request().body().unwrap().as_bytes().is_none(),
        "not buffered"
    );
}
