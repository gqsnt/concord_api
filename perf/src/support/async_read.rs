use bytes::Bytes;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

#[derive(Clone)]
pub struct InMemoryAsyncRead {
    bytes: Bytes,
    pos: usize,
}

impl InMemoryAsyncRead {
    pub fn new(bytes: Bytes) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }
}

impl fmt::Debug for InMemoryAsyncRead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryAsyncRead")
            .field("len", &self.len())
            .field("pos", &self.pos)
            .finish()
    }
}

impl AsyncRead for InMemoryAsyncRead {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.pos >= this.bytes.len() {
            return Poll::Ready(Ok(()));
        }
        let remaining = &this.bytes[this.pos..];
        let to_copy = remaining.len().min(buf.remaining());
        buf.put_slice(&remaining[..to_copy]);
        this.pos += to_copy;
        Poll::Ready(Ok(()))
    }
}
