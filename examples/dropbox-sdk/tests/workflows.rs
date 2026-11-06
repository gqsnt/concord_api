//! Public client workflows and honest partial outcomes against offline fixtures.

mod support;

use concord::bytes::Bytes;
use concord_example_testserver::{Body, Script, TestServer};
use concord_sdk_dropbox::{
    Entry, ListFolderOptions, ProviderErrorKind, TaggedError,
    pagination::{Outcome, PagesError, Resume},
};
use futures_util::StreamExt;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use support::*;

#[tokio::test]
async fn workflow_a_lists_member_folder_with_page_metadata_and_exact_exhaustion() {
    let script = Script::new([
        json(200, A_FIRST, "synthetic-a-first"),
        json(200, A_LAST, "synthetic-a-last"),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let mut pages = client(&server)
        .list_folder_pages(context_a(), ListFolderOptions::new("/Projects"))
        .max_pages(NonZeroUsize::new(2).unwrap());
    assert!(server.requests().is_empty());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.meta.http.request_id(), Some("synthetic-a-first"));
    assert!(first.next.is_some());
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(last.items.len(), 2);
    assert!(matches!(last.items[1], Entry::Deleted(_)));
    assert_eq!(last.meta.http.request_id(), Some("synthetic-a-last"));
    assert_eq!(last.meta.cursor.context(), &context_a());
    assert!(!last.meta.has_more);
    assert!(last.next.is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_request(
        &requests[0],
        MEMBER_A,
        r#"{".tag":"root","root":"100"}"#,
        "/2/files/list_folder",
        include_bytes!("../fixtures/request-list-projects.json"),
    );
    assert_request(
        &requests[1],
        MEMBER_A,
        r#"{".tag":"root","root":"100"}"#,
        "/2/files/list_folder/continue",
        include_bytes!("../fixtures/request-continue-a.json"),
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn workflow_b_changes_member_and_root_while_a_is_in_flight_without_leaking_context() {
    // The generator is synchronous, so it must never wait for another request. Empty chunks
    // write no bytes; the server's awaited inter-chunk delay yields to the client and B.
    // Only observing B releases A's body. The delay does not determine request ordering.
    // Use a single-thread runtime to ensure the gate needs no spare worker to make progress.
    let gate = Arc::new(AtomicBool::new(false));
    let server_gate = gate.clone();
    let server = TestServer::start(move |request| {
        let continuation = request.target.ends_with("/continue");
        match (request.header("dropbox-api-select-user"), continuation) {
            (Some(MEMBER_A), false) => {
                let gate = server_gate.clone();
                let mut body = Some(Bytes::from_static(A_FIRST));
                json(200, b"", "synthetic-a-first").with_body(Body::Generate(
                    Box::new(move || {
                        if gate.load(Ordering::Acquire) {
                            body.take()
                        } else {
                            Some(Bytes::new())
                        }
                    }),
                    Some(Duration::from_millis(1)),
                ))
            }
            (Some(MEMBER_A), true) => json(200, A_LAST, "synthetic-a-last"),
            (Some(MEMBER_B), false) => {
                server_gate.store(true, Ordering::Release);
                json(200, B_FIRST, "synthetic-b-first")
            }
            (Some(MEMBER_B), true) => json(200, B_LAST, "synthetic-b-last"),
            _ => panic!("unexpected selected user"),
        }
    })
    .await;
    let api = client(&server);
    let mut selected = context_a();
    let first = api.list_folder_pages(selected.clone(), ListFolderOptions::new("/Projects"));
    let pending_a = tokio::spawn(first.collect_complete());
    assert!(
        server
            .wait_until(Duration::from_secs(2), |s| {
                s.requests().len() == 1 && s.active_bodies() == 1
            })
            .await
    );
    assert!(!gate.load(Ordering::Acquire));
    assert_eq!(server.body_bytes_sent(), 0);
    assert!(!pending_a.is_finished());
    selected = context_b(); // Both the selected member and the root mode/ID change now.
    let b = api
        .list_folder_pages(selected, ListFolderOptions::new("/Projects"))
        .collect_complete()
        .await
        .unwrap();
    let a = pending_a.await.unwrap().unwrap();
    assert!(gate.load(Ordering::Acquire));
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 2);
    assert_eq!(a[0].items[0].path().name, "a-plan.txt");
    assert!(b[0].items.is_empty()); // Still continued to B's terminal page.
    assert_eq!(b[1].items[0].path().name, "B only");
    for (pages, context, prefix) in [
        (&a, context_a(), "synthetic-a"),
        (&b, context_b(), "synthetic-b"),
    ] {
        for page in pages {
            assert_eq!(page.meta.cursor.context(), &context);
            assert!(page.meta.http.request_id().unwrap().starts_with(prefix));
        }
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for request in &requests {
        let continuing = request.target.ends_with("/continue");
        let (member, root, continuation) = match request.header("dropbox-api-select-user") {
            Some(MEMBER_A) => (
                MEMBER_A,
                r#"{".tag":"root","root":"100"}"#,
                include_bytes!("../fixtures/request-continue-a.json").as_slice(),
            ),
            Some(MEMBER_B) => (
                MEMBER_B,
                r#"{".tag":"namespace_id","namespace_id":"200"}"#,
                include_bytes!("../fixtures/request-continue-b.json").as_slice(),
            ),
            _ => panic!("unexpected context"),
        };
        assert_request(
            request,
            member,
            root,
            if continuing {
                "/2/files/list_folder/continue"
            } else {
                "/2/files/list_folder"
            },
            if continuing {
                continuation
            } else {
                include_bytes!("../fixtures/request-list-projects.json")
            },
        );
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn one_context_failure_does_not_poison_another_listing() {
    let server = TestServer::start(|r| match r.header("dropbox-api-select-user") {
        Some(MEMBER_A) => json(
            422,
            include_bytes!("../fixtures/invalid-root.json"),
            "synthetic-a-root-failure",
        ),
        Some(MEMBER_B) => json(200, B_LAST, "synthetic-b-success"),
        _ => panic!("unexpected context"),
    })
    .await;
    let api = client(&server);
    let (a, b) = tokio::join!(
        api.list_folder(context_a(), ListFolderOptions::default()),
        api.list_folder(context_b(), ListFolderOptions::default()),
    );
    assert_eq!(
        a.unwrap_err().meta().unwrap().request_id(),
        Some("synthetic-a-root-failure")
    );
    let b = b.unwrap();
    assert_eq!(b.meta.request_id(), Some("synthetic-b-success"));
    assert_eq!(b.value.entries[0].path().name, "B only");
    assert_eq!(b.value.cursor.context(), &context_b());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_continuation_returns_collected_prefix_and_context_bound_failed_cursor() {
    let script = Script::new([
        json(200, A_FIRST, "synthetic-prefix"),
        json(
            409,
            include_bytes!("../fixtures/reset.json"),
            "synthetic-failed-page",
        ),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let incomplete = client(&server)
        .list_folder_pages(context_a(), ListFolderOptions::default())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].path().name, "a-plan.txt");
    let PagesError::Provider {
        error,
        failed,
        progress,
        ..
    } = incomplete.error
    else {
        panic!("expected provider failure")
    };
    assert_eq!(
        error.meta().unwrap().request_id(),
        Some("synthetic-failed-page")
    );
    assert!(matches!(
        error.provider().unwrap().kind,
        ProviderErrorKind::Tagged(TaggedError::Reset)
    ));
    let Resume::At(cursor) = failed else {
        panic!("expected failed continuation")
    };
    assert_eq!(cursor.context(), &context_a());
    assert_eq!(progress.succeeded, 1);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn page_cap_is_incomplete_then_ended_and_retains_resume_context() {
    let server = TestServer::start(|_| json(200, A_FIRST, "synthetic-bounded")).await;
    let mut pages = client(&server)
        .list_folder_pages(context_a(), ListFolderOptions::default())
        .max_pages(NonZeroUsize::new(1).unwrap());
    assert!(pages.next_page().await.unwrap().is_some());
    let PagesError::Bounded { resume, .. } = pages.next_page().await.unwrap_err() else {
        panic!("expected cap")
    };
    let Resume::At(cursor) = resume else {
        panic!("expected continuation")
    };
    assert_eq!(cursor.context(), &context_a());
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Bounded,
            ..
        })
    ));
    assert_eq!(pages.finish().outcome, Outcome::Bounded);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn item_stream_yields_typed_failure_before_ending() {
    let script = Script::new([
        json(200, A_FIRST, "synthetic-first"),
        json(
            429,
            include_bytes!("../fixtures/rate-limited.json"),
            "synthetic-stream-rate",
        ),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let mut stream = client(&server)
        .list_folder_pages(context_a(), ListFolderOptions::default())
        .items()
        .into_stream();
    assert_eq!(
        stream.next().await.unwrap().unwrap().path().name,
        "a-plan.txt"
    );
    let PagesError::Provider { error, .. } = stream.next().await.unwrap().unwrap_err() else {
        panic!("expected failure")
    };
    assert_eq!(
        error.meta().unwrap().request_id(),
        Some("synthetic-stream-rate")
    );
    assert_eq!(error.meta().unwrap().status.as_u16(), 429);
    assert!(stream.next().await.is_none());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn dropping_a_pending_poll_resumes_same_request_and_stopping_is_interrupted() {
    let server = TestServer::start(|_| {
        json(200, A_FIRST, "synthetic-delayed").with_body(Body::Chunks(vec![(
            Duration::from_millis(100),
            Bytes::from_static(A_FIRST),
        )]))
    })
    .await;
    let mut pages = client(&server).list_folder_pages(context_a(), ListFolderOptions::default());
    assert!(
        tokio::time::timeout(Duration::from_millis(10), pages.next_page())
            .await
            .is_err()
    );
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].path().name, "a-plan.txt");
    let report = pages.finish(); // Caller stopped before requesting the known next page.
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}
