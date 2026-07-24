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
    CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType, OctetStream, Text,
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
    RecordBatches, RecordDecoder, RecordError, RecordErrorKind, RecordStream, Records,
};
use crate::request::{HeadBuilder, PreparedHead, RequestMeta, RuntimeEndpoint};
#[cfg(feature = "records")]
use crate::response::{
    ApiResponse, Buffered, ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead,
};
use crate::response::{ByteStream, NoContent};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "execution_api.health.bytes";
const NO_CONTENT_ENDPOINT: &str = "execution_api.health.ping";
const CODEC_ENDPOINT: &str = "execution_api.codec.round_trip";
const BODY_ENDPOINT: &str = "execution_api.body.upload";
const STREAM_ENDPOINT: &str = "execution_api.stream.download";
#[cfg(feature = "records")]
const RECORD_ENDPOINT: &str = "execution_api.records.stream";

struct StreamEndpoint<M> {
    marker: std::marker::PhantomData<fn() -> M>,
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
        Ok(builder.finish())
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
        Ok(builder.finish())
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
        Ok(builder.finish())
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
                MultipartPart::text("factory-multipart-value"),
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
        Ok(builder.finish())
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
            Err(CodecError::new("request-codec-secret-ec2d"))
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
            Err(CodecError::new("media-contract-secret-7cb1"))
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
                CodecError::new("probe decoder UTF-8 failure")
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
        Err(CodecError::new("response-decoder-secret-c413"))
    }
}

static REJECTING_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct RejectingDecoder;

impl DecodeBody for RejectingDecoder {
    type Output = ();

    const ACCEPT: Option<&'static str> = Some("application/x-required");

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Err(CodecError::new("content-type-secret-4fea"))
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
            Err(CodecError::new("custom media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|error| {
            drop(error);
            CodecError::new("custom UTF-8 decode failed")
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
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

impl crate::request::RawResponseEndpoint for BytesEndpoint {}

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
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

impl crate::request::RawResponseEndpoint for RateLimitedBytesEndpoint {}

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
        Ok(builder.finish())
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
        Ok(builder.finish())
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
        Ok(builder.finish())
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

struct ProbeRateLimiter {
    acquisitions: AtomicUsize,
    observations: AtomicUsize,
    requests: Mutex<Vec<LimiterRequestRecord>>,
    response_cooldown_caps: Mutex<Vec<Duration>>,
    trace: Option<Arc<Mutex<Vec<String>>>>,
    acquire_gate: Option<Gate>,
    observe_gate: Option<Gate>,
    response_body_polls: Option<Arc<AtomicUsize>>,
    fail: bool,
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

    fn on_response_head<'a>(&'a self, context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        self.entries
            .lock()
            .expect("automatic hook entries")
            .push(format!("response-head {context:?}"));
        Box::pin(async {})
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

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
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
        Box::pin(async {})
    }
}

impl ProbeRateLimiter {
    fn new() -> Self {
        Self {
            acquisitions: AtomicUsize::new(0),
            observations: AtomicUsize::new(0),
            requests: Mutex::new(Vec::new()),
            response_cooldown_caps: Mutex::new(Vec::new()),
            trace: None,
            acquire_gate: None,
            observe_gate: None,
            response_body_polls: None,
            fail: false,
            key_limit: crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
        }
    }

    fn request_records(&self) -> Vec<LimiterRequestRecord> {
        self.requests.lock().expect("limiter requests").clone()
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

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        self.observations.fetch_add(1, Ordering::SeqCst);
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

    fn on_response_head<'a>(&'a self, context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
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

#[tokio::test]
async fn custom_limiter_context_and_response_observation_use_one_ordered_attempt() {
    const KEY_SENTINEL: &str = "controlled-custom-key-5e8c7f2b";
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"limited"))
                .body_poll_counter(Arc::clone(&body_polls)),
        )
        .expect("limited response");
    let trace = Arc::new(Mutex::new(Vec::new()));
    let hooks = Arc::new(ProbeHooks::traced(Arc::clone(&trace)));
    let observe_gate = Gate::closed();
    let limiter = Arc::new(ProbeRateLimiter {
        trace: Some(Arc::clone(&trace)),
        observe_gate: Some(observe_gate.clone()),
        response_body_polls: Some(Arc::clone(&body_polls)),
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .rate_limiter(configured)
            .rate_limit_cooldown_cap(Duration::from_secs(7))
            .debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));
    let request = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), KEY_SENTINEL);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let task = tokio::spawn(request.into_future());
    observe_gate.wait_until_reached().await;
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(
        trace.lock().expect("trace").as_slice(),
        [
            "before_send",
            "limiter acquire",
            "on_response_head",
            "limiter observe",
        ]
    );
    observe_gate.open();
    assert_eq!(
        task.await.expect("request task").expect("limited result"),
        Bytes::from_static(b"limited")
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(
        limiter
            .response_cooldown_caps
            .lock()
            .expect("response cooldown caps")
            .as_slice(),
        [Duration::from_secs(7)]
    );
    let records = limiter.request_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].endpoint, ENDPOINT);
    assert_eq!(records[0].method, Method::GET);
    assert_eq!(
        records[0].safe_url,
        "http://execution.example.test/v1/health/bytes"
    );
    assert_eq!(records[0].host.as_deref(), Some("execution.example.test"));
    assert_eq!(records[0].page_index, 0);
    assert_eq!(records[0].rule_ids, ["ExecutionApi::rate_limit::tenant"]);
    assert_eq!(records[0].key_values, [KEY_SENTINEL]);
    let ordinary_diagnostics = format!(
        "{}\n{:?}\n{:?}",
        debug.lock().expect("debug output").join("\n"),
        hooks.records(),
        trace.lock().expect("trace"),
    );
    assert!(
        !ordinary_diagnostics.contains(KEY_SENTINEL),
        "resolved keys are visible only in the controlled custom-limiter capture"
    );
}

#[cfg(feature = "dangerous-raw-response")]
#[tokio::test]
async fn raw_response_reuses_limiter_hooks_observation_and_success_completion() {
    const RAW_BODY: &[u8] = b"raw-lifecycle-body-secret-2f5e";
    const RATE_KEY: &str = "raw-lifecycle-rate-key-secret-e3b1";
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(RAW_BODY)))
        .expect("raw lifecycle response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .rate_limiter(configured)
            .debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));

    let raw = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), RATE_KEY)
        .raw_response()
        .await
        .expect("raw lifecycle success");
    assert_eq!(raw.body(), RAW_BODY);
    assert_eq!(raw.status(), StatusCode::OK);
    assert_eq!(
        raw.url().as_str(),
        "http://execution.example.test/v1/health/bytes"
    );
    assert_eq!(raw.metadata().endpoint(), ENDPOINT);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert!(matches!(
        hooks.records().as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Response(_, StatusCode::OK),
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::Success,
                status: Some(StatusCode::OK),
                ..
            }
        ]
    ));
    let automatic = format!(
        "{}\n{:?}\n{raw:?}",
        debug.lock().expect("debug output").join("\n"),
        hooks.records(),
    );
    assert!(!automatic.contains("raw-lifecycle-body-secret-2f5e"));
    assert!(!automatic.contains(RATE_KEY));
}

#[tokio::test]
async fn limiter_failure_prevents_request_debug_and_native_execution() {
    const KEY_SENTINEL: &str = "limiter-failure-key-private-73cf9a0d";
    let executor = DeterministicExecutor::new();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let hooks = Arc::new(ProbeHooks::traced(Arc::clone(&trace)));
    let limiter = Arc::new(ProbeRateLimiter {
        fail: true,
        trace: Some(Arc::clone(&trace)),
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = development_core(
        &executor,
        hooks,
        RuntimeConfigBuilder::new()
            .rate_limiter(configured)
            .debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));
    let error = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), KEY_SENTINEL)
        .await
        .expect_err("limiter rejection");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::Acquire)
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = source {
        assert!(!current.to_string().contains(KEY_SENTINEL));
        assert!(!format!("{current:?}").contains(KEY_SENTINEL));
        source = current.source();
    }
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
    assert_eq!(
        trace.lock().expect("trace").as_slice(),
        ["before_send", "limiter acquire", "on_complete"]
    );
}

#[tokio::test(start_paused = true)]
async fn original_deadline_cancels_limiter_wait_without_native_send() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter {
        acquire_gate: Some(gate.clone()),
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = development_core(
        &executor,
        hooks,
        RuntimeConfigBuilder::new().rate_limiter(configured),
    );
    let task = tokio::spawn(
        rate_limited_bytes_request(core, Some(Duration::from_secs(5)), "limited").into_future(),
    );
    gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    let error = task
        .await
        .expect("request task")
        .expect_err("deadline timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn prepared_plans_use_selected_limiter_metadata_and_configured_key_limit() {
    fn rule(id: &str, key: RateLimitKey) -> RateLimitRule {
        RateLimitRule::new(
            RateLimitId::new(id).expect("prepared limiter rule ID"),
            key,
            NonZeroU32::new(1).expect("prepared limiter cost"),
            [RateLimitWindow::new(
                NonZeroU32::new(10).expect("prepared limiter capacity"),
                Duration::from_secs(1),
            )
            .expect("prepared limiter window")],
        )
        .expect("prepared limiter rule")
    }

    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"governed")))
        .expect("prepared limiter response");
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(configured),
    );
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.governed"), Method::GET)
            .path_segment("governed")
            .rate_limit(rule("Prepared::governed", RateLimitKey::global()))
            .response_bytes()
            .build()
            .expect("prepared governed endpoint");
    assert_eq!(
        Request::new(core, endpoint)
            .await
            .expect("prepared limiter execution"),
        Bytes::from_static(b"governed")
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);

    let rejected_executor = DeterministicExecutor::new();
    let restricted = Arc::new(ProbeRateLimiter {
        key_limit: 3,
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = restricted.clone();
    let rejected_core = development_core(
        &rejected_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(configured),
    );
    let rejected =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.key_limit"), Method::GET)
            .rate_limit(rule(
                "Prepared::key_limit",
                RateLimitKey::from_parts([
                    RateLimitKeyPart::new("tenant", "sensitive-key").expect("tenant key part")
                ])
                .expect("prepared oversized key"),
            ))
            .response_bytes()
            .build()
            .expect("prepared oversized-plan endpoint");
    let error = Request::new(rejected_core, rejected)
        .await
        .expect_err("prepared key limit must fail during preparation");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(restricted.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(
        rejected_executor
            .application()
            .request_count()
            .expect("rejected count"),
        0
    );
}

#[tokio::test]
async fn hook_body_and_key_preparation_failures_perform_no_limiter_work() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    hooks.veto.store(true, Ordering::SeqCst);
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(configured),
    );
    let error = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), "hook-veto-key")
        .await
        .expect_err("hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);

    hooks.veto.store(false, Ordering::SeqCst);
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let error = body_request(
        core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::new(AtomicUsize::new(0)),
            create_calls: Arc::new(AtomicUsize::new(0)),
            polls: Arc::new(AtomicUsize::new(0)),
            fail: true,
        }),
    )
    .await
    .expect_err("body failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);

    let hook_records_before_key = hooks.records().len();
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let error = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), "x".repeat(1_025))
        .await
        .expect_err("key bound");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidRateLimit)
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(
        hooks.records().len(),
        hook_records_before_key + 1,
        "key rejection occurs before before-send; only completion is added"
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn default_key_limit_accepts_1024_bytes_and_rejects_the_first_excess() {
    const KEY_PART_NAME_BYTES: usize = "vars.tenant".len();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"boundary")))
        .expect("boundary response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );

    let fitting = key_value_with_len(1_024 - KEY_PART_NAME_BYTES, "default-fit-");
    assert_eq!(fitting.len() + KEY_PART_NAME_BYTES, 1_024);
    assert_eq!(
        rate_limited_bytes_request(core.clone(), Some(Duration::from_secs(30)), fitting)
            .await
            .expect("exact default boundary"),
        Bytes::from_static(b"boundary")
    );

    let excess = key_value_with_len(1_025 - KEY_PART_NAME_BYTES, "default-excess-");
    let error = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), excess)
        .await
        .expect_err("first excess must fail during preparation");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidRateLimit)
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        hooks
            .records()
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
            .count(),
        1,
        "only the fitting request reaches before-send"
    );
}

