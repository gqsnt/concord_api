//! Details of the shared `concord_codecs::stream` items: the in-memory `Chunks` source's `Debug`
//! shows counts, never content, and `from_iter` accepts items that are not `Unpin` (its
//! iterator is never pinned).
#![cfg(feature = "stream")]

use std::convert::Infallible;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use concord_codecs::stream::{Chunks, FromIter, from_iter};
use futures_core::Stream;

#[test]
fn chunks_debug_shows_counts_not_content() {
    let source = Chunks::new(["TOP_SECRET_BODY", "", "more secret"]);
    let failing = Chunks::new(["TOP_SECRET_BODY"]).then_fail("the peer reset the stream");
    for shown in [
        format!("{source:?}"),
        format!("{source:#?}"),
        format!("{failing:?}"),
    ] {
        assert!(!shown.contains("SECRET"), "{shown}");
        assert!(!shown.contains("secret"), "{shown}");
    }
    assert_eq!(
        format!("{source:?}"),
        "Chunks { chunks: 3, bytes: 26, fails_at_end: false }"
    );
    assert_eq!(
        format!("{failing:?}"),
        "Chunks { chunks: 1, bytes: 15, fails_at_end: true }"
    );
}

/// An owned, `Send`, serializable row that is not `Unpin`.
#[derive(Debug, serde::Serialize)]
struct Pinned {
    id: u32,
    #[serde(skip)]
    _pin: PhantomPinned,
}

fn row(id: u32) -> Result<Pinned, Infallible> {
    Ok(Pinned {
        id,
        _pin: PhantomPinned,
    })
}

fn assert_unpin<T: Unpin>() {}

#[test]
fn from_iter_accepts_items_that_are_not_unpin() {
    // `vec::IntoIter<T>` is not `Unpin` when `T` is not; the adapter is anyway.
    assert_unpin::<FromIter<std::vec::IntoIter<Result<Pinned, Infallible>>>>();
    let mut rows = from_iter(vec![row(1), row(2)]);
    assert_eq!(rows.size_hint(), (2, Some(2)));
    let mut cx = Context::from_waker(Waker::noop());
    let mut ids = Vec::new();
    while let Poll::Ready(Some(item)) = Pin::new(&mut rows).poll_next(&mut cx) {
        ids.push(item.unwrap().id);
    }
    assert_eq!(ids, [1, 2]);
}

/// The same rows uploaded as JSON Lines: the body serializes them lazily, as for any row.
#[cfg(feature = "jsonl")]
#[test]
fn rows_that_are_not_unpin_upload_as_json_lines() {
    use concord_codecs::jsonl::{JsonLinesOptions, JsonLinesWriter, StreamRecords};
    use http_body::Body as _;

    let upload = JsonLinesWriter::new(JsonLinesOptions::json_lines())
        .upload(StreamRecords::new(from_iter(vec![row(1), row(2)])));
    let (body, _) = upload.into_parts();
    let mut body = std::pin::pin!(body);
    let mut cx = Context::from_waker(Waker::noop());
    let mut out = Vec::new();
    loop {
        match body.as_mut().poll_frame(&mut cx) {
            Poll::Ready(None) => break,
            Poll::Ready(Some(frame)) => {
                out.extend_from_slice(frame.unwrap().data_ref().expect("a data frame"));
            }
            Poll::Pending => panic!("an always-ready source"),
        }
    }
    assert_eq!(out, b"{\"id\":1}\n{\"id\":2}\n");
}
