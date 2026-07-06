use bytes::Bytes;
use concord_core::advanced::{StreamBodyError, TransportBody, TransportError};
use futures_core::Stream;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub fn filled_bytes(len: usize, byte: u8) -> Bytes {
    Bytes::from(vec![byte; len])
}

pub fn patterned_bytes(len: usize) -> Bytes {
    let mut data = Vec::with_capacity(len);
    for idx in 0..len {
        data.push((idx % 251) as u8);
    }
    Bytes::from(data)
}

pub fn chunked_bytes(payload: Bytes, chunk_size: usize) -> Vec<Bytes> {
    assert!(chunk_size > 0, "chunk size must be non-zero");
    payload
        .chunks(chunk_size)
        .map(Bytes::copy_from_slice)
        .collect()
}

#[derive(Clone, Copy, Default)]
pub struct EmptyBody;

impl fmt::Debug for EmptyBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EmptyBody")
    }
}

impl TransportBody for EmptyBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(None) })
    }
}

#[derive(Clone)]
pub struct FixedBody {
    body: Option<Bytes>,
}

impl FixedBody {
    pub fn new(body: Bytes) -> Self {
        Self { body: Some(body) }
    }

    pub fn len(&self) -> usize {
        self.body.as_ref().map_or(0, Bytes::len)
    }
}

impl fmt::Debug for FixedBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FixedBody(<{} bytes>)", self.len())
    }
}

impl TransportBody for FixedBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.body.take()) })
    }
}

#[derive(Clone)]
pub struct ChunkedBodyStream {
    chunks: VecDeque<Bytes>,
}

impl ChunkedBodyStream {
    pub fn new(chunks: impl IntoIterator<Item = Bytes>) -> Self {
        Self {
            chunks: chunks.into_iter().collect(),
        }
    }

    pub fn from_payload(payload: Bytes, chunk_size: usize) -> Self {
        Self::new(chunked_bytes(payload, chunk_size))
    }
}

impl fmt::Debug for ChunkedBodyStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChunkedBodyStream")
            .field("chunks", &self.chunks.len())
            .finish()
    }
}

impl Stream for ChunkedBodyStream {
    type Item = Result<Bytes, StreamBodyError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().chunks.pop_front().map(Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Builder;

    #[test]
    fn empty_body_returns_eof() {
        let rt = Builder::new_current_thread()
            .build()
            .expect("test runtime");
        let mut body = EmptyBody;
        let chunk = rt.block_on(async { body.next_chunk().await.expect("empty body") });
        assert!(chunk.is_none());
    }

    #[test]
    fn chunked_body_stream_debug_is_safe() {
        let body = ChunkedBodyStream::from_payload(Bytes::from_static(b"abc"), 2);
        assert_eq!(format!("{:?}", body), "ChunkedBodyStream { chunks: 2 }");
    }
}
