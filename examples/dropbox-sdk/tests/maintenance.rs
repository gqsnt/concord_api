//! Selection changes while an initial or continuation response is still in flight.

mod support;

use concord::{bytes::Bytes, reqwest::StatusCode};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_dropbox::{
    FolderPage, InputError, ListFolderOptions, PathRoot, ProviderErrorKind, RequestContext,
    Response, TaggedError,
};
use std::{
    future::{Future, poll_fn},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
    time::Duration,
};
use support::*;

const ROOT_A: &str = r#"{".tag":"root","root":"100"}"#;
const ROOT_B: &str = r#"{".tag":"namespace_id","namespace_id":"200"}"#;

/// Release on drop too, so a failed assertion cannot strand a server body.
#[derive(Default)]
struct Gate(Arc<AtomicBool>);

impl Gate {
    fn hold(open: Arc<AtomicBool>, reply: Reply, bytes: &'static [u8]) -> Reply {
        let mut body = Some(Bytes::from_static(bytes));
        reply.with_body(Body::Generate(
            Box::new(move || {
                if open.load(Ordering::Acquire) {
                    body.take()
                } else {
                    Some(Bytes::new())
                }
            }),
            // Yield between empty chunks; the synchronous generator must never block.
            Some(Duration::from_millis(1)),
        ))
    }

    fn release(&self) {
        self.0.store(true, Ordering::Release);
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        self.release();
    }
}

