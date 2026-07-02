use bytes::Bytes;
use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthDecision, AuthError, AuthErrorKind,
    AuthPlacement, AuthProvenance, AuthRequirement, AuthUsageId, BuiltResponse, CursorPagination,
    DecodedResponse, OffsetLimitPagination, PagedPagination, PaginateBinding,
    PostResponseHookContext, PreSendHookContext, RateLimitContext, RateLimitFuture,
    RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext, RateLimiter, RequestMeta,
    RetryContext, RetryDecision, RetryPolicy, RuntimeHooks, SingleObjectPaginationRuntimeAdapter,
    Transport, TransportBody, TransportByteStream, TransportError, TransportErrorHookContext,
    TransportErrorKind, TransportRequest, TransportRequestBody, TransportResponse,
    apply_basic_credential,
};
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, PaginationPlan, RequestArgs,
    RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{
    ApiClient, ApiClientError, ApiKey, BasicCredential, ClientContext, Endpoint, PaginatedEndpoint,
};
use concord_core::prelude::{HasNextCursor, PageItems};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::{Mutex, Notify, watch};

pub struct CapturedTransportRequest {
    pub meta: RequestMeta,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: TransportRequestBody,
    pub timeout: Option<Duration>,
    pub rate_limit: concord_core::advanced::RateLimitPlan,
    pub transport_auth: Option<concord_core::advanced::TransportAuth>,
    pub extensions: concord_core::auth::RequestExtensions,
}

impl fmt::Debug for CapturedTransportRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body = match &self.body {
            TransportRequestBody::Empty => TransportRequestBody::Empty,
            TransportRequestBody::Bytes(bytes) => TransportRequestBody::from_bytes(bytes.clone()),
            TransportRequestBody::Stream(_) => {
                TransportRequestBody::Stream(TransportByteStream::new(EmptyDebugStream))
            }
        };
        let temp = TransportRequest {
            meta: self.meta.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body,
            timeout: self.timeout,
            rate_limit: self.rate_limit.clone(),
            transport_auth: self.transport_auth.clone(),
            extensions: self.extensions.clone(),
        };
        write!(f, "{temp:?}")
    }
}

struct EmptyDebugStream;

impl futures_core::Stream for EmptyDebugStream {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(None)
    }
}

#[derive(Clone, Debug)]
pub struct TestAuthVars {
    pub token: Option<String>,
    pub identity: &'static str,
}

impl Default for TestAuthVars {
    fn default() -> Self {
        Self {
            token: None,
            identity: "anon",
        }
    }
}

#[derive(Clone)]
pub struct TestCx;

impl ClientContext for TestCx {
    type Vars = ();
    type AuthVars = TestAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<concord_core::advanced::PreparedAuthCredential, AuthError>,
    > {
        Box::pin(async move {
            let token = auth.token.as_deref().ok_or_else(|| {
                AuthError::new(
                    AuthErrorKind::MissingCredential,
                    format!(
                        "missing credential `{}`; acquire or configure it before sending request",
                        requirement.credential.id
                    ),
                )
            })?;
            let application = match requirement.placement {
                AuthPlacement::Bearer | AuthPlacement::Header(_) | AuthPlacement::Query(_) => {
                    let material = ApiKey::new(token.to_string());
                    concord_core::advanced::apply_secret_credential(
                        request,
                        requirement,
                        &material,
                    )?
                }
                AuthPlacement::Basic | AuthPlacement::Certificate => {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "test context supports bearer/header/query auth only",
                    ));
                }
            };
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(concord_core::advanced::PreparedAuthCredential::new(
                applied,
                application,
            ))
        })
    }

    fn handle_auth_response<'a>(
        requirement: &'a AuthRequirement,
        applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async move {
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                if auth.identity == "refresh" {
                    return Ok(AuthDecision::RetryAfterRefresh {
                        credential: requirement.credential.clone(),
                        generation: applied.generation,
                        reason: concord_core::advanced::AuthRetryReason::Unauthorized,
                    });
                }
                if auth.identity == "refresh-error" {
                    return Err(AuthError::new(
                        AuthErrorKind::ProviderRejected,
                        "auth refresh failed",
                    ));
                }
                Ok(AuthDecision::Fail)
            } else {
                Ok(AuthDecision::Continue)
            }
        })
    }
}

#[derive(Clone)]
pub struct TextEndpoint {
    pub name: &'static str,
    pub method: Method,
    pub path: &'static str,
    pub policy: ResolvedPolicy,
    pub pagination: Option<PaginationPlan>,
}

impl Default for TextEndpoint {
    fn default() -> Self {
        Self {
            name: "Text",
            method: Method::GET,
            path: "/text",
            policy: ResolvedPolicy::default(),
            pagination: None,
        }
    }
}

impl Endpoint<TestCx> for TextEndpoint {
    type Response = String;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.clone(),
            decode_string,
        ))
    }
}

#[derive(Clone)]
pub struct ItemsEndpoint {
    pub start: u64,
    pub count: u64,
    pub policy: ResolvedPolicy,
    pub pagination: PaginationPlan,
}

impl Default for ItemsEndpoint {
    fn default() -> Self {
        Self {
            start: 0,
            count: 2,
            policy: Default::default(),
            pagination: PaginationPlan::OffsetLimit {
                offset: 0,
                limit: 2,
            },
        }
    }
}

impl Endpoint<TestCx> for ItemsEndpoint {
    type Response = Vec<String>;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "Items",
            Method::GET,
            "/items",
            self.policy.clone(),
            Some(self.pagination.clone()),
            decode_items,
        );
        match &self.pagination {
            PaginationPlan::OffsetLimit { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("offset".to_string(), self.start.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("limit".to_string(), self.count.to_string()));
            }
            PaginationPlan::Paged { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("page".to_string(), self.start.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
            PaginationPlan::Cursor { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("cursor".to_string(), self.start.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
        }
        Ok(plan)
    }
}

impl PaginatedEndpoint<TestCx> for ItemsEndpoint {
    fn single_object_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::SingleObjectPaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        match &self.pagination {
            PaginationPlan::OffsetLimit { .. } => {
                Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
                    OffsetLimitPagination,
                >::new()))
            }
            PaginationPlan::Paged { .. } => Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
                PagedPagination,
            >::new())),
            PaginationPlan::Cursor { .. } => None,
        }
    }
}

