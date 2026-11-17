//! C12.11–C12.14: body values, terminal errors, lengths and cooperative composition.
#![allow(clippy::unwrap_used)]
use concord_core::{body::BodyStream, bytes::Bytes, Body};
use futures_core::Stream;
use futures_util::{stream, StreamExt};
use std::{
    io,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Wake, Waker},
};

fn source(opens: Arc<AtomicUsize>, parts: Vec<Result<Bytes, io::Error>>) -> Body {
    let chunks: Vec<_> = parts
        .into_iter()
        .map(|part| part.map_err(|error| error.kind()))
        .collect();
    Body::repeatable(move || {
        opens.fetch_add(1, Ordering::SeqCst);
        let parts = chunks
            .iter()
            .map(|part| part.clone().map_err(io::Error::from))
            .collect::<Vec<_>>();
        async move { Ok::<_, io::Error>(stream::iter(parts)) }
    })
}
async fn collect(mut body: BodyStream) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(piece) = body.next().await {
        out.extend_from_slice(&piece.unwrap());
    }
    out
}
fn request(body: Body, length: &str) -> concord_core::reqwest::Request {
    use concord_core::__private::{Core, Draft, Options, Piece};
    let mut draft = Draft::new(concord_core::http::Method::PUT);
    draft.base(&[Piece::Lit("https://body.test")]);
    draft.header("content-length", &[Piece::Val("length", &length)]);
    draft.body(body);
    draft
        .finish_for_send::<()>(&Core::default(), &Options::default(), "body.put")
        .unwrap()
}
fn exported(body: Body, length: &str) -> BodyStream {
    let mut request = request(body, length);
    Box::pin(
        concord_core::reqwest::Response::from(concord_core::http::Response::new(
            request.body_mut().take().unwrap(),
        ))
        .bytes_stream()
        .map(|piece| piece.map_err(Into::into)),
    )
}
#[tokio::test(start_paused = true)]
async fn c12_11_content_type_is_the_whole_unvalidated_borrowed_label() {
    let label = "multipart/mixed; boundary=a";
    let body = Body::new("x", label);
    assert_eq!(body.get_content_type(), Some(label));
    assert_eq!(body.get_content_type().unwrap().as_ptr(), label.as_ptr());
    for label in ["", "invalid\nlabel", "text/plain; charset=utf-8"] {
        let opens = Arc::<AtomicUsize>::default();
        let body = source(opens.clone(), vec![]).content_type(label);
        assert_eq!(body.get_content_type(), Some(label));
        assert_eq!(opens.load(Ordering::SeqCst), 0);
    }
    assert_eq!(Body::from("x").get_content_type(), None);
    use concord_core::__private::{Core, Draft, Options, Piece};
    let mut draft = Draft::new(concord_core::http::Method::PUT);
    draft.base(&[Piece::Lit("https://body.test")]);
    draft.header("content-type", &[Piece::Lit("description/type")]);
    draft.body(body);
    assert_eq!(
        draft
            .finish::<()>(&Core::default(), &Options::default(), "body.put")
            .unwrap()
            .headers()["content-type"],
        "description/type"
    );
}
#[tokio::test(start_paused = true)]
async fn c12_12_clones_and_maps_open_independent_streams_and_bytes_maps_stay_one_shot() {
    let opens = Arc::new(AtomicUsize::new(0));
    let maps = Arc::new(AtomicUsize::new(0));
    let seen = maps.clone();
    let body = source(opens.clone(), vec![Ok(Bytes::from_static(b"abc"))]).map_stream(move |s| {
        seen.fetch_add(1, Ordering::SeqCst);
        s
    });
    assert_eq!(format!("{body:?}"), "Body::Stream");
    let a = body.clone().into_stream().unwrap();
    let b = Body::from(&body).into_stream().unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(maps.load(Ordering::SeqCst), 0);
    assert_eq!(collect(a).await, b"abc");
    assert_eq!(collect(b).await, b"abc");
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(maps.load(Ordering::SeqCst), 2);
    let bytes = Body::from("x").map_stream(|s| s);
    drop(bytes.clone().into_stream().unwrap());
    assert!(bytes.into_stream().is_err());
}
#[tokio::test(start_paused = true)]
async fn c12_13_errors_are_terminal_and_later_parts_never_open() {
    for opening in [true, false] {
        let later = Arc::new(AtomicUsize::new(0));
        let bad = Body::repeatable(move || async move {
            if opening {
                Err(io::Error::new(io::ErrorKind::NotFound, "private-source"))
            } else {
                Ok(stream::iter([Err::<Bytes, _>(io::Error::new(
                    io::ErrorKind::NotFound,
                    "private-source",
                ))]))
            }
        });
        let mut stream = Body::concat([bad, source(later.clone(), vec![])])
            .into_stream()
            .unwrap();
        let error = stream.next().await.unwrap().unwrap_err();
        let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&*error);
        let mut found = false;
        while let Some(error) = cause {
            found |= error
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::NotFound);
            cause = error.source();
        }
        assert!(found);
        for _ in 0..3 {
            assert!(stream.next().await.is_none());
        }
        assert_eq!(later.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test(start_paused = true)]
