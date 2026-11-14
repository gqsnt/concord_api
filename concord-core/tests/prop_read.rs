//! C5.6/C5.11 buffering budgets and early termination, with lazy chunk allocation.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use concord_core::{
    __private::read_body,
    bytes::Bytes,
    error::Kind,
    http,
    read::{Incoming, ReadError},
    transport::TryInfo,
    Error, Reader,
};
use futures_util::stream;
use proptest::prelude::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
struct Buffered(usize);
impl Reader<Bytes> for Buffered {
    async fn read(&self, incoming: Incoming) -> Result<Bytes, ReadError> {
        incoming.bytes(self.0).await
    }
}
proptest! {
    #![proptest_config(ProptestConfig{failure_persistence:None,..Default::default()})]
    #[test]
    /// Also proves C5.11.
    fn c5_6_buffering_stops_at_limit_and_keeps_bounded_capacity(
        lengths in proptest::collection::vec(prop_oneof![0usize..8192,Just(65537)],0..20),
        limit in 0usize..131073,announced in prop::option::of(0u64..262144),
    ){
        let polls = Arc::new(AtomicUsize::new(0));
        let counted = polls.clone();
        // Each piece is allocated only when the reader asks: the test never pre-buffers the body.
        let source = stream::iter(lengths.clone()).map(move |length| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; length]))
        });
        let mut response = http::Response::new(concord_core::reqwest::Body::wrap_stream(source));
        if let Some(n) = announced {
            response
                .headers_mut()
                .insert("content-length", n.to_string().parse().unwrap());
        }
        let response: concord_core::reqwest::Response = response.into();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let actual: Result<Bytes, Error> = rt.block_on(read_body(
            &Buffered(limit),
            response,
            &TryInfo::new("props.memory", 1),
        ));
        let early = announced.is_some_and(|n| n > limit as u64);
        let mut kept = 0;
        let mut asked = 0;
        let mut limited = early;
        if !early {
            for n in &lengths {
                asked += 1;
                if *n > limit - kept {
                    limited = true;
                    break;
                }
                kept += n;
            }
        }
        prop_assert_eq!(polls.load(Ordering::SeqCst), asked);
        if limited {
            let e = actual.unwrap_err();
            prop_assert_eq!(e.kind(), Kind::Mismatch);
            prop_assert_eq!(
                e.body().map(<[u8]>::len),
                if kept == 0 {
                    None
                } else {
                    Some(kept.min(64 * 1024))
                }
            );
        } else if announced.is_some_and(|n| n > kept as u64) {
            let e = actual.unwrap_err();
            prop_assert_eq!(e.kind(), Kind::Transport);
            prop_assert_eq!(e.class(), Some(concord_core::error::Class::Body));
        } else {
            let bytes = actual.unwrap();
            prop_assert_eq!(bytes.len(), kept);
            // Bytes -> BytesMut retains the Vec allocation without copying, exposing its capacity.
            let capacity = if bytes.is_empty() {
                0
            } else {
                bytes.try_into_mut().unwrap().capacity()
            };
            prop_assert!(
                capacity <= limit,
                "capacity {} exceeds limit {}",
                capacity,
                limit
            );
        }
        // End the same lazy stream after each accepted piece. This exposes the
        // retained allocation at every buffering state, including before a limit error.
        let mut total = 0usize;
        for end in 0..=lengths.len() {
            if end > 0 {
                total += lengths[end - 1];
            }
            if total > limit {
                break;
            }
            let source = stream::iter(lengths[..end].to_vec())
                .map(|n| Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; n])));
            let response: concord_core::reqwest::Response =
                http::Response::new(concord_core::reqwest::Body::wrap_stream(source)).into();
            let bytes = rt.block_on(Incoming::new(response).bytes(limit)).unwrap();
            let capacity = if bytes.is_empty() {
                0
            } else {
                bytes.try_into_mut().unwrap().capacity()
            };
            prop_assert!(
                capacity <= limit,
                "prefix {} capacity {} exceeds {}",
                end,
                capacity,
                limit
            );
        }
    }
}
use futures_util::StreamExt as _;

#[tokio::test]
async fn c5_4_a_single_frame_is_returned_without_copying() {
    let frame = Bytes::from(vec![b'x'; 1024 * 1024]);
    let pointer = frame.as_ptr();
    for announced in [None, Some(frame.len())] {
        let mut response =
            http::Response::new(concord_core::reqwest::Body::wrap_stream(stream::iter([
                Ok::<_, std::io::Error>(frame.clone()),
            ])));
        if let Some(size) = announced {
            response.headers_mut().insert("content-length", size.into());
        }
        let value = Incoming::new(response.into())
            .bytes(frame.len())
            .await
            .unwrap();
        assert_eq!(value.as_ptr(), pointer);
        assert_eq!(value.len(), frame.len());
    }
}

#[tokio::test]
async fn c5_4_joining_reserves_the_announced_size_within_the_64_kib_floor() {
    let source =
        stream::iter((0..3).map(|_| Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; 1000]))));
    let mut response = http::Response::new(concord_core::reqwest::Body::wrap_stream(source));
    response
        .headers_mut()
        .insert("content-length", 3000usize.into());
    let bytes = Incoming::new(response.into()).bytes(4096).await.unwrap();
    assert_eq!(bytes.len(), 3000);
    assert_eq!(bytes.try_into_mut().unwrap().capacity(), 3000);
}

#[tokio::test]
async fn c5_4_an_unrepresentable_announced_allocation_remains_a_body_error() {
    let source = stream::iter([
        Ok::<_, std::io::Error>(Bytes::from_static(b"a")),
        Ok(Bytes::from_static(b"b")),
    ]);
    let mut response = http::Response::new(concord_core::reqwest::Body::wrap_stream(source));
    response
        .headers_mut()
        .insert("content-length", usize::MAX.into());
    let error: Error = read_body(
        &Buffered(usize::MAX),
        concord_core::reqwest::Response::from(response),
        &TryInfo::new("props.huge", 1),
    )
    .await
    .unwrap_err();
    assert_eq!(error.class(), Some(concord_core::error::Class::Body));
}
