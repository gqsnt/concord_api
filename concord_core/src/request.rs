use crate::advanced::DecodedResponse;
use crate::client::{ApiClient, ClientContext};
use crate::debug::DebugLevel;
use crate::endpoint::{
    Endpoint, IntoEndpointPlan, PaginatedEndpoint, ResponseTerminalEndpoint, ReusableEndpoint,
};
use crate::error::{ApiClientError, ErrorContext, PaginationErrorKind};
use crate::multipart_response::MultipartStream;
use crate::pagination::{
    Control, PageAdvance, PageApply, PageItems, PaginationCaps, PaginationRuntime,
    PaginationTermination, ProgressKey,
};
use crate::record::RecordStream;
use crate::sse::SseStream;
use crate::stream_response::StreamResponse;
use crate::timeout::TimeoutOverride;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::future::{Future, IntoFuture};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::time::Duration;

/// Options runtime partagées entre requête simple et pagination.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RequestOptions {
    pub debug_level: Option<DebugLevel>,
    pub timeout_override: TimeoutOverride,
    pub attempt: u32,
}
impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            debug_level: None,
            timeout_override: TimeoutOverride::Inherit,
            attempt: 0,
        }
    }
}

impl RequestOptions {
    fn apply_to(&self, plan: &mut crate::endpoint::RequestPlan, page_index: u32) {
        plan.overrides.timeout = match self.timeout_override {
            TimeoutOverride::Inherit => plan.overrides.timeout,
            TimeoutOverride::Clear => None,
            TimeoutOverride::Set(d) => Some(d),
        };
        plan.overrides.debug_level = self.debug_level;
        plan.overrides.attempt = self.attempt;
        plan.overrides.page_index = page_index;
    }
}

pub struct PendingRequest<
    'a,
    Cx: ClientContext,
    E: IntoEndpointPlan<Cx>,
    T: crate::transport::Transport,
> {
    client: &'a ApiClient<Cx, T>,
    ep: E,
    opts: RequestOptions,
}

impl<'a, Cx: ClientContext, E: IntoEndpointPlan<Cx>, T: crate::transport::Transport>
    PendingRequest<'a, Cx, E, T>
{
    #[inline]
    pub(crate) fn new(client: &'a ApiClient<Cx, T>, ep: E) -> Self {
        Self {
            client,
            ep,
            opts: RequestOptions::default(),
        }
    }

    #[inline]
    pub fn map_endpoint(mut self, f: impl FnOnce(E) -> E) -> Self {
        self.ep = f(self.ep);
        self
    }

    #[inline]
    pub fn debug_level(mut self, level: DebugLevel) -> Self {
        self.opts.debug_level = Some(level);
        self
    }

    #[inline]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.opts.timeout_override = TimeoutOverride::Set(d);
        self
    }

    #[inline]
    pub fn clear_timeout(mut self) -> Self {
        self.opts.timeout_override = TimeoutOverride::Clear;
        self
    }

    #[inline]
    pub fn inherit_timeout(mut self) -> Self {
        self.opts.timeout_override = TimeoutOverride::Inherit;
        self
    }

    #[inline]
    pub fn attempt(mut self, v: u32) -> Self {
        self.opts.attempt = v;
        self
    }

    #[inline]
    pub async fn execute(self) -> Result<E::Response, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute(client, plan).await
    }

    pub async fn execute_and_store_manual<F>(self, slot: F) -> Result<(), ApiClientError>
    where
        E::Response: crate::auth::CredentialMaterial,
        F: FnOnce(
            &Cx::AuthState,
        ) -> &crate::auth::CredentialSlot<
            Cx,
            crate::auth::ManualCredentialProvider<E::Response>,
        >,
    {
        let client = self.client;
        let plan = self.request_plan()?;
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let value = E::execute(client, plan).await?;
        let auth_state = client
            .try_auth_state()
            .map_err(|source| ApiClientError::Auth {
                ctx: ctx.clone(),
                source,
            })?;
        slot(auth_state.as_ref())
            .set_manual(value)
            .await
            .map_err(|source| ApiClientError::Auth { ctx, source })?;
        Ok(())
    }

    /// Return the decoded endpoint value together with response metadata.
    #[inline]
    pub async fn response(self) -> Result<DecodedResponse<E::Response>, ApiClientError>
    where
        E: ResponseTerminalEndpoint<Cx>,
    {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute_response(client, plan).await
    }

    #[cfg(feature = "dangerous-raw-response")]
    pub async fn execute_raw_response(
        self,
    ) -> Result<crate::dangerous::BuiltResponse, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        client.execute_plan_raw(plan).await
    }

    fn request_plan(self) -> Result<crate::endpoint::RequestPlan, ApiClientError> {
        let Self { client, ep, opts } = self;
        let plan_ctx = client.plan_context();
        let mut plan = ep.into_plan(&plan_ctx)?;
        opts.apply_to(&mut plan, 0);
        Ok(plan)
    }

    #[inline]
    pub fn paginate(self, termination: PaginationTermination) -> PaginatedRequest<'a, Cx, E, T>
    where
        E: PaginatedEndpoint<Cx>,
        E::Response: PageItems,
    {
        PaginatedRequest::new(self, termination)
    }
}

