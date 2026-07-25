use std::cell::Cell;
use std::error::Error as _;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::gate::Gate;
use concord_test_support::script::{
    CapturedHeaderValue, ExpectedRequest, ScriptedResponse, TransportFailure,
};
use http::{Method, StatusCode};

use super::execute::paginate_with;
use super::{PageLimit, PaginatedEndpoint, PaginationController, PaginationError};
#[cfg(feature = "default-tls")]
use crate::auth::{
    AccessToken, Auth, CredentialProvider, CredentialRef, CredentialRequest, SlotSelector,
};
use crate::body::{EmptyBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec};
use crate::client::ClientCore;
use crate::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use crate::error::{ApiError, ErrorCategory, PaginationErrorKind, RateLimitErrorKind};
use crate::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel,
    PaginationCompletionEvent, ResponseHeadEvent, RuntimeHooks,
};
use crate::http::HttpConfigBuilder;
use crate::operation::OperationId;
use crate::rate_limit::{
    RateLimitError, RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimiter,
};
use crate::request::{EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "pagination_runtime_api.items.list";

struct PageCodec;

struct TestPage {
    items: Vec<String>,
    next: Option<String>,
}

impl IntoIterator for TestPage {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl DecodeBody for PageCodec {
    type Output = TestPage;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text =
            std::str::from_utf8(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        if text.is_empty() {
            return Ok(TestPage {
                items: Vec::new(),
                next: None,
            });
        }
        let (items, next) = text
            .split_once('|')
            .ok_or_else(|| CodecError::new(CodecErrorKind::Decode))?;
        let items = if items.is_empty() {
            Vec::new()
        } else {
            items.split(',').map(str::to_owned).collect()
        };
        let next = (next != "-").then(|| next.to_owned());
        Ok(TestPage { items, next })
    }
}

#[derive(Clone)]
struct PageEndpoint {
    endpoint: &'static str,
    method: Method,
    timeout: Option<Duration>,
    cursor: String,
    validate_calls: Arc<AtomicUsize>,
    next_calls: Arc<AtomicUsize>,
    construction_calls: Arc<AtomicUsize>,
    binding_calls: Arc<AtomicUsize>,
    fail_construction: bool,
    fail_validate: bool,
    fail_next: bool,
    short_page_size: Option<NonZeroUsize>,
}

impl PageEndpoint {
    fn get(timeout: Option<Duration>, controller: CursorController) -> Self {
        Self {
            endpoint: ENDPOINT,
            method: Method::GET,
            timeout,
            cursor: controller.state,
            validate_calls: controller.validate_calls,
            next_calls: controller.next_calls,
            construction_calls: Arc::new(AtomicUsize::new(0)),
            binding_calls: Arc::new(AtomicUsize::new(0)),
            fail_construction: false,
            fail_validate: controller.fail_validate,
            fail_next: controller.fail_next,
            short_page_size: controller.short_page_size,
        }
    }

    fn failing_construction(timeout: Option<Duration>) -> Self {
        let mut endpoint = Self::get(timeout, CursorController::new("unused"));
        endpoint.fail_construction = true;
        endpoint
    }
}

impl EndpointSpec for PageEndpoint {
    type Body = EmptyBody;
    type Response = PageCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(self.endpoint, self.method.clone(), self.timeout)
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("items")?;
        builder.push_query_value("stable", &"public")?;
        builder.push_pagination_query_value("cursor", &self.cursor)?;
        builder.insert_pagination_header_value("x-page-cursor", &self.cursor)?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, crate::body::BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

impl PaginatedEndpoint for PageEndpoint {
    type Item = String;
    type Controller = CursorController;

    fn initial_controller(&self) -> Result<Self::Controller, PaginationError> {
        self.construction_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_construction {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(CursorController {
            state: self.cursor.clone(),
            validate_calls: Arc::clone(&self.validate_calls),
            next_calls: Arc::clone(&self.next_calls),
            fail_validate: self.fail_validate,
            fail_next: self.fail_next,
            short_page_size: self.short_page_size,
            _not_sync: Cell::new(()),
        })
    }

    fn bind_controller(
        endpoint: &mut Self,
        controller: &Self::Controller,
    ) -> Result<(), PaginationError> {
        endpoint.binding_calls.fetch_add(1, Ordering::SeqCst);
        endpoint.cursor.clone_from(&controller.state);
        Ok(())
    }
}

#[derive(Clone)]
struct HostRateEndpoint {
    host: String,
}

impl EndpointSpec for HostRateEndpoint {
    type Body = EmptyBody;
    type Response = PageCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "pagination_runtime_api.items.host_rate_key",
            Method::GET,
            Some(Duration::from_secs(30)),
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_pagination_host_value(&self.host)?;
        builder.push_static("host-rate-items")?;
        builder.push_query_value("stable", &"public")?;
        builder.push_pagination_query_value("cursor", &self.host)?;
        builder.insert_pagination_header_value("x-page-cursor", &self.host)?;
        let rate_limit_host = builder.rate_limit_host().to_owned();
        builder.push_rate_limit_rule(
            "PaginationRuntimeApi::rate_limit::host_page_key",
            [
                ("host", rate_limit_host.as_str()),
                ("ep.host", self.host.as_str()),
            ],
            1,
            [(100, Duration::from_secs(60))],
        )?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, crate::body::BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

impl PaginatedEndpoint for HostRateEndpoint {
    type Item = String;
    type Controller = CursorController;

    fn initial_controller(&self) -> Result<Self::Controller, PaginationError> {
        Ok(CursorController::new(self.host.clone()))
    }

    fn bind_controller(
        endpoint: &mut Self,
        controller: &Self::Controller,
    ) -> Result<(), PaginationError> {
        endpoint.host.clone_from(&controller.state);
        Ok(())
    }
}

#[derive(Clone)]
#[cfg(feature = "default-tls")]
struct RateKeyEndpoint {
    key: String,
    credential: CredentialRef<AccessToken>,
}

#[cfg(feature = "default-tls")]
impl EndpointSpec for RateKeyEndpoint {
    type Body = EmptyBody;
    type Response = PageCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "pagination_runtime_api.items.rate_key",
            Method::GET,
            Some(Duration::from_secs(30)),
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("rate-key-items")?;
        builder.push_query_value("stable", &"public")?;
        builder.push_pagination_query_value("cursor", &self.key)?;
        builder.insert_pagination_header_value("x-page-cursor", &self.key)?;
        builder.push_auth(Auth::bearer(self.credential.clone()))?;
        builder.push_rate_limit_rule(
            "PaginationRuntimeApi::rate_limit::page_key",
            [("ep.cursor", self.key.as_str())],
            1,
            [(1, Duration::from_secs(60))],
        )?;
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, crate::body::BodyError> {
        EmptyBody::metadata(&(), context)
    }

    fn into_body(self) {}
}

#[cfg(feature = "default-tls")]
impl PaginatedEndpoint for RateKeyEndpoint {
    type Item = String;
    type Controller = CursorController;

    fn initial_controller(&self) -> Result<Self::Controller, PaginationError> {
        Ok(CursorController::new(self.key.clone()))
    }

    fn bind_controller(
        endpoint: &mut Self,
        controller: &Self::Controller,
    ) -> Result<(), PaginationError> {
        endpoint.key.clone_from(&controller.state);
        Ok(())
    }
}

#[cfg(feature = "default-tls")]
struct CountingRateKeyProvider {
    calls: Arc<AtomicUsize>,
}

#[cfg(feature = "default-tls")]
impl CredentialProvider for CountingRateKeyProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, crate::auth::AuthError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(
                AccessToken::try_new("rate-key-auth-token-8b71")
                    .expect("bounded test access token"),
            )
        })
    }
}

struct CursorController {
    state: String,
    validate_calls: Arc<AtomicUsize>,
    next_calls: Arc<AtomicUsize>,
    fail_validate: bool,
    fail_next: bool,
    short_page_size: Option<NonZeroUsize>,
    _not_sync: Cell<()>,
}

impl CursorController {
    fn new(state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            validate_calls: Arc::new(AtomicUsize::new(0)),
            next_calls: Arc::new(AtomicUsize::new(0)),
            fail_validate: false,
            fail_next: false,
            short_page_size: None,
            _not_sync: Cell::new(()),
        }
    }

    fn failing_validate(state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            validate_calls: Arc::new(AtomicUsize::new(0)),
            next_calls: Arc::new(AtomicUsize::new(0)),
            fail_validate: true,
            fail_next: false,
            short_page_size: None,
            _not_sync: Cell::new(()),
        }
    }

    fn failing_next(state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            validate_calls: Arc::new(AtomicUsize::new(0)),
            next_calls: Arc::new(AtomicUsize::new(0)),
            fail_validate: false,
            fail_next: true,
            short_page_size: None,
            _not_sync: Cell::new(()),
        }
    }

    fn with_short_page_size(mut self, value: NonZeroUsize) -> Self {
        self.short_page_size = Some(value);
        self
    }
}

