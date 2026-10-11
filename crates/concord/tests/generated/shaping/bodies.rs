//! Plan side effects, request bodies and media ownership, head-only absence
//! and terminal status bodies of generated requests (§9, §10, §13.2).

// The plan-effects harness needs an https client; without `default-tls` its
// helpers and imports are intentionally unused.
#![cfg_attr(not(feature = "default-tls"), allow(unused_imports, dead_code))]

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use concord::auth::{AccessToken, AuthError, BoxFuture, CredentialProvider, CredentialRequest};
use concord::body::{
    BodyDescription, BodyError, PreparedBody, RequestBodyContext, RequestRepresentation,
    RequestStream,
};
use concord::codec::{
    CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext, NotFound,
};
use concord::error::{
    DispatchKnowledge, ErrorCategory, StatusBodyDecodeErrorKind, StatusBodyState,
};
#[cfg(feature = "default-tls")]
use concord::http::HeaderName;
use concord::rate_limit::{
    Enforcement, LimiterCapability, RateLimitError, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimiter,
};
use concord::response::BodyDisposition;
use concord::{ApiResponse, Binary, Bytes, Text, api};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use serde::Deserialize;

use super::request_shaping_test_guard;
use crate::support::{Json, loopback_proxy};

#[derive(Default)]
pub struct Effects {
    body: AtomicUsize,
    provider: AtomicUsize,
    limiter_acquire: AtomicUsize,
    limiter_observe: AtomicUsize,
}

impl Effects {
    fn snapshot(&self) -> [usize; 4] {
        [
            self.body.load(Ordering::SeqCst),
            self.provider.load(Ordering::SeqCst),
            self.limiter_acquire.load(Ordering::SeqCst),
            self.limiter_observe.load(Ordering::SeqCst),
        ]
    }
}

/// A body payload whose representation counts every preparation.
pub struct CountedInput {
    effects: Arc<Effects>,
    bytes: &'static [u8],
}

/// Counts preparations; the only authority for the body and its media.
pub struct CountingBody;

impl RequestRepresentation<CountedInput> for CountingBody {
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(Some("application/x-counted"));

    fn prepare(
        input: CountedInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        input.effects.body.fetch_add(1, Ordering::SeqCst);
        PreparedBody::bytes(Bytes::from_static(input.bytes)).content_type("application/x-counted")
    }
}

/// A custom credential provider that counts acquisitions.
pub struct CountingProvider {
    effects: Arc<Effects>,
}

impl CredentialProvider for CountingProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.effects.provider.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(AccessToken::try_new("plan-provider-token").expect("bounded token")) })
    }
}

/// A local limiter that counts acquisitions and observations.
#[cfg(feature = "default-tls")]
pub struct CountingLimiter {
    effects: Arc<Effects>,
}

#[cfg(feature = "default-tls")]
impl RateLimiter for CountingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "test.counting", 1)
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.effects.limiter_acquire.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, _response: RateLimitResponse<'_>) {
        self.effects.limiter_observe.fetch_add(1, Ordering::SeqCst);
    }
}

api! {
    client PlanEffectsApi {
        base "https://plan-effects.example.test/";
        timeout 5s;

        credentials {
            credential provided = custom CountingProvider;
        }

        rate_limits {
            windows standard { allow 100 per 1s; }
        }

        scope secured() {
            auth { bearer provided; }
            rate_limit "plan.effects.reads" key [origin] using standard;

            POST upload(body input: CountedInput via CountingBody) -> Bytes via Binary {
                path "upload";
            }

            POST maybe_upload(body input?: CountedInput via CountingBody) -> Bytes via Binary {
                path "maybe";
            }
        }
    }
}

api! {
    client OptionalBodyApi {
        base "http://optional-body.example.test/";
        timeout 5s;

        POST maybe_upload(body input?: CountedInput via CountingBody) -> Bytes via Binary {
            path "maybe";
        }
    }
}

