//! C8.6: fallible edits share the same decision as infallible edits.
// PANIC-SAFETY: Unwraps make unexpected scripted outcomes fail the test.
#![allow(clippy::unwrap_used)]

use concord::error::Kind;
use concord::{by_cursor, mismatch, Fail, Json, Retry};
use concord_testkit::Script;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Problem {
    code: String,
}

fn reject_bad_number(bytes: &[u8]) -> Result<(), Fail<Problem>> {
    let _: u32 = Json::decode(bytes).map_err(Fail::from_read)?;
    Ok(())
}

concord::api! {
    client Lists {
        base "https://lists.test";
        read Json;
        problem Problem;

        GET edited(page?: u32) -> Vec<u32> {
            query { page; }
            paginate by_cursor(page, |_| Some(2))
                .map_next(|next, _| next.page(3u32))
                .try_map_next(move |next, _| {
                    if concord::page::Slot::get(&page, &next) == Some(&3) {
                        Ok(next.page(4u32))
                    } else {
                        Err(mismatch("edit ran out of order"))
                    }
                })
                .map_next(move |next, _| {
                    let at = concord::page::Slot::get(&page, &next).copied().unwrap_or_default();
                    next.page(at + 1)
                })
                .has_more(|items| !items.is_empty());
        }

        GET short(page?: u32) -> Vec<u32> {
            query { page; }
            paginate by_cursor(page, |_| Some(2))
                .try_map_next(|_, _| Err(mismatch("edited a short page")))
                .stop_on_short_page_of(2);
        }

        GET ended(page?: u32) -> Vec<u32> {
            query { page; }
            paginate by_cursor(page, |_| None)
                .try_map_next(|_, _| Err(mismatch("edited the last page")));
        }

        GET rejected(page?: u32) -> Vec<u32> {
            query { page; }
            paginate by_cursor(page, |_| Some(2))
                .try_map_next(|_, _| Err(Problem { code: String::from("closed") }.into()))
                .try_map_next(|_, _| Err(mismatch("ran an edit after a failure")));
        }

        GET bad_number(page?: u32) -> Vec<u32> {
            query { page; }
            paginate by_cursor(page, |_| Some(2))
                .try_map_next(|next, _| {
                    reject_bad_number(br#""private""#)?;
                    Ok(next)
                });
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_mixed_edits_share_one_order_and_later_rules_can_end_the_list() {
    let script = Script::new();
    script.answer(200).body("[1]");
    script.answer(200).body("[]");
    let mut pages = Lists::new().with_transport(script.clone()).edited().pages();
    assert_eq!(pages.try_next().await.unwrap(), Some(vec![1]));
    assert_eq!(pages.try_next().await.unwrap(), Some(vec![]));
    assert_eq!(pages.try_next().await.unwrap(), None);
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests.last().unwrap().url().query(), Some("page=5"));
}

#[tokio::test(start_paused = true)]
async fn c8_6_edits_need_a_successor_that_survives_the_short_page_rules() {
    let script = Script::new();
    script.answer(200).body("[1]");
    script.answer(200).body("[2]");
    let lists = Lists::new().with_transport(script.clone());
    for mut pages in [lists.short().pages(), lists.ended().pages()] {
        assert!(pages.try_next().await.unwrap().is_some());
        assert_eq!(pages.try_next().await.unwrap(), None);
    }
    assert_eq!(script.requests().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn c8_6_the_first_edit_failure_keeps_the_page_evidence_and_ends_the_list() {
    let script = Script::new();
    script.answer(503);
    script.answer(200).header("x-trace", "kept").body("[1]");
    let mut pages = Lists::new()
        .with_transport(script.clone())
        .with_retry(Retry::standard())
        .rejected()
        .pages();
    assert_eq!(pages.try_next().await.unwrap(), Some(vec![1]));
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.problem().unwrap().code, "closed");
    assert_eq!(error.endpoint(), "lists.rejected");
    assert!(error.is_status(200));
    assert_eq!(error.tries(), 2);
    assert_eq!(error.headers().unwrap().get("x-trace"), Some("kept"));
    assert_eq!(pages.try_next().await.unwrap(), None);
    assert_eq!(script.requests().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn c8_6_an_edit_can_return_a_decoder_failure() {
    let script = Script::new();
    script.answer(200).body("[1]");
    let mut pages = Lists::new()
        .with_transport(script.clone())
        .bad_number()
        .pages();
    assert_eq!(pages.try_next().await.unwrap(), Some(vec![1]));
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert!(error.is_status(200));
    assert_eq!(error.body(), Some(br#""private""#.as_slice()));
    assert!(error
        .to_string()
        .contains("the decoder's input could not be decoded"));
    assert!(!format!("{error} {error:?}").contains("private"));
    assert_eq!(pages.try_next().await.unwrap(), None);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c8_6_a_failed_edit_never_runs_the_following_edit() {
    use std::sync::atomic::{AtomicBool, Ordering};

    static LATER_EDIT_RAN: AtomicBool = AtomicBool::new(false);

    concord::api! {
        client EditProbe {
            base "https://lists.test";
            read Json;
            GET list(page?: u32) -> Vec<u32> {
                query { page; }
                paginate by_cursor(page, |_| Some(2))
                    .try_map_next(|_, _| Err(mismatch("the next call is invalid")))
                    .map_next(|next, _| {
                        LATER_EDIT_RAN.store(true, Ordering::Relaxed);
                        next
                    });
            }
        }
    }

    let script = Script::new();
    script.answer(200).body("[1]");
    let mut pages = EditProbe::new()
        .with_transport(script.clone())
        .list()
        .pages();
    assert_eq!(pages.try_next().await.unwrap(), Some(vec![1]));
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.reason(), Some("the next call is invalid"));
    assert!(!LATER_EDIT_RAN.load(Ordering::Relaxed));
    assert_eq!(script.requests().len(), 1);
}
