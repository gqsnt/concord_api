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

use super::{PageLimit, PaginationController, PaginationError, PaginationRequest, paginate_with};
use crate::body::EmptyBody;
use crate::client::ClientCore;
use crate::codec::{CodecError, DecodeBody, DecodeContext};
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
use crate::request::{
    BodyMetadata, BufferedResponse, EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta,
};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "PaginationRuntimeApi::items::list";

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
        let text = std::str::from_utf8(&body).map_err(|_| CodecError::new("page utf8"))?;
        if text.is_empty() {
            return Ok(TestPage {
                items: Vec::new(),
                next: None,
            });
        }
        let (items, next) = text
            .split_once('|')
            .ok_or_else(|| CodecError::new("page structure"))?;
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
}

impl PageEndpoint {
    fn get(timeout: Option<Duration>) -> Self {
        Self {
            endpoint: ENDPOINT,
            method: Method::GET,
            timeout,
        }
    }
}

impl EndpointSpec for PageEndpoint {
    type Body = EmptyBody;
    type Response = BufferedResponse<PageCodec>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(self.endpoint, self.method.clone(), self.timeout)
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("items")?;
        builder.push_query_value("stable", &"public");
        Ok((builder.finish(), EmptyBody::metadata()))
    }

    fn into_body(self) -> Self::Body {
        EmptyBody::new()
    }
}

#[derive(Clone)]
struct CursorController {
    state: String,
    apply_calls: Arc<AtomicUsize>,
    next_calls: Arc<AtomicUsize>,
    fail_apply: bool,
    fail_next: bool,
}

impl CursorController {
    fn new(state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            apply_calls: Arc::new(AtomicUsize::new(0)),
            next_calls: Arc::new(AtomicUsize::new(0)),
            fail_apply: false,
            fail_next: false,
        }
    }

    fn failing_apply(state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            apply_calls: Arc::new(AtomicUsize::new(0)),
            next_calls: Arc::new(AtomicUsize::new(0)),
            fail_apply: true,
            fail_next: false,
        }
    }

    fn failing_next(state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            apply_calls: Arc::new(AtomicUsize::new(0)),
            next_calls: Arc::new(AtomicUsize::new(0)),
            fail_apply: false,
            fail_next: true,
        }
    }
}

impl PaginationController<TestPage> for CursorController {
    fn apply(&self, request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        self.apply_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_apply {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        request.set_query("cursor", &self.state)?;
        request.set_header(http::HeaderName::from_static("x-page-cursor"), &self.state)
    }

    fn next(&self, page: &TestPage) -> Result<Option<Self>, PaginationError> {
        self.next_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_next {
            return Err(PaginationError::new(PaginationErrorKind::Controller));
        }
        Ok(page.next.as_ref().map(|state| Self {
            state: state.clone(),
            apply_calls: Arc::clone(&self.apply_calls),
            next_calls: Arc::clone(&self.next_calls),
            fail_apply: false,
            fail_next: false,
        }))
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

type ResponseRecord = (OperationId, Option<OperationId>, u32, u8, StatusCode);

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

impl RateLimiter for RejectingLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(RateLimitError::new(RateLimitErrorKind::Acquire)) })
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
        "https://pagination-runtime.example.test/v1/",
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
        Request::new(core, PageEndpoint::get(timeout)),
        limit,
        move |_endpoint| Ok(controller),
        |_endpoint, _controller| Ok(()),
        |_controller| None,
    )
}

fn pagination_error(result: Result<Vec<String>, ApiError>) -> ApiError {
    result.expect_err("pagination operation should fail")
}

