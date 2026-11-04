//! The fixture's own behaviour, checked with raw HTTP/1.1 written by hand (no client library).

use concord_example_fixture::{Server, rfc3339_seconds, services};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn raw(server: &Server, request: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).await.unwrap();
        out.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&out);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let len: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if body.len() >= len {
                return text.into_owned();
            }
        }
        if n == 0 {
            return String::from_utf8_lossy(&out).into_owned();
        }
    }
}

#[tokio::test]
async fn catalog_filters_pages_and_requires_a_key() {
    let server = services().await;
    let answer = raw(
        &server,
        "GET /catalog/v1/tenants/acme/items?tag=featured&limit=3 HTTP/1.1\r\nhost: x\r\nx-api-key: k\r\n\r\n",
    )
    .await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains("x-result-count: 4"), "{answer}");
    assert!(
        answer.contains(r#""next_cursor":"catalog-offset-3""#),
        "{answer}"
    );

    let refused = raw(
        &server,
        "GET /catalog/v1/health HTTP/1.1\r\nhost: x\r\n\r\n",
    )
    .await;
    assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");

    let recorded = server.requests();
    assert_eq!(recorded[0].query_all("tag"), ["featured"]);
    assert_eq!(
        recorded[0].segments(),
        ["catalog", "v1", "tenants", "acme", "items"]
    );
}

#[tokio::test]
async fn a_chunked_request_body_is_decoded_and_flagged() {
    let server = services().await;
    let answer = raw(
        &server,
        "PUT /catalog/v1/tenants/a%2Fb/items/item-001/attachment HTTP/1.1\r\nhost: x\r\nx-api-key: k\r\ntransfer-encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
    )
    .await;
    assert!(answer.contains(r#""bytes_received":11"#), "{answer}");
    assert!(answer.contains("x-tenant-echo: a%2Fb"), "{answer}");
    let recorded = server.single();
    assert!(recorded.chunked);
    assert_eq!(recorded.body, b"hello world");
    assert_eq!(recorded.segments()[3], "a/b");
}

#[tokio::test]
async fn ledger_pages_with_its_own_envelope() {
    let server = services().await;
    let answer = raw(
        &server,
        "GET /ledger/v1/entries?account=acct-002a&from_ms=1769904000000&page_size=2 HTTP/1.1\r\nhost: x\r\nauthorization: Bearer t\r\n\r\n",
    )
    .await;
    assert!(
        answer.contains("content-type: application/vnd.demo.ledger+json"),
        "{answer}"
    );
    assert!(answer.contains("x-record-count: 4"), "{answer}");
    assert!(answer.contains(r#""next":"ledger-from-2""#), "{answer}");
    let bad = raw(
        &server,
        "GET /ledger/v1/entries?account=42 HTTP/1.1\r\nhost: x\r\nauthorization: Bearer t\r\n\r\n",
    )
    .await;
    assert!(bad.starts_with("HTTP/1.1 400"), "{bad}");
}

#[test]
fn whole_second_timestamps_parse() {
    assert_eq!(rfc3339_seconds("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(rfc3339_seconds("2026-01-15T10:00:00Z"), Some(1_768_471_200));
    assert_eq!(rfc3339_seconds("2026-01-15T10:00:00.5Z"), None);
    assert_eq!(rfc3339_seconds("2026-13-01T00:00:00Z"), None);
    // Calendar anchors around century leap rules, and impossible dates.
    assert_eq!(
        rfc3339_seconds("1900-03-01T00:00:00Z"),
        Some(-2_203_891_200)
    );
    assert_eq!(rfc3339_seconds("2000-02-29T00:00:00Z"), Some(951_782_400));
    assert_eq!(rfc3339_seconds("2100-02-28T00:00:00Z"), Some(4_107_456_000));
    assert_eq!(rfc3339_seconds("1900-02-29T00:00:00Z"), None);
    assert_eq!(rfc3339_seconds("2026-02-30T00:00:00Z"), None);
    assert_eq!(rfc3339_seconds("2026-04-31T00:00:00Z"), None);
    // Signs are not digits.
    assert_eq!(rfc3339_seconds("1970-01-01T-1:00:00Z"), None);
    assert_eq!(rfc3339_seconds("+970-01-01T00:00:00Z"), None);
}
