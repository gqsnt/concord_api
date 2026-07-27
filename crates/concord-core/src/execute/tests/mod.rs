use std::collections::VecDeque;
use std::fmt;
use std::future::IntoFuture;
use std::io;
use std::num::{NonZeroU32, NonZeroUsize};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
#[cfg(feature = "records")]
use bytes::BytesMut;
use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::gate::Gate;
use concord_test_support::script::{
    BodyCategory, ExpectedRequest, ScriptedResponse, TransportFailure,
};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use futures_core::Stream;
use http::{Method, StatusCode};

use super::*;
use crate::body::{
    BodyError, BodyFactory, EmptyBody, RequestBody, RequestBodyContext, RequestBodyMetadata,
    RequestBodySpec, Stream as StreamSpec, StreamBody,
};
#[cfg(feature = "multipart")]
use crate::body::{MultipartBody, MultipartFactory, MultipartPart};
use crate::codec::{
    CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType,
    OctetStream, Text,
};
use crate::endpoint::EndpointId;
use crate::error::{ErrorCategory, HookError, StatusBodyState};
use crate::hooks::{BoxFuture, RuntimeHooks};
use crate::http::HttpConfigBuilder;
#[cfg(feature = "records")]
use crate::rate_limit::NoopRateLimiter;
use crate::rate_limit::{
    RateLimitError, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimitRule, RateLimitWindow, RateLimiter,
};
#[cfg(feature = "records")]
use crate::records::{
    BufferedRecords, RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream,
    Records,
};
use crate::request::{HeadBuilder, PreparedHead, RequestMeta, RuntimeEndpoint};
#[cfg(feature = "records")]
use crate::response::ApiResponse;
use crate::response::{
    Buffered, ByteStream, NoContent, ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec,
    ResponseHead, Streaming,
};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "execution_api.health.bytes";
const NO_CONTENT_ENDPOINT: &str = "execution_api.health.ping";
const CODEC_ENDPOINT: &str = "execution_api.codec.round_trip";
const BODY_ENDPOINT: &str = "execution_api.body.upload";
const STREAM_ENDPOINT: &str = "execution_api.stream.download";
const RESPONSE_ENDPOINT: &str = "execution_api.response.selected";
#[cfg(feature = "records")]
const RECORD_ENDPOINT: &str = "execution_api.records.stream";

struct StreamEndpoint<M> {
    marker: std::marker::PhantomData<fn() -> M>,
}

struct ResponseEndpoint<R> {
    timeout: Option<Duration>,
    marker: std::marker::PhantomData<fn() -> R>,
}

impl<R> ResponseEndpoint<R> {
    fn new(timeout: Option<Duration>) -> Self {
        Self {
            timeout,
            marker: std::marker::PhantomData,
        }
    }
}

impl<R> EndpointSpec for ResponseEndpoint<R>
where
    R: ResponseBodySpec,
{
    type Body = EmptyBody;
    type Response = R;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(RESPONSE_ENDPOINT, Method::GET, self.timeout)
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("response")?;
        builder.push_static("selected")?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

impl<M> StreamEndpoint<M> {
    fn new() -> Self {
        Self {
            marker: std::marker::PhantomData,
        }
    }
}

impl<M> EndpointSpec for StreamEndpoint<M>
where
    M: MediaType,
{
    type Body = EmptyBody;
    type Response = StreamSpec<M>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(STREAM_ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("stream")?;
        builder.push_static("download")?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

struct VendorStream;

impl MediaType for VendorStream {
    const MEDIA_TYPE: &'static str = "application/vnd.concord.stream; version=1";
}

struct InvalidStreamMedia;

impl MediaType for InvalidStreamMedia {
    const MEDIA_TYPE: &'static str = "not-a-media-type";
}

#[cfg(feature = "records")]
struct SurfaceEndpoint<R> {
    query: &'static str,
    marker: std::marker::PhantomData<fn() -> R>,
}

#[cfg(feature = "records")]
impl<R> SurfaceEndpoint<R> {
    fn new(query: &'static str) -> Self {
        Self {
            query,
            marker: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "records")]
impl<R> EndpointSpec for SurfaceEndpoint<R>
where
    R: ResponseBodySpec,
{
    type Body = EmptyBody;
    type Response = R;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(RECORD_ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("records")?;
        builder.push_static("stream")?;
        builder.push_pagination_query_value("continuation", &self.query)?;
        builder.push_rate_limit_rule(
            "ExecutionApi::rate_limit::records",
            [("records.partition", "record-surface-rate-key")],
            1,
            [(100, Duration::from_secs(1))],
        )?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

#[cfg(feature = "records")]
fn surface_request<R>(core: ClientCore, query: &'static str) -> Request<SurfaceEndpoint<R>>
where
    R: ResponseBodySpec,
{
    Request::new(core, SurfaceEndpoint::new(query))
}

#[cfg(feature = "records")]
fn record_headers(content_type: &'static str, secret: &'static str) -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(content_type),
    );
    headers.insert("x-record-secret", http::HeaderValue::from_static(secret));
    headers
}

#[cfg(feature = "records")]
static OPENING_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static OPENING_START_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static OPENING_DECODE_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static OPENING_EOF_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static OPENING_DECODER_DROPS: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "records")]
struct OpeningRecordDecoder;

#[cfg(feature = "records")]
impl RecordDecoder for OpeningRecordDecoder {
    type Record = Bytes;

    fn accept() -> Option<&'static str> {
        Some("application/x-concord-records")
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        OPENING_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        assert!(head.url.as_str().contains("opening-query-sentinel-4e15b9"));
        assert_eq!(
            head.headers.get("x-record-secret"),
            Some(&http::HeaderValue::from_static(
                "opening-header-sentinel-762db1"
            ))
        );
        Ok(())
    }

    fn start(head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        OPENING_START_CALLS.fetch_add(1, Ordering::SeqCst);
        assert_eq!(head.endpoint, RECORD_ENDPOINT);
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        OPENING_DECODE_CALLS.fetch_add(1, Ordering::SeqCst);
        let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        Ok(Some(record.freeze()))
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        OPENING_EOF_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok((!buffer.is_empty()).then(|| buffer.split_to(buffer.len()).freeze()))
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(4 * 1024).expect("positive opening decoder staging bound")
    }
}

#[cfg(feature = "records")]
impl Drop for OpeningRecordDecoder {
    fn drop(&mut self) {
        OPENING_DECODER_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(feature = "records")]
static FAILURE_MODE: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static FAILURE_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static FAILURE_START_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static FAILURE_DECODE_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static FAILURE_EOF_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "records")]
static FAILURE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(feature = "records")]
struct FailingRecordDecoder;

