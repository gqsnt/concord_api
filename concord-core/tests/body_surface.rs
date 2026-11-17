//! Public body writer contracts C12.11–C12.12.
#![allow(clippy::unwrap_used)]
use concord_core::{bytes::Bytes, Body};
use futures_util::{stream, StreamExt};
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[test]
fn c12_11_text_bytes_and_concatenations_have_no_implicit_label() {
    for body in [
        Body::from("text"),
        Body::from(String::from("text")),
        Body::from(vec![1, 2]),
        Body::from(Bytes::from_static(b"bytes")),
    ] {
        assert_eq!(body.get_content_type(), None);
    }
    assert_eq!(Body::concat([]).get_content_type(), None);
    assert_eq!(
        Body::concat([Body::new("a", "first/type")]).get_content_type(),
        None
    );
    let source = Body::repeatable(|| async {
        Ok::<_, io::Error>(stream::empty::<Result<Bytes, io::Error>>())
    })
    .content_type("part/type");
    assert_eq!(Body::concat([source]).get_content_type(), None);
}
#[test]
fn c12_12_querying_repeatability_never_claims_opens_or_maps() {
    let opens = Arc::new(AtomicUsize::new(0));
    let opened = opens.clone();
    let maps = Arc::new(AtomicUsize::new(0));
    let mapped = maps.clone();
    let source = Body::repeatable(move || {
        opened.fetch_add(1, Ordering::SeqCst);
        async { Ok::<_, io::Error>(stream::empty::<Result<Bytes, io::Error>>()) }
    })
    .map_stream(move |input| {
        mapped.fetch_add(1, Ordering::SeqCst);
        input
    });
    assert!(Body::from("buffered").is_repeatable());
    assert!(source.is_repeatable());
    assert!(Body::concat([Body::from("prefix"), source.clone()]).is_repeatable());
    assert!(!Body::from("buffered")
        .map_stream(|input| input)
        .is_repeatable());
    let once = Body::stream(stream::empty::<Result<Bytes, io::Error>>());
    assert!(!once.is_repeatable());
    assert!(!Body::concat([source, once.clone()]).is_repeatable());
    assert!(once.into_stream().is_ok());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(maps.load(Ordering::SeqCst), 0);
}
#[tokio::test(start_paused = true)]
async fn c12_12_a_map_frames_transforms_and_rechunks_without_changing_output_between_openings() {
    let opens = Arc::new(AtomicUsize::new(0));
    let opened = opens.clone();
    let maps = Arc::new(AtomicUsize::new(0));
    let mapped = maps.clone();
    let source = Body::repeatable(move || {
        let pieces = if opened.fetch_add(1, Ordering::SeqCst) == 0 {
            [b"a".as_slice(), b"bc"]
        } else {
            [b"ab".as_slice(), b"c"]
        };
        async move { Ok::<_, io::Error>(stream::iter(pieces.map(|p| Ok(Bytes::from_static(p))))) }
    })
    .content_type("input/type")
    .map_stream(move |input| {
        mapped.fetch_add(1, Ordering::SeqCst);
        Box::pin(
            stream::once(async { Ok(Bytes::from_static(b"<")) })
                .chain(
                    input.map(|piece| piece.map(|bytes| Bytes::from(bytes.to_ascii_uppercase()))),
                )
                .chain(stream::once(async { Ok(Bytes::from_static(b">")) })),
        )
    });
    let first = source.clone().into_stream().unwrap();
    let second = source.clone().into_stream().unwrap();
    assert_eq!(source.get_content_type(), Some("input/type"));
    assert!(source.is_repeatable());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(maps.load(Ordering::SeqCst), 0);
    for mut stream in [first, second] {
        let mut bytes = Vec::new();
        while let Some(piece) = stream.next().await {
            bytes.extend_from_slice(&piece.unwrap());
        }
        assert_eq!(bytes, b"<ABC>");
    }
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(maps.load(Ordering::SeqCst), 2);
}
#[tokio::test(start_paused = true)]
async fn c12_12_a_map_can_emit_framing_before_opening_input_and_a_drop_opens_nothing() {
    let opens = Arc::new(AtomicUsize::new(0));
    let opened = opens.clone();
    let source = Body::repeatable(move || {
        opened.fetch_add(1, Ordering::SeqCst);
        async { Ok::<_, io::Error>(stream::iter([Ok(Bytes::from_static(b"input"))])) }
    })
    .map_stream(|input| {
        Box::pin(stream::once(async { Ok(Bytes::from_static(b"prefix")) }).chain(input))
    });
    let mut output = source.into_stream().unwrap();
    assert_eq!(output.next().await.unwrap().unwrap(), b"prefix"[..]);
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    drop(output);
    assert_eq!(opens.load(Ordering::SeqCst), 0);
}
#[test]
fn c12_12_a_failed_atomic_claim_runs_no_map_and_leaves_other_sources_unclaimed() {
    let maps = Arc::new(AtomicUsize::new(0));
    let mapped = maps.clone();
    let once = Body::stream(stream::empty::<Result<Bytes, io::Error>>());
    let other = Body::stream(stream::empty::<Result<Bytes, io::Error>>());
    let result = Body::concat([once.clone(), once, other.clone()])
        .map_stream(move |input| {
            mapped.fetch_add(1, Ordering::SeqCst);
            input
        })
        .into_stream();
    assert!(result.is_err());
    assert_eq!(maps.load(Ordering::SeqCst), 0);
    assert!(other.into_stream().is_ok());
}

