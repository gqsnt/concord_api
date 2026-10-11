#![forbid(unsafe_code)]
//! The §23 application integration examples executed against the canonical
//! Catalog over a verified TLS loopback (declared fixed HTTPS base, CONNECT
//! tunnel, owned fixture trust root). Only fake sentinels are used.
#![cfg(all(feature = "json", feature = "rate-limit", feature = "default-tls"))]

mod support;

use std::future::{Future, IntoFuture};
use std::num::NonZeroU32;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use concord::error::ErrorCategory;
use concord::pagination::{ContinuationKnowledge, StopReason};
use concord::response::StreamState;
use concord::{CallOptions, DispatchKnowledge, StoreOutcome};
use concord_examples::application::construction::{self, SignIn};
use concord_examples::application::execute::execute;
use concord_examples::application::export::{ExportPrefix, export_prefix};
use concord_examples::application::reconciliation::{
    CommandId, TitleChange, WriteFailure, change_title, classify,
};
use concord_examples::application::saved_state::{
    CliListPatch, SavedList, configured_list, list_request,
};
use concord_examples::application::search::{SearchResult, find_summary, find_summary_within};
use concord_examples::contracts::catalog_v3::{Item, ItemSummary, LoginInput};
use support::CatalogHarness;

const SESSION: &str = "session-sentinel-3f9e";

fn limits() -> CallOptions {
    CallOptions::new()
        .cap_timeout(Duration::from_secs(10))
        .expect("positive cap")
        .cap_sends(3)
}

fn summary(id: &str) -> ItemSummary {
    ItemSummary { id: id.to_owned() }
}

/// Signs in through the §23.1 helper and returns the harness.
async fn signed_in() -> CatalogHarness {
    let harness = CatalogHarness::start();
    harness.token(SESSION);
    let outcome = construction::sign_in(&harness.client, "user".to_owned())
        .await
        .expect("explicit sign-in");
    assert!(matches!(outcome, SignIn::Installed { .. }), "{outcome:?}");
    harness
}

/// Polls a future once, returning its output if it completed.
async fn poll_once<F: Future + ?Sized>(future: Pin<&mut F>) -> Option<F::Output> {
    let mut future = Some(future);
    std::future::poll_fn(move |cx| {
        let future = future.take().expect("polled once");
        Poll::Ready(match future.poll(cx) {
            Poll::Ready(output) => Some(output),
            Poll::Pending => None,
        })
    })
    .await
}

// ---- §23.1 construction and explicit authentication ----

