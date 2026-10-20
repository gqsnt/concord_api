//! Durable pagination checkpoints through the generated public request API.

use std::num::{NonZeroU32, NonZeroU64};
use std::sync::{Arc, Mutex};

use concord::http::SafeProxy;
use concord::observation::{OperationEvent, OperationObserver};
use concord::pagination::{
    Checkpoint, CheckpointErrorKind, ContinuationKnowledge, CursorInit, CursorPagination,
    LinkCursorPagination, OffsetInit, OffsetPagination, PageInit, PagePagination,
    PaginationErrorKind, StopReason,
};
use concord::{CallOptions, Text, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

use super::pagination::{PageJson, VendorPage, VendorPageCodec};
use crate::support::OffsetController;

api! {
    client CheckpointApi {
        timeout 30s;
        base "http://pagination-checkpoint.example.test/v1/";

        scope catalogs(catalog: String) {
            path "catalogs/{catalog}";

            GET items(
                query offset: u64,
                query limit: NonZeroU32,
                query filter: String = "blue",
                header("x-region") region: String = "eu",
                #[sensitive] query private_query?: String,
                query api_key?: String,
                #[sensitive] header("x-private") private_header?: String,
            ) -> Vec<u32> via PageJson {
                id "checkpoint.items";
                path "items";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: limit };
                    cursor -> offset;
                }
            }

            GET alternate(query offset: u64, query limit: NonZeroU32) -> Vec<u32> via PageJson {
                path "alternate";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: limit };
                    cursor -> offset;
                }
            }
        }

        GET ephemeral(query offset: u64, query limit: NonZeroU32) -> Vec<u32> via PageJson {
            path "ephemeral";
            paginate via OffsetController {
                init OffsetController { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }

        POST search(query offset: u64, query limit: NonZeroU32, body payload: String via Text) -> Vec<u32> via PageJson {
            path "search";
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }

        GET linked(query cursor?: String, query filter: String = "blue") -> Vec<u32> via PageJson {
            path "linked";
            paginate via LinkCursorPagination {
                init CursorInit { cursor: cursor };
                cursor -> cursor;
            }
        }

        GET private_size(query offset: u64, #[sensitive] query limit: NonZeroU32) -> Vec<u32> via PageJson {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }

        GET secret(#[sensitive] query cursor?: String) -> VendorPage via VendorPageCodec {
            path "secret";
            paginate via CursorPagination<String> {
                init CursorInit { cursor: cursor };
                cursor -> cursor;
            }
        }
    }
}

api! {
    client OtherCheckpointApi {
        timeout 30s;
        base "http://pagination-checkpoint.example.test/v1/";

        GET items(query page: NonZeroU64, query limit: NonZeroU32) -> Vec<u32> via PageJson {
            id "checkpoint.items";
            path "catalogs/books/items";
            paginate via PagePagination {
                init PageInit { page: page, page_size: limit };
                cursor -> page;
            }
        }
    }
}

fn size(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("positive page size")
}

fn json(body: &str) -> ResponseScript {
    ResponseScript::ok(body.as_bytes().to_vec()).with_header("content-type", "application/json")
}

fn vendor(body: &str) -> ResponseScript {
    ResponseScript::ok(body.as_bytes().to_vec())
        .with_header("content-type", "application/x-concord-page")
}

fn client(server: &ScriptedServer) -> CheckpointApi {
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    CheckpointApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("checkpoint client")
}

#[derive(Default)]
struct RecordedEvents(Mutex<Vec<String>>);

impl OperationObserver for RecordedEvents {
    fn observe(&self, event: OperationEvent<'_>) {
        self.0
            .lock()
            .expect("event capture")
            .push(format!("{event:?}"));
    }
}

#[tokio::test]
async fn checkpoint_before_start_is_inert_and_resumes_the_initial_offset() {
    let server = ScriptedServer::start([json("[41]")]).expect("prestart server");
    let client = client(&server);
    let pages = client.catalogs("books").items(40, size(2)).pages(4);
    let checkpoint = pages.checkpoint().expect("unstarted checkpoint");
    assert_eq!(checkpoint.next_page_index(), 0);
    assert_eq!(server.request_count(), 0);
    assert_eq!(pages.summary().operation_id(), None);
    assert_eq!(pages.summary().stop_reason(), StopReason::NotStarted);
    assert_eq!(pages.summary().pages_received(), 0);
    assert_eq!(pages.summary().application_attempts(), 0);

    let token = checkpoint.to_token();
    assert!(token.is_ascii());
    assert!(token.len() <= 4096);
    let saved = Checkpoint::from_token(&token).expect("portable checkpoint");
    drop(pages);
    let mut resumed = client
        .catalogs("books")
        .items(0, size(2))
        .resume_pages(&saved, 1)
        .expect("resume initial request");
    assert_eq!(server.request_count(), 0, "resume construction is inert");
    let page = resumed
        .next_page()
        .await
        .expect("initial page")
        .expect("page");
    assert_eq!(page.items(), [41]);
    assert_eq!(page.index(), 0);
    assert_eq!(page.metadata().page_index(), 0);
    assert!(server.requests()[0].target().contains("offset=40&limit=2"));
    assert_eq!(resumed.summary().stop_reason(), StopReason::Exhausted);
    assert_eq!(
        resumed.checkpoint().expect_err("exhausted").kind(),
        CheckpointErrorKind::Finished
    );
}

#[tokio::test]
async fn bound_checkpoint_survives_recreation_and_keeps_global_indices_with_fresh_budgets() {
    let server = ScriptedServer::start([json("[1,2]"), json("[3,4]"), json("[5]")])
        .expect("durable checkpoint server");
    let original = client(&server);
    let mut pages = original
        .catalogs("books")
        .items(0, size(2))
        .with_options(&CallOptions::new().cap_sends(1))
        .pages(1);
    let first = pages.next_page().await.expect("first page").expect("page");
    assert_eq!(first.items(), [1, 2]);
    assert_eq!(first.index(), 0);
    let initial_summary = pages.summary();
    assert_eq!(initial_summary.stop_reason(), StopReason::PageBound);
    assert_eq!(
        initial_summary.continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    let token = pages
        .checkpoint()
        .expect("checkpoint at page bound")
        .to_token();
    assert!(pages.next_page().await.expect("bound").is_none());
    assert_eq!(pages.finish().stop_reason(), StopReason::PageBound);
    drop(original);

    let saved = Checkpoint::from_token(&token).expect("reload after original client is dropped");
    assert_eq!(saved.next_page_index(), 1);
    let recreated = client(&server);
    let mut resumed = recreated
        .catalogs("books")
        .items(777, size(2))
        .with_options(&CallOptions::new().cap_sends(2))
        .resume_pages(&saved, 2)
        .expect("fresh traversal");
    assert_eq!(resumed.summary().operation_id(), None);
    assert_eq!(resumed.summary().pages_received(), 0);
    assert_eq!(resumed.summary().application_attempts(), 0);
    assert_eq!(server.request_count(), 1);
    assert_eq!(
        resumed
            .checkpoint()
            .expect("same durable position on recreated client")
            .to_token(),
        token
    );
    let second = resumed
        .next_page()
        .await
        .expect("second page")
        .expect("page");
    assert_eq!(second.items(), [3, 4]);
    assert_eq!(second.index(), 1);
    assert_eq!(second.metadata().page_index(), 1);
    let checkpoint = resumed.checkpoint().expect("checkpoint after resumed page");
    assert_eq!(checkpoint.next_page_index(), 2);
    assert_ne!(
        resumed.summary().operation_id(),
        initial_summary.operation_id()
    );
    assert_eq!(resumed.summary().pages_received(), 1);
    assert_eq!(resumed.summary().pages_delivered(), 1);
    assert_eq!(resumed.summary().items_delivered(), 2);
    assert_eq!(resumed.summary().application_attempts(), 1);
    let third = resumed.next_page().await.expect("last page").expect("page");
    assert_eq!(third.items(), [5]);
    assert_eq!(third.index(), 2);
    assert_eq!(third.metadata().page_index(), 2);
    assert!(resumed.next_page().await.expect("exhausted").is_none());
    assert_eq!(resumed.summary().stop_reason(), StopReason::Exhausted);
    assert_eq!(resumed.summary().pages_received(), 2);
    assert_eq!(resumed.summary().application_attempts(), 2);
    assert_eq!(
        resumed.checkpoint().expect_err("exhausted").kind(),
        CheckpointErrorKind::Finished
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for (request, offset) in requests.iter().zip([0, 2, 4]) {
        assert!(
            request
                .target()
                .contains(&format!("offset={offset}&limit=2"))
        );
    }
}

#[tokio::test]
async fn checkpoint_binding_rejects_changed_public_inputs_before_dispatch() {
    let server = ScriptedServer::start([json("[1,2]")]).expect("binding server");
    let client = client(&server);
    let mut pages = client.catalogs("books").items(0, size(2)).pages(1);
    pages.next_page().await.expect("first page").expect("page");
    let saved = pages.checkpoint().expect("bound checkpoint");

    for error in [
        client
            .catalogs("books")
            .items(0, size(3))
            .resume_pages(&saved, 1)
            .expect_err("changed page size"),
        client
            .catalogs("books")
            .items(0, size(2))
            .set_filter("green")
            .resume_pages(&saved, 1)
            .expect_err("changed filter"),
        client
            .catalogs("books")
            .items(0, size(2))
            .set_region("us")
            .resume_pages(&saved, 1)
            .expect_err("changed public header"),
        client
            .catalogs("games")
            .items(0, size(2))
            .resume_pages(&saved, 1)
            .expect_err("changed path"),
    ] {
        assert_eq!(error.kind(), CheckpointErrorKind::InputMismatch);
    }
    let wrong_endpoint = client
        .catalogs("books")
        .alternate(0, size(2))
        .resume_pages(&saved, 1)
        .expect_err("different endpoint");
    assert_eq!(wrong_endpoint.kind(), CheckpointErrorKind::EndpointMismatch);
    let wrong_controller = OtherCheckpointApi::builder()
        .build()
        .expect("other controller client")
        .items(NonZeroU64::new(1).expect("first page"), size(2))
        .resume_pages(&saved, 1)
        .expect_err("different controller for the same endpoint identity");
    assert_eq!(
        wrong_controller.kind(),
        CheckpointErrorKind::ControllerMismatch
    );
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn failed_fetch_preserves_the_last_delivered_position_for_explicit_resume() {
    let server = ScriptedServer::start([
        json("[1,2]"),
        ResponseScript::status(503, b"unavailable".to_vec()),
        json("[3]"),
    ])
    .expect("failed traversal server");
    let client = client(&server);
    let mut pages = client.catalogs("books").items(0, size(2)).pages(3);
    pages.next_page().await.expect("first page").expect("page");
    let saved = pages.checkpoint().expect("last delivered position");
    let token = saved.to_token();
    let error = pages.next_page().await.expect_err("continuation failure");
    assert_eq!(error.page_index(), Some(1));
    assert_eq!(pages.summary().stop_reason(), StopReason::Failed);
    let failed_checkpoint = pages.checkpoint().expect("last delivered checkpoint");
    assert_eq!(failed_checkpoint.to_token(), token);
    assert_eq!(failed_checkpoint.next_page_index(), 1);
    assert!(
        pages
            .next_page()
            .await
            .expect("failure returned once")
            .is_none()
    );
    assert_eq!(saved.to_token(), token, "a saved checkpoint is immutable");

    let restored =
        Checkpoint::from_token(&failed_checkpoint.to_token()).expect("failure checkpoint");
    let mut resumed = client
        .catalogs("books")
        .items(0, size(2))
        .resume_pages(&restored, 1)
        .expect("retry from explicit prior position");
    let page = resumed
        .next_page()
        .await
        .expect("retried page")
        .expect("page");
    assert_eq!(page.items(), [3]);
    assert_eq!(page.index(), 1);
    assert_eq!(resumed.summary().application_attempts(), 1);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].target().contains("offset=2&limit=2"));
    assert!(requests[2].target().contains("offset=2&limit=2"));
}

#[tokio::test]
async fn first_page_failure_can_checkpoint_and_retry_without_an_earlier_saved_token() {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"unavailable".to_vec()),
        json("[41]"),
    ])
    .expect("first-page failure server");
    let client = client(&server);
    let mut pages = client.catalogs("books").items(40, size(2)).pages(3);
    let error = pages.next_page().await.expect_err("first page fails");
    assert_eq!(error.page_index(), Some(0));
    assert_eq!(pages.summary().stop_reason(), StopReason::Failed);
    assert_eq!(pages.summary().pages_received(), 0);
    assert_eq!(pages.summary().pages_delivered(), 0);
    let failed_operation = pages.summary().operation_id();
    let saved = pages.checkpoint().expect("initial failed request position");
    assert_eq!(saved.next_page_index(), 0);
    assert_eq!(server.request_count(), 1);
    let saved = Checkpoint::from_token(&saved.to_token()).expect("reload failed position");
    let mut resumed = client
        .catalogs("books")
        .items(0, size(2))
        .resume_pages(&saved, 1)
        .expect("retry initial cursor from checkpoint");
    assert_eq!(server.request_count(), 1);
    let page = resumed
        .next_page()
        .await
        .expect("retried first page")
        .expect("page");
    assert_eq!(page.index(), 0);
    assert_eq!(page.metadata().page_index(), 0);
    assert_eq!(page.items(), [41]);
    assert_ne!(resumed.summary().operation_id(), failed_operation);
    assert_eq!(resumed.summary().application_attempts(), 1);
    assert_eq!(resumed.summary().stop_reason(), StopReason::Exhausted);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| request.target().contains("offset=40&limit=2"))
    );
}