impl PaginateBinding<OffsetLimitPagination> for ItemsEndpoint {
    fn load_pagination(&self) -> OffsetLimitPagination {
        OffsetLimitPagination {
            offset: self.start,
            limit: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &OffsetLimitPagination) {
        self.start = pagination.offset;
        self.count = pagination.limit;
    }
}

impl PaginateBinding<PagedPagination> for ItemsEndpoint {
    fn load_pagination(&self) -> PagedPagination {
        PagedPagination {
            page: self.start,
            per_page: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &PagedPagination) {
        self.start = pagination.page;
        self.count = pagination.per_page;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageOnlyItems {
    pub items: Vec<String>,
}

impl PageItems for PageOnlyItems {
    type Item = String;

    fn item_count_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoHintItems {
    pub items: Vec<String>,
}

impl PageItems for NoHintItems {
    type Item = String;

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

#[derive(Clone)]
pub struct NoHintItemsEndpoint {
    pub policy: ResolvedPolicy,
    pub pagination: PaginationPlan,
}

impl Default for NoHintItemsEndpoint {
    fn default() -> Self {
        Self {
            policy: Default::default(),
            pagination: PaginationPlan::OffsetLimit {
                offset: 0,
                limit: 2,
            },
        }
    }
}

impl Endpoint<TestCx> for NoHintItemsEndpoint {
    type Response = NoHintItems;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            "NoHintItems",
            Method::GET,
            "/no-hint-items",
            self.policy.clone(),
            Some(self.pagination.clone()),
            decode_no_hint_items,
        ))
    }
}

impl PaginatedEndpoint<TestCx> for NoHintItemsEndpoint {}

#[derive(Clone)]
pub struct PageOnlyItemsEndpoint {
    pub page: u64,
    pub count: u64,
    pub policy: ResolvedPolicy,
    pub pagination: PaginationPlan,
}

impl Default for PageOnlyItemsEndpoint {
    fn default() -> Self {
        Self {
            page: 0,
            count: 2,
            policy: Default::default(),
            pagination: PaginationPlan::OffsetLimit {
                offset: 0,
                limit: 2,
            },
        }
    }
}

impl Endpoint<TestCx> for PageOnlyItemsEndpoint {
    type Response = PageOnlyItems;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "PageOnlyItems",
            Method::GET,
            "/page-only-items",
            self.policy.clone(),
            Some(self.pagination.clone()),
            decode_page_only_items,
        );
        match &self.pagination {
            PaginationPlan::Paged { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("page".to_string(), self.page.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
            PaginationPlan::OffsetLimit { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("offset".to_string(), self.page.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("limit".to_string(), self.count.to_string()));
            }
            PaginationPlan::Cursor { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("cursor".to_string(), self.page.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
        }
        Ok(plan)
    }
}

impl PaginatedEndpoint<TestCx> for PageOnlyItemsEndpoint {
    fn single_object_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::SingleObjectPaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        match &self.pagination {
            PaginationPlan::Paged { .. } => Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
                PagedPagination,
            >::new())),
            PaginationPlan::OffsetLimit { .. } => {
                Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
                    OffsetLimitPagination,
                >::new()))
            }
            PaginationPlan::Cursor { .. } => None,
        }
    }
}

impl PaginateBinding<PagedPagination> for PageOnlyItemsEndpoint {
    fn load_pagination(&self) -> PagedPagination {
        PagedPagination {
            page: self.page,
            per_page: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &PagedPagination) {
        self.page = pagination.page;
        self.count = pagination.per_page;
    }
}

impl PaginateBinding<OffsetLimitPagination> for PageOnlyItemsEndpoint {
    fn load_pagination(&self) -> OffsetLimitPagination {
        OffsetLimitPagination {
            offset: self.page,
            limit: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &OffsetLimitPagination) {
        self.page = pagination.offset;
        self.count = pagination.limit;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorItems {
    pub items: Vec<String>,
    pub next: Option<String>,
}

impl PageItems for CursorItems {
    type Item = String;

    fn item_count_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

impl HasNextCursor for CursorItems {
    type Cursor = String;

    fn next_cursor(&self) -> Option<Self::Cursor> {
        self.next.clone()
    }
}

#[derive(Clone)]
pub struct CursorItemsEndpoint {
    pub cursor: Option<String>,
    pub count: u64,
    pub policy: ResolvedPolicy,
    pub pagination: PaginationPlan,
}

impl Default for CursorItemsEndpoint {
    fn default() -> Self {
        Self {
            cursor: Some("start".to_string()),
            count: 2,
            policy: Default::default(),
            pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
                cursor: Some("start".to_string()),
                per_page: 2,
                send_cursor_on_first: false,
                stop_when_cursor_missing: true,
            }),
        }
    }
}

impl Endpoint<TestCx> for CursorItemsEndpoint {
    type Response = CursorItems;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "CursorItems",
            Method::GET,
            "/cursor-items",
            self.policy.clone(),
            Some(self.pagination.clone()),
            decode_cursor_items,
        );
        match &self.pagination {
            PaginationPlan::Cursor { .. } => {
                if let Some(cursor) = &self.cursor {
                    plan.endpoint
                        .policy
                        .query
                        .push(("cursor".to_string(), cursor.clone()));
                }
                plan.endpoint
                    .policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
            PaginationPlan::OffsetLimit { .. } => {
                if let Some(cursor) = &self.cursor {
                    plan.endpoint
                        .policy
                        .query
                        .push(("offset".to_string(), cursor.clone()));
                }
                plan.endpoint
                    .policy
                    .query
                    .push(("limit".to_string(), self.count.to_string()));
            }
            PaginationPlan::Paged { .. } => {
                if let Some(cursor) = &self.cursor {
                    plan.endpoint
                        .policy
                        .query
                        .push(("page".to_string(), cursor.clone()));
                }
                plan.endpoint
                    .policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
        }
        Ok(plan)
    }
}

impl PaginatedEndpoint<TestCx> for CursorItemsEndpoint {
    fn single_object_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::SingleObjectPaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        match &self.pagination {
            PaginationPlan::Cursor { .. } => {
                Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
                    CursorPagination<String>,
                >::new()))
            }
            PaginationPlan::OffsetLimit { .. } | PaginationPlan::Paged { .. } => None,
        }
    }
}