impl<'a, Cx, E, T, M> PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: IntoEndpointPlan<Cx> + Endpoint<Cx, Response = StreamResponse<M>> + 'a,
    T: crate::transport::Transport + 'a,
    M: 'a,
{
    #[inline]
    pub async fn execute_stream(self) -> Result<StreamResponse<M>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute(client, plan).await
    }
}

impl<'a, Cx, E, T, Item> PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: IntoEndpointPlan<Cx> + Endpoint<Cx, Response = RecordStream<Item>> + 'a,
    T: crate::transport::Transport + 'a,
    Item: 'a,
{
    #[inline]
    pub async fn execute_records(self) -> Result<RecordStream<Item>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute(client, plan).await
    }
}

impl<'a, Cx, E, T, Part> PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: IntoEndpointPlan<Cx> + Endpoint<Cx, Response = MultipartStream<Part>> + 'a,
    T: crate::transport::Transport + 'a,
    Part: 'a,
{
    #[inline]
    pub async fn execute_multipart(self) -> Result<MultipartStream<Part>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute(client, plan).await
    }
}

impl<'a, Cx, E, T, Event> PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: IntoEndpointPlan<Cx> + Endpoint<Cx, Response = SseStream<Event>> + 'a,
    T: crate::transport::Transport + 'a,
    Event: 'a,
{
    #[inline]
    pub async fn execute_sse(self) -> Result<SseStream<Event>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute(client, plan).await
    }
}

impl<'a, Cx, E, T> IntoFuture for PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: IntoEndpointPlan<Cx> + 'a,
    T: crate::transport::Transport + 'a,
    E::Response: 'a,
{
    type Output = Result<E::Response, ApiClientError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    #[inline]
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.execute())
    }
}

pub struct PaginatedRequest<
    'a,
    Cx: ClientContext,
    E: ReusableEndpoint<Cx>,
    T: crate::transport::Transport,
> {
    pending: PendingRequest<'a, Cx, E, T>,
    caps: PaginationCaps,
}

