#[cfg(test)]
use crate::body::DynBody;
use crate::body::{BodyError, BodyErrorKind};
use crate::codec::ContentType;
use crate::error::{ApiClientError, ErrorContext};
use crate::transport::ExecutionResponse;
#[cfg(test)]
use crate::transport::NativeResponseErrorMapper;
use bytes::Bytes;
use http::{HeaderMap, StatusCode, Version, header::CONTENT_LENGTH};
#[cfg(test)]
use http_body::{Body, Frame, SizeHint};
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
#[cfg(test)]
use std::pin::Pin;
#[cfg(test)]
use std::task::{Context, Poll};
use tokio::io::AsyncWriteExt;

/// A streaming response façade over the runtime's native response body.
///
/// [`StreamResponse::next_chunk`] and [`StreamResponse::write_to_file`] are
/// data-only conveniences over the native Reqwest response stream.
pub struct StreamResponse<M> {
    resp: ExecutionResponse,
    terminal: bool,
    _media: PhantomData<fn() -> M>,
}

impl<M> StreamResponse<M> {
    pub(crate) fn new(resp: ExecutionResponse) -> Self {
        Self {
            resp,
            terminal: false,
            _media: PhantomData,
        }
    }

    pub fn meta(&self) -> &crate::execution_meta::RequestExecutionMeta {
        &self.resp.context.meta
    }

    pub fn url(&self) -> &url::Url {
        if self.resp.error_mapper.uses_test_origin_override() {
            &self.resp.context.request_url
        } else {
            self.resp.message.url()
        }
    }

    pub fn status(&self) -> StatusCode {
        self.resp.message.status()
    }

    pub fn version(&self) -> Version {
        self.resp.message.version()
    }

    pub fn headers(&self) -> &HeaderMap {
        self.resp.message.headers()
    }

    pub fn extensions(&self) -> &http::Extensions {
        self.resp.message.extensions()
    }

    pub fn content_length(&self) -> Option<u64> {
        self.headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
    }

    pub fn rate_limit(&self) -> &crate::rate_limit::RateLimitPlan {
        &self.resp.context.rate_limit
    }

    #[cfg(test)]
    pub(crate) fn into_body(self) -> DynBody {
        let terminal = self.terminal;
        let ExecutionResponse {
            message,
            context: _,
            error_mapper,
            body_limit,
            body_seen,
        } = self.resp;
        let inner: reqwest::Body = message.into();
        DynBody::from_body(NativeFrameBody {
            inner: Box::pin(inner),
            error_mapper,
            limit: body_limit,
            seen: body_seen,
            terminal,
        })
    }
}

impl<M: ContentType> StreamResponse<M> {
    pub fn media_type(&self) -> &'static str {
        M::CONTENT_TYPE
    }

    /// Returns the next data chunk, skipping non-data frames such as trailers.
    /// EOF, a native body error, or a limit error permanently terminates this
    /// stream; subsequent calls return `Ok(None)` without polling again.
    pub async fn next_chunk(&mut self) -> Result<Option<Bytes>, ApiClientError> {
        if self.terminal {
            return Ok(None);
        }
        let ctx = self.error_context();
        let chunk = match self.resp.message.chunk().await {
            Ok(chunk) => chunk,
            Err(error) => {
                self.terminal = true;
                let source = self.resp.map_body_error(error);
                return Err(Self::sanitize_body_error(ctx, source));
            }
        };
        let Some(chunk) = chunk else {
            self.terminal = true;
            return Ok(None);
        };
        let actual = self.resp.body_seen.saturating_add(chunk.len() as u64);
        if let Some(limit) = self.resp.body_limit
            && actual > limit
        {
            self.terminal = true;
            return Err(Self::sanitize_body_error(
                ctx,
                BodyError::limit_exceeded(limit, actual),
            ));
        }
        self.resp.body_seen = actual;
        Ok(Some(chunk))
    }

    /// Writes data chunks to a file; trailer frames are not written.
    pub async fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), ApiClientError> {
        let ctx = self.error_context();
        let mut file = tokio::fs::File::create(path.as_ref())
            .await
            .map_err(|source| {
                Self::io_error(
                    ctx.clone(),
                    "failed to create stream response output file",
                    source,
                )
            })?;
        while let Some(chunk) = self.next_chunk().await? {
            file.write_all(&chunk).await.map_err(|source| {
                Self::io_error(
                    ctx.clone(),
                    "failed to write stream response output file",
                    source,
                )
            })?;
        }
        file.flush().await.map_err(|source| {
            Self::io_error(ctx, "failed to flush stream response output file", source)
        })?;
        Ok(())
    }
}