impl PaginateBinding<CursorPagination<String>> for CursorItemsEndpoint {
    fn load_pagination(&self) -> CursorPagination<String> {
        let (send_cursor_on_first, stop_when_cursor_missing) = match &self.pagination {
            PaginationPlan::Cursor {
                send_cursor_on_first,
                stop_when_cursor_missing,
                ..
            } => (*send_cursor_on_first, *stop_when_cursor_missing),
            _ => (false, true),
        };
        CursorPagination {
            cursor: self.cursor.clone(),
            per_page: self.count,
            send_cursor_on_first,
            stop_when_cursor_missing,
        }
    }

    fn store_pagination(&mut self, pagination: &CursorPagination<String>) {
        self.cursor = pagination.cursor.clone();
        self.count = pagination.per_page;
    }
}

pub fn request_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    pagination: Option<PaginationPlan>,
    decode: fn(
        BuiltResponse,
        concord_core::advanced::ErrorContext,
    ) -> Result<Box<dyn std::any::Any + Send>, ApiClientError>,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method,
                idempotent: true,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body: BodyPlan::None,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: concord_core::internal::Format::Text,
                decode,
            },
            pagination,
        },
        args: RequestArgs::default(),
        overrides: RequestOverrides::default(),
    }
}

pub fn decode_string(
    resp: BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let value = std::str::from_utf8(&resp.body)
        .map(str::to_string)
        .map_err(|e| ApiClientError::decode_error(ctx, resp.status, content_type, e))?;
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value,
    }))
}

pub fn decode_items(
    resp: BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let text = std::str::from_utf8(&resp.body)
        .map_err(|e| ApiClientError::decode_error(ctx, resp.status, content_type, e))?;
    let value = if text.is_empty() {
        Vec::new()
    } else {
        text.split(',').map(ToOwned::to_owned).collect()
    };
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value,
    }))
}

pub fn decode_page_only_items(
    resp: BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let text = std::str::from_utf8(&resp.body)
        .map_err(|e| ApiClientError::decode_error(ctx, resp.status, content_type, e))?;
    let items = if text.is_empty() {
        Vec::new()
    } else {
        text.split(',').map(ToOwned::to_owned).collect()
    };
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value: PageOnlyItems { items },
    }))
}

pub fn decode_no_hint_items(
    resp: BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let text = std::str::from_utf8(&resp.body)
        .map_err(|e| ApiClientError::decode_error(ctx, resp.status, content_type, e))?;
    let items = if text.is_empty() {
        Vec::new()
    } else {
        text.split(',').map(ToOwned::to_owned).collect()
    };
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value: NoHintItems { items },
    }))
}

pub fn decode_cursor_items(
    resp: BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let text = std::str::from_utf8(&resp.body)
        .map_err(|e| ApiClientError::decode_error(ctx, resp.status, content_type, e))?;
    let (items_text, next_text) = text.split_once('|').unwrap_or((text, ""));
    let items = if items_text.is_empty() {
        Vec::new()
    } else {
        items_text.split(',').map(ToOwned::to_owned).collect()
    };
    let next = next_text.strip_prefix("next=").map(ToOwned::to_owned);
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value: CursorItems { items, next },
    }))
}

#[derive(Clone)]
pub struct ObservationAuthVars {
    pub token: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub identity: &'static str,
    pub events: Arc<Mutex<Vec<String>>>,
}

impl ObservationAuthVars {
    pub fn bearer(
        token: impl Into<String>,
        identity: &'static str,
        events: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            token: Some(token.into()),
            username: None,
            password: None,
            identity,
            events,
        }
    }

    pub fn basic(
        username: impl Into<String>,
        password: impl Into<String>,
        identity: &'static str,
        events: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            token: None,
            username: Some(username.into()),
            password: Some(password.into()),
            identity,
            events,
        }
    }
}

#[derive(Clone)]
pub struct ObservationAuthCx;

impl ClientContext for ObservationAuthCx {
    type Vars = ();
    type AuthVars = ObservationAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<concord_core::advanced::PreparedAuthCredential, AuthError>,
    > {
        Box::pin(async move {
            let application = match requirement.placement {
                AuthPlacement::Bearer | AuthPlacement::Header(_) | AuthPlacement::Query(_) => {
                    let token = auth.token.as_deref().ok_or_else(|| {
                        AuthError::new(
                            AuthErrorKind::MissingCredential,
                            format!(
                                "missing credential `{}`; acquire or configure it before sending request",
                                requirement.credential.id
                            ),
                        )
                    })?;
                    let material = ApiKey::new(token.to_string());
                    concord_core::advanced::apply_secret_credential(
                        request,
                        requirement,
                        &material,
                    )?
                }
                AuthPlacement::Basic => {
                    let username = auth.username.as_deref().ok_or_else(|| {
                        AuthError::new(
                            AuthErrorKind::MissingCredential,
                            format!(
                                "missing username credential `{}`; acquire or configure it before sending request",
                                requirement.credential.id
                            ),
                        )
                    })?;
                    let password = auth.password.as_deref().ok_or_else(|| {
                        AuthError::new(
                            AuthErrorKind::MissingCredential,
                            format!(
                                "missing password credential `{}`; acquire or configure it before sending request",
                                requirement.credential.id
                            ),
                        )
                    })?;
                    let material = BasicCredential::new(username.to_string(), password.to_string());
                    apply_basic_credential(request, requirement, &material)?
                }
                AuthPlacement::Certificate => {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "observation test context does not use certificate auth",
                    ));
                }
            };
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(concord_core::advanced::PreparedAuthCredential::new(
                applied,
                application,
            ))
        })
    }

    fn handle_auth_response<'a>(
        requirement: &'a AuthRequirement,
        applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        let events = auth.events.clone();
        Box::pin(async move {
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                events.lock().await.push(format!("auth_rejection:{status}"));
                if auth.identity == "refresh" {
                    events.lock().await.push("auth_retry".to_string());
                    return Ok(AuthDecision::RetryAfterRefresh {
                        credential: requirement.credential.clone(),
                        generation: applied.generation,
                        reason: concord_core::advanced::AuthRetryReason::Unauthorized,
                    });
                }
                events.lock().await.push("auth_fail".to_string());
                Ok(AuthDecision::Fail)
            } else {
                Ok(AuthDecision::Continue)
            }
        })
    }
}

impl Endpoint<ObservationAuthCx> for TextEndpoint {
    type Response = String;

    fn plan(
        &self,
        _ctx: &ClientPlanContext<'_, ObservationAuthCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.clone(),
            decode_string,
        ))
    }
}