#[tokio::test]
async fn restored_link_controller_rejects_cross_origin_and_changed_path_continuations() {
    for (link, kind) in [
        (
            "<http://elsewhere.example.test/v1/linked?filter=blue&cursor=other>; rel=next",
            PaginationErrorKind::CrossOriginContinuation,
        ),
        (
            "</v1/other?filter=blue&cursor=other>; rel=next",
            PaginationErrorKind::InvalidContinuation,
        ),
    ] {
        let server = ScriptedServer::start([
            json("[1]").with_header("link", "</v1/linked?filter=blue&cursor=next>; rel=next"),
            json("[2]").with_header("link", link),
        ])
        .expect("restored link server");
        let client = client(&server);
        let mut pages = client.linked().pages(1);
        let first = pages
            .next_page()
            .await
            .expect("valid link page")
            .expect("page");
        assert_eq!(first.items(), [1]);
        let saved = pages.checkpoint().expect("link continuation checkpoint");
        assert_eq!(saved.next_page_index(), 1);
        let token = saved.to_token();
        let restored = Checkpoint::from_token(&token).expect("restore link state");
        let mut resumed = client
            .linked()
            .resume_pages(&restored, 2)
            .expect("resume link cursor");
        let error = resumed
            .next_page()
            .await
            .expect_err("invalid next link withholds the page");
        assert_eq!(error.pagination_kind(), Some(kind), "{link}");
        assert_eq!(error.page_index(), Some(1));
        assert_eq!(resumed.summary().stop_reason(), StopReason::Failed);
        assert_eq!(resumed.summary().pages_received(), 1);
        assert_eq!(resumed.summary().pages_delivered(), 0);
        assert_eq!(resumed.summary().items_delivered(), 0);
        assert_eq!(
            resumed
                .checkpoint()
                .expect("invalid continuation cannot advance checkpoint")
                .to_token(),
            token
        );
        assert!(
            resumed
                .next_page()
                .await
                .expect("failure returned once")
                .is_none()
        );
        let requests = server.requests();
        assert_eq!(
            requests.len(),
            2,
            "the invalid link must never be dispatched"
        );
        assert!(
            requests[1]
                .target()
                .contains("/v1/linked?cursor=next&filter=blue")
        );
        assert!(!requests[1].target().contains("elsewhere"));
        assert!(!requests[1].target().contains("/v1/other"));
    }
}