#[tokio::test]
async fn page_lifecycle_is_sequential_correlated_and_limited_once_per_page() {
    const FIRST_CURSOR: &str = "first-cursor-sentinel-a271";
    const SECOND_CURSOR: &str = "second-cursor-sentinel-f804";

    let executor = DeterministicExecutor::new();
    let delivery_gate = Gate::closed();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url(format!(
                    "https://pagination-runtime.example.test/v1/items?stable=public&cursor={FIRST_CURSOR}"
                ))
                .header(
                    http::HeaderName::from_static("x-page-cursor"),
                    http::HeaderValue::from_static(FIRST_CURSOR),
                ),
            ScriptedResponse::success(Bytes::from_static(
                b"first-item-sentinel-110b|second-cursor-sentinel-f804",
            ))
            .delivery_gate(delivery_gate.clone()),
        )
        .expect("first page response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url(format!(
                    "https://pagination-runtime.example.test/v1/items?stable=public&cursor={SECOND_CURSOR}"
                ))
                .header(
                    http::HeaderName::from_static("x-page-cursor"),
                    http::HeaderValue::from_static(SECOND_CURSOR),
                ),
            ScriptedResponse::success(Bytes::from_static(b"second-item-sentinel-7d92|-")),
        )
        .expect("second page response");
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        |runtime| runtime.debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));

    let operation = tokio::spawn(collect(
        core,
        PageLimit::Take(5),
        CursorController::new(FIRST_CURSOR),
        Some(Duration::from_secs(30)),
    ));
    delivery_gate.wait_until_reached().await;

    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        1,
        "the second page cannot start while the first response is active"
    );
    assert_eq!(
        limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .iter()
            .map(|(page, _)| *page)
            .collect::<Vec<_>>(),
        [0]
    );

    delivery_gate.open();
    let items = operation
        .await
        .expect("pagination task")
        .expect("pagination success");
    assert_eq!(
        items,
        ["first-item-sentinel-110b", "second-item-sentinel-7d92"]
    );
    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        2
    );
    for capture in executor.application().captures().expect("captures") {
        assert!(capture.headers().iter().any(|header| {
            header.name() == "x-page-cursor" && header.value() == &CapturedHeaderValue::Redacted
        }));
        let rendered = format!("{capture:?}");
        assert!(!rendered.contains(FIRST_CURSOR));
        assert!(!rendered.contains(SECOND_CURSOR));
    }

    let before = hooks.before.lock().expect("before lock").clone();
    assert_eq!(before.len(), 2);
    assert_eq!(
        before
            .iter()
            .map(|record| record.page_index)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(before.iter().all(|record| record.recovery_index == 0));
    assert_ne!(before[0].operation_id, before[1].operation_id);
    assert_eq!(before[0].parent_operation_id, before[1].parent_operation_id);
    assert!(before[0].parent_operation_id.is_some());
    for record in &before {
        assert!(!record.safe.contains(FIRST_CURSOR));
        assert!(!record.safe.contains(SECOND_CURSOR));
    }

    let completion = hooks.completion.lock().expect("completion lock").clone();
    assert_eq!(completion.len(), 2);
    for (before, completion) in before.iter().zip(&completion) {
        assert_eq!(completion.operation_id, before.operation_id);
        assert_eq!(completion.parent_operation_id, before.parent_operation_id);
        assert_eq!(completion.page_index, before.page_index);
        assert!(!completion.recovered_authentication);
        assert_eq!(completion.final_status, Some(StatusCode::OK));
        assert_eq!(completion.outcome, CompletionOutcome::Success);
        assert!(completion.elapsed <= Duration::from_secs(30));
    }

    let aggregate = hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(aggregate.len(), 1);
    let aggregate = &aggregate[0];
    assert_eq!(before[0].parent_operation_id, Some(aggregate.operation_id));
    assert_eq!(aggregate.pages_completed, 2);
    assert_eq!(aggregate.items_collected, 2);
    assert!(!aggregate.recovered_authentication);
    assert_eq!(aggregate.final_status, Some(StatusCode::OK));
    assert_eq!(aggregate.outcome, CompletionOutcome::Success);
    assert!(aggregate.elapsed >= completion[0].elapsed);

    assert_eq!(
        limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .iter()
            .map(|(page, safe)| {
                assert!(!safe.contains(FIRST_CURSOR));
                assert!(!safe.contains(SECOND_CURSOR));
                *page
            })
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(
        limiter
            .observations
            .lock()
            .expect("observation lock")
            .as_slice(),
        [(0, StatusCode::OK), (1, StatusCode::OK)]
    );
    let rendered_debug = debug.lock().expect("debug output").join("\n");
    assert!(rendered_debug.contains("pagination-completion"));
    for forbidden in [
        FIRST_CURSOR,
        SECOND_CURSOR,
        "first-item-sentinel-110b",
        "second-item-sentinel-7d92",
    ] {
        assert!(!rendered_debug.contains(forbidden));
    }
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn hidden_reqwest_retry_does_not_repeat_pagination_preparation_or_page_lifecycle() {
    let server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let physical_attempts = server.physical_attempts();
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = hooks.clone();
    let configured_limiter: Arc<dyn RateLimiter> = limiter.clone();
    let core = ClientCore::build(
        &server.base_url(),
        HttpConfigBuilder::new()
            .http2_prior_knowledge()
            .retry_reqwest_default(),
        RuntimeConfigBuilder::new()
            .hooks(configured_hooks)
            .rate_limiter(configured_limiter),
    )
    .expect("HTTP/2 pagination core");
    let controller = CursorController::new("native-retry-cursor-sentinel-f120");
    let apply_calls = Arc::clone(&controller.apply_calls);
    let next_calls = Arc::clone(&controller.next_calls);
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let calls = Arc::clone(&factory_calls);

    let items = paginate_with(
        Request::new(core, PageEndpoint::get(Some(Duration::from_secs(30)))),
        PageLimit::Take(1),
        move |_endpoint| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(controller)
        },
        |_endpoint, _controller| Ok(()),
        |_controller| None,
    )
    .await
    .expect("native retry pagination");
    assert!(items.is_empty());
    server.finish().await;

    assert_eq!(physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert_eq!(apply_calls.load(Ordering::SeqCst), 1);
    assert_eq!(next_calls.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.before.lock().expect("before lock").len(), 1);
    assert_eq!(hooks.response.lock().expect("response lock").len(), 1);
    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 1);
    assert_eq!(hooks.aggregate.lock().expect("aggregate lock").len(), 1);
    assert_eq!(
        limiter.acquisitions.lock().expect("acquisition lock").len(),
        1
    );
    assert_eq!(
        limiter.observations.lock().expect("observation lock").len(),
        1
    );
}

#[tokio::test]
async fn cancellation_drops_request_local_state_and_prevents_later_pages() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"item|next")).delivery_gate(gate.clone()),
        )
        .expect("gated page");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"later|-")))
        .expect("unused later page");
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let core = core(&executor, Arc::clone(&hooks), limiter, |runtime| runtime);
    let task = tokio::spawn(collect(
        core,
        PageLimit::Take(5),
        CursorController::new("initial"),
        Some(Duration::from_secs(30)),
    ));
    gate.wait_until_reached().await;
    task.abort();
    assert!(task.await.expect_err("task cancellation").is_cancelled());
    gate.open();
    tokio::task::yield_now().await;

    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert!(hooks.completion.lock().expect("completion lock").is_empty());
    assert!(hooks.aggregate.lock().expect("aggregate lock").is_empty());
}

