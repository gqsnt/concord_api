use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::codec::{CodecError, EncodeBody, EncodeContext};
use concord::error::HookError;
use concord::hooks::{
    AttemptInfo, BeforeSend, BoxFuture, CompletionEvent, ResponseHeadEvent, RuntimeHooks,
};
use concord::http::SafeProxy;
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
        base "http://runtime.example.test/";
    }

    scope resources(scope_value: String) {
        path ["resources", scope.scope_value];
        POST submit(
            id: String,
            verbose: Option<bool>,
            limit: u32 = 25,
            #[body] body: crate::CountingBody,
        ) -> Bytes {
            id "runtime.resources.submit";
            path [ep.id];
            query { "verbose" = ep.verbose; }
            headers { "X-Limit" = ep.limit; }
        }
    }
}

#[derive(Default)]
struct LifecycleCounts {
    before: AtomicUsize,
    response: AtomicUsize,
    complete: AtomicUsize,
    endpoints: Mutex<Vec<String>>,
    attempts: Mutex<Vec<AttemptInfo>>,
    completion_totals: Mutex<Vec<(u32, u32, bool)>>,
}

impl RuntimeHooks for LifecycleCounts {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
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

    fn on_response_head<'a>(
        &'a self,
        _context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        self.response.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }

    fn on_complete<'a>(&'a self, context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.complete.fetch_add(1, Ordering::SeqCst);
        self.completion_totals
            .lock()
            .expect("completion totals")
            .push((
                context.attempts_started,
                context.policy_retries,
                context.recovered_authentication,
            ));
        Box::pin(async {})
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
    let hook_object: Arc<dyn RuntimeHooks> = hooks.clone();
    let client = RuntimeApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.hooks(hook_object))
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
        .set_verbose(true)
        .set_limit(50)
        .timeout(Duration::from_secs(5));
    let endpoint = runtime_api::resources::Submit::new(
        SCOPE_SENTINEL.to_owned(),
        ID_SENTINEL.to_owned(),
        BodyInput {
            value: BODY_SENTINEL.to_owned(),
            encodes: Arc::clone(&encodes),
        },
    )
    .set_verbose(true)
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
async fn facade_runtime_policy_retries_rejected_status_with_visible_attempts() {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"intermediate".to_vec()),
        ResponseScript::ok(b"recovered".to_vec()),
    ])
    .expect("scripted retry server");
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let hooks = Arc::new(LifecycleCounts::default());
    let hook_object: Arc<dyn RuntimeHooks> = hooks.clone();
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
        .configure_runtime(move |runtime| runtime.hooks(hook_object).retry_policy(policy))
        .build()
        .expect("retry runtime client");
    let encodes = Arc::new(AtomicUsize::new(0));

    let response = client
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
    assert_eq!(response.value(), &Bytes::from_static(b"recovered"));
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 1);
    assert!(!response.metadata().recovered_authentication());
    assert_eq!(server.request_count(), 2);
    assert_eq!(encodes.load(Ordering::SeqCst), 1);
    assert_eq!(classifications.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 1);
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
        ]
    );
    assert_eq!(
        hooks
            .completion_totals
            .lock()
            .expect("completion totals")
            .as_slice(),
        [(2, 1, false)]
    );
}
