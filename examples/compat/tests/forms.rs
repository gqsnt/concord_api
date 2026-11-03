//! Loopback compatibility evidence for forms.
mod support;
use concord::ErrorKind;
use concord_example_compat::{Flat, Forms, Nested, Order};
use support::{http, Reply, Server};

#[tokio::test]
async fn flat_nested_and_brackets() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Forms::new(http()).with_base_url(server.url()).unwrap();
    api.flat(Flat {
        title: "blue pen&ink".into(),
        order: 7,
    })
    .send()
    .await
    .unwrap();
    let nested = Nested {
        metadata: Order { order: 7 },
    };
    let error = api.nested(nested.clone()).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_eq!(
        error.to_string(),
        "POST Forms.nested: request not built: body: the value cannot be encoded as a form"
    );
    assert_eq!(
        error
            .codec_error()
            .and_then(concord::CodecError::source_unredacted)
            .unwrap()
            .to_string(),
        "unsupported value"
    );
    assert_eq!(server.requests().len(), 1); // nested form never reached the server
    api.brackets(nested).send().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests[0].body, b"title=blue+pen%26ink&order=7");
    assert_eq!(requests[1].body, b"metadata%5Border%5D=7");
    for r in &requests {
        assert_eq!(
            r.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
    }
}
