//! JSON lines: any body, cut into chunks, under any bound up to 255 bytes.
#![no_main]

use concord_core::bytes::Bytes;
use concord_core::read::{Incoming, Reader};
use concord_extras::read::{JsonLines, RecordStream};
use futures_util::{FutureExt as _, Stream};
use libfuzzer_sys::fuzz_target;
use std::pin::Pin;
use std::task::{Context, Poll};

/// The first byte is the bound, the next eight are the chunk sizes, the rest is the body.
fn body(data: &[u8]) -> Option<(usize, Incoming)> {
    let (&bound, rest) = data.split_first()?;
    let (cuts, mut body) = rest.split_at(rest.len().min(8));
    let mut chunks = Vec::new();
    for cut in cuts.iter().cycle().take(256) {
        if body.is_empty() {
            break;
        }
        let (head, tail) = body.split_at(usize::from(*cut % 17).min(body.len()));
        chunks.push(Ok::<_, std::io::Error>(Bytes::copy_from_slice(head)));
        body = tail;
    }
    chunks.push(Ok(Bytes::copy_from_slice(body)));
    let source = futures_util::stream::iter(chunks);
    let response =
        concord_core::http::Response::new(concord_core::reqwest::Body::wrap_stream(source));
    Some((
        usize::from(bound),
        Incoming::new(concord_core::reqwest::Response::from(response)),
    ))
}

fuzz_target!(|data: &[u8]| {
    let Some((bound, incoming)) = body(data) else {
        return;
    };
    let mut events: RecordStream<serde_json::Value> = JsonLines::new(bound)
        .read(incoming)
        .now_or_never()
        .expect("the reader returns at once")
        .expect("the reader accepts every head");
    let mut cx = Context::from_waker(futures_util::task::noop_waker_ref());
    // A body that is always ready ends within a bounded number of polls.
    for _ in 0..data.len() + 300 {
        match Pin::new(&mut events).poll_next(&mut cx) {
            Poll::Ready(None) => return,
            Poll::Ready(Some(Ok(_))) => {}
            Poll::Ready(Some(Err(_))) => {
                assert!(matches!(
                    Pin::new(&mut events).poll_next(&mut cx),
                    Poll::Ready(None)
                ));
                return;
            }
            Poll::Pending => {}
        }
    }
    panic!("the stream did not end");
});