#[tokio::test]
async fn configured_key_limit_enforces_lower_zero_and_increased_boundaries() {
    const KEY_PART_NAME_BYTES: usize = "vars.tenant".len();

    let lower_executor = DeterministicExecutor::new();
    lower_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"lower")))
        .expect("lower response");
    let lower = Arc::new(ProbeRateLimiter {
        key_limit: 16,
        ..ProbeRateLimiter::new()
    });
    let lower_object: Arc<dyn RateLimiter> = lower.clone();
    let lower_core = development_core(
        &lower_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(lower_object),
    );
    let fitting = key_value_with_len(16 - KEY_PART_NAME_BYTES, "fit16");
    assert_eq!(
        rate_limited_bytes_request(lower_core.clone(), Some(Duration::from_secs(30)), fitting)
            .await
            .expect("configured lower boundary"),
        Bytes::from_static(b"lower")
    );
    let error = rate_limited_bytes_request(
        lower_core,
        Some(Duration::from_secs(30)),
        key_value_with_len(17 - KEY_PART_NAME_BYTES, "over16"),
    )
    .await
    .expect_err("first excess over lower limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(lower.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        lower_executor.application().request_count().expect("count"),
        1
    );

    let zero_executor = DeterministicExecutor::new();
    zero_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"global")))
        .expect("global response");
    let zero = Arc::new(ProbeRateLimiter {
        key_limit: 0,
        ..ProbeRateLimiter::new()
    });
    let zero_object: Arc<dyn RateLimiter> = zero.clone();
    let zero_core = development_core(
        &zero_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(zero_object),
    );
    assert_eq!(
        Request::new(zero_core.clone(), GlobalRateLimitedBytesEndpoint)
            .await
            .expect("global key under zero limit"),
        Bytes::from_static(b"global")
    );
    let error = rate_limited_bytes_request(zero_core, Some(Duration::from_secs(30)), String::new())
        .await
        .expect_err("nonempty key structure exceeds zero even with an empty value");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(zero.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        zero_executor.application().request_count().expect("count"),
        1
    );

    let increased_executor = DeterministicExecutor::new();
    increased_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"increased")))
        .expect("increased response");
    let increased = Arc::new(ProbeRateLimiter {
        key_limit: 2_048,
        ..ProbeRateLimiter::new()
    });
    let increased_object: Arc<dyn RateLimiter> = increased.clone();
    let increased_core = development_core(
        &increased_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(increased_object),
    );
    let fitting = key_value_with_len(2_048 - KEY_PART_NAME_BYTES, "increased-fit-");
    assert!(fitting.len() + KEY_PART_NAME_BYTES > 1_024);
    assert_eq!(
        rate_limited_bytes_request(
            increased_core.clone(),
            Some(Duration::from_secs(30)),
            fitting,
        )
        .await
        .expect("key above the default but within configured limit"),
        Bytes::from_static(b"increased")
    );
    let error = rate_limited_bytes_request(
        increased_core,
        Some(Duration::from_secs(30)),
        key_value_with_len(2_049 - KEY_PART_NAME_BYTES, "increased-excess-"),
    )
    .await
    .expect_err("first excess over increased limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(increased.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        increased_executor
            .application()
            .request_count()
            .expect("count"),
        1
    );
}

#[tokio::test]
async fn configured_key_limit_snapshot_is_shared_by_clones_and_retained_by_pending_requests() {
    const KEY_PART_NAME_BYTES: usize = "vars.tenant".len();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"clone")))
        .expect("clone response");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"independent",
        )))
        .expect("independent response");

    let configured = Arc::new(ProbeRateLimiter {
        key_limit: 16,
        ..ProbeRateLimiter::new()
    });
    let configured_object: Arc<dyn RateLimiter> = configured.clone();
    let configured_core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::clone(&configured_object)),
    );
    let configured_clone = configured_core.clone();
    assert!(Arc::ptr_eq(
        configured_core.rate_limiter(),
        configured_clone.rate_limiter()
    ));
    assert_eq!(configured_core.rate_limit_key_limit(), 16);
    assert_eq!(configured_clone.rate_limit_key_limit(), 16);

    let pending = rate_limited_bytes_request(
        configured_core,
        Some(Duration::from_secs(30)),
        key_value_with_len(17 - KEY_PART_NAME_BYTES, "pend17"),
    );
    assert_eq!(
        rate_limited_bytes_request(
            configured_clone,
            Some(Duration::from_secs(30)),
            key_value_with_len(16 - KEY_PART_NAME_BYTES, "clone"),
        )
        .await
        .expect("clone uses configured limit"),
        Bytes::from_static(b"clone")
    );

    let independent_limiter = Arc::new(ProbeRateLimiter::new());
    let independent = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(independent_limiter),
    );
    assert_eq!(independent.rate_limit_key_limit(), 1_024);
    assert!(!Arc::ptr_eq(independent.rate_limiter(), &configured_object));
    assert_eq!(
        rate_limited_bytes_request(
            independent,
            Some(Duration::from_secs(30)),
            key_value_with_len(17 - KEY_PART_NAME_BYTES, "def017"),
        )
        .await
        .expect("independent default build retains default limit"),
        Bytes::from_static(b"independent")
    );

    let error = pending
        .await
        .expect_err("pending request retains originating configured limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(configured.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

#[tokio::test]
async fn private_fallback_rejects_a_nonempty_runtime_plan_before_send() {
    let executor = DeterministicExecutor::new();
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error = rate_limited_bytes_request(core.clone(), Some(Duration::from_secs(30)), "nonempty")
        .await
        .expect_err("fallback must reject a nonempty runtime plan");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::InvalidConfiguration)
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let rule = RateLimitRule::new(
        RateLimitId::new("Prepared::fallback").expect("prepared fallback ID"),
        RateLimitKey::global(),
        NonZeroU32::new(1).expect("prepared fallback cost"),
        [RateLimitWindow::new(
            NonZeroU32::new(1).expect("prepared fallback capacity"),
            Duration::from_secs(1),
        )
        .expect("prepared fallback window")],
    )
    .expect("prepared fallback rule");
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.fallback"), Method::GET)
            .rate_limit(rule)
            .response_bytes()
            .build()
            .expect("prepared fallback endpoint");
    let error = Request::new(core, endpoint)
        .await
        .expect_err("fallback must reject a prepared nonempty plan");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::InvalidConfiguration)
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn hidden_reqwest_retry_does_not_reenter_custom_limiter_or_lifecycle() {
    let server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let physical_attempts = server.physical_attempts();
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = ClientCore::build(
        &server.base_url(),
        HttpConfigBuilder::new()
            .http2_prior_knowledge()
            .retry_reqwest_default(),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .rate_limiter(configured),
    )
    .expect("real HTTP/2 core");
    assert_eq!(
        bytes_request(core).await.expect("native retry recovery"),
        Bytes::new()
    );
    server.finish().await;
    assert_eq!(physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    let records = hooks.records();
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Response(_, _)))
            .count(),
        1
    );

    let prepared_server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let prepared_physical_attempts = prepared_server.physical_attempts();
    let prepared_hooks = Arc::new(ProbeHooks::new());
    let prepared_limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = prepared_limiter.clone();
    let prepared_core = ClientCore::build(
        &prepared_server.base_url(),
        HttpConfigBuilder::new()
            .http2_prior_knowledge()
            .retry_reqwest_default(),
        RuntimeConfigBuilder::new()
            .hooks(prepared_hooks.clone())
            .rate_limiter(configured),
    )
    .expect("prepared real HTTP/2 core");
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.hidden_retry"), Method::GET)
            .response_bytes()
            .build()
            .expect("prepared hidden-retry endpoint");
    assert_eq!(
        Request::new(prepared_core, endpoint)
            .await
            .expect("prepared native retry recovery"),
        Bytes::new()
    );
    prepared_server.finish().await;
    assert_eq!(prepared_physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(prepared_limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(prepared_limiter.observations.load(Ordering::SeqCst), 1);
    let prepared_records = prepared_hooks.records();
    assert_eq!(
        prepared_records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
            .count(),
        1
    );
    assert_eq!(
        prepared_records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Response(_, _)))
            .count(),
        1
    );
}

#[cfg(all(feature = "http2", feature = "dangerous-raw-response"))]
#[tokio::test]
async fn raw_response_contains_hidden_reqwest_retry_inside_one_logical_attempt() {
    let server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let physical_attempts = server.physical_attempts();
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = ClientCore::build(
        &server.base_url(),
        HttpConfigBuilder::new()
            .http2_prior_knowledge()
            .retry_reqwest_default(),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .rate_limiter(configured),
    )
    .expect("raw HTTP/2 core");

    let raw = bytes_request(core)
        .raw_response()
        .await
        .expect("raw native retry recovery");
    assert!(raw.body().is_empty());
    server.finish().await;
    assert_eq!(physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    let records = hooks.records();
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Response(_, _)))
            .count(),
        1
    );
    assert_eq!(completion(&records).1, CompletionOutcome::Success);
}

#[tokio::test]
async fn direct_and_metadata_terminals_use_one_typed_managed_path() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let expected_url = "http://execution.example.test/v1/health/bytes";
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::GET)
                .safe_url(expected_url)
                .body(Bytes::new()),
            ScriptedResponse::success(Bytes::from_static(b"direct")),
        )
        .expect("direct script");
    assert_eq!(
        bytes_request(core.clone()).await.expect("direct bytes"),
        b"direct"[..]
    );

    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"metadata")))
        .expect("metadata script");
    let response = bytes_request(core)
        .response()
        .await
        .expect("metadata response");
    assert_eq!(response.value(), &b"metadata"[..]);
    assert_eq!(response.url().as_str(), expected_url);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), ENDPOINT);
    assert_eq!(response.metadata().method(), Method::GET);
    assert_eq!(response.metadata().page_index(), 0);
    assert!(!response.metadata().recovered_authentication());

    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[0].body_category(), BodyCategory::Empty);
    assert_eq!(captures[0].known_body_length(), Some(0));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn ordering_crosses_preparation_hooks_debug_transport_response_and_completion_once() {
    let executor = DeterministicExecutor::new();
    let transport_gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(transport_gate.clone())
        .expect("transport gate");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"ordered")))
        .expect("ordered response");
    let trace = Arc::new(Mutex::new(Vec::new()));
    let hooks = Arc::new(ProbeHooks::traced(Arc::clone(&trace)));
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new())
        .with_debug_output(Arc::clone(&trace));
    let request =
        Request::new(core, BytesEndpoint::traced(Arc::clone(&trace))).debug(DebugLevel::Headers);
    let task = tokio::spawn(async move { request.await });

    transport_gate.wait_until_reached().await;
    let before_transport = trace.lock().expect("trace lock").clone();
    assert_eq!(before_transport[0], "head preparation");
    assert_eq!(before_transport[1], "before_send");
    assert!(before_transport[2].starts_with("request-ready"));
    assert_eq!(executor.application().request_count().expect("count"), 0);

    transport_gate.open();
    assert_eq!(
        task.await.expect("join").expect("ordered bytes"),
        b"ordered"[..]
    );
    let trace = trace.lock().expect("trace lock");
    assert!(trace[3].starts_with("response-head"));
    assert_eq!(trace[4], "on_response_head");
    assert!(trace[5].starts_with("completion"));
    assert_eq!(trace[6], "on_complete");
    assert_eq!(trace.len(), 7);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn before_send_veto_prevents_native_execution_and_later_response_phases() {
    let executor = DeterministicExecutor::new();
    let mut probe = ProbeHooks::new();
    probe.veto = AtomicBool::new(true);
    let hooks = Arc::new(probe);
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core).await.expect_err("hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let records = hooks.records();
    assert!(matches!(
        records.as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::Hook,
                status: None,
                ..
            }
        ]
    ));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn vetoed_buffered_request_does_not_invoke_its_encoder() {
    let executor = DeterministicExecutor::new();
    let mut probe = ProbeHooks::new();
    probe.veto = AtomicBool::new(true);
    let hooks = Arc::new(probe);
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        core,
        probe_input(b"veto-body-secret-78f4", &encoder_calls),
    )
    .await
    .expect_err("hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    assert!(matches!(
        hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Buffered,
            known_body_length: None,
            ..
        })
    ));
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("veto-body-secret-78f4"));
    }
}