#[cfg(feature = "records")]
impl RecordDecoder for FailingRecordDecoder {
    type Record = Bytes;

    fn validate(_head: &ResponseHead<'_>) -> Result<(), RecordError> {
        FAILURE_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        FAILURE_START_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        FAILURE_DECODE_CALLS.fetch_add(1, Ordering::SeqCst);
        if FAILURE_MODE.load(Ordering::SeqCst) == 1 && !buffer.is_empty() {
            Err(RecordError::new(RecordErrorKind::InvalidFraming))
        } else {
            Ok(None)
        }
    }

    fn decode_eof(&mut self, _buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        FAILURE_EOF_CALLS.fetch_add(1, Ordering::SeqCst);
        if FAILURE_MODE.load(Ordering::SeqCst) == 2 {
            Err(RecordError::new(RecordErrorKind::IncompleteRecord))
        } else {
            Ok(None)
        }
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(4 * 1024).expect("positive failure decoder staging bound")
    }
}

#[cfg(feature = "records")]
macro_rules! drop_decoder {
    ($name:ident, $drops:ident) => {
        static $drops: AtomicUsize = AtomicUsize::new(0);

        struct $name;

        impl RecordDecoder for $name {
            type Record = Bytes;

            fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
                Ok(Self)
            }

            fn decode(
                &mut self,
                _buffer: &mut BytesMut,
            ) -> Result<Option<Self::Record>, RecordError> {
                Ok(None)
            }

            fn decode_eof(
                &mut self,
                _buffer: &mut BytesMut,
            ) -> Result<Option<Self::Record>, RecordError> {
                Ok(None)
            }

            fn max_buffered_bytes() -> NonZeroUsize {
                NonZeroUsize::new(4 * 1024).expect("positive drop decoder staging bound")
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                $drops.fetch_add(1, Ordering::SeqCst);
            }
        }
    };
}

#[cfg(feature = "records")]
drop_decoder!(StreamDropDecoder, STREAM_DROP_DECODER_DROPS);
#[cfg(feature = "records")]
drop_decoder!(BatchDropDecoder, BATCH_DROP_DECODER_DROPS);

#[cfg(feature = "records")]
const RECORD_RAW_SENTINEL: &str = "record-raw-payload-sentinel-93d714";
#[cfg(feature = "records")]
const RECORD_CSV_HEADER_SENTINEL: &str = "record-csv-header-sentinel-b7a503";
#[cfg(feature = "records")]
const RECORD_CSV_FIELD_SENTINEL: &str = "record-csv-field-sentinel-4f872a";
#[cfg(feature = "records")]
const RECORD_NDJSON_SENTINEL: &str = "record-ndjson-value-sentinel-e329c1";
#[cfg(feature = "records")]
const RECORD_DECODER_STATE_SENTINEL: &str = "record-decoder-state-sentinel-6d80af";
#[cfg(feature = "records")]
const RECORD_PARSER_SENTINEL: &str = "record-parser-text-sentinel-1c5e92";
#[cfg(feature = "records")]
const RECORD_HEADER_SENTINEL: &str = "record-response-header-sentinel-a481d6";
#[cfg(feature = "records")]
const RECORD_QUERY_SENTINEL: &str = "record-continuation-query-sentinel-f2057b";