/// V3-S12: planning — direct, unbound and with an absent optional body —
/// invokes no body representation, credential provider, limiter or transport.
#[cfg(feature = "default-tls")] // https base: TlsUnavailable without TLS
#[tokio::test(flavor = "current_thread")]
async fn plans_invoke_no_body_credential_limiter_or_transport_work() {
    let _guard = request_shaping_test_guard().await;
    let effects = Arc::new(Effects::default());
    let server = ScriptedServer::start([]).expect("transport trap");
    let proxy = loopback_proxy(&server);
    let limiter: Arc<dyn RateLimiter> = Arc::new(CountingLimiter {
        effects: Arc::clone(&effects),
    });
    let client = PlanEffectsApi::builder()
        .provided(CountingProvider {
            effects: Arc::clone(&effects),
        })
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("plan effects client");

    let input = || CountedInput {
        effects: Arc::clone(&effects),
        bytes: b"payload",
    };
    let direct = client
        .secured()
        .upload(input())
        .plan()
        .expect("direct body plan");
    let unbound = client
        .plan(&PlanEffectsApi::unbound().secured().upload(input()))
        .expect("unbound body plan");
    let absent = client
        .secured()
        .maybe_upload()
        .plan()
        .expect("absent optional body plan");
    let present = client
        .secured()
        .maybe_upload()
        .set_input(input())
        .plan()
        .expect("present optional body plan");

    assert_eq!(effects.snapshot(), [0, 0, 0, 0], "plans perform no effects");
    assert_eq!(server.request_count(), 0, "plans perform no transport work");
    assert_eq!(direct.observed_url(), unbound.observed_url());
    assert_eq!(direct.rate_limit_policy_count(), 1);
    assert!(direct.capabilities().authentication());
    assert_eq!(
        direct.body_description().to_string(),
        "buffered body, media hint application/x-counted; exact Content-Type deferred to preparation"
    );
    assert!(
        direct
            .public_headers()
            .get(&HeaderName::from_static("content-type"))
            .is_none(),
        "the plan never prepares the body to learn its media"
    );
    assert!(
        absent
            .public_headers()
            .get(&HeaderName::from_static("authorization"))
            .is_none(),
        "the plan never acquires or places credential material"
    );
    assert_eq!(present.observed_url(), absent.observed_url());
}

/// V3-S12/BODY-02: an outer-absent optional body never reaches the
/// representation, sends no bytes and places no body-owned media; a present
/// one is prepared exactly once per operation.
#[tokio::test(flavor = "current_thread")]
async fn absent_optional_bodies_invoke_no_representation_at_execution() {
    let _guard = request_shaping_test_guard().await;
    let effects = Arc::new(Effects::default());
    let server = ScriptedServer::start((0..2).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("optional body server");
    let proxy = loopback_proxy(&server);
    let client = OptionalBodyApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("optional body client");

    client.maybe_upload().await.expect("absent body request");
    assert_eq!(effects.snapshot()[0], 0);
    client
        .maybe_upload()
        .set_input(CountedInput {
            effects: Arc::clone(&effects),
            bytes: b"present",
        })
        .await
        .expect("present body request");
    assert_eq!(effects.snapshot()[0], 1);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("content-type"), None);
    assert!(requests[0].body().is_empty());
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/x-counted")
    );
    assert_eq!(requests[1].body(), b"present");
}

#[cfg(feature = "multipart")]
mod multipart {
    use std::sync::Arc;
    use std::time::Duration;

    use concord::body::{Multipart, MultipartBody, MultipartPart, RequestStream};
    use concord::error::ErrorCategory;
    use concord::retry::{RetryBackoff, RetryDecision, RetryPolicy};
    use concord::{Binary, Bytes, api};
    use concord_test_support::server::{ResponseScript, ScriptedServer};

    use crate::support::loopback_proxy;

    api! {
        client MultipartApi {
            base "http://multipart.example.test/";
            timeout 5s;

            PUT upload(body form: MultipartBody via Multipart) -> Bytes via Binary {
                path "upload";
            }
        }
    }

