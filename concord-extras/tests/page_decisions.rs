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

/// A limit of any new public trait with an implementation for closures: with a glob import of
/// `concord::page::*`, `Pager::next_call` is found on a box of a closure before the author's own
/// method of that name, which Rust reaches only through the box. This pins it, so a change notices.
mod a_glob_import {
    use super::Page;
    use concord::page::*;
    use concord::{Answer, Fail, Json};

    type Callback = dyn FnMut(lists::Call<lists::Tokens>, &Answer<Page>) -> Option<lists::Call<lists::Tokens>>
        + Send;
    // Rust reports the author's trait as never used: that is what this test shows.
    #[allow(dead_code)]
    trait LocalMove {
        fn next_call(
            &mut self,
            call: lists::Call<lists::Tokens>,
            page: &Answer<Page>,
        ) -> Result<Option<lists::Call<lists::Tokens>>, Fail<()>>;
    }
    impl LocalMove for Callback {
        fn next_call(
            &mut self,
            call: lists::Call<lists::Tokens>,
            page: &Answer<Page>,
        ) -> Result<Option<lists::Call<lists::Tokens>>, Fail<()>> {
            Ok(match page.value().items.first() {
                Some(1) => Some(call.after("next")),
                _ => self(call, page),
            })
        }
    }

    concord::api! {
        client Lists {
            base "https://lists.test";
            read Json;
            GET tokens(after?: String) -> Page {
                path "tokens";
                query { after; }
                paginate {
                    let mut f: Box<Callback> = Box::new(|_, _| None);
                    move |call, page| f.next_call(call, page)
                };
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c8_6_a_glob_import_makes_a_boxed_closure_call_concords_next_call_not_the_authors() {
        let script = concord_testkit::Script::new();
        script.answer(200).body(r#"{"items":[1]}"#);
        script.answer(200).body(r#"{"items":[2]}"#);
        let mut pages = Lists::new().with_transport(script.clone()).tokens().pages();
        let mut items = Vec::new();
        while let Some(page) = pages.try_next().await.unwrap() {
            items.extend(page.items);
        }
        // The author's method would have asked for the second page; Concord's ends the list.
        assert_eq!(items, [1]);
        assert_eq!(script.requests().len(), 1);
    }
}

/// An offset that cannot be read as a number of items is not an offset that cannot grow, and a
/// short page does not hide it; an offset type needs no `Default`.
mod offsets {
    use super::Page;
    use concord::error::Kind;
    use concord::page::by_offset_total;
    use concord::{by_offset, by_offset_limit, Json};
    use concord_testkit::Script;
    use serde::Serialize;

    /// A total that the page states.
    #[derive(Debug, serde::Deserialize)]
    pub struct Counted {
        /// The numbers.
        pub items: Vec<u32>,
        /// How many the list holds.
        pub total: u64,
    }
    impl concord::page::IntoItems for Counted {
        type Item = u32;
        fn into_items(self) -> Vec<u32> {
            self.items
        }
        fn count(&self) -> usize {
            self.items.len()
        }
    }

    /// An offset with no `Default`.
    #[derive(Clone, Copy, Debug, Serialize)]
    #[serde(transparent)]
    pub struct At(u64);
    impl From<u64> for At {
        fn from(at: u64) -> At {
            At(at)
        }
    }
    impl From<At> for u64 {
        fn from(at: At) -> u64 {
            at.0
        }
    }

    concord::api! {
        client Offsets {
            base "https://offsets.test";
            read Json;
            GET total(start?: i32) -> Counted {
                path "total";
                query { start; }
                paginate by_offset_total(start, |page: &Counted| page.total);
            }
            GET limited(start?: i32, count?: u32) -> Page {
                path "limited";
                query { start; count; }
                paginate by_offset_limit(start, count);
            }
            GET plain(start?: At) -> Page { path "plain"; query { start; } paginate by_offset(start); }
            GET bounded(start?: At, count?: u32) -> Page {
                path "bounded";
                query { start; count; }
                paginate by_offset_limit(start, count);
            }
            GET stated(start?: At) -> Counted {
                path "stated";
                query { start; }
                paginate by_offset_total(start, |page: &Counted| page.total);
            }
        }
    }

    const UNREADABLE: &str = "the offset of the call is not a number of items";

    #[tokio::test(start_paused = true)]
    async fn c8_6_an_offset_that_cannot_be_read_is_its_own_mismatch() {
        // by_offset_total at -3: the statement cannot read the offset
        let script = Script::new();
        script.answer(200).body(r#"{"items":[1],"total":9}"#);
        let mut pages = Offsets::new()
            .with_transport(script.clone())
            .total()
            .start(-3)
            .pages();
        assert!(pages.try_next().await.unwrap().is_some());
        let error = pages.try_next().await.unwrap_err();
        assert_eq!(
            (error.kind(), error.reason()),
            (Kind::Mismatch, Some(UNREADABLE))
        );
        // by_offset_limit at -3, on a short page that is not empty: not a quiet end
        let script = Script::new();
        script.answer(200).body(r#"{"items":[1]}"#);
        let mut pages = Offsets::new()
            .with_transport(script.clone())
            .limited()
            .start(-3)
            .count(5u32)
            .pages();
        assert!(pages.try_next().await.unwrap().is_some());
        let error = pages.try_next().await.unwrap_err();
        assert_eq!(
            (error.kind(), error.reason()),
            (Kind::Mismatch, Some(UNREADABLE))
        );
        assert_eq!(script.requests().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn c8_6_an_offset_type_needs_no_default() {
        let script = Script::new();
        script.answer(200).body(r#"{"items":[1,2]}"#);
        script.answer(200).body(r#"{"items":[]}"#);
        let mut pages = Offsets::new()
            .with_transport(script.clone())
            .plain()
            .pages();
        while pages.try_next().await.unwrap().is_some() {}
        assert_eq!(
            script.requests()[1].url().as_str(),
            "https://offsets.test/plain?start=2"
        );
        let _ = Offsets::new().bounded();
        let _ = Offsets::new().stated();
    }
}