#[cfg(feature = "records")]
struct DiagnosticRecord(Bytes);

#[cfg(feature = "records")]
impl fmt::Debug for DiagnosticRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("DiagnosticRecord")
            .field(&String::from_utf8_lossy(&self.0))
            .finish()
    }
}

#[cfg(feature = "records")]
struct DiagnosticRecordDecoder {
    state: &'static str,
}

#[cfg(feature = "records")]
impl RecordDecoder for DiagnosticRecordDecoder {
    type Record = DiagnosticRecord;

    fn accept() -> Option<&'static str> {
        Some("application/x-concord-records")
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        if head
            .headers
            .get(http::header::CONTENT_TYPE)
            .is_none_or(|value| value != "application/x-concord-records")
        {
            return Err(RecordError::new(RecordErrorKind::InvalidMediaType));
        }
        assert!(head.url.as_str().contains(RECORD_QUERY_SENTINEL));
        assert_eq!(
            head.headers.get("x-record-secret"),
            Some(&http::HeaderValue::from_static(RECORD_HEADER_SENTINEL))
        );
        Ok(())
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self {
            state: RECORD_DECODER_STATE_SENTINEL,
        })
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        assert_eq!(self.state, RECORD_DECODER_STATE_SENTINEL);
        let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        if record.starts_with(b"parser-trigger") {
            let parser_text = RECORD_PARSER_SENTINEL;
            assert!(!parser_text.is_empty());
            return Err(RecordError::new(RecordErrorKind::InvalidFraming));
        }
        Ok(Some(DiagnosticRecord(record.freeze())))
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        if buffer.is_empty() {
            Ok(None)
        } else {
            Err(RecordError::new(RecordErrorKind::IncompleteRecord))
        }
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(4 * 1024).expect("positive diagnostic decoder staging bound")
    }
}

#[cfg(feature = "records")]
struct SensitiveBufferedOutput(String);

#[cfg(feature = "records")]
impl fmt::Debug for SensitiveBufferedOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SensitiveBufferedOutput")
            .field(&self.0)
            .finish()
    }
}

#[cfg(feature = "records")]
struct SensitiveBufferedSpec;

#[cfg(feature = "records")]
impl ResponseBodySpec for SensitiveBufferedSpec {
    type Output = SensitiveBufferedOutput;
    type Mode = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Self::Output, ResponseBodyError> {
        Ok(SensitiveBufferedOutput(
            String::from_utf8_lossy(&body).into_owned(),
        ))
    }
}

#[cfg(feature = "records")]
struct FailingBufferedSpec;

#[cfg(feature = "records")]
impl ResponseBodySpec for FailingBufferedSpec {
    type Output = ();
    type Mode = Buffered;

    fn from_body(
        _body: Bytes,
        _head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        Err(ResponseBodyError::with_safe_message(
            ResponseBodyErrorKind::Decode,
            "response-parser-text-sentinel-11fa62",
        ))
    }
}

#[cfg(feature = "records")]
struct ResponseSurfaceRecordDecoder;

#[cfg(feature = "records")]
impl RecordDecoder for ResponseSurfaceRecordDecoder {
    type Record = DiagnosticRecord;

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        Ok(Some(DiagnosticRecord(record.freeze())))
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        Ok((!buffer.is_empty()).then(|| DiagnosticRecord(buffer.split_to(buffer.len()).freeze())))
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(4 * 1024).expect("positive response-surface staging bound")
    }
}

struct BodyEndpoint {
    body: RequestBody,
}

impl BodyEndpoint {
    fn new(body: RequestBody) -> Self {
        Self { body }
    }
}

impl EndpointSpec for BodyEndpoint {
    type Body = RequestBody;
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(BODY_ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("body")?;
        builder.push_static("upload")?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        RequestBody::metadata(&self.body, context)
    }

    fn into_body(self) -> RequestBody {
        self.body
    }
}

