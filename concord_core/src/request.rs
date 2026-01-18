use crate::client::{ApiClient, ClientContext};
use crate::debug::DebugLevel;
use crate::endpoint::{Endpoint, ResponseSpec};
use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    Caps, Control, Controller, PageItems, PaginatedEndpoint, PaginationPart, ProgressKey,
};
use crate::policy::PolicyPatch;
use crate::timeout::TimeoutOverride;
use crate::transport::{DecodedResponse, RequestMeta};
use std::collections::HashSet;
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
    pub async fn execute(self) -> Result<<E::Response as ResponseSpec>::Output, ApiClientError> {
        Ok(self.execute_decoded().await?.value)
    }

    pub async fn execute_decoded(
        self,
    ) -> Result<DecodedResponse<<E::Response as ResponseSpec>::Output>, ApiClientError> {
        let dbg = self.opts.debug_level.unwrap_or(self.client.debug_level());
        let timeout_override = self.opts.timeout_override;
        let _ctx = ErrorContext {
            endpoint: self.ep.name(),
            method: E::METHOD.clone(),
        };
        let meta = RequestMeta {
            endpoint: self.ep.name(),
            method: E::METHOD.clone(),
            idempotent: is_idempotent(&E::METHOD),
            attempt: self.opts.attempt,
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
        PaginatedRequest::new(self)
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

    pub async fn for_each_page<F>(mut self, mut f: F) -> Result<(), ApiClientError>
    where
        E: PaginatedEndpoint<Cx>,
        <E::Response as ResponseSpec>::Output: PageItems,
        <E::Pagination as PaginationPart<Cx, E>>::Ctrl: Controller<Cx, E>,
        T: crate::transport::Transport,
        F: FnMut(
            DecodedResponse<<E::Response as ResponseSpec>::Output>,
        ) -> Result<Control, ApiClientError>,
    {
        let ctrl = <E::Pagination as PaginationPart<Cx, E>>::controller(
            self.pending.client.vars(),
            &self.pending.ep,
        )?;
        let mut st = ctrl.init(&self.pending.ep)?;
        let mut seen: Option<HashSet<ProgressKey>> = if self.caps.detect_loops {
            Some(HashSet::new())
        } else {
            None
        };

        let dbg = self
            .pending
            .opts
            .debug_level
            .unwrap_or(self.pending.client.debug_level());
        let timeout_override = self.pending.opts.timeout_override;
        let attempt = self.pending.opts.attempt;

        let ctx = ErrorContext {
            endpoint: self.pending.ep.name(),
            method: E::METHOD.clone(),
        };

        let mut items_count: u64 = 0;
        for page_index in 0..self.caps.max_pages {
            if let Some(seen) = seen.as_mut()
                && let Some(k) = ctrl.progress_key(&st, &self.pending.ep)
                && !seen.insert(k.clone())
            {
                return Err(ApiClientError::Pagination {
                    ctx: ctx.clone(),
                    msg: format!("loop detected (page_index={} key={:?})", page_index, k).into(),
                });
            }

            let meta = RequestMeta {
                endpoint: self.pending.ep.name(),
                method: E::METHOD.clone(),
                idempotent: is_idempotent(&E::METHOD),
                attempt,
                page_index,
            };

            let resp: DecodedResponse<<E::Response as ResponseSpec>::Output> = self
                .pending
                .client
                .execute_decoded_ref_with(&self.pending.ep, meta, dbg, |policy| {
                    apply_timeout_override(policy, timeout_override);
                    ctrl.apply_policy(&st, &self.pending.ep, policy)
                })
                .await?;

            let control_ctrl = ctrl.on_page(&mut st, &mut self.pending.ep, &resp)?;
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

    pub async fn collect(
        self,
    ) -> Result<Vec<<<E::Response as ResponseSpec>::Output as PageItems>::Item>, ApiClientError>
    where
        E: PaginatedEndpoint<Cx>,
        <E::Response as ResponseSpec>::Output: PageItems,
        <E::Pagination as PaginationPart<Cx, E>>::Ctrl: Controller<Cx, E>,
        T: crate::transport::Transport,
    {
        let mut out: Vec<<<E::Response as ResponseSpec>::Output as PageItems>::Item> = Vec::new();
        self.for_each_page(|resp| {
            out.extend(
                <<E::Response as ResponseSpec>::Output as PageItems>::inner_into_iter(resp.value),
            );
            Ok(Control::Continue)
        })
        .await?;
        Ok(out)
    }
}
