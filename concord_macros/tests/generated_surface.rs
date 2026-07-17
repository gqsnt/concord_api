use std::fmt::{self, Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_core::advanced::{
    BeforeSend, BoxFuture, ClientBuildErrorKind, CompletionEvent, DebugLevel, HookError,
    ResponseHeadEvent, RuntimeHooks, SafeProxy,
};
use concord_core::prelude::{Bytes, ClientBuildError, DecodedResponse, Request, ResponseRequest};
use concord_macros::api;
use concord_test_support::server::{ResponseScript, ScriptedServer};

api! {
    client ExampleApi {
        base "http://generated.example.test/v1/";
        scope health() {
            path ["health"];
            GET Bytes() -> Bytes;
            GET Ping() -> NoContent;
            GET Item(id: String) -> Bytes {
                path ["items", ep.id];
            }
        }
    }
}

struct CountingValue {
    text: &'static str,
    calls: Arc<AtomicUsize>,
}

impl Display for CountingValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.calls.fetch_add(1, Ordering::SeqCst);
        formatter.write_str(self.text)
    }
}

api! {
    client LazyApi {
        base "http://lazy.generated.example.test/";
        scope resources(scope_value: crate::CountingValue) {
            path ["scopes", scope.scope_value, "resources"];
            GET ById(endpoint_value: crate::CountingValue) -> Bytes {
                path [ep.endpoint_value];
            }
            GET Ping() -> NoContent;
        }
    }
}

mod nested_case {
    pub mod inner {
        use concord_macros::api;

        api! {
            client NestedApi {
                base "http://nested.generated.example.test/";
                scope health() {
                    path ["health"];
                    GET Ping() -> NoContent;
                }
            }
        }
    }
}

mod repeated_case {
    #![allow(dead_code)]

    use concord_macros::api;

    struct Result;
    struct Option;
    struct String;
    struct Vec;
    struct Method;
    struct HeadBuilder;
    mod core {}
    mod runtime {}

    api! {
        client AlphaApi {
            base "http://alpha.generated.example.test/";
            scope health() {
                path ["health"];
                GET Ping() -> NoContent;
            }
        }
    }

    api! {
        client BetaApi {
            base "http://beta.generated.example.test/";
            scope health() {
                path ["health"];
                GET Ping() -> NoContent;
            }
        }
    }

    pub fn requests_are_isolated() {
        let alpha = AlphaApi::new().expect("alpha client");
        let beta = BetaApi::new().expect("beta client");
        let _: concord_core::Request<_> = alpha.health().ping();
        let _: concord_core::Request<_> = beta.health().ping();

        fn accepts_alpha_endpoint<E: alpha_api::AlphaApiEndpoint>(_: &E) {}
        fn accepts_beta_endpoint<E: beta_api::BetaApiEndpoint>(_: &E) {}

        let alpha_endpoint = alpha_api::endpoints::health::Ping::new();
        accepts_alpha_endpoint(&alpha_endpoint);
        let _: concord_core::Request<_> = alpha.request(alpha_endpoint);

        let beta_endpoint = beta_api::endpoints::health::Ping::new();
        accepts_beta_endpoint(&beta_endpoint);
        let _: concord_core::Request<_> = beta.request(beta_endpoint);
    }
}

struct CountingHooks {
    before: AtomicUsize,
    response: AtomicUsize,
    complete: AtomicUsize,
}

impl CountingHooks {
    const fn new() -> Self {
        Self {
            before: AtomicUsize::new(0),
            response: AtomicUsize::new(0),
            complete: AtomicUsize::new(0),
        }
    }
}