enum ProbeFrame {
    Data(&'static [u8]),
    Failure(&'static str),
}

struct BodyPollProbe {
    frames: VecDeque<ProbeFrame>,
    polls: Arc<AtomicUsize>,
    dropped: Option<Arc<AtomicBool>>,
}

impl BodyPollProbe {
    fn new(frames: impl IntoIterator<Item = ProbeFrame>, polls: &Arc<AtomicUsize>) -> Self {
        Self {
            frames: frames.into_iter().collect(),
            polls: Arc::clone(polls),
            dropped: None,
        }
    }
}

impl Stream for BodyPollProbe {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(self.frames.pop_front().map(|frame| match frame {
            ProbeFrame::Data(bytes) => Ok(Bytes::from_static(bytes)),
            ProbeFrame::Failure(message) => Err(io::Error::other(message)),
        }))
    }
}

impl Drop for BodyPollProbe {
    fn drop(&mut self) {
        if let Some(dropped) = &self.dropped {
            dropped.store(true, Ordering::SeqCst);
        }
    }
}

struct ProbeBodyFactory {
    content_type_calls: Arc<AtomicUsize>,
    create_calls: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    fail: bool,
}

impl BodyFactory for ProbeBodyFactory {
    fn content_type(&self) -> Option<http::HeaderValue> {
        self.content_type_calls.fetch_add(1, Ordering::SeqCst);
        Some(http::HeaderValue::from_static(
            "application/x-concord-factory",
        ))
    }

    fn create(&self) -> Result<StreamBody, BodyError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(BodyError::with_safe_message(
                crate::body::BodyErrorKind::Factory,
                "safe factory configuration failure",
            ))
        } else {
            Ok(StreamBody::from_stream(BodyPollProbe::new(
                [ProbeFrame::Data(b"factory")],
                &self.polls,
            ))
            .exact_length(7))
        }
    }
}

struct PendingBodyStream {
    polls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
    reached: Arc<tokio::sync::Notify>,
}

impl Stream for PendingBodyStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        self.reached.notify_one();
        Poll::Pending
    }
}

impl Drop for PendingBodyStream {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

struct PendingBodyFactory {
    create_calls: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
    reached: Arc<tokio::sync::Notify>,
}

impl BodyFactory for PendingBodyFactory {
    fn create(&self) -> Result<StreamBody, BodyError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(StreamBody::from_stream(PendingBodyStream {
            polls: Arc::clone(&self.polls),
            dropped: Arc::clone(&self.dropped),
            reached: Arc::clone(&self.reached),
        }))
    }
}

#[cfg(feature = "multipart")]
struct ProbeMultipartFactory {
    create_calls: Arc<AtomicUsize>,
    fail: bool,
}

#[cfg(feature = "multipart")]
impl MultipartFactory for ProbeMultipartFactory {
    fn create(&self) -> Result<MultipartBody, BodyError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(BodyError::with_safe_message(
                crate::body::BodyErrorKind::Factory,
                "multipart-factory-error-secret-88a1",
            ))
        } else {
            MultipartBody::new().part(
                "factory-field",
                MultipartPart::text("factory-multipart-value")?,
            )
        }
    }
}

struct CodecEndpoint<Encoder, Decoder>
where
    Encoder: EncodeBody,
    Decoder: DecodeBody,
{
    input: Encoder::Input,
    marker: std::marker::PhantomData<Decoder>,
}

impl<Encoder, Decoder> CodecEndpoint<Encoder, Decoder>
where
    Encoder: EncodeBody,
    Decoder: DecodeBody,
{
    fn new(input: Encoder::Input) -> Self {
        Self {
            input,
            marker: std::marker::PhantomData,
        }
    }
}

impl<Encoder, Decoder> EndpointSpec for CodecEndpoint<Encoder, Decoder>
where
    Encoder: EncodeBody,
    Decoder: DecodeBody,
{
    type Body = Encoder;
    type Response = Decoder;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(CODEC_ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("codec")?;
        builder.push_static("round-trip")?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Encoder::metadata(&self.input, context)
    }

    fn into_body(self) -> Encoder::Input {
        self.input
    }
}

struct EncoderInput {
    bytes: Bytes,
    calls: Arc<AtomicUsize>,
    fail: bool,
}

struct ProbeEncoder;

impl EncodeBody for ProbeEncoder {
    type Input = EncoderInput;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-concord-probe");

    fn encode(input: Self::Input, context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        assert_eq!(context.endpoint, CODEC_ENDPOINT);
        assert_eq!(context.method, Method::POST);
        assert!(context.body_limit.is_some_and(|limit| limit > 0));
        input.calls.fetch_add(1, Ordering::SeqCst);
        if input.fail {
            Err(CodecError::new(CodecErrorKind::Encode))
        } else {
            Ok(input.bytes)
        }
    }
}

static PROBE_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);
static PROBE_VALIDATOR_CALLS: AtomicUsize = AtomicUsize::new(0);

struct ProbeDecoder;

impl DecodeBody for ProbeDecoder {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-probe");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        PROBE_VALIDATOR_CALLS.fetch_add(1, Ordering::SeqCst);
        if content_type == Some("application/x-concord-probe") {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        PROBE_DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        assert_eq!(context.endpoint, CODEC_ENDPOINT);
        assert_eq!(context.method, Method::POST);
        assert_eq!(context.status, StatusCode::OK);
        assert_eq!(context.content_type, Some("application/x-concord-probe"));
        String::from_utf8(body.to_vec())
            .map(|value| format!("decoded:{value}"))
            .map_err(|error| {
                drop(error);
                CodecError::new(CodecErrorKind::Decode)
            })
    }
}

