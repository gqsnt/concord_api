//! Runtime body/media through a declared body: exact wire values, the precedence table of
//! `EncodedBody`, zero polls during preparation, one-shot sources, and a second dynamic-media
//! codec (a generated multipart boundary) besides CSV.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use concord::csv::{CsvOptions, CsvWriter, HeaderMode, StreamRows};
use concord::reqwest::header::HeaderValue;
use concord::{CodecError, Encode, EncodedBody, ErrorKind};
use concord_check_phase_a::{Reports, ReportsConfig, Row};
use concord_example_testserver::{Reply, TestServer};

fn api(server: &TestServer) -> Reports {
    Reports::new(
        concord::reqwest::Client::new(),
        ReportsConfig {
            token: "Bearer t".to_owned(),
        },
    )
    .with_base_url(&format!("{}/v1", server.url()))
    .unwrap()
}

async fn ok_server() -> TestServer {
    TestServer::start(|_| Reply::json(200, "{\"id\":1,\"amount\":1}")).await
}

/// A one-shot, non-`Clone` row source that counts how often it is polled.
struct Counted {
    rows: std::vec::IntoIter<Row>,
    polls: Arc<AtomicUsize>,
}

impl futures_util::Stream for Counted {
    type Item = Result<Row, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(self.rows.next().map(Ok))
    }
}

fn upload(header: HeaderMode, delimiter: u8, polls: &Arc<AtomicUsize>) -> concord::stream::Upload {
    let writer = CsvWriter::new(CsvOptions::new(header, delimiter)).unwrap();
    writer
        .upload(StreamRows::new(Counted {
            rows: vec![Row { id: 1, amount: 10 }, Row { id: 2, amount: 20 }].into_iter(),
            polls: Arc::clone(polls),
        }))
        .unwrap()
}

#[tokio::test]
async fn the_runtime_media_type_travels_with_the_declared_body_and_preparation_polls_nothing() {
    let server = ok_server().await;
    let polls = Arc::new(AtomicUsize::new(0));
    let prepared = api(&server)
        .upload("j/1", upload(HeaderMode::Absent, b';', &polls))
        .prepare()
        .unwrap();
    assert_eq!(
        polls.load(Ordering::SeqCst),
        0,
        "preparation polls the source zero times"
    );
    assert!(format!("{prepared:?}").contains("text/csv; header=absent"));
    prepared.send().await.unwrap();
    assert!(polls.load(Ordering::SeqCst) > 0);
    let request = server.single();
    assert_eq!(request.target, "/v1/jobs/j%2F1/export");
    assert_eq!(
        request.header_all("content-type"),
        ["text/csv; header=absent"]
    );
    assert_eq!(request.body_text(), "1;10\r\n2;20\r\n");
    assert!(
        request.chunked,
        "a lazy source of unknown length is sent chunked"
    );
}

#[tokio::test]
async fn a_declared_media_type_must_equal_the_runtime_one() {
    let server = ok_server().await;
    let polls = Arc::new(AtomicUsize::new(0));
    // Equal byte for byte: accepted.
    api(&server)
        .upload_declared("a", upload(HeaderMode::Present, b',', &polls))
        .send()
        .await
        .unwrap();
    assert_eq!(
        server.single().header("content-type"),
        Some("text/csv; header=present")
    );
    // Different: refused before any I/O, nothing sent, the source never polled.
    let polls = Arc::new(AtomicUsize::new(0));
    let error = api(&server)
        .upload_declared("b", upload(HeaderMode::Absent, b',', &polls))
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(error.to_string().contains("content-type"), "{error}");
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_content_type_header_must_agree_with_the_runtime_media_type() {
    let server = ok_server().await;
    let polls = Arc::new(AtomicUsize::new(0));
    api(&server)
        .upload_header(
            "a",
            "text/csv; header=present",
            upload(HeaderMode::Present, b',', &polls),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        server.single().header_all("content-type"),
        ["text/csv; header=present"],
        "an equal header is sent once"
    );
    for contradicting in ["text/plain", "text/csv;header=present"] {
        let error = api(&server)
            .upload_header(
                "b",
                contradicting,
                upload(HeaderMode::Present, b',', &polls),
            )
            .prepare()
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Build, "{contradicting}");
    }
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn removing_an_inherited_content_type_keeps_the_runtime_one() {
    let server = ok_server().await;
    let polls = Arc::new(AtomicUsize::new(0));
    api(&server)
        .legacy()
        .upload("a", upload(HeaderMode::Present, b';', &polls))
        .send()
        .await
        .unwrap();
    assert_eq!(
        server.single().header_all("content-type"),
        ["text/csv; header=present"]
    );
}

#[tokio::test]
async fn an_absent_optional_body_sends_no_media_type() {
    let server = ok_server().await;
    api(&server).attach("a").send().await.unwrap();
    let request = server.single();
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.body_len, 0);
    // Present, without a runtime media type: no Content-Type either (the format has none).
    api(&server)
        .attach("b")
        .set_attachment(EncodedBody::new(concord::reqwest::Body::from("raw")))
        .send()
        .await
        .unwrap();
    let request = &server.requests()[1];
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.body_text(), "raw");
}

