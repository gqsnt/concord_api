//! The explicit accepted-bytes terminal (§10.4, V3-E09): it bypasses
//! representation-specific accepted-head validation and conversion while
//! retaining origin, status, `Accept` negotiation and body limits.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::api;
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::endpoint::AcceptedBytesRequest;
use concord::error::ErrorCategory;
use concord::http::StatusCode;
use concord::response::{Buffered, ResponseBodyError, ResponseHead, ResponseRepresentation};
use concord::{ApiResponse, Binary, Bytes, Empty, Text};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use tokio::sync::{Mutex, MutexGuard};

use crate::support::loopback_proxy;

static CONTENT_TYPE_VALIDATIONS: AtomicUsize = AtomicUsize::new(0);
static DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);
static ACCEPTED_SPEC_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
static PROBE_LOCK: Mutex<()> = Mutex::const_new(());

async fn probe_guard() -> MutexGuard<'static, ()> {
    PROBE_LOCK.lock().await
}

pub struct ObservedCodec;

impl DecodeBody for ObservedCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-observed-codec");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        CONTENT_TYPE_VALIDATIONS.fetch_add(1, Ordering::SeqCst);
        if content_type == <Self as DecodeBody>::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}

pub struct AcceptedMissing;

impl ResponseRepresentation<String> for AcceptedMissing {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some("application/x-accepted-missing");

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        ACCEPTED_SPEC_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<String, ResponseBodyError> {
        ACCEPTED_SPEC_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        Ok(String::from_utf8_lossy(&body).into_owned())
    }
}

api! {
    client AcceptedBytesApi {
        base "http://response-bytes.example.test/root/";
        timeout 30s;

        GET text_value() -> String via Text { path "text"; expose accepted_bytes; }
        GET custom_value() -> String via ObservedCodec { path "custom"; expose accepted_bytes; }
        GET bytes_value() -> Bytes via Binary { path "bytes"; expose accepted_bytes; }
        GET empty_value() -> () via Empty { path "empty"; expose accepted_bytes; }
        GET accepted_missing() -> String via AcceptedMissing {
            path "accepted";
            status 404;
            expose accepted_bytes;
        }
        GET stream_value() -> ::concord::response::ByteStream via Binary { path "stream"; }
    }
}

api! {
    client AcceptedBytesJsonApi {
        base "http://response-bytes-json.example.test/";
        timeout 30s;

        GET json_value() -> Vec<u32> via crate::support::Json { expose accepted_bytes; }
    }
}

