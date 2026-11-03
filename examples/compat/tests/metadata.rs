//! Loopback compatibility evidence for metadata.
mod support;
use concord::{Decode, ErrorKind, Json};
use concord_example_compat::{metadata, Document};
use support::{http, Reply, Server};

#[tokio::test]
async fn metadata_and_typed_bodies() {
    let server = Server::reply(Reply::json(200, r#"{"value":"typed"}"#).header("etag", "v7")).await;
    let (client, request, decoder) = metadata(http(), &server.url()).unwrap().into_parts();
    let response = client.execute(request).await.unwrap();
    assert_eq!(response.headers()["etag"], "v7");
    assert_eq!(decoder.decode(response).await.unwrap().value, "typed");
    assert_eq!(server.single().target, "/document");

    let server =
        Server::reply(Reply::json(409, r#"{"value":"conflict"}"#).header("etag", "v8")).await;
    let error = metadata(http(), &server.url())
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    let response = error.response().unwrap();
    assert_eq!(response.status().as_u16(), 409);
    assert_eq!(response.headers()["etag"], "v8");
    // The borrowed observation leaves the error available to consume.
    let response = error.into_response().unwrap();
    let document =
        <Json as Decode<Document>>::decode_body((), response.bytes().await.unwrap()).unwrap();
    assert_eq!(document.value, "conflict");
    assert_eq!(server.single().target, "/document");
}