fn assert_page(
    response: &Response<FolderPage>,
    context: &RequestContext,
    fixture: &[u8],
    request_id: &str,
    continuing: bool,
) {
    let expected: serde_json::Value = serde_json::from_slice(fixture).unwrap();
    assert_eq!(response.meta.status, StatusCode::OK);
    assert_eq!(response.meta.request_id(), Some(request_id));
    assert_eq!(
        response.meta.operation.unwrap().name(),
        if continuing {
            "DropboxApi.selected.list_folder_continue"
        } else {
            "DropboxApi.selected.list_folder"
        }
    );
    assert_eq!(response.value.cursor.context(), context);
    assert_eq!(
        response.value.cursor.as_str(),
        expected["cursor"].as_str().unwrap()
    );
    assert_eq!(
        response.value.has_more,
        expected["has_more"].as_bool().unwrap()
    );
    let names: Vec<_> = response
        .value
        .entries
        .iter()
        .map(|entry| entry.path().name.as_str())
        .collect();
    let expected_names: Vec<_> = expected["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, expected_names);
}

async fn overlap(
    changed: RequestContext,
    expected_root: &'static str,
    hold_continuation: bool,
    fail_changed: bool,
) {
    let gate = Gate::default();
    let server_gate = gate.0.clone();
    let server = TestServer::start(move |request| {
        let continuing = request.target.ends_with("/continue");
        if request.header("dropbox-api-select-user") == Some(MEMBER_A)
            && request.header("dropbox-api-path-root") == Some(ROOT_A)
        {
            let (body, id) = if continuing {
                (A_LAST, "synthetic-a-last")
            } else {
                (A_FIRST, "synthetic-a-first")
            };
            let reply = json(200, body, id);
            if continuing == hold_continuation {
                Gate::hold(server_gate.clone(), reply, body)
            } else {
                reply
            }
        } else if fail_changed {
            json(
                422,
                include_bytes!("../fixtures/invalid-root.json"),
                "synthetic-b-root-error",
            )
        } else if continuing {
            json(200, B_LAST, "synthetic-b-last")
        } else {
            json(200, B_FIRST, "synthetic-b-first")
        }
    })
    .await;
    let api = client(&server);
    let mut selected = context_a();
    let options = ListFolderOptions::new("/Projects");
    let cursor = if hold_continuation {
        let first = api
            .list_folder(selected.clone(), options.clone())
            .await
            .unwrap();
        assert_page(&first, &context_a(), A_FIRST, "synthetic-a-first", false);
        Some(first.value.cursor)
    } else {
        None
    };
    let original = selected.clone();
    let pending = async {
        match &cursor {
            Some(cursor) => api.list_folder_continue(cursor).await,
            None => api.list_folder(original, options.clone()).await,
        }
    };
    tokio::pin!(pending);

    // Poll A until the server has its complete request and is holding its response body.
    // The gate, rather than elapsed time or task scheduling, determines completion order.
    tokio::select! {
        result = &mut pending => panic!("A completed before selection changed: {result:?}"),
        observed = server.wait_until(Duration::from_secs(2), |s| {
            s.requests().len() == usize::from(hold_continuation) + 1 && s.active_bodies() == 1
        }) => assert!(observed, "A must be in flight before selection changes"),
    }

    selected = selected
        .with_member(changed.member_id())
        .unwrap()
        .with_path_root(changed.path_root().clone());
    tokio::time::timeout(Duration::from_secs(2), async {
        let b = api.list_folder(selected, options.clone()).await;
        if fail_changed {
            let error = b.unwrap_err();
            let meta = error.meta().unwrap();
            assert_eq!(meta.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(meta.request_id(), Some("synthetic-b-root-error"));
            assert_eq!(
                meta.operation.unwrap().name(),
                "DropboxApi.selected.list_folder"
            );
            assert!(matches!(
                &error.provider().unwrap().kind,
                ProviderErrorKind::Tagged(TaggedError::InvalidRoot { invalid_root })
                    if invalid_root.root_namespace_id == "300"
            ));
        } else {
            let b = b.unwrap();
            assert_page(&b, &changed, B_FIRST, "synthetic-b-first", false);
            let last = api.list_folder_continue(&b.value.cursor).await.unwrap();
            assert_page(&last, &changed, B_LAST, "synthetic-b-last", true);
        }
    })
    .await
    .expect("B must finish while A's response body is held");
    assert!(!gate.0.load(Ordering::Acquire));
    poll_fn(|cx| {
        assert!(pending.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    gate.release();
    let a = tokio::time::timeout(Duration::from_secs(2), pending)
        .await
        .expect("A must finish after release")
        .unwrap();
    if hold_continuation {
        assert_page(&a, &context_a(), A_LAST, "synthetic-a-last", true);
    } else {
        assert_page(&a, &context_a(), A_FIRST, "synthetic-a-first", false);
        // Even after B completed, A's opaque cursor still selects A's original context.
        let last = api.list_folder_continue(&a.value.cursor).await.unwrap();
        assert_page(&last, &context_a(), A_LAST, "synthetic-a-last", true);
    }

    let requests = server.requests();
    let (mut a_count, mut b_count) = (0, 0);
    for request in &requests {
        let continuing = request.target.ends_with("/continue");
        let (member, root, body) = if request.header("dropbox-api-select-user") == Some(MEMBER_A)
            && request.header("dropbox-api-path-root") == Some(ROOT_A)
        {
            a_count += 1;
            (
                MEMBER_A,
                ROOT_A,
                include_bytes!("../fixtures/request-continue-a.json").as_slice(),
            )
        } else {
            b_count += 1;
            (
                changed.member_id(),
                expected_root,
                include_bytes!("../fixtures/request-continue-b.json").as_slice(),
            )
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
                body
            } else {
                include_bytes!("../fixtures/request-list-projects.json")
            },
        );
    }
    assert_eq!(a_count, 2);
    assert_eq!(b_count, if fail_changed { 1 } else { 2 });
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn user_and_root_change_while_initial_response_is_in_flight() {
    overlap(context_b(), ROOT_B, false, false).await;
}

#[tokio::test]
async fn user_and_root_change_while_continuation_response_is_in_flight() {
    overlap(context_b(), ROOT_B, true, false).await;
}

#[tokio::test]
async fn changing_only_user_or_only_root_keeps_responses_isolated() {
    overlap(
        RequestContext::new(MEMBER_B, PathRoot::root("100").unwrap()).unwrap(),
        ROOT_A,
        false,
        false,
    )
    .await;
    overlap(
        RequestContext::new(MEMBER_A, PathRoot::namespace("200").unwrap()).unwrap(),
        ROOT_B,
        false,
        false,
    )
    .await;
    overlap(
        RequestContext::new(MEMBER_A, PathRoot::Home).unwrap(),
        r#"{".tag":"home"}"#,
        false,
        false,
    )
    .await;
}

#[tokio::test]
async fn changed_selections_typed_failure_does_not_change_a_pending_response() {
    for hold_continuation in [false, true] {
        overlap(
            RequestContext::new(MEMBER_B, PathRoot::root("200").unwrap()).unwrap(),
            r#"{".tag":"root","root":"200"}"#,
            hold_continuation,
            true,
        )
        .await;
    }
}

#[test]
fn changing_member_reuses_validation_without_disclosing_rejected_values() {
    let original = context_a();
    for member in [
        "",
        "dbmid:has space",
        "dbmid:Équipe",
        "dbmid:SYNTHETIC\r\nInjected: yes",
    ] {
        let error = original.clone().with_member(member).unwrap_err();
        assert_eq!(error, InputError::MemberId);
        assert_eq!(error.to_string(), "invalid Dropbox input: MemberId");
    }
    assert_eq!(original.member_id(), MEMBER_A);
    assert_eq!(original.path_root(), &PathRoot::root("100").unwrap());
}
