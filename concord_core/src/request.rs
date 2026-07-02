use crate::client::{ApiClient, ClientContext};
use crate::debug::DebugLevel;
use crate::endpoint::{
    Endpoint, MultipartResponseEndpoint, PaginatedEndpoint, RecordResponseEndpoint,
    SseResponseEndpoint, StreamResponseEndpoint,
};
use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    Control, EndpointPaginationRuntime, PageAdvance, PageApply, PageItems, PaginationCaps,
    PaginationTermination, ProgressKey,
};
use crate::timeout::TimeoutOverride;
use crate::transport::{BuiltResponse, DecodedResponse};
use base64::Engine;
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

pub struct PendingRequest<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport> {
    client: &'a ApiClient<Cx, T>,
    ep: E,
    opts: RequestOptions,
}

impl<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport>
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
        let value = client.execute_plan::<E::Response>(plan).await?.value;
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

    pub async fn execute_decoded(self) -> Result<DecodedResponse<E::Response>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        client.execute_plan::<E::Response>(plan).await
    }

    pub async fn execute_raw(self) -> Result<BuiltResponse, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        client.execute_plan_raw(plan).await
    }

    fn request_plan(self) -> Result<crate::endpoint::RequestPlan, ApiClientError> {
        let mut plan = self.ep.plan(&self.client.plan_context())?;
        plan.overrides.timeout = match self.opts.timeout_override {
            TimeoutOverride::Inherit => plan.overrides.timeout,
            TimeoutOverride::Clear => None,
            TimeoutOverride::Set(d) => Some(d),
        };
        plan.overrides.debug_level = self.opts.debug_level;
        plan.overrides.attempt = self.opts.attempt;
        plan.overrides.page_index = 0;
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

    #[inline]
    pub fn pages(self, termination: PaginationTermination) -> PaginatedRequest<'a, Cx, E, T>
    where
        E: PaginatedEndpoint<Cx>,
        E::Response: PageItems,
    {
        self.paginate(termination)
    }
}

impl<'a, Cx, E, T> PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: StreamResponseEndpoint<Cx> + 'a,
    T: crate::transport::Transport + 'a,
{
    #[inline]
    pub async fn execute_stream(
        self,
    ) -> Result<crate::stream_response::StreamResponse<E::Media>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute_stream(client, plan).await
    }
}

impl<'a, Cx, E, T> PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: RecordResponseEndpoint<Cx> + 'a,
    T: crate::transport::Transport + 'a,
{
    #[inline]
    pub async fn execute_records(
        self,
    ) -> Result<crate::record::RecordStream<E::Item>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute_records(client, plan).await
    }
}

impl<'a, Cx, E, T> PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: MultipartResponseEndpoint<Cx> + 'a,
    T: crate::transport::Transport + 'a,
{
    #[inline]
    pub async fn execute_multipart(
        self,
    ) -> Result<crate::multipart_response::MultipartStream<E::Part>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute_multipart(client, plan).await
    }
}

impl<'a, Cx, E, T> PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: SseResponseEndpoint<Cx> + 'a,
    T: crate::transport::Transport + 'a,
{
    #[inline]
    pub async fn execute_sse(self) -> Result<crate::sse::SseStream<E::Event>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute_sse(client, plan).await
    }
}

impl<'a, Cx, E, T> IntoFuture for PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext + 'a,
    E: Endpoint<Cx> + 'a,
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

pub struct PaginatedRequest<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport>
{
    pending: PendingRequest<'a, Cx, E, T>,
    caps: PaginationCaps,
}

