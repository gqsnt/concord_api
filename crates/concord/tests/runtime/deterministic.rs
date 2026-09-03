use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::codec::{CodecError, EncodeBody, EncodeContext};
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::error::GateError;
use concord::http::{Method, SafeProxy};
use concord::observation::{
    AttemptInfo, BeforeSend, BoxFuture, CompletionEvent, CompletionObserver, RequestGate,
    ResponseGate, ResponseHeadEvent,
};
use concord::retry::{RetryDecision, RetryFailure, RetryPolicy};
use concord::{Bytes, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

pub struct BodyInput {
    value: String,
    encodes: Arc<AtomicUsize>,
}

pub struct CountingBody;

impl EncodeBody for CountingBody {
    type Input = BodyInput;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-concord-phase6");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        input.encodes.fetch_add(1, Ordering::SeqCst);
        Ok(Bytes::from(input.value))
    }
}

api! {
    client RuntimeApi {
        timeout off;
        base "http://runtime.example.test/";


    scope resources(scope_value: String) {
        path ["resources", resources.scope_value];
        POST submit(
            id: String,
            verbose?: bool,
            limit: u32 = 25,
            body body: crate::runtime::deterministic::BodyInput using crate::CountingBody,
        ) -> Bytes using Bytes {
            id "runtime.resources.submit";
            path [endpoint.id];
            query { "verbose" = endpoint.verbose; }
            headers { "X-Limit" = endpoint.limit; }
            repeatability idempotent;
        }
    }
}}

#[derive(Default)]
struct LifecycleCounts {
    before: AtomicUsize,
    response: AtomicUsize,
    complete: AtomicUsize,
    endpoints: Mutex<Vec<String>>,
    attempts: Mutex<Vec<AttemptInfo>>,
    completion_totals: Mutex<Vec<(u32, u32, bool)>>,
}

impl RequestGate for LifecycleCounts {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), GateError>> {
        self.before.fetch_add(1, Ordering::SeqCst);
        self.endpoints
            .lock()
            .expect("endpoint capture")
            .push(context.endpoint.to_owned());
        self.attempts
            .lock()
            .expect("attempt capture")
            .push(context.attempt);
        Box::pin(async { Ok(()) })
    }
}

impl ResponseGate for LifecycleCounts {
    fn before_response_handling<'a>(
        &'a self,
        _context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), GateError>> {
        self.response.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

impl CompletionObserver for LifecycleCounts {
    fn on_complete(&self, context: CompletionEvent<'_>) {
        self.complete.fetch_add(1, Ordering::SeqCst);
        self.completion_totals
            .lock()
            .expect("completion totals")
            .push((
                context.attempts_started,
                context.policy_retries,
                context.recovered_authentication,
            ));
    }
}

#[tokio::test]
async fn direct_and_explicit_named_wrappers_preserve_inertness_and_lifecycle_equivalence() {
    const SCOPE_SENTINEL: &str = "scope-sensitive-phase6";
    const ID_SENTINEL: &str = "id-sensitive-phase6";
    const BODY_SENTINEL: &str = "body-sensitive-phase6";

    let server = ScriptedServer::start([
        ResponseScript::ok(b"direct".to_vec()),
        ResponseScript::ok(b"explicit".to_vec()),
    ])
    .expect("scripted runtime server");
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let hooks = Arc::new(LifecycleCounts::default());
    let observation = hooks.clone();
    let client = RuntimeApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .request_gate(observation.clone())
                .response_gate(observation.clone())
                .completion_observer(observation)
        })
        .build()
        .expect("runtime client");
    let encodes = Arc::new(AtomicUsize::new(0));

    let direct = client
        .resources(SCOPE_SENTINEL.to_owned())
        .submit(
            ID_SENTINEL.to_owned(),
            BodyInput {
                value: BODY_SENTINEL.to_owned(),
                encodes: Arc::clone(&encodes),
            },
        )
        .with_verbose(true)
        .set_limit(50)
        .timeout(Duration::from_secs(5));
    let endpoint = RuntimeApi::unbound()
        .resources(SCOPE_SENTINEL.to_owned())
        .submit(
            ID_SENTINEL.to_owned(),
            BodyInput {
                value: BODY_SENTINEL.to_owned(),
                encodes: Arc::clone(&encodes),
            },
        )
        .with_verbose(true)
        .set_limit(50);
    let explicit = client.request(endpoint).timeout(Duration::from_secs(5));

    assert_eq!(server.request_count(), 0);
    assert_eq!(encodes.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 0);
    for debug in [format!("{direct:?}"), format!("{explicit:?}")] {
        assert_eq!(debug, "SubmitRequest { .. }");
        assert!(!debug.contains(SCOPE_SENTINEL));
        assert!(!debug.contains(ID_SENTINEL));
        assert!(!debug.contains(BODY_SENTINEL));
    }

    assert_eq!(
        direct.await.expect("direct generated request"),
        Bytes::from_static(b"direct")
    );
    assert_eq!(
        explicit.await.expect("explicit generated request"),
        Bytes::from_static(b"explicit")
    );
    assert_eq!(encodes.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 2);
    assert_eq!(
        hooks.endpoints.lock().expect("endpoint capture").as_slice(),
        ["runtime.resources.submit", "runtime.resources.submit"]
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method(), requests[1].method());
    assert_eq!(requests[0].target(), requests[1].target());
    assert_eq!(requests[0].header("X-Limit"), Some("50"));
    assert_eq!(requests[0].header("X-Limit"), requests[1].header("X-Limit"));
    assert_eq!(requests[0].body(), BODY_SENTINEL.as_bytes());
    assert_eq!(requests[0].body(), requests[1].body());
}

