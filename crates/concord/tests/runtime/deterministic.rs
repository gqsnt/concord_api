use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::codec::{CodecError, EncodeBody, EncodeContext};
use concord::error::HookError;
use concord::hooks::{BeforeSend, BoxFuture, CompletionEvent, ResponseHeadEvent, RuntimeHooks};
use concord::http::SafeProxy;
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
}

impl RuntimeHooks for LifecycleCounts {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        self.before.fetch_add(1, Ordering::SeqCst);
        self.endpoints
            .lock()
            .expect("endpoint capture")
            .push(context.endpoint.to_owned());
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(
        &'a self,
        _context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        self.response.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }

    fn on_complete<'a>(&'a self, _context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.complete.fetch_add(1, Ordering::SeqCst);
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