impl PaginationController<TestPage> for CursorController {
    fn validate(&self) -> Result<(), PaginationError> {
        self.validate_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_validate {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(())
    }

    fn next(self, page: &TestPage) -> Result<Option<Self>, PaginationError> {
        self.next_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_next {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(page.next.as_ref().map(|state| Self {
            state: state.clone(),
            validate_calls: Arc::clone(&self.validate_calls),
            next_calls: Arc::clone(&self.next_calls),
            fail_validate: false,
            fail_next: false,
            short_page_size: self.short_page_size,
            _not_sync: Cell::new(()),
        }))
    }

    fn short_page_size(&self) -> Option<NonZeroUsize> {
        self.short_page_size
    }
}

#[derive(Clone)]
struct BeforeRecord {
    operation_id: OperationId,
    parent_operation_id: Option<OperationId>,
    page_index: u32,
    recovery_index: u8,
    safe: String,
}

#[derive(Clone)]
struct CompletionRecord {
    operation_id: OperationId,
    parent_operation_id: Option<OperationId>,
    page_index: u32,
    recovered_authentication: bool,
    final_status: Option<StatusCode>,
    outcome: CompletionOutcome,
    elapsed: Duration,
    safe: String,
}

#[derive(Clone)]
struct AggregateRecord {
    operation_id: OperationId,
    pages_completed: u32,
    items_collected: usize,
    recovered_authentication: bool,
    final_status: Option<StatusCode>,
    outcome: CompletionOutcome,
    elapsed: Duration,
}

type ResponseRecord = (
    OperationId,
    Option<OperationId>,
    u32,
    u8,
    StatusCode,
    String,
);

#[derive(Default)]
struct PaginationProbe {
    before: Mutex<Vec<BeforeRecord>>,
    response: Mutex<Vec<ResponseRecord>>,
    completion: Mutex<Vec<CompletionRecord>>,
    aggregate: Mutex<Vec<AggregateRecord>>,
    page_zero_completion_gate: Option<Gate>,
    aggregate_gate: Option<Gate>,
}

impl PaginationProbe {
    fn with_page_zero_completion_gate(gate: Gate) -> Self {
        Self {
            page_zero_completion_gate: Some(gate),
            ..Self::default()
        }
    }

    fn with_aggregate_gate(gate: Gate) -> Self {
        Self {
            aggregate_gate: Some(gate),
            ..Self::default()
        }
    }
}

impl RuntimeHooks for PaginationProbe {
    fn before_send<'a>(
        &'a self,
        context: BeforeSend<'a>,
    ) -> BoxFuture<'a, Result<(), crate::error::HookError>> {
        self.before.lock().expect("before lock").push(BeforeRecord {
            operation_id: context.operation_id,
            parent_operation_id: context.parent_operation_id,
            page_index: context.page_index,
            recovery_index: context.recovery_index,
            safe: format!("{} {:?}", context.safe_url, context.headers),
        });
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(&'a self, context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        self.response.lock().expect("response lock").push((
            context.operation_id,
            context.parent_operation_id,
            context.page_index,
            context.recovery_index,
            context.status,
            format!("{} {:?}", context.safe_url, context.headers),
        ));
        Box::pin(async {})
    }

    fn on_complete<'a>(&'a self, context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.completion
            .lock()
            .expect("completion lock")
            .push(CompletionRecord {
                operation_id: context.operation_id,
                parent_operation_id: context.parent_operation_id,
                page_index: context.page_index,
                recovered_authentication: context.recovered_authentication,
                final_status: context.final_status,
                outcome: context.outcome,
                elapsed: context.elapsed,
                safe: context.safe_url.to_owned(),
            });
        let gate = (context.page_index == 0)
            .then(|| self.page_zero_completion_gate.clone())
            .flatten();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
        })
    }

    fn on_pagination_complete<'a>(
        &'a self,
        context: PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        self.aggregate
            .lock()
            .expect("aggregate lock")
            .push(AggregateRecord {
                operation_id: context.operation_id,
                pages_completed: context.pages_completed,
                items_collected: context.items_collected,
                recovered_authentication: context.recovered_authentication,
                final_status: context.final_status,
                outcome: context.outcome,
                elapsed: context.elapsed,
            });
        let gate = self.aggregate_gate.clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
        })
    }
}

