//! A `HEAD` answer declares the length of the representation without sending it.

use concord_example_testserver::{Body, Reply, TestServer};

#[tokio::test]
async fn a_head_answer_declares_a_length_and_keeps_the_connection() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("etag", "\"e1\"")
            .with_body(Body::HeadOnly(1234))
    })
    .await;
    let client = reqwest::Client::new();
    for _ in 0..2 {
        let response = client.head(server.url()).send().await.unwrap();
        assert_eq!(response.headers()["content-length"], "1234");
        assert_eq!(response.headers()["etag"], "\"e1\"");
        assert!(response.bytes().await.unwrap().is_empty());
    }
    assert_eq!(server.connections(), 1, "the connection is reused");
    assert_eq!(server.body_bytes_sent(), 0);
}
