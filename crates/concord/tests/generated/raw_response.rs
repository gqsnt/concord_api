#![cfg(feature = "dangerous-raw-response")]

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::api;
use concord::body::Stream;
use concord::codec::{CodecError, DecodeBody, DecodeContext, OctetStream};
use concord::dangerous::{RawResponse, RawResponseRequest};
use concord::error::ErrorCategory;
use concord::http::SafeProxy;
use concord::{ApiResponse, Bytes};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};

static CONTENT_TYPE_VALIDATIONS: AtomicUsize = AtomicUsize::new(0);
static DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);

pub struct ObservedCodec;

impl DecodeBody for ObservedCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-observed-codec");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        CONTENT_TYPE_VALIDATIONS.fetch_add(1, Ordering::SeqCst);
        if content_type == Self::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new("observed codec media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("observed codec utf8"))
    }
}

api! {
    client RawSuccessApi {
        base "http://raw-success.example.test/root/";
    }

    GET text_value() -> Text { path ["text"]; }
    GET custom_value() -> crate::ObservedCodec { path ["custom"]; }
    GET bytes_value() -> Bytes { path ["bytes"]; }
    GET empty_value() -> NoContent { path ["empty"]; }
    GET stream_value() -> Stream<OctetStream> { path ["stream"]; }
}

api! {
    client RawJsonApi {
        base "http://raw-json.example.test/";
    }

    GET json_value() -> crate::support::Json<Vec<u32>>;
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("loopback proxy")
}

fn client(server: &ScriptedServer, response_limit: usize) -> RawSuccessApi {
    RawSuccessApi::builder()
        .configure_http({
            let proxy = proxy(server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| {
            runtime.response_body_limit(
                NonZeroUsize::new(response_limit).expect("nonzero response limit"),
            )
        })
        .build()
        .expect("raw-success client")
}

#[test]
fn eligible_generated_modes_expose_the_exact_raw_terminal_type() {
    let client = RawSuccessApi::new().expect("side-effect-free client construction");

    let _: RawResponseRequest<_> = client.text_value().raw_response();
    let _: RawResponseRequest<_> = client.custom_value().raw_response();
    let _: RawResponseRequest<_> = client.bytes_value().raw_response();
    let _: RawResponseRequest<_> = client.empty_value().raw_response();

    let _direct_await_remains_selected = client.bytes_value();
    let _metadata_terminal_remains_selected = client.bytes_value().response();
}

#[test]
fn generated_json_mode_exposes_the_raw_terminal() {
    let client = RawJsonApi::new().expect("side-effect-free JSON client construction");
    let _: RawResponseRequest<_> = client.json_value().raw_response();
}

#[tokio::test]
async fn raw_success_skips_decoder_media_validation_and_preserves_exact_metadata() {
    CONTENT_TYPE_VALIDATIONS.store(0, Ordering::SeqCst);
    DECODER_CALLS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        ResponseScript::ok(b"raw-body-secret-618ae4".to_vec())
            .with_header("content-type", "application/x-intentionally-different")
            .with_header("x-raw-secret", "raw-header-secret-9e672c"),
        ResponseScript::ok(b"ordinary-direct".to_vec())
            .with_header("content-type", "application/x-observed-codec"),
        ResponseScript::ok(b"ordinary-metadata".to_vec())
            .with_header("content-type", "application/x-observed-codec"),
        ResponseScript::ok(b"no-content-still-collected".to_vec()),
    ])
    .expect("raw-success server");
    let client = client(&server, 1024);

    let raw: RawResponse = client
        .custom_value()
        .raw_response()
        .await
        .expect("valid raw success must not use the declared decoder");
    assert_eq!(CONTENT_TYPE_VALIDATIONS.load(Ordering::SeqCst), 0);
    assert_eq!(DECODER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(
        raw.url().as_str(),
        "http://raw-success.example.test/root/custom"
    );
    assert_eq!(raw.status(), 200);
    assert_eq!(raw.version(), concord::http::Version::HTTP_11);
    assert_eq!(raw.headers()["x-raw-secret"], "raw-header-secret-9e672c");
    assert_eq!(raw.metadata().endpoint(), "raw_success_api.custom_value");
    assert_eq!(raw.metadata().method(), concord::http::Method::GET);
    assert_eq!(raw.metadata().page_index(), 0);
    assert!(!raw.metadata().recovered_authentication());
    assert_eq!(raw.body(), b"raw-body-secret-618ae4".as_slice());

    let debug = format!("{raw:?}");
    for forbidden in [
        "raw-body-secret-618ae4",
        "raw-header-secret-9e672c",
        "raw-success.example.test",
        "root/custom",
    ] {
        assert!(
            !debug.contains(forbidden),
            "RawResponse Debug leaked {forbidden}"
        );
    }
    for expected in ["RawResponse", "200", "body_len", "22", "ExecutionMeta"] {
        assert!(
            debug.contains(expected),
            "RawResponse Debug omitted {expected}"
        );
    }
    assert_eq!(raw.into_body(), b"raw-body-secret-618ae4".as_slice());

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
        .raw_response()
        .await
        .expect("raw no-content success");
    assert_eq!(no_content.body(), b"no-content-still-collected".as_slice());
}

#[tokio::test]
async fn raw_success_uses_known_length_preflight_and_never_returns_partial_excess() {
    let known_gate = BodyGate::closed();
    let known_server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"known-excess-secret".to_vec(),
        gate: known_gate.clone(),
    }])
    .expect("known-length server");
    let known_result = tokio::time::timeout(
        Duration::from_secs(1),
        client(&known_server, 4).bytes_value().raw_response(),
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
        .raw_response()
        .await
        .expect_err("first cumulative excess returns no partial RawResponse");
    assert_eq!(unknown_error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{unknown_error:?} {unknown_error}").contains("first-excess-secret"));
}

#[tokio::test]
async fn non_success_remains_the_ordinary_typed_status_error() {
    let server = ScriptedServer::start([ResponseScript::status(
        422,
        b"raw-terminal-status-secret".to_vec(),
    )])
    .expect("status server");
    let error = client(&server, 1024)
        .bytes_value()
        .raw_response()
        .await
        .expect_err("non-success cannot produce RawResponse");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(!format!("{error:?} {error}").contains("raw-terminal-status-secret"));
}