    fn form(streaming: bool) -> MultipartBody {
        let file = if streaming {
            MultipartPart::stream(RequestStream::from_bytes(Bytes::from_static(b"file-bytes")))
        } else {
            MultipartPart::bytes(Bytes::from_static(b"file-bytes")).expect("bytes part")
        };
        MultipartBody::new()
            .part("field", MultipartPart::text("value").expect("text part"))
            .expect("field part")
            .part(
                "upload",
                file.file_name("data.bin")
                    .expect("file name")
                    .content_type("application/octet-stream")
                    .expect("part media"),
            )
            .expect("file part")
    }

    fn retrying_client(server: &ScriptedServer) -> MultipartApi {
        let proxy = loopback_proxy(server);
        let policy = RetryPolicy::builder()
            .without_budget()
            .max_retries_per_operation(1)
            .backoff(RetryBackoff::fixed(Duration::from_millis(1)))
            .classify_fn(|_| RetryDecision::Retry)
            .build()
            .expect("retry policy");
        MultipartApi::builder()
            .configure_http(move |http| http.proxy(proxy))
            .retry(policy)
            .build()
            .expect("multipart client")
    }

    fn boundary(content_type: &str) -> &str {
        content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("exact multipart media type")
    }

    /// V3-E06: the boundary is generated once by preparation (never by the
    /// plan), is exact in header and body, and is reused byte-for-byte by a
    /// mechanical replay; a one-shot streaming part is never replayed.
    #[tokio::test(flavor = "current_thread")]
    async fn multipart_boundary_is_generated_once_and_one_shot_bodies_never_replay() {
        let _guard = super::request_shaping_test_guard().await;
        let server = Arc::new(
            ScriptedServer::start([
                ResponseScript::status(503, Vec::new()),
                ResponseScript::ok(b"stored".to_vec()),
                ResponseScript::status(503, Vec::new()),
            ])
            .expect("multipart server"),
        );
        let client = retrying_client(&server);

        let plan = client.upload(form(false)).plan().expect("multipart plan");
        assert!(
            plan.body_description()
                .boundary_generated_during_execution()
        );
        assert_eq!(
            plan.body_description().to_string(),
            "multipart/form-data; boundary generated during execution"
        );
        assert!(
            plan.public_headers()
                .get(&concord::http::HeaderName::from_static("content-type"))
                .is_none()
        );
        assert_eq!(server.request_count(), 0);

        assert_eq!(
            client
                .upload(form(false))
                .await
                .expect("replayed multipart"),
            b"stored"[..]
        );
        let requests = server.requests();
        assert_eq!(requests.len(), 2, "a replayable recipe is retried once");
        let first = requests[0].header("content-type").expect("multipart media");
        let marker = boundary(first);
        assert!(!marker.is_empty());
        assert_eq!(
            requests[1].header("content-type"),
            Some(first),
            "the replay reuses the prepared boundary"
        );
        assert_eq!(requests[0].body(), requests[1].body());
        let body = String::from_utf8(requests[0].body().to_vec()).expect("UTF-8 fixture body");
        assert!(body.starts_with(&format!("--{marker}\r\n")), "{body}");
        assert!(
            body.trim_end().ends_with(&format!("--{marker}--")),
            "{body}"
        );
        assert_eq!(body.matches(&format!("--{marker}")).count(), 3);
        assert!(body.contains("name=\"field\"\r\n\r\nvalue\r\n"), "{body}");
        assert!(body.contains("filename=\"data.bin\""), "{body}");

        let error = client
            .upload(form(true))
            .await
            .expect_err("one-shot multipart is not replayed");
        assert_eq!(error.category(), ErrorCategory::HttpStatus);
        assert_eq!(
            server.request_count(),
            3,
            "no automatic replay of a one-shot body"
        );
    }
}

/// A downstream buffered encoder that respects the body limit while
/// serializing and stops at the first excess (no serialize-then-truncate).
pub struct LineEncoder;

pub static LINES_ENCODED: AtomicUsize = AtomicUsize::new(0);

impl EncodeBody for LineEncoder {
    type Input = Vec<String>;

    const CONTENT_TYPE: Option<&'static str> = Some("text/x-lines");