pub fn auth_policy(placement: AuthPlacement) -> ResolvedPolicy {
    ResolvedPolicy {
        auth: concord_core::advanced::AuthPlan {
            requirements: vec![AuthRequirement {
                credential: concord_core::advanced::CredentialRef {
                    id: concord_core::advanced::CredentialId::new("test", "token"),
                },
                placement,
                usage_id: AuthUsageId::new("test-token"),
                step_id: Some("test"),
                provenance: AuthProvenance::new("test"),
                challenge: Default::default(),
            }],
        },
        ..Default::default()
    }
}

pub fn retry_policy(max_attempts: u32) -> ResolvedPolicy {
    retry_policy_for_statuses(max_attempts, vec![StatusCode::INTERNAL_SERVER_ERROR])
}

pub fn retry_policy_for_statuses(max_attempts: u32, statuses: Vec<StatusCode>) -> ResolvedPolicy {
    ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts,
            methods: vec![Method::GET],
            statuses,
            transport_errors: Vec::new(),
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: true,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    }
}

pub fn retry_policy_for_transport_errors(
    max_attempts: u32,
    transport_errors: Vec<TransportErrorKind>,
) -> ResolvedPolicy {
    ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts,
            methods: vec![Method::GET],
            statuses: Vec::new(),
            transport_errors,
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: true,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    }
}

#[derive(Clone)]
pub struct MockTransport {
    outcomes: Arc<Mutex<VecDeque<MockOutcome>>>,
    events: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<CapturedTransportRequest>>>,
}

#[derive(Clone)]
pub enum MockOutcome {
    Response(MockResponse),
    TransportError(TransportErrorKind),
}

impl From<MockResponse> for MockOutcome {
    fn from(value: MockResponse) -> Self {
        Self::Response(value)
    }
}

#[derive(Clone)]
pub struct MockResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub content_length: Option<u64>,
    pub chunks: Option<Vec<Bytes>>,
    pub read_count: Option<Arc<AtomicUsize>>,
}

impl MockResponse {
    pub fn text(status: StatusCode, body: impl Into<Bytes>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        );
        Self {
            status,
            headers,
            body: body.into(),
            content_length: None,
            chunks: None,
            read_count: None,
        }
    }

    pub fn with_content_length(mut self, content_length: Option<u64>) -> Self {
        self.content_length = content_length;
        self
    }

    pub fn with_chunks(mut self, chunks: Vec<Bytes>) -> Self {
        self.chunks = Some(chunks);
        self
    }

    pub fn with_read_count(mut self, read_count: Arc<AtomicUsize>) -> Self {
        self.read_count = Some(read_count);
        self
    }
}

impl MockTransport {
    pub fn new(events: Arc<Mutex<Vec<String>>>, responses: Vec<MockResponse>) -> Self {
        Self::with_outcomes(events, responses.into_iter().map(Into::into).collect())
    }

    pub fn with_outcomes(events: Arc<Mutex<Vec<String>>>, outcomes: Vec<MockOutcome>) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(outcomes.into())),
            events,
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn sent_count(&self) -> usize {
        self.requests.lock().await.len()
    }

    pub async fn requests(&self) -> Vec<CapturedTransportRequest> {
        let mut requests = self.requests.lock().await;
        std::mem::take(&mut *requests)
    }
}

impl Transport for MockTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let outcomes = self.outcomes.clone();
        let events = self.events.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            let TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout,
                rate_limit,
                transport_auth,
                extensions,
            } = req;
            events.lock().await.push("transport".to_string());
            requests.lock().await.push(CapturedTransportRequest {
                meta: meta.clone(),
                url: url.clone(),
                headers: headers.clone(),
                body,
                timeout,
                rate_limit: rate_limit.clone(),
                transport_auth: transport_auth.clone(),
                extensions: extensions.clone(),
            });
            let outcome = outcomes
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| MockResponse::text(StatusCode::OK, "ok").into());
            let response = match outcome {
                MockOutcome::Response(response) => response,
                MockOutcome::TransportError(kind) => {
                    return Err(TransportError::with_kind(
                        kind,
                        std::io::Error::other("mock transport error"),
                    ));
                }
            };
            Ok(TransportResponse {
                meta,
                url,
                status: response.status,
                headers: response.headers,
                content_length: response.content_length.or_else(|| {
                    response
                        .chunks
                        .is_none()
                        .then_some(response.body.len() as u64)
                }),
                rate_limit,
                body: if let Some(chunks) = response.chunks {
                    Box::new(ChunkBody {
                        chunks: chunks.into(),
                        read_count: response.read_count.clone(),
                    })
                } else {
                    Box::new(StaticBody {
                        body: Some(response.body),
                        read_count: response.read_count.clone(),
                    })
                },
            })
        })
    }
}

#[derive(Clone)]
pub struct GateTransport {
    outcomes: Arc<Mutex<VecDeque<MockOutcome>>>,
    events: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<CapturedTransportRequest>>>,
    arrived: Arc<Notify>,
    release: watch::Sender<bool>,
}

impl GateTransport {
    pub fn new(events: Arc<Mutex<Vec<String>>>, responses: Vec<MockResponse>) -> Self {
        Self::with_outcomes(events, responses.into_iter().map(Into::into).collect())
    }

    pub fn with_outcomes(events: Arc<Mutex<Vec<String>>>, outcomes: Vec<MockOutcome>) -> Self {
        let (release, _) = watch::channel(false);
        Self {
            outcomes: Arc::new(Mutex::new(outcomes.into())),
            events,
            requests: Arc::new(Mutex::new(Vec::new())),
            arrived: Arc::new(Notify::new()),
            release,
        }
    }

    pub async fn sent_count(&self) -> usize {
        self.requests.lock().await.len()
    }

    pub async fn requests(&self) -> Vec<CapturedTransportRequest> {
        let mut requests = self.requests.lock().await;
        std::mem::take(&mut *requests)
    }

    pub async fn wait_for_sends(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notified = self.arrived.notified();
                if self.sent_count().await >= expected {
                    break;
                }
                notified.await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {expected} transport sends"));
    }

    pub fn release_all(&self) {
        let _ = self.release.send(true);
    }
}