/// A second dynamic-media codec: `multipart/form-data` whose boundary is generated per body at
/// encode time, with a known length (sent with `Content-Length`, not chunked).
struct Multipart;

/// Named text fields and one file part.
struct Form {
    fields: Vec<(&'static str, String)>,
    file: (&'static str, &'static str, Vec<u8>),
}

static BOUNDARIES: AtomicUsize = AtomicUsize::new(0);

impl Encode<Form> for Multipart {
    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(_: Form) -> Result<concord::reqwest::Body, CodecError> {
        Err(CodecError::new("a multipart body has a runtime media type"))
    }

    fn encode_body(form: Form) -> Result<EncodedBody, CodecError> {
        let boundary = format!("concord-{:08}", BOUNDARIES.fetch_add(1, Ordering::SeqCst));
        let mut out = Vec::new();
        for (name, value) in &form.fields {
            out.extend_from_slice(
                format!("--{boundary}\r\ncontent-disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes(),
            );
        }
        let (name, filename, bytes) = &form.file;
        out.extend_from_slice(
            format!("--{boundary}\r\ncontent-disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\ncontent-type: application/octet-stream\r\n\r\n").as_bytes(),
        );
        out.extend_from_slice(bytes);
        out.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let media = HeaderValue::try_from(format!("multipart/form-data; boundary={boundary}"))
            .map_err(|_| CodecError::new("the boundary is not a header value"))?;
        Ok(EncodedBody::with_content_type(out.into(), media))
    }
}

concord::api! {
    pub client Store {
        base "http://store.invalid";
        POST put(body form: Form) -> Row { path "files"; request Multipart; response concord::Json; }
        POST put_declared(body form: Form) -> Row {
            path "files";
            request Multipart "multipart/form-data";
            response concord::Json;
        }
    }
}

#[tokio::test]
async fn a_generated_multipart_boundary_is_bound_to_its_bytes() {
    let server = ok_server().await;
    let store = Store::new(concord::reqwest::Client::new())
        .with_base_url(&server.url())
        .unwrap();
    let form = || Form {
        fields: vec![("purpose", "batch".into())],
        file: ("file", "in.jsonl", b"{\"a\":1}\n--concord-\r\n".to_vec()),
    };
    store.put(form()).send().await.unwrap();
    let request = server.single();
    let media = request.header("content-type").unwrap().to_owned();
    let boundary = media
        .strip_prefix("multipart/form-data; boundary=")
        .unwrap();
    let body = request.body_text();
    assert!(body.starts_with(&format!("--{boundary}\r\n")), "{body}");
    assert!(body.ends_with(&format!("--{boundary}--\r\n")));
    assert!(
        body.contains("{\"a\":1}\n--concord-\r\n"),
        "file bytes verbatim"
    );
    assert!(
        !request.chunked,
        "a known length is sent with Content-Length"
    );
    assert_eq!(
        request
            .header("content-length")
            .unwrap()
            .parse::<u64>()
            .unwrap(),
        request.body_len
    );
    // A declared media type without the boundary contradicts the bytes: refused before I/O.
    let error = store.put_declared(form()).prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_eq!(server.requests().len(), 1);
}

/// A static custom encoder keeps its old precedence: a declared header wins over
/// `CONTENT_TYPE`, and only `encode` is implemented.
struct Legacy;

impl Encode<String> for Legacy {
    const CONTENT_TYPE: Option<&'static str> = Some("text/x-legacy");
    fn encode(value: String) -> Result<concord::reqwest::Body, CodecError> {
        Ok(value.into())
    }
}

concord::api! {
    pub client Old {
        base "http://old.invalid";
        POST plain(body text: String) { path "a"; request Legacy; }
        POST overridden(body text: String) {
            path "b";
            request Legacy;
            headers { "content-type" = "text/plain"; }
        }
    }
}

#[tokio::test]
async fn static_encoders_keep_their_precedence() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    let old = Old::new(concord::reqwest::Client::new())
        .with_base_url(&server.url())
        .unwrap();
    old.plain("x").send().await.unwrap();
    old.overridden("y").send().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests[0].header_all("content-type"), ["text/x-legacy"]);
    assert_eq!(requests[1].header_all("content-type"), ["text/plain"]);
}
