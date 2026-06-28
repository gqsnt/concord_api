use crate::codec::CodecError;
use crate::error::{ApiClientError, ErrorContext};
use crate::media::MediaType;
use crate::transport::{
    StreamBodyLimitError, StreamLimitDirection, TransportByteStream, TransportError,
    TransportResponse,
};
use bytes::Bytes;
use futures_core::Stream;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

pub trait RecordEncoder<T>: Send + 'static {
    fn encode_record(&mut self, value: T) -> Result<Bytes, CodecError>;

    fn finish(&mut self) -> Result<Option<Bytes>, CodecError> {
        Ok(None)
    }
}

pub trait RecordDecoder<T>: Send + 'static {
    fn push_chunk(&mut self, chunk: Bytes) -> Result<Vec<T>, CodecError>;
    fn finish(&mut self) -> Result<Vec<T>, CodecError>;
}

pub trait RecordFormat<T>: MediaType + Send + Sync + 'static {
    fn encoder() -> Box<dyn RecordEncoder<T>>;
    fn decoder() -> Box<dyn RecordDecoder<T>>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NdJson;

impl MediaType for NdJson {
    const CONTENT_TYPE: &'static str = "application/x-ndjson";
}

impl<T> RecordFormat<T> for NdJson
where
    T: Serialize + DeserializeOwned + Send + 'static,
{
    fn encoder() -> Box<dyn RecordEncoder<T>> {
        Box::new(NdJsonEncoder::<T>::new())
    }

    fn decoder() -> Box<dyn RecordDecoder<T>> {
        Box::new(NdJsonDecoder::<T>::new())
    }
}

pub struct RecordBody<T> {
    stream: Pin<Box<dyn Stream<Item = Result<T, CodecError>> + Send>>,
}

impl<T> RecordBody<T> {
    pub fn from_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<T, E>> + Send + 'static,
        E: Into<CodecError> + Send + 'static,
        T: Send + 'static,
    {
        Self {
            stream: Box::pin(MapRecordErrorStream::<S, T, E> {
                inner: Box::pin(stream),
                _marker: PhantomData,
            }),
        }
    }

    pub fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: Send + 'static + Unpin,
        T: Send + 'static,
    {
        Self {
            stream: Box::pin(IterRecordStream::<I::IntoIter, T> {
                iter: iter.into_iter(),
                _marker: PhantomData,
            }),
        }
    }

    pub fn into_transport_stream<F>(self) -> TransportByteStream
    where
        F: RecordFormat<T>,
        T: Send + 'static,
    {
        TransportByteStream::new(RecordEncodeStream::new(self.stream, F::encoder()))
    }

    pub fn into_transport_body<F>(self) -> crate::transport::TransportRequestBody
    where
        F: RecordFormat<T>,
        T: Send + 'static,
    {
        crate::transport::TransportRequestBody::Stream(self.into_transport_stream::<F>())
    }
}

impl<T> fmt::Debug for RecordBody<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<record body>")
    }
}

pub struct RecordStream<T> {
    resp: TransportResponse,
    decoder: Box<dyn RecordDecoder<T>>,
    pending: VecDeque<T>,
    finished: bool,
}

impl<T> RecordStream<T> {
    pub(crate) fn new(resp: TransportResponse, decoder: Box<dyn RecordDecoder<T>>) -> Self {
        Self {
            resp,
            decoder,
            pending: VecDeque::new(),
            finished: false,
        }
    }

    pub fn meta(&self) -> &crate::transport::RequestMeta {
        &self.resp.meta
    }

    pub fn url(&self) -> &url::Url {
        &self.resp.url
    }

    pub fn status(&self) -> http::StatusCode {
        self.resp.status
    }

    pub fn headers(&self) -> &http::HeaderMap {
        &self.resp.headers
    }

    pub fn content_length(&self) -> Option<u64> {
        self.resp.content_length
    }

    pub fn rate_limit(&self) -> &crate::rate_limit::RateLimitPlan {
        &self.resp.rate_limit
    }
}

