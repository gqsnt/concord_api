use crate::codec::CodecError;
use crate::codec::ContentType;
use crate::error::{ApiClientError, ErrorContext};
use crate::transport::{
    StreamBodyLimitError, StreamLimitDirection, TransportByteStream, TransportError,
    TransportResponse,
};
use bytes::{Buf, Bytes, BytesMut};
use csv::{ByteRecord, WriterBuilder};
use csv_core::{ReadRecordResult, Reader as CsvReader, ReaderBuilder as CsvReaderBuilder};
use futures_core::Stream;
use serde::{Serialize, de::DeserializeOwned};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Write};
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

pub trait RecordFormat<T>: ContentType + Send + Sync + 'static {
    fn encoder() -> Box<dyn RecordEncoder<T>>;
    fn decoder() -> Box<dyn RecordDecoder<T>>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Csv<Cfg>(PhantomData<Cfg>);

pub trait CsvConfig {
    const DELIMITER: u8;
    const HAS_HEADERS: bool;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CsvCommaDelim;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CsvSemicolonDelim;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CsvTabDelim;

impl CsvConfig for CsvCommaDelim {
    const DELIMITER: u8 = b',';
    const HAS_HEADERS: bool = true;
}

impl CsvConfig for CsvSemicolonDelim {
    const DELIMITER: u8 = b';';
    const HAS_HEADERS: bool = true;
}

impl CsvConfig for CsvTabDelim {
    const DELIMITER: u8 = b'\t';
    const HAS_HEADERS: bool = true;
}

impl<Cfg> ContentType for Csv<Cfg>
where
    Cfg: CsvConfig + Send + Sync + 'static,
{
    const CONTENT_TYPE: &'static str = "text/csv";
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NdJson;

impl ContentType for NdJson {
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

impl<T, Cfg> RecordFormat<T> for Csv<Cfg>
where
    T: Serialize + DeserializeOwned + Send + 'static,
    Cfg: CsvConfig + Send + Sync + 'static,
{
    fn encoder() -> Box<dyn RecordEncoder<T>> {
        Box::new(CsvEncoder::<T, Cfg>::new())
    }

    fn decoder() -> Box<dyn RecordDecoder<T>> {
        Box::new(CsvDecoder::<T, Cfg>::new())
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
    buffer: BytesMut,
    _marker: PhantomData<fn() -> T>,
}

impl<T> NdJsonDecoder<T> {
    fn new() -> Self {
        Self {
            buffer: BytesMut::new(),
            _marker: PhantomData,
        }
    }

    fn decode_line(&self, mut line: BytesMut) -> Result<T, CodecError>
    where
        T: DeserializeOwned,
    {
        if line.last() == Some(&b'\r') {
            line.truncate(line.len() - 1);
        }
        if line.is_empty() {
            return Err(CodecError::new("record response decode failed"));
        }
        serde_json::from_slice(line.as_ref())
            .map_err(|_| CodecError::new("record response decode failed"))
    }

    fn parse_available(&mut self, finalizing: bool) -> Result<Vec<T>, CodecError>
    where
        T: DeserializeOwned,
    {
        let mut out = Vec::new();
        while let Some(pos) = memchr::memchr(b'\n', &self.buffer) {
            let mut line = self.buffer.split_to(pos + 1);
            line.truncate(pos);
            out.push(self.decode_line(line)?);
        }
        if finalizing && !self.buffer.is_empty() {
            let line = self.buffer.split_to(self.buffer.len());
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

#[derive(Default)]
struct CsvOutputBuffer {
    bytes: RefCell<Vec<u8>>,
}

impl CsvOutputBuffer {
    fn take_bytes(&self) -> Bytes {
        Bytes::from(std::mem::take(&mut *self.bytes.borrow_mut()))
    }
}

impl Write for CsvOutputBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.get_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct CsvEncoder<T, Cfg> {
    writer: csv::Writer<CsvOutputBuffer>,
    _marker: PhantomData<fn() -> (T, Cfg)>,
}

impl<T, Cfg> CsvEncoder<T, Cfg>
where
    Cfg: CsvConfig,
{
    fn new() -> Self {
        let mut builder = WriterBuilder::new();
        builder
            .delimiter(Cfg::DELIMITER)
            .has_headers(Cfg::HAS_HEADERS);
        Self {
            writer: builder.from_writer(CsvOutputBuffer::default()),
            _marker: PhantomData,
        }
    }

    fn take_new_bytes(&mut self) -> Result<Bytes, CodecError> {
        self.writer
            .flush()
            .map_err(|_| CodecError::new("record request encoding failed"))?;
        let bytes = self.writer.get_ref().take_bytes();
        if bytes.is_empty() {
            return Ok(Bytes::new());
        }
        Ok(bytes)
    }
}

impl<T, Cfg> RecordEncoder<T> for CsvEncoder<T, Cfg>
where
    T: Serialize + Send + 'static,
    Cfg: CsvConfig + Send + Sync + 'static,
{
    fn encode_record(&mut self, value: T) -> Result<Bytes, CodecError> {
        self.writer
            .serialize(value)
            .map_err(|_| CodecError::new("record request encoding failed"))?;
        self.take_new_bytes()
    }

    fn finish(&mut self) -> Result<Option<Bytes>, CodecError> {
        let bytes = self.take_new_bytes()?;
        if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(bytes))
        }
    }
}

struct CsvDecoder<T, Cfg> {
    reader: CsvReader,
    buffer: BytesMut,
    data: Vec<u8>,
    data_len: usize,
    ends: Vec<usize>,
    ends_len: usize,
    header: Option<ByteRecord>,
    _marker: PhantomData<fn() -> (T, Cfg)>,
}

impl<T, Cfg> CsvDecoder<T, Cfg>
where
    Cfg: CsvConfig,
{
    fn new() -> Self {
        let mut builder = CsvReaderBuilder::new();
        builder.delimiter(Cfg::DELIMITER);
        Self {
            reader: builder.build(),
            buffer: BytesMut::new(),
            data: Vec::new(),
            data_len: 0,
            ends: Vec::new(),
            ends_len: 0,
            header: None,
            _marker: PhantomData,
        }
    }

    fn ensure_capacity(&mut self, input_len: usize) {
        let needed = input_len.max(1024);
        if self.data.len() < self.data_len + needed {
            self.data.resize(self.data_len + needed, 0);
        }
        if self.ends.len() < self.ends_len + needed {
            self.ends.resize(self.ends_len + needed, 0);
        }
    }

    fn grow_buffers(&mut self, input_len: usize) {
        let grow = input_len.max(1024);
        self.data.resize(self.data.len() + grow, 0);
        self.ends.resize(self.ends.len() + grow, 0);
    }

    fn clear_current_record(&mut self) {
        self.data_len = 0;
        self.ends_len = 0;
    }

    fn finish_record(&self) -> ByteRecord {
        let mut record = ByteRecord::with_capacity(self.data_len, self.ends_len);
        let mut start = 0;
        for &end in &self.ends[..self.ends_len] {
            record.push_field(&self.data[start..end]);
            start = end;
        }
        record
    }

    fn sanitize_decode_error(&self) -> CodecError {
        CodecError::new("record response decode failed")
    }

    fn parse_available(&mut self, finalizing: bool) -> Result<Vec<T>, CodecError>
    where
        T: DeserializeOwned,
    {
        let mut out = Vec::new();
        loop {
            if self.buffer.is_empty() {
                if !finalizing {
                    break;
                }
                if self.data_len == 0 && self.ends_len == 0 {
                    break;
                }
            }

            self.ensure_capacity(self.buffer.len());
            let input = if self.buffer.is_empty() {
                &[][..]
            } else {
                self.buffer.as_ref()
            };
            let (result, bytes_read, bytes_written, end_positions) = self.reader.read_record(
                input,
                &mut self.data[self.data_len..],
                &mut self.ends[self.ends_len..],
            );
            self.buffer.advance(bytes_read);
            self.data_len += bytes_written;
            self.ends_len += end_positions;

            match result {
                ReadRecordResult::Record => {
                    let record = self.finish_record();
                    if record.is_empty() {
                        // Empty records are ignored.
                    } else if self.header.is_none() && Cfg::HAS_HEADERS {
                        self.header = Some(record);
                    } else {
                        let decoded = record
                            .deserialize(self.header.as_ref())
                            .map_err(|_| self.sanitize_decode_error())?;
                        out.push(decoded);
                    }
                    self.clear_current_record();
                }
                ReadRecordResult::InputEmpty => {
                    break;
                }
                ReadRecordResult::OutputFull | ReadRecordResult::OutputEndsFull => {
                    self.grow_buffers(self.buffer.len());
                    continue;
                }
                ReadRecordResult::End => {
                    break;
                }
            }
        }
        if finalizing && self.data_len > 0 {
            loop {
                self.ensure_capacity(0);
                let (result, bytes_written, end_positions) = {
                    let (result, _bytes_read, bytes_written, end_positions) =
                        self.reader.read_record(
                            &[],
                            &mut self.data[self.data_len..],
                            &mut self.ends[self.ends_len..],
                        );
                    (result, bytes_written, end_positions)
                };
                self.data_len += bytes_written;
                self.ends_len += end_positions;
                match result {
                    ReadRecordResult::Record => {
                        let record = self.finish_record();
                        if !record.is_empty() {
                            if self.header.is_none() && Cfg::HAS_HEADERS {
                                self.header = Some(record);
                            } else {
                                let decoded = record
                                    .deserialize(self.header.as_ref())
                                    .map_err(|_| self.sanitize_decode_error())?;
                                out.push(decoded);
                            }
                        }
                        self.clear_current_record();
                        break;
                    }
                    ReadRecordResult::InputEmpty | ReadRecordResult::End => {
                        self.clear_current_record();
                        break;
                    }
                    ReadRecordResult::OutputFull | ReadRecordResult::OutputEndsFull => {
                        self.grow_buffers(0);
                        continue;
                    }
                }
            }
        }
        Ok(out)
    }
}

impl<T, Cfg> RecordDecoder<T> for CsvDecoder<T, Cfg>
where
    T: DeserializeOwned + Send + 'static,
    Cfg: CsvConfig + Send + Sync + 'static,
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

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    struct CsvRow {
        id: u32,
        name: String,
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct HeaderlessPipe;

    impl CsvConfig for HeaderlessPipe {
        const DELIMITER: u8 = b'|';
        const HAS_HEADERS: bool = false;
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

    fn collect_bytes(stream: &mut TransportByteStream) -> Bytes {
        let mut out = Vec::new();
        loop {
            match poll_next(stream) {
                Poll::Ready(Some(Ok(bytes))) => out.extend_from_slice(&bytes),
                Poll::Ready(Some(Err(error))) => panic!("unexpected transport error: {error:?}"),
                Poll::Ready(None) => return Bytes::from(out),
                Poll::Pending => panic!("transport stream should not pend in unit tests"),
            }
        }
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
            .push_chunk(Bytes::from_static(b"\n"))
            .expect_err("empty line should fail");
        assert_eq!(err.to_string(), "record response decode failed");

        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let err = decoder
            .push_chunk(Bytes::from_static(b"\r\n"))
            .expect_err("empty crlf line should fail");
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
    fn ndjson_decoder_emits_multiple_records_from_one_chunk() {
        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let records = decoder
            .push_chunk(Bytes::from_static(b"{\"id\":1}\n{\"id\":2}\n"))
            .expect("chunk should decode");
        assert_eq!(records, vec![Item { id: 1 }, Item { id: 2 }]);
        assert!(decoder.finish().expect("finish").is_empty());
    }

    #[test]
    fn ndjson_decoder_waits_for_complete_record_across_chunks() {
        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        assert!(
            decoder
                .push_chunk(Bytes::from_static(b"{\"id\":"))
                .expect("first chunk")
                .is_empty()
        );
        let records = decoder
            .push_chunk(Bytes::from_static(b"1}\n"))
            .expect("second chunk");
        assert_eq!(records, vec![Item { id: 1 }]);
        assert!(decoder.finish().expect("finish").is_empty());
    }

    #[test]
    fn ndjson_decoder_accepts_crlf_and_final_line_without_newline() {
        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let records = decoder
            .push_chunk(Bytes::from_static(b"{\"id\":1}\r\n"))
            .expect("crlf chunk");
        assert_eq!(records, vec![Item { id: 1 }]);
        let final_records = decoder
            .push_chunk(Bytes::from_static(b"{\"id\":2}"))
            .expect("final partial");
        assert!(final_records.is_empty());
        assert_eq!(decoder.finish().expect("finish"), vec![Item { id: 2 }]);
    }

    #[test]
    fn ndjson_decoder_invalid_json_errors_are_sanitized() {
        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let err = decoder
            .push_chunk(Bytes::from_static(
                b"{\"token\":\"SECRET_RECORD_SENTINEL\"\n",
            ))
            .expect_err("invalid json should fail");
        let debug = format!("{err:?}");
        let display = err.to_string();
        assert!(!debug.contains("SECRET_RECORD_SENTINEL"));
        assert!(!display.contains("SECRET_RECORD_SENTINEL"));
        assert_eq!(display, "record response decode failed");
    }

    #[test]
    fn ndjson_decoder_handles_large_batches() {
        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let mut input = String::new();
        for id in 0..1000u32 {
            input.push_str(&format!("{{\"id\":{id}}}\n"));
        }
        let records = decoder
            .push_chunk(Bytes::from(input))
            .expect("large batch should decode");
        assert_eq!(records.len(), 1000);
        assert_eq!(records[0], Item { id: 0 });
        assert_eq!(records[999], Item { id: 999 });
        assert!(decoder.finish().expect("finish").is_empty());
    }

    #[test]
    fn ndjson_decoder_handles_bytewise_chunks() {
        let mut decoder: Box<dyn RecordDecoder<Item>> = NdJson::decoder();
        let input = b"{\"id\":1}\n";
        for byte in &input[..input.len() - 1] {
            assert!(
                decoder
                    .push_chunk(Bytes::copy_from_slice(&[*byte]))
                    .expect("partial byte")
                    .is_empty()
            );
        }
        let records = decoder
            .push_chunk(Bytes::copy_from_slice(&input[input.len() - 1..]))
            .expect("newline byte");
        assert_eq!(records, vec![Item { id: 1 }]);
        assert!(decoder.finish().expect("finish").is_empty());
    }

    #[test]
    fn csv_content_type_and_config_constants_are_exposed() {
        assert_eq!(
            Csv::<CsvCommaDelim>::try_header_value().expect("csv content type"),
            http::HeaderValue::from_static("text/csv")
        );
        assert_eq!(CsvCommaDelim::DELIMITER, b',');
        assert_eq!(CsvCommaDelim::HAS_HEADERS, true);
        assert_eq!(CsvSemicolonDelim::DELIMITER, b';');
        assert_eq!(CsvSemicolonDelim::HAS_HEADERS, true);
        assert_eq!(CsvTabDelim::DELIMITER, b'\t');
        assert_eq!(CsvTabDelim::HAS_HEADERS, true);
    }

    #[test]
    fn csv_request_encoding_with_headers_and_without_headers_is_incremental() {
        let mut headered = RecordBody::from_iter(vec![CsvRow {
            id: 1,
            name: "Ada".to_string(),
        }])
        .into_transport_stream::<Csv<CsvCommaDelim>>();
        let headered_bytes = match poll_next(&mut headered) {
            Poll::Ready(Some(Ok(bytes))) => bytes,
            other => panic!("unexpected headered poll result: {other:?}"),
        };
        let headered_text = String::from_utf8(headered_bytes.to_vec()).expect("utf8");
        assert!(headered_text.contains("id"));
        assert!(headered_text.contains("name"));
        assert!(headered_text.contains("Ada"));
        assert!(matches!(poll_next(&mut headered), Poll::Ready(None)));

        let mut headerless = RecordBody::from_iter(vec![CsvRow {
            id: 2,
            name: "Bea".to_string(),
        }])
        .into_transport_stream::<Csv<HeaderlessPipe>>();
        let headerless_bytes = match poll_next(&mut headerless) {
            Poll::Ready(Some(Ok(bytes))) => bytes,
            other => panic!("unexpected headerless poll result: {other:?}"),
        };
        let headerless_text = String::from_utf8(headerless_bytes.to_vec()).expect("utf8");
        assert!(!headerless_text.starts_with("id"));
        assert!(headerless_text.contains("2"));
        assert!(headerless_text.contains("Bea"));
        assert!(matches!(poll_next(&mut headerless), Poll::Ready(None)));
    }

    #[test]
    fn csv_request_encoding_does_not_accumulate_emitted_bytes() {
        let expected = Bytes::from_static(b"7|repeat\n");
        let mut stream = RecordBody::from_iter((0..128).map(|_| CsvRow {
            id: 7,
            name: "repeat".to_string(),
        }))
        .into_transport_stream::<Csv<HeaderlessPipe>>();

        let mut count = 0usize;
        loop {
            match poll_next(&mut stream) {
                Poll::Ready(Some(Ok(bytes))) => {
                    assert_eq!(bytes, expected);
                    count += 1;
                }
                Poll::Ready(Some(Err(error))) => {
                    panic!("unexpected csv request encoding error: {error:?}")
                }
                Poll::Ready(None) => break,
                Poll::Pending => panic!("transport stream should not pend in unit tests"),
            }
        }
        assert_eq!(count, 128);
    }

    #[test]
    fn csv_semicolon_and_tab_round_trip() {
        let mut semicolon = RecordBody::from_iter(vec![CsvRow {
            id: 3,
            name: "Cleo".to_string(),
        }])
        .into_transport_stream::<Csv<CsvSemicolonDelim>>();
        let semicolon_bytes = collect_bytes(&mut semicolon);
        let mut semicolon_decoder: Box<dyn RecordDecoder<CsvRow>> =
            Csv::<CsvSemicolonDelim>::decoder();
        assert_eq!(
            semicolon_decoder
                .push_chunk(semicolon_bytes)
                .expect("semicolon decode"),
            vec![CsvRow {
                id: 3,
                name: "Cleo".to_string(),
            }]
        );
        assert!(semicolon_decoder.finish().expect("finish").is_empty());

        let mut tab = RecordBody::from_iter(vec![CsvRow {
            id: 4,
            name: "Drew".to_string(),
        }])
        .into_transport_stream::<Csv<CsvTabDelim>>();
        let tab_bytes = collect_bytes(&mut tab);
        let mut tab_decoder: Box<dyn RecordDecoder<CsvRow>> = Csv::<CsvTabDelim>::decoder();
        assert_eq!(
            tab_decoder.push_chunk(tab_bytes).expect("tab decode"),
            vec![CsvRow {
                id: 4,
                name: "Drew".to_string(),
            }]
        );
        assert!(tab_decoder.finish().expect("finish").is_empty());
    }

    #[test]
    fn csv_response_decoder_handles_headers_headerless_and_chunk_boundaries() {
        let mut decoder: Box<dyn RecordDecoder<CsvRow>> = Csv::<CsvCommaDelim>::decoder();
        assert!(
            decoder
                .push_chunk(Bytes::from_static(b"id,name\n1,Ada"))
                .expect("first chunk")
                .is_empty()
        );
        let records = decoder
            .push_chunk(Bytes::from_static(b"\n2,Bob\n"))
            .expect("second chunk");
        assert_eq!(
            records,
            vec![
                CsvRow {
                    id: 1,
                    name: "Ada".to_string(),
                },
                CsvRow {
                    id: 2,
                    name: "Bob".to_string(),
                },
            ]
        );
        assert!(decoder.finish().expect("finish").is_empty());

        let mut headerless: Box<dyn RecordDecoder<CsvRow>> = Csv::<HeaderlessPipe>::decoder();
        assert!(
            headerless
                .push_chunk(Bytes::from_static(b"3|Cleo"))
                .expect("first headerless chunk")
                .is_empty()
        );
        let records = headerless
            .push_chunk(Bytes::from_static(b"\n4|Drew\n"))
            .expect("second headerless chunk");
        assert_eq!(
            records,
            vec![
                CsvRow {
                    id: 3,
                    name: "Cleo".to_string(),
                },
                CsvRow {
                    id: 4,
                    name: "Drew".to_string(),
                },
            ]
        );
        assert!(headerless.finish().expect("finish").is_empty());
    }

    #[test]
    fn csv_response_decoder_supports_semicolon_tab_quotes_crlf_and_final_rows() {
        let mut semicolon: Box<dyn RecordDecoder<CsvRow>> = Csv::<CsvSemicolonDelim>::decoder();
        let records = semicolon
            .push_chunk(Bytes::from_static(
                b"id;name\r\n1;\"A,da\"\r\n2;\"He said \"\"hi\"\"\"\r\n3;\"line1\r\nline2\"\r\n",
            ))
            .expect("semicolon chunk");
        assert_eq!(
            records,
            vec![
                CsvRow {
                    id: 1,
                    name: "A,da".to_string(),
                },
                CsvRow {
                    id: 2,
                    name: "He said \"hi\"".to_string(),
                },
                CsvRow {
                    id: 3,
                    name: "line1\r\nline2".to_string(),
                },
            ]
        );
        assert!(semicolon.finish().expect("finish").is_empty());

        let mut tab: Box<dyn RecordDecoder<CsvRow>> = Csv::<CsvTabDelim>::decoder();
        assert!(
            tab.push_chunk(Bytes::from_static(b"id\tname\n5\tEve"))
                .expect("tab first chunk")
                .is_empty()
        );
        let records = tab.finish().expect("finish");
        assert_eq!(
            records,
            vec![CsvRow {
                id: 5,
                name: "Eve".to_string(),
            }]
        );
    }

    #[test]
    fn csv_response_decoder_rejects_malformed_quoted_eof_row() {
        let mut decoder: Box<dyn RecordDecoder<CsvRow>> = Csv::<CsvCommaDelim>::decoder();
        assert!(
            decoder
                .push_chunk(Bytes::from_static(b"id,name\n1,\"ba\xffd\""))
                .expect("malformed quoted row should not decode yet")
                .is_empty()
        );
        let err = decoder
            .finish()
            .expect_err("malformed quoted row should fail");
        let debug = format!("{err:?}");
        let display = err.to_string();
        assert_eq!(display, "record response decode failed");
        assert!(!debug.contains("ba"));
        assert!(!display.contains("ba"));
    }

    #[test]
    fn csv_response_decoder_ignores_empty_rows_and_sanitizes_errors() {
        let mut decoder: Box<dyn RecordDecoder<CsvRow>> = Csv::<CsvCommaDelim>::decoder();
        assert!(
            decoder
                .push_chunk(Bytes::from_static(b"\n"))
                .expect("empty line")
                .is_empty()
        );
        assert!(decoder.finish().expect("finish").is_empty());

        let mut decoder: Box<dyn RecordDecoder<CsvRow>> = Csv::<CsvCommaDelim>::decoder();
        let err = decoder
            .push_chunk(Bytes::from_static(b"id,name\n1\n"))
            .expect_err("wrong field count should fail");
        let debug = format!("{err:?}");
        let display = err.to_string();
        assert!(!debug.contains("1\n"));
        assert!(!display.contains("1\n"));
        assert_eq!(display, "record response decode failed");
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
