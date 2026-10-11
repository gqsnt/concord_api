//! Deterministic traversal tests over the development transport.
//!
//! Pages use the test wire format `item,item|next` where `-` means "no next
//! cursor" and an empty body is an empty terminal page.

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

use super::{
    CollectAll, CollectPrefix, ContinuationKnowledge, ControllerCursor, ControllerInit,
    PageDecisionLimits, PageResponse, PageStep, PageableEndpoint, Pages, PaginationController,
    PaginationDescription, PaginationError, Prefix, StopReason, TraversalSummary,
    collect_all_request, collect_prefix_request, pages_request,
};
#[cfg(feature = "default-tls")]
use crate::auth::{
    AccessToken, Auth, CredentialProvider, CredentialRef, CredentialRequest, SlotSelector,
};
use crate::body::EmptyBody;
use crate::client::ClientCore;
use crate::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use crate::controls::{CallOptions, CancellationToken};
use crate::endpoint::{AcceptedStatus, Effect};
use crate::error::{
    ApiError, DispatchKnowledge, ErrorCategory, PaginationErrorKind, RateLimitErrorKind,
};
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
    Buffered, ResponseBodyError, ResponseBodyErrorKind, ResponseHead, ResponseRepresentation,
};
use crate::retry::{RetryDecision, RetryFailure, RetryPolicy};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "pagination_runtime_api.items.list";
const BASE: &str = "http://pagination-runtime.example.test/v1/";
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

impl DecodeBody for PageCodec {
    type Output = TestPage;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        decode_test_page(&body)
    }
}