impl Transport for GateTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let outcomes = self.outcomes.clone();
        let events = self.events.clone();
        let requests = self.requests.clone();
        let arrived = self.arrived.clone();
        let mut release = self.release.subscribe();
        Box::pin(async move {
            let TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout,
                rate_limit,
                transport_auth,
                extensions,
            } = req;
            events.lock().await.push("transport".to_string());
            requests.lock().await.push(CapturedTransportRequest {
                meta: meta.clone(),
                url: url.clone(),
                headers: headers.clone(),
                body,
                timeout,
                rate_limit: rate_limit.clone(),
                transport_auth: transport_auth.clone(),
                extensions: extensions.clone(),
            });
            let outcome = outcomes
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| MockResponse::text(StatusCode::OK, "ok").into());
            arrived.notify_waiters();

            while !*release.borrow() {
                if release.changed().await.is_err() {
                    break;
                }
            }

            let response = match outcome {
                MockOutcome::Response(response) => response,
                MockOutcome::TransportError(kind) => {
                    return Err(TransportError::with_kind(
                        kind,
                        std::io::Error::other("mock transport error"),
                    ));
                }
            };
            Ok(TransportResponse {
                meta,
                url,
                status: response.status,
                headers: response.headers,
                content_length: response.content_length.or_else(|| {
                    response
                        .chunks
                        .is_none()
                        .then_some(response.body.len() as u64)
                }),
                rate_limit,
                body: if let Some(chunks) = response.chunks {
                    Box::new(ChunkBody {
                        chunks: chunks.into(),
                        read_count: response.read_count.clone(),
                    })
                } else {
                    Box::new(StaticBody {
                        body: Some(response.body),
                        read_count: response.read_count.clone(),
                    })
                },
            })
        })
    }
}

struct StaticBody {
    body: Option<Bytes>,
    read_count: Option<Arc<AtomicUsize>>,
}

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            let chunk = self.body.take();
            if chunk.is_some()
                && let Some(read_count) = &self.read_count
            {
                read_count.fetch_add(1, AtomicOrdering::Relaxed);
            }
            Ok(chunk)
        })
    }
}

struct ChunkBody {
    chunks: VecDeque<Bytes>,
    read_count: Option<Arc<AtomicUsize>>,
}

impl TransportBody for ChunkBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            let chunk = self.chunks.pop_front();
            if chunk.is_some()
                && let Some(read_count) = &self.read_count
            {
                read_count.fetch_add(1, AtomicOrdering::Relaxed);
            }
            Ok(chunk)
        })
    }
}

#[derive(Default)]
pub struct RecordingRateLimiter {
    pub events: Arc<Mutex<Vec<String>>>,
}

impl RecordingRateLimiter {
    pub fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            self.events.lock().await.push("rate_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async move {
            self.events.lock().await.push("rate_response".to_string());
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Default)]
pub struct ObservationRateLimiter {
    pub events: Arc<Mutex<Vec<String>>>,
}

impl ObservationRateLimiter {
    pub fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for ObservationRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            self.events.lock().await.push("rate_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let events = self.events.clone();
        let meta = ctx.meta;
        let status = ctx.status;
        let headers = ctx.headers;
        Box::pin(async move {
            let mut events = events.lock().await;
            events.push(format!(
                "rate_meta:{}:{}:{}:{}:{}:{}",
                meta.endpoint,
                meta.method,
                meta.url,
                meta.url_host.unwrap_or("<none>"),
                meta.attempt,
                meta.page_index
            ));
            events.push(format!("rate_idempotent:{}", meta.idempotent));
            events.push(format!("rate_status:{status}"));
            events.push(format!("rate_headers:{headers:?}"));
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

pub struct RecordingRetryPolicy {
    pub events: Arc<Mutex<Vec<String>>>,
    pub decision: RetryDecision,
    pub max_retries: u32,
}

impl RecordingRetryPolicy {
    pub fn new(events: Arc<Mutex<Vec<String>>>, decision: RetryDecision, max_retries: u32) -> Self {
        Self {
            events,
            decision,
            max_retries,
        }
    }
}

impl RetryPolicy for RecordingRetryPolicy {
    fn max_retries(&self) -> u32 {
        self.max_retries
    }

    fn should_retry_checked(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<RetryDecision, ApiClientError> {
        let events = self.events.clone();
        let endpoint = ctx.endpoint;
        let method = ctx.method.clone();
        let url = ctx.url.to_string();
        let attempt = ctx.attempt;
        let retry_count = ctx.retry_count;
        let page_index = ctx.page_index;
        let idempotent = ctx.idempotent;
        let outcome = format!("{:?}", ctx.outcome);
        let request_headers = format!("{:?}", ctx.request_headers);
        let response_headers = format!("{:?}", ctx.response_headers);
        let decision = self.decision;
        let mut events = events.try_lock().expect("retry events lock");
        events.push(format!(
            "retry_ctx:{endpoint}:{method}:{url}:{attempt}:{retry_count}:{page_index}:{idempotent}"
        ));
        events.push(format!("retry_outcome:{outcome}"));
        events.push(format!("retry_request_headers:{request_headers}"));
        events.push(format!("retry_response_headers:{response_headers}"));
        events.push(format!("retry_decision:{decision:?}"));
        Ok(self.decision)
    }
}

#[derive(Default)]
pub struct ObservationRuntimeHooks {
    pub events: Arc<Mutex<Vec<String>>>,
}

impl ObservationRuntimeHooks {
    pub fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RuntimeHooks for ObservationRuntimeHooks {
    fn pre_send<'a>(
        &'a self,
        _ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push("pre_send".to_string());
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        let meta = ctx.meta;
        let status = ctx.status;
        let headers = ctx.headers;
        Box::pin(async move {
            let mut events = events.lock().await;
            events.push(format!(
                "hook_meta:{}:{}:{}:{}:{}",
                meta.endpoint, meta.method, meta.url, meta.attempt, meta.page_index
            ));
            events.push(format!("hook_idempotent:{}", meta.idempotent));
            events.push(format!("hook_status:{status}"));
            events.push(format!("hook_headers:{headers:?}"));
        })
    }

    fn transport_error<'a>(
        &'a self,
        ctx: TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .await
                .push(format!("transport_error:{:?}:{ctx:?}", ctx.error.kind()));
        })
    }
}

#[derive(Default)]
pub struct RecordingRuntimeHooks {
    pub events: Arc<Mutex<Vec<String>>>,
}

impl RecordingRuntimeHooks {
    pub fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RuntimeHooks for RecordingRuntimeHooks {
    fn pre_send<'a>(
        &'a self,
        _ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push("pre_send".to_string());
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        _ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push("classify_response".to_string());
        })
    }

    fn transport_error<'a>(
        &'a self,
        _ctx: TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push("transport_error".to_string());
        })
    }
}