#[tokio::test]
async fn zero_bound_resume_performs_no_fetch_and_preserves_the_saved_position() {
    let server = ScriptedServer::start([json("[1,2]")]).expect("zero bound server");
    let client = client(&server);
    let mut pages = client.catalogs("books").items(0, size(2)).pages(1);
    pages.next_page().await.expect("first page").expect("page");
    let saved = pages.checkpoint().expect("checkpoint");
    let mut resumed = client
        .catalogs("books")
        .items(0, size(2))
        .resume_pages(&saved, 0)
        .expect("zero bound resume");
    assert!(resumed.next_page().await.expect("zero bound").is_none());
    assert_eq!(resumed.summary().stop_reason(), StopReason::PageBound);
    assert_eq!(resumed.summary().operation_id(), None);
    assert_eq!(resumed.summary().application_attempts(), 0);
    assert_eq!(resumed.summary().pages_received(), 0);
    assert_eq!(
        resumed.checkpoint().expect("same position").to_token(),
        saved.to_token()
    );
    assert_eq!(server.request_count(), 1);
    let zero = client.catalogs("books").items(0, size(2)).pages(0);
    assert_eq!(
        zero.checkpoint()
            .expect("initial zero-bound position")
            .next_page_index(),
        0
    );
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn sensitive_cursor_checkpoint_is_rejected_without_leaking_the_cursor() {
    const SECRET: &str = "checkpoint-sensitive-cursor-78bf";
    let server = ScriptedServer::start([vendor(&format!("1|{SECRET}")), vendor("2|-")])
        .expect("sensitive cursor server");
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let events = Arc::new(RecordedEvents::default());
    let observer: Arc<dyn OperationObserver> = events.clone();
    let client = CheckpointApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.operation_observer(observer))
        .build()
        .expect("sensitive checkpoint client");
    let mut pages = client.secret().pages(3);
    let before = pages
        .checkpoint()
        .expect_err("sensitive cursor before start");
    assert_eq!(before.kind(), CheckpointErrorKind::SensitiveCursor);
    assert_eq!(server.request_count(), 0);
    let first = pages.next_page().await.expect("first page").expect("page");
    assert_eq!(first.items(), [1]);
    let error = pages
        .checkpoint()
        .expect_err("sensitive cursor continuation");
    assert_eq!(error.kind(), CheckpointErrorKind::SensitiveCursor);
    assert_eq!(server.request_count(), 1, "checkpoint never fetches");
    let debug = format!("{client:?} {pages:?} {first:?} {error} {error:?} {before:?}");
    let second = pages.next_page().await.expect("second page").expect("page");
    assert_eq!(
        second.items(),
        [2],
        "failed checkpoint does not change traversal"
    );
    let summary = pages.finish();
    assert!(server.requests()[1].target().contains(SECRET));
    let corpus = format!("{debug} {summary:?} {:?}", events.0.lock().expect("events"));
    assert!(
        !corpus.contains(SECRET),
        "sensitive cursor leaked: {corpus}"
    );
}

