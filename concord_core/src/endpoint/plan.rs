#![allow(dead_code)]

use crate::advanced::{MultipartBody, MultipartBodyError, MultipartFormat};
use crate::error::{ApiClientError, ErrorContext};
use crate::multipart::MultipartBodyErrorKind;
use crate::pagination::{
    HasNextCursor, PageAdvance, PageDecision, PageInit, PageItems, PageRequest,
    PaginationController, ProgressKey,
};
use crate::policy::ResolvedPolicy;
use crate::record::RecordBody;
use crate::stream_body::StreamBody;
use crate::transport::RequestMeta;
use crate::transport::TransportRequestBody;
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

#[derive(Debug, Default)]
pub struct RequestArgs {
    pub body: TransportRequestBody,
    pub(crate) stream_size_hint: Option<crate::stream_body::BodySizeHint>,
    pub(crate) multipart_content_type: Option<HeaderValue>,
}

impl RequestArgs {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_body_bytes(body: Bytes) -> Self {
        Self {
            body: TransportRequestBody::from_bytes(body),
            stream_size_hint: None,
            multipart_content_type: None,
        }
    }

    pub fn with_stream_body(body: StreamBody) -> Self {
        let stream_size_hint = body.size_hint();
        Self {
            body: body.into_transport_body(),
            stream_size_hint: Some(stream_size_hint),
            multipart_content_type: None,
        }
    }

    pub fn with_record_body<T, F>(body: RecordBody<T>) -> Self
    where
        F: crate::record::RecordFormat<T>,
        T: Send + 'static,
    {
        Self {
            body: body.into_transport_body::<F>(),
            stream_size_hint: None,
            multipart_content_type: None,
        }
    }

    pub fn with_multipart_body<F>(body: MultipartBody) -> Result<Self, MultipartBodyError>
    where
        F: MultipartFormat,
    {
        let multipart_content_type = body.try_content_type::<F>().map_err(|_| {
            MultipartBodyError::new(MultipartBodyErrorKind::InvalidMultipartContentType)
        })?;
        Ok(Self {
            body: body.into_transport_body::<F>()?,
            stream_size_hint: None,
            multipart_content_type: Some(multipart_content_type),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct RequestOverrides {
    pub debug_level: Option<crate::debug::DebugLevel>,
    pub timeout: Option<std::time::Duration>,
    pub attempt: u32,
    pub page_index: u32,
}

#[derive(Debug)]
pub struct RequestPlan {
    pub endpoint: EndpointPlan,
    pub args: RequestArgs,
    pub overrides: RequestOverrides,
}

#[derive(Clone, Debug)]
pub struct RequestPlanView {
    pub endpoint: EndpointPlan,
    pub overrides: RequestOverrides,
}

#[derive(Clone, Debug, Default)]
pub struct AttemptState {
    pub attempt: u32,
    pub page_index: u32,
    pub auth_attempt: crate::auth::AuthAttemptSummary,
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
    RawStream {
        content_type: HeaderValue,
    },
    Multipart {
        content_type: HeaderValue,
        format: crate::codec::Format,
    },
    Records {
        content_type: HeaderValue,
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
        offset: u64,
        limit: u64,
    },
    Cursor {
        cursor: Option<String>,
        per_page: u64,
        send_cursor_on_first: bool,
        stop_when_cursor_missing: bool,
        next_cursor: CursorNextFn,
    },
    Paged {
        page: u64,
        per_page: u64,
    },
    Custom(CustomPaginationPlan),
}

pub type CursorNextFn =
    for<'a> fn(&(dyn Any + Send), ErrorContext) -> Result<Option<String>, ApiClientError>;

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
    pub fn cursor<Page>(value: crate::pagination::CursorPagination) -> Self
    where
        Page: PageItems + HasNextCursor,
    {
        Self::Cursor {
            cursor: value.cursor,
            per_page: value.per_page,
            send_cursor_on_first: value.send_cursor_on_first,
            stop_when_cursor_missing: value.stop_when_cursor_missing,
            next_cursor: cursor_next::<Page>,
        }
    }

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

fn cursor_next<Page>(
    page: &(dyn Any + Send),
    ctx: ErrorContext,
) -> Result<Option<String>, ApiClientError>
where
    Page: PageItems + HasNextCursor,
{
    let Some(page) = page.downcast_ref::<Page>() else {
        return Err(ApiClientError::Pagination {
            ctx,
            msg: "cursor pagination page type mismatch".into(),
        });
    };
    Ok(page
        .next_cursor()
        .map(|cursor| cursor.to_string())
        .filter(|cursor| !cursor.is_empty()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pagination::{CursorPagination, OffsetLimitPagination, PagedPagination};

    #[test]
    fn built_in_pagination_plan_metadata_has_no_query_keys() {
        let offset = PaginationPlan::from(OffsetLimitPagination {
            offset_key: "offset".into(),
            limit_key: "limit".into(),
            offset: 3,
            limit: 9,
        });
        let offset_debug = format!("{offset:?}");
        assert!(offset_debug.contains("OffsetLimit"));
        assert!(offset_debug.contains("offset: 3"));
        assert!(offset_debug.contains("limit: 9"));
        assert!(!offset_debug.contains("offset_key"));
        assert!(!offset_debug.contains("limit_key"));

        let paged = PaginationPlan::from(PagedPagination {
            page_key: "page".into(),
            per_page_key: "per_page".into(),
            page: 2,
            per_page: 7,
        });
        let paged_debug = format!("{paged:?}");
        assert!(paged_debug.contains("Paged"));
        assert!(paged_debug.contains("page: 2"));
        assert!(paged_debug.contains("per_page: 7"));
        assert!(!paged_debug.contains("page_key"));
        assert!(!paged_debug.contains("per_page_key"));
    }

    #[test]
    fn cursor_pagination_plan_preserves_endpoint_state_flags() {
        let plan = PaginationPlan::cursor::<Vec<String>>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "per_page".into(),
            cursor: Some("start".to_string()),
            per_page: 5,
            send_cursor_on_first: true,
            stop_when_cursor_missing: false,
        });
        let debug = format!("{plan:?}");

        match &plan {
            PaginationPlan::Cursor {
                cursor,
                per_page,
                send_cursor_on_first,
                stop_when_cursor_missing,
                ..
            } => {
                assert_eq!(cursor, &Some("start".to_string()));
                assert_eq!(*per_page, 5);
                assert!(send_cursor_on_first);
                assert!(!stop_when_cursor_missing);
            }
            other => panic!("expected cursor plan, got {other:?}"),
        }

        assert!(debug.contains("Cursor"));
        assert!(!debug.contains("cursor_key"));
        assert!(!debug.contains("per_page_key"));
    }
}
