use crate::cache::CacheRequestMode;
use crate::client::{ApiClient, ClientContext};
use crate::debug::DebugLevel;
use crate::endpoint::{Endpoint, PaginationPlan};
use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{Caps, Control, HasNextCursor, PageItems, ProgressKey, Stop};
use crate::timeout::TimeoutOverride;
use crate::transport::DecodedResponse;
use std::collections::HashSet;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::time::Duration;

/// Options runtime partagées entre requête simple et pagination.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RequestOptions {
    pub debug_level: Option<DebugLevel>,
    pub timeout_override: TimeoutOverride,
    pub attempt: u32,
    pub cache_mode: CacheRequestMode,
}
impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            debug_level: None,
            timeout_override: TimeoutOverride::Inherit,
            attempt: 0,
            cache_mode: CacheRequestMode::Default,
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
    pub fn cache_default(mut self) -> Self {
        self.opts.cache_mode = CacheRequestMode::Default;
        self
    }

    #[inline]
    pub fn cache_bypass(mut self) -> Self {
        self.opts.cache_mode = CacheRequestMode::Bypass;
        self
    }

    #[inline]
    pub fn cache_refresh(mut self) -> Self {
        self.opts.cache_mode = CacheRequestMode::Refresh;
        self
    }

    #[inline]
    pub async fn execute(self) -> Result<E::Response, ApiClientError> {
        Ok(self.execute_decoded().await?.value)
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
        let value = self.execute().await?;
        let auth_state = client.auth_state();
        slot(auth_state.as_ref()).set_manual(value).await;
        Ok(())
    }

    pub async fn execute_decoded(self) -> Result<DecodedResponse<E::Response>, ApiClientError> {
        let mut plan = self.ep.plan(&self.client.plan_context())?;
        plan.overrides.timeout = match self.opts.timeout_override {
            TimeoutOverride::Inherit => plan.overrides.timeout,
            TimeoutOverride::Clear => None,
            TimeoutOverride::Set(d) => Some(d),
        };
        plan.overrides.attempt = self.opts.attempt;
        plan.overrides.page_index = 0;
        plan.overrides.cache_mode = self.opts.cache_mode;
        self.client.execute_plan::<E::Response>(plan).await
    }

    #[inline]
    pub fn paginate(self) -> PaginatedRequest<'a, Cx, E, T>
    where
        E::Response: PageItems,
    {
        PaginatedRequest::new(self)
    }

    #[inline]
    pub fn pages(self) -> PaginatedRequest<'a, Cx, E, T>
    where
        E::Response: PageItems,
    {
        self.paginate()
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
    caps: Caps,
}

