//! C8.4 and C8.6 cover the complete requests of a page walk.

// PANIC-SAFETY: Assertions and unwraps are confined to the test executable.
#![allow(clippy::unwrap_used)]

#[path = "support/paging_requests.rs"]
mod transcript;

use concord::error::Kind;
use concord::page::{by_offset_total, IntoItems};
use concord::{by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page, Body, Pages};
use concord_testkit::Script;
use serde::Deserialize;

const DATA: &str = include_str!("fixtures/paging_requests.http");
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
    }
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
        transcript::assert_requests(DATA, &script, name);
    }

    for (name, body, failure) in [
        (
            "repeated-cursor",
            r#"{"next":"again"}"#,
            Some(Kind::Mismatch),
        ),
        (
            "foreign-origin",
            r#"{"next":"https://foreign.test/next"}"#,
            Some(Kind::Build),
        ),
        (
            "same-path-query",
            r#"{"next":"/api/announced?&&#discard"}"#,
            None,
        ),
    ] {
        let script = Script::new();
        script.answer(200).body(body);
        if failure.is_none() {
            script.answer(200).body("{}");
        }
        let client = Shelf::new().with_transport(script.clone());
        let mut pages = if name == "repeated-cursor" {
            client.cursors().after("again").pages()
        } else {
            client.announced().pages()
        };
        assert!(pages.try_next().await.unwrap().is_some(), "{name}");
        if let Some(kind) = failure {
            let error = pages.try_next().await.unwrap_err();
            assert_eq!(error.kind(), kind, "{name}");
            assert_eq!(error.reason(), None, "{name}");
        } else {
            assert!(pages.try_next().await.unwrap().is_some(), "{name}");
        }
        assert!(pages.try_next().await.unwrap().is_none(), "{name}");
        transcript::assert_requests(DATA, &script, name);
    }
}