static FAILING_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct FailingDecoder;

impl DecodeBody for FailingDecoder {
    type Output = String;

    const ACCEPT: Option<&'static str> = None;

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        FAILING_DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        Err(CodecError::new(CodecErrorKind::Decode))
    }
}

static REJECTING_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct RejectingDecoder;

impl DecodeBody for RejectingDecoder {
    type Output = ();

    const ACCEPT: Option<&'static str> = Some("application/x-required");

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Err(CodecError::new(CodecErrorKind::Decode))
    }

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        REJECTING_DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

static OVERFLOW_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct OverflowDecoder;

impl DecodeBody for OverflowDecoder {
    type Output = ();

    const ACCEPT: Option<&'static str> = None;

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        OVERFLOW_DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct RealCustomDecoder;

impl DecodeBody for RealCustomDecoder {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-probe");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Some("application/x-concord-probe") {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|error| {
            drop(error);
            CodecError::new(CodecErrorKind::Decode)
        })
    }
}

struct BytesEndpoint {
    inherited_timeout: Option<Duration>,
    trace: Option<Arc<Mutex<Vec<String>>>>,
}

impl BytesEndpoint {
    fn new(inherited_timeout: Option<Duration>) -> Self {
        Self {
            inherited_timeout,
            trace: None,
        }
    }

    fn traced(trace: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            inherited_timeout: Some(Duration::from_secs(30)),
            trace: Some(trace),
        }
    }
}

impl EndpointSpec for BytesEndpoint {
    type Body = EmptyBody;
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, self.inherited_timeout)
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        if let Some(trace) = &self.trace {
            trace
                .lock()
                .expect("trace lock")
                .push("head preparation".into());
        }
        builder.push_static("health")?;
        builder.push_static("bytes")?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

struct RateLimitedBytesEndpoint {
    inherited_timeout: Option<Duration>,
    key: String,
    capacity: u32,
    window: Duration,
}

impl RateLimitedBytesEndpoint {
    fn new(inherited_timeout: Option<Duration>, key: impl Into<String>) -> Self {
        Self {
            inherited_timeout,
            key: key.into(),
            capacity: 100,
            window: Duration::from_secs(1),
        }
    }
}

impl EndpointSpec for RateLimitedBytesEndpoint {
    type Body = EmptyBody;
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, self.inherited_timeout)
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("health")?;
        builder.push_static("bytes")?;
        builder.push_rate_limit_rule(
            "ExecutionApi::rate_limit::tenant",
            [("vars.tenant", self.key.as_str())],
            1,
            [(self.capacity, self.window)],
        )?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

struct GlobalRateLimitedBytesEndpoint;

