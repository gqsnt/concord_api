use bytes::Bytes;
use concord_core::advanced::{DynBody, TransportError};
use futures_core::Stream;
use futures_util::{StreamExt, pin_mut};
use std::hint::black_box;

pub async fn drain_dyn_body(body: &mut DynBody) -> (usize, usize) {
    use http_body_util::BodyExt as _;
    let mut total_bytes = 0usize;
    let mut total_chunks = 0usize;
    loop {
        match body
            .frame()
            .await
            .transpose()
            .expect("benchmark body should not fail")
        {
            Some(frame) => {
                if let Ok(bytes) = frame.into_data() {
                    total_bytes += bytes.len();
                    total_chunks += 1;
                    black_box(bytes);
                }
            }
            None => break,
        }
    }
    (total_bytes, total_chunks)
}

pub async fn drain_transport_stream<S, E>(stream: S) -> (usize, usize)
where
    S: Stream<Item = Result<Bytes, E>>,
{
    let mut total_bytes = 0usize;
    let mut total_chunks = 0usize;
    pin_mut!(stream);
    while let Some(item) = stream.next().await {
        match item {
            Ok(bytes) => {
                total_bytes += bytes.len();
                total_chunks += 1;
                black_box(bytes);
            }
            Err(_) => panic!("benchmark stream should not fail"),
        }
    }
    (total_bytes, total_chunks)
}

pub async fn drain_transport_error_stream<S>(stream: S) -> (usize, usize)
where
    S: Stream<Item = Result<Bytes, TransportError>>,
{
    drain_transport_stream(stream).await
}