impl<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport>
    PaginatedRequest<'a, Cx, E, T>
{
    #[inline]
    pub(crate) fn new(pending: PendingRequest<'a, Cx, E, T>) -> Self {
        let caps = pending.client.pagination_caps();
        Self { pending, caps }
    }

    #[inline]
    pub fn max_pages(mut self, v: u32) -> Self {
        self.caps.max_pages = v;
        self
    }

    #[inline]
    pub fn max_items(mut self, v: u64) -> Self {
        self.caps.max_items = v;
        self
    }

    #[inline]
    pub fn detect_loops(mut self, v: bool) -> Self {
        self.caps.detect_loops = v;
        self
    }

    pub async fn for_each_page<F>(self, mut f: F) -> Result<(), ApiClientError>
    where
        E::Response: PageItems + HasNextCursor,
        T: crate::transport::Transport,
        F: FnMut(DecodedResponse<E::Response>) -> Result<Control, ApiClientError>,
    {
        let first_plan = self.pending.ep.plan(&self.pending.client.plan_context())?;
        let mut runner =
            PaginationRunner::new(first_plan.endpoint.pagination.clone(), &first_plan)?;
        let mut first_plan = Some(first_plan);
        let mut seen: Option<HashSet<ProgressKey>> = if self.caps.detect_loops {
            Some(HashSet::new())
        } else {
            None
        };

        let ctx = crate::error::ErrorContext {
            endpoint: std::any::type_name::<E>(),
            method: http::Method::GET,
        };

        let mut items_count: u64 = 0;
        for page_index in 0..self.caps.max_pages {
            if let Some(seen) = seen.as_mut()
                && let Some(k) = runner.progress_key()
                && !seen.insert(k.clone())
            {
                return Err(ApiClientError::Pagination {
                    ctx: ctx.clone(),
                    msg: format!("loop detected (page_index={} key={:?})", page_index, k).into(),
                });
            }

            let mut plan = if let Some(plan) = first_plan.take() {
                plan
            } else {
                self.pending.ep.plan(&self.pending.client.plan_context())?
            };
            plan.overrides.timeout = match self.pending.opts.timeout_override {
                TimeoutOverride::Inherit => plan.overrides.timeout,
                TimeoutOverride::Clear => None,
                TimeoutOverride::Set(d) => Some(d),
            };
            plan.overrides.attempt = self.pending.opts.attempt;
            plan.overrides.page_index = page_index;
            plan.overrides.cache_mode = self.pending.opts.cache_mode;
            runner.apply_query(&mut plan);
            let resp: DecodedResponse<E::Response> = self
                .pending
                .client
                .execute_plan::<E::Response>(plan)
                .await?;

            let control_ctrl = runner.on_page(&resp, &ctx)?;
            let page_len = resp.value.len() as u64;
            if page_len > 0 {
                let new_total = items_count.checked_add(page_len).ok_or_else(|| {
                    ApiClientError::Pagination {
                        ctx: ctx.clone(),
                        msg: "items overflow".into(),
                    }
                })?;
                if new_total > self.caps.max_items {
                    return Err(ApiClientError::PaginationLimit {
                        ctx: ctx.clone(),
                        msg: format!(
                            "max_items reached (max={} seen={})",
                            self.caps.max_items, new_total
                        )
                        .into(),
                    });
                }
                items_count = new_total;
            }

            let control_user = f(resp)?;
            let control = match (control_ctrl, control_user) {
                (Control::Stop, _) => Control::Stop,
                (_, Control::Stop) => Control::Stop,
                _ => Control::Continue,
            };
            match control {
                Control::Continue => continue,
                Control::Stop => return Ok(()),
            }
        }

        Err(ApiClientError::PaginationLimit {
            ctx,
            msg: format!(
                "max_pages reached (max_pages={} seen_items={})",
                self.caps.max_pages, items_count
            )
            .into(),
        })
    }

    pub async fn collect(self) -> Result<Vec<<E::Response as PageItems>::Item>, ApiClientError>
    where
        E::Response: PageItems + HasNextCursor,
        T: crate::transport::Transport,
    {
        let mut out: Vec<<E::Response as PageItems>::Item> = Vec::new();
        self.for_each_page(|resp| {
            out.extend(<E::Response as PageItems>::inner_into_iter(resp.value));
            Ok(Control::Continue)
        })
        .await?;
        Ok(out)
    }
}

#[derive(Clone, Debug)]
enum PaginationRunner {
    OffsetLimit {
        offset_key: String,
        limit_key: String,
        offset: u64,
        limit: u64,
        stop_on_short_page: bool,
        stop: Stop,
    },
    Cursor {
        cursor_key: String,
        per_page_key: String,
        cursor: Option<String>,
        per_page: u64,
        send_cursor_on_first: bool,
        stop_when_cursor_missing: bool,
        stop: Stop,
        started: bool,
    },
    Paged {
        page_key: String,
        per_page_key: String,
        page: u64,
        per_page: u64,
        stop_on_short_page: bool,
        stop: Stop,
    },
}

fn set_query(query: &mut Vec<(String, String)>, key: &str, value: String) {
    query.retain(|(existing, _)| existing != key);
    query.push((key.to_string(), value));
}

fn remove_query(query: &mut Vec<(String, String)>, key: &str) {
    query.retain(|(existing, _)| existing != key);
}

impl PaginationRunner {
    fn new(
        plan: Option<PaginationPlan>,
        request: &crate::endpoint::RequestPlan,
    ) -> Result<Self, ApiClientError> {
        let ctx = ErrorContext {
            endpoint: request.endpoint.meta.name,
            method: request.endpoint.meta.method.clone(),
        };
        match plan {
            Some(PaginationPlan::OffsetLimit {
                offset_key,
                limit_key,
                offset,
                limit,
                stop_on_short_page,
                stop,
            }) => {
                if limit == 0 {
                    return Err(ApiClientError::Pagination {
                        ctx,
                        msg: "offset/limit: limit=0".into(),
                    });
                }
                Ok(Self::OffsetLimit {
                    offset_key,
                    limit_key,
                    offset,
                    limit,
                    stop_on_short_page,
                    stop,
                })
            }
            Some(PaginationPlan::Cursor {
                cursor_key,
                per_page_key,
                cursor,
                per_page,
                send_cursor_on_first,
                stop_when_cursor_missing,
                stop,
            }) => {
                if per_page == 0 {
                    return Err(ApiClientError::Pagination {
                        ctx,
                        msg: "cursor: per_page=0".into(),
                    });
                }
                Ok(Self::Cursor {
                    cursor_key,
                    per_page_key,
                    cursor,
                    per_page,
                    send_cursor_on_first,
                    stop_when_cursor_missing,
                    stop,
                    started: false,
                })
            }
            Some(PaginationPlan::Paged {
                page_key,
                per_page_key,
                page,
                per_page,
                stop_on_short_page,
                stop,
            }) => {
                if per_page == 0 {
                    return Err(ApiClientError::Pagination {
                        ctx,
                        msg: "paged: per_page=0".into(),
                    });
                }
                if page == 0 {
                    return Err(ApiClientError::Pagination {
                        ctx,
                        msg: "paged: page=0".into(),
                    });
                }
                Ok(Self::Paged {
                    page_key,
                    per_page_key,
                    page,
                    per_page,
                    stop_on_short_page,
                    stop,
                })
            }
            None => Err(ApiClientError::Pagination {
                ctx,
                msg: "endpoint is not paginated".into(),
            }),
        }
    }