#[tokio::test]
async fn request_stream_and_factory_remain_lazy_through_hook_veto() {
    let stream_executor = DeterministicExecutor::new();
    let mut stream_probe = ProbeHooks::new();
    stream_probe.veto = AtomicBool::new(true);
    let stream_hooks = Arc::new(stream_probe);
    let stream_core = development_core(
        &stream_executor,
        Arc::clone(&stream_hooks),
        RuntimeConfigBuilder::new(),
    );
    let stream_polls = Arc::new(AtomicUsize::new(0));
    let stream = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"lazy-stream-secret-a8ef")],
        &stream_polls,
    ))
    .exact_length(23);
    assert_eq!(stream_polls.load(Ordering::SeqCst), 0);
    let request = body_request(stream_core, RequestBody::stream(stream));
    assert_eq!(stream_polls.load(Ordering::SeqCst), 0);
    let error = request.await.expect_err("stream hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(stream_polls.load(Ordering::SeqCst), 0);
    assert_eq!(stream_executor.application().request_count().unwrap(), 0);
    assert!(matches!(
        stream_hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Stream,
            known_body_length: Some(23),
            ..
        })
    ));

    let factory_executor = DeterministicExecutor::new();
    let mut factory_probe = ProbeHooks::new();
    factory_probe.veto = AtomicBool::new(true);
    let factory_hooks = Arc::new(factory_probe);
    let factory_core = development_core(
        &factory_executor,
        Arc::clone(&factory_hooks),
        RuntimeConfigBuilder::new(),
    );
    let content_type_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let factory_polls = Arc::new(AtomicUsize::new(0));
    let request = body_request(
        factory_core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::clone(&content_type_calls),
            create_calls: Arc::clone(&create_calls),
            polls: Arc::clone(&factory_polls),
            fail: false,
        }),
    );
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_polls.load(Ordering::SeqCst), 0);
    let error = request.await.expect_err("factory hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_polls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_executor.application().request_count().unwrap(), 0);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_stream_and_factory_remain_lazy_through_hook_veto() {
    let direct_executor = DeterministicExecutor::new();
    let mut direct_probe = ProbeHooks::new();
    direct_probe.veto = AtomicBool::new(true);
    let direct_hooks = Arc::new(direct_probe);
    let direct_core = development_core(
        &direct_executor,
        Arc::clone(&direct_hooks),
        RuntimeConfigBuilder::new(),
    );
    let polls = Arc::new(AtomicUsize::new(0));
    let stream = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"multipart-lazy-stream-secret")],
        &polls,
    ))
    .exact_length(28);
    let body = RequestBody::multipart(
        MultipartBody::new()
            .part("stream-field", MultipartPart::stream(stream))
            .expect("multipart stream part"),
    );
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let error = body_request(direct_core, body)
        .await
        .expect_err("multipart hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(direct_executor.application().request_count().unwrap(), 0);
    assert!(matches!(
        direct_hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Multipart,
            ..
        })
    ));

    let factory_executor = DeterministicExecutor::new();
    let mut factory_probe = ProbeHooks::new();
    factory_probe.veto = AtomicBool::new(true);
    let factory_hooks = Arc::new(factory_probe);
    let factory_core = development_core(
        &factory_executor,
        Arc::clone(&factory_hooks),
        RuntimeConfigBuilder::new(),
    );
    let create_calls = Arc::new(AtomicUsize::new(0));
    let body = RequestBody::multipart_factory(ProbeMultipartFactory {
        create_calls: Arc::clone(&create_calls),
        fail: false,
    });
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    let error = body_request(factory_core, body)
        .await
        .expect_err("multipart factory hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_executor.application().request_count().unwrap(), 0);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_aggregate_framing_limit_and_factory_failure_are_pre_native() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(64).expect("positive limit")),
    );
    let body = RequestBody::multipart(
        MultipartBody::new()
            .part("field", MultipartPart::text("x"))
            .expect("multipart part"),
    );
    let error = body_request(core, body)
        .await
        .expect_err("multipart framing exceeds the request limit");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(executor.application().request_count().unwrap(), 0);
    assert!(matches!(
        hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Multipart,
            known_body_length: Some(1),
            ..
        })
    ));

    let failing_executor = DeterministicExecutor::new();
    let failing_hooks = Arc::new(ProbeHooks::new());
    let failing_core = development_core(
        &failing_executor,
        Arc::clone(&failing_hooks),
        RuntimeConfigBuilder::new(),
    );
    let create_calls = Arc::new(AtomicUsize::new(0));
    let error = body_request(
        failing_core,
        RequestBody::multipart_factory(ProbeMultipartFactory {
            create_calls: Arc::clone(&create_calls),
            fail: true,
        }),
    )
    .await
    .expect_err("multipart factory failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_executor.application().request_count().unwrap(), 0);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("multipart-factory-error-secret-88a1"));
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_stream_failure_is_sanitized_and_request_body_primary() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    let polls = Arc::new(AtomicUsize::new(0));
    let stream = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Failure("multipart-producer-error-secret-9b31")],
        &polls,
    ));
    let body = RequestBody::multipart(
        MultipartBody::new()
            .part("stream", MultipartPart::stream(stream))
            .expect("multipart stream part"),
    );
    let error = body_request(core, body)
        .await
        .expect_err("multipart producer failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().unwrap(), 0);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("multipart-producer-error-secret-9b31"));
}

#[tokio::test]
async fn managed_request_stream_enforces_exact_length_and_aggregate_limit() {
    let success_executor = DeterministicExecutor::new();
    let success_hooks = Arc::new(ProbeHooks::new());
    let success_core = development_core(
        &success_executor,
        Arc::clone(&success_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(5).expect("positive limit")),
    );
    success_executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .safe_url("http://execution.example.test/v1/body/upload")
                .body("abcde"),
            ScriptedResponse::success("accepted"),
        )
        .expect("stream success script");
    let success_polls = Arc::new(AtomicUsize::new(0));
    let success_body = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"ab"), ProbeFrame::Data(b"cde")],
        &success_polls,
    ))
    .exact_length(5);
    assert_eq!(
        body_request(success_core, RequestBody::stream(success_body))
            .await
            .expect("exact stream success"),
        b"accepted"[..]
    );
    assert_eq!(success_polls.load(Ordering::SeqCst), 3);
    let captures = success_executor.application().captures().unwrap();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].body_category(), BodyCategory::Streaming);
    assert_eq!(captures[0].known_body_length(), Some(5));

    let preflight_executor = DeterministicExecutor::new();
    let preflight_hooks = Arc::new(ProbeHooks::new());
    let preflight_core = development_core(
        &preflight_executor,
        Arc::clone(&preflight_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let preflight_polls = Arc::new(AtomicUsize::new(0));
    let preflight_body = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"never-polled")],
        &preflight_polls,
    ))
    .exact_length(5);
    let error = body_request(preflight_core, RequestBody::stream(preflight_body))
        .await
        .expect_err("known impossible stream size");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(preflight_polls.load(Ordering::SeqCst), 0);
    assert_eq!(preflight_executor.application().request_count().unwrap(), 0);
    assert!(preflight_hooks.records().iter().all(|record| !matches!(
        record,
        LifecycleRecord::Before { .. } | LifecycleRecord::Response(..)
    )));

    async fn managed_failure(
        body: StreamBody,
        polls: &Arc<AtomicUsize>,
        expected_polls: usize,
        limit: usize,
    ) -> ApiError {
        let executor = DeterministicExecutor::new();
        let hooks = Arc::new(ProbeHooks::new());
        let core = development_core(
            &executor,
            hooks,
            RuntimeConfigBuilder::new()
                .request_body_limit(NonZeroUsize::new(limit).expect("positive limit")),
        );
        let error = body_request(core, RequestBody::stream(body))
            .await
            .expect_err("managed stream failure");
        assert_eq!(error.category(), ErrorCategory::RequestBody);
        assert_eq!(polls.load(Ordering::SeqCst), expected_polls);
        assert_eq!(executor.application().request_count().unwrap(), 0);
        error
    }

    let underflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        StreamBody::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc")],
            &underflow_polls,
        ))
        .exact_length(4),
        &underflow_polls,
        2,
        64,
    )
    .await;

    let exact_overflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        StreamBody::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc"), ProbeFrame::Data(b"de")],
            &exact_overflow_polls,
        ))
        .exact_length(4),
        &exact_overflow_polls,
        2,
        64,
    )
    .await;

    let limit_overflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        StreamBody::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc"), ProbeFrame::Data(b"de")],
            &limit_overflow_polls,
        )),
        &limit_overflow_polls,
        2,
        4,
    )
    .await;
}

#[tokio::test]
async fn producer_failure_is_request_body_primary_and_body_free() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    let polls = Arc::new(AtomicUsize::new(0));
    let body = StreamBody::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"accepted-prefix-secret-029b"),
            ProbeFrame::Failure("producer-authored-secret-80d1"),
        ],
        &polls,
    ));
    let error = body_request(core, RequestBody::stream(body))
        .await
        .expect_err("producer failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 0);
    let recursive = format!("{error:?} {error}");
    assert!(!recursive.contains("accepted-prefix-secret-029b"));
    assert!(!recursive.contains("producer-authored-secret-80d1"));
}

#[tokio::test]
async fn body_factory_is_created_once_after_approval_and_failure_is_pre_native() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .safe_url("http://execution.example.test/v1/body/upload")
                .body("factory"),
            ScriptedResponse::success("factory-ok"),
        )
        .expect("factory script");
    let content_type_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let request = body_request(
        core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::clone(&content_type_calls),
            create_calls: Arc::clone(&create_calls),
            polls: Arc::clone(&polls),
            fail: false,
        }),
    );
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(request.await.expect("factory request"), b"factory-ok"[..]);
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);

    let failing_executor = DeterministicExecutor::new();
    let failing_hooks = Arc::new(ProbeHooks::new());
    let failing_core = development_core(
        &failing_executor,
        failing_hooks,
        RuntimeConfigBuilder::new(),
    );
    let failing_content_type_calls = Arc::new(AtomicUsize::new(0));
    let failing_create_calls = Arc::new(AtomicUsize::new(0));
    let failing_polls = Arc::new(AtomicUsize::new(0));
    let error = body_request(
        failing_core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::clone(&failing_content_type_calls),
            create_calls: Arc::clone(&failing_create_calls),
            polls: Arc::clone(&failing_polls),
            fail: true,
        }),
    )
    .await
    .expect_err("factory creation failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(failing_content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_polls.load(Ordering::SeqCst), 0);
    assert_eq!(failing_executor.application().request_count().unwrap(), 0);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("safe factory configuration failure"));
}

#[tokio::test]
async fn bytes_reader_and_file_stream_constructors_use_the_same_native_path() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    for (expected, response) in [
        (b"bytes-source".as_slice(), "bytes-ok"),
        (b"reader-source".as_slice(), "reader-ok"),
        (b"file-source".as_slice(), "file-ok"),
    ] {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .method(Method::POST)
                    .safe_url("http://execution.example.test/v1/body/upload")
                    .body(Bytes::copy_from_slice(expected)),
                ScriptedResponse::success(response),
            )
            .expect("constructor script");
    }

    assert_eq!(
        body_request(
            core.clone(),
            RequestBody::stream(StreamBody::from_bytes(Bytes::from_static(b"bytes-source"))),
        )
        .await
        .expect("bytes stream"),
        b"bytes-ok"[..]
    );
    assert_eq!(
        body_request(
            core.clone(),
            RequestBody::stream(
                StreamBody::from_reader(std::io::Cursor::new(b"reader-source".to_vec()))
                    .exact_length(13),
            ),
        )
        .await
        .expect("reader stream"),
        b"reader-ok"[..]
    );

    let file_path = std::env::temp_dir().join(format!(
        "concord-request-stream-{}-{}.tmp",
        std::process::id(),
        BODY_ENDPOINT.len()
    ));
    tokio::fs::write(&file_path, b"file-source")
        .await
        .expect("write stream fixture");
    let file_body = StreamBody::from_file(&file_path)
        .await
        .expect("open stream fixture");
    let file_result = body_request(core, RequestBody::stream(file_body)).await;
    tokio::fs::remove_file(&file_path)
        .await
        .expect("remove stream fixture");
    assert_eq!(file_result.expect("file stream"), b"file-ok"[..]);
    assert_eq!(executor.application().request_count().unwrap(), 3);
}

fn stream_headers(content_type: &'static str) -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(content_type),
    );
    headers
}

fn server_stream_headers(content_type: &str) -> Vec<(String, String)> {
    vec![("Content-Type".to_owned(), content_type.to_owned())]
}

