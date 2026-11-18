//! C8.6: what the pages end with when a page and its pager disagree, kind and text, through a
//! scripted client.
// Test fixtures intentionally fail on unexpected outcomes.
#![allow(clippy::unwrap_used)]

use concord::error::Kind;
use concord::page::IntoItems;
use concord::{by_offset_limit, by_page, Json};
use concord_testkit::Script;
use serde::Deserialize;

/// A page of numbers.
#[derive(Debug, Deserialize)]
pub struct Page {
    /// The numbers.
    pub items: Vec<u32>,
}
impl IntoItems for Page {
    type Item = u32;
    fn into_items(self) -> Vec<u32> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

concord::api! {
    client Lists {
        base "https://lists.test";
        read Json;
        GET offsets(start?: u32, count?: u32) -> Page {
            path "offsets";
            query { start; count; }
            paginate by_offset_limit(start, count);
        }
        GET more(start?: u32, count?: u32) -> Page {
            path "more";
            query { start; count; }
            paginate by_offset_limit(start, count).has_more(|_| true);
        }
        GET less(start?: u32, count?: u32) -> Page {
            path "less";
            query { start; count; }
            paginate by_offset_limit(start, count).has_more(|_| false);
        }
        GET numbered(page?: u32) -> Page {
            path "numbered";
            query { page; }
            paginate by_page(page, 1).has_more(|_| true);
        }
    }
}

const MORE: &str = "the page says that more items follow, and gives no next page";

/// The largest offset, a short page of one item: the end, unless the page says that more follow.
#[tokio::test(start_paused = true)]
async fn c8_6_a_short_page_at_the_largest_offset_ends_unless_the_page_says_more() {
    for (endpoint, said) in [
        ("offsets", None),
        ("less", Some(false)),
        ("more", Some(true)),
    ] {
        let script = Script::new();
        script.answer(200).body(r#"{"items":[1]}"#);
        let client = Lists::new().with_transport(script.clone());
        let mut pages = match endpoint {
            "offsets" => client.offsets().start(u32::MAX).count(3u32).pages(),
            "less" => client.less().start(u32::MAX).count(3u32).pages(),
            _ => client.more().start(u32::MAX).count(3u32).pages(),
        };
        assert!(pages.try_next().await.unwrap().is_some());
        match said {
            Some(true) => {
                let error = pages.try_next().await.unwrap_err();
                assert_eq!(error.kind(), Kind::Mismatch);
                assert_eq!(error.reason(), Some(MORE));
            }
            _ => assert!(pages.try_next().await.unwrap().is_none(), "{endpoint}"),
        }
        assert_eq!(script.requests().len(), 1);
    }
}

/// An empty page that says more follow is never the end.
#[tokio::test(start_paused = true)]
async fn c8_6_an_empty_page_that_announces_another_is_a_mismatch() {
    let script = Script::new();
    script.answer(200).body(r#"{"items":[]}"#);
    script.answer(200).body(r#"{"items":[1]}"#);
    let mut pages = Lists::new()
        .with_transport(script.clone())
        .numbered()
        .page(1u32)
        .pages();
    assert!(pages.try_next().await.unwrap().is_some());
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(error.reason(), Some(MORE));
    assert_eq!(script.requests().len(), 1);
}
