//! The named inherent generated surface reachable through one facade
//! dependency (§7.1–§7.4, §4.3, §4.5).

use std::future::IntoFuture;
use std::sync::{Arc, Mutex};

use concord::endpoint::ResponseAcquisition;
use concord::http::StatusCode;
use concord::inspection::{ValueControl, ValuePresence, ValueRole};
use concord::observation::{OperationEvent, OperationObserver};
use concord::retry::{RetryDecision, RetryPolicy};
use concord::runtime::OperationId;
use concord::{ApiError, Binary, ByteStream, Bytes, Empty, Text, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

api! {
    client FacadeApi {
        base "http://facade.example.test/";
        timeout 30s;

        scope resources(
            scope_value: String,
            query region?: String,
            query scope_limit: u32 = 50,
        ) {
            path "resources/{scope_value}";

            GET fetch(
                id: String,
                query verbose?: bool,
                query page_size: u32 = 25,
            ) -> Bytes via Binary {
                path "{id}";
            }

            POST update(
                id: String,
                body payload: String via Text,
                header("X-Revision") revision: u32,
            ) -> Bytes via Binary {
                path "{id}";
            }

            GET delete(id: String) -> () via Empty {
                path "{id}";
            }

            GET download(id: String) -> ByteStream via Binary {
                path "{id}/download";
            }
        }
    }
}

api! {
    client OtherFacadeApi {
        base "http://other-facade.example.test/";

        GET echo() -> Bytes via Binary;
    }
}

// VALUE-05: requests and intents whose declaration contains a body slot are
// never `Clone`, even though the `String` payload is.
crate::assert_not_impl!(facade_api::resources::UpdateRequest: Clone);
crate::assert_not_impl!(facade_api::resources::UpdateEndpoint: Clone);
// §7.3: binding an intent to another client type fails type checking.
crate::assert_not_impl!(other_facade_api::EchoEndpoint: facade_api::GeneratedEndpoint);
crate::assert_not_impl!(facade_api::resources::FetchEndpoint: other_facade_api::GeneratedEndpoint);
// ByteStream output is not Clone; that is not required for a body-free
// request or intent to be Clone (checked below).
crate::assert_not_impl!(ByteStream: Clone);
// STREAM-01: both managed streams are owned, non-Clone values, even when the
// record type itself is Clone.
#[cfg(feature = "records")]
crate::assert_not_impl!(concord::ItemStream<u32>: Clone);
// VALUE-05: in-flight futures, pages, streams and credential-storage terminal
// futures are not cloneable. Every item type here is itself Clone, so only
// the terminal's own declaration can supply the absence.
crate::assert_not_impl!(concord::pagination::Pages<u32>: Clone);
crate::assert_not_impl!(concord::pagination::CollectPrefix<u32>: Clone);
crate::assert_not_impl!(concord::pagination::CollectAll<u32>: Clone);
crate::assert_not_impl!(concord::endpoint::ResponseRequest<Bytes>: Clone);
crate::assert_not_impl!(concord::endpoint::AcceptedBytesRequest: Clone);
crate::assert_not_impl!(concord::auth::StoreRequest: Clone);
crate::assert_not_impl!(
    <facade_api::resources::FetchRequest as IntoFuture>::IntoFuture: Clone
);
crate::assert_not_impl!(
    <facade_api::resources::DownloadRequest as IntoFuture>::IntoFuture: Clone
);
crate::assert_not_impl!(<concord::endpoint::ResponseRequest<Bytes> as IntoFuture>::IntoFuture: Clone);
crate::assert_not_impl!(<concord::endpoint::AcceptedBytesRequest as IntoFuture>::IntoFuture: Clone);
crate::assert_not_impl!(<concord::auth::StoreRequest as IntoFuture>::IntoFuture: Clone);
crate::assert_not_impl!(<concord::pagination::CollectPrefix<u32> as IntoFuture>::IntoFuture: Clone);
crate::assert_not_impl!(<concord::pagination::CollectAll<u32> as IntoFuture>::IntoFuture: Clone);

fn assert_clone<T: Clone>() {}
fn assert_send<T: Send>() {}
fn assert_output<O, R: IntoFuture<Output = Result<O, ApiError>>>(_: &R) {}

/// V3-L12: unbound navigation, direct requests and bound intents produce
/// equivalent selected plans; binding an intent to another client type fails
/// type checking (the `assert_not_impl!` probes above).
#[test]
fn one_facade_dependency_exposes_predictable_named_inherent_surface() {
    let client = FacadeApi::new().expect("zero-input facade client");
    let request = client
        .resources("tenant-sentinel")
        .set_region("eu")
        .set_scope_limit(75)
        .reset_scope_limit()
        .fetch("resource-sentinel")
        .set_verbose(true)
        .omit_verbose()
        .set_page_size(100)
        .reset_page_size();
    let debug = format!("{request:?}");
    assert!(debug.starts_with("FetchRequest {"), "{debug}");
    assert!(debug.contains("facade_api.resources.fetch"), "{debug}");
    for sentinel in ["tenant-sentinel", "resource-sentinel"] {
        assert!(
            !debug.contains(sentinel),
            "request Debug leaked {sentinel}: {debug}"
        );
    }
    assert_eq!(
        request.plan().expect("direct plan").observed_url(),
        "http://facade.example.test/resources/tenant-sentinel/resource-sentinel?region=eu&scope_limit=50&page_size=25"
    );
    assert_output::<Bytes, _>(&request);
    let _: facade_api::resources::FetchRequest = request;

    let _: facade_api::resources::DeleteRequest = client.resources("tenant").delete("resource");
    let update: facade_api::resources::UpdateRequest =
        client
            .resources("tenant")
            .update("resource", "body".to_owned(), 7);
    let update_plan = update.plan().expect("update plan");
    assert_eq!(
        update_plan.observed_url(),
        "http://facade.example.test/resources/tenant/resource?scope_limit=50"
    );
    assert!(
        format!("{:?}", update_plan.public_headers()).contains("7"),
        "direct header placement is planned"
    );

    let endpoint = FacadeApi::unbound()
        .resources("tenant")
        .fetch("resource")
        .set_verbose(true)
        .set_page_size(100);
    let cloned = endpoint.clone();
    let stored = [endpoint.clone(), cloned];
    assert_eq!(stored.len(), 2);
    let description = endpoint.description();
    assert_eq!(description.id(), "facade_api.resources.fetch");
    assert_eq!(description.source_path(), "FacadeApi::resources::fetch");
    assert_eq!(
        description.route_shape(),
        "/resources/{resources.scope_value}/{endpoint.id}"
    );
    assert_eq!(description.method(), concord::http::Method::GET);
    assert_eq!(
        description.response_acquisition(),
        ResponseAcquisition::Buffered
    );
    let unbound_plan = client.plan(&endpoint).expect("unbound plan");
    let direct_plan = client
        .resources("tenant")
        .fetch("resource")
        .set_verbose(true)
        .set_page_size(100)
        .plan()
        .expect("equivalent direct plan");
    assert_eq!(unbound_plan.observed_url(), direct_plan.observed_url());
    assert_eq!(
        unbound_plan.observed_url(),
        "http://facade.example.test/resources/tenant/resource?scope_limit=50&verbose=true&page_size=100"
    );
    let bound: facade_api::resources::FetchRequest = client.request(endpoint);
    assert_eq!(
        bound.plan().expect("bound plan").observed_url(),
        direct_plan.observed_url()
    );

    let body_endpoint: facade_api::resources::UpdateEndpoint = FacadeApi::unbound()
        .resources("tenant")
        .update("resource", "body".to_owned(), 7);
    let _: facade_api::resources::UpdateRequest = client.request(body_endpoint);

    fn bind<E>(client: &FacadeApi, endpoint: E) -> E::Request
    where
        E: facade_api::GeneratedEndpoint,
    {
        client.request(endpoint)
    }

    fn describe<E>(client: &FacadeApi, endpoint: &E) -> String
    where
        E: facade_api::GeneratedEndpoint,
    {
        client
            .plan(endpoint)
            .expect("generic intent plan")
            .description()
            .id()
            .to_owned()
    }

    let reusable = FacadeApi::unbound().resources("tenant").fetch("resource");
    assert_eq!(describe(&client, &reusable), "facade_api.resources.fetch");
    let _: facade_api::resources::FetchRequest = bind(&client, reusable);

    // The declared representation is a static contract fact, not a
    // user-visible associated type.
    let fetch = FacadeApi::contract()
        .endpoint("facade_api.resources.fetch")
        .expect("fetch contract");
    assert_eq!(fetch.result().representation().type_name(), "Binary");
    assert_eq!(fetch.result().output_type(), "Bytes");
    assert_eq!(fetch.result().driver(), ResponseAcquisition::Buffered);
}

/// V3-L10/L11: constructor inputs are positional with no controls,
/// initialized required inputs have no omission, body-free requests and
/// intents are `Clone` without an `Output: Clone` bound and preserve their
/// selected snapshots.
#[test]
fn value_matrix_and_clone_law_follow_the_declaration() {
    assert_clone::<FacadeApi>();
    assert_clone::<facade_api::resources::Resources>();
    assert_clone::<facade_api::resources::FetchRequest>();
    assert_clone::<facade_api::resources::FetchEndpoint>();
    assert_clone::<facade_api::resources::DownloadRequest>();
    assert_clone::<facade_api::resources::DownloadEndpoint>();
    assert_send::<facade_api::resources::UpdateRequest>();
    assert_send::<facade_api::resources::DownloadRequest>();

    let contract = FacadeApi::contract();
    let scope = contract.owner("FacadeApi::resources").expect("scope owner");
    let scope_value = scope.value("scope_value").expect("scope_value");
    assert_eq!(scope_value.role(), ValueRole::Constructor);
    assert!(scope_value.controls().is_empty());
    assert_eq!(scope_value.constructor_position(), Some(0));
    let limit = scope.value("scope_limit").expect("scope_limit");
    assert_eq!(limit.presence(), ValuePresence::Required);
    assert_eq!(
        limit.controls(),
        [
            ValueControl::Set,
            ValueControl::SetIfSome,
            ValueControl::Reset
        ]
    );
    assert_eq!(
        scope.value("region").expect("region").controls(),
        [
            ValueControl::Set,
            ValueControl::Replace,
            ValueControl::SetIfSome,
            ValueControl::Omit
        ]
    );
    let update = contract
        .endpoint("facade_api.resources.update")
        .expect("update contract");
    let body = update.body().expect("required body");
    assert_eq!(body.presence(), ValuePresence::Required);
    assert_eq!(body.constructor_position(), Some(1));
    let update_owner = contract
        .owner("FacadeApi::resources::update")
        .expect("update owner");
    assert_eq!(
        update_owner
            .value("revision")
            .expect("revision")
            .constructor_position(),
        Some(2)
    );

    let client = FacadeApi::new().expect("client");
    let scope = client.resources("tenant").set_region("eu");
    let before_change = scope.fetch("id");
    let changed = scope.clone().omit_region().set_scope_limit(9);
    let after_change = changed.fetch("id");
    let cloned_request = before_change.clone().set_page_size(1);
    assert_eq!(
        before_change.plan().expect("old snapshot").observed_url(),
        "http://facade.example.test/resources/tenant/id?region=eu&scope_limit=50&page_size=25"
    );
    assert_eq!(
        cloned_request
            .plan()
            .expect("clone snapshot")
            .observed_url(),
        "http://facade.example.test/resources/tenant/id?region=eu&scope_limit=50&page_size=1"
    );
    assert_eq!(
        after_change.plan().expect("new snapshot").observed_url(),
        "http://facade.example.test/resources/tenant/id?scope_limit=9&page_size=25"
    );
    assert_eq!(
        scope
            .fetch("id")
            .plan()
            .expect("source scope")
            .observed_url(),
        before_change.plan().expect("old snapshot").observed_url(),
        "changing a cloned scope never mutates its source"
    );

    let download = client.resources("tenant").download("file");
    let download_clone = download.clone();
    assert_eq!(
        download_clone.plan().expect("stream plan").observed_url(),
        "http://facade.example.test/resources/tenant/file/download?scope_limit=50"
    );
}

api! {
    /// VALUE-05 loopback fixture: one safe and one non-idempotent body-free
    /// endpoint.
    client CloneLawApi {
        base "http://clone-law.example.test/";
        timeout 30s;

        GET read(id: String) -> Bytes via Binary {
            path "items/{id}";
        }

        POST touch(id: String) -> Bytes via Binary {
            path "items/{id}/touch";
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FinishedOperation {
    operation_id: OperationId,
    attempts_started: u32,
    policy_retries: u32,
}

#[derive(Default)]
struct CloneLawObserver {
    finished: Mutex<Vec<FinishedOperation>>,
}

impl OperationObserver for CloneLawObserver {
    fn observe(&self, event: OperationEvent<'_>) {
        if let OperationEvent::OperationFinished(finished) = event {
            self.finished
                .lock()
                .expect("finished operations")
                .push(FinishedOperation {
                    operation_id: finished.operation_id,
                    attempts_started: finished.attempts_started,
                    policy_retries: finished.policy_retries,
                });
        }
    }
}

fn clone_law_client(server: &ScriptedServer, observer: &Arc<CloneLawObserver>) -> CloneLawApi {
    let proxy = crate::support::loopback_proxy(server);
    let observer: Arc<dyn OperationObserver> = Arc::clone(observer) as _;
    // A policy that recommends a replay for every failure: only the
    // executor's replay authority can refuse one.
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(4)
        .without_budget()
        .classify_fn(|_attempt| RetryDecision::Retry)
        .build()
        .expect("always-retry policy");
    CloneLawApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.operation_observer(observer))
        .retry(policy)
        .build()
        .expect("clone-law client")
}

/// VALUE-05: each executed clone of a body-free request is a new operation,
/// and cloning grants no retry authority.
#[tokio::test]
async fn executed_clones_are_new_operations_and_cloning_grants_no_retry_authority() {
    let server = ScriptedServer::start([
        ResponseScript::ok("first-clone"),
        ResponseScript::ok("second-clone"),
        ResponseScript::status(503, "original-unavailable"),
        ResponseScript::status(503, "clone-unavailable"),
        ResponseScript::ok("never-used"),
    ])
    .expect("loopback server");
    let observer = Arc::new(CloneLawObserver::default());
    let client = clone_law_client(&server, &observer);

    // Two executed clones of one safe body-free request: two operations with
    // distinct identities, each with its own single attempt.
    let read = client.read("item");
    let first = read.clone().response().await.expect("first clone");
    let second = read.response().await.expect("second clone");
    assert_eq!(first.into_value(), Bytes::from_static(b"first-clone"));
    assert_eq!(second.into_value(), Bytes::from_static(b"second-clone"));
    let finished = observer.finished.lock().expect("finished").clone();
    assert_eq!(finished.len(), 2);
    assert_ne!(finished[0].operation_id, finished[1].operation_id);
    for operation in &finished {
        assert_eq!(operation.attempts_started, 1);
        assert_eq!(operation.policy_retries, 0);
    }

    // A non-idempotent body-free request is Clone too, but a clone is a new,
    // application-initiated operation, never a replay authorized by the
    // original: under an always-retry policy an observed failure of either is
    // not resubmitted automatically, and neither inherits the other's counts.
    let touch = client.touch("item");
    let clone = touch.clone();
    let original = touch.await.expect_err("observed non-idempotent failure");
    let cloned = clone.await.expect_err("observed failure of the clone");
    assert_eq!(
        original.http_status(),
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    assert_eq!(cloned.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    let original_id = original.operation_id().expect("original operation");
    let cloned_id = cloned.operation_id().expect("clone operation");
    assert_ne!(original_id, cloned_id);
    for error in [&original, &cloned] {
        let counts = error.operation_counts().expect("journal counts");
        assert_eq!(counts.attempts_started(), 1);
        assert_eq!(counts.application_handoffs(), 1);
        assert_eq!(counts.policy_retries(), 0);
        assert_eq!(counts.automatic_replays(), 0);
    }
    let finished = observer.finished.lock().expect("finished").clone();
    assert_eq!(finished.len(), 4);
    let identities: std::collections::HashSet<_> = finished
        .iter()
        .map(|operation| operation.operation_id)
        .collect();
    assert_eq!(
        identities.len(),
        4,
        "every executed clone is a new operation"
    );
    assert_eq!(finished[2].operation_id, original_id);
    assert_eq!(finished[3].operation_id, cloned_id);
    assert_eq!(server.request_count(), 4, "no automatic replay was sent");
}
