//! Diagnostics (contract §5): no `Debug` or `Display` of this crate prints a key, cursor, URL,
//! item or metadata value. Every public type is formatted with secret-bearing values.

mod support;

use std::future::ready;
use std::num::NonZeroUsize;

use concord_pagination::{
    Budget, ItemCheckpoint, JobOptions, JobsError, Keyed, Manifest, OnError, Options, Outcome,
    Page, Resume, first_page_then, independent, manifest_jobs, sequential,
};
use support::now;

const SECRET: &str = "SECRET";

fn secret(tag: &str) -> String {
    format!("https://example.test/{SECRET}-{tag}?token={SECRET}")
}

fn assert_clean(what: &str, text: String) {
    assert!(!text.contains(SECRET), "{what} leaked: {text}");
}

fn budget(n: usize) -> Budget {
    Budget::new(NonZeroUsize::new(n).unwrap())
}

/// An error whose own derived `Debug` would print its secret detail: the crate's `Debug` impls
/// must not delegate to it.
#[derive(Debug)]
struct Fail(#[allow(dead_code)] String);

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("request failed")
    }
}

impl std::error::Error for Fail {}

#[test]
fn values_and_reports_never_print_keys_items_or_metadata() {
    let page = Page {
        items: vec![secret("item")],
        next: Some(secret("cursor")),
        meta: secret("meta"),
    };
    assert_clean("Page", format!("{page:?}"));
    assert_eq!(format!("{page:?}"), "Page { items: 1, next: Some(..), .. }");
    let manifest: Manifest<String, String> = Manifest::new(vec![secret("url")], None);
    assert_clean("Manifest", format!("{manifest:?}"));
    assert_clean("Resume", format!("{:?}", Resume::At(secret("cursor"))));
    let checkpoint = ItemCheckpoint {
        page: Resume::At(secret("cursor")),
        skip: 2,
    };
    assert_clean("ItemCheckpoint", format!("{checkpoint:?}"));
    let keyed = Keyed {
        index: 3,
        key: secret("key"),
        value: secret("value"),
    };
    assert_clean("Keyed", format!("{keyed:?}"));
    let failure = Keyed {
        index: 3,
        key: secret("key"),
        value: Fail(secret("detail")),
    };
    assert_clean("Keyed<error> Display", failure.to_string());
    assert_eq!(failure.to_string(), "#3: request failed");
    let job: JobsError<String, Fail, Fail> = JobsError::Job(failure);
    assert_clean("JobsError Debug", format!("{job:?}"));
    assert_clean("JobsError Display", job.to_string());
    let discovery: JobsError<String, Fail, Fail> = JobsError::Discovery(Fail(secret("detail")));
    assert_clean("JobsError::Discovery Debug", format!("{discovery:?}"));
    assert_clean("JobsError::Discovery Display", discovery.to_string());
    assert_clean("Options", format!("{:?}", Options::new(budget(3))));
    assert_clean(
        "JobOptions",
        format!("{:?}", JobOptions::new(budget(3), Options::new(budget(3)))),
    );
    for outcome in [Outcome::Exhausted, Outcome::DiscoveryFailed] {
        assert_clean("Outcome", format!("{outcome:?} {outcome}"));
    }
}

#[test]
fn traversals_and_their_reports_never_print_keys_or_items() {
    // Sequential, items.
    let fetch = |cursor: Option<String>| {
        ready(Ok::<_, Fail>(Page {
            items: vec![secret("item"), secret("item2")],
            next: if cursor.is_none() {
                Some(secret("cursor"))
            } else {
                None
            },
            meta: secret("meta"),
        }))
    };
    let mut pages = sequential(Resume::Start, budget(5), fetch);
    now(pages.next()).unwrap().unwrap().unwrap();
    assert_clean("Sequential", format!("{pages:?}"));
    assert_clean("SequentialReport", format!("{:?}", pages.finish()));

    let mut items = sequential(Resume::Start, budget(5), fetch).items();
    now(items.next()).unwrap().unwrap().unwrap();
    assert_clean("Items", format!("{items:?}"));
    assert_clean("ItemsReport", format!("{:?}", items.finish()));

    // Independent, first page, with retained and failed keys.
    let mut keys = independent(
        vec![secret("k1"), secret("k2"), secret("k3")],
        Options::new(budget(5))
            .window(NonZeroUsize::new(3).unwrap())
            .on_error(OnError::Continue),
        |key: String| {
            ready(if key.ends_with("k2") {
                Err(Fail(secret("detail")))
            } else {
                Ok(secret("result"))
            })
        },
    );
    now(keys.next()).unwrap().unwrap().unwrap();
    assert_clean("Independent", format!("{keys:?}"));
    assert_clean("Checkpoint", format!("{:?}", keys.checkpoint()));
    assert_clean("IndependentReport", format!("{:?}", keys.finish()));

    let mut first = first_page_then(
        secret("first"),
        Options::new(budget(5)),
        |key: String| ready(Ok::<_, Fail>(key)),
        |_: &String| Ok::<_, Fail>(vec![secret("second")]),
    );
    now(first.next()).unwrap().unwrap().unwrap();
    assert_clean("first_page_then", format!("{first:?}"));
    assert_clean("first_page_then report", format!("{:?}", first.finish()));

    // Jobs.
    let mut jobs = manifest_jobs(
        Resume::At(secret("manifest")),
        JobOptions::new(budget(5), Options::new(budget(5))),
        |_: Option<String>| {
            ready(Ok::<_, Fail>(Manifest::new(
                vec![secret("url1"), secret("url2")],
                Some(secret("next-manifest")),
            )))
        },
        |url: String| ready(Ok::<_, Fail>(url)),
    );
    now(jobs.next()).unwrap().unwrap().unwrap();
    assert_clean("Jobs", format!("{jobs:?}"));
    assert_clean("JobsProgress", format!("{:?}", jobs.progress()));
    assert_clean("JobsCheckpoint", format!("{:?}", jobs.checkpoint()));
    assert_clean("JobsReport", format!("{:?}", jobs.finish()));
}