impl EndpointSpec for GlobalRateLimitedBytesEndpoint {
    type Body = EmptyBody;
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("health")?;
        builder.push_static("bytes")?;
        builder.push_rate_limit_rule(
            "ExecutionApi::rate_limit::global",
            std::iter::empty::<(&str, &str)>(),
            1,
            [(100, Duration::from_secs(1))],
        )?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

struct NoContentEndpoint;

impl EndpointSpec for NoContentEndpoint {
    type Body = EmptyBody;
    type Response = NoContent;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            NO_CONTENT_ENDPOINT,
            Method::GET,
            Some(Duration::from_secs(30)),
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("health")?;
        builder.push_static("ping")?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

struct InvalidEndpoint;

impl EndpointSpec for InvalidEndpoint {
    type Body = EmptyBody;
    type Response = Bytes;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_value(&"")?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

#[derive(Clone, Debug)]
enum LifecycleRecord {
    Before {
        operation_id: OperationId,
        body_kind: crate::hooks::BodyKind,
        known_body_length: Option<u64>,
    },
    Response(OperationId, StatusCode),
    Complete {
        operation_id: OperationId,
        outcome: CompletionOutcome,
        status: Option<StatusCode>,
        elapsed: Duration,
    },
}

struct ProbeHooks {
    records: Mutex<Vec<LifecycleRecord>>,
    trace: Option<Arc<Mutex<Vec<String>>>>,
    before_gate: Option<Gate>,
    response_gate: Option<Gate>,
    completion_gate: Option<Gate>,
    veto: AtomicBool,
    response_fail: bool,
    completion_returned: AtomicUsize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LimiterRequestRecord {
    endpoint: String,
    method: Method,
    safe_url: String,
    host: Option<String>,
    page_index: u32,
    rule_ids: Vec<String>,
    key_values: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LimiterResponseRecord {
    safe_url: String,
    status: StatusCode,
    headers: String,
}

struct ProbeRateLimiter {
    acquisitions: AtomicUsize,
    observations: AtomicUsize,
    learned_cooldown: AtomicBool,
    requests: Mutex<Vec<LimiterRequestRecord>>,
    responses: Mutex<Vec<LimiterResponseRecord>>,
    response_cooldown_caps: Mutex<Vec<Duration>>,
    trace: Option<Arc<Mutex<Vec<String>>>>,
    acquire_gate: Option<Gate>,
    observe_gate: Option<Gate>,
    response_body_polls: Option<Arc<AtomicUsize>>,
    fail: bool,
    observe_fail: bool,
    key_limit: usize,
}

#[cfg(feature = "records")]
#[derive(Default)]
struct AutomaticSurfaceHooks {
    entries: Mutex<Vec<String>>,
    outcomes: Mutex<Vec<CompletionOutcome>>,
}

#[cfg(feature = "records")]
impl AutomaticSurfaceHooks {
    fn entries(&self) -> Vec<String> {
        self.entries.lock().expect("automatic hook entries").clone()
    }

    fn outcomes(&self) -> Vec<CompletionOutcome> {
        self.outcomes
            .lock()
            .expect("automatic completion outcomes")
            .clone()
    }
}

#[cfg(feature = "records")]
impl RuntimeHooks for AutomaticSurfaceHooks {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        self.entries
            .lock()
            .expect("automatic hook entries")
            .push(format!("before-send {context:?}"));
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(
        &'a self,
        context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        self.entries
            .lock()
            .expect("automatic hook entries")
            .push(format!("response-head {context:?}"));
        Box::pin(async { Ok(()) })
    }

    fn on_complete<'a>(&'a self, context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.entries
            .lock()
            .expect("automatic hook entries")
            .push(format!("completion {context:?}"));
        self.outcomes
            .lock()
            .expect("automatic completion outcomes")
            .push(context.outcome);
        Box::pin(async {})
    }
}

#[cfg(feature = "records")]
#[derive(Default)]
struct AutomaticSurfaceLimiter {
    acquisitions: AtomicUsize,
    observations: AtomicUsize,
    entries: Mutex<Vec<String>>,
}

#[cfg(feature = "records")]
impl AutomaticSurfaceLimiter {
    fn entries(&self) -> Vec<String> {
        self.entries
            .lock()
            .expect("automatic limiter entries")
            .clone()
    }
}

#[cfg(feature = "records")]
impl RateLimiter for AutomaticSurfaceLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        let keys = request
            .plan
            .rules()
            .iter()
            .flat_map(|rule| rule.key().parts())
            .map(|part| part.value().to_owned())
            .collect::<Vec<_>>();
        self.entries
            .lock()
            .expect("automatic limiter entries")
            .push(format!(
                "acquire endpoint={} method={} url={} page={} keys={keys:?}",
                request.endpoint, request.method, request.safe_url, request.page_index
            ));
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe<'a>(
        &'a self,
        response: RateLimitResponse<'a>,
    ) -> BoxFuture<'a, Result<(), RateLimitError>> {
        self.observations.fetch_add(1, Ordering::SeqCst);
        self.entries
            .lock()
            .expect("automatic limiter entries")
            .push(format!(
                "observe endpoint={} method={} url={} page={} status={} headers={:?}",
                response.request.endpoint,
                response.request.method,
                response.request.safe_url,
                response.request.page_index,
                response.status,
                response.headers,
            ));
        Box::pin(async { Ok(()) })
    }
}

impl ProbeRateLimiter {
    fn new() -> Self {
        Self {
            acquisitions: AtomicUsize::new(0),
            observations: AtomicUsize::new(0),
            learned_cooldown: AtomicBool::new(false),
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(Vec::new()),
            response_cooldown_caps: Mutex::new(Vec::new()),
            trace: None,
            acquire_gate: None,
            observe_gate: None,
            response_body_polls: None,
            fail: false,
            observe_fail: false,
            key_limit: crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
        }
    }

    fn request_records(&self) -> Vec<LimiterRequestRecord> {
        self.requests.lock().expect("limiter requests").clone()
    }

    fn response_records(&self) -> Vec<LimiterResponseRecord> {
        self.responses.lock().expect("limiter responses").clone()
    }
}

impl RateLimiter for ProbeRateLimiter {
    fn preparation_key_limit(&self) -> usize {
        self.key_limit
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        if let Some(trace) = &self.trace {
            trace
                .lock()
                .expect("trace lock")
                .push("limiter acquire".into());
        }
        self.requests
            .lock()
            .expect("limiter requests")
            .push(LimiterRequestRecord {
                endpoint: request.endpoint.to_owned(),
                method: request.method.clone(),
                safe_url: request.safe_url.to_owned(),
                host: request.host.map(str::to_owned),
                page_index: request.page_index,
                rule_ids: request
                    .plan
                    .rules()
                    .iter()
                    .map(|rule| rule.id().as_str().to_owned())
                    .collect(),
                key_values: request
                    .plan
                    .rules()
                    .iter()
                    .flat_map(|rule| rule.key().parts())
                    .map(|part| part.value().to_owned())
                    .collect(),
            });
        Box::pin(async move {
            if let Some(gate) = &self.acquire_gate {
                gate.wait().await;
            }
            if self.fail {
                Err(RateLimitError::new(
                    crate::error::RateLimitErrorKind::Acquire,
                ))
            } else {
                Ok(RateLimitPermit::granted())
            }
        })
    }

