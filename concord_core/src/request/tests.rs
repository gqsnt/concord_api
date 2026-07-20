use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_test_support::executor::DeterministicExecutor;
use http::Method;

use super::*;
use crate::client::ClientCore;
use crate::error::{ClientBuildErrorKind, ErrorCategory};
use crate::hooks::{BoxFuture, DebugLevel, RuntimeHooks};
use crate::http::HttpConfigBuilder;
use crate::runtime::RuntimeConfigBuilder;

struct CountingHooks {
    calls: AtomicUsize,
}

impl CountingHooks {
    const fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl RuntimeHooks for CountingHooks {
    fn before_send<'a>(
        &'a self,
        _context: crate::hooks::BeforeSend<'a>,
    ) -> BoxFuture<'a, Result<(), crate::error::HookError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

struct DisplayValue {
    value: &'static str,
    calls: Arc<AtomicUsize>,
}

impl fmt::Display for DisplayValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.calls.fetch_add(1, Ordering::SeqCst);
        formatter.write_str(self.value)
    }
}

struct BytesEndpoint {
    account: DisplayValue,
}

impl EndpointSpec for BytesEndpoint {
    type Body = EmptyBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "PreparationApi::accounts::bytes",
            Method::GET,
            Some(Duration::from_secs(20)),
        )
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), crate::error::ApiError> {
        builder.push_static("accounts")?;
        builder.push_value(&self.account)?;
        builder.push_static("bytes")?;
        Ok((builder.finish(), EmptyBody::metadata()))
    }

    fn into_body(self) -> Self::Body {
        EmptyBody::new()
    }
}

struct NoContentEndpoint;

impl EndpointSpec for NoContentEndpoint {
    type Body = EmptyBody;
    type Response = NoContentResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "PreparationApi::health::ping",
            Method::GET,
            Some(Duration::from_secs(5)),
        )
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), crate::error::ApiError> {
        builder.push_static("health")?;
        builder.push_static("ping")?;
        Ok((builder.finish(), EmptyBody::metadata()))
    }

    fn into_body(self) -> Self::Body {
        EmptyBody::new()
    }
}

fn core(executor: &DeterministicExecutor, hooks: Arc<dyn RuntimeHooks>) -> ClientCore {
    ClientCore::build_with_development(
        "http://preparation.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().hooks(hooks),
        executor,
    )
    .expect("test core")
}

fn bytes_endpoint(value: &'static str, calls: Arc<AtomicUsize>) -> BytesEndpoint {
    BytesEndpoint {
        account: DisplayValue { value, calls },
    }
}

fn ready<E>(preparation: OperationPreparation<E>) -> PreparedOperation<E> {
    match preparation {
        OperationPreparation::Ready(prepared) => prepared,
        OperationPreparation::Failed(failure) => {
            panic!(
                "unexpected preparation failure: {:?}",
                failure.error.category()
            )
        }
    }
}

#[tokio::test]
async fn request_creation_and_setters_are_side_effect_free_until_first_poll() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(CountingHooks::new());
    let display_calls = Arc::new(AtomicUsize::new(0));
    let core = core(&executor, hooks.clone());
    let request = Request::new(
        core.clone(),
        bytes_endpoint("team-alpha", Arc::clone(&display_calls)),
    )
    .timeout(Duration::from_secs(2))
    .disable_timeout()
    .inherit_timeout()
    .debug(DebugLevel::Headers);
    let future = prepare_operation(request);

    assert_eq!(display_calls.load(Ordering::SeqCst), 0);
    assert_eq!(core.operation_allocations(), 0);
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let prepared = ready(future.await.expect("first poll should prepare"));
    assert_eq!(display_calls.load(Ordering::SeqCst), 1);
    assert_eq!(core.operation_allocations(), 1);
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(
        prepared.head.logical_url().as_str(),
        "http://preparation.example.test/v1/accounts/team-alpha/bytes"
    );
    assert_eq!(
        prepared.head.safe_url(),
        prepared.head.logical_url().as_str()
    );
    assert_ne!(
        prepared.head.safe_url().as_ptr(),
        prepared.head.logical_url().as_str().as_ptr(),
        "safe and logical URL storage must remain distinct"
    );
    assert_eq!(prepared.head.timeout(), Some(Duration::from_secs(20)));
    assert_eq!(prepared.head.debug(), DebugLevel::Headers);
    assert!(prepared.head.public_headers().is_empty());
    assert_eq!(prepared.body_metadata.kind(), crate::hooks::BodyKind::Empty);
    assert_eq!(prepared.body_metadata.known_length(), Some(0));
    prepared
        .endpoint
        .into_body()
        .materialize(
            prepared.head.endpoint(),
            prepared.head.method(),
            NonZeroUsize::new(16).expect("positive test body limit"),
        )
        .expect("empty body materialization");
}