    fn encode(input: Vec<String>, context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        let limit = context.body_limit.unwrap_or(usize::MAX);
        let mut output = Vec::new();
        for line in input {
            if output.len() + line.len() + 1 > limit {
                return Err(CodecError::new(CodecErrorKind::Limit));
            }
            LINES_ENCODED.fetch_add(1, Ordering::SeqCst);
            output.extend_from_slice(line.as_bytes());
            output.push(b'\n');
        }
        Ok(output.into())
    }
}

/// A downstream buffered decoder bridged by the blanket implementation.
pub struct LineDecoder;

impl DecodeBody for LineDecoder {
    type Output = Vec<String>;

    const ACCEPT: Option<&'static str> = Some("text/x-lines");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Vec<String>, CodecError> {
        let text =
            std::str::from_utf8(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        Ok(text.lines().map(str::to_owned).collect())
    }
}

/// A finite unknown-length stream of fixed chunks.
struct Chunks(Vec<&'static [u8]>);

impl futures_core::Stream for Chunks {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.0.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Ready(Some(Ok(Bytes::from_static(self.0.remove(0)))))
        }
    }
}

api! {
    client LimitApi {
        base "http://limits.example.test/";
        timeout 5s;

        POST text(body input: String via Text) -> Bytes via Binary { path "text"; }
        POST stream(body input: RequestStream via Binary) -> Bytes via Binary { path "stream"; }
        POST lines(body rows: Vec<String> via LineEncoder) -> Vec<String> via LineDecoder {
            path "lines";
        }
    }
}