impl RuntimeHooks for CountingHooks {
    fn before_send<'a>(&'a self, _context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        self.before.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(&'a self, _context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        self.response.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }

    fn on_complete<'a>(&'a self, _context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.complete.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("local proxy origin")
}

fn proxied_example(server: &ScriptedServer, hooks: Option<Arc<dyn RuntimeHooks>>) -> ExampleApi {
    let proxy = proxy(server);
    let builder = ExampleApi::builder().http(move |http| http.proxy(proxy));
    let builder = match hooks {
        Some(hooks) => {
            builder.runtime(move |runtime| runtime.hooks(hooks).debug(DebugLevel::Headers))
        }
        None => builder,
    };
    builder.build().expect("proxied generated client")
}

fn exact_new_shape() -> Result<(), ClientBuildError> {
    let client = ExampleApi::new()?;
    let _: Request<_> = client.health().bytes();
    let _: Request<_> = client.health().ping();
    Ok(())
}

fn build_error(result: Result<ExampleApi, ClientBuildError>) -> ClientBuildError {
    match result {
        Ok(_) => panic!("generated build unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[test]
fn new_builder_and_public_request_shapes_are_final() {
    exact_new_shape().expect("fallible no-input constructor");
    let from_builder = ExampleApi::builder().build().expect("client builder");
    let from_named_builder = ExampleApiBuilder::new().build().expect("named builder");
    let _: Request<_> = from_builder.health().bytes();
    let _: Request<_> = from_named_builder.health().ping();
    let _: ResponseRequest<_> = from_builder.health().bytes().response();
    let _: Request<_> = from_builder.request(example_api::endpoints::health::Bytes::new());
}

#[test]
fn configuration_closures_are_immediate_and_duplicates_are_suppressed() {
    let first_http = AtomicUsize::new(0);
    let duplicate_http = AtomicUsize::new(0);
    let builder = ExampleApi::builder()
        .http(|http| {
            first_http.fetch_add(1, Ordering::SeqCst);
            http.retry_off()
        })
        .http(|http| {
            duplicate_http.fetch_add(1, Ordering::SeqCst);
            http
        });
    assert_eq!(first_http.load(Ordering::SeqCst), 1);
    assert_eq!(duplicate_http.load(Ordering::SeqCst), 0);
    assert_eq!(
        build_error(builder.build()).kind(),
        ClientBuildErrorKind::HttpConfiguration
    );

    let first_runtime = AtomicUsize::new(0);
    let duplicate_runtime = AtomicUsize::new(0);
    let builder = ExampleApi::builder()
        .runtime(|runtime| {
            first_runtime.fetch_add(1, Ordering::SeqCst);
            runtime.debug(DebugLevel::Basic)
        })
        .runtime(|runtime| {
            duplicate_runtime.fetch_add(1, Ordering::SeqCst);
            runtime
        });
    assert_eq!(first_runtime.load(Ordering::SeqCst), 1);
    assert_eq!(duplicate_runtime.load(Ordering::SeqCst), 0);
    assert_eq!(
        build_error(builder.build()).kind(),
        ClientBuildErrorKind::RuntimeConfiguration
    );
}

#[test]
fn fixed_base_and_http_configuration_fail_during_generated_build() {
    let error = build_error(
        ExampleApi::builder()
            .http(|http| http.https_only(true))
            .build(),
    );
    assert_eq!(error.kind(), ClientBuildErrorKind::HttpConfiguration);
}

#[tokio::test]
async fn exact_generated_direct_and_metadata_workflows_execute_through_local_proxy() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"healthy".to_vec()),
        ResponseScript::status(205, b"ignored-and-unpolled".to_vec()),
    ])
    .expect("generated proxy");
    let hooks = Arc::new(CountingHooks::new());
    let hook_object: Arc<dyn RuntimeHooks> = hooks.clone();
    let client = proxied_example(&server, Some(hook_object));

    let bytes: Bytes = client.health().bytes().await.expect("generated bytes");
    assert_eq!(bytes, b"healthy"[..]);
    let response: DecodedResponse<()> = client
        .health()
        .ping()
        .response()
        .await
        .expect("generated metadata");
    assert_eq!(response.status().as_u16(), 205);
    assert_eq!(response.value(), &());
    assert_eq!(hooks.before.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 2);
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn facade_and_explicit_endpoints_execute_with_identical_encoded_paths() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"facade".to_vec()),
        ResponseScript::ok(b"explicit".to_vec()),
    ])
    .expect("parity proxy");
    let client = proxied_example(&server, None);
    let dynamic = "team/alpha".to_owned();

    let facade = client
        .health()
        .item(dynamic.clone())
        .timeout(Duration::from_secs(5))
        .debug(DebugLevel::None)
        .await
        .expect("facade request");
    let explicit = example_api::endpoints::health::Item::new(dynamic);
    let explicit = client
        .request(explicit)
        .inherit_timeout()
        .await
        .expect("explicit request");
    assert_eq!(facade, b"facade"[..]);
    assert_eq!(explicit, b"explicit"[..]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target(), requests[1].target());
    assert!(
        requests[0]
            .target()
            .contains("/v1/health/items/team%2Falpha")
    );
    assert!(!requests[0].target().contains("/team/alpha"));
}

#[tokio::test]
async fn request_creation_is_lazy_and_generated_client_clone_shares_runtime_configuration() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("lazy proxy");
    let proxy = proxy(&server);
    let hooks = Arc::new(CountingHooks::new());
    let hook_object: Arc<dyn RuntimeHooks> = hooks.clone();
    let client = LazyApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| runtime.hooks(hook_object))
        .build()
        .expect("lazy client");
    let clone = client.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let facade = client.resources(CountingValue {
        text: "scope/value",
        calls: Arc::clone(&calls),
    });
    let request = facade.clone().by_id(CountingValue {
        text: "endpoint/value",
        calls: Arc::clone(&calls),
    });
    let sibling = facade.ping();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 0);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.response.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 0);
    drop(sibling);
    assert_eq!(request.await.expect("lazy request"), b"first"[..]);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        server.requests()[0]
            .target()
            .contains("/scopes/scope%2Fvalue/resources/endpoint%2Fvalue")
    );

    let second = clone
        .resources(CountingValue {
            text: "other-scope",
            calls: Arc::clone(&calls),
        })
        .by_id(CountingValue {
            text: "other-endpoint",
            calls: Arc::clone(&calls),
        })
        .disable_timeout()
        .await
        .expect("clone request");
    assert_eq!(second, b"second"[..]);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 2);
    assert_eq!(hooks.complete.load(Ordering::SeqCst), 2);
}

#[test]
fn nested_and_repeated_invocations_keep_generated_paths_isolated() {
    let nested = nested_case::inner::NestedApi::new().expect("nested client");
    let _: concord_core::Request<_> = nested.health().ping();
    let _: concord_core::Request<_> =
        nested.request(nested_case::inner::nested_api::endpoints::health::Ping::new());
    repeated_case::requests_are_isolated();
}