    fn observe<'a>(
        &'a self,
        response: RateLimitResponse<'a>,
    ) -> BoxFuture<'a, Result<(), RateLimitError>> {
        self.observations.fetch_add(1, Ordering::SeqCst);
        if response.status == StatusCode::TOO_MANY_REQUESTS
            && response.headers.get(&http::header::RETRY_AFTER)
                == Some(crate::redaction::SanitizedHeaderValue::Text("5"))
        {
            self.learned_cooldown.store(true, Ordering::SeqCst);
        }
        self.responses
            .lock()
            .expect("limiter responses")
            .push(LimiterResponseRecord {
                safe_url: response.request.safe_url.to_owned(),
                status: response.status,
                headers: format!("{:?}", response.headers),
            });
        self.response_cooldown_caps
            .lock()
            .expect("response cooldown caps")
            .push(response.cooldown_cap);
        if let Some(trace) = &self.trace {
            trace
                .lock()
                .expect("trace lock")
                .push("limiter observe".into());
        }
        Box::pin(async move {
            if let Some(body_polls) = &self.response_body_polls {
                assert_eq!(
                    body_polls.load(Ordering::SeqCst),
                    0,
                    "response body was polled before limiter observation finished"
                );
            }
            if let Some(gate) = &self.observe_gate {
                gate.wait().await;
            }
            if self.observe_fail {
                Err(RateLimitError::new(
                    crate::error::RateLimitErrorKind::Observe,
                ))
            } else {
                Ok(())
            }
        })
    }
}

impl ProbeHooks {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            trace: None,
            before_gate: None,
            response_gate: None,
            completion_gate: None,
            veto: AtomicBool::new(false),
            response_fail: false,
            completion_returned: AtomicUsize::new(0),
        }
    }

    fn traced(trace: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            trace: Some(trace),
            ..Self::new()
        }
    }

    fn records(&self) -> Vec<LifecycleRecord> {
        self.records.lock().expect("records lock").clone()
    }

    fn push_trace(&self, stage: &str) {
        if let Some(trace) = &self.trace {
            trace.lock().expect("trace lock").push(stage.into());
        }
    }
}

impl RuntimeHooks for ProbeHooks {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        self.push_trace("before_send");
        self.records
            .lock()
            .expect("records lock")
            .push(LifecycleRecord::Before {
                operation_id: context.operation_id,
                body_kind: context.body_kind,
                known_body_length: context.known_body_length,
            });
        Box::pin(async move {
            if let Some(gate) = &self.before_gate {
                gate.wait().await;
            }
            if self.veto.load(Ordering::SeqCst) {
                Err(HookError::new("safe hook veto"))
            } else {
                Ok(())
            }
        })
    }

    fn on_response_head<'a>(
        &'a self,
        context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        self.push_trace("on_response_head");
        self.records
            .lock()
            .expect("records lock")
            .push(LifecycleRecord::Response(
                context.operation_id,
                context.status,
            ));
        Box::pin(async move {
            if let Some(gate) = &self.response_gate {
                gate.wait().await;
            }
            if self.response_fail {
                Err(HookError::new("safe response-hook rejection"))
            } else {
                Ok(())
            }
        })
    }

    fn on_complete<'a>(&'a self, context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.push_trace("on_complete");
        self.records
            .lock()
            .expect("records lock")
            .push(LifecycleRecord::Complete {
                operation_id: context.operation_id,
                outcome: context.outcome,
                status: context.final_status,
                elapsed: context.elapsed,
            });
        Box::pin(async move {
            if let Some(gate) = &self.completion_gate {
                gate.wait().await;
            }
            self.completion_returned.fetch_add(1, Ordering::SeqCst);
        })
    }
}

fn development_core(
    executor: &DeterministicExecutor,
    hooks: Arc<ProbeHooks>,
    runtime: RuntimeConfigBuilder,
) -> ClientCore {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    ClientCore::build_with_development(
        "http://execution.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        runtime.hooks(hooks),
        executor,
    )
    .expect("development core")
}

#[cfg(feature = "records")]
fn automatic_surface_core(
    executor: &DeterministicExecutor,
    hooks: Arc<AutomaticSurfaceHooks>,
    limiter: Arc<AutomaticSurfaceLimiter>,
    debug: Arc<Mutex<Vec<String>>>,
) -> ClientCore {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    let configured: Arc<dyn RateLimiter> = limiter;
    ClientCore::build_with_development(
        "http://execution.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .hooks(hooks)
            .rate_limiter(configured)
            .debug(DebugLevel::Headers),
        executor,
    )
    .expect("automatic-surface development core")
    .with_debug_output(debug)
}

