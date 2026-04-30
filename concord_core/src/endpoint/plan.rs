#![allow(dead_code)]

use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    PageAdvance, PageDecision, PageInit, PageItems, PageRequest, PaginationController, ProgressKey,
};
use crate::policy::ResolvedPolicy;
use crate::transport::RequestMeta;
use bytes::Bytes;
use http::HeaderValue;
use http::Method;
use std::any::Any;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointMeta {
    pub name: &'static str,
    pub method: Method,
    pub idempotent: bool,
    pub facade_path: &'static [&'static str],
}

impl EndpointMeta {
    #[inline]
    pub fn request_meta(&self, attempt: u32, page_index: u32) -> RequestMeta {
        RequestMeta {
            endpoint: self.name,
            method: self.method.clone(),
            idempotent: self.idempotent,
            attempt,
            page_index,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EndpointPlan {
    pub meta: EndpointMeta,
    pub route: ResolvedRoute,
    pub policy: ResolvedPolicy,
    pub body: BodyPlan,
    pub response: ResponsePlan,
    pub pagination: Option<PaginationPlan>,
}

#[derive(Clone, Debug, Default)]
pub struct RequestArgs {
    pub body: Option<Bytes>,
}

#[derive(Clone, Debug, Default)]
pub struct RequestOverrides {
    pub debug_level: Option<crate::debug::DebugLevel>,
    pub timeout: Option<std::time::Duration>,
    pub attempt: u32,
    pub page_index: u32,
    pub cache_mode: crate::cache::CacheRequestMode,
}

#[derive(Clone, Debug)]
pub struct RequestPlan {
    pub endpoint: EndpointPlan,
    pub args: RequestArgs,
    pub overrides: RequestOverrides,
}

#[derive(Clone, Debug, Default)]
pub struct AttemptState {
    pub attempt: u32,
    pub page_index: u32,
    pub auth_attempt: crate::auth::AuthAttemptSummary,
    pub cache_revalidation: Option<crate::cache::CacheRevalidation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedRoute {
    pub scheme: http::uri::Scheme,
    pub host: String,
    pub path: String,
}

impl Default for ResolvedRoute {
    fn default() -> Self {
        Self {
            scheme: http::uri::Scheme::HTTPS,
            host: String::new(),
            path: "/".to_string(),
        }
    }
}

impl ResolvedRoute {
    pub fn new(
        scheme: http::uri::Scheme,
        host: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            scheme,
            host: host.into(),
            path: path.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BodyPlan {
    #[default]
    None,
    Encoded {
        content_type: Option<HeaderValue>,
        format: crate::codec::Format,
    },
}

pub type PlanDecodeFn = fn(
    crate::transport::BuiltResponse,
    ErrorContext,
) -> Result<Box<dyn Any + Send>, ApiClientError>;

#[derive(Clone)]
pub struct ResponsePlan {
    pub accept: Option<HeaderValue>,
    pub no_content: bool,
    pub format: crate::codec::Format,
    pub decode: PlanDecodeFn,
}

impl fmt::Debug for ResponsePlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponsePlan")
            .field("accept", &self.accept)
            .field("no_content", &self.no_content)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub enum PaginationPlan {
    OffsetLimit {
        offset_key: String,
        limit_key: String,
        offset: u64,
        limit: u64,
        stop_on_short_page: bool,
        stop: crate::pagination::Stop,
    },
    Cursor {
        cursor_key: String,
        per_page_key: String,
        cursor: Option<String>,
        per_page: u64,
        send_cursor_on_first: bool,
        stop_when_cursor_missing: bool,
        stop: crate::pagination::Stop,
    },
    Paged {
        page_key: String,
        per_page_key: String,
        page: u64,
        per_page: u64,
        stop_on_short_page: bool,
        stop: crate::pagination::Stop,
    },
    Custom(CustomPaginationPlan),
}

pub type CustomPaginationInitFn =
    for<'a> fn(PageInit<'a>) -> Result<Box<dyn Any + Send + Sync>, ApiClientError>;
pub type CustomPaginationApplyFn =
    for<'a> fn(&dyn Any, &mut PageRequest<'a>) -> Result<(), ApiClientError>;
pub type CustomPaginationAdvanceFn = for<'a> fn(
    &mut dyn Any,
    &(dyn Any + Send),
    PageAdvance<'a>,
) -> Result<PageDecision, ApiClientError>;
pub type CustomPaginationProgressKeyFn = fn(&dyn Any) -> Option<ProgressKey>;

#[derive(Clone, Debug)]
pub struct CustomPaginationPlan {
    pub controller: &'static str,
    pub init: CustomPaginationInitFn,
    pub apply: CustomPaginationApplyFn,
    pub advance: CustomPaginationAdvanceFn,
    pub progress_key: CustomPaginationProgressKeyFn,
}

impl PaginationPlan {
    pub fn custom<C, Page>() -> Self
    where
        C: PaginationController<Page> + Default,
        Page: PageItems,
    {
        Self::Custom(CustomPaginationPlan {
            controller: std::any::type_name::<C>(),
            init: custom_pagination_init::<C, Page>,
            apply: custom_pagination_apply::<C, Page>,
            advance: custom_pagination_advance::<C, Page>,
            progress_key: custom_pagination_progress_key::<C, Page>,
        })
    }
}

fn custom_pagination_init<C, Page>(
    ctx: PageInit<'_>,
) -> Result<Box<dyn Any + Send + Sync>, ApiClientError>
where
    C: PaginationController<Page> + Default,
    Page: PageItems,
{
    let controller = C::default();
    let state = controller.init(ctx)?;
    Ok(Box::new(state))
}

fn custom_pagination_apply<C, Page>(
    state: &dyn Any,
    request: &mut PageRequest<'_>,
) -> Result<(), ApiClientError>
where
    C: PaginationController<Page> + Default,
    Page: PageItems,
{
    let Some(state) = state.downcast_ref::<C::State>() else {
        return Err(custom_pagination_error(
            "custom pagination state type mismatch",
        ));
    };
    C::default().apply(state, request)
}

fn custom_pagination_advance<C, Page>(
    state: &mut dyn Any,
    page: &(dyn Any + Send),
    ctx: PageAdvance<'_>,
) -> Result<PageDecision, ApiClientError>
where
    C: PaginationController<Page> + Default,
    Page: PageItems,
{
    let Some(state) = state.downcast_mut::<C::State>() else {
        return Err(custom_pagination_error(
            "custom pagination state type mismatch",
        ));
    };
    let Some(page) = page.downcast_ref::<Page>() else {
        return Err(custom_pagination_error(
            "custom pagination page type mismatch",
        ));
    };
    C::default().advance(state, page, ctx)
}

fn custom_pagination_progress_key<C, Page>(state: &dyn Any) -> Option<ProgressKey>
where
    C: PaginationController<Page> + Default,
    Page: PageItems,
{
    let state = state.downcast_ref::<C::State>()?;
    C::default().progress_key(state)
}

fn custom_pagination_error(msg: &'static str) -> ApiClientError {
    ApiClientError::Pagination {
        ctx: ErrorContext {
            endpoint: "custom pagination",
            method: Method::GET,
        },
        msg: msg.into(),
    }
}
