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
use super::{
    PageDecisionLimits, PageLimit, PageResponse, PageStep, PaginatedEndpoint, PaginationController,
    PaginationDescription, PaginationError,
};
#[cfg(feature = "default-tls")]
use crate::auth::{
    AccessToken, Auth, CredentialProvider, CredentialRef, CredentialRequest, SlotSelector,
};
use crate::body::{EmptyBody, RequestBodyContext, RequestBodyMetadata, RequestRepresentation};
use crate::client::ClientCore;
use crate::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use crate::endpoint::AcceptedStatus;
use crate::error::{ApiError, ErrorCategory, PaginationErrorKind, RateLimitErrorKind};
use crate::hooks::{
    AttemptInfo, BoxFuture, CompletionOutcome, DebugLevel, OperationEvent, OperationObserver,
};
use crate::http::HttpConfigBuilder;
use crate::operation::OperationId;
use crate::rate_limit::{
    RateLimitError, RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimiter,
};
use crate::request::{EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta};
use crate::response::{
    Buffered, PageResponseRepresentation, ResponseBodyError, ResponseBodyErrorKind, ResponseHead,
    ResponseRepresentation,
};
use crate::retry::{RetryDecision, RetryFailure, RetryPolicy};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "pagination_runtime_api.items.list";
const INITIAL_ATTEMPT: AttemptInfo = AttemptInfo {
    index: 0,
    policy_retry_index: 0,
    authentication_recovery: false,
};

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
        decode_test_page(body)
    }
}