pub fn client(auth: TestAuthVars, transport: MockTransport) -> ApiClient<TestCx, MockTransport> {
    ApiClient::with_transport((), auth, transport)
}

pub fn configure_runtime<Cx: ClientContext, T: Transport>(
    client: &mut ApiClient<Cx, T>,
    limiter: Option<Arc<dyn RateLimiter>>,
) {
    client.configure(|cfg| {
        cfg.debug(concord_core::prelude::DebugLevel::V);
        cfg.pagination_detect_loops(true);
        if let Some(limiter) = limiter {
            cfg.rate_limiter(limiter);
        }
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessTimeout {
    pub label: &'static str,
}

pub async fn wait_bounded<T>(label: &'static str, fut: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(2), fut)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {label}"))
}

pub async fn assert_still_pending(label: &'static str, fut: impl Future<Output = ()>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(50), fut)
            .await
            .is_err(),
        "{label} completed unexpectedly"
    );
}

#[derive(Clone, Default)]
pub struct PhaseGate {
    inner: Arc<PhaseGateInner>,
}

#[derive(Default)]
struct PhaseGateInner {
    phases: StdMutex<HashMap<&'static str, PhaseState>>,
    events: Mutex<Vec<String>>,
    notify: Notify,
}

#[derive(Default)]
struct PhaseState {
    entered: usize,
    waiting: usize,
    released_pending: usize,
    blocked: bool,
    waiters: VecDeque<Arc<PhaseWaiter>>,
}

struct PhaseWaiter {
    notify: Notify,
    released: AtomicBool,
}

impl PhaseGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn block(&self, phase: &'static str) {
        let mut phases = self
            .inner
            .phases
            .lock()
            .expect("phase mutex should not be poisoned");
        let state = phases.entry(phase).or_insert_with(|| PhaseState {
            ..PhaseState::default()
        });
        state.blocked = true;
    }

    pub async fn enter(&self, phase: &'static str) {
        let maybe_waiter = {
            let mut phases = self
                .inner
                .phases
                .lock()
                .expect("phase mutex should not be poisoned");
            let state = phases.entry(phase).or_insert_with(|| PhaseState {
                ..PhaseState::default()
            });
            state.entered += 1;
            if state.blocked {
                state.waiting += 1;
                let waiter = Arc::new(PhaseWaiter {
                    notify: Notify::new(),
                    released: AtomicBool::new(false),
                });
                state.waiters.push_back(waiter.clone());
                Some(waiter)
            } else {
                None
            }
        };
        self.inner.events.lock().await.push(phase.to_string());
        self.inner.notify.notify_waiters();
        if let Some(waiter) = maybe_waiter {
            let token = PhaseWaiterToken {
                gate: self.clone(),
                phase,
                waiter,
                completed: false,
            };
            token.waiter.notify.notified().await;
            let mut token = token;
            token.completed = true;
            token.finish();
        }
    }

    pub async fn wait_for(&self, phase: &'static str, count: usize) {
        self.try_wait_for(phase, count)
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for phase {phase} count {count}"));
    }

    pub async fn try_wait_for(
        &self,
        phase: &'static str,
        count: usize,
    ) -> Result<(), HarnessTimeout> {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let notified = self.inner.notify.notified();
                let entered = self
                    .inner
                    .phases
                    .lock()
                    .expect("phase mutex should not be poisoned")
                    .get(phase)
                    .map(|state| state.entered)
                    .unwrap_or(0);
                if entered >= count {
                    break;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| HarnessTimeout { label: phase })
    }

    pub async fn release_one(&self, phase: &'static str) {
        let waiter = {
            let mut phases = self
                .inner
                .phases
                .lock()
                .expect("phase mutex should not be poisoned");
            phases.get_mut(phase).and_then(|state| {
                while let Some(waiter) = state.waiters.pop_front() {
                    if Arc::strong_count(&waiter) > 1 && state.waiting > state.released_pending {
                        state.released_pending += 1;
                        waiter.released.store(true, AtomicOrdering::SeqCst);
                        return Some(waiter);
                    }
                }
                None
            })
        };
        if let Some(waiter) = waiter {
            waiter.notify.notify_one();
        }
    }

    pub async fn release_all(&self, phase: &'static str) {
        let waiters = {
            let mut phases = self
                .inner
                .phases
                .lock()
                .expect("phase mutex should not be poisoned");
            phases
                .get_mut(phase)
                .map(|state| {
                    let mut waiters = Vec::new();
                    while let Some(waiter) = state.waiters.pop_front() {
                        if Arc::strong_count(&waiter) > 1 && state.waiting > state.released_pending
                        {
                            state.released_pending += 1;
                            waiter.released.store(true, AtomicOrdering::SeqCst);
                            waiters.push(waiter);
                        }
                    }
                    waiters
                })
                .unwrap_or_default()
        };
        for waiter in waiters {
            waiter.notify.notify_one();
        }
    }

    pub async fn events(&self) -> Vec<String> {
        self.inner.events.lock().await.clone()
    }
}

struct PhaseWaiterToken {
    gate: PhaseGate,
    phase: &'static str,
    waiter: Arc<PhaseWaiter>,
    completed: bool,
}

impl PhaseWaiterToken {
    fn finish(&mut self) {
        if self.completed {
            let mut phases = self
                .gate
                .inner
                .phases
                .lock()
                .expect("phase mutex should not be poisoned");
            if let Some(state) = phases.get_mut(self.phase) {
                state.waiting = state.waiting.saturating_sub(1);
                if self.waiter.released.load(AtomicOrdering::SeqCst) {
                    state.released_pending = state.released_pending.saturating_sub(1);
                }
            }
        } else {
            let mut phases = self
                .gate
                .inner
                .phases
                .lock()
                .expect("phase mutex should not be poisoned");
            if let Some(state) = phases.get_mut(self.phase) {
                state.waiting = state.waiting.saturating_sub(1);
                if self.waiter.released.load(AtomicOrdering::SeqCst) {
                    state.released_pending = state.released_pending.saturating_sub(1);
                }
                state
                    .waiters
                    .retain(|queued| !Arc::ptr_eq(queued, &self.waiter));
            }
        }
    }
}

impl Drop for PhaseWaiterToken {
    fn drop(&mut self) {
        if !self.completed {
            self.finish();
        }
    }
}

#[derive(Clone)]
pub struct DropProbe {
    inner: Arc<DropProbeInner>,
}

struct DropProbeInner {
    label: &'static str,
    count: AtomicUsize,
    notify: Notify,
    events: Arc<Mutex<Vec<String>>>,
}

impl DropProbe {
    pub fn new(label: &'static str, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            inner: Arc::new(DropProbeInner {
                label,
                count: AtomicUsize::new(0),
                notify: Notify::new(),
                events,
            }),
        }
    }

    pub fn token(&self) -> DropProbeToken {
        DropProbeToken {
            inner: self.inner.clone(),
        }
    }

    pub fn count(&self) -> usize {
        self.inner.count.load(AtomicOrdering::SeqCst)
    }

    pub async fn wait_for(&self, expected: usize) {
        wait_bounded("drop probe", async {
            loop {
                let notified = self.inner.notify.notified();
                if self.count() >= expected {
                    break;
                }
                notified.await;
            }
        })
        .await;
    }
}