    fn apply_query(&self, plan: &mut crate::endpoint::RequestPlan) {
        match self {
            Self::OffsetLimit {
                offset_key,
                limit_key,
                offset,
                limit,
                ..
            } => {
                set_query(
                    &mut plan.endpoint.policy.query,
                    offset_key,
                    offset.to_string(),
                );
                set_query(
                    &mut plan.endpoint.policy.query,
                    limit_key,
                    limit.to_string(),
                );
            }
            Self::Cursor {
                cursor_key,
                per_page_key,
                cursor,
                per_page,
                send_cursor_on_first,
                started,
                ..
            } => {
                set_query(
                    &mut plan.endpoint.policy.query,
                    per_page_key,
                    per_page.to_string(),
                );
                let should_send_cursor = *started || *send_cursor_on_first;
                match (should_send_cursor, cursor) {
                    (true, Some(c)) if !c.is_empty() => {
                        set_query(&mut plan.endpoint.policy.query, cursor_key, c.clone());
                    }
                    _ => {
                        remove_query(&mut plan.endpoint.policy.query, cursor_key);
                    }
                }
            }
            Self::Paged {
                page_key,
                per_page_key,
                page,
                per_page,
                ..
            } => {
                set_query(&mut plan.endpoint.policy.query, page_key, page.to_string());
                set_query(
                    &mut plan.endpoint.policy.query,
                    per_page_key,
                    per_page.to_string(),
                );
            }
        }
    }

    fn on_page<R>(
        &mut self,
        resp: &DecodedResponse<R>,
        ctx: &ErrorContext,
    ) -> Result<Control, ApiClientError>
    where
        R: PageItems + HasNextCursor,
    {
        match self {
            Self::OffsetLimit {
                offset,
                limit,
                stop_on_short_page,
                stop,
                ..
            } => {
                if matches!(stop, Stop::OnEmpty) && resp.value.len() == 0 {
                    return Ok(Control::Stop);
                }
                if *stop_on_short_page && (resp.value.len() as u64) < *limit {
                    return Ok(Control::Stop);
                }
                *offset = offset
                    .checked_add(*limit)
                    .ok_or_else(|| ApiClientError::Pagination {
                        ctx: ctx.clone(),
                        msg: "offset/limit: offset overflow".into(),
                    })?;
                Ok(Control::Continue)
            }
            Self::Cursor {
                cursor,
                stop_when_cursor_missing,
                stop,
                started,
                ..
            } => {
                *started = true;
                if matches!(stop, Stop::OnEmpty) && resp.value.len() == 0 {
                    return Ok(Control::Stop);
                }
                *cursor = resp
                    .value
                    .next_cursor()
                    .map(|c| c.to_string())
                    .filter(|s| !s.is_empty());
                if cursor.is_none() && *stop_when_cursor_missing {
                    return Ok(Control::Stop);
                }
                Ok(Control::Continue)
            }
            Self::Paged {
                page,
                per_page,
                stop_on_short_page,
                stop,
                ..
            } => {
                if matches!(stop, Stop::OnEmpty) && resp.value.len() == 0 {
                    return Ok(Control::Stop);
                }
                if *stop_on_short_page && (resp.value.len() as u64) < *per_page {
                    return Ok(Control::Stop);
                }
                *page = page
                    .checked_add(1)
                    .ok_or_else(|| ApiClientError::Pagination {
                        ctx: ctx.clone(),
                        msg: "paged: page overflow".into(),
                    })?;
                Ok(Control::Continue)
            }
        }
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        match self {
            Self::OffsetLimit { offset, .. } => Some(ProgressKey::U64(*offset)),
            Self::Cursor { cursor, .. } => cursor.clone().map(ProgressKey::Str),
            Self::Paged { page, .. } => Some(ProgressKey::U64(*page)),
        }
    }
}