fn client(server: &ScriptedServer, response_limit: usize) -> AcceptedBytesApi {
    AcceptedBytesApi::builder()
        .configure_http({
            let proxy = loopback_proxy(server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| runtime.response_body_limit(response_limit))
        .build()
        .expect("response-bytes client")
}

#[test]
fn eligible_generated_modes_expose_the_exact_accepted_bytes_terminal_type() {
    let client = AcceptedBytesApi::new().expect("side-effect-free client construction");

    let _: AcceptedBytesRequest = client.text_value().accepted_bytes();
    let _: AcceptedBytesRequest = client.custom_value().accepted_bytes();
    let _: AcceptedBytesRequest = client.bytes_value().accepted_bytes();
    let _: AcceptedBytesRequest = client.empty_value().accepted_bytes();
    let _: AcceptedBytesRequest = client.accepted_missing().accepted_bytes();

    let _direct_await_remains_selected = client.bytes_value();
    let _metadata_terminal_remains_selected = client.bytes_value().response();

    let contract = AcceptedBytesApi::contract();
    for id in [
        "accepted_bytes_api.text_value",
        "accepted_bytes_api.custom_value",
        "accepted_bytes_api.bytes_value",
        "accepted_bytes_api.empty_value",
        "accepted_bytes_api.accepted_missing",
    ] {
        assert!(
            contract
                .endpoint(id)
                .expect("endpoint")
                .terminals()
                .accepted_bytes(),
            "{id}"
        );
    }
    // CALL-03: an undeclared terminal is absent, not a runtime error.
    assert!(
        !contract
            .endpoint("accepted_bytes_api.stream_value")
            .expect("stream endpoint")
            .terminals()
            .accepted_bytes()
    );
}

#[test]
fn generated_json_mode_exposes_the_accepted_bytes_terminal() {
    let client = AcceptedBytesJsonApi::new().expect("side-effect-free JSON client construction");
    let _: AcceptedBytesRequest = client.json_value().accepted_bytes();
}

#[tokio::test]
async fn accepted_bytes_skips_declared_conversion_and_preserves_accept_status_and_metadata() {
    let _guard = probe_guard().await;
    CONTENT_TYPE_VALIDATIONS.store(0, Ordering::SeqCst);
    DECODER_CALLS.store(0, Ordering::SeqCst);
    ACCEPTED_SPEC_CALLBACKS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        ResponseScript::ok(b"response-body-secret-618ae4".to_vec())
            .with_header("content-type", "application/x-intentionally-different")
            .with_header("x-response-secret", "response-header-secret-9e672c"),
        ResponseScript::ok(b"ordinary-direct".to_vec())
            .with_header("content-type", "application/x-observed-codec"),
        ResponseScript::ok(b"ordinary-metadata".to_vec())
            .with_header("content-type", "application/x-observed-codec"),
        ResponseScript::ok(b"no-content-still-collected".to_vec()),
        ResponseScript::status(404, b"accepted-missing-body".to_vec())
            .with_header("content-type", "application/x-intentionally-different"),
    ])
    .expect("response-bytes server");
    let client = client(&server, 1024);

    let response: ApiResponse<Bytes> = client
        .custom_value()
        .accepted_bytes()
        .await
        .expect("bounded bytes must not use the declared decoder");
    assert_eq!(CONTENT_TYPE_VALIDATIONS.load(Ordering::SeqCst), 0);
    assert_eq!(DECODER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(
        response.url().as_str(),
        "http://response-bytes.example.test/root/custom"
    );
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.version(), concord::http::Version::HTTP_11);
    assert_eq!(
        response.headers()["x-response-secret"],
        "response-header-secret-9e672c"
    );
    assert_eq!(
        response.metadata().endpoint(),
        "accepted_bytes_api.custom_value"
    );
    assert_eq!(response.metadata().method(), concord::http::Method::GET);
    assert_eq!(response.metadata().page_index(), 0);
    assert!(!response.metadata().recovered_authentication());
    assert_eq!(response.value(), b"response-body-secret-618ae4".as_slice());

    let debug = format!("{response:?}");
    for forbidden in [
        "response-body-secret-618ae4",
        "response-header-secret-9e672c",
        "response-bytes.example.test",
        "root/custom",
    ] {
        assert!(
            !debug.contains(forbidden),
            "ApiResponse Debug leaked {forbidden}"
        );
    }
    for expected in ["ApiResponse", "200", "header_count", "ExecutionMeta"] {
        assert!(
            debug.contains(expected),
            "ApiResponse Debug omitted {expected}: {debug}"
        );
    }
    assert_eq!(
        response.into_value(),
        b"response-body-secret-618ae4".as_slice()
    );

    assert_eq!(
        client
            .custom_value()
            .await
            .expect("ordinary direct decode remains unchanged"),
        "ordinary-direct"
    );
    let metadata: ApiResponse<String> = client
        .custom_value()
        .response()
        .await
        .expect("ordinary metadata decode remains unchanged");
    assert_eq!(metadata.value(), "ordinary-metadata");
    assert_eq!(CONTENT_TYPE_VALIDATIONS.load(Ordering::SeqCst), 2);
    assert_eq!(DECODER_CALLS.load(Ordering::SeqCst), 2);

    let no_content = client
        .empty_value()
        .accepted_bytes()
        .await
        .expect("empty declaration still collects explicit bytes");
    assert_eq!(no_content.value(), b"no-content-still-collected".as_slice());

    let accepted = client
        .accepted_missing()
        .accepted_bytes()
        .await
        .expect("declared accepted status");
    assert_eq!(accepted.status(), StatusCode::NOT_FOUND);
    assert_eq!(accepted.value(), b"accepted-missing-body".as_slice());
    assert_eq!(ACCEPTED_SPEC_CALLBACKS.load(Ordering::SeqCst), 0);

    let requests = server.requests();
    assert_eq!(
        requests[0].header("accept"),
        Some("application/x-observed-codec"),
        "accepted bytes keep the declared Accept negotiation"
    );
    assert_eq!(
        requests[4].header("accept"),
        Some("application/x-accepted-missing")
    );
}

#[tokio::test]
async fn generated_direct_and_explicit_endpoints_share_accepted_bytes_semantics() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"generated-direct".to_vec()),
        ResponseScript::ok(b"explicit-endpoint".to_vec()),
    ])
    .expect("generated response-bytes parity server");
    let client = client(&server, 1024);

    let generated = client
        .bytes_value()
        .accepted_bytes()
        .await
        .expect("generated response bytes");
    let explicit = client
        .request(AcceptedBytesApi::unbound().bytes_value())
        .accepted_bytes()
        .await
        .expect("explicit endpoint response bytes");

    assert_eq!(generated.value(), b"generated-direct".as_slice());
    assert_eq!(explicit.value(), b"explicit-endpoint".as_slice());
    assert_eq!(
        generated.metadata().endpoint(),
        explicit.metadata().endpoint()
    );
}