#[tokio::test]
async fn stream_response_carries_metadata_and_completes_once_when_opened() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut headers = stream_headers("application/octet-stream");
    headers.insert(
        "x-stream-sentinel",
        http::HeaderValue::from_static("stream-header-secret-c62f"),
    );
    executor
        .application()
        .queue_response(ScriptedResponse::success("stream-value-secret-8d0a").headers(headers))
        .expect("stream response script");

    let response = stream_request::<OctetStream>(core)
        .response()
        .await
        .expect("stream response");
    assert_eq!(
        response.url().as_str(),
        "http://execution.example.test/v1/stream/download"
    );
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), STREAM_ENDPOINT);
    assert_eq!(response.metadata().method(), Method::GET);
    assert_eq!(response.metadata().page_index(), 0);
    assert!(!response.metadata().recovered_authentication());
    assert_eq!(
        response.headers()["x-stream-sentinel"],
        "stream-header-secret-c62f"
    );
    let records = hooks.records();
    assert_eq!(
        completion(&records).1,
        CompletionOutcome::StreamOpened,
        "completion must describe stream creation, not consumption"
    );
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);

    let debug = format!("{response:?}");
    for forbidden in [
        "stream-value-secret-8d0a",
        "stream-header-secret-c62f",
        "execution.example.test",
        "/stream/download",
    ] {
        assert!(
            !debug.contains(forbidden),
            "stream Debug leaked {forbidden}"
        );
    }
    let mut response = response.into_value();
    assert_eq!(
        next_stream_chunk(&mut response)
            .await
            .expect("stream chunk")
            .expect("valid chunk"),
        b"stream-value-secret-8d0a"[..]
    );
    assert!(next_stream_chunk(&mut response).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stream_response_known_length_and_media_fail_before_body_polling() {
    let known_gate = BodyGate::closed();
    let known_server = ScriptedServer::start([ResponseScript::GatedBodyWithHeaders {
        status: 200,
        headers: server_stream_headers("application/octet-stream"),
        body: b"known-excess-secret-f45c".to_vec(),
        gate: known_gate.clone(),
    }])
    .expect("known stream server");
    let known_core = real_core(
        &known_server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .stream_response_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(known_core),
    )
    .await
    .expect("known excess must not await the body")
    .expect_err("known stream excess");
    known_gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("known-excess-secret-f45c"));

    let media_gate = BodyGate::closed();
    let media_server = ScriptedServer::start([ResponseScript::GatedBodyWithHeaders {
        status: 200,
        headers: server_stream_headers("text/plain"),
        body: b"media-mismatch-secret-18bf".to_vec(),
        gate: media_gate.clone(),
    }])
    .expect("media stream server");
    let media_core = real_core(
        &media_server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(media_core),
    )
    .await
    .expect("media mismatch must not await the body")
    .expect_err("stream media mismatch");
    media_gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert!(!format!("{error:?} {error}").contains("media-mismatch-secret-18bf"));
}

#[tokio::test]
async fn invalid_stream_marker_is_rejected_preflight_and_required_parameters_are_enforced() {
    let invalid_executor = DeterministicExecutor::new();
    let invalid_hooks = Arc::new(ProbeHooks::new());
    let invalid_core = development_core(
        &invalid_executor,
        Arc::clone(&invalid_hooks),
        RuntimeConfigBuilder::new(),
    );
    let error = stream_request::<InvalidStreamMedia>(invalid_core)
        .await
        .expect_err("invalid media marker");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert_eq!(invalid_executor.application().request_count().unwrap(), 0);
    assert!(invalid_hooks.records().iter().all(|record| !matches!(
        record,
        LifecycleRecord::Before { .. } | LifecycleRecord::Response(..)
    )));

    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    executor
        .application()
        .queue_response(ScriptedResponse::success("vendor").headers(stream_headers(
            "application/vnd.concord.stream; charset=utf-8; version=1",
        )))
        .expect("valid vendor media");
    let mut accepted = stream_request::<VendorStream>(core.clone())
        .await
        .expect("required media parameter accepted with extras");
    assert_eq!(
        next_stream_chunk(&mut accepted)
            .await
            .expect("vendor chunk")
            .expect("vendor chunk success"),
        b"vendor"[..]
    );

    executor
        .application()
        .queue_response(
            ScriptedResponse::success("must-not-be-yielded")
                .headers(stream_headers("application/vnd.concord.stream; version=2")),
        )
        .expect("invalid vendor media");
    let error = stream_request::<VendorStream>(core)
        .await
        .expect_err("required media parameter mismatch");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
}

#[tokio::test]
async fn unknown_stream_is_lazy_and_first_cumulative_excess_chunk_is_withheld() {
    let lazy_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedChunkedBodyWithHeaders {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"ab".to_vec(), b"cd".to_vec()],
            gate: lazy_gate.clone(),
        },
        ResponseScript::Chunked {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"abc".to_vec(), b"first-excess-secret-4fa2".to_vec()],
        },
    ])
    .expect("unknown stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(
        &server,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .response_body_limit(NonZeroUsize::new(1).expect("positive limit"))
            .stream_response_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let mut lazy = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(core.clone()),
    )
    .await
    .expect("unknown stream must return before body gate")
    .expect("lazy stream response");
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
    lazy_gate.release();
    assert_eq!(
        next_stream_chunk(&mut lazy).await.unwrap().unwrap(),
        b"ab"[..]
    );
    assert_eq!(
        next_stream_chunk(&mut lazy).await.unwrap().unwrap(),
        b"cd"[..]
    );
    assert!(next_stream_chunk(&mut lazy).await.is_none());

    let mut overflow = stream_request::<OctetStream>(core)
        .await
        .expect("overflow stream opens");
    assert_eq!(
        next_stream_chunk(&mut overflow).await.unwrap().unwrap(),
        b"abc"[..]
    );
    let error = next_stream_chunk(&mut overflow)
        .await
        .expect("overflow result")
        .expect_err("first excess chunk withheld");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("first-excess-secret-4fa2"));
    assert!(next_stream_chunk(&mut overflow).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
    assert!(
        hooks
            .records()
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Complete { .. }))
            .all(|record| matches!(
                record,
                LifecycleRecord::Complete {
                    outcome: CompletionOutcome::StreamOpened,
                    ..
                }
            ))
    );
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn response_stream_limit_counts_reqwest_post_decompression_bytes() {
    const GZIP_128_X: &[u8] = &[
        31, 139, 8, 0, 0, 0, 0, 0, 0, 10, 171, 168, 24, 88, 0, 0, 55, 138, 224, 172, 128, 0, 0, 0,
    ];
    let server = ScriptedServer::start([ResponseScript::Complete {
        status: 200,
        headers: vec![
            ("Content-Encoding".into(), "gzip".into()),
            ("Content-Type".into(), "application/octet-stream".into()),
        ],
        body: GZIP_128_X.to_vec(),
    }])
    .expect("compressed stream server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .stream_response_limit(NonZeroUsize::new(64).expect("positive limit")),
    );
    let mut response = stream_request::<OctetStream>(core)
        .await
        .expect("compressed stream opens");
    let error = next_stream_chunk(&mut response)
        .await
        .expect("post-decompression limit result")
        .expect_err("128 decoded bytes exceed a 64-byte stream limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(next_stream_chunk(&mut response).await.is_none());
}

#[tokio::test]
async fn response_stream_native_failure_is_safe_and_emits_no_second_completion() {
    let server = ScriptedServer::start([ResponseScript::TruncatedWithHeaders {
        status: 200,
        declared_length: 6,
        prefix: b"short".to_vec(),
        headers: server_stream_headers("application/octet-stream"),
    }])
    .expect("truncated stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut response = stream_request::<OctetStream>(core)
        .await
        .expect("truncated stream opens after valid head");
    let mut saw_native_error = None;
    while let Some(chunk) = next_stream_chunk(&mut response).await {
        match chunk {
            Ok(bytes) => assert_eq!(bytes, b"short"[..]),
            Err(error) => {
                saw_native_error = Some(error);
                break;
            }
        }
    }
    let error = saw_native_error.expect("truncation must become a stream item error");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("short"));
    assert!(next_stream_chunk(&mut response).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
}

#[tokio::test]
async fn caller_stream_polling_is_outside_operation_deadline_and_drop_is_terminal() {
    let deadline_gate = BodyGate::closed();
    let drop_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedChunkedBodyWithHeaders {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"after-deadline".to_vec()],
            gate: deadline_gate.clone(),
        },
        ResponseScript::GatedChunkedWithHeaders {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            first_chunk: b"first".to_vec(),
            remaining_chunk: b"must-not-be-observed".to_vec(),
            gate: drop_gate.clone(),
        },
    ])
    .expect("deadline and drop stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut after_deadline = stream_request::<OctetStream>(core.clone())
        .timeout(Duration::from_millis(100))
        .await
        .expect("stream opens inside deadline");
    tokio::time::sleep(Duration::from_millis(180)).await;
    deadline_gate.release();
    assert_eq!(
        next_stream_chunk(&mut after_deadline)
            .await
            .expect("post-deadline chunk")
            .expect("post-deadline polling is allowed"),
        b"after-deadline"[..]
    );
    assert!(next_stream_chunk(&mut after_deadline).await.is_none());

    let mut dropped = stream_request::<OctetStream>(core)
        .await
        .expect("drop stream opens");
    assert_eq!(
        next_stream_chunk(&mut dropped).await.unwrap().unwrap(),
        b"first"[..]
    );
    drop(dropped);
    drop_gate.release();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
    assert!(
        hooks
            .records()
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Complete { .. }))
            .all(|record| matches!(
                record,
                LifecycleRecord::Complete {
                    outcome: CompletionOutcome::StreamOpened,
                    ..
                }
            ))
    );
}