#[derive(Default)]
struct LimiterProbe {
    acquisitions: Mutex<Vec<(u32, String)>>,
    observations: Mutex<Vec<(u32, StatusCode)>>,
}

impl RateLimiter for LimiterProbe {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions
            .lock()
            .expect("acquisition lock")
            .push((request.page_index, request.safe_url.to_owned()));
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        self.observations
            .lock()
            .expect("observation lock")
            .push((response.request.page_index, response.status));
        Box::pin(async {})
    }
}

#[derive(Default)]
struct RejectingLimiter {
    acquisitions: AtomicUsize,
}

type HostLimiterContext = (u32, String, Option<String>, Vec<(String, String)>);

#[derive(Default)]
struct HostLimiterProbe {
    acquisitions: Mutex<Vec<HostLimiterContext>>,
    observations: Mutex<Vec<HostLimiterContext>>,
}

impl HostLimiterProbe {
    fn capture(request: &RateLimitRequest<'_>) -> HostLimiterContext {
        (
            request.page_index,
            request.safe_url.to_owned(),
            request.host.map(str::to_owned),
            request
                .plan
                .rules()
                .iter()
                .flat_map(|rule| rule.key().parts())
                .map(|part| (part.name().to_owned(), part.value().to_owned()))
                .collect(),
        )
    }
}