async fn c12_13_lengths_check_eof_on_a_chunk_boundary_and_fuse_after_failure() {
    for (length, chunks, valid) in [
        (3, vec!["abc"], true),
        (3, vec!["abc", "x"], false),
        (3, vec!["ab"], false),
        (3, vec!["abcd"], false),
        (0, vec!["x"], false),
        (0, vec![], true),
        (3, vec!["abc", ""], true),
    ] {
        let opens = Arc::new(AtomicUsize::new(0));
        let body = source(
            opens.clone(),
            chunks
                .into_iter()
                .map(|s| Ok(Bytes::copy_from_slice(s.as_bytes())))
                .collect(),
        );
        let mut stream = exported(body, &length.to_string());
        assert_eq!(opens.load(Ordering::SeqCst), 0);
        let mut failure = false;
        let mut bytes = Vec::new();
        while let Some(piece) = stream.next().await {
            match piece {
                Ok(chunk) => bytes.extend_from_slice(&chunk),
                Err(error) => {
                    failure = true;
                    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&*error);
                    let mut invalid = false;
                    while let Some(e) = cause {
                        invalid |= e
                            .downcast_ref::<io::Error>()
                            .is_some_and(|e| e.kind() == io::ErrorKind::InvalidData);
                        cause = e.source();
                    }
                    assert!(invalid);
                    break;
                }
            }
        }
        assert_eq!(!failure, valid, "length {length}");
        if length == 3 && !valid && bytes.len() != 2 {
            assert!(bytes.is_empty(), "the final chunk must be withheld");
        }
        for _ in 0..3 {
            assert!(stream.next().await.is_none());
        }
    }
}
struct Wakes(AtomicUsize);
impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn c12_12_nested_empty_compositions_yield_with_bounded_work_and_keep_progress() {
    let opens = Arc::new(AtomicUsize::new(0));
    let parts = (0..10_000)
        .map(|_| Body::concat([source(opens.clone(), vec![])]))
        .collect::<Vec<_>>();
    let mut stream = Body::concat(parts).into_stream().unwrap();
    let wakes = Arc::new(Wakes(AtomicUsize::new(0)));
    let waker = Waker::from(wakes.clone());
    let mut cx = Context::from_waker(&waker);
    assert!(Pin::new(&mut stream).poll_next(&mut cx).is_pending());
    assert!(opens.load(Ordering::SeqCst) < 100);
    assert!(wakes.0.load(Ordering::SeqCst) > 0);
    let mut completed = false;
    for _ in 0..10_000 {
        if matches!(Pin::new(&mut stream).poll_next(&mut cx), Poll::Ready(None)) {
            completed = true;
            break;
        }
    }
    assert!(completed);
    assert_eq!(opens.load(Ordering::SeqCst), 10_000);
}
#[tokio::test(start_paused = true)]
async fn c12_12_mixed_claim_failure_neither_opens_factories_nor_spends_other_sources() {
    let opens = Arc::new(AtomicUsize::new(0));
    let once = Body::stream(stream::iter([Ok::<_, io::Error>(Bytes::from_static(
        b"one",
    ))]));
    let fresh = Body::stream(stream::iter([Ok::<_, io::Error>(Bytes::from_static(
        b"fresh",
    ))]));
    let body = Body::concat([
        source(opens.clone(), vec![]),
        once.clone(),
        once.clone(),
        fresh.clone(),
    ]);
    assert!(body.into_stream().is_err());
    assert_eq!(opens.load(Ordering::SeqCst), 0);
    assert_eq!(collect(fresh.into_stream().unwrap()).await, b"fresh");
}

#[tokio::test(start_paused = true)]
async fn c12_13_a_map_cannot_hide_an_observed_source_failure_or_open_a_later_part() {
    for kind in 0..3 {
        let later = Arc::new(AtomicUsize::new(0));
        let bad = if kind == 2 {
            Body::stream(stream::iter([Err::<Bytes, _>(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private-once",
            ))]))
        } else {
            Body::repeatable(move || async move {
                if kind == 0 {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "private-open",
                    ))
                } else {
                    Ok(stream::iter([Err::<Bytes, _>(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "private-read",
                    ))]))
                }
            })
        };
        let mapped = Body::concat([bad, source(later.clone(), vec![])]).map_stream(|mut input| {
            Box::pin(stream::once(async move {
                let _ = input.next().await;
                let _ = input.next().await;
                Ok(Bytes::from_static(b"hidden failure"))
            }))
        });
        let mut stream = mapped.into_stream().unwrap();
        assert!(stream.next().await.unwrap().is_err());
        assert!(stream.next().await.is_none());
        assert_eq!(later.load(Ordering::SeqCst), 0);
    }
}
