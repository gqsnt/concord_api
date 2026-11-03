//! U29 (R09): the multipart recipe on the wire, read back by an independent parser.

mod support;

use concord::ErrorKind;
use concord_example_multipart::{FileUpload, Files, FilesConfig, Stored, file_form, upload};
use reqwest::multipart::{Form, Part};
use support::{Server, boundary_of, parse_multipart};

fn files_at(server: &Server, key: &str) -> Files {
    Files::new(
        reqwest::Client::new(),
        FilesConfig {
            key: key.to_owned(),
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

const STORED: support::Reply = Some((201, "application/json", r#"{"id":7,"files":2}"#));

/// Sends `form` and returns what the server received, parsed independently.
async fn round_trip(form: Form) -> (support::Recorded, Vec<support::ParsedPart>) {
    let server = Server::reply(STORED).await;
    let stored = upload(&files_at(&server, "Bearer t"), "inbox", form)
        .await
        .unwrap();
    assert_eq!(stored, Stored { id: 7, files: 2 });
    let request = server.single();
    let boundary = boundary_of(request.header("content-type").unwrap());
    let parts = parse_multipart(&request.body, &boundary);
    (request, parts)
}

/// Declared path, authorization and decoder; native body with several parts, in order.
#[tokio::test]
async fn several_parts_keep_declaration_and_order() {
    let server = Server::reply(STORED).await;
    let form = file_form(
        &[("note", "quarterly stock"), ("tag", "a"), ("tag", "b")],
        vec![
            file("report", "q3.csv", "text/csv", b"sku,qty\r\nA-1,5\r\n"),
            file(
                "image",
                "logo.png",
                "image/png",
                &[0x89, b'P', b'N', b'G', 0, 0xff],
            ),
        ],
    )
    .unwrap();
    let stored = upload(&files_at(&server, "Bearer secret-token"), "a b/c", form)
        .await
        .unwrap();
    assert_eq!(stored, Stored { id: 7, files: 2 });

    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/v1/folders/a%20b%2Fc/files")
    );
    assert_eq!(request.header("authorization"), Some("Bearer secret-token"));
    assert_eq!(
        request.header("content-length"),
        Some(request.body.len().to_string().as_str())
    );
    let parts = parse_multipart(
        &request.body,
        &boundary_of(request.header("content-type").unwrap()),
    );
    let summary: Vec<_> = parts
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.file_name.as_deref(),
                p.content_type.as_deref(),
                p.content.as_slice(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("note", None, None, &b"quarterly stock"[..]),
            ("tag", None, None, b"a"),
            ("tag", None, None, b"b"),
            (
                "report",
                Some("q3.csv"),
                Some("text/csv"),
                b"sku,qty\r\nA-1,5\r\n"
            ),
            (
                "image",
                Some("logo.png"),
                Some("image/png"),
                &[0x89, b'P', b'N', b'G', 0, 0xff]
            ),
        ]
    );
}

/// Field names: plain names stay `name="..."`; any other name is an RFC 8187 extended value
/// that decodes to the original (`"`, `\`, CR/LF, space, `/`, `%`, non-ASCII).
#[tokio::test]
async fn field_names_round_trip() {
    let names = [
        "file",
        "a-b_c.d",
        "say \"hi\"",
        r"file\name",
        r"trail\",
        "line\r\nbreak",
        "a/b%20c",
        "fichier-é-数据",
    ];
    let files = names
        .iter()
        .map(|n| file(n, "x.txt", "text/plain", b"x"))
        .collect();
    let (_, parts) = round_trip(file_form(&[], files).unwrap()).await;
    let got: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(got, names);
    assert!(parts[0].disposition.starts_with("form-data; name=\"file\""));
    assert!(
        parts[1]
            .disposition
            .starts_with("form-data; name=\"a-b_c.d\"")
    );
    assert!(
        parts[3]
            .disposition
            .starts_with("form-data; name*=utf-8''file%5Cname")
    );
}

/// File names: `"` and `\` are backslash-escaped, non-ASCII is raw UTF-8, CR/LF are escaped
/// so they never form a line break; all decode to the original.
#[tokio::test]
async fn file_names_round_trip() {
    let names = [
        "report.csv",
        "report \"q3\".csv",
        r"C:\temp\x.csv",
        r"ends\",
        "two\r\nlines\n.txt",
        "résumé 数据.pdf",
    ];
    let files = names
        .iter()
        .map(|n| file("f", n, "application/octet-stream", b"x"))
        .collect();
    let (request, parts) = round_trip(file_form(&[], files).unwrap()).await;
    let got: Vec<&str> = parts
        .iter()
        .filter_map(|p| p.file_name.as_deref())
        .collect();
    assert_eq!(got, names);
    // Wire form of the CR/LF name: a backslash before each raw CR and LF, so no CRLF pair.
    assert!(
        parts[4]
            .disposition
            .ends_with("filename=\"two\\\r\\\nlines\\\n.txt\"")
    );
    // The part header block is still exactly two lines for every part.
    let text = String::from_utf8_lossy(&request.body);
    assert_eq!(
        text.matches("Content-Type: application/octet-stream")
            .count(),
        6
    );
}

/// Content is opaque bytes: every byte value, a boundary-shaped line and the delimiter prefix
/// survive, and the real boundary does not occur inside any part.
#[tokio::test]
async fn bytes_survive_including_boundary_lookalikes() {
    let all: Vec<u8> = (0..=255).collect();
    let lookalike =
        b"\r\n--0000000000000000-0000000000000000-0000000000000000-0000000000000000--\r\n\
                      --\r\n\r\n--";
    let (request, parts) = round_trip(
        file_form(
            &[],
            vec![
                file("all", "all.bin", "application/octet-stream", &all),
                file("look", "look.bin", "application/octet-stream", lookalike),
                file("empty", "empty.bin", "application/octet-stream", b""),
            ],
        )
        .unwrap(),
    )
    .await;
    assert_eq!(parts[0].content, all);
    assert_eq!(parts[1].content, lookalike);
    assert_eq!(parts[2].content, b"");
    let boundary = boundary_of(request.header("content-type").unwrap());
    for p in &parts {
        assert!(
            !p.content
                .windows(boundary.len())
                .any(|w| w == boundary.as_bytes())
        );
    }
}

/// The part content type is sent as given; an invalid media type fails before any I/O.
#[tokio::test]
async fn part_content_type() {
    let (_, parts) = round_trip(
        file_form(
            &[],
            vec![
                file("a", "a.json", "application/json; charset=utf-8", b"{}"),
                file("b", "b.svg", "image/svg+xml", b"<svg/>"),
            ],
        )
        .unwrap(),
    )
    .await;
    assert_eq!(
        parts[0].content_type.as_deref(),
        Some("application/json; charset=utf-8")
    );
    assert_eq!(parts[1].content_type.as_deref(), Some("image/svg+xml"));
    assert!(file_form(&[], vec![file("a", "a", "not a media type", b"")]).is_err());
}

/// reqwest's own parts work unchanged with the recipe (e.g. a part without a file name).
#[tokio::test]
async fn native_parts_compose() {
    let form = Form::new()
        .percent_encode_attr_chars()
        .part(
            "meta",
            Part::text("{}").mime_str("application/json").unwrap(),
        )
        .part("raw", Part::bytes(&b"\0\x01"[..]));
    let (_, parts) = round_trip(form).await;
    assert_eq!(parts[0].content_type.as_deref(), Some("application/json"));
    assert_eq!(parts[1].file_name, None);
    assert_eq!(parts[1].content, b"\0\x01");
}

/// Why the recipe opts into attr-char encoding: reqwest's default leaves `\` raw inside
/// `name="..."`, where a quoted-string reader treats it as an escape.
#[tokio::test]
async fn default_name_encoding_is_ambiguous_for_backslashes() {
    let form = Form::new().part("file\\name", Part::bytes(&b"x"[..]));
    let (_, parts) = round_trip(form).await;
    assert!(
        parts[0]
            .disposition
            .starts_with("form-data; name=\"file\\name\"")
    );
    assert_eq!(parts[0].name, "filename");
}

/// Non-2xx: a `Status` error whose response is still unread and available.
#[tokio::test]
async fn status_error_keeps_the_unread_response() {
    let server = Server::reply(Some((413, "text/plain", "too large"))).await;
    let form = file_form(
        &[],
        vec![file("f", "a.bin", "application/octet-stream", b"x")],
    )
    .unwrap();
    let error = upload(&files_at(&server, "Bearer t"), "inbox", form)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(413));
    assert_eq!(error.endpoint(), Some("Files.upload"));
    let response = error.into_response().unwrap();
    assert_eq!(response.text().await.unwrap(), "too large");
    // The body was sent in full before the server answered.
    let request = server.single();
    let parts = parse_multipart(
        &request.body,
        &boundary_of(request.header("content-type").unwrap()),
    );
    assert_eq!(parts[0].content, b"x");
}

/// A 2xx with an undecodable body is a `Decode` error from the declared `Json`.
#[tokio::test]
async fn decode_error_from_the_declared_representation() {
    let server = Server::reply(Some((200, "application/json", "{\"id\":"))).await;
    let form = file_form(&[("a", "b")], vec![]).unwrap();
    let error = upload(&files_at(&server, "Bearer t"), "inbox", form)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
}

/// No answer: `finish` turns reqwest's error into a `Transport` error.
#[tokio::test]
async fn transport_failure_through_finish() {
    let server = Server::reply(None).await;
    let form = file_form(
        &[],
        vec![file("f", "a.bin", "application/octet-stream", b"x")],
    )
    .unwrap();
    let error = upload(&files_at(&server, "Bearer t"), "inbox", form)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.transport().is_some());
    assert_eq!(server.single().method, "POST");
}

/// A build error in the declared part is reported before any multipart work or I/O.
#[tokio::test]
async fn build_error_before_io() {
    let server = Server::reply(STORED).await;
    let api = Files::new(
        reqwest::Client::new(),
        FilesConfig {
            key: "bad\nkey".to_owned(),
        },
    )
    .with_base_url(format!("http://{}/v1", server.addr))
    .unwrap();
    let error = upload(&api, "inbox", Form::new()).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
}