#[cfg(test)]
struct NativeFrameBody {
    inner: Pin<Box<reqwest::Body>>,
    error_mapper: NativeResponseErrorMapper,
    limit: Option<u64>,
    seen: u64,
    terminal: bool,
}

#[cfg(test)]
impl Body for NativeFrameBody {
    type Data = Bytes;
    type Error = BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_mut().get_mut();
        if this.terminal {
            return Poll::Ready(None);
        }
        match this.inner.as_mut().poll_frame(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                this.terminal = true;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                this.terminal = true;
                Poll::Ready(Some(Err(this.error_mapper.map_body_error(error))))
            }
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let actual = this.seen.saturating_add(data.len() as u64);
                    if let Some(limit) = this.limit
                        && actual > limit
                    {
                        this.terminal = true;
                        return Poll::Ready(Some(Err(BodyError::limit_exceeded(limit, actual))));
                    }
                    this.seen = actual;
                }
                Poll::Ready(Some(Ok(frame)))
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.terminal || self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        if self.terminal {
            return SizeHint::with_exact(0);
        }
        let inner = self.inner.size_hint();
        let Some(limit) = self.limit else {
            return inner;
        };
        let remaining = limit.saturating_sub(self.seen);
        let mut hint = SizeHint::new();
        if inner.lower() <= remaining {
            hint.set_lower(inner.lower());
        }
        hint.set_upper(inner.upper().unwrap_or(remaining).min(remaining));
        hint
    }
}

impl<M> StreamResponse<M> {
    fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.resp.context.meta.endpoint,
            method: self.resp.context.meta.method.clone(),
        }
    }

    fn sanitize_body_error(ctx: ErrorContext, source: BodyError) -> ApiClientError {
        if source.kind() == BodyErrorKind::LimitExceeded {
            return ApiClientError::ResponseBodyLimitExceeded {
                ctx,
                limit: source.limit().unwrap_or_default() as usize,
            };
        }
        ApiClientError::response_body_error(ctx, source)
    }

    fn io_error(ctx: ErrorContext, msg: &'static str, source: std::io::Error) -> ApiClientError {
        ApiClientError::response_file_error(ctx, msg, source)
    }
}