#[cfg(feature = "records")]
#[tokio::test]
async fn records_response_validates_starts_unpolled_and_completes_stream_opened_once() {
    OPENING_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    OPENING_START_CALLS.store(0, Ordering::SeqCst);
    OPENING_DECODE_CALLS.store(0, Ordering::SeqCst);
    OPENING_EOF_CALLS.store(0, Ordering::SeqCst);
    OPENING_DECODER_DROPS.store(0, Ordering::SeqCst);

    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("opening-record-body-sentinel-0dd64a\n")
                .headers(record_headers(
                    "application/x-concord-records",
                    "opening-header-sentinel-762db1",
                ))
                .body_poll_counter(Arc::clone(&body_polls))
                .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("record opening response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter {
        response_body_polls: Some(Arc::clone(&body_polls)),
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .rate_limiter(configured)
            .debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));

    let records =
        surface_request::<Records<OpeningRecordDecoder>>(core, "opening-query-sentinel-4e15b9")
            .await
            .expect("record stream opens");

    assert_eq!(OPENING_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(OPENING_START_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(OPENING_DECODE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(OPENING_EOF_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(OPENING_DECODER_DROPS.load(Ordering::SeqCst), 0);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(body_drops.load(Ordering::SeqCst), 0);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        1
    );
    let lifecycle = hooks.records();
    assert!(matches!(
        lifecycle.as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Response(_, StatusCode::OK),
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::StreamOpened,
                status: Some(StatusCode::OK),
                ..
            }
        ]
    ));
    assert_eq!(completion_count(&lifecycle), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    let automatic = format!(
        "{:?}\n{:?}\n{}\n{records:?}",
        lifecycle,
        limiter.request_records(),
        debug.lock().expect("record opening debug").join("\n"),
    );
    assert_omits(
        &automatic,
        &[
            "opening-record-body-sentinel-0dd64a",
            "opening-header-sentinel-762db1",
            "opening-query-sentinel-4e15b9",
        ],
    );

    drop(records);
    assert_eq!(OPENING_DECODER_DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "records")]
#[tokio::test]
async fn record_stream_body_and_decoder_failures_are_items_without_second_completion() {
    FAILURE_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    FAILURE_START_CALLS.store(0, Ordering::SeqCst);
    FAILURE_DECODE_CALLS.store(0, Ordering::SeqCst);
    FAILURE_EOF_CALLS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::new())
                .headers(record_headers(
                    "application/x-concord-records",
                    "failure-header-secret-58ca11",
                ))
                .body_read_failure("body-read-source-sentinel-4a60de"),
        )
        .expect("body failure response");
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("decoder-framing-body-sentinel-a14f2b").headers(
                record_headers(
                    "application/x-concord-records",
                    "failure-header-secret-58ca11",
                ),
            ),
        )
        .expect("decoder failure response");
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("decoder-eof-body-sentinel-72b59c").headers(record_headers(
                "application/x-concord-records",
                "failure-header-secret-58ca11",
            )),
        )
        .expect("EOF failure response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );

    FAILURE_MODE.store(0, Ordering::SeqCst);
    let mut body_failure = surface_request::<Records<FailingRecordDecoder>>(
        core.clone(),
        "failure-query-secret-7a184e",
    )
    .await
    .expect("body-failure stream opens");
    assert_eq!(completion_count(&hooks.records()), 1);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
    let error = next_record(&mut body_failure)
        .await
        .expect("body failure item")
        .expect_err("body read fails after opening");
    assert_eq!(error.kind(), RecordErrorKind::Body);
    assert_omits(
        &recursive_error_surfaces(&error),
        &[
            "body-read-source-sentinel-4a60de",
            "failure-header-secret-58ca11",
            "failure-query-secret-7a184e",
        ],
    );
    assert_eq!(completion_count(&hooks.records()), 1);

    FAILURE_MODE.store(1, Ordering::SeqCst);
    let mut decode_failure = surface_request::<Records<FailingRecordDecoder>>(
        core.clone(),
        "failure-query-secret-7a184e",
    )
    .await
    .expect("decode-failure stream opens");
    assert_eq!(completion_count(&hooks.records()), 2);
    let error = next_record(&mut decode_failure)
        .await
        .expect("decode failure item")
        .expect_err("decoder framing fails after opening");
    assert_eq!(error.kind(), RecordErrorKind::InvalidFraming);
    assert_omits(
        &recursive_error_surfaces(&error),
        &["decoder-framing-body-sentinel-a14f2b"],
    );
    assert_eq!(completion_count(&hooks.records()), 2);

    FAILURE_MODE.store(2, Ordering::SeqCst);
    let mut eof_failure =
        surface_request::<Records<FailingRecordDecoder>>(core, "failure-query-secret-7a184e")
            .await
            .expect("EOF-failure stream opens");
    assert_eq!(completion_count(&hooks.records()), 3);
    let error = next_record(&mut eof_failure)
        .await
        .expect("EOF failure item")
        .expect_err("decoder EOF fails after opening");
    assert_eq!(error.kind(), RecordErrorKind::IncompleteRecord);
    assert_omits(
        &recursive_error_surfaces(&error),
        &["decoder-eof-body-sentinel-72b59c"],
    );
    let lifecycle = hooks.records();
    assert_eq!(completion_count(&lifecycle), 3);
    assert!(
        lifecycle
            .iter()
            .filter_map(|record| match record {
                LifecycleRecord::Complete { outcome, .. } => Some(outcome),
                _ => None,
            })
            .all(|outcome| *outcome == CompletionOutcome::StreamOpened)
    );
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 3);
    assert_eq!(FAILURE_VALIDATE_CALLS.load(Ordering::SeqCst), 3);
    assert_eq!(FAILURE_START_CALLS.load(Ordering::SeqCst), 3);
    assert!(FAILURE_DECODE_CALLS.load(Ordering::SeqCst) >= 5);
    assert_eq!(FAILURE_EOF_CALLS.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "records")]
#[tokio::test]
async fn dropping_record_stream_synchronously_drops_decoder_staging_and_byte_stream() {
    STREAM_DROP_DECODER_DROPS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("stream-drop-record-sentinel-15f80a")
                .body_poll_counter(Arc::clone(&body_polls))
                .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("record-stream drop response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let stream =
        surface_request::<Records<StreamDropDecoder>>(core, "stream-drop-query-sentinel-f81d0a")
            .await
            .expect("record stream opens before drop");
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(body_drops.load(Ordering::SeqCst), 0);
    assert_eq!(STREAM_DROP_DECODER_DROPS.load(Ordering::SeqCst), 0);
    assert_eq!(completion_count(&hooks.records()), 1);

    drop(stream);
    assert_eq!(STREAM_DROP_DECODER_DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(completion_count(&hooks.records()), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "records")]
#[tokio::test]
async fn dropping_record_batches_synchronously_drops_record_stream_and_byte_stream() {
    BATCH_DROP_DECODER_DROPS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("batch-drop-record-sentinel-a7f302")
                .body_poll_counter(Arc::clone(&body_polls))
                .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("record-batches drop response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let stream =
        surface_request::<Records<BatchDropDecoder>>(core, "batch-drop-query-sentinel-46d31c")
            .await
            .expect("record stream opens before batching");
    let batches = stream.batches(NonZeroUsize::new(2).expect("positive batch"));
    assert_omits(
        &format!("{batches:?}"),
        &[
            "batch-drop-record-sentinel-a7f302",
            "batch-drop-query-sentinel-46d31c",
        ],
    );
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(body_drops.load(Ordering::SeqCst), 0);
    assert_eq!(BATCH_DROP_DECODER_DROPS.load(Ordering::SeqCst), 0);

    drop(batches);
    assert_eq!(BATCH_DROP_DECODER_DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(completion_count(&hooks.records()), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "records")]
#[tokio::test]
async fn record_payloads_never_enter_hooks_debug_limiter_completion_or_sources() {
    let executor = DeterministicExecutor::new();
    let record_body = format!(
        "{RECORD_RAW_SENTINEL},{RECORD_CSV_HEADER_SENTINEL},{RECORD_CSV_FIELD_SENTINEL},{{\"value\":\"{RECORD_NDJSON_SENTINEL}\"}}\nparser-trigger-{RECORD_PARSER_SENTINEL}\n"
    );
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(record_body.clone()).headers(record_headers(
                "application/x-concord-records",
                RECORD_HEADER_SENTINEL,
            )),
        )
        .expect("diagnostic record response");
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(format!("invalid-media-{RECORD_RAW_SENTINEL}"))
                .headers(record_headers("text/plain", RECORD_HEADER_SENTINEL)),
        )
        .expect("diagnostic invalid-media response");
    let hooks = Arc::new(AutomaticSurfaceHooks::default());
    let limiter = Arc::new(AutomaticSurfaceLimiter::default());
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = automatic_surface_core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        Arc::clone(&debug),
    );

    let response: ApiResponse<RecordStream<DiagnosticRecordDecoder>> =
        surface_request::<Records<DiagnosticRecordDecoder>>(core.clone(), RECORD_QUERY_SENTINEL)
            .response()
            .await
            .expect("diagnostic record stream opens");
    let response_debug = format!("{response:?}");
    let stream = response.into_value();
    let stream_debug = format!("{stream:?}");
    let mut batches = stream.batches(NonZeroUsize::new(1).expect("positive diagnostic batch"));
    let batches_debug = format!("{batches:?}");
    let selected = next_batch(&mut batches)
        .await
        .expect("selected record batch")
        .expect("selected record succeeds");
    assert_eq!(selected.len(), 1);
    let selected_debug = format!("{:?}", selected[0]);
    for expected in [
        RECORD_RAW_SENTINEL,
        RECORD_CSV_HEADER_SENTINEL,
        RECORD_CSV_FIELD_SENTINEL,
        RECORD_NDJSON_SENTINEL,
    ] {
        assert!(selected_debug.contains(expected));
    }
    let record_error = next_batch(&mut batches)
        .await
        .expect("record error batch item")
        .expect_err("parser failure remains a record item");
    assert_eq!(record_error.kind(), RecordErrorKind::InvalidFraming);
    let record_error_surfaces = recursive_error_surfaces(&record_error);
    assert_eq!(hooks.outcomes(), [CompletionOutcome::StreamOpened]);

    let api_error =
        surface_request::<Records<DiagnosticRecordDecoder>>(core, RECORD_QUERY_SENTINEL)
            .await
            .expect_err("invalid record media is an endpoint error");
    assert_eq!(api_error.category(), ErrorCategory::ResponseContract);
    let api_error_surfaces = recursive_error_surfaces(&api_error);
    assert_eq!(
        hooks.outcomes(),
        [
            CompletionOutcome::StreamOpened,
            CompletionOutcome::ResponseContract,
        ]
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 2);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 2);
    let captures = executor.application().captures().expect("record captures");
    let automatic = format!(
        "{}\n{:?}\n{:?}\n{response_debug}\n{stream_debug}\n{batches_debug}\n{record_error_surfaces}\n{api_error_surfaces}",
        debug.lock().expect("record debug output").join("\n"),
        hooks.entries(),
        limiter.entries(),
    ) + &format!("\n{captures:?}");
    assert_omits(
        &automatic,
        &[
            RECORD_RAW_SENTINEL,
            RECORD_CSV_HEADER_SENTINEL,
            RECORD_CSV_FIELD_SENTINEL,
            RECORD_NDJSON_SENTINEL,
            RECORD_DECODER_STATE_SENTINEL,
            RECORD_PARSER_SENTINEL,
            RECORD_HEADER_SENTINEL,
            RECORD_QUERY_SENTINEL,
        ],
    );
}

#[cfg(feature = "records")]
#[tokio::test]
async fn response_payloads_never_enter_hooks_debug_limiter_completion_or_wrapper_debug() {
    const BUFFERED_BODY: &str = "response-buffered-body-sentinel-c48a09";
    const CUSTOM_OUTPUT: &str = "response-custom-output-sentinel-815fce";
    const STREAM_BODY: &str = "response-byte-stream-sentinel-369d0b";
    const RECORD_BODY: &str = "response-record-value-sentinel-5a903c";
    const HEADER_VALUE: &str = "response-header-value-sentinel-1928de";
    const QUERY_VALUE: &str = "response-query-value-sentinel-61f32a";
    const PARSER_TEXT: &str = "response-parser-text-sentinel-11fa62";

    let mut buffered_headers = http::HeaderMap::new();
    buffered_headers.insert(
        "x-response-secret",
        http::HeaderValue::from_static(HEADER_VALUE),
    );
    let mut stream_headers = buffered_headers.clone();
    stream_headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/octet-stream"),
    );
    let executor = DeterministicExecutor::new();
    for response in [
        ScriptedResponse::success(BUFFERED_BODY).headers(buffered_headers.clone()),
        ScriptedResponse::success(CUSTOM_OUTPUT).headers(buffered_headers.clone()),
        ScriptedResponse::success(STREAM_BODY).headers(stream_headers),
        ScriptedResponse::success(format!("{RECORD_BODY}\n")).headers(buffered_headers),
    ] {
        executor
            .application()
            .queue_response(response)
            .expect("response diagnostic script");
    }
    let hooks = Arc::new(AutomaticSurfaceHooks::default());
    let limiter = Arc::new(AutomaticSurfaceLimiter::default());
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = automatic_surface_core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        Arc::clone(&debug),
    );

    let error = surface_request::<FailingBufferedSpec>(core.clone(), QUERY_VALUE)
        .await
        .expect_err("custom parser error is reduced");
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    let error_surfaces = recursive_error_surfaces(&error);

    let buffered: ApiResponse<SensitiveBufferedOutput> =
        surface_request::<SensitiveBufferedSpec>(core.clone(), QUERY_VALUE)
            .response()
            .await
            .expect("custom buffered output");
    assert_eq!(buffered.value().0, CUSTOM_OUTPUT);
    let buffered_debug = format!("{buffered:?}");

    let stream_response: ApiResponse<ByteStream> =
        surface_request::<StreamSpec<OctetStream>>(core.clone(), QUERY_VALUE)
            .response()
            .await
            .expect("byte stream output");
    let stream_wrapper_debug = format!("{stream_response:?}");
    let mut stream = stream_response.into_value();
    let stream_debug = format!("{stream:?}");
    assert_eq!(
        next_stream_chunk(&mut stream)
            .await
            .expect("stream response chunk")
            .expect("stream response bytes"),
        Bytes::from_static(STREAM_BODY.as_bytes())
    );
    assert!(next_stream_chunk(&mut stream).await.is_none());

    let record_response: ApiResponse<RecordStream<ResponseSurfaceRecordDecoder>> =
        surface_request::<Records<ResponseSurfaceRecordDecoder>>(core, QUERY_VALUE)
            .response()
            .await
            .expect("record response output");
    let record_wrapper_debug = format!("{record_response:?}");
    let record_stream = record_response.into_value();
    let record_stream_debug = format!("{record_stream:?}");
    let mut record_batches =
        record_stream.batches(NonZeroUsize::new(1).expect("positive response batch"));
    let record_batches_debug = format!("{record_batches:?}");
    let selected = next_batch(&mut record_batches)
        .await
        .expect("response record batch")
        .expect("response record output");
    assert_eq!(selected.len(), 1);
    assert!(format!("{:?}", selected[0]).contains(RECORD_BODY));

    assert_eq!(
        hooks.outcomes(),
        [
            CompletionOutcome::ResponseDecoding,
            CompletionOutcome::Success,
            CompletionOutcome::StreamOpened,
            CompletionOutcome::StreamOpened,
        ]
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 4);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 4);
    let captures = executor
        .application()
        .captures()
        .expect("response captures");
    let automatic = format!(
        "{}\n{:?}\n{:?}\n{:?}\n{error_surfaces}\n{buffered_debug}\n{stream_wrapper_debug}\n{stream_debug}\n{record_wrapper_debug}\n{record_stream_debug}\n{record_batches_debug}",
        debug.lock().expect("response debug output").join("\n"),
        hooks.entries(),
        limiter.entries(),
        captures,
    );
    assert_omits(
        &automatic,
        &[
            BUFFERED_BODY,
            CUSTOM_OUTPUT,
            STREAM_BODY,
            RECORD_BODY,
            HEADER_VALUE,
            QUERY_VALUE,
            PARSER_TEXT,
        ],
    );
}