#[tokio::test(flavor = "current_thread")]
async fn explicit_sign_in_installs_a_session_used_by_regional_calls() {
    let harness = signed_in().await;
    harness.json(200, r#"{"id":"42","title":"Widget"}"#);
    let item = execute(harness.client.regional("eu").item("42"), &limits())
        .await
        .expect("item");
    assert_eq!(
        item,
        Item {
            id: "42".to_owned(),
            title: "Widget".to_owned()
        }
    );

    let requests = harness.server.requests();
    assert_eq!(requests.len(), 2);
    let login = &requests[0];
    assert_eq!(login.method(), "POST");
    assert_eq!(login.header("host"), Some("api.catalog.example.test"));
    assert_eq!(
        login.target(),
        format!("/root/v2/auth/session?tenant={}", support::TENANT_SENTINEL)
    );
    assert_eq!(
        login.header("x-bootstrap-key"),
        Some(support::BOOTSTRAP_SENTINEL)
    );
    assert_eq!(login.header("authorization"), None);
    assert_eq!(login.body(), br#"{"user":"user"}"#);

    // `auth replace` in the regional scope: bearer session only.
    let lookup = &requests[1];
    assert_eq!(lookup.header("host"), Some("eu.api.catalog.example.test"));
    assert_eq!(
        lookup.target(),
        format!(
            "/root/v2/regions/eu/items/42?tenant={}",
            support::TENANT_SENTINEL
        )
    );
    assert_eq!(
        lookup.header("authorization"),
        Some(format!("Bearer {SESSION}").as_str())
    );
    assert_eq!(lookup.header("x-bootstrap-key"), None);
    assert_eq!(lookup.header("x-client-version"), Some("3"));
}

#[tokio::test(flavor = "current_thread")]
async fn regional_calls_before_sign_in_fail_locally_without_a_hidden_login() {
    let harness = CatalogHarness::start();
    let error = harness
        .client
        .regional("eu")
        .item("42")
        .await
        .expect_err("no installed session");
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(
        harness.server.requests().is_empty(),
        "regional() hides no login"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_superseded_sign_in_is_reported_and_never_repeated() {
    let harness = CatalogHarness::start();
    harness.token(SESSION);
    let mut login = Box::pin(construction::sign_in(&harness.client, "user".to_owned()));
    // Drive the explicit acquisition until the provider has received it. The
    // loopback is checked before the future is polled again: once the request
    // is recorded the response may already be buffered, and the login stays
    // pending only because it is not polled until after the sign-out below.
    for _ in 0..2_000 {
        assert!(
            poll_once(login.as_mut()).await.is_none(),
            "the login cannot complete before the loopback observed it"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
        if !harness.server.requests().is_empty() {
            break;
        }
    }
    assert_eq!(
        harness.server.requests().len(),
        1,
        "login observed by the provider"
    );

    // A logout while the sign-in is pending advances the session intent.
    construction::sign_out(&harness.client).expect("local sign-out");
    let outcome = login.await.expect("the exchange itself succeeded");
    assert_eq!(outcome, SignIn::Superseded);

    // No automatic new login undid the logout, and nothing was installed.
    assert_eq!(harness.server.requests().len(), 1);
    let error = harness
        .client
        .regional("eu")
        .item("42")
        .await
        .expect_err("the superseded material was dropped");
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(harness.server.requests().len(), 1);
}

// ---- §23.2 one generic helper for requests and terminal wrappers ----

#[tokio::test(flavor = "current_thread")]
async fn execute_accepts_distinct_requests_and_library_terminals() {
    let harness = CatalogHarness::start();
    let client = &harness.client;

    // StoreRequest (library terminal).
    harness.token(SESSION);
    let stored = execute(
        client
            .authenticate(LoginInput {
                user: "user".to_owned(),
            })
            .store_as_session(),
        &limits(),
    )
    .await
    .expect("store");
    assert!(matches!(stored, StoreOutcome::Installed { .. }));

    let regional = client.regional("eu");
    // Two distinct generated request types.
    harness.json(200, r#"{"id":"1","title":"One"}"#);
    let item: Item = execute(regional.item("1"), &limits()).await.expect("item");
    assert_eq!(item.title, "One");
    harness
        .server
        .queue_response(404, b"not polled".to_vec())
        .expect("queue 404");
    let absent: Option<Item> = execute(regional.find_item("2"), &limits())
        .await
        .expect("head-only absence");
    assert!(absent.is_none());

    // ResponseRequest and AcceptedBytesRequest (library terminals).
    harness.json(200, r#"{"id":"3","title":"Three"}"#);
    let response = execute(regional.item("3").response(), &limits())
        .await
        .expect("response");
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.value().id, "3");
    harness.json(200, r#"{"id":"4","title":"Four"}"#);
    let bytes = execute(regional.item("4").accepted_bytes(), &limits())
        .await
        .expect("accepted bytes");
    assert_eq!(bytes.value().as_ref(), br#"{"id":"4","title":"Four"}"#);

    // Collection terminals: CollectPrefix and CollectAll.
    harness.json(200, r#"[{"id":"a"},{"id":"b"}]"#);
    let prefix = execute(
        regional
            .item_summaries(Vec::new())
            .set_count(NonZeroU32::new(2).expect("nonzero"))
            .collect_prefix(1),
        &limits(),
    )
    .await
    .expect("prefix");
    assert_eq!(prefix.items(), [summary("a"), summary("b")]);
    assert_eq!(prefix.summary().stop_reason(), StopReason::PageBound);
    assert_eq!(
        prefix.summary().continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    harness.json(200, r#"[{"id":"c"},{"id":"d"}]"#);
    harness.json(200, r#"[{"id":"e"}]"#);
    let all = execute(
        regional
            .item_summaries(Vec::new())
            .set_count(NonZeroU32::new(2).expect("nonzero"))
            .collect_all(NonZeroU32::new(5).expect("nonzero")),
        &CallOptions::new().cap_sends(2),
    )
    .await
    .expect("complete collection");
    assert_eq!(all, [summary("c"), summary("d"), summary("e")]);

    let requests = harness.server.requests();
    assert_eq!(requests.len(), 8, "one send per request and page");
    assert!(
        requests[5].target().contains("offset=0&count=2"),
        "{}",
        requests[5].target()
    );
    assert!(
        requests[6].target().contains("offset=0&count=2"),
        "{}",
        requests[6].target()
    );
    assert!(
        requests[7].target().contains("offset=2&count=2"),
        "{}",
        requests[7].target()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn execute_applies_the_callers_caps() {
    let harness = signed_in().await;
    // A zero send cap prevents application dispatch: nothing reaches the
    // provider and the error proves it.
    let error = execute(
        harness.client.regional("eu").item("1"),
        &CallOptions::new().cap_sends(0),
    )
    .await
    .expect_err("zero sends");
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(
        harness.server.requests().len(),
        1,
        "only the login was sent"
    );

    let summary = harness
        .client
        .regional("eu")
        .item("1")
        .with_options(&limits())
        .control_summary();
    assert_eq!(summary.send_cap(), Some(3));
    assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(10)));
}

// ---- §23.3 saved state versus CLI opinions ----

#[tokio::test(flavor = "current_thread")]
async fn saved_absence_and_cli_no_opinion_are_different_intents() {
    let harness = CatalogHarness::start();
    let scope = harness.client.regional("eu");
    let two = NonZeroU32::new(2).expect("nonzero");
    let url = |saved: Option<&str>, cli: Option<&str>, count: Option<NonZeroU32>| {
        list_request(
            &scope,
            SavedList {
                category: saved.map(str::to_owned),
                count: two,
            },
            CliListPatch {
                category: cli.map(str::to_owned),
                count,
            },
        )
        .plan()
        .expect("plan")
        .observed_url()
        .to_owned()
    };
    let base = "https://eu.api.catalog.example.test/root/v2/regions/eu/items?tenant=%3Credacted%3E";
    // Saved None: exact absence of the category key.
    assert_eq!(url(None, None, None), format!("{base}&offset=0&count=2"));
    // CLI None: no new opinion, the saved value stays.
    assert_eq!(
        url(Some("tools"), None, None),
        format!("{base}&category=tools&offset=0&count=2")
    );
    // A CLI opinion replaces the saved value.
    assert_eq!(
        url(Some("tools"), Some("toys"), NonZeroU32::new(9)),
        format!("{base}&category=toys&offset=0&count=9")
    );
    assert_eq!(
        url(None, Some("toys"), None),
        format!("{base}&category=toys&offset=0&count=2")
    );

    // The returned Vec is one response, never an undisclosed traversal: a
    // full page (which a traversal would continue) sends exactly one request.
    let harness = signed_in().await;
    harness.json(200, r#"[{"id":"a"},{"id":"b"}]"#);
    let page = configured_list(
        &harness.client.regional("eu"),
        SavedList {
            category: None,
            count: two,
        },
        CliListPatch::default(),
        &limits(),
    )
    .await
    .expect("one page");
    assert_eq!(page, [summary("a"), summary("b")]);
    assert_eq!(
        harness.server.requests().len(),
        2,
        "login plus one list page"
    );
}

// ---- §23.4 application-controlled search ----

#[tokio::test(flavor = "current_thread")]
async fn search_distinguishes_found_exhausted_and_bounded() {
    let harness = signed_in().await;
    let scope = harness.client.regional("eu");

    // Found on the second page: the application stops the traversal.
    harness.json(200, r#"{"data":[{"id":"a"}],"next":"c1"}"#);
    harness.json(200, r#"{"data":[{"id":"b"},{"id":"wanted"}],"next":"c2"}"#);
    match find_summary(&scope, "wanted", &limits())
        .await
        .expect("search")
    {
        SearchResult::Found { item, traversal } => {
            assert_eq!(item, summary("wanted"));
            assert_eq!(traversal.stop_reason(), StopReason::Application);
            assert_eq!(traversal.pages_received(), 2);
            assert_eq!(traversal.requested_page_bound(), 100);
        }
        other => panic!("expected Found, got {other:?}"),
    }

    // The provider proves exhaustion without a match.
    harness.json(200, r#"{"data":[{"id":"a"}],"next":"c1"}"#);
    harness.json(200, r#"{"data":[{"id":"b"}],"next":null}"#);
    match find_summary(&scope, "wanted", &limits())
        .await
        .expect("search")
    {
        SearchResult::Exhausted { traversal } => {
            assert_eq!(traversal.stop_reason(), StopReason::Exhausted);
            assert_eq!(traversal.continuation(), ContinuationKnowledge::Exhausted);
        }
        other => panic!("expected Exhausted, got {other:?}"),
    }

    // A page bound is not "not found": the item may still exist.
    harness.json(200, r#"{"data":[{"id":"a"}],"next":"c1"}"#);
    harness.json(200, r#"{"data":[{"id":"b"}],"next":"c2"}"#);
    match find_summary_within(&scope, "wanted", &limits(), 2)
        .await
        .expect("search")
    {
        SearchResult::Stopped { traversal } => {
            assert_eq!(traversal.stop_reason(), StopReason::PageBound);
            assert_eq!(
                traversal.continuation(),
                ContinuationKnowledge::NextRequestKnown
            );
            assert_eq!(traversal.pages_delivered(), 2);
        }
        other => panic!("expected Stopped, got {other:?}"),
    }

    let requests = harness.server.requests();
    assert_eq!(requests.len(), 7, "login plus six feed pages, no prefetch");
    assert!(
        !requests[1].target().contains("after="),
        "{}",
        requests[1].target()
    );
    assert!(
        requests[2].target().contains("after=c1"),
        "{}",
        requests[2].target()
    );
}

// ---- §23.5 bounded export prefix ----

#[tokio::test(flavor = "current_thread")]
async fn export_prefix_is_bounded_and_is_not_eof() {
    let harness = signed_in().await;
    let scope = harness.client.regional("eu");
    let body: Vec<u8> = (0..64_u8).collect();

    harness
        .server
        .queue_response_with_headers(
            200,
            &[("content-type", "application/octet-stream")],
            body.clone(),
        )
        .expect("queue export");
    match export_prefix(&scope, 10, &limits()).await.expect("prefix") {
        ExportPrefix::Read { bytes, stream } => {
            assert_eq!(bytes, body[..10]);
            assert_eq!(
                stream.state(),
                StreamState::Application,
                "a prefix is not EOF"
            );
            assert!(stream.acquired_bytes() >= 10);
            assert!(stream.acquired_bytes() <= 64);
        }
        ExportPrefix::NotStarted => panic!("a nonzero prefix starts the export"),
    }

    // The zero path constructs and executes nothing.
    assert!(matches!(
        export_prefix(&scope, 0, &limits()).await.expect("zero"),
        ExportPrefix::NotStarted
    ));
    assert_eq!(harness.server.requests().len(), 2, "login plus one export");

    // A prefix longer than the body ends at the actual end of the body.
    harness
        .server
        .queue_response_with_headers(
            200,
            &[("content-type", "application/octet-stream")],
            body.clone(),
        )
        .expect("queue export");
    match export_prefix(&scope, 1_000, &limits())
        .await
        .expect("whole body")
    {
        ExportPrefix::Read { bytes, stream } => {
            assert_eq!(bytes, body);
            assert_eq!(stream.state(), StreamState::Eof);
            assert_eq!(stream.acquired_bytes(), 64);
        }
        ExportPrefix::NotStarted => panic!("a nonzero prefix starts the export"),
    }
}

// ---- §23.6 write reconciliation ----

const COMMAND_SENTINEL: &str = "command-sentinel-8c21";
const TITLE_SENTINEL: &str = "title-sentinel-0d4b";

fn change() -> TitleChange {
    TitleChange {
        item_id: "item-1".to_owned(),
        command_id: CommandId::new(COMMAND_SENTINEL),
        title: TITLE_SENTINEL.to_owned(),
    }
}

fn assert_no_sentinel(text: &str) {
    assert!(
        !text.contains(COMMAND_SENTINEL),
        "command identifier leaked: {text}"
    );
    assert!(!text.contains(TITLE_SENTINEL), "payload leaked: {text}");
}

#[tokio::test(flavor = "current_thread")]
async fn write_failures_keep_the_command_identifier_for_reconciliation() {
    let harness = signed_in().await;
    let scope = harness.client.regional("eu");

    // Confirmed write.
    harness.json(200, r#"{"id":"item-1","title":"t"}"#);
    let item = change_title(&scope, change(), &limits())
        .await
        .expect("write");
    assert_eq!(item.id, "item-1");

    // Not dispatched: resubmission with the same identifier is safe later.
    let failure = change_title(&scope, change(), &CallOptions::new().cap_sends(0))
        .await
        .expect_err("zero sends");
    assert!(matches!(failure, WriteFailure::NotDispatched { .. }));
    assert!(!failure.needs_reconciliation());
    assert_eq!(failure.command_id().expose(), COMMAND_SENTINEL);

    // An observed 2xx followed by a decode failure may have committed.
    harness.json(200, "not json");
    let failure = change_title(&scope, change(), &limits())
        .await
        .expect_err("undecodable success body");
    assert!(matches!(failure, WriteFailure::OutcomeUnknown { .. }));
    assert!(failure.needs_reconciliation());
    assert_eq!(failure.command_id().expose(), COMMAND_SENTINEL);
    assert_eq!(
        failure.error().dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );

    // Exactly one send per submitted write; no automatic resubmission.
    let requests = harness.server.requests();
    assert_eq!(
        requests.len(),
        3,
        "login, confirmed write, undecodable write"
    );
    for write in &requests[1..] {
        assert_eq!(write.method(), "PUT");
        assert_eq!(write.header("idempotency-key"), Some(COMMAND_SENTINEL));
        assert_eq!(
            write.body(),
            format!(r#"{{"title":"{TITLE_SENTINEL}"}}"#).as_bytes()
        );
    }

    // The example's own formatters print neither the identifier nor payload.
    assert_no_sentinel(&format!("{failure:?}"));
    assert_no_sentinel(&format!("{failure}"));
    assert_no_sentinel(&format!("{:?}", change()));
    assert_no_sentinel(&format!("{:?}", CommandId::new(COMMAND_SENTINEL)));
    assert!(format!("{failure}").contains("OutcomeUnknown"));
}

#[test]
fn the_sentinel_check_detects_a_derived_formatter() {
    // Control for the test above: a derived Debug does print both values, so
    // the sentinel assertions are able to fail.
    #[derive(Debug)]
    #[allow(dead_code)]
    struct Derived {
        command_id: String,
        title: String,
    }
    let derived = format!(
        "{:?}",
        Derived {
            command_id: COMMAND_SENTINEL.to_owned(),
            title: TITLE_SENTINEL.to_owned(),
        }
    );
    assert!(derived.contains(COMMAND_SENTINEL) && derived.contains(TITLE_SENTINEL));
    let caught = std::panic::catch_unwind(|| assert_no_sentinel(&derived));
    assert!(caught.is_err());
}

#[test]
fn classification_follows_aggregate_dispatch_knowledge_only() {
    // `classify` never invents a new identifier; the variant is chosen only
    // by the library error's aggregate dispatch knowledge.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let harness = CatalogHarness::start();
    let error = runtime
        .block_on(harness.client.regional("eu").item("1").into_future())
        .expect_err("no session");
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    let failure = classify(CommandId::new(COMMAND_SENTINEL), error);
    assert!(matches!(failure, WriteFailure::NotDispatched { .. }));
    assert_eq!(failure.command_id(), &CommandId::new(COMMAND_SENTINEL));
}