fn decode_test_page(body: &[u8]) -> Result<TestPage, CodecError> {
    let text = std::str::from_utf8(body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
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

/// A buffered representation that is not a `DecodeBody` codec and has no
/// iterable/page marker.
struct AcceptedPageSpec;

impl ResponseRepresentation<TestPage> for AcceptedPageSpec {
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<TestPage, ResponseBodyError> {
        decode_test_page(&body).map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
    }
}

/// Shared counters proving which extension calls happened.
#[derive(Clone, Default)]
struct Calls {
    starts: Arc<AtomicUsize>,
    advances: Arc<AtomicUsize>,
    cursors: Arc<AtomicUsize>,
    inits: Arc<AtomicUsize>,
    applies: Arc<AtomicUsize>,
    clones: Arc<AtomicUsize>,
}

impl Calls {
    fn get(counter: &AtomicUsize) -> usize {
        counter.load(Ordering::SeqCst)
    }
}

/// Public initialization of the test controller.
struct CursorInit {
    cursor: String,
    calls: Calls,
    fail_start: bool,
    fail_advance: bool,
    short_page_size: Option<NonZeroUsize>,
    /// Replaces a provider cursor before it is proposed (to force invalid,
    /// repeated or cyclic continuations).
    rewrite: Option<fn(&str) -> String>,
}

/// Non-`Clone`, non-`Sync` controller with private state.
struct CursorController {
    state: String,
    calls: Calls,
    fail_advance: bool,
    short_page_size: Option<NonZeroUsize>,
    rewrite: Option<fn(&str) -> String>,
    _not_sync: Cell<()>,
}

impl PaginationController<TestPage> for CursorController {
    type Item = String;
    type Init = CursorInit;
    type Cursor = String;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: CursorInit) -> Result<Self, PaginationError> {
        init.calls.starts.fetch_add(1, Ordering::SeqCst);
        if init.fail_start {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(Self {
            state: init.cursor,
            calls: init.calls,
            fail_advance: init.fail_advance,
            short_page_size: init.short_page_size,
            rewrite: init.rewrite,
            _not_sync: Cell::new(()),
        })
    }

    fn cursor(&self) -> String {
        self.calls.cursors.fetch_add(1, Ordering::SeqCst);
        self.state.clone()
    }

    fn advance(
        self,
        page: PageResponse<TestPage>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        self.calls.advances.fetch_add(1, Ordering::SeqCst);
        let TestPage { items, next } = page.into_page();
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
        if self.fail_advance {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(match next {
            Some(next) => PageStep::Continue {
                next: Self {
                    state: self.rewrite.map_or(next.clone(), |rewrite| rewrite(&next)),
                    calls: self.calls.clone(),
                    fail_advance: false,
                    short_page_size: self.short_page_size,
                    rewrite: self.rewrite,
                    _not_sync: Cell::new(()),
                },
                items,
            },
            None => PageStep::Done { items },
        })
    }
}

/// Bodyless test endpoint with one query and one header cursor slot.
#[derive(Clone)]
struct PageEndpoint {
    endpoint: &'static str,
    effect: Effect,
    timeout: Option<Duration>,
    accepted: Option<AcceptedStatus>,
    cursor: String,
    /// When set, the controller starts from this cursor instead of the slot.
    init_cursor: Option<String>,
    calls: Calls,
    fail_start: bool,
    fail_advance: bool,
    short_page_size: Option<NonZeroUsize>,
    rewrite: Option<fn(&str) -> String>,
}

impl PageEndpoint {
    fn new(cursor: &str) -> Self {
        Self {
            endpoint: ENDPOINT,
            effect: Effect::Safe,
            timeout: Some(Duration::from_secs(30)),
            accepted: None,
            cursor: cursor.to_owned(),
            init_cursor: None,
            calls: Calls::default(),
            fail_start: false,
            fail_advance: false,
            short_page_size: None,
            rewrite: None,
        }
    }

    fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl EndpointSpec for PageEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = TestPage;
    type Response = PageCodec;

    fn request_meta(&self) -> RequestMeta {
        let method = if self.effect == Effect::Safe {
            Method::GET
        } else {
            Method::POST
        };
        RequestMeta::new_with_contract(
            self.endpoint,
            method,
            self.timeout,
            self.accepted.clone().unwrap_or(AcceptedStatus::Success),
            self.effect,
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("items")?;
        builder.push_query_value("stable", &"public")?;
        builder.push_pagination_query_value("cursor", &self.cursor)?;
        builder.insert_pagination_header_value("x-page-cursor", &self.cursor)?;
        builder.finish()
    }

    fn into_body(self) {}
}

impl PageableEndpoint for PageEndpoint {
    type Controller = CursorController;

    fn controller_init(&self) -> ControllerInit<Self> {
        self.calls.inits.fetch_add(1, Ordering::SeqCst);
        CursorInit {
            cursor: self
                .init_cursor
                .clone()
                .unwrap_or_else(|| self.cursor.clone()),
            calls: self.calls.clone(),
            fail_start: self.fail_start,
            fail_advance: self.fail_advance,
            short_page_size: self.short_page_size,
            rewrite: self.rewrite,
        }
    }

    fn apply_cursor(&mut self, cursor: ControllerCursor<Self>) {
        self.calls.applies.fetch_add(1, Ordering::SeqCst);
        self.cursor = cursor;
    }

    fn page_clone(&self) -> Self {
        self.calls.clones.fetch_add(1, Ordering::SeqCst);
        self.clone()
    }

    fn page_body_present(&self) -> bool {
        false
    }
}

/// The same endpoint decoded through a non-codec buffered representation
/// with an explicit accepted-status set.
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

    fn into_body(self) {}
}

impl PageableEndpoint for AcceptedPageEndpoint {
    type Controller = CursorController;

    fn controller_init(&self) -> ControllerInit<Self> {
        self.0.controller_init()
    }

    fn apply_cursor(&mut self, cursor: ControllerCursor<Self>) {
        self.0.apply_cursor(cursor);
    }

    fn page_clone(&self) -> Self {
        Self(self.0.page_clone())
    }

    fn page_body_present(&self) -> bool {
        false
    }
}

/// An endpoint whose cursor destination illegally feeds the host label and a
/// rate-limit key dimension (forbidden in v3; the macro rejects it, and core
/// rejects it at runtime as defense in depth).
#[derive(Clone)]
struct ForbiddenDestinationEndpoint {
    host: String,
    rate_dimension: String,
    move_host: bool,
    calls: Calls,
}

impl EndpointSpec for ForbiddenDestinationEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = TestPage;
    type Response = PageCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "pagination_runtime_api.items.forbidden_destination",
            Method::GET,
            Some(Duration::from_secs(30)),
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_pagination_host_value(&self.host)?;
        builder.push_static("host-rate-items")?;
        builder.push_query_value("stable", &"public")?;
        builder.push_pagination_query_value("cursor", &self.rate_dimension)?;
        builder.push_rate_limit(
            crate::rate_limit::RateLimitDeclaration::new(
                "PaginationRuntimeApi::rate_limit::host_page_key",
            )
            .window(100, Duration::from_secs(60))
            .origin()
            .part("endpoint.cursor", self.rate_dimension.as_str()),
        )?;
        builder.finish()
    }

    fn into_body(self) {}
}

impl PageableEndpoint for ForbiddenDestinationEndpoint {
    type Controller = CursorController;

    fn controller_init(&self) -> ControllerInit<Self> {
        CursorInit {
            cursor: self.rate_dimension.clone(),
            calls: self.calls.clone(),
            fail_start: false,
            fail_advance: false,
            short_page_size: None,
            rewrite: None,
        }
    }

    fn apply_cursor(&mut self, cursor: ControllerCursor<Self>) {
        if self.move_host {
            self.host.clone_from(&cursor);
        }
        self.rate_dimension = cursor;
    }

    fn page_clone(&self) -> Self {
        self.clone()
    }

    fn page_body_present(&self) -> bool {
        false
    }
}

#[derive(Clone)]
#[cfg(feature = "default-tls")]
struct RateKeyEndpoint {
    key: String,
    credential: CredentialRef<AccessToken>,
    calls: Calls,
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
        builder.push_rate_limit(
            crate::rate_limit::RateLimitDeclaration::new(
                "PaginationRuntimeApi::rate_limit::page_key",
            )
            .window(1, Duration::from_secs(60))
            .part("endpoint.cursor", self.key.as_str()),
        )?;
        builder.finish()
    }

    fn into_body(self) {}
}

