use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bytes::Bytes;
use concord_test_support::executor::DeterministicExecutor;
use http::Method;

use super::*;
use crate::__private::GeneratedCredential;
use crate::atom::{AtomError, BoundedAtomWriter, RequestAtom};
use crate::auth::{AccessToken, Auth};
use crate::body::EmptyBody;
use crate::client::ClientCore;
use crate::codec::Binary;
use crate::endpoint::EndpointId;
use crate::error::{ClientBuildErrorKind, ErrorCategory};
use crate::hooks::{DebugLevel, OperationEvent, OperationObserver};
use crate::http::HttpConfigBuilder;
use crate::response::Empty;
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

impl OperationObserver for CountingHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        if matches!(event, OperationEvent::AttemptStarted(_)) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
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

impl RequestAtom for DisplayValue {
    fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        output.push_str(self.value)
    }
}

struct BytesEndpoint {
    account: DisplayValue,
}

impl EndpointSpec for BytesEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = Bytes;
    type Response = Binary;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "preparation_api.accounts.bytes",
            Method::GET,
            Some(Duration::from_secs(20)),
        )
    }

    fn prepare_head(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<PreparedHead, crate::error::ApiError> {
        builder.push_static("accounts")?;
        builder.push_value(&self.account)?;
        builder.push_static("bytes")?;
        builder.finish()
    }

    fn into_body(self) {}
}

struct NoContentEndpoint;

impl EndpointSpec for NoContentEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = ();
    type Response = Empty;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "preparation_api.health.ping",
            Method::GET,
            Some(Duration::from_secs(5)),
        )
    }

    fn prepare_head(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<PreparedHead, crate::error::ApiError> {
        builder.push_static("health")?;
        builder.push_static("ping")?;
        builder.finish()
    }

    fn into_body(self) {}
}

fn core(executor: &DeterministicExecutor, observer: Arc<dyn OperationObserver>) -> ClientCore {
    ClientCore::build_with_development(
        "http://preparation.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().operation_observer(observer),
        executor,
    )
    .expect("test core")
}

fn bytes_endpoint(value: &'static str, calls: Arc<AtomicUsize>) -> BytesEndpoint {
    BytesEndpoint {
        account: DisplayValue { value, calls },
    }
}

fn ready<E>(preparation: OperationPreparation<E>) -> PreparedOperation<E>
where
    E: EndpointSpec,
{
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
    .expect("valid timeout")
    .reset_timeout()
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
        prepared.head.observed_url(),
        prepared.head.logical_url().as_str()
    );
    assert_ne!(
        prepared.head.observed_url().as_ptr(),
        prepared.head.logical_url().as_str().as_ptr(),
        "safe and logical URL storage must remain distinct"
    );
    assert_eq!(prepared.head.timeout(), Some(Duration::from_secs(20)));
    assert_eq!(prepared.head.debug(), DebugLevel::Headers);
    assert_eq!(prepared.head.public_headers().len(), 1);
    assert_eq!(
        prepared.head.public_headers().get(http::header::ACCEPT),
        Some(&http::HeaderValue::from_static("application/octet-stream"))
    );
    // Body preparation is phase 3 of execution, not public preparation: no
    // body facts, Content-Type or absence exist before the executor runs.
    assert!(
        !prepared
            .head
            .public_headers()
            .contains_key(http::header::CONTENT_TYPE)
    );
    assert!(prepared.absence.is_none());
    prepared.endpoint.into_body();
}

#[test]
fn hand_authored_endpoint_types_select_only_declared_response_handlers() {
    fn assert_bytes<E>()
    where
        E: EndpointSpec<Body = EmptyBody, Response = Binary>,
    {
    }
    fn assert_no_content<E>()
    where
        E: EndpointSpec<Body = EmptyBody, Response = Empty>,
    {
    }
    assert_bytes::<BytesEndpoint>();
    assert_no_content::<NoContentEndpoint>();
}