fn real_core(
    server: &ScriptedServer,
    hooks: Arc<ProbeHooks>,
    runtime: RuntimeConfigBuilder,
) -> ClientCore {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    ClientCore::build(
        &server.url("/v1/"),
        HttpConfigBuilder::new().http1_only(),
        runtime.hooks(hooks),
    )
    .expect("real HTTP core")
}

fn bytes_request(core: ClientCore) -> Request<BytesEndpoint> {
    Request::new(core, BytesEndpoint::new(Some(Duration::from_secs(30))))
}

fn rate_limited_bytes_request(
    core: ClientCore,
    timeout: Option<Duration>,
    key: impl Into<String>,
) -> Request<RateLimitedBytesEndpoint> {
    Request::new(core, RateLimitedBytesEndpoint::new(timeout, key))
}

fn key_value_with_len(length: usize, prefix: &str) -> String {
    assert!(prefix.len() <= length);
    let mut value = String::with_capacity(length);
    value.push_str(prefix);
    value.extend(std::iter::repeat_n('k', length - prefix.len()));
    value
}

fn no_content_request(core: ClientCore) -> Request<NoContentEndpoint> {
    Request::new(core, NoContentEndpoint)
}

fn codec_request<Encoder, Decoder>(
    core: ClientCore,
    input: Encoder::Input,
) -> Request<CodecEndpoint<Encoder, Decoder>>
where
    Encoder: EncodeBody,
    Decoder: DecodeBody,
{
    Request::new(core, CodecEndpoint::new(input))
}

fn body_request(core: ClientCore, body: RequestBody) -> Request<BodyEndpoint> {
    Request::new(core, BodyEndpoint::new(body))
}

fn stream_request<M>(core: ClientCore) -> Request<StreamEndpoint<M>>
where
    M: MediaType,
{
    Request::new(core, StreamEndpoint::new())
}

fn response_request<R>(core: ClientCore, timeout: Option<Duration>) -> Request<ResponseEndpoint<R>>
where
    R: ResponseBodySpec,
{
    Request::new(core, ResponseEndpoint::new(timeout))
}

async fn next_stream_chunk(response: &mut ByteStream) -> Option<Result<Bytes, ApiError>> {
    std::future::poll_fn(|context| Pin::new(&mut *response).poll_next(context)).await
}

#[cfg(feature = "records")]
async fn next_record<D>(stream: &mut RecordStream<D>) -> Option<Result<D::Record, RecordError>>
where
    D: RecordDecoder,
{
    std::future::poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}

#[cfg(feature = "records")]
async fn next_batch<D>(stream: &mut RecordBatches<D>) -> Option<Result<Vec<D::Record>, RecordError>>
where
    D: RecordDecoder,
{
    std::future::poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}

#[cfg(feature = "records")]
fn recursive_error_surfaces(error: &(dyn std::error::Error + 'static)) -> String {
    let mut rendered = String::new();
    let mut current = Some(error);
    while let Some(source) = current {
        rendered.push_str(&source.to_string());
        rendered.push('\n');
        rendered.push_str(&format!("{source:?}"));
        rendered.push('\n');
        current = source.source();
    }
    rendered
}

#[cfg(feature = "records")]
fn completion_count(records: &[LifecycleRecord]) -> usize {
    records
        .iter()
        .filter(|record| matches!(record, LifecycleRecord::Complete { .. }))
        .count()
}

#[cfg(feature = "records")]
fn assert_omits(rendered: &str, sentinels: &[&str]) {
    for sentinel in sentinels {
        assert!(
            !rendered.contains(sentinel),
            "automatic surface exposed {sentinel}: {rendered}"
        );
    }
}

fn probe_input(value: &'static [u8], calls: &Arc<AtomicUsize>) -> EncoderInput {
    EncoderInput {
        bytes: Bytes::from_static(value),
        calls: Arc::clone(calls),
        fail: false,
    }
}

fn probe_response(body: impl Into<Bytes>) -> ScriptedResponse {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/x-concord-probe"),
    );
    ScriptedResponse::success(body).headers(headers)
}

fn completion(
    records: &[LifecycleRecord],
) -> (OperationId, CompletionOutcome, Option<StatusCode>, Duration) {
    records
        .iter()
        .find_map(|record| match record {
            LifecycleRecord::Complete {
                operation_id,
                outcome,
                status,
                elapsed,
            } => Some((*operation_id, *outcome, *status, *elapsed)),
            _ => None,
        })
        .expect("completion record")
}

#[cfg(feature = "records")]
mod buffered_records;
mod cancellation;
mod completion;
mod laziness;
mod ordering;
mod retry;
mod status;
mod streaming;
