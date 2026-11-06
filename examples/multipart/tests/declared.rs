//! The declared multipart body (`request Multipart;`) on the wire: the generated `send()`, the
//! exact bytes and headers, read back by the same independent parser as the native recipe, and
//! the runtime media type checked against the declaration before any I/O.

mod support;

use concord::{ErrorKind, Json};
use concord_example_multipart::{FileUpload, Files, FilesConfig, Multipart, Stored, UploadForm};
use support::{Server, boundary_of, parse_multipart};

const STORED: support::Reply = Some((201, "application/json", r#"{"id":7,"files":1}"#));

fn files_at(server: &Server) -> Files {
    Files::new(
        reqwest::Client::new(),
        FilesConfig {
            key: "Bearer secret-token".to_owned(),
        },
    )
    .with_base_url(format!("http://{}/v1", server.addr))
    .unwrap()
}

fn file(field: &str, file_name: &str, content_type: &str, content: &[u8]) -> FileUpload {
    FileUpload {
        field: field.to_owned(),
        file_name: file_name.to_owned(),
        content_type: content_type.to_owned(),
        content: content.to_vec(),
    }
}

/// File content that looks like multipart framing, including this format's own boundary prefix.
const LOOKALIKE: &[u8] =
    b"sku,qty\r\n--concord-0000000000000000\r\n\r\n--\r\n--concord-\r\n--\0\xff";

#[tokio::test]
async fn the_declared_body_is_sent_with_its_own_boundary_and_length() {
    let server = Server::reply(STORED).await;
    let form = UploadForm::new()
        .text("note", "quarterly stock")
        .file(file("report", "q3.csv", "text/csv", LOOKALIKE));
    let stored = files_at(&server)
        .upload_form("a b/c", form)
        .send()
        .await
        .unwrap();
    assert_eq!(stored, Stored { id: 7, files: 1 });

    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/v1/folders/a%20b%2Fc/files")
    );
    assert_eq!(request.header("authorization"), Some("Bearer secret-token"));
    assert_eq!(request.header("accept"), Some("application/json"));
    // One Content-Type (`header` asserts it occurs at most once), naming the body's boundary.
    let content_type = request.header("content-type").unwrap();
    let boundary = boundary_of(content_type);
    assert_eq!(
        content_type,
        format!("multipart/form-data; boundary={boundary}")
    );
    assert!(
        boundary.starts_with("concord-") && boundary.len() == 40,
        "{boundary}"
    );
    // A known length: Content-Length, not chunked.
    assert_eq!(request.header("transfer-encoding"), None);
    assert_eq!(
        request.header("content-length"),
        Some(request.body.len().to_string().as_str())
    );

    // The exact bytes.
    let mut expected = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"note\"\r\n\
         \r\n\
         quarterly stock\r\n\
         --{boundary}\r\n\
         Content-Disposition: form-data; name=\"report\"; filename=\"q3.csv\"\r\n\
         Content-Type: text/csv\r\n\
         \r\n"
    )
    .into_bytes();
    expected.extend_from_slice(LOOKALIKE);
    expected.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    assert_eq!(request.body, expected);

    // An independent reader finds the same parts, the look-alike bytes verbatim.
    let parts = parse_multipart(&request.body, &boundary);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].name, "note");
    assert_eq!(parts[0].content, b"quarterly stock");
    assert_eq!(parts[1].name, "report");
    assert_eq!(parts[1].file_name.as_deref(), Some("q3.csv"));
    assert_eq!(parts[1].content_type.as_deref(), Some("text/csv"));
    assert_eq!(parts[1].content, LOOKALIKE);
    assert_eq!(
        request
            .body
            .windows(boundary.len())
            .filter(|w| *w == boundary.as_bytes())
            .count(),
        3,
        "the boundary occurs only in the three delimiters"
    );
}

#[tokio::test]
async fn every_body_gets_a_fresh_boundary() {
    let server = Server::reply(STORED).await;
    let api = files_at(&server);
    let mut boundaries = Vec::new();
    for _ in 0..2 {
        let form = UploadForm::new().file(file("f", "a.bin", "application/octet-stream", b"x"));
        let prepared = api.upload_form("inbox", form).prepare().unwrap();
        let content_type = prepared.request().headers()["content-type"]
            .to_str()
            .unwrap();
        boundaries.push(boundary_of(content_type));
    }
    assert_ne!(boundaries[0], boundaries[1]);
    assert!(server.requests().is_empty(), "preparing sends nothing");
}

#[tokio::test]
async fn names_and_file_names_round_trip_and_unwritable_names_are_refused() {
    let server = Server::reply(STORED).await;
    let names = [
        "file",
        "say \"hi\"",
        r"file\name",
        "line\r\nbreak",
        "fichier-é-数据",
    ];
    let file_names = [
        "report.csv",
        "report \"q3\".csv",
        r"C:\temp\x.csv",
        r"ends\",
        "résumé 数据.pdf",
    ];
    let mut form = UploadForm::new();
    for (name, file_name) in names.iter().zip(file_names) {
        form = form.file(file(name, file_name, "application/octet-stream", b"x"));
    }
    files_at(&server)
        .upload_form("inbox", form)
        .send()
        .await
        .unwrap();
    let request = server.single();
    let parts = parse_multipart(
        &request.body,
        &boundary_of(request.header("content-type").unwrap()),
    );
    let got: Vec<_> = parts
        .iter()
        .map(|p| (p.name.as_str(), p.file_name.as_deref().unwrap()))
        .collect();
    assert_eq!(
        got,
        names.iter().copied().zip(file_names).collect::<Vec<_>>()
    );
    assert!(
        parts[0]
            .disposition
            .starts_with("form-data; name=\"file\";")
    );
    assert!(
        parts[2]
            .disposition
            .starts_with("form-data; name*=utf-8''file%5Cname;")
    );

    // A file name with CR/LF cannot be a quoted string, a media type with CR/LF cannot be a
    // header: refused while preparing, nothing sent.
    for bad in [
        file("f", "two\r\nlines.txt", "text/plain", b"x"),
        file("f", "ok.txt", "text/plain\r\nX-Injected: 1", b"x"),
    ] {
        let error = files_at(&server)
            .upload_form("inbox", UploadForm::new().file(bad))
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Build);
    }
    assert_eq!(server.requests().len(), 1);
}

concord::api! {
    /// The same upload with declarations that contradict a per-body boundary.
    pub client Strict {
        base "http://files.invalid/v1";

        /// A declared media type without the boundary.
        POST declared(body form: UploadForm) -> Stored {
            path "files";
            request Multipart "multipart/form-data";
            response Json;
        }

        /// A `content-type` header with a fixed boundary.
        POST fixed(body form: UploadForm) -> Stored {
            path "files";
            request Multipart;
            response Json;
            headers { "content-type" = "multipart/form-data; boundary=fixed"; }
        }
    }
}

#[tokio::test]
async fn a_contradicting_declared_media_type_is_a_build_error_before_io() {
    let server = Server::reply(STORED).await;
    let strict = Strict::new(reqwest::Client::new())
        .with_base_url(format!("http://{}/v1", server.addr))
        .unwrap();
    let form = || UploadForm::new().file(file("f", "a.bin", "application/octet-stream", b"x"));

    let error = strict.declared(form()).prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_eq!(error.endpoint(), Some("Strict.declared"));
    assert!(error.to_string().contains("content-type"), "{error}");
    let error = strict.declared(form()).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);

    let error = strict.fixed(form()).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_eq!(error.endpoint(), Some("Strict.fixed"));

    assert!(server.requests().is_empty(), "nothing was sent");
}