#[tokio::test]
async fn buffered_codec_direct_and_metadata_results_share_one_execution_path() {
    PROBE_DECODER_CALLS.store(0, Ordering::SeqCst);
    PROBE_VALIDATOR_CALLS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let runtime = RuntimeConfigBuilder::new()
        .request_body_limit(NonZeroUsize::new(5).expect("positive limit"));
    let core = development_core(&executor, Arc::clone(&hooks), runtime);
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .safe_url("http://execution.example.test/v1/codec/round-trip")
                .body("exact"),
            probe_response("first"),
        )
        .expect("direct codec response");
    let value = codec_request::<ProbeEncoder, ProbeDecoder>(
        core.clone(),
        probe_input(b"exact", &encoder_calls),
    )
    .await
    .expect("direct codec output");
    assert_eq!(value, "decoded:first");

    executor
        .application()
        .queue_response(probe_response("second"))
        .expect("metadata codec response");
    let response =
        codec_request::<ProbeEncoder, ProbeDecoder>(core, probe_input(b"again", &encoder_calls))
            .response()
            .await
            .expect("metadata codec output");
    assert_eq!(response.value(), "decoded:second");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), CODEC_ENDPOINT);
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 2);
    assert_eq!(PROBE_VALIDATOR_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(PROBE_DECODER_CALLS.load(Ordering::SeqCst), 2);
    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    for capture in captures {
        assert_eq!(capture.body_category(), BodyCategory::Bytes);
        assert_eq!(capture.known_body_length(), Some(5));
    }
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn encoding_failure_and_request_limit_prevent_native_execution_safely() {
    let failing_executor = DeterministicExecutor::new();
    let failing_hooks = Arc::new(ProbeHooks::new());
    let failing_core = development_core(
        &failing_executor,
        Arc::clone(&failing_hooks),
        RuntimeConfigBuilder::new(),
    );
    let failing_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        failing_core,
        EncoderInput {
            bytes: Bytes::from_static(b"request-encoding-body-secret-98d2"),
            calls: Arc::clone(&failing_calls),
            fail: true,
        },
    )
    .await
    .expect_err("encoder failure");
    assert_eq!(error.category(), ErrorCategory::RequestEncoding);
    assert_eq!(failing_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_executor.application().request_count().unwrap(), 0);
    assert_eq!(failing_hooks.completion_returned.load(Ordering::SeqCst), 1);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("request-encoding-body-secret-98d2"));
        assert!(!formatted.contains("request-codec-secret-ec2d"));
    }

    let limit_executor = DeterministicExecutor::new();
    let limit_hooks = Arc::new(ProbeHooks::new());
    let limit_core = development_core(
        &limit_executor,
        Arc::clone(&limit_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    limit_executor
        .application()
        .queue_response(ScriptedResponse::success("ok"))
        .expect("below-limit response");
    assert_eq!(
        codec_request::<Text, Text>(limit_core.clone(), "abc".to_owned())
            .await
            .expect("below request limit"),
        "ok"
    );
    let limit_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        limit_core,
        probe_input(b"first-excess-request-secret", &limit_calls),
    )
    .await
    .expect_err("request body limit");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(limit_calls.load(Ordering::SeqCst), 1);
    assert_eq!(limit_executor.application().request_count().unwrap(), 1);
    assert_eq!(limit_hooks.completion_returned.load(Ordering::SeqCst), 2);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("first-excess-request-secret"));
    }
}

#[tokio::test]
async fn content_type_and_decoder_failures_are_safe_and_lifecycle_ordered() {
    REJECTING_DECODER_CALLS.store(0, Ordering::SeqCst);
    let contract_executor = DeterministicExecutor::new();
    contract_executor
        .application()
        .queue_response(ScriptedResponse::success("body-must-not-reach-decoder"))
        .expect("contract response");
    let contract_core = development_core(
        &contract_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, RejectingDecoder>(
        contract_core,
        probe_input(b"input", &calls),
    )
    .await
    .expect_err("content-type mismatch");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert_eq!(REJECTING_DECODER_CALLS.load(Ordering::SeqCst), 0);

    FAILING_DECODER_CALLS.store(0, Ordering::SeqCst);
    let decode_executor = DeterministicExecutor::new();
    decode_executor
        .application()
        .queue_response(ScriptedResponse::success(
            "successful-response-body-secret-0b6c",
        ))
        .expect("decode response");
    let decode_core = development_core(
        &decode_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error =
        codec_request::<ProbeEncoder, FailingDecoder>(decode_core, probe_input(b"input", &calls))
            .await
            .expect_err("decoder failure");
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(FAILING_DECODER_CALLS.load(Ordering::SeqCst), 1);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("successful-response-body-secret-0b6c"));
        assert!(!formatted.contains("response-decoder-secret-c413"));
        assert!(!formatted.contains("content-type-secret-4fea"));
    }
}

#[tokio::test]
async fn preparation_failure_skips_before_send_and_transport_but_still_completes() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = Request::new(core, InvalidEndpoint)
        .await
        .expect_err("invalid route");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let records = hooks.records();
    assert!(matches!(
        records.as_slice(),
        [LifecycleRecord::Complete {
            outcome: CompletionOutcome::Configuration,
            status: None,
            ..
        }]
    ));
}

#[tokio::test(start_paused = true)]
async fn timeout_in_before_send_is_fixed_then_completion_runs_outside_deadline() {
    let executor = DeterministicExecutor::new();
    let before_gate = Gate::closed();
    let completion_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks {
        before_gate: Some(before_gate.clone()),
        completion_gate: Some(completion_gate.clone()),
        ..ProbeHooks::new()
    });
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let task =
        tokio::spawn(async move { bytes_request(core).timeout(Duration::from_secs(5)).await });

    before_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    completion_gate.wait_until_reached().await;
    assert!(
        !task.is_finished(),
        "completion hook must delay future delivery"
    );

    let records = hooks.records();
    let before_id = match records.first() {
        Some(LifecycleRecord::Before { operation_id, .. }) => *operation_id,
        _ => panic!("before-send record"),
    };
    let (completion_id, outcome, status, elapsed) = completion(&records);
    assert_eq!(completion_id, before_id);
    assert_eq!(outcome, CompletionOutcome::Timeout);
    assert_eq!(status, None);
    assert_eq!(elapsed, Duration::from_secs(5));

    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(completion(&hooks.records()).3, elapsed);
    assert!(!task.is_finished());
    completion_gate.open();
    let error = task.await.expect("join").expect_err("fixed timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn prepared_builder_deadline_is_fixed_and_completion_remains_outside_it() {
    let executor = DeterministicExecutor::new();
    let before_gate = Gate::closed();
    let completion_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks {
        before_gate: Some(before_gate.clone()),
        completion_gate: Some(completion_gate.clone()),
        ..ProbeHooks::new()
    });
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.deadline"), Method::GET)
            .path_segment("prepared-deadline")
            .timeout(Duration::from_secs(5))
            .response_bytes()
            .build()
            .expect("prepared deadline endpoint");
    let task = tokio::spawn(async move { Request::new(core, endpoint).await });

    before_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    completion_gate.wait_until_reached().await;
    assert!(!task.is_finished());
    let records = hooks.records();
    let before_id = match records.first() {
        Some(LifecycleRecord::Before { operation_id, .. }) => *operation_id,
        _ => panic!("prepared before-send record"),
    };
    let (completion_id, outcome, status, elapsed) = completion(&records);
    assert_eq!(completion_id, before_id);
    assert_eq!(outcome, CompletionOutcome::Timeout);
    assert_eq!(status, None);
    assert_eq!(elapsed, Duration::from_secs(5));

    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(completion(&hooks.records()).3, elapsed);
    assert!(!task.is_finished());
    completion_gate.open();
    let error = task
        .await
        .expect("prepared deadline join")
        .expect_err("prepared deadline timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn success_and_status_results_remain_fixed_while_completion_is_delayed() {
    let success_executor = DeterministicExecutor::new();
    success_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"fixed")))
        .expect("success script");
    let success_gate = Gate::closed();
    let success_hooks = Arc::new(ProbeHooks {
        completion_gate: Some(success_gate.clone()),
        ..ProbeHooks::new()
    });
    let success_core = development_core(
        &success_executor,
        Arc::clone(&success_hooks),
        RuntimeConfigBuilder::new(),
    );
    let success_task = tokio::spawn(async move {
        bytes_request(success_core)
            .timeout(Duration::from_secs(1))
            .await
    });
    success_gate.wait_until_reached().await;
    let success_elapsed = completion(&success_hooks.records()).3;
    tokio::time::advance(Duration::from_secs(10)).await;
    assert!(!success_task.is_finished());
    assert_eq!(completion(&success_hooks.records()).3, success_elapsed);
    success_gate.open();
    assert_eq!(
        success_task.await.expect("join").expect("fixed success"),
        b"fixed"[..]
    );

    let status_executor = DeterministicExecutor::new();
    status_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"status"),
        ))
        .expect("status script");
    let status_gate = Gate::closed();
    let status_hooks = Arc::new(ProbeHooks {
        completion_gate: Some(status_gate.clone()),
        ..ProbeHooks::new()
    });
    let status_core = development_core(
        &status_executor,
        Arc::clone(&status_hooks),
        RuntimeConfigBuilder::new(),
    );
    let status_task = tokio::spawn(async move {
        bytes_request(status_core)
            .timeout(Duration::from_secs(1))
            .await
    });
    status_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(10)).await;
    status_gate.open();
    let error = status_task.await.expect("join").expect_err("fixed status");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        completion(&status_hooks.records()).1,
        CompletionOutcome::HttpStatus
    );
}

#[tokio::test]
async fn cancellation_while_awaiting_completion_may_prevent_delivery() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("response");
    let completion_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks {
        completion_gate: Some(completion_gate.clone()),
        ..ProbeHooks::new()
    });
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let task = tokio::spawn(async move { bytes_request(core).await });
    completion_gate.wait_until_reached().await;
    task.abort();
    assert!(task.await.expect_err("cancelled join").is_cancelled());
    completion_gate.open();
    tokio::task::yield_now().await;
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.records().len(), 3);
}

#[cfg(feature = "dangerous-raw-response")]
#[tokio::test(start_paused = true)]
async fn raw_response_keeps_the_fixed_deadline_and_cancellation_contract() {
    let timeout_executor = DeterministicExecutor::new();
    let before_gate = Gate::closed();
    let completion_gate = Gate::closed();
    let timeout_hooks = Arc::new(ProbeHooks {
        before_gate: Some(before_gate.clone()),
        completion_gate: Some(completion_gate.clone()),
        ..ProbeHooks::new()
    });
    let timeout_core = development_core(
        &timeout_executor,
        Arc::clone(&timeout_hooks),
        RuntimeConfigBuilder::new(),
    );
    let timeout_task = tokio::spawn(async move {
        bytes_request(timeout_core)
            .timeout(Duration::from_secs(5))
            .raw_response()
            .await
    });
    before_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    completion_gate.wait_until_reached().await;
    let fixed_elapsed = completion(&timeout_hooks.records()).3;
    assert_eq!(fixed_elapsed, Duration::from_secs(5));
    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(completion(&timeout_hooks.records()).3, fixed_elapsed);
    assert!(!timeout_task.is_finished());
    completion_gate.open();
    let error = timeout_task
        .await
        .expect("raw timeout join")
        .expect_err("raw fixed timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(
        timeout_executor
            .application()
            .request_count()
            .expect("count"),
        0
    );

    let cancellation_executor = DeterministicExecutor::new();
    cancellation_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"fixed raw")))
        .expect("raw cancellation response");
    let cancellation_gate = Gate::closed();
    let cancellation_hooks = Arc::new(ProbeHooks {
        completion_gate: Some(cancellation_gate.clone()),
        ..ProbeHooks::new()
    });
    let cancellation_core = development_core(
        &cancellation_executor,
        Arc::clone(&cancellation_hooks),
        RuntimeConfigBuilder::new(),
    );
    let cancellation_task =
        tokio::spawn(async move { bytes_request(cancellation_core).raw_response().await });
    cancellation_gate.wait_until_reached().await;
    cancellation_task.abort();
    assert!(
        cancellation_task
            .await
            .expect_err("raw request task must be cancelled")
            .is_cancelled()
    );
    cancellation_gate.open();
    tokio::task::yield_now().await;
    assert_eq!(
        cancellation_hooks
            .completion_returned
            .load(Ordering::SeqCst),
        0
    );
}

