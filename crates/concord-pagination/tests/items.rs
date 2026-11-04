//! Item consumers (P07, contract §5): the page checkpoint advances only after a page's last item,
//! mid-page stop keeps an offset and the remainder, empty pages advance explicitly, and the
//! documented resume pattern continues without gaps or duplicates.

mod support;

use std::future::ready;
use std::num::NonZeroUsize;

use concord_pagination::{Budget, ItemCheckpoint, Outcome, Page, Resume, resume_items, sequential};
use support::{Server, now};

fn budget(n: usize) -> Budget {
    Budget::new(NonZeroUsize::new(n).unwrap())
}

fn at(page: u32, skip: usize) -> ItemCheckpoint<u32> {
    ItemCheckpoint {
        page: Resume::At(page),
        skip,
    }
}

/// A stable listing: page `n` holds `sizes[n]` items named `p{n}i{i}`; the key of page `n` is
/// `n` (page 0 is the start).
fn listing(
    sizes: &'static [usize],
) -> impl FnMut(Option<u32>) -> std::future::Ready<Result<Page<String, u32>, &'static str>> {
    move |key| {
        let n = key.unwrap_or(0);
        let items = (0..sizes[n as usize])
            .map(|i| format!("p{n}i{i}"))
            .collect();
        let next = ((n as usize) + 1 < sizes.len()).then_some(n + 1);
        ready(Ok(Page::new(items, next)))
    }
}

fn all_items(sizes: &'static [usize]) -> Vec<String> {
    let mut items = sequential(Resume::Start, budget(100), listing(sizes)).items();
    let mut out = Vec::new();
    while let Some(item) = now(items.next()).unwrap() {
        out.push(item.unwrap());
    }
    out
}

#[test]
fn the_page_checkpoint_advances_only_after_the_last_item_of_the_page() {
    let mut items = sequential(Resume::Start, budget(10), listing(&[2, 1])).items();
    assert_eq!(
        items.checkpoint(),
        Some(&ItemCheckpoint {
            page: Resume::Start,
            skip: 0
        })
    );
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), "p0i0");
    // Page 0 was fetched and passed to the adapter, but one of its items is not emitted yet.
    assert_eq!(
        items.checkpoint(),
        Some(&ItemCheckpoint {
            page: Resume::Start,
            skip: 1
        })
    );
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), "p0i1");
    assert_eq!(items.checkpoint(), Some(&at(1, 0)));
    assert_eq!(items.progress().pages_consumed, 1);
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), "p1i0");
    assert_eq!(items.checkpoint(), None);
    assert!(now(items.next()).unwrap().is_none());
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert!(report.resume.is_none());
    assert_eq!(report.progress.items, 3);
}

#[test]
fn a_mid_page_stop_keeps_the_offset_and_the_remainder_and_resumes_without_gaps() {
    let sizes: &'static [usize] = &[3, 4, 2];
    let expected = all_items(sizes);

    // Consume five items (page 0 fully, two items of page 1), then stop.
    let mut items = sequential(Resume::Start, budget(10), listing(sizes)).items();
    let mut first = Vec::new();
    for _ in 0..5 {
        first.push(now(items.next()).unwrap().unwrap().unwrap());
    }
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    // The current page with its offset, not the following page's key.
    assert_eq!(report.resume, Some(at(1, 2)));
    assert_eq!(report.remainder, ["p1i2", "p1i3"]);
    assert_eq!(report.after_remainder, Some(at(2, 0)));
    assert_eq!(report.progress.items, 5);
    assert_eq!(report.progress.pages_consumed, 1);

    // Policy 1: replay the stable page and skip the emitted items.
    let mut resumed = resume_items(report.resume.clone().unwrap(), budget(10), listing(sizes));
    let mut rest = Vec::new();
    while let Some(item) = now(resumed.next()).unwrap() {
        rest.push(item.unwrap());
    }
    let replayed: Vec<String> = first.iter().cloned().chain(rest).collect();
    assert_eq!(replayed, expected);

    // Policy 2: process the remainder, continue after it.
    let mut resumed = resume_items(report.after_remainder.unwrap(), budget(10), listing(sizes));
    let mut rest = Vec::new();
    while let Some(item) = now(resumed.next()).unwrap() {
        rest.push(item.unwrap());
    }
    let kept: Vec<String> = first
        .into_iter()
        .chain(report.remainder)
        .chain(rest)
        .collect();
    assert_eq!(kept, expected);
}

#[test]
fn a_checkpoint_kept_before_drop_resumes_the_same_way() {
    let sizes: &'static [usize] = &[2, 2];
    let mut items = sequential(Resume::Start, budget(10), listing(sizes)).items();
    let mut seen = Vec::new();
    seen.push(now(items.next()).unwrap().unwrap().unwrap());
    seen.push(now(items.next()).unwrap().unwrap().unwrap());
    seen.push(now(items.next()).unwrap().unwrap().unwrap());
    // The documented pattern: keep the checkpoint after processing, then a raw drop.
    let kept = items.checkpoint().cloned().unwrap();
    drop(items);
    assert_eq!(kept, at(1, 1));
    let mut resumed = resume_items(kept, budget(10), listing(sizes));
    while let Some(item) = now(resumed.next()).unwrap() {
        seen.push(item.unwrap());
    }
    assert_eq!(seen, all_items(sizes));
}