#[test]
fn hand_authored_endpoint_types_select_only_declared_response_handlers() {
    fn assert_bytes<E>()
    where
        E: EndpointSpec<Body = EmptyBody, Response = BytesResponse>,
    {
    }
    fn assert_no_content<E>()
    where
        E: EndpointSpec<Body = EmptyBody, Response = NoContentResponse>,
    {
    }
    assert_bytes::<BytesEndpoint>();
    assert_no_content::<NoContentEndpoint>();
}

#[tokio::test]
async fn invalid_dynamic_segment_fails_before_hooks_or_transport() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(CountingHooks::new());
    let display_calls = Arc::new(AtomicUsize::new(0));
    let core = core(&executor, hooks.clone());
    let error = match prepare_operation(Request::new(
        core.clone(),
        bytes_endpoint("", display_calls),
    ))
    .await
    {
        Ok(OperationPreparation::Ready(_)) => panic!("empty dynamic segment must fail"),
        Ok(OperationPreparation::Failed(failure)) => failure.error,
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(core.operation_allocations(), 1);
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn request_options_are_local_and_core_clones_share_one_inner_owner() {
    let executor = DeterministicExecutor::new();
    let core = core(&executor, Arc::new(CountingHooks::new()));
    let sibling = core.clone();
    assert!(core.shares_inner_with(&sibling));

    let first = prepare_operation(
        Request::new(core.clone(), NoContentEndpoint)
            .timeout(Duration::from_secs(1))
            .debug(DebugLevel::Basic),
    );
    let second = prepare_operation(Request::new(sibling, NoContentEndpoint).disable_timeout());
    let (first, second) = tokio::join!(first, second);
    let first = ready(first.expect("first preparation"));
    let second = ready(second.expect("second preparation"));
    assert_eq!(first.head.timeout(), Some(Duration::from_secs(1)));
    assert_eq!(first.head.debug(), DebugLevel::Basic);
    assert_eq!(second.head.timeout(), None);
    assert_eq!(second.head.debug(), DebugLevel::None);
    assert_ne!(first.operation_id, second.operation_id);
}

#[test]
fn generated_private_endpoint_mapping_preserves_core_and_request_options() {
    struct ValueEndpoint(u32);

    let executor = DeterministicExecutor::new();
    let core = core(&executor, Arc::new(CountingHooks::new()));
    let request = Request::new(core.clone(), ValueEndpoint(1))
        .timeout(Duration::from_secs(7))
        .debug(DebugLevel::Headers);
    let mapped =
        crate::__private::map_endpoint(request, |ValueEndpoint(value)| ValueEndpoint(value + 1));
    let Request {
        core: mapped_core,
        endpoint: ValueEndpoint(value),
        options,
    } = mapped;

    assert_eq!(value, 2);
    assert!(core.shares_inner_with(&mapped_core));
    assert_eq!(
        options.resolve_timeout(Some(Duration::from_secs(99))),
        Some(Duration::from_secs(7))
    );
    assert_eq!(options.resolve_debug(DebugLevel::None), DebugLevel::Headers);
    assert_eq!(core.operation_allocations(), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn live_operation_ids_are_unique_across_cores_sharing_hooks() {
    let hooks: Arc<dyn RuntimeHooks> = Arc::new(CountingHooks::new());
    let first_executor = DeterministicExecutor::new();
    let second_executor = DeterministicExecutor::new();
    let first_core = core(&first_executor, Arc::clone(&hooks));
    let second_core = core(&second_executor, hooks);
    let (first, second) = tokio::join!(
        prepare_operation(Request::new(first_core, NoContentEndpoint)),
        prepare_operation(Request::new(second_core, NoContentEndpoint)),
    );
    assert_ne!(
        ready(first.expect("first operation")).operation_id,
        ready(second.expect("second operation")).operation_id
    );
}

#[test]
fn fixed_tls_preflights_fail_during_core_construction() {
    let http_only = match ClientCore::build(
        "http://example.test",
        HttpConfigBuilder::new().https_only(true),
        RuntimeConfigBuilder::new(),
    ) {
        Ok(_) => panic!("https-only must reject a fixed HTTP base"),
        Err(error) => error,
    };
    assert_eq!(http_only.kind(), ClientBuildErrorKind::HttpConfiguration);

    #[cfg(not(feature = "default-tls"))]
    {
        let unavailable = match ClientCore::build(
            "https://example.test",
            HttpConfigBuilder::new(),
            RuntimeConfigBuilder::new(),
        ) {
            Ok(_) => panic!("fixed HTTPS requires TLS capability"),
            Err(error) => error,
        };
        assert_eq!(unavailable.kind(), ClientBuildErrorKind::TlsUnavailable);
    }
}