#[cfg(feature = "default-tls")]
impl PageableEndpoint for RateKeyEndpoint {
    type Controller = CursorController;

    fn controller_init(&self) -> ControllerInit<Self> {
        CursorInit {
            cursor: self.key.clone(),
            calls: self.calls.clone(),
            fail_start: false,
            fail_advance: false,
            short_page_size: None,
            rewrite: None,
        }
    }

    fn apply_cursor(&mut self, cursor: ControllerCursor<Self>) {
        self.key = cursor;
    }

    fn page_clone(&self) -> Self {
        self.clone()
    }

    fn page_body_present(&self) -> bool {
        false
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
    stop_reason: StopReason,
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

impl PaginationProbe {
    fn aggregate(&self) -> Vec<AggregateRecord> {
        self.aggregate.lock().expect("aggregate lock").clone()
    }

    fn before_count(&self) -> usize {
        self.before.lock().expect("before lock").len()
    }
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
                    safe: format!("{:?}", context.target),
                });
            }
            OperationEvent::ResponseHeadObserved(context) => {
                self.response.lock().expect("response lock").push((
                    context.operation_id,
                    context.parent_operation_id,
                    context.page_index,
                    context.attempt,
                    context.status,
                    format!(
                        "{:?} content_length={:?} server_delay={:?}",
                        context.target, context.content_length, context.server_delay
                    ),
                ));
            }
            OperationEvent::OperationFinished(context) => {
                self.completion
                    .lock()
                    .expect("completion lock")
                    .push(CompletionRecord {
                        safe: format!("{:?}", context.target),
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
                        stop_reason: context.stop_reason,
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
    acquisitions: Mutex<Vec<(u32, Option<String>)>>,
    observations: Mutex<Vec<(u32, StatusCode)>>,
}

impl LimiterProbe {
    fn acquisition_pages(&self) -> Vec<u32> {
        self.acquisitions
            .lock()
            .expect("acquisition lock")
            .iter()
            .map(|(page, _)| *page)
            .collect()
    }
}

impl RateLimiter for LimiterProbe {
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "test.limiter-probe",
            1,
        )
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.lock().expect("acquisition lock").push((
            request.page_index,
            request.logical_origin.map(str::to_owned),
        ));
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

impl RateLimiter for RejectingLimiter {
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "test.rejecting-limiter",
            1,
        )
    }

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
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "test.key-bound-limiter",
            1,
        )
    }

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

/// One deterministic client plus its probes.
struct Harness {
    executor: DeterministicExecutor,
    hooks: Arc<PaginationProbe>,
    limiter: Arc<LimiterProbe>,
    core: ClientCore,
}

impl Harness {
    fn new() -> Self {
        Self::configured(|runtime| runtime)
    }

    fn configured(configure: impl FnOnce(RuntimeConfigBuilder) -> RuntimeConfigBuilder) -> Self {
        let executor = DeterministicExecutor::new();
        let hooks = Arc::new(PaginationProbe::default());
        let limiter = Arc::new(LimiterProbe::default());
        let core = core(
            &executor,
            Arc::clone(&hooks),
            Arc::clone(&limiter),
            configure,
        );
        Self {
            executor,
            hooks,
            limiter,
            core,
        }
    }

    fn queue(&self, body: &'static str) {
        self.executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::from_static(
                body.as_bytes(),
            )))
            .expect("queued page");
    }

    fn queue_status(&self, status: StatusCode, body: &'static str) {
        self.executor
            .application()
            .queue_response(ScriptedResponse::status(
                status,
                Bytes::from_static(body.as_bytes()),
            ))
            .expect("queued status page");
    }

    fn requests(&self) -> usize {
        self.executor
            .application()
            .request_count()
            .expect("request count")
    }

    fn request(&self, endpoint: PageEndpoint) -> Request<PageEndpoint> {
        Request::new(self.core.clone(), endpoint)
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
        BASE,
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

fn prefix(
    core: ClientCore,
    endpoint: PageEndpoint,
    maximum_pages: u32,
) -> BoxFuture<'static, Result<Prefix<String>, ApiError>> {
    Box::pin(collect_prefix_request(Request::new(core, endpoint), maximum_pages).into_future())
}

fn all(
    core: ClientCore,
    endpoint: PageEndpoint,
    maximum_pages: u32,
) -> BoxFuture<'static, Result<Vec<String>, ApiError>> {
    Box::pin(
        collect_all_request(
            Request::new(core, endpoint),
            NonZeroU32::new(maximum_pages).expect("nonzero test bound"),
        )
        .into_future(),
    )
}

fn summary_of(error: &ApiError) -> &TraversalSummary {
    error
        .traversal_summary()
        .expect("traversal errors carry their summary")
}

use std::future::IntoFuture as _;

mod bodies;
mod completion;
mod envelope;
mod identity;
mod journal;
mod limits;
mod sequence;
mod terminals;