pub struct DropProbeToken {
    inner: Arc<DropProbeInner>,
}

impl Drop for DropProbeToken {
    fn drop(&mut self) {
        self.inner.count.fetch_add(1, AtomicOrdering::SeqCst);
        if let Ok(mut events) = self.inner.events.try_lock() {
            events.push(format!("drop:{}", self.inner.label));
        }
        self.inner.notify.notify_waiters();
    }
}

#[derive(Clone)]
pub struct GateableTransport {
    gate: PhaseGate,
    outcomes: Arc<Mutex<VecDeque<MockOutcome>>>,
    events: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<CapturedTransportRequest>>>,
    drop_probe: Option<DropProbe>,
}

impl GateableTransport {
    pub fn new(
        gate: PhaseGate,
        events: Arc<Mutex<Vec<String>>>,
        responses: Vec<MockResponse>,
    ) -> Self {
        Self::with_outcomes(
            gate,
            events,
            responses.into_iter().map(Into::into).collect(),
        )
    }

    pub fn with_outcomes(
        gate: PhaseGate,
        events: Arc<Mutex<Vec<String>>>,
        outcomes: Vec<MockOutcome>,
    ) -> Self {
        Self {
            gate,
            outcomes: Arc::new(Mutex::new(outcomes.into())),
            events,
            requests: Arc::new(Mutex::new(Vec::new())),
            drop_probe: None,
        }
    }

    pub fn with_drop_probe(mut self, probe: DropProbe) -> Self {
        self.drop_probe = Some(probe);
        self
    }

    pub async fn sent_count(&self) -> usize {
        self.requests.lock().await.len()
    }
}

impl Transport for GateableTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let gate = self.gate.clone();
        let outcomes = self.outcomes.clone();
        let events = self.events.clone();
        let requests = self.requests.clone();
        let drop_token = self.drop_probe.as_ref().map(DropProbe::token);
        Box::pin(async move {
            let TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout,
                rate_limit,
                transport_auth,
                extensions,
            } = req;
            let _drop_token = drop_token;
            events.lock().await.push("transport_send_start".to_string());
            requests.lock().await.push(CapturedTransportRequest {
                meta: meta.clone(),
                url: url.clone(),
                headers: headers.clone(),
                body,
                timeout,
                rate_limit: rate_limit.clone(),
                transport_auth: transport_auth.clone(),
                extensions: extensions.clone(),
            });
            gate.enter("transport_send").await;
            let outcome = outcomes
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| MockResponse::text(StatusCode::OK, "ok").into());
            let response = match outcome {
                MockOutcome::Response(response) => response,
                MockOutcome::TransportError(kind) => {
                    return Err(TransportError::with_kind(
                        kind,
                        std::io::Error::other("gateable transport error"),
                    ));
                }
            };
            Ok(TransportResponse {
                meta,
                url,
                status: response.status,
                headers: response.headers,
                content_length: response.content_length.or_else(|| {
                    response
                        .chunks
                        .is_none()
                        .then_some(response.body.len() as u64)
                }),
                rate_limit,
                body: if let Some(chunks) = response.chunks {
                    Box::new(ChunkBody {
                        chunks: chunks.into(),
                        read_count: response.read_count.clone(),
                    })
                } else {
                    Box::new(StaticBody {
                        body: Some(response.body),
                        read_count: response.read_count.clone(),
                    })
                },
            })
        })
    }
}

#[derive(Clone)]
pub struct GateableBodyTransport {
    gate: PhaseGate,
    events: Arc<Mutex<Vec<String>>>,
    chunks: Arc<Mutex<VecDeque<VecDeque<Bytes>>>>,
    read_count: Arc<AtomicUsize>,
    drop_probe: Option<DropProbe>,
}

impl GateableBodyTransport {
    pub fn new(gate: PhaseGate, events: Arc<Mutex<Vec<String>>>, chunks: Vec<Bytes>) -> Self {
        Self {
            gate,
            events,
            chunks: Arc::new(Mutex::new(vec![chunks.into()].into())),
            read_count: Arc::new(AtomicUsize::new(0)),
            drop_probe: None,
        }
    }

    pub fn with_drop_probe(mut self, probe: DropProbe) -> Self {
        self.drop_probe = Some(probe);
        self
    }

    pub fn read_count(&self) -> usize {
        self.read_count.load(AtomicOrdering::SeqCst)
    }
}

impl Transport for GateableBodyTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let gate = self.gate.clone();
        let events = self.events.clone();
        let chunks = self.chunks.clone();
        let read_count = self.read_count.clone();
        let drop_probe = self.drop_probe.clone();
        Box::pin(async move {
            events.lock().await.push("transport_send_start".to_string());
            let body_chunks = chunks
                .lock()
                .await
                .pop_front()
                .expect("gateable body response should be available");
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: StatusCode::OK,
                headers: {
                    let mut h = HeaderMap::new();
                    h.insert(
                        http::header::CONTENT_TYPE,
                        HeaderValue::from_static("text/plain"),
                    );
                    h
                },
                content_length: None,
                rate_limit: req.rate_limit,
                body: Box::new(GateableBody {
                    gate,
                    chunks: body_chunks,
                    read_count,
                    _drop_token: drop_probe.map(|probe| probe.token()),
                }),
            })
        })
    }
}

struct GateableBody {
    gate: PhaseGate,
    chunks: VecDeque<Bytes>,
    read_count: Arc<AtomicUsize>,
    _drop_token: Option<DropProbeToken>,
}