#[tokio::test]
async fn generated_retry_method_has_direct_explicit_and_runtime_endpoint_parity() {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"intermediate".to_vec()),
        ResponseScript::ok(b"direct".to_vec()),
        ResponseScript::status(503, b"intermediate".to_vec()),
        ResponseScript::ok(b"explicit".to_vec()),
        ResponseScript::status(503, b"intermediate".to_vec()),
        ResponseScript::ok(b"runtime".to_vec()),
    ])
    .expect("scripted retry server");
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let hooks = Arc::new(LifecycleCounts::default());
    let observation = hooks.clone();
    let classifications = Arc::new(AtomicUsize::new(0));
    let classifications_for_policy = Arc::clone(&classifications);
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .classify_fn(move |attempt| {
            classifications_for_policy.fetch_add(1, Ordering::SeqCst);
            match attempt.failure() {
                RetryFailure::RejectedStatus { status, .. }
                    if status == concord::http::StatusCode::SERVICE_UNAVAILABLE =>
                {
                    RetryDecision::Retry
                }
                _ => RetryDecision::Stop,
            }
        })
        .build()
        .expect("managed retry policy");
    let client = RuntimeApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .retry(policy)
        .configure_runtime(move |runtime| {
            runtime
                .request_gate(observation.clone())
                .response_gate(observation.clone())
                .completion_observer(observation)
        })
        .build()
        .expect("retry runtime client");
    let encodes = Arc::new(AtomicUsize::new(0));

    let direct = client
        .resources("retry-scope".to_owned())
        .submit(
            "retry-id".to_owned(),
            BodyInput {
                value: "encoded-once".to_owned(),
                encodes: Arc::clone(&encodes),
            },
        )
        .response()
        .await
        .expect("managed retry succeeds");
    let explicit = client
        .request(
            RuntimeApi::unbound()
                .resources("retry-scope".to_owned())
                .submit(
                    "retry-id".to_owned(),
                    BodyInput {
                        value: "encoded-once".to_owned(),
                        encodes: Arc::clone(&encodes),
                    },
                ),
        )
        .response()
        .await
        .expect("explicit managed retry succeeds");
    let runtime_endpoint = RuntimeEndpoint::builder(
        EndpointId::new("runtime.retry").expect("runtime endpoint ID"),
        Method::GET,
    )
    .path("runtime-retry")
    .response::<Bytes>()
    .build()
    .expect("runtime retry endpoint");
    let runtime = client
        .request(runtime_endpoint)
        .response()
        .await
        .expect("runtime endpoint managed retry succeeds");

    assert_eq!(direct.value(), &Bytes::from_static(b"direct"));
    assert_eq!(explicit.value(), &Bytes::from_static(b"explicit"));
    assert_eq!(runtime.value(), &Bytes::from_static(b"runtime"));
    for response in [&direct, &explicit, &runtime] {
        assert_eq!(response.metadata().attempts_started(), 2);
        assert_eq!(response.metadata().policy_retries(), 1);
        assert!(!response.metadata().recovered_authentication());
    }
    assert_eq!(server.request_count(), 6);
    assert_eq!(encodes.load(Ordering::SeqCst), 2);
    assert_eq!(classifications.load(Ordering::SeqCst), 3);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 6);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 6);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 3);
    assert_eq!(
        hooks.attempts.lock().expect("attempt capture").as_slice(),
        [
            AttemptInfo {
                index: 0,
                policy_retry_index: 0,
                authentication_recovery: false,
            },
            AttemptInfo {
                index: 1,
                policy_retry_index: 1,
                authentication_recovery: false,
            },
            AttemptInfo {
                index: 0,
                policy_retry_index: 0,
                authentication_recovery: false,
            },
            AttemptInfo {
                index: 1,
                policy_retry_index: 1,
                authentication_recovery: false,
            },
            AttemptInfo {
                index: 0,
                policy_retry_index: 0,
                authentication_recovery: false,
            },
            AttemptInfo {
                index: 1,
                policy_retry_index: 1,
                authentication_recovery: false,
            },
        ]
    );
    assert_eq!(
        hooks
            .completion_totals
            .lock()
            .expect("completion totals")
            .as_slice(),
        [(2, 1, false), (2, 1, false), (2, 1, false)]
    );
}