impl<T: 'static> RecordStream<T> {
    pub async fn next_record(&mut self) -> Result<Option<T>, ApiClientError> {
        loop {
            if let Some(item) = self.pending.pop_front() {
                return Ok(Some(item));
            }
            if self.finished {
                return Ok(None);
            }

            match self.resp.body.next_chunk().await {
                Ok(Some(chunk)) => match self.decoder.push_chunk(chunk) {
                    Ok(records) => self.pending.extend(records),
                    Err(source) => {
                        self.finished = true;
                        return Err(self.decode_error(source));
                    }
                },
                Ok(None) => {
                    self.finished = true;
                    match self.decoder.finish() {
                        Ok(records) => self.pending.extend(records),
                        Err(source) => return Err(self.decode_error(source)),
                    }
                }
                Err(source) => {
                    self.finished = true;
                    return Err(self.body_error(source));
                }
            }
        }
    }

    fn decode_error(&self, _source: CodecError) -> ApiClientError {
        let content_type = self
            .resp
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        ApiClientError::decode_error(
            self.error_context(),
            self.resp.status,
            content_type,
            CodecError::new("record response decode failed"),
        )
    }

    fn body_error(&self, source: TransportError) -> ApiClientError {
        if let Some(limit_error) = source.source_error().downcast_ref::<StreamBodyLimitError>() {
            if matches!(limit_error.direction, StreamLimitDirection::Response) {
                return ApiClientError::ResponseBodyLimitExceeded {
                    ctx: self.error_context(),
                    limit: limit_error.limit,
                };
            }
        }
        ApiClientError::Transport {
            ctx: self.error_context(),
            source: TransportError::with_kind(
                source.kind(),
                std::io::Error::other("record response body read failed"),
            ),
        }
    }

    fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.resp.meta.endpoint,
            method: self.resp.meta.method.clone(),
        }
    }
}

impl<T> fmt::Debug for RecordStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordStream")
            .field("meta", &self.resp.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(&self.resp.url, [] as [&str; 0]),
            )
            .field("status", &self.resp.status)
            .field(
                "headers",
                &crate::debug::RedactedHeaders(&self.resp.headers),
            )
            .field("content_length", &self.resp.content_length)
            .field("rate_limit", &self.resp.rate_limit)
            .field("body", &"<record stream>")
            .finish()
    }
}

struct RecordEncodeStream<T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, CodecError>> + Send>>,
    encoder: Box<dyn RecordEncoder<T>>,
    tail: Option<Bytes>,
    finished: bool,
}

impl<T> RecordEncodeStream<T> {
    fn new(
        inner: Pin<Box<dyn Stream<Item = Result<T, CodecError>> + Send>>,
        encoder: Box<dyn RecordEncoder<T>>,
    ) -> Self {
        Self {
            inner,
            encoder,
            tail: None,
            finished: false,
        }
    }
}

impl<T> Stream for RecordEncodeStream<T>
where
    T: Send + 'static,
{
    type Item = Result<Bytes, CodecError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(tail) = this.tail.take() {
                this.finished = true;
                return Poll::Ready(Some(Ok(tail)));
            }
            if this.finished {
                return Poll::Ready(None);
            }
            match this.inner.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(value))) => match this.encoder.encode_record(value) {
                    Ok(bytes) => return Poll::Ready(Some(Ok(bytes))),
                    Err(_) => {
                        this.finished = true;
                        return Poll::Ready(Some(Err(CodecError::new(
                            "record request encoding failed",
                        ))));
                    }
                },
                Poll::Ready(Some(Err(_error))) => {
                    this.finished = true;
                    return Poll::Ready(Some(Err(CodecError::new(
                        "record request encoding failed",
                    ))));
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    match this.encoder.finish() {
                        Ok(Some(bytes)) => {
                            this.tail = Some(bytes);
                            continue;
                        }
                        Ok(None) => return Poll::Ready(None),
                        Err(_) => {
                            return Poll::Ready(Some(Err(CodecError::new(
                                "record request encoding failed",
                            ))));
                        }
                    }
                }
            }
        }
    }
}

struct IterRecordStream<I, T> {
    iter: I,
    _marker: PhantomData<fn() -> T>,
}

impl<I, T> Stream for IterRecordStream<I, T>
where
    I: Iterator<Item = T> + Send + 'static + Unpin,
    T: Send + 'static,
{
    type Item = Result<T, CodecError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        Poll::Ready(this.iter.next().map(Ok))
    }
}

