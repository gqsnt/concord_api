#![allow(dead_code)]
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_mailgun::{Attachment, Domain, EventRange, Mailgun, MailgunDomain, TextEmail};
use std::{num::NonZeroUsize, time::Duration};

pub const ACCEPTED: &str = include_str!("../../fixtures/accepted.json");
pub const FIRST: &str = include_str!("../../fixtures/events-first.json");
pub const SECOND: &str = include_str!("../../fixtures/events-second.json");
pub const EMPTY: &str = include_str!("../../fixtures/events-empty.json");
pub const PROBLEM: &str = include_str!("../../fixtures/problem.json");
pub const MALFORMED: &str = include_str!("../../fixtures/malformed.json");
pub const TRUNCATED: &str = include_str!("../../fixtures/truncated.json");
pub const FILE: &[u8] = include_bytes!("../../fixtures/report.txt");
pub const KEY: &str = "synthetic-key";
pub const BASIC: &str = "Basic YXBpOnN5bnRoZXRpYy1rZXk=";

pub fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
pub fn range() -> EventRange {
    EventRange::new(1700000000, 1700000100).unwrap()
}
pub fn domain(api: &Mailgun) -> MailgunDomain {
    api.domain(Domain::new("mg.example.test").unwrap())
}
pub fn client(server: &TestServer) -> Mailgun {
    Mailgun::local(&server.url(), KEY, Duration::from_secs(2)).unwrap()
}
pub fn email() -> TextEmail {
    with_attachment(Attachment::bytes("report.txt", "text/plain", FILE).unwrap())
}
pub fn with_attachment(attachment: Attachment) -> TextEmail {
    TextEmail::new(
        "Sender <sender@mg.example.test>",
        vec!["alice@example.test".into(), "bob@example.test".into()],
        "Fixture report",
        "Please find the report attached.",
        attachment,
    )
    .unwrap()
}
pub fn page(fixture: &str, request: &Recorded) -> Reply {
    Reply::json(
        200,
        fixture.replace("__ORIGIN__", &format!("http://{}", request.host)),
    )
}
pub fn workflow(request: &Recorded) -> Reply {
    match (request.method.as_str(), request.path()) {
        ("POST", "/v3/mg.example.test/messages") => Reply::json(200, ACCEPTED),
        ("GET", "/v3/mg.example.test/events") => page(FIRST, request),
        ("GET", "/v3/mg.example.test/events/page-two") => page(SECOND, request),
        ("GET", "/v3/mg.example.test/events/empty") => page(EMPTY, request),
        _ => Reply::json(404, PROBLEM),
    }
}

// Independent, deliberately small multipart wire reader for these fixtures. It obtains the
// boundary from the received header; no dependency on the SDK's choice of boundary or encoder.
pub fn multipart(request: &Recorded) -> Vec<(String, Vec<u8>)> {
    let content_type = request.header("content-type").unwrap();
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .unwrap();
    let delimiter = format!("--{boundary}");
    let wire = std::str::from_utf8(&request.body).unwrap();
    assert!(wire.ends_with(&format!("{delimiter}--\r\n")));
    wire.split(&delimiter)
        .skip(1)
        .take_while(|part| !part.starts_with("--"))
        .map(|part| {
            let part = part
                .strip_prefix("\r\n")
                .unwrap()
                .strip_suffix("\r\n")
                .unwrap();
            let (headers, bytes) = part.split_once("\r\n\r\n").unwrap();
            (headers.to_owned(), bytes.as_bytes().to_vec())
        })
        .collect()
}

pub fn assert_email_wire(request: &Recorded) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/v3/mg.example.test/messages");
    assert_eq!(request.header("authorization"), Some(BASIC));
    assert_eq!(request.header("accept"), Some("application/json"));
    let parts = multipart(request);
    let expected = [
        ("from", "Sender <sender@mg.example.test>".as_bytes()),
        ("to", "alice@example.test".as_bytes()),
        ("to", "bob@example.test".as_bytes()),
        ("subject", "Fixture report".as_bytes()),
        ("text", "Please find the report attached.".as_bytes()),
    ];
    assert_eq!(parts.len(), 6);
    for ((headers, body), (name, value)) in parts.iter().zip(expected) {
        assert_eq!(
            headers,
            &format!("Content-Disposition: form-data; name=\"{name}\"")
        );
        assert_eq!(body, value);
    }
    assert!(
        parts[5].0.to_ascii_lowercase().contains(
            "content-disposition: form-data; name=\"attachment\"; filename=\"report.txt\""
        )
    );
    assert!(
        parts[5]
            .0
            .to_ascii_lowercase()
            .contains("content-type: text/plain")
    );
    assert_eq!(parts[5].1, FILE);
}
