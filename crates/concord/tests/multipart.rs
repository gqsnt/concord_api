//! The `multipart` format (feature `multipart`) through declared endpoints, on the wire: the
//! generated `send()` with the exact body, its boundary and `Content-Length`; the runtime media
//! type against a declared media type and `content-type` headers (`EncodedBody`'s precedence:
//! a contradiction is a `Build` error before any I/O); the format's refusals as `Build` errors
//! that show no part bytes. Runs alone in the `--no-default-features --features multipart` lane.
#![deny(warnings)]

mod support;

use concord::multipart::{Multipart, MultipartBody, Part};
use concord::{ErrorKind, Text, reqwest};
use support::{Recorded, Reply, Server};

const SECRET: &str = "SECRET-7f3a91";

concord::api! {
    /// A file store with declared multipart bodies.
    pub client Files {
        base "http://files.invalid/v1";

        /// Upload into a folder.
        POST upload(folder: String, body form: MultipartBody) -> String {
            path "folders/{folder}/files";
            request Multipart;
            response Text;
        }
        /// An optional multipart body.
        POST maybe(body form?: MultipartBody) { path "maybe"; request Multipart; }
        /// A declared media type without the boundary: it contradicts every body.
        POST declared(body form: MultipartBody) {
            path "declared";
            request Multipart "multipart/form-data";
        }
        /// A declared media type naming the boundary of a body whose parts contain no candidate.
        POST declared_equal(body form: MultipartBody) {
            path "declared";
            request Multipart "multipart/form-data; boundary=concord-boundary-0";
        }
        /// A `content-type` header with a fixed boundary.
        POST fixed(body form: MultipartBody) {
            path "fixed";
            request Multipart;
            headers { "content-type" = "multipart/form-data; boundary=fixed"; }
        }
        /// A `content-type` header from a value.
        POST header_value(header("content-type") media: String, body form: MultipartBody) {
            path "header";
            request Multipart;
        }
        scope legacy {
            headers { "content-type" = "application/octet-stream"; }
            /// The inherited header is kept: it contradicts the runtime media type.
            POST inherited(body form: MultipartBody) { path "inherited"; request Multipart; }
            /// The inherited header is removed: the runtime media type is sent.
            POST removed(body form: MultipartBody) {
                path "removed";
                request Multipart;
                headers { remove "content-type"; }
            }
        }
    }
}

fn files(server: &Server) -> Files {
    Files::new(reqwest::Client::new())
        .with_base_url(format!("{}/v1", server.url()))
        .unwrap()
}

fn form() -> MultipartBody {
    MultipartBody::form_data().text("purpose", "batch")
}

/// The single `content-type` sent, and that the body went with `Content-Length`, not chunked.
fn sent_with_length(request: &Recorded) -> &str {
    assert_eq!(request.header("transfer-encoding"), None);
    assert_eq!(
        request.header("content-length"),
        Some(request.body.len().to_string().as_str())
    );
    request.header("content-type").expect("one content-type")
}

#[tokio::test]
async fn a_declared_body_is_sent_with_its_boundary_and_length() {
    let server = Server::reply(Reply::text(201, "stored")).await;
    // The file holds the first candidate as a would-be delimiter: the second one is chosen.
    let content: &[u8] = b"sku,qty\r\n--concord-boundary-0\r\n\x00\xff";
    let body = MultipartBody::form_data().text("note", "q3").file(
        "report",
        "q3.csv",
        "text/csv",
        content.to_vec(),
    );
    let stored = files(&server).upload("a b/c", body).send().await.unwrap();
    assert_eq!(stored, "stored");

    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/v1/folders/a%20b%2Fc/files")
    );
    assert_eq!(
        sent_with_length(&request),
        "multipart/form-data; boundary=concord-boundary-1"
    );
    let mut expected = b"--concord-boundary-1\r\n\
        Content-Disposition: form-data; name=\"note\"\r\n\
        \r\n\
        q3\r\n\
        --concord-boundary-1\r\n\
        Content-Disposition: form-data; name=\"report\"; filename=\"q3.csv\"\r\n\
        Content-Type: text/csv\r\n\
        \r\n"
        .to_vec();
    expected.extend_from_slice(content);
    expected.extend_from_slice(b"\r\n--concord-boundary-1--\r\n");
    assert_eq!(request.body, expected);
}