struct MapRecordErrorStream<S, T, E> {
    inner: Pin<Box<S>>,
    _marker: PhantomData<fn() -> (T, E)>,
}

impl<S, T, E> Stream for MapRecordErrorStream<S, T, E>
where
    S: Stream<Item = Result<T, E>> + Send + 'static,
    E: Into<CodecError> + Send + 'static,
    T: Send + 'static,
{
    type Item = Result<T, CodecError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(value))) => Poll::Ready(Some(Ok(value))),
            Poll::Ready(Some(Err(err))) => {
                let _ = err.into();
                Poll::Ready(Some(Err(CodecError::new("record request encoding failed"))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

struct NdJsonEncoder<T> {
    _marker: PhantomData<fn() -> T>,
}

impl<T> NdJsonEncoder<T> {
    fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T> RecordEncoder<T> for NdJsonEncoder<T>
where
    T: Serialize + Send + 'static,
{
    fn encode_record(&mut self, value: T) -> Result<Bytes, CodecError> {
        let mut bytes = serde_json::to_vec(&value)
            .map_err(|_| CodecError::new("record request encoding failed"))?;
        bytes.push(b'\n');
        Ok(Bytes::from(bytes))
    }
}

struct NdJsonDecoder<T> {
    buffer: Vec<u8>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> NdJsonDecoder<T> {
    fn new() -> Self {
        Self {
            buffer: Vec::new(),
            _marker: PhantomData,
        }
    }

    fn decode_line(&self, mut line: Vec<u8>) -> Result<T, CodecError>
    where
        T: DeserializeOwned,
    {
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            return Err(CodecError::new("record response decode failed"));
        }
        serde_json::from_slice(&line).map_err(|_| CodecError::new("record response decode failed"))
    }

    fn parse_available(&mut self, finalizing: bool) -> Result<Vec<T>, CodecError>
    where
        T: DeserializeOwned,
    {
        let mut out = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=pos).collect();
            line.pop();
            out.push(self.decode_line(line)?);
        }
        if finalizing && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            out.push(self.decode_line(line)?);
        }
        Ok(out)
    }
}

impl<T> RecordDecoder<T> for NdJsonDecoder<T>
where
    T: DeserializeOwned + Send + 'static,
{
    fn push_chunk(&mut self, chunk: Bytes) -> Result<Vec<T>, CodecError> {
        self.buffer.extend_from_slice(&chunk);
        self.parse_available(false)
    }

    fn finish(&mut self) -> Result<Vec<T>, CodecError> {
        self.parse_available(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    struct Item {
        id: u32,
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn poll_next(stream: &mut TransportByteStream) -> Poll<Option<Result<Bytes, TransportError>>> {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        Pin::new(stream).poll_next(&mut cx)
    }

    #[test]
    fn ndjson_request_encoding_appends_one_newline_per_record() {
        let body = RecordBody::from_iter(vec![Item { id: 1 }, Item { id: 2 }]);
        let mut stream = body.into_transport_stream::<NdJson>();
        let first = match poll_next(&mut stream) {
            Poll::Ready(Some(Ok(bytes))) => bytes,
            other => panic!("unexpected stream poll result: {other:?}"),
        };
        let second = match poll_next(&mut stream) {
            Poll::Ready(Some(Ok(bytes))) => bytes,
            other => panic!("unexpected stream poll result: {other:?}"),
        };
        assert_eq!(first, Bytes::from_static(b"{\"id\":1}\n"));
        assert_eq!(second, Bytes::from_static(b"{\"id\":2}\n"));
        assert!(matches!(poll_next(&mut stream), Poll::Ready(None)));
    }

    #[test]
    fn ndjson_decoder_streams_incrementally_and_accepts_final_line_without_newline() {
        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let first = decoder
            .push_chunk(Bytes::from_static(b"{\"id\":1}\n{\"id\":"))
            .expect("first chunk");
        assert_eq!(first, vec![Item { id: 1 }]);
        let second = decoder
            .push_chunk(Bytes::from_static(b"2}"))
            .expect("second chunk");
        assert!(second.is_empty());
        let final_items = decoder.finish().expect("finish");
        assert_eq!(final_items, vec![Item { id: 2 }]);
    }

    #[test]
    fn ndjson_decoder_rejects_blank_lines_invalid_utf8_and_invalid_json() {
        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let err = decoder
            .push_chunk(Bytes::from_static(b"{\"id\":1}\n\n{\"id\":2}\n"))
            .expect_err("blank line should fail");
        assert_eq!(err.to_string(), "record response decode failed");

        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let err = decoder
            .push_chunk(Bytes::from_static(b"\xff\n"))
            .expect_err("invalid utf-8 should fail");
        assert_eq!(err.to_string(), "record response decode failed");

        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let err = decoder
            .push_chunk(Bytes::from_static(b"{\"id\":1,}\n"))
            .expect_err("invalid json should fail");
        assert_eq!(err.to_string(), "record response decode failed");
    }

    #[test]
    fn record_stream_debug_is_body_free_and_does_not_require_debug_items() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq)]
        struct NoDebug;

        struct Body {
            next: Option<Bytes>,
        }

        impl crate::transport::TransportBody for Body {
            fn next_chunk<'a>(
                &'a mut self,
            ) -> Pin<
                Box<
                    dyn std::future::Future<Output = Result<Option<Bytes>, TransportError>>
                        + Send
                        + 'a,
                >,
            > {
                Box::pin(async move { Ok(self.next.take()) })
            }
        }

        let resp = TransportResponse {
            meta: crate::transport::RequestMeta {
                endpoint: "Records",
                method: http::Method::GET,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
            url: url::Url::parse("https://example.com/records").expect("url"),
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            content_length: Some(0),
            rate_limit: Default::default(),
            body: Box::new(Body { next: None }),
        };
        let stream: RecordStream<NoDebug> = RecordStream::new(resp, NdJson::decoder());
        let rendered = format!("{stream:?}");
        assert!(rendered.contains("<record stream>"));
        assert!(!rendered.contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
    }

    #[test]
    fn record_body_from_stream_hides_payload_errors() {
        let body = RecordBody::from_stream(ErrorStream::new());
        let mut stream = body.into_transport_stream::<NdJson>();
        let error = match poll_next(&mut stream) {
            Poll::Ready(Some(Err(error))) => error,
            other => panic!("unexpected stream poll result: {other:?}"),
        };
        assert!(
            !error
                .to_string()
                .contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR")
        );
    }

    #[tokio::test]
    async fn record_stream_sanitizes_decoder_error_text() {
        struct Body {
            next: Option<Bytes>,
        }

        impl crate::transport::TransportBody for Body {
            fn next_chunk<'a>(
                &'a mut self,
            ) -> Pin<
                Box<
                    dyn std::future::Future<Output = Result<Option<Bytes>, TransportError>>
                        + Send
                        + 'a,
                >,
            > {
                Box::pin(async move { Ok(self.next.take()) })
            }
        }

        struct UnsafeDecoder;

        impl RecordDecoder<Item> for UnsafeDecoder {
            fn push_chunk(&mut self, _chunk: Bytes) -> Result<Vec<Item>, CodecError> {
                Err(CodecError::new("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"))
            }

            fn finish(&mut self) -> Result<Vec<Item>, CodecError> {
                Ok(Vec::new())
            }
        }

        let resp = TransportResponse {
            meta: crate::transport::RequestMeta {
                endpoint: "Records",
                method: http::Method::GET,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
            url: url::Url::parse("https://example.com/records").expect("url"),
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            content_length: Some(1),
            rate_limit: Default::default(),
            body: Box::new(Body {
                next: Some(Bytes::from_static(b"{\"id\":1}\n")),
            }),
        };
        let mut stream = RecordStream::new(resp, Box::new(UnsafeDecoder));
        let err = stream
            .next_record()
            .await
            .expect_err("decoder failure should surface");
        let debug = format!("{err:?}");
        let display = err.to_string();
        assert!(!debug.contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
        assert!(!display.contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
        assert!(display.contains("record response decode failed"));
    }

    struct ErrorStream {
        done: Cell<bool>,
    }

    impl ErrorStream {
        fn new() -> Self {
            Self {
                done: Cell::new(false),
            }
        }
    }

    impl Stream for ErrorStream {
        type Item = Result<Item, CodecError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = self.get_mut();
            if this.done.get() {
                return Poll::Ready(None);
            }
            this.done.set(true);
            Poll::Ready(Some(Err(CodecError::new(
                "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR",
            ))))
        }
    }
}