impl<M: ContentType> fmt::Debug for StreamResponse<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamResponse")
            .field("meta", self.meta())
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(self.url(), [] as [&str; 0]),
            )
            .field("status", &self.status())
            .field("version", &self.version())
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(self.headers()),
            )
            .field("content_length", &self.content_length())
            .field("rate_limit", self.rate_limit())
            .field("body", &"<stream>")
            .field("media_type", &M::CONTENT_TYPE)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_meta::RequestExecutionMeta;
    use crate::media::OctetStream;
    use crate::rate_limit::RateLimitPlan;
    use crate::transport::{ManagedReqwestClient, ResponseContext, SafeProxy};
    use http::HeaderValue;
    use http_body_util::BodyExt;
    use std::collections::VecDeque;
    use std::error::Error;

    struct ScriptedNativeBody {
        frames: VecDeque<Result<Frame<Bytes>, NativeTestError>>,
        exact_hint: bool,
    }

    impl ScriptedNativeBody {
        fn exact(frames: Vec<Result<Frame<Bytes>, NativeTestError>>) -> Self {
            Self {
                frames: frames.into(),
                exact_hint: true,
            }
        }

        fn unknown(frames: Vec<Result<Frame<Bytes>, NativeTestError>>) -> Self {
            Self {
                frames: frames.into(),
                exact_hint: false,
            }
        }
    }

    impl Body for ScriptedNativeBody {
        type Data = Bytes;
        type Error = NativeTestError;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(self.frames.pop_front())
        }

        fn is_end_stream(&self) -> bool {
            self.frames.is_empty()
        }

        fn size_hint(&self) -> SizeHint {
            if !self.exact_hint {
                return SizeHint::new();
            }
            let remaining = self
                .frames
                .iter()
                .filter_map(|frame| frame.as_ref().ok()?.data_ref())
                .fold(0_u64, |total, data| total.saturating_add(data.len() as u64));
            SizeHint::with_exact(remaining)
        }
    }

    #[derive(Debug)]
    struct NativeTestError(&'static str);

    impl fmt::Display for NativeTestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }

    impl Error for NativeTestError {}

    fn mapper() -> NativeResponseErrorMapper {
        ManagedReqwestClient::new().response_error_mapper()
    }

    fn proxy_mapper() -> NativeResponseErrorMapper {
        let proxy = SafeProxy::all("http://proxy-sentinel.invalid").expect("safe proxy");
        ManagedReqwestClient::with_builder(|builder| builder.proxy(proxy))
            .expect("managed client")
            .response_error_mapper()
    }

    fn response(
        body: ScriptedNativeBody,
        limit: Option<u64>,
        error_mapper: NativeResponseErrorMapper,
    ) -> StreamResponse<OctetStream> {
        let native_body = reqwest::Body::wrap(body);
        let message = reqwest::Response::from(
            http::Response::builder()
                .status(StatusCode::OK)
                .body(native_body)
                .expect("native response"),
        );
        StreamResponse::new(ExecutionResponse {
            message,
            context: ResponseContext {
                meta: RequestExecutionMeta {
                    endpoint: "FrameAwareStream",
                    method: http::Method::GET,
                    idempotent: true,
                    page_index: 0,
                },
                request_url: url::Url::parse("http://example.invalid/stream").expect("request URL"),
                rate_limit: RateLimitPlan::new(),
            },
            error_mapper,
            body_limit: limit,
            body_seen: 0,
        })
    }

    fn data(value: &'static [u8]) -> Result<Frame<Bytes>, NativeTestError> {
        Ok(Frame::data(Bytes::from_static(value)))
    }

    fn trailers(name: &'static str) -> Result<Frame<Bytes>, NativeTestError> {
        let mut trailers = HeaderMap::new();
        trailers.insert(name, HeaderValue::from_static("present"));
        Ok(Frame::trailers(trailers))
    }

    #[test]
    fn streaming_body_failures_keep_the_safe_body_kind() {
        let error = StreamResponse::<()>::sanitize_body_error(
            ErrorContext {
                endpoint: "StreamBodyFailure",
                method: http::Method::GET,
            },
            BodyError::input(),
        );
        assert!(matches!(
            error,
            ApiClientError::ResponseBody {
                kind: BodyErrorKind::Input,
                ..
            }
        ));
        assert!(!error.to_string().contains("producer"));
    }

    #[tokio::test]
    async fn into_body_preserves_native_data_trailers_order_hint_and_terminal_state() {
        let expected_data = Bytes::from_static(b"frame-data");
        let mut body = response(
            ScriptedNativeBody::exact(vec![
                Ok(Frame::data(expected_data.clone())),
                trailers("x-native-trailer"),
            ]),
            None,
            mapper(),
        )
        .into_body();

        assert_eq!(body.size_hint().exact(), Some(expected_data.len() as u64));
        let frame = body.frame().await.expect("data frame").expect("data");
        assert_eq!(frame.into_data().expect("data frame"), expected_data);
        let frame = body
            .frame()
            .await
            .expect("trailer frame")
            .expect("trailers");
        let trailers = frame.into_trailers().expect("trailer frame");
        assert_eq!(trailers["x-native-trailer"], "present");
        assert!(body.frame().await.is_none());
        assert!(body.frame().await.is_none());
        assert!(body.is_end_stream());
        assert_eq!(body.size_hint().exact(), Some(0));
    }

    #[tokio::test]
    async fn into_body_preserves_unknown_size_hint_and_bounds_limited_size_hint() {
        let unknown = response(
            ScriptedNativeBody::unknown(vec![data(b"unknown")]),
            None,
            mapper(),
        )
        .into_body();
        assert_eq!(unknown.size_hint().lower(), 0);
        assert_eq!(unknown.size_hint().upper(), None);

        let limited = response(
            ScriptedNativeBody::exact(vec![data(b"ten-bytes!")]),
            Some(6),
            mapper(),
        )
        .into_body();
        assert!(limited.size_hint().lower() <= 6);
        assert_eq!(limited.size_hint().upper(), Some(6));

        let mut with_trailers = response(
            ScriptedNativeBody::exact(vec![
                data(b"abc"),
                trailers("x-accounting-trailer"),
                data(b"de"),
            ]),
            Some(5),
            mapper(),
        )
        .into_body();
        assert_eq!(with_trailers.size_hint().exact(), Some(5));
        assert!(
            with_trailers
                .frame()
                .await
                .expect("first frame")
                .expect("first frame")
                .is_data()
        );
        assert_eq!(with_trailers.size_hint().exact(), Some(2));
        assert!(
            with_trailers
                .frame()
                .await
                .expect("trailer")
                .expect("trailer")
                .is_trailers()
        );
        assert_eq!(with_trailers.size_hint().exact(), Some(2));
    }

    #[tokio::test]
    async fn into_body_response_body_limit_counts_only_data_and_rejects_overflow_before_yielding_it()
     {
        let mut exact = response(
            ScriptedNativeBody::exact(vec![
                data(b"abc"),
                trailers("x-boundary-trailer"),
                data(b"de"),
            ]),
            Some(5),
            mapper(),
        )
        .into_body();
        assert!(exact.frame().await.expect("data").expect("data").is_data());
        assert!(
            exact
                .frame()
                .await
                .expect("trailers")
                .expect("trailers")
                .is_trailers()
        );
        assert!(exact.frame().await.expect("data").expect("data").is_data());
        assert!(exact.frame().await.is_none());

        let mut overflow = response(
            ScriptedNativeBody::exact(vec![
                data(b"abc"),
                trailers("x-before-overflow"),
                data(b"def"),
            ]),
            Some(5),
            mapper(),
        )
        .into_body();
        assert!(
            overflow
                .frame()
                .await
                .expect("valid data")
                .expect("valid data")
                .is_data()
        );
        assert!(
            overflow
                .frame()
                .await
                .expect("valid trailer")
                .expect("valid trailer")
                .is_trailers()
        );
        let error = overflow
            .frame()
            .await
            .expect("limit error")
            .expect_err("overflow frame must not be yielded");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert_eq!(error.limit(), Some(5));
        assert_eq!(error.observed(), Some(6));
        assert!(overflow.frame().await.is_none());
    }

    #[tokio::test]
    async fn into_body_redaction_maps_native_midstream_errors_without_body_or_proxy_diagnostics() {
        const BODY_SENTINEL: &str = "body-frame-secret-sentinel";
        const PROXY_SENTINEL: &str = "proxy-sentinel.invalid";
        let mut body = response(
            ScriptedNativeBody::unknown(vec![
                data(b"valid"),
                Err(NativeTestError(
                    "body-frame-secret-sentinel via proxy-sentinel.invalid",
                )),
            ]),
            None,
            proxy_mapper(),
        )
        .into_body();
        assert!(
            body.frame()
                .await
                .expect("valid frame")
                .expect("valid frame")
                .is_data()
        );
        let error = body
            .frame()
            .await
            .expect("native error")
            .expect_err("mid-stream failure");
        let diagnostics = format!("{error} {error:?}");
        assert!(!diagnostics.contains(BODY_SENTINEL));
        assert!(!diagnostics.contains(PROXY_SENTINEL));
        assert!(body.frame().await.is_none());
    }

    #[tokio::test]
    async fn next_chunk_and_write_to_file_remain_data_only() {
        let mut chunks = response(
            ScriptedNativeBody::exact(vec![
                data(b"first"),
                trailers("x-data-only"),
                data(b"second"),
            ]),
            None,
            mapper(),
        );
        assert_eq!(
            chunks.next_chunk().await.expect("first"),
            Some(Bytes::from_static(b"first"))
        );
        assert_eq!(
            chunks.next_chunk().await.expect("second"),
            Some(Bytes::from_static(b"second"))
        );
        assert_eq!(chunks.next_chunk().await.expect("EOF"), None);

        let mut file_response = response(
            ScriptedNativeBody::exact(vec![
                data(b"first"),
                trailers("x-file-trailer"),
                data(b"second"),
            ]),
            None,
            mapper(),
        );
        let path = std::env::temp_dir().join(format!(
            "concord-frame-aware-stream-{}-{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        file_response
            .write_to_file(&path)
            .await
            .expect("write data frames");
        assert_eq!(
            tokio::fs::read(&path).await.expect("read file"),
            b"firstsecond"
        );
        tokio::fs::remove_file(path).await.expect("remove file");
    }

    #[tokio::test]
    async fn next_chunk_response_body_limit_failure_is_permanently_terminal() {
        let mut response = response(
            ScriptedNativeBody::exact(vec![data(b"abc"), data(b"def"), data(b"tail")]),
            Some(5),
            mapper(),
        );

        assert_eq!(
            response.next_chunk().await.expect("first chunk"),
            Some(Bytes::from_static(b"abc"))
        );
        let error = response.next_chunk().await.expect_err("overflow must fail");
        assert!(matches!(
            error,
            ApiClientError::ResponseBodyLimitExceeded { limit: 5, .. }
        ));
        assert_eq!(response.next_chunk().await.expect("terminal"), None);
        assert_eq!(response.next_chunk().await.expect("terminal again"), None);

        let mut extracted = response.into_body();
        assert!(extracted.frame().await.is_none());
        assert!(extracted.frame().await.is_none());
        assert_eq!(extracted.size_hint().exact(), Some(0));
    }

    #[tokio::test]
    async fn next_chunk_redaction_native_error_is_mapped_once_and_permanently_terminal() {
        const BODY_SENTINEL: &str = "body-after-native-error-sentinel";
        const PROXY_SENTINEL: &str = "proxy-sentinel.invalid";
        let mut response = response(
            ScriptedNativeBody::unknown(vec![
                data(b"valid"),
                Err(NativeTestError(
                    "body-after-native-error-sentinel via proxy-sentinel.invalid",
                )),
                data(b"additional"),
            ]),
            None,
            proxy_mapper(),
        );

        assert_eq!(
            response.next_chunk().await.expect("valid data"),
            Some(Bytes::from_static(b"valid"))
        );
        let error = response
            .next_chunk()
            .await
            .expect_err("native body failure");
        let diagnostics = format!("{error} {error:?}");
        assert!(!diagnostics.contains(BODY_SENTINEL));
        assert!(!diagnostics.contains(PROXY_SENTINEL));
        assert_eq!(response.next_chunk().await.expect("terminal"), None);

        let mut extracted = response.into_body();
        assert!(extracted.frame().await.is_none());
        assert!(extracted.frame().await.is_none());
    }

    #[tokio::test]
    async fn next_chunk_eof_and_into_body_remain_terminal() {
        let mut response = response(
            ScriptedNativeBody::exact(vec![data(b"complete")]),
            None,
            mapper(),
        );

        assert_eq!(
            response.next_chunk().await.expect("data"),
            Some(Bytes::from_static(b"complete"))
        );
        assert_eq!(response.next_chunk().await.expect("EOF"), None);
        assert_eq!(response.next_chunk().await.expect("repeated EOF"), None);

        let mut extracted = response.into_body();
        assert!(extracted.frame().await.is_none());
        assert!(extracted.frame().await.is_none());
        assert_eq!(extracted.size_hint().exact(), Some(0));
    }

    #[tokio::test]
    async fn partial_next_chunk_then_into_body_preserves_frames_hint_and_limit() {
        let mut response = response(
            ScriptedNativeBody::exact(vec![
                data(b"abc"),
                trailers("x-after-partial-chunk"),
                data(b"de"),
                data(b"x"),
            ]),
            Some(5),
            mapper(),
        );

        assert_eq!(
            response.next_chunk().await.expect("first chunk"),
            Some(Bytes::from_static(b"abc"))
        );

        let mut body = response.into_body();
        assert_eq!(body.size_hint().lower(), 0);
        assert_eq!(body.size_hint().upper(), Some(2));
        let trailer = body
            .frame()
            .await
            .expect("trailer")
            .expect("trailer")
            .into_trailers()
            .expect("trailer frame");
        assert_eq!(trailer["x-after-partial-chunk"], "present");
        assert_eq!(body.size_hint().upper(), Some(2));
        assert_eq!(
            body.frame()
                .await
                .expect("remaining data")
                .expect("remaining data")
                .into_data()
                .expect("data frame"),
            Bytes::from_static(b"de")
        );
        let error = body
            .frame()
            .await
            .expect("limit error")
            .expect_err("prior bytes must count toward limit");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert_eq!(error.limit(), Some(5));
        assert_eq!(error.observed(), Some(6));
        assert!(body.frame().await.is_none());
    }
}
