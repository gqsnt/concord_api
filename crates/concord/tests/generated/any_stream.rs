use std::pin::Pin;

use concord::api;
use concord::body::{RequestBodySpec, Stream, StreamBody};
use concord::codec::{AnyMedia, MediaType, OctetStream};
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::error::ErrorCategory;
use concord::http::{Method, SafeProxy};
use concord::response::{ByteStream, ResponseBodySpec, Streaming};
use concord::{ApiError, Bytes};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream as _;

pub struct VendorMedia;

impl MediaType for VendorMedia {
    const MEDIA_TYPE: &'static str = "application/vnd.concord.stream; version=1";
}

api! {
    client AnyStreamApi {
        base "http://any-stream.example.test/root/";
    }

    POST upload(#[body] body: Stream) -> Bytes {
        path ["upload"];
    }

    POST exact_upload(#[body] body: Stream<OctetStream>) -> Bytes {
        path ["exact-upload"];
    }

    GET missing_media() -> Stream {
        path ["missing-media"];
    }

    GET malformed_media() -> Stream {
        path ["malformed-media"];
    }

    GET arbitrary_media() -> Stream {
        path ["arbitrary-media"];
    }

    GET empty() -> Stream {
        path ["empty"];
    }

    GET exact() -> Stream<OctetStream> {
        path ["exact"];
    }

    GET vendor() -> Stream<crate::generated::any_stream::VendorMedia> {
        path ["vendor"];
    }

    GET exact_mismatch() -> Stream<OctetStream> {
        path ["exact-mismatch"];
    }
}

fn client(server: &ScriptedServer) -> AnyStreamApi {
    let proxy = SafeProxy::parse(server.url("")).expect("any-stream loopback proxy");
    AnyStreamApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("any-stream client")
}

async fn collect(mut stream: ByteStream) -> Result<Bytes, ApiError> {
    let mut collected = Vec::new();
    while let Some(chunk) =
        std::future::poll_fn(|context| Pin::new(&mut stream).poll_next(context)).await
    {
        collected.extend_from_slice(&chunk?);
    }
    Ok(Bytes::from(collected))
}

#[test]
fn default_and_explicit_any_media_streams_have_the_same_public_contract() {
    fn assert_request<S>()
    where
        S: RequestBodySpec<Input = StreamBody>,
    {
    }

    fn assert_response<S>()
    where
        S: ResponseBodySpec<Output = ByteStream, Mode = Streaming>,
    {
    }

    assert_request::<Stream>();
    assert_request::<Stream<AnyMedia>>();
    assert_request::<Stream<OctetStream>>();
    assert_response::<Stream>();
    assert_response::<Stream<AnyMedia>>();
    assert_response::<Stream<OctetStream>>();
    assert_eq!(<Stream as ResponseBodySpec>::accept(), None);
    assert_eq!(
        <Stream<OctetStream> as ResponseBodySpec>::accept(),
        Some("application/octet-stream")
    );
}

#[tokio::test]
async fn generated_and_runtime_streams_preserve_bare_and_exact_wire_semantics() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"bare-upload-accepted".to_vec()),
        ResponseScript::ok(b"exact-upload-accepted".to_vec()),
        ResponseScript::ok(b"missing-media".to_vec()),
        ResponseScript::ok(b"malformed-media".to_vec())
            .with_header("content-type", "not a media type"),
        ResponseScript::ok(b"arbitrary-media".to_vec()).with_header("content-type", "text/plain"),
        ResponseScript::ok(Vec::new()),
        ResponseScript::ok(b"exact-media".to_vec())
            .with_header("content-type", "application/octet-stream; version=1"),
        ResponseScript::ok(b"vendor-media".to_vec()).with_header(
            "content-type",
            "application/vnd.concord.stream; charset=utf-8; version=1",
        ),
        ResponseScript::ok(b"runtime-bare".to_vec())
            .with_header("content-type", "not a media type"),
        ResponseScript::ok(b"exact-mismatch-secret".to_vec())
            .with_header("content-type", "text/plain"),
    ])
    .expect("any-stream delivery server");
    let client = client(&server);

    assert_eq!(
        client
            .upload(StreamBody::from_bytes(Bytes::from_static(
                b"bare-upload-body"
            )))
            .await
            .expect("bare stream upload"),
        Bytes::from_static(b"bare-upload-accepted")
    );
    assert_eq!(
        client
            .exact_upload(StreamBody::from_bytes(Bytes::from_static(
                b"exact-upload-body"
            )))
            .await
            .expect("exact stream upload"),
        Bytes::from_static(b"exact-upload-accepted")
    );

    assert_eq!(
        collect(client.missing_media().await.expect("missing media stream"))
            .await
            .expect("missing media body"),
        Bytes::from_static(b"missing-media")
    );
    assert_eq!(
        collect(
            client
                .malformed_media()
                .await
                .expect("malformed media stream"),
        )
        .await
        .expect("malformed media body"),
        Bytes::from_static(b"malformed-media")
    );
    assert_eq!(
        collect(
            client
                .arbitrary_media()
                .await
                .expect("arbitrary media stream"),
        )
        .await
        .expect("arbitrary media body"),
        Bytes::from_static(b"arbitrary-media")
    );
    assert!(
        collect(client.empty().await.expect("empty bare stream"))
            .await
            .expect("empty bare body")
            .is_empty()
    );
    assert_eq!(
        collect(client.exact().await.expect("exact stream"))
            .await
            .expect("exact body"),
        Bytes::from_static(b"exact-media")
    );
    assert_eq!(
        collect(client.vendor().await.expect("vendor stream"))
            .await
            .expect("vendor body"),
        Bytes::from_static(b"vendor-media")
    );

    let runtime = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.any_stream").expect("runtime stream endpoint ID"),
        Method::GET,
    )
    .path_segment("runtime")
    .response::<Stream>()
    .build()
    .expect("runtime bare stream response");
    assert_eq!(
        collect(client.request(runtime).await.expect("runtime bare stream"))
            .await
            .expect("runtime bare body"),
        Bytes::from_static(b"runtime-bare")
    );

    let error = client
        .exact_mismatch()
        .await
        .expect_err("exact stream rejects mismatched media");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert!(!format!("{error:?} {error}").contains("exact-mismatch-secret"));

    let requests = server.requests();
    assert_eq!(requests.len(), 10);
    assert_eq!(requests[0].header("content-type"), None);
    assert_eq!(requests[0].body(), b"bare-upload-body");
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(requests[1].body(), b"exact-upload-body");
    assert_eq!(
        requests[6].header("accept"),
        Some("application/octet-stream")
    );
    assert_eq!(
        requests[7].header("accept"),
        Some("application/vnd.concord.stream; version=1")
    );
    assert_eq!(
        requests[9].header("accept"),
        Some("application/octet-stream")
    );
}
