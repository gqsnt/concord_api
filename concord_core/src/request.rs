use crate::client::{ApiClient, ClientContext};
use crate::debug::DebugLevel;
use crate::endpoint::{Endpoint, ResponseSpec};
use crate::error::ApiClientError;
use crate::pagination::{
    Caps, Control, Controller, PageItems, PaginationPart, ProgressKey, PaginatedEndpoint,
};
use crate::policy::PolicyPatch;
use crate::timeout::TimeoutOverride;
use crate::transport::{DecodedResponse, RequestMeta};
use core::future::IntoFuture;
use std::collections::HashSet;
use std::time::Duration;

fn apply_timeout_override(p: &mut PolicyPatch<'_>, t: TimeoutOverride) {
    match t {
        TimeoutOverride::Inherit => {}
        TimeoutOverride::Clear => p.set_timeout_override(None),
        TimeoutOverride::Set(d) => p.set_timeout_override(Some(d)),
    }
}

fn is_idempotent(m: &http::Method) -> bool {
    matches!(
    *m,
    http::Method::GET
      | http::Method::HEAD
      | http::Method::PUT
      | http::Method::DELETE
      | http::Method::OPTIONS
  )
}

pub struct PendingRequest<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport> {
    client: &'a ApiClient<Cx, T>,
    ep: E,
    debug_level: Option<DebugLevel>,
    timeout_override: TimeoutOverride,
    attempt: u32,
}

impl<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport> PendingRequest<'a, Cx, E, T> {
    #[inline]
    pub(crate) fn new(client: &'a ApiClient<Cx, T>, ep: E) -> Self {
        Self {
            client,
            ep,
            debug_level: None,
            timeout_override: TimeoutOverride::Inherit,
            attempt: 0,
        }
    }

    #[inline]
    pub fn debug_level(mut self, level: DebugLevel) -> Self {
        self.debug_level = Some(level);
        self
    }

    #[inline]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout_override = TimeoutOverride::Set(d);
        self
    }

    #[inline]
    pub fn clear_timeout(mut self) -> Self {
        self.timeout_override = TimeoutOverride::Clear;
        self
    }

    #[inline]
    pub fn inherit_timeout(mut self) -> Self {
        self.timeout_override = TimeoutOverride::Inherit;
        self
    }

    #[inline]
    pub fn attempt(mut self, v: u32) -> Self {
        self.attempt = v;
        self
    }

    #[inline]
    pub async fn execute(self) -> Result<<E::Response as ResponseSpec>::Output, ApiClientError> {
        Ok(self.execute_decoded().await?.value)
    }

    pub async fn execute_decoded(self) -> Result<DecodedResponse<<E::Response as ResponseSpec>::Output>, ApiClientError> {
        let dbg = self.debug_level.unwrap_or(self.client.debug_level());
        let timeout_override = self.timeout_override;
        let meta = RequestMeta {
            endpoint: self.ep.name(),
            method: E::METHOD.clone(),
            idempotent: is_idempotent(&E::METHOD),
            attempt: self.attempt,
            page_index: 0,
        };
        self.client
            .execute_decoded_ref_with(&self.ep, meta, dbg, move |policy| {
                apply_timeout_override(policy, timeout_override);
                Ok(())
            })
            .await
    }

    #[inline]
    pub fn paginate(self) -> PaginatedRequest<'a, Cx, E, T>
    where
        E: PaginatedEndpoint<Cx>,
        <E::Response as ResponseSpec>::Output: PageItems,
        <E::Pagination as PaginationPart<Cx, E>>::Ctrl: Controller<Cx, E>,
    {
        PaginatedRequest::new(self.client, self.ep)
            .debug_level_opt(self.debug_level)
            .timeout_override(self.timeout_override)
            .attempt(self.attempt)
    }

    // internal helper to pass through Option without re-wrapping
    #[inline]
    fn debug_level_opt(mut self, v: Option<DebugLevel>) -> Self {
        self.debug_level = v;
        self
    }
}

impl<'a, Cx, E, T> IntoFuture for PendingRequest<'a, Cx, E, T>
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    T: crate::transport::Transport,
{
    type Output = Result<<E::Response as ResponseSpec>::Output, ApiClientError>;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { self.execute().await })
    }
}

pub struct PaginatedRequest<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport> {
    client: &'a ApiClient<Cx, T>,
    ep: E,
    caps: Caps,
    debug_level: Option<DebugLevel>,
    timeout_override: TimeoutOverride,
    attempt: u32,
}