#[test]
fn token_parser_rejects_invalid_input_with_value_free_errors() {
    for invalid in ["", "x", "zz", "private-token-sentinel", "00"] {
        let error = Checkpoint::from_token(invalid).expect_err("malformed token");
        assert_eq!(error.kind(), CheckpointErrorKind::Malformed);
        assert!(!format!("{error} {error:?}").contains("private-token-sentinel"));
    }
    let error = Checkpoint::from_token(&"0".repeat(4097)).expect_err("oversized token");
    assert_eq!(error.kind(), CheckpointErrorKind::TooLarge);
}

#[tokio::test]
async fn ordinary_custom_controllers_remain_pageable_without_checkpoint_opt_in() {
    let server = ScriptedServer::start([json("[7]")]).expect("custom controller server");
    let client = client(&server);
    let mut pages = client.ephemeral(0, size(2)).pages(2);
    let error = pages
        .checkpoint()
        .expect_err("custom controller has no durable state contract");
    assert_eq!(error.kind(), CheckpointErrorKind::NotResumable);
    assert_eq!(server.request_count(), 0);
    assert_eq!(pages.summary().operation_id(), None);
    let page = pages
        .next_page()
        .await
        .expect("ordinary traversal")
        .expect("page");
    assert_eq!(page.items(), [7]);
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn checkpoint_binds_the_buffered_body_without_serializing_body_bytes() {
    let server = ScriptedServer::start([json("[9]")]).expect("body checkpoint server");
    let client = client(&server);
    let body = "private-body-sentinel".repeat(1024);
    let pages = client.search(0, size(2), body.clone()).pages(1);
    let saved = pages
        .checkpoint()
        .expect("bodyless token for a body-bearing request");
    let different_body = client
        .search(0, size(2), "another body".to_owned())
        .pages(1)
        .checkpoint()
        .expect("body is recreated by the application");
    assert_ne!(
        saved.to_token(),
        different_body.to_token(),
        "the body contributes to the request fingerprint"
    );
    let changed_body = client
        .search(0, size(2), "another body".to_owned())
        .resume_pages(&saved, 1)
        .expect_err("changed buffered body");
    assert_eq!(changed_body.kind(), CheckpointErrorKind::InputMismatch);
    assert!(saved.to_token().len() <= 4096);
    assert!(!format!("{saved:?} {pages:?}").contains("private-body-sentinel"));
    assert_eq!(server.request_count(), 0);
    let mut resumed = client
        .search(0, size(2), body.clone())
        .resume_pages(&saved, 1)
        .expect("recreate the original semantic body");
    assert_eq!(server.request_count(), 0);
    let page = resumed.next_page().await.expect("body page").expect("page");
    assert_eq!(page.items(), [9]);
    assert_eq!(server.requests()[0].body(), body.as_bytes());
}

#[tokio::test]
async fn noncursor_sensitive_values_do_not_bind_or_appear_in_tokens_and_resume_uses_new_values() {
    let server =
        ScriptedServer::start([json("[1,2]"), json("[3]")]).expect("noncursor secrets server");
    let client = client(&server);
    let fresh_checkpoint = |query: &str, header: &str| {
        client
            .catalogs("books")
            .items(0, size(2))
            .set_private_query(query)
            .set_private_header(header)
            .pages(1)
            .checkpoint()
            .expect("unstarted checkpoint")
    };
    let absent = client
        .catalogs("books")
        .items(0, size(2))
        .pages(1)
        .checkpoint()
        .expect("absent secrets");
    let categorized = client
        .catalogs("books")
        .items(0, size(2))
        .set_api_key("credential-query-value")
        .pages(1)
        .checkpoint()
        .expect("credential category");
    assert_eq!(
        absent.to_token(),
        categorized.to_token(),
        "credential-category presence is excluded"
    );
    let first = fresh_checkpoint("query-secret-old", "header-secret-old");
    assert_eq!(
        absent.to_token(),
        first.to_token(),
        "annotated sensitive presence is excluded"
    );
    let changed = fresh_checkpoint("query-secret-new", "header-secret-new");
    assert_eq!(
        first.to_token(),
        changed.to_token(),
        "sensitive noncursor values are excluded from token binding"
    );

    let mut pages = client
        .catalogs("books")
        .items(0, size(2))
        .set_private_query("query-secret-old")
        .set_private_header("header-secret-old")
        .pages(1);
    pages.next_page().await.expect("first page").expect("page");
    let saved = pages
        .checkpoint()
        .expect("checkpoint with public cursor and secret inputs");
    let restored = Checkpoint::from_token(&saved.to_token()).expect("portable checkpoint");
    let mut resumed = client
        .catalogs("books")
        .items(0, size(2))
        .set_private_query("query-secret-new")
        .set_private_header("header-secret-new")
        .resume_pages(&restored, 1)
        .expect("new sensitive values are allowed");
    let page = resumed
        .next_page()
        .await
        .expect("resumed page")
        .expect("page");
    assert_eq!(page.items(), [3]);
    assert_eq!(page.index(), 1);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0]
            .target()
            .contains("private_query=query-secret-old")
    );
    assert_eq!(requests[0].header("x-private"), Some("header-secret-old"));
    assert!(
        requests[1]
            .target()
            .contains("private_query=query-secret-new")
    );
    assert_eq!(requests[1].header("x-private"), Some("header-secret-new"));
    let corpus = format!("{first:?} {changed:?} {saved:?} {restored:?} {resumed:?}");
    for secret in [
        "query-secret-old",
        "query-secret-new",
        "header-secret-old",
        "header-secret-new",
    ] {
        assert!(
            !corpus.contains(secret),
            "secret in checkpoint diagnostics: {corpus}"
        );
        assert!(!saved.to_token().contains(secret));
    }
}