impl TransportBody for GateableBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            if self.chunks.front().is_some() {
                self.gate.enter("body_chunk").await;
            }
            let chunk = self.chunks.pop_front();
            if chunk.is_some() {
                self.read_count.fetch_add(1, AtomicOrdering::SeqCst);
            }
            Ok(chunk)
        })
    }
}

#[derive(Default)]
pub struct CountingRateLimiter {
    pub events: Arc<Mutex<Vec<String>>>,
    pub acquire_started: AtomicUsize,
    pub acquire_completed: AtomicUsize,
    pub permit_created: AtomicUsize,
    pub response_lifecycle_completed: AtomicUsize,
    pub response_observed: AtomicUsize,
    gate: PhaseGate,
    fail_acquire: bool,
    drop_probe: Option<DropProbe>,
}

impl CountingRateLimiter {
    pub fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            events,
            gate: PhaseGate::new(),
            fail_acquire: false,
            drop_probe: None,
            ..Self::default()
        }
    }

    pub fn with_gate(mut self, gate: PhaseGate) -> Self {
        self.gate = gate;
        self
    }

    pub fn failing(mut self) -> Self {
        self.fail_acquire = true;
        self
    }

    pub fn with_drop_probe(mut self, probe: DropProbe) -> Self {
        self.drop_probe = Some(probe);
        self
    }
}

impl RateLimiter for CountingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            let drop_token = self.drop_probe.as_ref().map(DropProbe::token);
            let _drop_token = drop_token;
            self.acquire_started.fetch_add(1, AtomicOrdering::SeqCst);
            self.events
                .lock()
                .await
                .push("rate_acquire_started".to_string());
            self.gate.enter("rate_acquire").await;
            if self.fail_acquire {
                return Err(ApiClientError::RuntimeState {
                    ctx: concord_core::advanced::ErrorContext {
                        endpoint: "Text",
                        method: Method::GET,
                    },
                    subsystem: "rate-limit",
                    msg: "counting limiter acquire failed",
                });
            }
            self.acquire_completed.fetch_add(1, AtomicOrdering::SeqCst);
            self.permit_created.fetch_add(1, AtomicOrdering::SeqCst);
            self.events
                .lock()
                .await
                .push("rate_permit_created".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async move {
            let drop_token = self.drop_probe.as_ref().map(DropProbe::token);
            let _drop_token = drop_token;
            self.response_observed.fetch_add(1, AtomicOrdering::SeqCst);
            self.response_lifecycle_completed
                .fetch_add(1, AtomicOrdering::SeqCst);
            self.events
                .lock()
                .await
                .push(format!("rate_response:{}", ctx.status));
            self.events
                .lock()
                .await
                .push("rate_lifecycle_completed".to_string());
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Clone)]
pub struct GateableHooks {
    gate: PhaseGate,
    events: Arc<Mutex<Vec<String>>>,
    block_pre_send: bool,
    drop_probe: Option<DropProbe>,
}

impl GateableHooks {
    pub fn new(gate: PhaseGate, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            gate,
            events,
            block_pre_send: true,
            drop_probe: None,
        }
    }

    pub fn with_drop_probe(mut self, probe: DropProbe) -> Self {
        self.drop_probe = Some(probe);
        self
    }
}

impl RuntimeHooks for GateableHooks {
    fn pre_send<'a>(
        &'a self,
        _ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            let drop_token = self.drop_probe.as_ref().map(DropProbe::token);
            let _drop_token = drop_token;
            self.events
                .lock()
                .await
                .push("hook_pre_send_started".to_string());
            if self.block_pre_send {
                self.gate.enter("hook_pre_send").await;
            }
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let drop_token = self.drop_probe.as_ref().map(DropProbe::token);
            let _drop_token = drop_token;
            self.events
                .lock()
                .await
                .push(format!("hook_post_response:{}", ctx.status));
            self.gate.enter("hook_post_response").await;
        })
    }

    fn transport_error<'a>(
        &'a self,
        _ctx: TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let drop_token = self.drop_probe.as_ref().map(DropProbe::token);
            let _drop_token = drop_token;
            self.events
                .lock()
                .await
                .push("hook_transport_error".to_string());
            self.gate.enter("hook_transport_error").await;
        })
    }
}

#[derive(Default)]
pub struct SafeRecordingDebugSink {
    pub events: Arc<Mutex<Vec<String>>>,
}

impl SafeRecordingDebugSink {
    pub fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl concord_core::advanced::DebugSink for SafeRecordingDebugSink {
    fn request_start(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        method: &Method,
        url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        self.events
            .try_lock()
            .expect("debug events lock")
            .push(format!(
                "debug_request:{dbg}:{method}:{url}:{endpoint}:{page_index}"
            ));
    }

    fn request_headers(&self, _dbg: concord_core::prelude::DebugLevel, headers: &HeaderMap) {
        self.events
            .try_lock()
            .expect("debug events lock")
            .push(format!("debug_request_headers:{headers:?}"));
    }

    fn response_status(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        status: StatusCode,
        url: &str,
        ok: bool,
    ) {
        self.events
            .try_lock()
            .expect("debug events lock")
            .push(format!("debug_response:{dbg}:{status}:{url}:{ok}"));
    }

    fn response_headers(&self, _dbg: concord_core::prelude::DebugLevel, headers: &HeaderMap) {
        self.events
            .try_lock()
            .expect("debug events lock")
            .push(format!("debug_response_headers:{headers:?}"));
    }
}

pub fn rate_limit_policy() -> ResolvedPolicy {
    let mut policy = ResolvedPolicy::default();
    let mut plan = concord_core::advanced::RateLimitPlan::new();
    plan.push_bucket(
        concord_core::advanced::RateLimitBucketUse::new(
            "async-harness",
            "endpoint",
            concord_core::advanced::RateLimitKey::new(vec![
                concord_core::advanced::RateLimitKeyPart::endpoint(),
            ]),
        )
        .with_window(concord_core::advanced::RateLimitWindow::new(
            std::num::NonZeroU32::new(10).expect("non-zero"),
            Duration::from_secs(1),
        )),
    );
    policy.rate_limit = plan;
    policy
}

pub async fn assert_events_do_not_contain(events: &Arc<Mutex<Vec<String>>>, sentinels: &[&str]) {
    let rendered = events.lock().await.join("\n");
    for sentinel in sentinels {
        assert!(
            !rendered.contains(sentinel),
            "event log leaked sentinel {sentinel}: {rendered}"
        );
    }
}