impl<'a, Cx: ClientContext, E: Endpoint<Cx>, T: crate::transport::Transport> PaginatedRequest<'a, Cx, E, T> {
    #[inline]
    pub(crate) fn new(client: &'a ApiClient<Cx, T>, ep: E) -> Self {
        Self {
            client,
            ep,
            caps: client.pagination_caps(),
            debug_level: None,
            timeout_override: TimeoutOverride::Inherit,
            attempt: 0,
        }
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

    #[inline]
    pub fn debug_level(mut self, level: DebugLevel) -> Self {
        self.debug_level = Some(level);
        self
    }

    #[inline]
    pub(crate) fn debug_level_opt(mut self, v: Option<DebugLevel>) -> Self {
        self.debug_level = v;
        self
    }

    #[inline]
    pub fn timeout_override(mut self, v: TimeoutOverride) -> Self {
        self.timeout_override = v;
        self
    }

    #[inline]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout_override = TimeoutOverride::Set(d);
        self
    }

    #[inline]
    pub fn clear_timeout(mut self) -> Self {
        self.timeout_override = TimeoutOverride::Clear;
        self
    }

    #[inline]
    pub fn attempt(mut self, v: u32) -> Self {
        self.attempt = v;
        self
    }
}

impl<'a, Cx, E, T> IntoFuture for PaginatedRequest<'a, Cx, E, T>
where
    Cx: ClientContext,
    E: PaginatedEndpoint<Cx>,
    <E::Response as ResponseSpec>::Output: PageItems,
    <E::Pagination as PaginationPart<Cx, E>>::Ctrl: Controller<Cx, E>,
    T: crate::transport::Transport,
{
    type Output = Result<
        Vec<<<E::Response as ResponseSpec>::Output as PageItems>::Item>,
        ApiClientError,
    >;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(mut self) -> Self::IntoFuture {
        Box::pin(async move {
            let ctrl = <E::Pagination as PaginationPart<Cx, E>>::controller(self.client.vars(), &self.ep)?;
            let mut st = ctrl.init(&self.ep)?;
            let mut seen: HashSet<ProgressKey> = HashSet::new();
            let mut out: Vec<<<E::Response as ResponseSpec>::Output as PageItems>::Item> = Vec::new();
            let mut items_count: u64 = 0;

            let dbg = self.debug_level.unwrap_or(self.client.debug_level());
            let timeout_override = self.timeout_override;
            let attempt = self.attempt;

            for page_index in 0..self.caps.max_pages {
                if self.caps.detect_loops
                    && let Some(k) = ctrl.progress_key(&st, &self.ep)
                    && !seen.insert(k.clone())
                {
                    return Err(ApiClientError::Pagination(
                        format!(
                            "loop detected (endpoint={} page_index={} key={:?})",
                            self.ep.name(),
                            page_index,
                            k
                        )
                            .into(),
                    ));
                }

                let meta = RequestMeta {
                    endpoint: self.ep.name(),
                    method: E::METHOD.clone(),
                    idempotent: is_idempotent(&E::METHOD),
                    attempt,
                    page_index,
                };

                let resp: DecodedResponse<<E::Response as ResponseSpec>::Output> = self
                    .client
                    .execute_decoded_ref_with(&self.ep, meta, dbg, |policy| {
                        apply_timeout_override(policy, timeout_override);
                        ctrl.apply_policy(&st, &self.ep, policy)
                    })
                    .await?;

                let control = ctrl.on_page(&mut st, &mut self.ep, &resp)?;
                let page_len = resp.value.len() as u64;
                if page_len > 0 {
                    let new_total = items_count.checked_add(page_len).ok_or_else(|| {
                        ApiClientError::Pagination(
                            format!("items overflow (endpoint={})", self.ep.name()).into(),
                        )
                    })?;
                    if new_total > self.caps.max_items {
                        return Err(ApiClientError::PaginationLimit(
                            format!(
                                "max_items reached (endpoint={} max={} seen={})",
                                self.ep.name(),
                                self.caps.max_items,
                                new_total
                            )
                                .into(),
                        ));
                    }
                    items_count = new_total;
                }

                out.extend(<<E::Response as ResponseSpec>::Output as PageItems>::inner_into_iter(resp.value));
                match control {
                    Control::Continue => continue,
                    Control::Stop => return Ok(out),
                }
            }

            Err(ApiClientError::PaginationLimit(
                format!(
                    "max_pages reached (endpoint={} max_pages={} seen_items={})",
                    self.ep.name(),
                    self.caps.max_pages,
                    items_count
                )
                    .into(),
            ))
        })
    }
}