#[tokio::test]
async fn invalid_dynamic_segment_fails_before_observation_or_transport() {
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
            .expect("valid timeout")
            .debug(DebugLevel::Basic),
    );
    let second = prepare_operation(Request::new(sibling, NoContentEndpoint));
    let (first, second) = tokio::join!(first, second);
    let first = ready(first.expect("first preparation"));
    let second = ready(second.expect("second preparation"));
    assert_eq!(first.head.timeout(), Some(Duration::from_secs(1)));
    assert_eq!(first.head.debug(), DebugLevel::Basic);
    assert_eq!(second.head.timeout(), Some(Duration::from_secs(5)));
    assert_eq!(second.head.debug(), DebugLevel::None);
    assert_ne!(first.operation_id, second.operation_id);
}

/// A hand-written specification applying a placement created by another
/// client build. v3 runtime endpoints cannot carry placements themselves
/// (RUNTIME-02), so the cross-build case is exercised both through a
/// mismatched owner projection (rejected at bind and plan) and through the
/// executor's own context check.
struct ForeignContextEndpoint {
    credential: crate::auth::CredentialRef<AccessToken>,
}

impl EndpointSpec for ForeignContextEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = ();
    type Response = Empty;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new("runtime.context.execute", Method::GET, None)
    }

    fn prepare_head(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<PreparedHead, crate::error::ApiError> {
        builder.push_auth(Auth::bearer(self.credential.clone()))?;
        builder.finish()
    }

    fn into_body(self) {}
}