#[cfg(feature = "default-tls")]
mod credentials {
    use concord::SecretString;
    use concord::auth::AccessToken;
    use concord_test_support::proxy::ConnectProxy;
    use concord_test_support::tls::TlsServer;

    use super::*;

    api! {
        client CredentialCheckpointApi {
            timeout 30s;
            base "https://localhost/v1/";
            credentials {
                secret bearer_secret;
                credential token = bearer(secret.bearer_secret);
            }
            auth { bearer token; }

            GET items(query offset: u64, query limit: NonZeroU32) -> Vec<u32> via PageJson {
                path "items";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: limit };
                    cursor -> offset;
                }
            }
        }
    }

    #[tokio::test]
    async fn credential_replacement_keeps_the_same_token_and_resume_sends_the_current_credential() {
        const OLD: &str = "checkpoint-bearer-old-9ac1";
        const NEW: &str = "checkpoint-bearer-new-a32d";
        let server = TlsServer::start().expect("credential checkpoint TLS server");
        let tunnel = ConnectProxy::start(server.address()).expect("CONNECT tunnel");
        for body in ["[1,2]", "[3]"] {
            server
                .queue_response_with_headers(
                    200,
                    &[("content-type", "application/json")],
                    body.as_bytes().to_vec(),
                )
                .expect("queued credential page");
        }
        let proxy = SafeProxy::parse(tunnel.url()).expect("loopback proxy");
        let certificate = server.certificate_pem();
        let client = CredentialCheckpointApi::builder()
            .bearer_secret(SecretString::try_new(OLD).expect("bounded old secret"))
            .configure_http(move |http| {
                http.http1_only()
                    .proxy(proxy)
                    .add_root_certificate_pem(certificate)
            })
            .build()
            .expect("credential checkpoint client");
        let mut pages = client.items(0, size(2)).pages(1);
        let first = pages
            .next_page()
            .await
            .expect("old credential page")
            .expect("page");
        assert_eq!(first.items(), [1, 2]);
        let before = pages.checkpoint().expect("checkpoint with old credential");
        client
            .credentials()
            .replace_token(AccessToken::try_new(NEW).expect("bounded new credential"))
            .expect("replace bearer credential");
        let after = pages
            .checkpoint()
            .expect("checkpoint with replacement credential");
        assert_eq!(
            before.to_token(),
            after.to_token(),
            "credential bytes cannot affect a checkpoint"
        );
        assert_eq!(server.requests().len(), 1, "checkpointing does not send");

        let restored =
            Checkpoint::from_token(&before.to_token()).expect("reload pre-replacement token");
        let mut resumed = client
            .items(0, size(2))
            .resume_pages(&restored, 1)
            .expect("resume after credential replacement");
        assert_eq!(
            resumed
                .checkpoint()
                .expect("same position after restore")
                .to_token(),
            before.to_token()
        );
        let page = resumed
            .next_page()
            .await
            .expect("replacement credential page")
            .expect("page");
        assert_eq!(page.items(), [3]);
        assert_eq!(page.index(), 1);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].header("authorization"),
            Some(format!("Bearer {OLD}").as_str())
        );
        assert_eq!(
            requests[1].header("authorization"),
            Some(format!("Bearer {NEW}").as_str())
        );
        let corpus = format!("{client:?} {before:?} {after:?} {restored:?} {resumed:?}");
        assert!(!corpus.contains(OLD));
        assert!(!corpus.contains(NEW));
    }
}

#[test]
fn sensitive_controller_initialization_cannot_enter_checkpoint_state() {
    let client = CheckpointApi::builder().build().expect("inert client");
    let pages = client.private_size(0, size(23)).pages(2);
    let error = pages
        .checkpoint()
        .expect_err("private controller initialization");
    assert_eq!(error.kind(), CheckpointErrorKind::SensitiveCursor);
    assert_eq!(pages.summary().operation_id(), None);
}
