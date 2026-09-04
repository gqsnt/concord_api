use std::pin::Pin;

use concord::api;
use concord::body::{RequestRepresentation, RequestStream};
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::error::ErrorCategory;
use concord::http::{Method, SafeProxy};
use concord::response::{ByteStream, ResponseRepresentation, Streaming};
use concord::{ApiError, Binary, Bytes};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream as _;

api! {
    client AnyStreamApi {
        timeout 30s;
        base "http://any-stream.example.test/root/";


    POST upload(body body: ::concord::body::RequestStream via Binary) -> Bytes via Binary {
        path ["upload"];
    }

    POST exact_upload(body body: ::concord::body::RequestStream via Binary) -> Bytes via Binary {
        path ["exact-upload"];
    }

    GET missing_media() -> ::concord::response::ByteStream via Binary {
        path ["missing-media"];
    }

    GET malformed_media() -> ::concord::response::ByteStream via Binary {
        path ["malformed-media"];
    }

    GET arbitrary_media() -> ::concord::response::ByteStream via Binary {
        path ["arbitrary-media"];
    }

    GET empty() -> ::concord::response::ByteStream via Binary {
        path ["empty"];
    }

    GET exact() -> ::concord::response::ByteStream via Binary {
        path ["exact"];
    }

    GET exact_mismatch() -> ::concord::response::ByteStream via Binary {
        path ["exact-mismatch"];
    }
}}

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
fn binary_is_the_exact_public_request_and_response_stream_representation() {
    fn assert_request<S>()
    where
        S: RequestRepresentation<RequestStream>,
    {
    }

    fn assert_response<S>()
    where
        S: ResponseRepresentation<ByteStream, Driver = Streaming>,
    {
    }

    assert_request::<Binary>();
    assert_response::<Binary>();
    assert_eq!(
        <Binary as ResponseRepresentation<ByteStream>>::accept(),
        Some("application/octet-stream")
    );
}

#[tokio::test]
async fn generated_and_runtime_binary_streams_preserve_bounded_wire_semantics() {
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
        ResponseScript::ok(b"runtime-bare".to_vec())
            .with_header("content-type", "application/octet-stream"),
        ResponseScript::ok(b"exact-mismatch-secret".to_vec())
            .with_header("content-type", "text/plain"),
    ])
    .expect("any-stream delivery server");
    let client = client(&server);

    assert_eq!(
        client
            .upload(RequestStream::from_bytes(Bytes::from_static(
                b"bare-upload-body"
            )))
            .await
            .expect("bare stream upload"),
        Bytes::from_static(b"bare-upload-accepted")
    );
    assert_eq!(
        client
            .exact_upload(RequestStream::from_bytes(Bytes::from_static(
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
        client
            .malformed_media()
            .await
            .expect_err("malformed media")
            .category(),
        ErrorCategory::ResponseContract
    );
    assert_eq!(
        client
            .arbitrary_media()
            .await
            .expect_err("wrong media")
            .category(),
        ErrorCategory::ResponseContract
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
    let runtime = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.any_stream").expect("runtime stream endpoint ID"),
        Method::GET,
    )
    .path("runtime")
    .response::<Binary, ByteStream>()
    .build()
    .expect("runtime bare stream response");
    assert_eq!(
        collect(
            runtime
                .try_bind(&client)
                .expect("runtime binding")
                .await
                .expect("runtime bare stream")
        )
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
    assert_eq!(requests.len(), 9);
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/octet-stream")
    );
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
        requests[8].header("accept"),
        Some("application/octet-stream")
    );
}