#[tokio::test]
async fn separate_collections_execute_concurrently_with_operation_local_identity_state() {
    let executor = DeterministicExecutor::new();
    let execution_gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(execution_gate.clone())
        .expect("execution gate");
    for item in ["first|-", "second|-"] {
        executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::copy_from_slice(
                item.as_bytes(),
            )))
            .expect("concurrent page");
    }
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        |runtime| runtime,
    );

    let first = tokio::spawn(collect(
        core.clone(),
        PageLimit::Take(1),
        CursorController::new("same-operation-local-cursor"),
        Some(Duration::from_secs(30)),
    ));
    let second = tokio::spawn(collect(
        core,
        PageLimit::Take(1),
        CursorController::new("same-operation-local-cursor"),
        Some(Duration::from_secs(30)),
    ));
    execution_gate.wait_until_arrivals(2).await;
    assert_eq!(
        limiter.acquisitions.lock().expect("acquisition lock").len(),
        2
    );
    execution_gate.open();
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.expect("first task").expect("first result").len(), 1);
    assert_eq!(
        second.expect("second task").expect("second result").len(),
        1
    );

    let before = hooks.before.lock().expect("before lock").clone();
    assert_eq!(before.len(), 2);
    assert_ne!(before[0].operation_id, before[1].operation_id);
    assert_ne!(before[0].parent_operation_id, before[1].parent_operation_id);
    assert_eq!(hooks.aggregate.lock().expect("aggregate lock").len(), 2);
}