fn decode_test_page(body: Bytes) -> Result<TestPage, CodecError> {
    let text = std::str::from_utf8(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
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

struct AcceptedPageSpec;

impl ResponseRepresentation<TestPage> for AcceptedPageSpec {
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<TestPage, ResponseBodyError> {
        decode_test_page(body).map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
    }
}

impl PageResponseRepresentation<TestPage> for AcceptedPageSpec {}

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
    type Input = ();
    type Body = EmptyBody;
    type Output = TestPage;
    type Response = PageCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(self.endpoint, self.method.clone(), self.timeout)
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("items")?;
        builder.push_query_value("stable", &"public")?;
        builder.push_pagination_query_value("cursor", &self.cursor)?;
        builder.insert_pagination_header_value("x-page-cursor", &self.cursor)?;
        builder.finish()
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
struct AcceptedPageEndpoint(PageEndpoint);

impl EndpointSpec for AcceptedPageEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = TestPage;
    type Response = AcceptedPageSpec;

    fn request_meta(&self) -> RequestMeta {
        self.0.request_meta().with_accepted_status(
            AcceptedStatus::codes([StatusCode::NOT_FOUND]).expect("nonempty test status set"),
        )
    }

    fn prepare_head(&self, builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        self.0.prepare_head(builder)
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, crate::body::BodyError> {
        self.0.body_metadata(context)
    }

    fn into_body(self) {}
}

impl PaginatedEndpoint for AcceptedPageEndpoint {
    type Controller = CursorController;

    fn initial_controller(&self) -> Result<Self::Controller, PaginationError> {
        self.0.initial_controller()
    }

    fn bind_controller(
        endpoint: &mut Self,
        controller: &Self::Controller,
    ) -> Result<(), PaginationError> {
        PageEndpoint::bind_controller(&mut endpoint.0, controller)
    }
}

#[derive(Clone)]
struct HostRateEndpoint {
    host: String,
}

impl EndpointSpec for HostRateEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = TestPage;
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
        let rate_limit_origin = builder.rate_limit_origin();
        builder.push_rate_limit_policy(
            "PaginationRuntimeApi::rate_limit::host_page_key",
            [
                ("origin", rate_limit_origin.as_str()),
                ("endpoint.host", self.host.as_str()),
            ],
            1,
            [(100, Duration::from_secs(60))],
        )?;
        builder.finish()
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
    type Input = ();
    type Body = EmptyBody;
    type Output = TestPage;
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
        builder.push_rate_limit_policy(
            "PaginationRuntimeApi::rate_limit::page_key",
            [("endpoint.cursor", self.key.as_str())],
            1,
            [(1, Duration::from_secs(60))],
        )?;
        builder.finish()
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
    type Item = String;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn validate(&self) -> Result<(), PaginationError> {
        self.validate_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_validate {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(())
    }

    fn advance(
        self,
        page: PageResponse<TestPage>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        self.next_calls.fetch_add(1, Ordering::SeqCst);
        let next_state = page.page().next.clone();
        let items = page.into_page().items;
        if items.len() > limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        if self
            .short_page_size
            .is_some_and(|size| items.len() < size.get())
        {
            return Ok(PageStep::Done { items });
        }
        if self.fail_next {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        let next = next_state.map(|state| Self {
            state,
            validate_calls: Arc::clone(&self.validate_calls),
            next_calls: Arc::clone(&self.next_calls),
            fail_validate: false,
            fail_next: false,
            short_page_size: self.short_page_size,
            _not_sync: Cell::new(()),
        });
        Ok(match next {
            Some(next) => PageStep::Continue { items, next },
            None => PageStep::Done { items },
        })
    }
}

#[derive(Clone)]
struct BeforeRecord {
    operation_id: OperationId,
    parent_operation_id: Option<OperationId>,
    page_index: u32,
    attempt: AttemptInfo,
    safe: String,
}

#[derive(Clone)]
struct CompletionRecord {
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
    attempts_started: u32,
    policy_retries: u32,
    elapsed: Duration,
}

type ResponseRecord = (
    OperationId,
    Option<OperationId>,
    u32,
    AttemptInfo,
    StatusCode,
    String,
);

#[derive(Default)]
struct PaginationProbe {
    before: Mutex<Vec<BeforeRecord>>,
    response: Mutex<Vec<ResponseRecord>>,
    completion: Mutex<Vec<CompletionRecord>>,
    aggregate: Mutex<Vec<AggregateRecord>>,
}

impl OperationObserver for PaginationProbe {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(context) => {
                self.before.lock().expect("before lock").push(BeforeRecord {
                    operation_id: context.operation_id,
                    parent_operation_id: context.parent_operation_id,
                    page_index: context.page_index,
                    attempt: context.attempt,
                    safe: format!("{} {:?}", context.observed_url, context.headers),
                });
            }
            OperationEvent::ResponseHeadObserved(context) => {
                self.response.lock().expect("response lock").push((
                    context.operation_id,
                    context.parent_operation_id,
                    context.page_index,
                    context.attempt,
                    context.status,
                    format!("{} {:?}", context.observed_url, context.headers),
                ));
            }
            OperationEvent::OperationFinished(context) => {
                self.completion
                    .lock()
                    .expect("completion lock")
                    .push(CompletionRecord {
                        safe: context.observed_url.to_owned(),
                    });
            }
            OperationEvent::PaginationFinished(context) => {
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
                        attempts_started: context.attempts_started,
                        policy_retries: context.policy_retries,
                        elapsed: context.elapsed,
                    });
            }
            _ => {}
        }
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
            .push((request.page_index, request.observed_url.to_owned()));
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        self.observations
            .lock()
            .expect("observation lock")
            .push((response.request.page_index, response.status));
    }
}

#[derive(Default)]
struct RejectingLimiter {
    acquisitions: AtomicUsize,
}

type HostLimiterContext = (u32, String, Option<String>, Vec<(String, Vec<u8>)>);

#[derive(Default)]
struct HostLimiterProbe {
    acquisitions: Mutex<Vec<HostLimiterContext>>,
    observations: Mutex<Vec<HostLimiterContext>>,
}

impl HostLimiterProbe {
    fn capture(request: &RateLimitRequest<'_>) -> HostLimiterContext {
        (
            request.page_index,
            request.observed_url.to_owned(),
            request.host.map(str::to_owned),
            request
                .plan
                .policies()
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

    fn observe(&self, response: RateLimitResponse<'_>) {
        self.observations
            .lock()
            .expect("host limiter observations")
            .push(Self::capture(&response.request));
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
    let limiter: Arc<dyn RateLimiter> = limiter;
    ClientCore::build_with_development(
        "http://pagination-runtime.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        configure(
            RuntimeConfigBuilder::new()
                .operation_observer(hooks.clone())
                .operation_observer(hooks.clone())
                .operation_observer(hooks)
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