#[tokio::test(start_paused = true)]
async fn one_deadline_bounds_native_execution_and_response_head_hook() {
    let native_executor = DeterministicExecutor::new();
    let native_gate = Gate::closed();
    native_executor
        .application()
        .pause_before_execution(native_gate.clone())
        .expect("native gate");
    native_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("native response");
    let native_hooks = Arc::new(ProbeHooks::new());
    let native_core = development_core(
        &native_executor,
        Arc::clone(&native_hooks),
        RuntimeConfigBuilder::new(),
    );
    let native_task = tokio::spawn(async move {
        bytes_request(native_core)
            .timeout(Duration::from_secs(2))
            .await
    });
    native_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    let error = native_task
        .await
        .expect("join")
        .expect_err("native timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(completion(&native_hooks.records()).2, None);

    let response_executor = DeterministicExecutor::new();
    response_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("response");
    let response_gate = Gate::closed();
    let response_hooks = Arc::new(ProbeHooks {
        response_gate: Some(response_gate.clone()),
        ..ProbeHooks::new()
    });
    let response_core = development_core(
        &response_executor,
        Arc::clone(&response_hooks),
        RuntimeConfigBuilder::new(),
    );
    let response_task = tokio::spawn(async move {
        bytes_request(response_core)
            .timeout(Duration::from_secs(2))
            .await
    });
    response_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    let error = response_task
        .await
        .expect("join")
        .expect_err("response hook timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    let records = response_hooks.records();
    let before_id = match records[0] {
        LifecycleRecord::Before { operation_id, .. } => operation_id,
        _ => panic!("before-send record"),
    };
    let response_id = match records[1] {
        LifecycleRecord::Response(id, StatusCode::OK) => id,
        _ => panic!("response-head record"),
    };
    assert_eq!(response_id, before_id);
    assert_eq!(completion(&records).0, before_id);
    assert_eq!(completion(&records).2, Some(StatusCode::OK));
}

#[tokio::test(start_paused = true)]
async fn delayed_polling_consumes_no_budget_and_disable_timeout_is_distinct() {
    let delayed_executor = DeterministicExecutor::new();
    delayed_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"late poll")))
        .expect("response");
    let delayed_core = development_core(
        &delayed_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let request = Request::new(
        delayed_core,
        BytesEndpoint::new(Some(Duration::from_secs(1))),
    );
    tokio::time::advance(Duration::from_secs(100)).await;
    assert_eq!(
        request.await.expect("deadline begins on poll"),
        b"late poll"[..]
    );

    let disabled_executor = DeterministicExecutor::new();
    let disabled_gate = Gate::closed();
    disabled_executor
        .application()
        .pause_before_execution(disabled_gate.clone())
        .expect("disabled gate");
    disabled_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"disabled")))
        .expect("response");
    let disabled_core = development_core(
        &disabled_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let task = tokio::spawn(async move { bytes_request(disabled_core).disable_timeout().await });
    disabled_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(100)).await;
    assert!(!task.is_finished());
    disabled_gate.open();
    assert_eq!(
        task.await.expect("join").expect("disabled timeout"),
        b"disabled"[..]
    );
}

#[tokio::test]
async fn transport_failures_are_safely_classified_and_complete_once() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_failure(TransportFailure::Connect)
        .expect("failure");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core).await.expect_err("connect failure");
    assert_eq!(error.category(), ErrorCategory::Connect);
    let records = hooks.records();
    assert_eq!(records.len(), 2);
    assert_eq!(completion(&records).1, CompletionOutcome::Connect);
}

#[tokio::test]
async fn real_reqwest_transmits_and_decodes_text_and_custom_buffered_codecs() {
    let server = ScriptedServer::start([
        ResponseScript::ok("text-response-♥".as_bytes().to_vec()),
        ResponseScript::ok(b"custom-response".to_vec())
            .with_header("Content-Type", "application/x-concord-probe"),
    ])
    .expect("codec server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let text = codec_request::<Text, Text>(core.clone(), "text-request-♥".to_owned())
        .response()
        .await
        .expect("text response");
    assert_eq!(text.value(), "text-response-♥");

    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let custom = codec_request::<ProbeEncoder, RealCustomDecoder>(
        core,
        probe_input(b"custom-request", &encoder_calls),
    )
    .await
    .expect("custom response");
    assert_eq!(custom, "custom-response");
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 1);
    server
        .wait_for_requests(2, Duration::from_secs(1))
        .expect("request recording");
    let requests = server.requests();
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(requests[0].body(), "text-request-♥".as_bytes());
    assert_eq!(
        requests[0].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(requests[0].header("accept"), Some("text/plain"));
    assert_eq!(requests[1].body(), b"custom-request");
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/x-concord-probe")
    );
    assert_eq!(
        requests[1].header("accept"),
        Some("application/x-concord-probe")
    );
}

#[tokio::test]
async fn buffered_decoder_never_observes_known_or_first_excess_response_bytes() {
    OVERFLOW_DECODER_CALLS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        ResponseScript::ok(b"four".to_vec()),
        ResponseScript::ok(b"known".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"four".to_vec(), b"first-excess-secret".to_vec()],
        },
    ])
    .expect("response limit server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .response_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    assert_eq!(
        codec_request::<Text, Text>(core.clone(), "four".to_owned())
            .await
            .expect("exact response limit"),
        "four"
    );
    for expected_path in ["known response preflight", "unknown response overflow"] {
        let error = codec_request::<Text, OverflowDecoder>(core.clone(), "input".to_owned())
            .await
            .expect_err(expected_path);
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains("known"));
            assert!(!formatted.contains("first-excess-secret"));
        }
    }
    assert_eq!(OVERFLOW_DECODER_CALLS.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn real_content_type_mismatch_rejects_before_response_body_polling() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"content-contract-body-secret-5dd4".to_vec(),
        gate: gate.clone(),
    }])
    .expect("content contract server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        codec_request::<Text, RejectingDecoder>(core, "input".to_owned()),
    )
    .await
    .expect("content validation must not wait for the body")
    .expect_err("missing content type");
    gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("content-contract-body-secret-5dd4"));
    }
}

#[tokio::test]
async fn real_reqwest_stream_upload_transmits_exact_bytes_and_owned_media_type() {
    let server = ScriptedServer::start([ResponseScript::ok(b"upload-ok".to_vec())])
        .expect("stream upload server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let polls = Arc::new(AtomicUsize::new(0));
    let stream = StreamBody::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"real-"),
            ProbeFrame::Data(b"stream-"),
            ProbeFrame::Data(b"upload"),
        ],
        &polls,
    ))
    .exact_length(18);
    let body = RequestBody::stream(stream)
        .content_type("application/x-concord-stream")
        .expect("valid stream content type");
    assert_eq!(
        body_request(core, body).await.expect("stream upload"),
        b"upload-ok"[..]
    );
    assert_eq!(polls.load(Ordering::SeqCst), 3);
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("recorded stream request");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(requests[0].target(), "/v1/body/upload");
    assert_eq!(requests[0].body(), b"real-stream-upload");
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-concord-stream")
    );
    assert_eq!(requests[0].header("content-length"), Some("18"));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn real_reqwest_multipart_upload_owns_framing_and_supports_streams_and_factories() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"buffered-ok".to_vec()),
        ResponseScript::ok(b"streamed-ok".to_vec()),
        ResponseScript::ok(b"factory-ok".to_vec()),
    ])
    .expect("multipart upload server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let buffered = MultipartBody::new()
        .part(
            "text-field",
            MultipartPart::text("multipart-text-value-secret-017c"),
        )
        .expect("text part")
        .part(
            "file-field",
            MultipartPart::bytes(Bytes::from_static(b"multipart-bytes-value-secret-929d"))
                .filename("upload-secret-name.txt")
                .expect("filename")
                .content_type("application/x-concord-part")
                .expect("part content type"),
        )
        .expect("bytes part");
    assert_eq!(
        body_request(core.clone(), RequestBody::multipart(buffered))
            .await
            .expect("buffered multipart upload"),
        b"buffered-ok"[..]
    );

    let polls = Arc::new(AtomicUsize::new(0));
    let streamed = StreamBody::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"multipart-streamed-"),
            ProbeFrame::Data(b"value-secret-62e7"),
        ],
        &polls,
    ));
    let streamed = MultipartBody::new()
        .part(
            "stream-field",
            MultipartPart::stream(streamed)
                .filename("stream-secret-name.bin")
                .expect("stream filename")
                .content_type("application/octet-stream")
                .expect("stream content type"),
        )
        .expect("stream part");
    assert_eq!(
        body_request(core.clone(), RequestBody::multipart(streamed))
            .await
            .expect("streamed multipart upload"),
        b"streamed-ok"[..]
    );

    let factory_calls = Arc::new(AtomicUsize::new(0));
    assert_eq!(
        body_request(
            core,
            RequestBody::multipart_factory(ProbeMultipartFactory {
                create_calls: Arc::clone(&factory_calls),
                fail: false,
            }),
        )
        .await
        .expect("factory multipart upload"),
        b"factory-ok"[..]
    );

    server
        .wait_for_requests(3, Duration::from_secs(2))
        .expect("recorded multipart requests");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        let content_type = request
            .header("content-type")
            .expect("Reqwest multipart content type");
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("boundary parameter");
        assert!(!boundary.is_empty());
        assert!(
            request
                .body()
                .starts_with(format!("--{boundary}\r\n").as_bytes())
        );
    }
    let buffered_wire = String::from_utf8_lossy(requests[0].body());
    assert!(buffered_wire.contains("name=\"text-field\""));
    assert!(buffered_wire.contains("multipart-text-value-secret-017c"));
    assert!(buffered_wire.contains("multipart-bytes-value-secret-929d"));
    assert!(buffered_wire.contains("upload-secret-name.txt"));
    assert!(buffered_wire.contains("application/x-concord-part"));
    assert!(requests[0].header("content-length").is_some());

    let streamed_wire = String::from_utf8_lossy(requests[1].body());
    assert!(streamed_wire.contains("multipart-streamed-value-secret-62e7"));
    assert!(streamed_wire.contains("stream-secret-name.bin"));
    assert_eq!(requests[1].header("transfer-encoding"), Some("chunked"));
    assert_eq!(polls.load(Ordering::SeqCst), 3);

    let factory_wire = String::from_utf8_lossy(requests[2].body());
    assert!(factory_wire.contains("factory-multipart-value"));
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn managed_producer_failure_precedes_reqwest_transport_failure() {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("managed producer listener");
    let address = listener.local_addr().expect("managed producer address");
    let peer = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.expect("managed producer accept");
        std::future::pending::<()>().await;
    });
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    let hooks = Arc::new(ProbeHooks::new());
    let core = ClientCore::build(
        &format!("http://{address}/v1/"),
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().hooks(hooks.clone()),
    )
    .expect("managed producer core");
    let polls = Arc::new(AtomicUsize::new(0));
    let body = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Failure("native-producer-secret-88be")],
        &polls,
    ))
    .exact_length(1);
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        body_request(core, RequestBody::stream(body)),
    )
    .await
    .expect("managed producer failure should be prompt")
    .expect_err("managed producer request must fail");
    peer.abort();
    let _ = peer.await;
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("native-producer-secret-88be"));
}