fn retry_rejected_status_policy(classifications: &Arc<AtomicUsize>) -> RetryPolicy {
    let classifications = Arc::clone(classifications);
    RetryPolicy::builder()
        .max_retries_per_operation(1)
        .classify_fn(move |attempt| {
            classifications.fetch_add(1, Ordering::SeqCst);
            match attempt.failure() {
                RetryFailure::RejectedStatus { .. } => RetryDecision::Retry,
                _ => RetryDecision::Stop,
            }
        })
        .build()
        .expect("bounded retry-sharing policy")
}

fn retry_client(server: &ScriptedServer, policy: RetryPolicy) -> RuntimeApi {
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    RuntimeApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .retry(policy)
        .build()
        .expect("retry client")
}

async fn execute_retry_probe(
    client: &RuntimeApi,
    label: &str,
) -> Result<Bytes, concord::error::ApiError> {
    client
        .resources("sharing".to_owned())
        .submit(
            label.to_owned(),
            BodyInput {
                value: label.to_owned(),
                encodes: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await
}

#[tokio::test]
async fn client_clones_share_the_selected_policy_budget() {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"first".to_vec()),
        ResponseScript::ok(b"first-recovered".to_vec()),
        ResponseScript::status(503, b"second-terminal".to_vec()),
        ResponseScript::ok(b"must-remain".to_vec()),
    ])
    .expect("clone-sharing server");
    let classifications = Arc::new(AtomicUsize::new(0));
    let client = retry_client(&server, retry_rejected_status_policy(&classifications));
    let clone = client.clone();

    assert_eq!(
        execute_retry_probe(&client, "first")
            .await
            .expect("first low-volume retry"),
        Bytes::from_static(b"first-recovered")
    );
    let second = execute_retry_probe(&clone, "second")
        .await
        .expect_err("shared low-volume budget suppresses the second retry");

    assert_eq!(second.category(), concord::error::ErrorCategory::HttpStatus);
    assert_eq!(server.request_count(), 3);
    assert_eq!(classifications.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn explicit_policy_clones_share_while_independent_policy_builds_isolate() {
    let shared_first = ScriptedServer::start([
        ResponseScript::status(503, b"first".to_vec()),
        ResponseScript::ok(b"shared-first".to_vec()),
    ])
    .expect("first shared-policy server");
    let shared_second = ScriptedServer::start([
        ResponseScript::status(503, b"second".to_vec()),
        ResponseScript::ok(b"must-remain".to_vec()),
    ])
    .expect("second shared-policy server");
    let shared_classifications = Arc::new(AtomicUsize::new(0));
    let shared_policy = retry_rejected_status_policy(&shared_classifications);
    let first_client = retry_client(&shared_first, shared_policy.clone());
    let second_proxy = SafeProxy::parse(shared_second.url("")).expect("second shared proxy");
    let second_client = RuntimeApi::builder()
        .configure_http(move |http| http.proxy(second_proxy))
        .configure_runtime(move |runtime| runtime.retry_policy(shared_policy))
        .build()
        .expect("lower-level runtime retry client");

    execute_retry_probe(&first_client, "shared-first")
        .await
        .expect("first explicit policy clone retries");
    execute_retry_probe(&second_client, "shared-second")
        .await
        .expect_err("second explicit policy clone shares consumed budget");
    assert_eq!(shared_first.request_count(), 2);
    assert_eq!(shared_second.request_count(), 1);
    assert_eq!(shared_classifications.load(Ordering::SeqCst), 2);

    let isolated_first = ScriptedServer::start([
        ResponseScript::status(503, b"first".to_vec()),
        ResponseScript::ok(b"isolated-first".to_vec()),
    ])
    .expect("first isolated-policy server");
    let isolated_second = ScriptedServer::start([
        ResponseScript::status(503, b"second".to_vec()),
        ResponseScript::ok(b"isolated-second".to_vec()),
    ])
    .expect("second isolated-policy server");
    let first_classifications = Arc::new(AtomicUsize::new(0));
    let second_classifications = Arc::new(AtomicUsize::new(0));
    let first_isolated_client = retry_client(
        &isolated_first,
        retry_rejected_status_policy(&first_classifications),
    );
    let second_isolated_client = retry_client(
        &isolated_second,
        retry_rejected_status_policy(&second_classifications),
    );

    execute_retry_probe(&first_isolated_client, "isolated-first")
        .await
        .expect("first independent policy retries");
    execute_retry_probe(&second_isolated_client, "isolated-second")
        .await
        .expect("second independent policy retries");
    assert_eq!(isolated_first.request_count(), 2);
    assert_eq!(isolated_second.request_count(), 2);
    assert_eq!(first_classifications.load(Ordering::SeqCst), 1);
    assert_eq!(second_classifications.load(Ordering::SeqCst), 1);
}