impl RateLimiter for HostLimiterProbe {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions
            .lock()
            .expect("host limiter acquisitions")
            .push(Self::capture(&request));
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        self.observations
            .lock()
            .expect("host limiter observations")
            .push(Self::capture(&response.request));
        Box::pin(async {})
    }
}

impl RateLimiter for RejectingLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(RateLimitError::new(RateLimitErrorKind::Acquire)) })
    }
}

#[cfg(feature = "default-tls")]
struct KeyBoundLimiter {
    acquisitions: AtomicUsize,
    key_limit: usize,
}

#[cfg(feature = "default-tls")]
impl RateLimiter for KeyBoundLimiter {
    fn preparation_key_limit(&self) -> usize {
        self.key_limit
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

fn core(
    executor: &DeterministicExecutor,
    hooks: Arc<PaginationProbe>,
    limiter: Arc<LimiterProbe>,
    configure: impl FnOnce(RuntimeConfigBuilder) -> RuntimeConfigBuilder,
) -> ClientCore {
    let hooks: Arc<dyn RuntimeHooks> = hooks;
    let limiter: Arc<dyn RateLimiter> = limiter;
    ClientCore::build_with_development(
        "http://pagination-runtime.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        configure(
            RuntimeConfigBuilder::new()
                .hooks(hooks)
                .rate_limiter(limiter),
        ),
        executor,
    )
    .expect("pagination development core")
}

fn collect(
    core: ClientCore,
    limit: PageLimit,
    controller: CursorController,
    timeout: Option<Duration>,
) -> BoxFuture<'static, Result<Vec<String>, ApiError>> {
    paginate_with(
        Request::new(core, PageEndpoint::get(timeout, controller)),
        limit,
    )
}

fn pagination_error(result: Result<Vec<String>, ApiError>) -> ApiError {
    result.expect_err("pagination operation should fail")
}

mod completion;
mod identity;
mod limits;
mod sequence;