/// V3-E07: the configured request-body limit is enforced for known lengths
/// before dispatch, during streaming for unknown lengths, and by a downstream
/// `EncodeBody` while serializing; `EncodeBody`/`DecodeBody` bridges execute
/// with exactly one media source each.
#[tokio::test(flavor = "current_thread")]
async fn request_body_limits_apply_before_dispatch_during_streaming_and_serialization() {
    let _guard = request_shaping_test_guard().await;
    LINES_ENCODED.store(0, Ordering::SeqCst);
    // The streaming excess runs last: an aborted upload may or may not
    // consume the final absorber script.
    let server = ScriptedServer::start([
        ResponseScript::ok(b"exact".to_vec()),
        ResponseScript::ok(b"streamed".to_vec()),
        ResponseScript::ok(b"one\ntwo\n".to_vec()).with_header("Content-Type", "text/x-lines"),
        ResponseScript::ok(b"absorber".to_vec()),
    ])
    .expect("limit server");
    let proxy = loopback_proxy(&server);
    let client = LimitApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.request_body_limit(8))
        .build()
        .expect("limit client");

    assert_eq!(
        client
            .text("12345678".to_owned())
            .await
            .expect("exact limit"),
        b"exact"[..]
    );
    let error = client
        .text("123456789".to_owned())
        .await
        .expect_err("known-length first excess");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(
        server.request_count(),
        1,
        "known excess is rejected before dispatch"
    );

    assert_eq!(
        client
            .stream(RequestStream::from_stream(Chunks(vec![b"1234", b"5678"])))
            .await
            .expect("unknown-length stream at the limit"),
        b"streamed"[..]
    );

    let decoded = client
        .lines(vec!["one".to_owned(), "two".to_owned()])
        .await
        .expect("bridged codecs");
    assert_eq!(decoded, ["one", "two"]);
    assert_eq!(LINES_ENCODED.load(Ordering::SeqCst), 2);
    let error = client
        .lines(vec!["one".to_owned(), "two".to_owned(), "three".to_owned()])
        .await
        .expect_err("serialization stops at the first excess");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(
        LINES_ENCODED.load(Ordering::SeqCst),
        4,
        "the encoder stopped before the excess line instead of truncating"
    );

    let error = client
        .stream(RequestStream::from_stream(Chunks(vec![
            b"1234", b"5678", b"9",
        ])))
        .await
        .expect_err("streaming first excess");
    assert_eq!(error.category(), ErrorCategory::RequestBody);

    let requests = server.requests();
    let lines = requests
        .iter()
        .find(|request| request.target().ends_with("/lines"))
        .expect("bridged request");
    assert_eq!(lines.header("content-type"), Some("text/x-lines"));
    assert_eq!(lines.header("accept"), Some("text/x-lines"));
    assert_eq!(lines.body(), b"one\ntwo\n");
    let exact = requests
        .iter()
        .find(|request| request.target().ends_with("/text"))
        .expect("text request");
    assert_eq!(
        exact.header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(exact.body(), b"12345678");
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Row {
    id: u32,
}

api! {
    client AbsenceApi {
        base "http://absence.example.test/";
        timeout 5s;

        GET find(id: u32) -> Option<Row> via Json {
            path "items/{id}";
            absent on 404;
        }

        GET buffered(id: u32) -> Option<Row> via NotFound<Json> {
            path "rows/{id}";
            status [200, 404];
        }
    }
}

fn json(body: &str) -> ResponseScript {
    ResponseScript::ok(body.as_bytes().to_vec()).with_header("Content-Type", "application/json")
}

/// V3-E08: head-only `absent on 404` decodes 200 normally, returns `None`
/// for a 404 whose huge, truncated or stalled body is never polled, and
/// keeps 500 an HTTP status error; the buffered `NotFound` comparator does
/// acquire the 404 body and can therefore fail on it (OUTCOME-02).
#[tokio::test(flavor = "current_thread")]
async fn head_only_absence_never_polls_the_missing_body() {
    let _guard = request_shaping_test_guard().await;
    let stalled = BodyGate::closed();
    let truncated_missing = || ResponseScript::Truncated {
        status: 404,
        declared_length: 10_000_000,
        prefix: b"partial".to_vec(),
    };
    // The stalled body is scripted last so a held gate cannot delay the
    // other scripted connections.
    let server = ScriptedServer::start([
        json("{\"id\":1}"),
        truncated_missing(),
        truncated_missing(),
        ResponseScript::status(500, b"server error".to_vec()),
        truncated_missing(),
        ResponseScript::GatedBody {
            status: 404,
            body: b"never polled".to_vec(),
            gate: stalled.clone(),
        },
    ])
    .expect("absence server");
    let proxy = loopback_proxy(&server);
    let client = AbsenceApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.response_body_limit(64))
        .build()
        .expect("absence client");

    assert_eq!(client.find(1).await.expect("present"), Some(Row { id: 1 }));
    assert_eq!(client.find(2).await.expect("huge truncated 404"), None);
    let response: ApiResponse<Option<Row>> = client
        .find(3)
        .response()
        .await
        .expect("absent response metadata");
    assert_eq!(response.status().as_u16(), 404);
    assert_eq!(response.value(), &None);
    assert_eq!(
        response.body_disposition(),
        BodyDisposition::DiscardedUnread
    );
    let error = client.find(4).await.expect_err("500 stays a status error");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status().map(|status| status.as_u16()), Some(500));

    let error = client
        .buffered(5)
        .await
        .expect_err("buffered NotFound acquires the truncated 404 body");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);

    let stalled_result = tokio::time::timeout(Duration::from_secs(2), client.find(6))
        .await
        .expect("a stalled 404 body is never awaited");
    assert_eq!(stalled_result.expect("stalled 404"), None);
    stalled.release();
}

api! {
    client StatusApi {
        base "http://status.example.test/";
        timeout 5s;

        GET read(id: u32) -> Bytes via Binary {
            path "items/{id}";
        }
    }
}