#[test]
fn c12_12_repeatability_docs_make_the_buffered_map_exception_explicit() {
    let docs = include_str!("../src/body.rs");
    assert!(docs.contains(
        "/// A map over buffered bytes is one-shot, even when its output is deterministic."
    ));
    let bytes = Body::from("bytes");
    assert!(bytes.is_repeatable());
    assert!(!bytes.map_stream(|input| input).is_repeatable());
    let factory = Body::repeatable(|| async {
        Ok::<_, io::Error>(stream::empty::<Result<Bytes, io::Error>>())
    });
    assert!(factory.map_stream(|input| input).is_repeatable());
}

struct ObservedReplacement {
    hints: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}
impl hyper::body::Body for ObservedReplacement {
    type Data = Bytes;
    type Error = io::Error;
    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, io::Error>>> {
        std::task::Poll::Ready(None)
    }
    fn size_hint(&self) -> hyper::body::SizeHint {
        self.hints.fetch_add(1, Ordering::SeqCst);
        hyper::body::SizeHint::with_exact(0)
    }
}
impl Drop for ObservedReplacement {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn c12_14_replacement_size_hint_and_destruction_run_during_building_without_opening_the_source() {
    use concord_core::__private::{Core, Draft, Options, Piece};
    let hints = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let opens = Arc::new(AtomicUsize::new(0));
    let held = Arc::new(std::sync::Mutex::new(None));
    let saved = held.clone();
    let queried = hints.clone();
    let destroyed = drops.clone();
    let opened = opens.clone();
    let mut draft = Draft::new(concord_core::http::Method::PUT);
    draft.base(&[Piece::Lit("https://body.test")]);
    draft.body(Body::repeatable(move || {
        opened.fetch_add(1, Ordering::SeqCst);
        async { Ok::<_, io::Error>(stream::empty::<Result<Bytes, io::Error>>()) }
    }));
    let mut options = Options::default();
    options.edit_request(move |request| {
        *saved.lock().unwrap() = request.body_mut().take();
        *request.body_mut() = Some(concord_core::reqwest::Body::wrap(ObservedReplacement {
            hints: queried.clone(),
            drops: destroyed.clone(),
        }));
    });
    let error = draft
        .finish_for_send::<()>(&Core::default(), &options, "body.put")
        .unwrap_err();
    assert_eq!(error.kind(), concord_core::error::Kind::Build);
    assert_eq!(error.tries(), 0);
    assert_eq!(hints.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(opens.load(Ordering::SeqCst), 0);
}