impl<'a, Cx: ClientContext, E: ReusableEndpoint<Cx>, T: crate::transport::Transport>
    PaginatedRequest<'a, Cx, E, T>
{
    #[inline]
    pub(crate) fn new(
        pending: PendingRequest<'a, Cx, E, T>,
        termination: PaginationTermination,
    ) -> Self {
        let caps =
            PaginationCaps::new(termination).detect_loops(pending.client.pagination_detect_loops());
        Self { pending, caps }
    }

    #[inline]
    pub fn detect_loops(mut self, v: bool) -> Self {
        self.caps = self.caps.detect_loops(v);
        self
    }

    pub async fn collect(self) -> Result<Vec<<E::Response as PageItems>::Item>, ApiClientError>
    where
        E: PaginatedEndpoint<Cx>,
        E::Response: PageItems,
        T: crate::transport::Transport,
    {
        // This intentionally has a dedicated loop instead of delegating to a
        // higher-level callback surface: collection can enforce hard item caps
        // from the actual `into_items()` length, while page-level processing
        // cannot. Keep pagination ordering changes in sync with other
        // pagination execution paths.
        let caps = self.caps;
        let pending = self.pending;
        let first_plan = pending.ep.plan(&pending.client.plan_context())?;
        let ctx = crate::error::ErrorContext {
            endpoint: first_plan.endpoint.meta.name,
            method: first_plan.endpoint.meta.method.clone(),
        };
        validate_collect_termination(caps.termination, &ctx)?;
        if matches!(
            caps.termination,
            PaginationTermination::TakePages(0) | PaginationTermination::TakeItems(0)
        ) {
            return Ok(Vec::new());
        }
        if let Some(runtime) = pending.ep.pagination_runtime() {
            return collect_with_pagination_runtime(pending, runtime, caps, ctx).await;
        }
        if first_plan.endpoint.pagination.is_some() {
            return Err(ApiClientError::pagination(
                ctx,
                PaginationErrorKind::UnsupportedPagination,
                "pagination requires runtime support",
            ));
        }
        Err(ApiClientError::pagination(
            ctx,
            PaginationErrorKind::UnsupportedPagination,
            "endpoint is not paginated",
        ))
    }
}
async fn collect_with_pagination_runtime<'a, Cx, E, T>(
    mut pending: PendingRequest<'a, Cx, E, T>,
    mut runtime: Box<dyn PaginationRuntime<E, E::Response>>,
    caps: PaginationCaps,
    ctx: ErrorContext,
) -> Result<Vec<<E::Response as PageItems>::Item>, ApiClientError>
where
    Cx: ClientContext + 'a,
    E: PaginatedEndpoint<Cx> + 'a,
    T: crate::transport::Transport + 'a,
    E::Response: PageItems,
{
    let page_apply_ctx = PageApply {
        endpoint: ctx.endpoint,
        page_index: 0,
        ctx: &ctx,
    };
    let mut out: Vec<<E::Response as PageItems>::Item> = Vec::new();
    let mut seen: Option<HashSet<ProgressKey>> = if caps.detect_loops {
        Some(HashSet::new())
    } else {
        None
    };
    let mut progress_state = PaginationRunState::default();
    let mut items_count: usize = 0;
    let mut page_index: u32 = 0;

    runtime.init(&pending.ep, page_apply_ctx)?;

    loop {
        if let Some(seen) = seen.as_mut()
            && let Some(k) = runtime.progress_key()
            && !seen.insert(k.clone())
        {
            return Err(ApiClientError::pagination(
                ctx.clone(),
                PaginationErrorKind::NonProgress,
                format!(
                    "loop detected (page_index={} {})",
                    page_index,
                    k.diagnostic_summary()
                ),
            ));
        }

        runtime.apply(
            &mut pending.ep,
            PageApply {
                endpoint: ctx.endpoint,
                page_index: page_index as u64,
                ctx: &ctx,
            },
        )?;
        let expected_items = runtime.expected_items_per_page();
        let mut plan = pending.ep.plan(&pending.client.plan_context())?;
        pending.opts.apply_to(&mut plan, page_index);
        let request_identity = pagination_request_identity(&plan);
        progress_state.ensure_progress(request_identity, &ctx, page_index)?;
        let page = E::execute(pending.client, plan).await?;
        let page_len = page.item_count();
        let pre_advance = pre_advance_decision(
            caps.termination,
            items_count,
            page_len,
            expected_items,
            &ctx,
        )?;
        if let (PaginationTermination::HardItemCap(max_items), Some(new_total)) =
            (caps.termination, pre_advance.hard_item_cap_exceeded)
        {
            return Err(hard_item_cap_error(&ctx, max_items, new_total, page_index));
        }
        let control_ctrl = if pre_advance.common_stop || pre_advance.take_items_done {
            Control::Stop
        } else {
            runtime
                .advance(
                    &mut pending.ep,
                    &ctx,
                    &page,
                    PageAdvance {
                        endpoint: ctx.endpoint,
                        page_index: page_index as u64,
                        item_count: page_len,
                    },
                )?
                .into()
        };
        let items = <E::Response as PageItems>::into_items(page);
        if page_len == 0 {
            return Ok(out);
        }
        let common_stop = common_content_stop(page_len, expected_items);
        match caps.termination {
            PaginationTermination::HardItemCap(max_items) => {
                let new_total = items_count.checked_add(page_len).ok_or_else(|| {
                    ApiClientError::pagination(
                        ctx.clone(),
                        PaginationErrorKind::Overflow,
                        "items overflow",
                    )
                })?;
                if new_total > max_items {
                    return Err(hard_item_cap_error(&ctx, max_items, new_total, page_index));
                }
                items_count = new_total;
                out.extend(items);
            }
            PaginationTermination::TakeItems(max_items) => {
                let remaining = max_items.checked_sub(items_count).ok_or_else(|| {
                    ApiClientError::pagination(
                        ctx.clone(),
                        PaginationErrorKind::Overflow,
                        "items overflow",
                    )
                })?;
                if page_len >= remaining {
                    out.extend(items.into_iter().take(remaining));
                    return Ok(out);
                }
                items_count = items_count.checked_add(page_len).ok_or_else(|| {
                    ApiClientError::pagination(
                        ctx.clone(),
                        PaginationErrorKind::Overflow,
                        "items overflow",
                    )
                })?;
                out.extend(items);
            }
            _ => {
                items_count = items_count.checked_add(page_len).ok_or_else(|| {
                    ApiClientError::pagination(
                        ctx.clone(),
                        PaginationErrorKind::Overflow,
                        "items overflow",
                    )
                })?;
                out.extend(items);
            }
        }
        if common_stop {
            return Ok(out);
        }
        let fetched_pages = page_index as usize + 1;
        match control_ctrl {
            Control::Continue => match caps.termination {
                PaginationTermination::HardPageCap(max_pages) if fetched_pages >= max_pages => {
                    return Err(ApiClientError::pagination_limit(
                        ctx,
                        PaginationErrorKind::PageLimitExceeded,
                        format!(
                            "pagination hard page cap exceeded (max={} seen_items={} page_index={})",
                            max_pages, items_count, fetched_pages
                        ),
                    ));
                }
                PaginationTermination::TakePages(max_pages) if fetched_pages >= max_pages => {
                    return Ok(out);
                }
                _ => {
                    page_index = page_index.checked_add(1).ok_or_else(|| {
                        ApiClientError::pagination(
                            ctx.clone(),
                            PaginationErrorKind::Overflow,
                            "page index overflow",
                        )
                    })?;
                }
            },
            Control::Stop => return Ok(out),
        }
    }
}

