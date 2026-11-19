//! C8.2, C8.4, C8.6 and C12.10: complete requests sent by a page walk.

use super::{CanonicalPath, Clock, Credentials, SigV4};
use concord::page::{by_offset_total, IntoItems};
use concord::{
    by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page, Body, Pages, Secret,
};
use concord_testkit::{script::Request, Script};
use serde::Deserialize;
use std::time::{Duration, UNIX_EPOCH};

const ORIGIN: &str = "https://pages.test";
const PAYLOAD: &[u8] = "one\né\n".as_bytes();

#[derive(Debug, Deserialize)]
struct Batch {
    #[serde(default)]
    rows: Vec<u32>,
    total: Option<u64>,
    next: Option<String>,
}

impl IntoItems for Batch {
    type Item = u32;
    fn into_items(self) -> Vec<u32> {
        self.rows
    }
    fn count(&self) -> usize {
        self.rows.len()
    }
}

concord::api! {
    client Shelf {
        base "https://pages.test";
        read concord::Json;
        headers { "x-mode" = ["one", "two"]; }
        part payload() { body Body::new(PAYLOAD, "application/octet-stream"); }

        POST offsets(start?: u32, ..payload) -> Batch {
            path "api/offsets";
            query { start; }
            paginate by_offset(start);
        }
        POST limited(start?: u32, count?: u32, ..payload) -> Batch {
            path "api/limited";
            query { start; count; }
            paginate by_offset_limit(start, count);
        }
        POST totals(start?: u32, ..payload) -> Batch {
            path "api/totals";
            query { start; }
            paginate by_offset_total(start, |batch: &Batch| batch.total.unwrap());
        }
        POST numbered(number?: u32, ..payload) -> Batch {
            path "api/numbered";
            query { number; }
            paginate by_page(number, 3);
        }
        POST cursors(after?: String, ..payload) -> Batch {
            path "api/cursors";
            query { after; }
            paginate by_cursor(after, |batch: &Batch| batch.next.clone());
        }
        POST linked(..payload) -> Batch {
            path "api/linked";
            paginate by_link();
        }
        POST announced(..payload) -> Batch {
            path "api/announced";
            paginate by_next_url(|batch: &Batch| batch.next.clone());
        }
        POST signed(token: Secret, ..payload) -> Batch {
            path "api/signed";
            query { "mode" = "fixed"; token; }
            headers { "authorization" = "old-authorization"; }
            paginate by_link();
        }
    }
}

fn transcript(requests: &[Request]) -> Vec<u8> {
    use std::io::Write;
    let mut bytes = Vec::new();
    for request in requests {
        writeln!(bytes, "{} {}", request.method(), request.url()).unwrap();
        // Header names have no relative order. The values of each name keep their order.
        let mut names: Vec<_> = request.headers().keys().collect();
        names.sort_unstable_by_key(|name| name.as_str());
        for name in names {
            for value in request.headers().get_all(name) {
                write!(bytes, "{name}: ").unwrap();
                bytes.extend_from_slice(value.as_bytes());
                bytes.push(b'\n');
            }
        }
        if let Some(body) = request.body() {
            write!(bytes, "\n{} bytes\n", body.len()).unwrap();
            bytes.extend_from_slice(body);
        } else {
            bytes.extend_from_slice(b"\nno body\n");
        }
    }
    bytes
}

fn fixture(script: &Script, name: &str) {
    let data = include_str!("../fixtures/paging_requests.http");
    let section = format!("=== {name} ===\n");
    let (_, expected) = data.split_once(&section).unwrap();
    let expected = expected.split("=== ").next().unwrap().as_bytes();
    let sent = transcript(&script.requests());
    assert!(
        sent == expected,
        "{name}\nexpected:\n{}\nrecorded:\n{}",
        String::from_utf8_lossy(expected),
        String::from_utf8_lossy(&sent),
    );
    assert_eq!(script.unused(), 0);
}

async fn two_pages(mut pages: Pages<Batch>, name: &str) {
    assert!(pages.try_next().await.unwrap().is_some(), "{name}");
    assert!(pages.try_next().await.unwrap().is_some(), "{name}");
    assert!(pages.try_next().await.unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn c8_6_shipped_pagers_send_the_complete_requests() {
    const FULL: &str = r#"{"rows":[1,2]}"#;
    for (name, first, last, link) in [
        ("offsets", FULL, "{}", None),
        ("limited", FULL, r#"{"rows":[3]}"#, None),
        (
            "totals",
            r#"{"rows":[1,2],"total":3}"#,
            r#"{"rows":[3],"total":3}"#,
            None,
        ),
        ("numbered", FULL, "{}", None),
        ("cursors", r#"{"next":"a+b /"}"#, "{}", None),
        (
            "linked",
            FULL,
            "{}",
            Some(r#"<../wire%2fnext?b=%2f&&b=a+b#discard>; rel="next""#),
        ),
        ("announced", r#"{"next":"/from-body?#discard"}"#, "{}", None),
    ] {
        let script = Script::new();
        let first = script.answer(200).body(first);
        let first = match link {
            Some(field) => first
                .header("link", "</back>; rel=prev")
                .header("link", field),
            None => first,
        };
        drop(first);
        script.answer(200).body(last);
        let client = Shelf::new().with_transport(script.clone());
        let pages = match name {
            "offsets" => client.offsets().pages(),
            "limited" => client.limited().count(2).pages(),
            "totals" => client.totals().pages(),
            "numbered" => client.numbered().pages(),
            "cursors" => client.cursors().pages(),
            "linked" => client.linked().pages(),
            "announced" => client.announced().pages(),
            _ => unreachable!(),
        };
        two_pages(pages, name).await;
        fixture(&script, name);
    }
}

#[tokio::test(start_paused = true)]
async fn c8_2_the_signer_sees_the_exact_next_url_and_current_secret() {
    for (name, supplied) in [
        ("signed-empty", "/slice?#discard"),
        (
            "signed-pairs",
            "/slice?x=%2f&&token=fake&x=a+b&token=again#discard",
        ),
    ] {
        let signer = SigV4::new(
            &ORIGIN.parse().unwrap(),
            Credentials::new("EXAMPLE", "fixture-secret"),
            "us-east-1",
            "s3",
            CanonicalPath::Preserve,
        );
        assert!(signer
            .clock
            .set(Clock {
                wall: UNIX_EPOCH + Duration::from_secs(1_440_938_160),
                tick: tokio::time::Instant::now(),
            })
            .is_ok());
        let script = Script::new();
        script
            .answer(200)
            .header("link", "</back>; rel=prev")
            .header("link", &format!("<{supplied}>; rel=next"))
            .body("{}");
        script.answer(200).body("{}");
        let client = Shelf::new()
            .with_transport(script.clone())
            .with_middleware(signer);
        let token = Secret::new("A");
        let mut pages = client.signed(token.clone()).pages();
        assert!(pages.try_next().await.unwrap().is_some(), "{name}");
        token.replace("B+z");
        assert!(pages.try_next().await.unwrap().is_some(), "{name}");
        assert!(pages.try_next().await.unwrap().is_none());
        fixture(&script, name);
    }
}