#[tokio::test]
async fn runtime_auth_rejects_a_foreign_client_context_before_planning_or_execution() {
    let executor = DeterministicExecutor::new();
    let owner_hooks = Arc::new(CountingHooks::new());
    let foreign_hooks = Arc::new(CountingHooks::new());
    let owner = core(&executor, owner_hooks.clone());
    let foreign = core(&executor, foreign_hooks.clone());
    let credential = GeneratedCredential::static_value(
        &owner,
        AccessToken::try_new("client-context-secret").expect("bounded access token"),
    );
    let endpoint = |id: &'static str| {
        RuntimeEndpoint::builder(EndpointId::from_static(id), Method::GET)
            .response::<Empty, ()>()
            .build()
            .expect("runtime endpoint")
    };
    // An owner projection whose placements belong to another client build.
    let mismatched = test_owner::TestOwner::new(foreign.clone())
        .auth(Auth::bearer(credential.credential()))
        .into_projection();

    let planning_error = test_owner::plan(&mismatched, &endpoint("runtime.context.plan"))
        .expect_err("foreign planning context must fail");
    assert_eq!(
        planning_error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );
    let binding_error = test_owner::bind(&mismatched, endpoint("runtime.context.bind"))
        .expect_err("foreign binding context must fail");
    assert_eq!(
        binding_error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );

    let execution_error = match prepare_operation(Request::new(
        foreign.clone(),
        ForeignContextEndpoint {
            credential: credential.credential(),
        },
    ))
    .await
    {
        Ok(OperationPreparation::Ready(_)) => panic!("foreign execution context must fail"),
        Ok(OperationPreparation::Failed(failure)) => failure.error,
        Err(error) => error,
    };
    assert_eq!(
        execution_error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );
    assert_eq!(owner_hooks.calls.load(Ordering::SeqCst), 0);
    assert_eq!(foreign_hooks.calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    // A clone of the owning client shares its context: the head a runtime
    // binding composes (runtime metadata plus the inherited placement)
    // validates against the clone. Binding this cleartext base is separately
    // refused by the credentials-over-HTTPS rule
    // (`runtime_endpoint::tests::protected::credentials_require_https_at_bind`).
    let owner_clone = owner.clone();
    let accepted = endpoint("runtime.context.clone");
    let mut accepted_head = HeadBuilder::new(
        owner_clone.base_url(),
        RequestMeta::from_description(accepted.description()),
        &RequestOptions::new(),
        owner_clone.default_debug(),
        owner_clone.default_runtime_endpoint_timeout(),
    );
    accepted_head
        .push_auth(Auth::bearer(credential.credential()))
        .expect("inherited placement");
    accepted_head
        .finish()
        .expect("prepared owner head")
        .validate_auth_context(owner_clone.credential_context())
        .expect("owner clone context");
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn runtime_endpoints_inherit_the_snapshot_timeout_with_finite_overrides() {
    let executor = DeterministicExecutor::new();
    let core = ClientCore::build_with_development(
        "http://runtime-timeout.example.test/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().runtime_endpoint_timeout(Duration::from_secs(13)),
        &executor,
    )
    .expect("runtime timeout core");

    let inherited =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.inherited"), Method::GET)
            .response::<Empty, ()>()
            .build()
            .expect("inherited runtime endpoint");
    let inherited = ready(
        prepare_operation(test_owner::root(&core, inherited).into_request())
            .await
            .expect("inherited preparation"),
    );
    assert_eq!(inherited.head.timeout(), Some(Duration::from_secs(13)));

    // §20.4: at binding the owner's effective timeout applies when no
    // runtime-local selection exists; the runtime default only fills an
    // owner without a declared selection.
    let owned = RuntimeEndpoint::builder(EndpointId::from_static("runtime.owned"), Method::GET)
        .response::<Empty, ()>()
        .build()
        .expect("owner-timeout runtime endpoint");
    let owned = ready(
        prepare_operation(
            test_owner::bind(
                &test_owner::TestOwner::new(core.clone())
                    .timeout(Duration::from_secs(11))
                    .into_projection(),
                owned,
            )
            .expect("owner-timeout binding")
            .into_request(),
        )
        .await
        .expect("owner-timeout preparation"),
    );
    assert_eq!(owned.head.timeout(), Some(Duration::from_secs(11)));

    let explicit =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.explicit"), Method::GET)
            .timeout(Duration::from_secs(7))
            .response::<Empty, ()>()
            .build()
            .expect("explicit runtime endpoint");
    let explicit = ready(
        prepare_operation(
            test_owner::bind(
                &test_owner::TestOwner::new(core.clone())
                    .timeout(Duration::from_secs(11))
                    .into_projection(),
                explicit,
            )
            .expect("explicit binding")
            .into_request(),
        )
        .await
        .expect("explicit preparation"),
    );
    assert_eq!(explicit.head.timeout(), Some(Duration::from_secs(7)));

    let request_override =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.request"), Method::GET)
            .response::<Empty, ()>()
            .build()
            .expect("request override endpoint");
    let request_override = ready(
        prepare_operation(
            test_owner::root(&core, request_override)
                .timeout(Duration::from_secs(3))
                .expect("valid timeout")
                .into_request(),
        )
        .await
        .expect("request override preparation"),
    );
    assert_eq!(
        request_override.head.timeout(),
        Some(Duration::from_secs(3))
    );
}

#[test]
fn generated_private_endpoint_mapping_preserves_core_and_request_options() {
    struct ValueEndpoint(u32);

    let executor = DeterministicExecutor::new();
    let core = core(&executor, Arc::new(CountingHooks::new()));
    let request = Request::new(core.clone(), ValueEndpoint(1))
        .timeout(Duration::from_secs(7))
        .expect("valid timeout")
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
async fn live_operation_ids_are_unique_across_cores_sharing_gates() {
    let gate: Arc<dyn OperationObserver> = Arc::new(CountingHooks::new());
    let first_executor = DeterministicExecutor::new();
    let second_executor = DeterministicExecutor::new();
    let first_core = core(&first_executor, Arc::clone(&gate));
    let second_core = core(&second_executor, gate);
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
        HttpConfigBuilder::new().require_https(),
        RuntimeConfigBuilder::new(),
    ) {
        Ok(_) => panic!("https-only must reject a fixed HTTP base"),
        Err(error) => error,
    };
    assert_eq!(http_only.kind(), ClientBuildErrorKind::HttpsRequired);

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