fn validate_collect_termination(
    termination: PaginationTermination,
    ctx: &crate::error::ErrorContext,
) -> Result<(), ApiClientError> {
    match termination {
        PaginationTermination::HardPageCap(0) => Err(ApiClientError::pagination(
            ctx.clone(),
            PaginationErrorKind::InvalidSize,
            "hard pagination page cap must be greater than zero",
        )),
        PaginationTermination::HardItemCap(0) => Err(ApiClientError::pagination(
            ctx.clone(),
            PaginationErrorKind::InvalidSize,
            "hard pagination item cap must be greater than zero",
        )),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreAdvanceDecision {
    common_stop: bool,
    take_items_done: bool,
    hard_item_cap_exceeded: Option<usize>,
}

fn pre_advance_decision(
    termination: PaginationTermination,
    items_count: usize,
    item_count: usize,
    expected_items: Option<NonZeroUsize>,
    ctx: &crate::error::ErrorContext,
) -> Result<PreAdvanceDecision, ApiClientError> {
    let hinted_total = items_count.checked_add(item_count).ok_or_else(|| {
        ApiClientError::pagination(ctx.clone(), PaginationErrorKind::Overflow, "items overflow")
    })?;
    let hard_item_cap_exceeded = match termination {
        PaginationTermination::HardItemCap(max_items) if hinted_total > max_items => {
            Some(hinted_total)
        }
        _ => None,
    };
    let take_items_done = match termination {
        PaginationTermination::TakeItems(max_items) => {
            item_count >= max_items.saturating_sub(items_count)
        }
        _ => false,
    };
    Ok(PreAdvanceDecision {
        common_stop: common_content_stop(item_count, expected_items),
        take_items_done,
        hard_item_cap_exceeded,
    })
}

fn hard_item_cap_error(
    ctx: &crate::error::ErrorContext,
    max_items: usize,
    seen_items: usize,
    page_index: u32,
) -> ApiClientError {
    ApiClientError::pagination_limit(
        ctx.clone(),
        PaginationErrorKind::ItemLimitExceeded,
        format!(
            "pagination hard item cap exceeded (max={} seen={} page_index={})",
            max_items, seen_items, page_index
        ),
    )
}

fn common_content_stop(item_count: usize, expected_items: Option<NonZeroUsize>) -> bool {
    item_count == 0 || expected_items.is_some_and(|expected| item_count < expected.get())
}

#[derive(Default)]
struct PaginationRunState {
    seen_request_identities: HashSet<PaginationRequestIdentity>,
}

impl PaginationRunState {
    fn ensure_progress(
        &mut self,
        current_identity: PaginationRequestIdentity,
        ctx: &crate::error::ErrorContext,
        page_index: u32,
    ) -> Result<(), ApiClientError> {
        if !self.seen_request_identities.insert(current_identity) {
            return Err(ApiClientError::pagination(
                ctx.clone(),
                PaginationErrorKind::NonProgress,
                format!(
                    "non-progress detected (page_index={} request repeated)",
                    page_index
                ),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct PaginationRequestIdentity {
    endpoint: &'static str,
    method: http::Method,
    scheme: http::uri::Scheme,
    host: String,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<PaginationRequestHeader>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct PaginationRequestHeader {
    name: String,
    value: Vec<u8>,
}

fn pagination_request_identity(plan: &crate::endpoint::RequestPlan) -> PaginationRequestIdentity {
    let mut headers: Vec<_> = plan
        .endpoint
        .policy
        .headers
        .iter()
        .map(|(name, value)| PaginationRequestHeader {
            name: name.as_str().to_string(),
            value: value.as_bytes().to_vec(),
        })
        .collect();
    headers.sort_unstable_by(|a, b| {
        let name_order = a.name.cmp(&b.name);
        if name_order == Ordering::Equal {
            a.value.cmp(&b.value)
        } else {
            name_order
        }
    });
    PaginationRequestIdentity {
        endpoint: plan.endpoint.meta.name,
        method: plan.endpoint.meta.method.clone(),
        scheme: plan.endpoint.route.scheme.clone(),
        host: plan.endpoint.route.host.clone(),
        path: plan.endpoint.route.path.clone(),
        query: plan.endpoint.policy.query.clone(),
        headers,
    }
}
