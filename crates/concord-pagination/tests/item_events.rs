//! Metadata event traversal and provider error access.

use std::future::ready;
use std::num::NonZeroUsize;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use concord_pagination::{Outcome, Page, PageItemEvent, Pages, Resume};

#[derive(Debug)]
struct Meta {
    page: u32,
    clones: Arc<AtomicUsize>,
}
impl Clone for Meta {
    fn clone(&self) -> Self {
        self.clones.fetch_add(1, Ordering::SeqCst);
        Self {
            page: self.page,
            clones: self.clones.clone(),
        }
    }
}

fn pages(clones: Arc<AtomicUsize>, fail: bool) -> Pages<u32, u32, Meta, &'static str> {
    Pages::new(Resume::Start, move |key| {
        let n = key.unwrap_or(0);
        let result = if fail && n == 1 {
            Err("private error")
        } else {
            Ok(Page {
                items: if n == 1 {
                    vec![]
                } else {
                    vec![n * 10, n * 10 + 1]
                },
                next: (n < 2).then_some(n + 1),
                meta: Meta {
                    page: n,
                    clones: clones.clone(),
                },
            })
        };
        ready(result)
    })
}

#[tokio::test]
async fn metadata_and_completion_move_once_even_for_empty_pages() {
    let clones = Arc::new(AtomicUsize::new(0));
    let mut events = pages(clones.clone(), false).item_events();
    let mut seen = Vec::new();
    loop {
        match events.next().await.unwrap() {
            Some(PageItemEvent::Item(n)) => seen.push(("item", n)),
            Some(PageItemEvent::Page(meta)) => seen.push(("page", meta.page)),
            Some(PageItemEvent::Complete(meta)) => seen.push(("complete", meta.page)),
            None => break,
        }
    }
    assert_eq!(
        seen,
        [
            ("item", 0),
            ("item", 1),
            ("page", 0),
            ("page", 1),
            ("item", 20),
            ("item", 21),
            ("complete", 2)
        ]
    );
    assert_eq!(clones.load(Ordering::SeqCst), 0);
    assert_eq!(events.finish().outcome, Outcome::Exhausted);
}

#[tokio::test]
async fn bounded_failed_and_stopped_have_no_completion() {
    for fail in [false, true] {
        let mut pages = pages(Arc::new(AtomicUsize::new(0)), fail);
        if !fail {
            pages = pages.max_pages(NonZeroUsize::new(1).unwrap());
        }
        let mut events = pages.item_events();
        loop {
            match events.next().await {
                Ok(Some(PageItemEvent::Complete(_))) => panic!("incomplete traversal completed"),
                Ok(Some(_)) => (),
                Ok(None) => panic!("incomplete traversal ended cleanly"),
                Err(error) => {
                    assert_eq!(
                        error.outcome(),
                        if fail {
                            Outcome::Failed
                        } else {
                            Outcome::Bounded
                        }
                    );
                    break;
                }
            }
        }
        assert_eq!(
            events.finish().outcome,
            if fail {
                Outcome::Failed
            } else {
                Outcome::Bounded
            }
        );
    }
    let mut stopped = pages(Arc::new(AtomicUsize::new(0)), false).item_events();
    assert!(matches!(
        stopped.next().await.unwrap(),
        Some(PageItemEvent::Item(0))
    ));
    let report = stopped.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.remainder, [1]);
    assert_eq!(report.resume.unwrap().skip, 1);

    let mut final_page = Pages::new(Resume::<u32>::Start, |_| async {
        Ok::<_, &'static str>(Page {
            items: vec![1],
            next: None,
            meta: "terminal",
        })
    })
    .item_events();
    assert!(matches!(
        final_page.next().await.unwrap(),
        Some(PageItemEvent::Item(1))
    ));
    let report = final_page.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert!(report.remainder.is_empty());
    assert_eq!(report.resume.unwrap().skip, 1);
}

#[tokio::test]
async fn typed_provider_error_access_preserves_resume_and_progress() {
    let mut pages = pages(Arc::new(AtomicUsize::new(0)), true);
    pages.next_page().await.unwrap();
    let error = pages.next_page().await.unwrap_err();
    assert_eq!(error.provider_error(), Some(&"private error"));
    assert!(!format!("{error:?} {error}").contains("private error"));
    assert_eq!(error.resume(), Some(&Resume::At(1)));
    let progress = error.progress();
    let (provider, resume, kept_progress) = error.into_provider_error().unwrap();
    assert_eq!(provider, "private error");
    assert_eq!(resume, Resume::At(1));
    assert_eq!(kept_progress, progress);
    let mut bounded = pages_factory_for_bound();
    bounded.next_page().await.unwrap();
    let error = bounded.next_page().await.unwrap_err();
    assert!(error.provider_error().is_none());
    let error = error.into_provider_error().unwrap_err();
    assert_eq!(error.resume(), Some(&Resume::At(1)));
}
fn pages_factory_for_bound() -> Pages<u32, u32, Meta, &'static str> {
    pages(Arc::new(AtomicUsize::new(0)), false).max_pages(NonZeroUsize::new(1).unwrap())
}