/// V3-E10: rejected statuses carry complete-only status bodies; decoding is
/// explicit and never mutates the failure; a capture failure is recorded as
/// body-state evidence without replacing the status classification.
#[tokio::test(flavor = "current_thread")]
async fn terminal_status_bodies_are_complete_only_and_explicitly_decoded() {
    const BODY_SENTINEL: &str = "status-body-sentinel-6a1f";
    let _guard = request_shaping_test_guard().await;
    let long = "x".repeat(64);
    let server = ScriptedServer::start([
        ResponseScript::status(422, b"{\"id\":7}".to_vec())
            .with_header("Content-Type", "application/json"),
        ResponseScript::status(500, Vec::new()),
        ResponseScript::status(500, long.clone().into_bytes()),
        ResponseScript::Chunked {
            status: 500,
            headers: Vec::new(),
            chunks: vec![long.clone().into_bytes(), long.into_bytes()],
        },
        ResponseScript::Truncated {
            status: 503,
            declared_length: 24,
            prefix: b"partial".to_vec(),
        },
        ResponseScript::status(502, BODY_SENTINEL.as_bytes().to_vec())
            .with_header("Content-Type", "text/plain"),
    ])
    .expect("status server");
    let proxy = loopback_proxy(&server);
    let client = StatusApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.status_body_limit(32))
        .build()
        .expect("status client");

    let complete = client.read(1).await.expect_err("422");
    assert_eq!(complete.category(), ErrorCategory::HttpStatus);
    let failure = complete.http_response().expect("status facts");
    assert_eq!(failure.status().as_u16(), 422);
    assert_eq!(failure.body().state(), StatusBodyState::Complete);
    assert_eq!(failure.body().len(), Some(8));
    assert_eq!(
        failure.body().bytes().map(|bytes| &bytes[..]),
        Some(&b"{\"id\":7}"[..])
    );
    assert_eq!(
        failure
            .decode_status_body::<Json, Row>()
            .expect("explicit JSON decode"),
        Row { id: 7 }
    );
    assert_eq!(complete.category(), ErrorCategory::HttpStatus);
    assert_eq!(
        complete.http_status().map(|status| status.as_u16()),
        Some(422)
    );

    let empty = client.read(2).await.expect_err("empty 500");
    let body = empty.http_response().expect("status facts").body();
    assert_eq!(body.state(), StatusBodyState::Empty);
    assert_eq!(body.len(), Some(0));
    assert!(body.bytes().is_none());
    assert_eq!(
        empty
            .http_response()
            .expect("status facts")
            .decode_status_body::<Json, Row>()
            .expect_err("no complete body")
            .kind(),
        StatusBodyDecodeErrorKind::Unavailable(StatusBodyState::Empty)
    );

    let known = client.read(3).await.expect_err("known too large");
    let body = known.http_response().expect("status facts").body();
    assert_eq!(body.state(), StatusBodyState::OmittedKnownTooLarge);
    assert_eq!(body.len(), None);
    assert!(body.bytes().is_none(), "no truncated prefix is exposed");

    let unknown = client.read(4).await.expect_err("unknown too large");
    let body = unknown.http_response().expect("status facts").body();
    assert_eq!(body.state(), StatusBodyState::OmittedLimitExceeded);
    assert!(body.bytes().is_none());

    let truncated = client.read(5).await.expect_err("capture failure");
    assert_eq!(
        truncated.category(),
        ErrorCategory::HttpStatus,
        "a failed capture does not replace the status classification"
    );
    let failure = truncated.http_response().expect("status facts");
    assert_eq!(failure.status().as_u16(), 503);
    assert_eq!(
        failure.body().state(),
        StatusBodyState::UnavailableReadFailure
    );

    let media = client.read(6).await.expect_err("text status body");
    let failure = media.http_response().expect("status facts");
    assert_eq!(failure.body().state(), StatusBodyState::Complete);
    assert_eq!(
        failure
            .decode_status_body::<Json, Row>()
            .expect_err("JSON requires JSON media")
            .kind(),
        StatusBodyDecodeErrorKind::InvalidMediaType
    );
    let automatic = format!("{media}\n{media:?}\n{failure:?}\n{:?}", failure.body());
    assert!(!automatic.contains(BODY_SENTINEL), "{automatic}");
}

api! {
    client OptionalBodyControlsApi {
        base "http://optional-body-controls.example.test/";
        timeout 5s;

        PUT put(body payload?: String via Text) -> Bytes via Binary {
            path "payload";
        }
    }
}