#[tokio::test]
async fn cancellation_drops_direct_and_factory_created_request_streams_without_completion() {
    async fn cancel(body: RequestBody, reached: Arc<tokio::sync::Notify>) -> Arc<ProbeHooks> {
        let executor = DeterministicExecutor::new();
        let hooks = Arc::new(ProbeHooks::new());
        let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
        let task = tokio::spawn(async move { body_request(core, body).await });
        reached.notified().await;
        assert_eq!(executor.application().request_count().unwrap(), 0);
        task.abort();
        let join = task.await.expect_err("request task must be cancelled");
        assert!(join.is_cancelled());
        tokio::task::yield_now().await;
        assert_eq!(executor.application().request_count().unwrap(), 0);
        hooks
    }

    let direct_polls = Arc::new(AtomicUsize::new(0));
    let direct_dropped = Arc::new(AtomicBool::new(false));
    let direct_reached = Arc::new(tokio::sync::Notify::new());
    let direct_body = RequestBody::stream(StreamBody::from_stream(PendingBodyStream {
        polls: Arc::clone(&direct_polls),
        dropped: Arc::clone(&direct_dropped),
        reached: Arc::clone(&direct_reached),
    }));
    let direct_hooks = cancel(direct_body, Arc::clone(&direct_reached)).await;
    let direct_polls_at_drop = direct_polls.load(Ordering::SeqCst);
    assert!(direct_polls_at_drop > 0);
    assert!(direct_dropped.load(Ordering::SeqCst));
    assert_eq!(direct_hooks.completion_returned.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(direct_polls.load(Ordering::SeqCst), direct_polls_at_drop);

    let factory_calls = Arc::new(AtomicUsize::new(0));
    let factory_polls = Arc::new(AtomicUsize::new(0));
    let factory_dropped = Arc::new(AtomicBool::new(false));
    let factory_reached = Arc::new(tokio::sync::Notify::new());
    let factory_body = RequestBody::factory(PendingBodyFactory {
        create_calls: Arc::clone(&factory_calls),
        polls: Arc::clone(&factory_polls),
        dropped: Arc::clone(&factory_dropped),
        reached: Arc::clone(&factory_reached),
    });
    let factory_hooks = cancel(factory_body, Arc::clone(&factory_reached)).await;
    let factory_polls_at_drop = factory_polls.load(Ordering::SeqCst);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert!(factory_polls_at_drop > 0);
    assert!(factory_dropped.load(Ordering::SeqCst));
    assert_eq!(factory_hooks.completion_returned.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(factory_polls.load(Ordering::SeqCst), factory_polls_at_drop);

    let prepared_polls = Arc::new(AtomicUsize::new(0));
    let prepared_dropped = Arc::new(AtomicBool::new(false));
    let prepared_reached = Arc::new(tokio::sync::Notify::new());
    let prepared_executor = DeterministicExecutor::new();
    let prepared_hooks = Arc::new(ProbeHooks::new());
    let prepared_core = development_core(
        &prepared_executor,
        Arc::clone(&prepared_hooks),
        RuntimeConfigBuilder::new(),
    );
    let prepared_endpoint = RuntimeEndpoint::builder(
        EndpointId::from_static("runtime.cancellation"),
        Method::POST,
    )
    .path_segment("prepared-cancellation")
    .body(RequestBody::stream(StreamBody::from_stream(
        PendingBodyStream {
            polls: Arc::clone(&prepared_polls),
            dropped: Arc::clone(&prepared_dropped),
            reached: Arc::clone(&prepared_reached),
        },
    )))
    .response_bytes()
    .build()
    .expect("prepared cancellation endpoint");
    let prepared_task =
        tokio::spawn(async move { Request::new(prepared_core, prepared_endpoint).await });
    prepared_reached.notified().await;
    assert_eq!(prepared_executor.application().request_count().unwrap(), 0);
    prepared_task.abort();
    assert!(
        prepared_task
            .await
            .expect_err("prepared request must be cancelled")
            .is_cancelled()
    );
    tokio::task::yield_now().await;
    let prepared_polls_at_drop = prepared_polls.load(Ordering::SeqCst);
    assert!(prepared_polls_at_drop > 0);
    assert!(prepared_dropped.load(Ordering::SeqCst));
    assert_eq!(prepared_hooks.completion_returned.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(
        prepared_polls.load(Ordering::SeqCst),
        prepared_polls_at_drop
    );

    #[cfg(feature = "multipart")]
    {
        let multipart_polls = Arc::new(AtomicUsize::new(0));
        let multipart_dropped = Arc::new(AtomicBool::new(false));
        let multipart_reached = Arc::new(tokio::sync::Notify::new());
        let stream = StreamBody::from_stream(PendingBodyStream {
            polls: Arc::clone(&multipart_polls),
            dropped: Arc::clone(&multipart_dropped),
            reached: Arc::clone(&multipart_reached),
        });
        let multipart_body = RequestBody::multipart(
            MultipartBody::new()
                .part("stream", MultipartPart::stream(stream))
                .expect("multipart pending stream"),
        );
        let multipart_hooks = cancel(multipart_body, Arc::clone(&multipart_reached)).await;
        let multipart_polls_at_drop = multipart_polls.load(Ordering::SeqCst);
        assert!(multipart_polls_at_drop > 0);
        assert!(multipart_dropped.load(Ordering::SeqCst));
        assert_eq!(
            multipart_hooks.completion_returned.load(Ordering::SeqCst),
            0
        );
        tokio::task::yield_now().await;
        assert_eq!(
            multipart_polls.load(Ordering::SeqCst),
            multipart_polls_at_drop
        );
    }
}

#[tokio::test]
async fn real_reqwest_bytes_are_finite_complete_only_and_body_failures_are_safe() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"within".to_vec()),
        ResponseScript::ok(b"known-too-large".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"ab".to_vec(), b"cd".to_vec()],
        },
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"abcd".to_vec(), b"excess".to_vec()],
        },
        ResponseScript::Truncated {
            status: 200,
            declared_length: 6,
            prefix: b"short".to_vec(),
        },
    ])
    .expect("bytes server");
    let runtime = || RuntimeConfigBuilder::new().response_body_limit(NonZeroUsize::new(6).unwrap());
    let core = real_core(&server, Arc::new(ProbeHooks::new()), runtime());
    assert_eq!(
        bytes_request(core.clone()).await.expect("within"),
        b"within"[..]
    );
    assert_eq!(
        bytes_request(core.clone())
            .await
            .expect_err("known excessive length")
            .category(),
        ErrorCategory::ResponseBody
    );
    assert_eq!(
        bytes_request(core.clone()).await.expect("unknown within"),
        b"abcd"[..]
    );
    let overflow = bytes_request(core.clone())
        .await
        .expect_err("unknown overflow");
    assert_eq!(overflow.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{overflow:?} {overflow}").contains("abcd"));
    assert!(!format!("{overflow:?} {overflow}").contains("excess"));
    assert_eq!(
        bytes_request(core)
            .await
            .expect_err("native body failure")
            .category(),
        ErrorCategory::ResponseBody
    );
    assert_eq!(server.request_count(), 5);
}

#[tokio::test]
async fn real_reqwest_bytes_collection_is_bounded_by_operation_deadline() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"delayed body".to_vec(),
        gate: gate.clone(),
    }])
    .expect("gated bytes server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core)
        .timeout(Duration::from_millis(500))
        .await
        .expect_err("body deadline");
    gate.release();
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(completion(&hooks.records()).2, Some(StatusCode::OK));
}

#[tokio::test]
async fn no_content_accepts_every_2xx_without_polling_the_body() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedBody {
            status: 200,
            body: b"must-not-be-polled".to_vec(),
            gate: gate.clone(),
        },
        ResponseScript::status(204, Vec::new()),
        ResponseScript::status(205, b"ignored".to_vec()),
    ])
    .expect("no-content server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    tokio::time::timeout(Duration::from_secs(1), no_content_request(core.clone()))
        .await
        .expect("200 must not wait for body gate")
        .expect("200 no-content");
    gate.release();
    no_content_request(core.clone())
        .await
        .expect("204 no-content");
    let response = no_content_request(core)
        .response()
        .await
        .expect("205 metadata");
    assert_eq!(response.status(), StatusCode::RESET_CONTENT);
    assert_eq!(response.value(), &());
}

#[tokio::test]
async fn all_non_2xx_statuses_use_complete_only_bounded_terminal_capture() {
    let deadline_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::status(302, b"redirect".to_vec()),
        ResponseScript::status(404, b"missing".to_vec()),
        ResponseScript::status(500, b"known-too-large".to_vec()),
        ResponseScript::Chunked {
            status: 502,
            headers: Vec::new(),
            chunks: vec![b"prefix".to_vec(), b"overflow".to_vec()],
        },
        ResponseScript::Truncated {
            status: 503,
            declared_length: 7,
            prefix: b"short".to_vec(),
        },
        ResponseScript::GatedBody {
            status: 504,
            body: b"late".to_vec(),
            gate: deadline_gate.clone(),
        },
    ])
    .expect("status server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(
        &server,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().status_body_limit(NonZeroUsize::new(8).unwrap()),
    );

    let redirect = bytes_request(core.clone()).await.expect_err("3xx terminal");
    assert_eq!(redirect.http_status(), Some(StatusCode::FOUND));
    assert_eq!(
        redirect.http_response().unwrap().body().state(),
        StatusBodyState::Complete
    );
    let missing = bytes_request(core.clone()).await.expect_err("4xx terminal");
    assert_eq!(missing.http_status(), Some(StatusCode::NOT_FOUND));
    assert_eq!(
        missing
            .http_response()
            .unwrap()
            .body()
            .bytes()
            .unwrap()
            .as_ref(),
        b"missing"
    );
    let known = bytes_request(core.clone()).await.expect_err("5xx terminal");
    assert_eq!(known.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(
        known.http_response().unwrap().body().state(),
        StatusBodyState::OmittedKnownTooLarge
    );
    let overflow = bytes_request(core.clone())
        .await
        .expect_err("unknown overflow");
    assert_eq!(overflow.http_status(), Some(StatusCode::BAD_GATEWAY));
    assert_eq!(
        overflow.http_response().unwrap().body().state(),
        StatusBodyState::OmittedLimitExceeded
    );
    assert!(overflow.http_response().unwrap().body().bytes().is_none());
    let read = bytes_request(core.clone())
        .await
        .expect_err("status read failure");
    assert_eq!(read.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        read.http_response().unwrap().body().state(),
        StatusBodyState::UnavailableReadFailure
    );
    let deadline = bytes_request(core)
        .timeout(Duration::from_millis(500))
        .await
        .expect_err("status capture deadline");
    deadline_gate.release();
    assert_eq!(deadline.category(), ErrorCategory::HttpStatus);
    assert_eq!(deadline.http_status(), Some(StatusCode::GATEWAY_TIMEOUT));
    assert_eq!(
        deadline.http_response().unwrap().body().state(),
        StatusBodyState::UnavailableDeadline
    );
    let records = hooks.records();
    let completions = records
        .iter()
        .filter_map(|record| match record {
            LifecycleRecord::Complete {
                outcome, status, ..
            } => Some((*outcome, *status)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(completions.len(), 6);
    assert!(
        completions
            .iter()
            .all(|(outcome, _)| *outcome == CompletionOutcome::HttpStatus)
    );
    assert_eq!(
        completions
            .iter()
            .map(|(_, status)| *status)
            .collect::<Vec<_>>(),
        [
            Some(StatusCode::FOUND),
            Some(StatusCode::NOT_FOUND),
            Some(StatusCode::INTERNAL_SERVER_ERROR),
            Some(StatusCode::BAD_GATEWAY),
            Some(StatusCode::SERVICE_UNAVAILABLE),
            Some(StatusCode::GATEWAY_TIMEOUT),
        ]
    );
}

#[test]
fn public_associated_futures_are_named_sendable_box_futures() {
    fn assert_box_future<T>(_: BoxFuture<'static, T>) {}

    let executor = DeterministicExecutor::new();
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    assert_box_future(bytes_request(core.clone()).into_future());
    assert_box_future(bytes_request(core).response().into_future());
}

impl fmt::Debug for ProbeHooks {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProbeHooks")
            .field(
                "record_count",
                &self.records.lock().map(|records| records.len()).ok(),
            )
            .field("veto", &self.veto.load(Ordering::SeqCst))
            .field(
                "completion_returned",
                &self.completion_returned.load(Ordering::SeqCst),
            )
            .finish()
    }
}