#[tokio::test]
async fn zero_page_and_configured_ceilings_cut_off_the_first_forbidden_work() {
    let zero_executor = DeterministicExecutor::new();
    let zero_hooks = Arc::new(PaginationProbe::default());
    let zero_limiter = Arc::new(LimiterProbe::default());
    let zero_core = core(
        &zero_executor,
        Arc::clone(&zero_hooks),
        Arc::clone(&zero_limiter),
        |runtime| runtime,
    );
    let zero_factory_calls = Arc::new(AtomicUsize::new(0));
    let calls = Arc::clone(&zero_factory_calls);
    let zero = paginate_with(
        Request::new(zero_core, PageEndpoint::get(Some(Duration::from_secs(30)))),
        PageLimit::Take(0),
        move |_endpoint| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(CursorController::new("unused"))
        },
        |_endpoint, _controller| Ok(()),
        |_controller| None,
    )
    .await
    .expect("zero-page success");
    assert!(zero.is_empty());
    assert_eq!(zero_factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        zero_executor.application().request_count().expect("count"),
        0
    );
    assert!(zero_hooks.before.lock().expect("before lock").is_empty());
    assert!(
        zero_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
    let zero_aggregate = zero_hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(zero_aggregate.len(), 1);
    assert_eq!(zero_aggregate[0].pages_completed, 0);
    assert_eq!(zero_aggregate[0].items_collected, 0);
    assert!(!zero_aggregate[0].recovered_authentication);
    assert_eq!(zero_aggregate[0].final_status, None);
    assert_eq!(zero_aggregate[0].outcome, CompletionOutcome::Success);

    let page_executor = DeterministicExecutor::new();
    let page_hooks = Arc::new(PaginationProbe::default());
    let page_limiter = Arc::new(LimiterProbe::default());
    let page_core = core(
        &page_executor,
        Arc::clone(&page_hooks),
        Arc::clone(&page_limiter),
        |runtime| runtime.pagination_max_pages(NonZeroU32::MIN),
    );
    let page_factory_calls = Arc::new(AtomicUsize::new(0));
    let calls = Arc::clone(&page_factory_calls);
    let error = pagination_error(
        paginate_with(
            Request::new(page_core, PageEndpoint::get(Some(Duration::from_secs(30)))),
            PageLimit::Take(2),
            move |_endpoint| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(CursorController::new("unused"))
            },
            |_endpoint, _controller| Ok(()),
            |_controller| None,
        )
        .await,
    );
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::PageCeilingExceeded)
    );
    assert_eq!(page_factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        page_executor.application().request_count().expect("count"),
        0
    );
    assert!(page_hooks.before.lock().expect("before lock").is_empty());
    assert!(
        page_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
    let page_aggregate = page_hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(page_aggregate.len(), 1);
    assert_eq!(page_aggregate[0].pages_completed, 0);
    assert_eq!(page_aggregate[0].items_collected, 0);
    assert_eq!(page_aggregate[0].final_status, None);
    assert_eq!(page_aggregate[0].outcome, CompletionOutcome::Pagination);

    let maximum_executor = DeterministicExecutor::new();
    maximum_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"first|second",
        )))
        .expect("first maximum page");
    maximum_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"")))
        .expect("natural maximum page");
    let maximum_hooks = Arc::new(PaginationProbe::default());
    let maximum_limiter = Arc::new(LimiterProbe::default());
    let maximum_core = core(
        &maximum_executor,
        Arc::clone(&maximum_hooks),
        maximum_limiter,
        |runtime| runtime.pagination_max_pages(NonZeroU32::new(2).expect("positive page ceiling")),
    );
    let maximum = collect(
        maximum_core,
        PageLimit::HardCap(NonZeroU32::new(2).expect("positive hard cap")),
        CursorController::new("first"),
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("configured maximum naturally terminates");
    assert_eq!(maximum, ["first"]);
    assert_eq!(
        maximum_executor
            .application()
            .request_count()
            .expect("count"),
        2
    );
    let maximum_aggregate = maximum_hooks
        .aggregate
        .lock()
        .expect("aggregate lock")
        .clone();
    assert_eq!(maximum_aggregate.len(), 1);
    assert_eq!(maximum_aggregate[0].pages_completed, 2);
    assert_eq!(maximum_aggregate[0].items_collected, 1);
    assert_eq!(maximum_aggregate[0].outcome, CompletionOutcome::Success);

    let identity_executor = DeterministicExecutor::new();
    let identity_hooks = Arc::new(PaginationProbe::default());
    let identity_limiter = Arc::new(LimiterProbe::default());
    let identity_core = core(
        &identity_executor,
        Arc::clone(&identity_hooks),
        Arc::clone(&identity_limiter),
        |runtime| runtime.pagination_identity_limit(NonZeroUsize::MIN),
    );
    let identity_error = pagination_error(
        collect(
            identity_core,
            PageLimit::Take(1),
            CursorController::new("identity-first-excess-sentinel-6f11"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(
        identity_error.pagination_kind(),
        Some(PaginationErrorKind::IdentityStorageCeilingExceeded)
    );
    assert_eq!(
        identity_executor
            .application()
            .request_count()
            .expect("count"),
        0
    );
    assert!(
        identity_hooks
            .before
            .lock()
            .expect("before lock")
            .is_empty()
    );
    assert!(
        identity_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );

    let item_executor = DeterministicExecutor::new();
    item_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"accepted-item-sentinel-62a3,first-excess-item-sentinel-029c|-",
        )))
        .expect("item page");
    let item_hooks = Arc::new(PaginationProbe::default());
    let item_limiter = Arc::new(LimiterProbe::default());
    let item_core = core(
        &item_executor,
        Arc::clone(&item_hooks),
        item_limiter,
        |runtime| runtime.pagination_max_items(NonZeroUsize::MIN),
    );
    let item_error = pagination_error(
        collect(
            item_core,
            PageLimit::Take(1),
            CursorController::new("item-page"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(
        item_error.pagination_kind(),
        Some(PaginationErrorKind::CollectedItemCeilingExceeded)
    );
    assert_eq!(
        item_executor.application().request_count().expect("count"),
        1
    );
    let item_aggregate = item_hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(item_aggregate.len(), 1);
    assert_eq!(item_aggregate[0].items_collected, 1);
    assert_eq!(item_aggregate[0].outcome, CompletionOutcome::Pagination);
    assert_eq!(item_aggregate[0].final_status, Some(StatusCode::OK));
}

#[tokio::test]
async fn controller_construction_and_apply_failures_are_pagination_only_and_page_side_effect_free()
{
    let factory_executor = DeterministicExecutor::new();
    let factory_hooks = Arc::new(PaginationProbe::default());
    let factory_limiter = Arc::new(LimiterProbe::default());
    let factory_core = core(
        &factory_executor,
        Arc::clone(&factory_hooks),
        Arc::clone(&factory_limiter),
        |runtime| runtime,
    );
    let factory_error = pagination_error(
        paginate_with(
            Request::new(
                factory_core,
                PageEndpoint::get(Some(Duration::from_secs(30))),
            ),
            PageLimit::Take(1),
            |_endpoint| -> Result<CursorController, PaginationError> {
                Err(PaginationError::new(PaginationErrorKind::Controller))
            },
            |_endpoint, _controller| Ok(()),
            |_controller| None,
        )
        .await,
    );
    assert_eq!(factory_error.category(), ErrorCategory::Pagination);
    assert_eq!(
        factory_error.pagination_kind(),
        Some(PaginationErrorKind::Controller)
    );
    assert_eq!(
        factory_executor
            .application()
            .request_count()
            .expect("count"),
        0
    );
    assert!(factory_hooks.before.lock().expect("before lock").is_empty());
    assert!(
        factory_hooks
            .completion
            .lock()
            .expect("completion lock")
            .is_empty()
    );
    assert_eq!(
        factory_hooks.aggregate.lock().expect("aggregate lock")[0].outcome,
        CompletionOutcome::Pagination
    );
    assert!(
        factory_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );

    let apply_executor = DeterministicExecutor::new();
    let apply_hooks = Arc::new(PaginationProbe::default());
    let apply_limiter = Arc::new(LimiterProbe::default());
    let apply_core = core(
        &apply_executor,
        Arc::clone(&apply_hooks),
        Arc::clone(&apply_limiter),
        |runtime| runtime,
    );
    let controller = CursorController::failing_apply("private-controller-state-sentinel-287e");
    let apply_calls = Arc::clone(&controller.apply_calls);
    let apply_error = pagination_error(
        collect(
            apply_core,
            PageLimit::Take(1),
            controller,
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(apply_calls.load(Ordering::SeqCst), 1);
    assert_eq!(apply_error.category(), ErrorCategory::Pagination);
    assert_eq!(
        apply_error.pagination_kind(),
        Some(PaginationErrorKind::Controller)
    );
    let rendered = format!("{apply_error}\n{apply_error:?}");
    assert!(!rendered.contains("private-controller-state-sentinel-287e"));
    assert_eq!(
        apply_executor.application().request_count().expect("count"),
        0
    );
    assert!(apply_hooks.before.lock().expect("before lock").is_empty());
    assert!(
        apply_hooks
            .completion
            .lock()
            .expect("completion lock")
            .is_empty()
    );
    assert_eq!(
        apply_hooks.aggregate.lock().expect("aggregate lock")[0].outcome,
        CompletionOutcome::Pagination
    );
    assert!(
        apply_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
}

#[tokio::test]
async fn final_soft_and_empty_pages_apply_the_exact_next_precedence() {
    let soft_executor = DeterministicExecutor::new();
    soft_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"item|unused-next",
        )))
        .expect("soft page");
    let soft_hooks = Arc::new(PaginationProbe::default());
    let soft_core = core(
        &soft_executor,
        soft_hooks,
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let controller = CursorController::new("soft");
    let next_calls = Arc::clone(&controller.next_calls);
    let items = collect(
        soft_core,
        PageLimit::Take(1),
        controller,
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("final soft page");
    assert_eq!(items, ["item"]);
    assert_eq!(next_calls.load(Ordering::SeqCst), 0);

    let empty_executor = DeterministicExecutor::new();
    empty_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"|unused-next",
        )))
        .expect("empty page");
    let empty_core = core(
        &empty_executor,
        Arc::new(PaginationProbe::default()),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let controller = CursorController::failing_next("empty");
    let next_calls = Arc::clone(&controller.next_calls);
    let items = collect(
        empty_core,
        PageLimit::HardCap(NonZeroU32::MIN),
        controller,
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("empty page overrides speculative controller error");
    assert!(items.is_empty());
    assert_eq!(next_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn immediate_two_state_and_long_cycles_stop_before_protected_page_work() {
    let cases: &[(&str, &[&str], &[&str])] = &[
        (
            "immediate-cycle-sentinel-8251",
            &["one|immediate-cycle-sentinel-8251"],
            &["immediate-cycle-sentinel-8251"],
        ),
        (
            "two-a-cycle-sentinel-e3d1",
            &[
                "one|two-b-cycle-sentinel-a044",
                "two|two-a-cycle-sentinel-e3d1",
            ],
            &["two-a-cycle-sentinel-e3d1", "two-b-cycle-sentinel-a044"],
        ),
        (
            "long-a-cycle-sentinel-a31b",
            &[
                "one|long-b-cycle-sentinel-bf11",
                "two|long-c-cycle-sentinel-48ce",
                "three|long-a-cycle-sentinel-a31b",
            ],
            &[
                "long-a-cycle-sentinel-a31b",
                "long-b-cycle-sentinel-bf11",
                "long-c-cycle-sentinel-48ce",
            ],
        ),
    ];

    for (initial, pages, forbidden) in cases {
        let executor = DeterministicExecutor::new();
        for page in *pages {
            executor
                .application()
                .queue_response(ScriptedResponse::success(Bytes::copy_from_slice(
                    page.as_bytes(),
                )))
                .expect("cycle page");
        }
        let hooks = Arc::new(PaginationProbe::default());
        let limiter = Arc::new(LimiterProbe::default());
        let core = core(
            &executor,
            Arc::clone(&hooks),
            Arc::clone(&limiter),
            |runtime| runtime,
        );
        let error = pagination_error(
            collect(
                core,
                PageLimit::Take(10),
                CursorController::new(*initial),
                Some(Duration::from_secs(30)),
            )
            .await,
        );
        assert_eq!(
            error.pagination_kind(),
            Some(PaginationErrorKind::RepeatedRequest)
        );
        assert_eq!(
            executor.application().request_count().expect("count"),
            pages.len()
        );
        assert_eq!(hooks.before.lock().expect("before lock").len(), pages.len());
        assert_eq!(
            limiter.acquisitions.lock().expect("acquisition lock").len(),
            pages.len()
        );
        let aggregate = hooks.aggregate.lock().expect("aggregate lock").clone();
        assert_eq!(aggregate.len(), 1);
        assert_eq!(aggregate[0].pages_completed as usize, pages.len());
        assert_eq!(aggregate[0].items_collected, pages.len());
        assert_eq!(aggregate[0].outcome, CompletionOutcome::Pagination);

        let mut rendered = vec![error.to_string(), format!("{error:?}")];
        let mut source = error.source();
        while let Some(error) = source {
            rendered.push(error.to_string());
            rendered.push(format!("{error:?}"));
            source = error.source();
        }
        let rendered = rendered.join("\n");
        for sentinel in *forbidden {
            assert!(!rendered.contains(sentinel));
        }
    }
}

#[tokio::test]
async fn later_page_errors_keep_ordinary_categories_and_aggregate_status() {
    let status_executor = DeterministicExecutor::new();
    status_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|next")))
        .expect("first page");
    status_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::TOO_MANY_REQUESTS,
            Bytes::from_static(b"status-body-sentinel"),
        ))
        .expect("status page");
    let status_hooks = Arc::new(PaginationProbe::default());
    let status_core = core(
        &status_executor,
        Arc::clone(&status_hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let error = pagination_error(
        collect(
            status_core,
            PageLimit::Take(5),
            CursorController::new("first"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.pagination_kind(), None);
    let aggregate = status_hooks
        .aggregate
        .lock()
        .expect("aggregate lock")
        .clone();
    assert_eq!(aggregate[0].pages_completed, 1);
    assert_eq!(aggregate[0].items_collected, 1);
    assert_eq!(
        aggregate[0].final_status,
        Some(StatusCode::TOO_MANY_REQUESTS)
    );
    assert_eq!(aggregate[0].outcome, CompletionOutcome::HttpStatus);

    let decode_executor = DeterministicExecutor::new();
    decode_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|next")))
        .expect("first page");
    decode_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"decoder-body-sentinel-without-structure",
        )))
        .expect("decode page");
    let decode_hooks = Arc::new(PaginationProbe::default());
    let decode_core = core(
        &decode_executor,
        Arc::clone(&decode_hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let error = pagination_error(
        collect(
            decode_core,
            PageLimit::Take(5),
            CursorController::new("first"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(error.pagination_kind(), None);
    let rendered = format!("{error}\n{error:?}");
    assert!(!rendered.contains("decoder-body-sentinel"));
    let aggregate = decode_hooks
        .aggregate
        .lock()
        .expect("aggregate lock")
        .clone();
    assert_eq!(aggregate[0].final_status, Some(StatusCode::OK));
    assert_eq!(aggregate[0].outcome, CompletionOutcome::ResponseDecoding);

    let timeout_executor = DeterministicExecutor::new();
    timeout_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|next")))
        .expect("first page");
    timeout_executor
        .application()
        .queue_failure(TransportFailure::Timeout)
        .expect("timeout page");
    let timeout_hooks = Arc::new(PaginationProbe::default());
    let timeout_core = core(
        &timeout_executor,
        Arc::clone(&timeout_hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let error = pagination_error(
        collect(
            timeout_core,
            PageLimit::Take(5),
            CursorController::new("first"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.pagination_kind(), None);
    let aggregate = timeout_hooks
        .aggregate
        .lock()
        .expect("aggregate lock")
        .clone();
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Timeout);

    let rate_executor = DeterministicExecutor::new();
    let rate_hooks = Arc::new(PaginationProbe::default());
    let rejecting_limiter = Arc::new(RejectingLimiter::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = rate_hooks.clone();
    let configured_limiter: Arc<dyn RateLimiter> = rejecting_limiter.clone();
    let rate_core = ClientCore::build_with_development(
        "https://pagination-runtime.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .hooks(configured_hooks)
            .rate_limiter(configured_limiter),
        &rate_executor,
    )
    .expect("rate-limit pagination core");
    let error = pagination_error(
        collect(
            rate_core,
            PageLimit::Take(1),
            CursorController::new("rate-limit-page"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(error.rate_limit_kind(), Some(RateLimitErrorKind::Acquire));
    assert_eq!(error.pagination_kind(), None);
    assert_eq!(rejecting_limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        rate_executor.application().request_count().expect("count"),
        0
    );
    let completion = rate_hooks
        .completion
        .lock()
        .expect("completion lock")
        .clone();
    assert_eq!(completion.len(), 1);
    assert_eq!(completion[0].outcome, CompletionOutcome::RateLimit);
    let aggregate = rate_hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 0);
    assert_eq!(aggregate[0].items_collected, 0);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::RateLimit);
}

#[tokio::test(start_paused = true)]
async fn page_completion_delay_is_outside_deadline_and_later_page_gets_a_fresh_deadline() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|next")))
        .expect("first page");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"two|-")))
        .expect("second page");
    let completion_gate = Gate::closed();
    let hooks = Arc::new(PaginationProbe::with_page_zero_completion_gate(
        completion_gate.clone(),
    ));
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let operation = tokio::spawn(collect(
        core,
        PageLimit::Take(5),
        CursorController::new("first"),
        Some(Duration::from_secs(1)),
    ));
    completion_gate.wait_until_reached().await;
    assert_eq!(executor.application().request_count().expect("count"), 1);

    tokio::time::advance(Duration::from_secs(100)).await;
    completion_gate.open();
    let items = operation
        .await
        .expect("pagination task")
        .expect("later page uses a fresh deadline");
    assert_eq!(items, ["one", "two"]);
    assert_eq!(executor.application().request_count().expect("count"), 2);
    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 2);
}

#[tokio::test(start_paused = true)]
async fn aggregate_completion_observes_a_fixed_result_and_excludes_its_hook_delay() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|-")))
        .expect("page");
    let aggregate_gate = Gate::closed();
    let hooks = Arc::new(PaginationProbe::with_aggregate_gate(aggregate_gate.clone()));
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let operation = tokio::spawn(collect(
        core,
        PageLimit::Take(1),
        CursorController::new("first"),
        Some(Duration::from_secs(1)),
    ));
    aggregate_gate.wait_until_reached().await;
    assert!(!operation.is_finished(), "the aggregate hook is awaited");
    let fixed_elapsed = hooks.aggregate.lock().expect("aggregate lock")[0].elapsed;

    tokio::time::advance(Duration::from_secs(100)).await;
    aggregate_gate.open();
    let items = operation
        .await
        .expect("pagination task")
        .expect("fixed success survives aggregate hook delay");
    assert_eq!(items, ["one"]);
    assert!(fixed_elapsed < Duration::from_secs(100));
}