#[test]
fn empty_non_terminal_pages_advance_through_an_explicit_transition() {
    type Reply = Result<Page<&'static str, u32>, &'static str>;
    let server = Server::<Option<u32>, Reply>::new();
    let mut items = sequential(Resume::Start, budget(10), |key| server.call(key)).items();

    assert!(now(items.next()).is_none());
    server.open(&None, Ok(Page::new(vec!["x"], Some(1))));
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), "x");
    assert_eq!(items.checkpoint(), Some(&at(1, 0)));

    // Page 1 is empty: once processed, the checkpoint moves to page 2 without an item, and the
    // move is visible while page 2 is still pending.
    assert!(now(items.next()).is_none());
    server.open(&Some(1), Ok(Page::new(vec![], Some(2))));
    assert!(now(items.next()).is_none());
    assert_eq!(items.checkpoint(), Some(&at(2, 0)));
    assert_eq!(items.progress().pages_consumed, 2);
    assert_eq!(items.progress().items, 1);

    server.open(&Some(2), Ok(Page::new(vec![], Some(3))));
    assert!(now(items.next()).is_none());
    assert_eq!(items.checkpoint(), Some(&at(3, 0)));

    server.open(&Some(3), Ok(Page::new(vec!["y"], None)));
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), "y");
    assert_eq!(items.checkpoint(), None);
    assert!(now(items.next()).unwrap().is_none());
    assert_eq!(items.finish().outcome, Outcome::Exhausted);
}

#[test]
fn a_page_failure_keeps_the_checkpoint_on_the_failed_page() {
    let mut items = sequential(Resume::Start, budget(10), |key: Option<u32>| {
        ready(match key {
            None => Ok(Page::new(vec![1, 2], Some(1))),
            Some(_) => Err("boom"),
        })
    })
    .items();
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), 1);
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), 2);
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap_err(), "boom");
    assert!(now(items.next()).unwrap().is_none());
    assert_eq!(items.checkpoint(), Some(&at(1, 0)));
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.failed, Some(Resume::At(1)));
    assert_eq!(report.resume, Some(at(1, 0)));
    assert!(report.remainder.is_empty());
}

#[test]
fn a_resumed_failure_keeps_the_resume_offset() {
    let mut items = resume_items(at(4, 3), budget(10), |_key: Option<u32>| {
        ready(Err::<Page<u32, u32>, _>("expired"))
    });
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap_err(), "expired");
    // Expiry is the provider's error; nothing restarts from the first page.
    assert_eq!(items.finish().resume, Some(at(4, 3)));
}

#[test]
fn a_finish_at_a_page_boundary_resumes_at_the_next_page() {
    let mut items = sequential(Resume::Start, budget(10), listing(&[2, 2])).items();
    now(items.next()).unwrap().unwrap().unwrap();
    now(items.next()).unwrap().unwrap().unwrap();
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.resume, Some(at(1, 0)));
    assert!(report.remainder.is_empty());
    assert_eq!(report.after_remainder, report.resume);
}

#[test]
fn a_finish_after_the_last_page_with_items_left_is_stopped_not_exhausted() {
    let mut items = sequential(Resume::Start, budget(10), listing(&[3])).items();
    now(items.next()).unwrap().unwrap().unwrap();
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.remainder, ["p0i1", "p0i2"]);
    assert_eq!(
        report.resume,
        Some(ItemCheckpoint {
            page: Resume::Start,
            skip: 1
        })
    );
    // Processing the remainder finishes the listing.
    assert_eq!(report.after_remainder, None);
}

#[test]
fn the_budget_bounds_item_consumption_at_a_page_boundary() {
    let mut items = sequential(Resume::Start, budget(2), listing(&[1, 1, 1, 1])).items();
    let mut seen = Vec::new();
    while let Some(item) = now(items.next()).unwrap() {
        seen.push(item.unwrap());
    }
    assert_eq!(seen, ["p0i0", "p1i0"]);
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.resume, Some(at(2, 0)));
}

#[test]
fn a_resumed_page_shorter_than_its_skip_is_skipped_entirely() {
    // Documented behaviour for an unstable page: the skip consumes it.
    let mut items = resume_items(at(0, 9), budget(10), |key: Option<u32>| {
        let n = key.unwrap_or(0);
        ready(Ok::<_, &str>(Page::new(
            vec![n * 10, n * 10 + 1],
            (n < 1).then_some(n + 1),
        )))
    });
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), 10);
}