impl<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport>
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
        if let Some(runtime) = pending.ep.endpoint_state_pagination() {
            return collect_with_endpoint_state_pagination(pending, runtime, caps, ctx).await;
        }
        if first_plan.endpoint.pagination.is_some() {
            return Err(ApiClientError::Pagination {
                ctx,
                msg: "pagination requires endpoint-state runtime support".into(),
            });
        }
        Err(ApiClientError::Pagination {
            ctx,
            msg: "endpoint is not paginated".into(),
        })
    }
}
async fn collect_with_endpoint_state_pagination<'a, Cx, E, T>(
    mut pending: PendingRequest<'a, Cx, E, T>,
    mut runtime: Box<dyn EndpointPaginationRuntime<E, E::Response>>,
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
            return Err(ApiClientError::Pagination {
                ctx: ctx.clone(),
                msg: format!("loop detected (page_index={} key={:?})", page_index, k).into(),
            });
        }

        let expected_items = runtime.apply(
            &mut pending.ep,
            PageApply {
                endpoint: ctx.endpoint,
                page_index: page_index as u64,
                ctx: &ctx,
            },
        )?;
        let mut plan = pending.ep.plan(&pending.client.plan_context())?;
        plan.overrides.timeout = match pending.opts.timeout_override {
            TimeoutOverride::Inherit => plan.overrides.timeout,
            TimeoutOverride::Clear => None,
            TimeoutOverride::Set(d) => Some(d),
        };
        plan.overrides.debug_level = pending.opts.debug_level;
        plan.overrides.attempt = pending.opts.attempt;
        plan.overrides.page_index = page_index;
        let request_identity = pagination_request_identity(&plan);
        progress_state.ensure_progress(request_identity.clone(), &ctx, page_index)?;
        let resp: DecodedResponse<E::Response> =
            pending.client.execute_plan::<E::Response>(plan).await?;
        let page_len_hint = resp.value.item_count_hint();
        let pre_advance = pre_advance_decision(
            caps.termination,
            items_count,
            page_len_hint,
            expected_items.expected_items_per_page,
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
                    &ctx,
                    &resp.value,
                    PageAdvance {
                        endpoint: ctx.endpoint,
                        page_index: page_index as u64,
                        received_items: page_len_hint.unwrap_or(0),
                    },
                )?
                .into()
        };
        let items = <E::Response as PageItems>::into_items(resp.value);
        let page_len = items.len();
        let common_stop =
            common_content_stop(page_len_hint, expected_items.expected_items_per_page);
        match caps.termination {
            PaginationTermination::HardItemCap(max_items) => {
                let new_total = items_count.checked_add(page_len).ok_or_else(|| {
                    ApiClientError::Pagination {
                        ctx: ctx.clone(),
                        msg: "items overflow".into(),
                    }
                })?;
                if new_total > max_items {
                    return Err(hard_item_cap_error(&ctx, max_items, new_total, page_index));
                }
                items_count = new_total;
                out.extend(items);
            }
            PaginationTermination::TakeItems(max_items) => {
                let remaining = max_items.checked_sub(items_count).ok_or_else(|| {
                    ApiClientError::Pagination {
                        ctx: ctx.clone(),
                        msg: "items overflow".into(),
                    }
                })?;
                if page_len >= remaining {
                    out.extend(items.into_iter().take(remaining));
                    return Ok(out);
                }
                items_count = items_count.checked_add(page_len).ok_or_else(|| {
                    ApiClientError::Pagination {
                        ctx: ctx.clone(),
                        msg: "items overflow".into(),
                    }
                })?;
                out.extend(items);
            }
            _ => {
                items_count = items_count.checked_add(page_len).ok_or_else(|| {
                    ApiClientError::Pagination {
                        ctx: ctx.clone(),
                        msg: "items overflow".into(),
                    }
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
                    return Err(ApiClientError::PaginationLimit {
                        ctx,
                        msg: format!(
                            "pagination hard page cap exceeded (max={} seen_items={} page_index={})",
                            max_pages, items_count, fetched_pages
                        )
                        .into(),
                    });
                }
                PaginationTermination::TakePages(max_pages) if fetched_pages >= max_pages => {
                    return Ok(out);
                }
                _ => {
                    page_index =
                        page_index
                            .checked_add(1)
                            .ok_or_else(|| ApiClientError::Pagination {
                                ctx: ctx.clone(),
                                msg: "page index overflow".into(),
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
        PaginationTermination::HardPageCap(0) => Err(ApiClientError::Pagination {
            ctx: ctx.clone(),
            msg: "hard pagination page cap must be greater than zero".into(),
        }),
        PaginationTermination::HardItemCap(0) => Err(ApiClientError::Pagination {
            ctx: ctx.clone(),
            msg: "hard pagination item cap must be greater than zero".into(),
        }),
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
    actual_hint: Option<usize>,
    expected_items: Option<NonZeroUsize>,
    ctx: &crate::error::ErrorContext,
) -> Result<PreAdvanceDecision, ApiClientError> {
    let hinted_total = actual_hint
        .map(|actual| {
            items_count
                .checked_add(actual)
                .ok_or_else(|| ApiClientError::Pagination {
                    ctx: ctx.clone(),
                    msg: "items overflow".into(),
                })
        })
        .transpose()?;
    let hard_item_cap_exceeded = match (termination, hinted_total) {
        (PaginationTermination::HardItemCap(max_items), Some(total)) if total > max_items => {
            Some(total)
        }
        _ => None,
    };
    let take_items_done = match (termination, actual_hint) {
        (PaginationTermination::TakeItems(max_items), Some(actual)) => {
            actual >= max_items.saturating_sub(items_count)
        }
        _ => false,
    };
    Ok(PreAdvanceDecision {
        common_stop: common_content_stop(actual_hint, expected_items),
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
    ApiClientError::PaginationLimit {
        ctx: ctx.clone(),
        msg: format!(
            "pagination hard item cap exceeded (max={} seen={} page_index={})",
            max_items, seen_items, page_index
        )
        .into(),
    }
}

fn common_content_stop(actual_items: Option<usize>, expected_items: Option<NonZeroUsize>) -> bool {
    actual_items == Some(0)
        || matches!(
            (actual_items, expected_items),
            (Some(actual), Some(expected)) if actual < expected.get()
        )
}

#[derive(Default)]
struct PaginationRunState {
    seen_request_identities: HashSet<String>,
}

impl PaginationRunState {
    fn ensure_progress(
        &mut self,
        current_identity: String,
        ctx: &crate::error::ErrorContext,
        page_index: u32,
    ) -> Result<(), ApiClientError> {
        if !self.seen_request_identities.insert(current_identity) {
            return Err(ApiClientError::Pagination {
                ctx: ctx.clone(),
                msg: format!(
                    "non-progress detected (page_index={} request repeated)",
                    page_index
                )
                .into(),
            });
        }
        Ok(())
    }
}

fn pagination_request_identity(plan: &crate::endpoint::RequestPlan) -> String {
    let mut out = String::new();
    push_identity_component(&mut out, "endpoint", plan.endpoint.meta.name);
    push_identity_component(&mut out, "method", plan.endpoint.meta.method.as_str());
    push_identity_component(&mut out, "url", &sanitized_plan_url(plan));

    let mut headers: Vec<_> = plan
        .endpoint
        .policy
        .headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                base64::engine::general_purpose::STANDARD_NO_PAD.encode(value.as_bytes()),
            )
        })
        .collect();
    headers.sort_unstable_by(|a, b| {
        let name_order = a.0.cmp(&b.0);
        if name_order == Ordering::Equal {
            a.1.cmp(&b.1)
        } else {
            name_order
        }
    });
    for (name, value) in headers {
        push_identity_component(&mut out, "header", &name);
        push_identity_component(&mut out, "value", &value);
    }

    out
}

fn sanitized_plan_url(plan: &crate::endpoint::RequestPlan) -> String {
    let route = &plan.endpoint.route;
    let mut url = format!("{}://{}{}", route.scheme.as_str(), route.host, route.path);
    if !plan.endpoint.policy.query.is_empty() {
        url.push('?');
        for (idx, (key, value)) in plan.endpoint.policy.query.iter().enumerate() {
            if idx > 0 {
                url.push('&');
            }
            url.push_str(urlencoding::encode(key).as_ref());
            url.push('=');
            url.push_str(urlencoding::encode(value).as_ref());
        }
    }
    url
}

fn push_identity_component(out: &mut String, label: &str, value: &str) {
    out.push_str(label);
    out.push(':');
    out.push_str(&value.len().to_string());
    out.push(':');
    out.push_str(value);
    out.push('|');
}
