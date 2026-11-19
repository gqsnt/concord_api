//! C8.6: a reported page number and last page are one statement.
// PANIC-SAFETY: Unwraps make unexpected scripted outcomes fail the test.
#![allow(clippy::unwrap_used)]

use concord::page::IntoItems;
use concord::{by_cursor, by_page, by_page_bounds, Json};
use concord_testkit::Script;
use serde::Deserialize;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Deserialize)]
struct Groups {
    bounds: Option<(i64, i64)>,
    files: Vec<String>,
    messages: Vec<String>,
    #[serde(skip)]
    visits: AtomicUsize,
}

#[derive(Debug, Deserialize)]
struct Counted(Groups);

impl IntoItems for Counted {
    type Item = String;

    fn count(&self) -> usize {
        self.0.files.len()
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.0.files
    }
}

fn count_bounds(page: &Groups) -> Option<(i64, i64)> {
    page.visits.fetch_add(1, Ordering::Relaxed);
    page.bounds
}

concord::api! {
    client Lists {
        base "https://lists.test";
        read Json;

        GET groups(page?: i64) -> Groups {
            query { page; }
            paginate by_page_bounds(page, count_bounds);
        }

        GET counted(page?: i64) -> Counted {
            query { page; }
            paginate by_page_bounds(page, |p| p.0.bounds).stop_on_short_page_of(9);
        }

        GET ordinary(page?: i64) -> Counted {
            query { page; }
            paginate by_page(page, 1).stop_on_short_page_of(9);
        }

        GET conflict(page?: i64) -> Groups {
            query { page; }
            paginate by_page_bounds(page, |p| p.bounds).has_more(|p| !p.messages.is_empty());
        }

        GET alternate(page?: i64, token?: String) -> Groups {
            query { page; token; }
            paginate by_page_bounds(page, |p| p.bounds)
                .or_when_set(token, by_cursor(token, |p| p.files.last().cloned()));
        }

        GET gated(page?: i64) -> Groups {
            query { page; }
            paginate by_page_bounds(page, count_bounds).only_when_set(page);
        }
    }
}

const MORE: &str = r#"{"bounds":[4,5],"files":[],"messages":[]}"#;
const LAST: &str = r#"{"bounds":[5,5],"files":[],"messages":[]}"#;

fn lists(script: &Script) -> Lists {
    Lists::new().with_transport(script.clone())
}

/// An empty envelope without IntoItems can still advance, with or without a requested number.
#[tokio::test(start_paused = true)]
async fn c8_6_bounds_are_read_once_and_the_reported_number_moves() {
    for requested in [None, Some(99i64)] {
        let script = Script::new();
        script.answer(200).body(MORE);
        script.answer(200).body(LAST);
        let mut pages = lists(&script).groups().page(requested).pages();
        for _ in 0..2 {
            let page = pages.try_next().await.unwrap().unwrap();
            assert_eq!(page.visits.load(Ordering::Relaxed), 1);
        }
        assert!(pages.try_next().await.unwrap().is_none());
        let requests = script.requests();
        let first_query = requested.map(|n| format!("page={n}"));
        assert_eq!(
            requests.first().unwrap().url().query(),
            first_query.as_deref()
        );
        assert_eq!(requests.last().unwrap().url().query(), Some("page=5"));
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_bounds_override_a_short_page_and_leave_counting_pagers_unchanged() {
    let script = Script::new();
    script.answer(200).body(MORE);
    script.answer(200).body(LAST);
    script.answer(200).body(MORE);
    let lists = lists(&script);
    let mut pages = lists.counted().pages();
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_none());
    let mut pages = lists.ordinary().pages();
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(script.requests().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn c8_6_bounds_can_disagree_and_bind_an_alternate_move() {
    let script = Script::new();
    script.answer(200).body(MORE);
    script.answer(200).body(MORE);
    script
        .answer(200)
        .body(r#"{"bounds":[-1,5],"files":["next"],"messages":[]}"#);
    script.answer(200).body(LAST);
    let lists = lists(&script);
    for (mut pages, reason) in [
        (
            lists.conflict().pages(),
            "the page's statements disagree on whether more items follow",
        ),
        (
            lists.alternate().token("old").pages(),
            "the page says that more items follow, and gives no next page",
        ),
    ] {
        assert!(pages.try_next().await.unwrap().is_some());
        assert_eq!(pages.try_next().await.unwrap_err().reason(), Some(reason));
        assert!(pages.try_next().await.unwrap().is_none());
    }
    let mut pages = lists.alternate().token("old").pages();
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(
        script.requests().last().unwrap().url().query(),
        Some("token=next")
    );
}

#[tokio::test(start_paused = true)]
async fn c8_6_missing_bounds_and_terminal_bounds_end_but_an_invalid_advance_fails() {
    for (bounds, reason) in [
        ("null", None),
        ("[5,5]", None),
        ("[5,4]", None),
        ("[-1,-1]", None),
        ("[-1,1]", Some("the page number cannot grow any further")),
    ] {
        let script = Script::new();
        script
            .answer(200)
            .body(format!(r#"{{"bounds":{bounds},"files":[],"messages":[]}}"#));
        let mut pages = lists(&script).groups().pages();
        assert!(pages.try_next().await.unwrap().is_some());
        match reason {
            Some(reason) => assert_eq!(pages.try_next().await.unwrap_err().reason(), Some(reason)),
            None => assert!(pages.try_next().await.unwrap().is_none()),
        }
        assert_eq!(script.requests().len(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_gates_precede_bounds_and_an_unchanged_clamped_request_cannot_repeat() {
    let script = Script::new();
    script.answer(200).body(MORE);
    script.answer(200).body(MORE);
    let lists = lists(&script);
    let mut pages = lists.gated().pages();
    let page = pages.try_next().await.unwrap().unwrap();
    assert_eq!(page.visits.load(Ordering::Relaxed), 0);
    assert!(pages.try_next().await.unwrap().is_none());

    let mut pages = lists.groups().page(5i64).pages();
    assert!(pages.try_next().await.unwrap().is_some());
    assert_eq!(
        pages.try_next().await.unwrap_err().kind(),
        concord::error::Kind::Mismatch
    );
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(script.requests().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn c8_6_a_reported_page_at_u64_max_cannot_advance() {
    #[derive(Debug, Deserialize)]
    struct WideBounds {
        bounds: Option<(u128, u128)>,
    }

    concord::api! {
        client WideLists {
            base "https://lists.test";
            read Json;
            GET list(page?: u128) -> WideBounds {
                query { page; }
                paginate by_page_bounds(page, |value| value.bounds);
            }
        }
    }

    let current = u128::from(u64::MAX);
    let last = current + 1;
    let script = Script::new();
    script
        .answer(200)
        .body(format!(r#"{{"bounds":[{current},{last}]}}"#));
    let mut pages = WideLists::new()
        .with_transport(script.clone())
        .list()
        .pages();
    assert_eq!(
        pages.try_next().await.unwrap().unwrap().bounds,
        Some((current, last))
    );
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), concord::error::Kind::Mismatch);
    assert_eq!(
        error.reason(),
        Some("the page number cannot grow any further")
    );
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(script.requests().len(), 1);
}