#[tokio::test]
async fn a_related_body_is_sent_with_its_type_parameter() {
    let server = Server::reply(Reply::new(204)).await;
    let body = MultipartBody::related_with_type("application/json")
        .part(Part::new("{}").header("Content-Type", "application/json; charset=UTF-8"))
        .part(Part::new("media").header("Content-Type", "application/octet-stream"));
    files(&server).maybe().set_form(body).send().await.unwrap();
    let request = server.single();
    assert_eq!(
        sent_with_length(&request),
        "multipart/related; boundary=concord-boundary-0; type=\"application/json\""
    );
    assert_eq!(
        request.body,
        b"--concord-boundary-0\r\n\
          Content-Type: application/json; charset=UTF-8\r\n\
          \r\n\
          {}\r\n\
          --concord-boundary-0\r\n\
          Content-Type: application/octet-stream\r\n\
          \r\n\
          media\r\n\
          --concord-boundary-0--\r\n"
    );
}

#[tokio::test]
async fn preparing_binds_the_media_type_and_sends_nothing() {
    let server = Server::reply(Reply::new(204)).await;
    let prepared = files(&server)
        .maybe()
        .set_form(form().text("token", SECRET))
        .prepare()
        .unwrap();
    assert_eq!(
        prepared.request().headers()["content-type"],
        "multipart/form-data; boundary=concord-boundary-0"
    );
    assert!(!format!("{prepared:?}").contains(SECRET));
    assert!(server.requests().is_empty(), "preparing sends nothing");
    // An absent optional body: no body and no media type.
    files(&server).maybe().send().await.unwrap();
    let request = server.single();
    assert!(request.body.is_empty());
    assert_eq!(request.header("content-type"), None);
}

#[tokio::test]
async fn a_declared_media_type_must_equal_the_runtime_one() {
    let server = Server::reply(Reply::new(204)).await;
    let api = files(&server);
    for error in [
        api.declared(form()).prepare().unwrap_err(),
        api.declared(form()).send().await.unwrap_err(),
        // Equal to candidate 0 only while no part contains it.
        api.declared_equal(form().text("x", "concord-boundary-0"))
            .send()
            .await
            .unwrap_err(),
    ] {
        assert_eq!(error.kind(), ErrorKind::Build);
        assert!(error.to_string().contains("content-type"), "{error}");
    }
    assert!(server.requests().is_empty(), "nothing was sent");

    api.declared_equal(form()).send().await.unwrap();
    assert_eq!(
        sent_with_length(&server.single()),
        "multipart/form-data; boundary=concord-boundary-0"
    );
}

#[tokio::test]
async fn a_content_type_header_must_agree_with_the_runtime_media_type() {
    let server = Server::reply(Reply::new(204)).await;
    let api = files(&server);
    for error in [
        api.fixed(form()).send().await.unwrap_err(),
        api.header_value("multipart/form-data".to_owned(), form())
            .send()
            .await
            .unwrap_err(),
        api.legacy().inherited(form()).send().await.unwrap_err(),
    ] {
        assert_eq!(error.kind(), ErrorKind::Build);
    }
    assert!(server.requests().is_empty(), "nothing was sent");

    api.header_value(
        "multipart/form-data; boundary=concord-boundary-0".to_owned(),
        form(),
    )
    .send()
    .await
    .unwrap();
    api.legacy().removed(form()).send().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        // Sent once (`header` asserts at most one line), equal to the runtime value.
        assert_eq!(
            sent_with_length(request),
            "multipart/form-data; boundary=concord-boundary-0"
        );
    }
}

#[tokio::test]
async fn refusals_are_build_errors_before_io_that_show_no_part_bytes() {
    let server = Server::reply(Reply::text(201, "stored")).await;
    let api = files(&server);
    let cases = [
        (
            form().file(format!("{SECRET}\r\n"), "a.txt", "text/plain", SECRET),
            "a multipart field name contains a control character, a quote or a backslash",
            Some(2),
        ),
        (
            form().file("f", format!("{SECRET}\".txt"), "text/plain", SECRET),
            "a multipart file name contains a control character, a quote or a backslash",
            Some(2),
        ),
        (
            form().file("f", "a.txt", format!("text/plain\r\nX-{SECRET}: 1"), SECRET),
            "a multipart part header value contains a control character",
            Some(2),
        ),
        (
            MultipartBody::related(),
            "a multipart/related body has no part",
            None,
        ),
    ];
    for (body, label, part) in cases {
        let error = api
            .upload("inbox".to_owned(), body)
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Build);
        assert_eq!(error.endpoint(), Some("Files.upload"));
        let codec = error.codec_error().expect("the format's error");
        assert_eq!(codec.label(), label);
        assert_eq!(codec.get("part"), part);
        for shown in [error.to_string(), format!("{error:?}")] {
            assert!(!shown.contains(SECRET), "{shown}");
        }
    }
    assert!(server.requests().is_empty(), "nothing was sent");
}