#[tokio::test]
async fn accepted_bytes_uses_known_length_preflight_and_never_returns_partial_excess() {
    let known_gate = BodyGate::closed();
    let known_server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"known-excess-secret".to_vec(),
        gate: known_gate.clone(),
    }])
    .expect("known-length server");
    let known_result = tokio::time::timeout(
        Duration::from_secs(1),
        client(&known_server, 4).bytes_value().accepted_bytes(),
    )
    .await
    .expect("known excess is rejected from the response head");
    known_gate.release();
    let known_error = known_result.expect_err("known response exceeds the limit");
    assert_eq!(known_error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{known_error:?} {known_error}").contains("known-excess-secret"));

    let unknown_server = ScriptedServer::start([ResponseScript::Chunked {
        status: 200,
        headers: Vec::new(),
        chunks: vec![b"four".to_vec(), b"first-excess-secret".to_vec()],
    }])
    .expect("unknown-length server");
    let unknown_error = client(&unknown_server, 4)
        .bytes_value()
        .accepted_bytes()
        .await
        .expect_err("first cumulative excess returns no partial response");
    assert_eq!(unknown_error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{unknown_error:?} {unknown_error}").contains("first-excess-secret"));
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn accepted_bytes_limit_counts_post_decompression_bytes() {
    const GZIP_128_X: &[u8] = &[
        31, 139, 8, 0, 0, 0, 0, 0, 0, 10, 171, 168, 24, 88, 0, 0, 55, 138, 224, 172, 128, 0, 0, 0,
    ];
    let server = ScriptedServer::start([ResponseScript::Complete {
        status: 200,
        headers: vec![("Content-Encoding".into(), "gzip".into())],
        body: GZIP_128_X.to_vec(),
    }])
    .expect("compressed response-bytes server");

    let error = client(&server, 64)
        .bytes_value()
        .accepted_bytes()
        .await
        .expect_err("128 decoded bytes exceed the 64-byte response limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
}

/// PRODUCT-04: the brotli and deflate feature paths decode through a
/// generated client, and the response limit counts decoded bytes.
///
/// The constants are reference encoder output for 128 `x` bytes, produced
/// offline and deterministically because neither encoder is a dependency:
/// `brotli` 8.0.4 `CompressorWriter` (quality 11, window 22) and Python
/// `zlib.compress(b"x" * 128, 9)` (RFC 1950 zlib framing, which HTTP
/// `Content-Encoding: deflate` denotes). Each is 12 encoded bytes.
#[cfg(any(feature = "brotli", feature = "deflate"))]
async fn assert_accepted_bytes_decode_and_limit_post_decompression(encoding: &str, encoded: &[u8]) {
    let script = || ResponseScript::Complete {
        status: 200,
        headers: vec![("Content-Encoding".into(), encoding.into())],
        body: encoded.to_vec(),
    };
    let server = ScriptedServer::start([script(), script()]).expect("compressed server");

    let decoded = client(&server, 128)
        .bytes_value()
        .accepted_bytes()
        .await
        .expect("128 decoded bytes fit a 128-byte limit");
    assert_eq!(decoded.value().as_ref(), &[b'x'; 128][..], "{encoding}");

    let error = client(&server, 64)
        .bytes_value()
        .accepted_bytes()
        .await
        .expect_err("128 decoded bytes exceed the 64-byte response limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody, "{encoding}");
}

#[cfg(feature = "brotli")]
#[tokio::test]
async fn accepted_bytes_limit_counts_brotli_post_decompression_bytes() {
    const BROTLI_128_X: &[u8] = &[139, 63, 0, 248, 37, 240, 34, 177, 64, 32, 27, 3];
    assert_accepted_bytes_decode_and_limit_post_decompression("br", BROTLI_128_X).await;
}

#[cfg(feature = "deflate")]
#[tokio::test]
async fn accepted_bytes_limit_counts_deflate_post_decompression_bytes() {
    const DEFLATE_128_X: &[u8] = &[120, 218, 171, 168, 24, 88, 0, 0, 31, 97, 60, 1];
    assert_accepted_bytes_decode_and_limit_post_decompression("deflate", DEFLATE_128_X).await;
}

#[tokio::test]
async fn non_accepted_status_remains_the_ordinary_typed_status_error() {
    let server = ScriptedServer::start([ResponseScript::status(
        422,
        b"response-terminal-status-secret".to_vec(),
    )])
    .expect("status server");
    let error = client(&server, 1024)
        .bytes_value()
        .accepted_bytes()
        .await
        .expect_err("non-accepted status cannot produce response bytes");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(!format!("{error:?} {error}").contains("response-terminal-status-secret"));
}