/// V3-L10 (§4.3 matrix row "Optional body", VALUE-03, BODY-02): an optional
/// body has exactly `set_x` (exact payload, no conversion), `replace_x` and
/// `set_x_if_some` (exact `Option<T>`) and `omit_x`, on the bound request
/// and on the unbound intent. On the wire, `replace_payload(None)` clears a
/// previously selected body (no bytes, no body media header) while
/// `set_payload_if_some(None)` keeps it and `Some` replaces it.
#[tokio::test(flavor = "current_thread")]
async fn optional_body_exact_controls_follow_the_value_matrix_on_the_wire() {
    use optional_body_controls_api::{PutEndpoint, PutRequest};

    // Exact signatures: bound request and unbound intent.
    let _: fn(PutRequest, String) -> PutRequest = PutRequest::set_payload;
    let _: fn(PutRequest, Option<String>) -> PutRequest = PutRequest::replace_payload;
    let _: fn(PutRequest, Option<String>) -> PutRequest = PutRequest::set_payload_if_some;
    let _: fn(PutRequest) -> PutRequest = PutRequest::omit_payload;
    let _: fn(PutEndpoint, String) -> PutEndpoint = PutEndpoint::set_payload;
    let _: fn(PutEndpoint, Option<String>) -> PutEndpoint = PutEndpoint::replace_payload;
    let _: fn(PutEndpoint, Option<String>) -> PutEndpoint = PutEndpoint::set_payload_if_some;
    let _: fn(PutEndpoint) -> PutEndpoint = PutEndpoint::omit_payload;

    let _guard = request_shaping_test_guard().await;
    let server = ScriptedServer::start((0..8).map(|_| ResponseScript::ok(b"ok".to_vec())))
        .expect("optional body controls server");
    let proxy = loopback_proxy(&server);
    let client = OptionalBodyControlsApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("optional body controls client");
    let selected = || client.put().set_payload("first-body".to_owned());

    // replace_x(None): current := Absent, even after a selected body.
    selected()
        .replace_payload(None)
        .await
        .expect("cleared body");
    // set_x_if_some(None): current := current.
    selected()
        .set_payload_if_some(None)
        .await
        .expect("kept body");
    // set_x_if_some(Some(v)): current := Present(v).
    selected()
        .set_payload_if_some(Some("second-body".to_owned()))
        .await
        .expect("replaced body");
    // replace_x(Some(v)): current := Present(v) from the initial absent slot.
    client
        .put()
        .replace_payload(Some("third-body".to_owned()))
        .await
        .expect("present body");
    // omit_x(): current := Absent.
    selected().omit_payload().await.expect("omitted body");
    // set_x_if_some(None) on the initial absent slot keeps it absent.
    client
        .put()
        .set_payload_if_some(None)
        .await
        .expect("still absent body");
    // The same transitions on the unbound intent, bound afterwards.
    let unbound = || OptionalBodyControlsApi::unbound().put();
    client
        .request(
            unbound()
                .set_payload("intent-body".to_owned())
                .replace_payload(None),
        )
        .await
        .expect("intent cleared body");
    client
        .request(
            unbound()
                .set_payload("intent-body".to_owned())
                .set_payload_if_some(None),
        )
        .await
        .expect("intent kept body");

    let requests = server.requests();
    assert_eq!(requests.len(), 8);
    let expected: [Option<&[u8]>; 8] = [
        None,
        Some(b"first-body"),
        Some(b"second-body"),
        Some(b"third-body"),
        None,
        None,
        None,
        Some(b"intent-body"),
    ];
    for (index, (request, expected)) in requests.iter().zip(expected).enumerate() {
        assert_eq!(request.method(), "PUT", "request {index}");
        match expected {
            Some(body) => {
                assert_eq!(request.body(), body, "request {index}");
                assert_eq!(
                    request.header("content-type"),
                    Some("text/plain; charset=utf-8"),
                    "request {index}"
                );
            }
            None => {
                assert!(request.body().is_empty(), "request {index}");
                assert_eq!(request.header("content-type"), None, "request {index}");
            }
        }
    }
}
