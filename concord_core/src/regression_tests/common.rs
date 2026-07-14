#![allow(dead_code, unused_imports)]

use crate::regression_tests::test_api::{
    AuthPlacement, AuthProvenance, AuthRequirement, AuthUsageId, BufferedResponse, EndpointMeta,
    EndpointPlan, PaginationMarker, PreparedBody, RegressionEndpoint, RegressionPaginatedEndpoint,
    RegressionPlanContext, RegressionReusableEndpoint, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute, ResponseEntity, ResponsePlan,
};
use bytes::Bytes;
use concord_core::advanced::{
    AuthError, AuthErrorKind, AuthFuture, CodecError, CredentialContext, CredentialId,
    CredentialProvider, CredentialProviderState, CursorPagination, DecodeContext, InvalidateReason,
    OffsetLimitPagination, PagedPagination, PaginateBinding, PaginationRuntimeAdapter,
    PostResponseHookContext, PreSendHookContext, RateLimitContext, RateLimitFuture,
    RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext, RateLimiter,
    RequestErrorHookContext, ResponseCodec, RuntimeHooks, TextContentType,
};
use concord_core::prelude::{
    ApiClient, ApiClientError, ApiKey, BasicCredential, ClientContext, RequestExecutionMeta,
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
use tokio::sync::{Mutex, Notify};

#[path = "../../../concord_test_support/src/mock.rs"]
pub(super) mod native_mock;
use native_mock::{MockHandle as NativeMockHandle, MockServer};
pub use native_mock::{MockReply as NativeMockReply, ReplyGate as NativeReplyGate};

pub struct CapturedWireRequest {
    pub meta: CapturedRequestExecutionMeta,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: CapturedBody,
    pub timeout: Option<Duration>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedRequestExecutionMeta {
    pub endpoint: Option<String>,
    pub method: Method,
    pub page_index: Option<u32>,
}

pub enum CapturedBody {
    Empty,
    Bytes(Bytes),
}

impl CapturedBody {
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn is_bytes(&self) -> bool {
        matches!(self, Self::Bytes(_))
    }
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

impl fmt::Debug for CapturedWireRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body = match &self.body {
            CapturedBody::Empty => "<empty>".to_string(),
            CapturedBody::Bytes(bytes) => format!("<{} bytes>", bytes.len()),
        };
        f.debug_struct("CapturedWireRequest")
            .field("meta", &self.meta)
            .field("url", &"<redacted>")
            .field(
                "headers",
                &concord_core::advanced::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &body)
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct CapturedWireRequestSnapshot {
    pub meta: RequestExecutionMeta,
    pub url: url::Url,
    pub headers: HeaderMap,
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

#[derive(Clone)]
pub struct TestCredentialProvider;

impl<Cx> CredentialProvider<Cx> for TestCredentialProvider
where
    Cx: ClientContext<AuthVars = TestAuthVars>,
{
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "token")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            ctx.auth
                .token
                .as_ref()
                .map(|token| ApiKey::new(token.clone()))
                .ok_or_else(|| {
                    AuthError::new(
                        AuthErrorKind::MissingCredential,
                        "missing credential `test/token`; configure it before sending request",
                    )
                })
        })
    }

    fn invalidate<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        _current: Option<&'a Self::Credential>,
        _reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            if ctx.auth.identity == "refresh-error" {
                return Err(AuthError::new(
                    AuthErrorKind::ProviderRejected,
                    "auth refresh failed",
                ));
            }
            Ok(())
        })
    }
}

pub struct TestAuthState<Cx: ClientContext<AuthVars = TestAuthVars>> {
    slot: Arc<CredentialProviderState<Cx, TestCredentialProvider>>,
    refresh_on_challenge: bool,
}

impl<Cx: ClientContext<AuthVars = TestAuthVars>> Clone for TestAuthState<Cx> {
    fn clone(&self) -> Self {
        Self {
            slot: self.slot.clone(),
            refresh_on_challenge: self.refresh_on_challenge,
        }
    }
}

impl<Cx: ClientContext<AuthVars = TestAuthVars>> TestAuthState<Cx> {
    pub fn new(auth: &TestAuthVars) -> Self {
        Self {
            slot: Arc::new(CredentialProviderState::new(TestCredentialProvider)),
            refresh_on_challenge: matches!(auth.identity, "refresh" | "refresh-error"),
        }
    }

    pub fn binding<'a>(
        &'a self,
        credential: &CredentialId,
    ) -> Option<concord_core::advanced::AuthProviderBinding<'a, Cx>> {
        (credential == &CredentialId::new("test", "token")).then(|| {
            self.slot.secret_binding(
                concord_core::advanced::AuthPreparationMode::RequestLocal,
                if self.refresh_on_challenge {
                    concord_core::advanced::AuthChallengeMode::Refresh
                } else {
                    concord_core::advanced::AuthChallengeMode::InvalidateOnly
                },
            )
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaginationVariant {
    OffsetLimit {
        offset: u64,
        limit: u64,
    },
    Paged {
        page: u64,
        per_page: u64,
    },
    Cursor {
        cursor: Option<String>,
        per_page: u64,
        send_cursor_on_first: bool,
        stop_when_cursor_missing: bool,
    },
}

impl PaginationVariant {
    pub fn cursor<Page>(value: CursorPagination) -> Self {
        let _ = std::marker::PhantomData::<Page>;
        Self::Cursor {
            cursor: value.cursor,
            per_page: value.per_page,
            send_cursor_on_first: value.send_cursor_on_first,
            stop_when_cursor_missing: value.stop_when_cursor_missing,
        }
    }
}

impl ClientContext for TestCx {
    type Vars = ();
    type AuthVars = TestAuthVars;
    type AuthState = TestAuthState<Self>;
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        TestAuthState::new(auth)
    }

    fn auth_provider_binding<'a>(
        credential: &CredentialId,
        auth_state: &'a Self::AuthState,
    ) -> Option<concord_core::advanced::AuthProviderBinding<'a, Self>> {
        auth_state.binding(credential)
    }
}

#[derive(Clone)]
pub struct TextEndpoint {
    pub name: &'static str,
    pub method: Method,
    pub path: &'static str,
    pub policy: ResolvedPolicy,
    pub pagination: Option<PaginationVariant>,
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

impl RegressionEndpoint<TestCx> for TextEndpoint {
    type Response = String;

    fn execute<'a>(
        client: &'a ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        execute_buffered::<_, concord_core::prelude::Text<String>>(client, plan)
    }
}

buffered_endpoint_response_terminal!(TextEndpoint, TestCx, concord_core::prelude::Text<String>);

impl RegressionReusableEndpoint<TestCx> for TextEndpoint {
    fn plan(
        &self,
        _ctx: &RegressionPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.as_ref().map(|_| PaginationMarker),
        ))
    }
}

#[derive(Clone)]
pub struct ItemsEndpoint {
    pub start: u64,
    pub count: u64,
    pub policy: ResolvedPolicy,
    pub pagination: PaginationVariant,
}

impl Default for ItemsEndpoint {
    fn default() -> Self {
        Self {
            start: 0,
            count: 2,
            policy: Default::default(),
            pagination: PaginationVariant::OffsetLimit {
                offset: 0,
                limit: 2,
            },
        }
    }
}

impl RegressionEndpoint<TestCx> for ItemsEndpoint {
    type Response = Vec<String>;

    fn execute<'a>(
        client: &'a ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        execute_buffered::<_, CommaSeparatedItems>(client, plan)
    }
}

impl RegressionReusableEndpoint<TestCx> for ItemsEndpoint {
    fn plan(
        &self,
        _ctx: &RegressionPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "Items",
            Method::GET,
            "/items",
            self.policy.clone(),
            Some(PaginationMarker),
        );
        match &self.pagination {
            PaginationVariant::OffsetLimit { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("offset".to_string(), self.start.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("limit".to_string(), self.count.to_string()));
            }
            PaginationVariant::Paged { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("page".to_string(), self.start.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
            PaginationVariant::Cursor { .. } => {
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

impl RegressionPaginatedEndpoint<TestCx> for ItemsEndpoint {
    type Pagination = OffsetLimitPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        match &self.pagination {
            PaginationVariant::OffsetLimit { .. } => Some(Box::new(PaginationRuntimeAdapter::<
                OffsetLimitPagination,
            >::new())),
            PaginationVariant::Paged { .. } => {
                Some(Box::new(PaginationRuntimeAdapter::<PagedPagination>::new()))
            }
            PaginationVariant::Cursor { .. } => None,
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

    fn item_count(&self) -> usize {
        self.items.len()
    }

    fn into_items(self) -> Vec<Self::Item> {
        self.items
    }
}

#[derive(Clone)]
pub struct PageOnlyItemsEndpoint {
    pub page: u64,
    pub count: u64,
    pub policy: ResolvedPolicy,
    pub pagination: PaginationVariant,
}

impl Default for PageOnlyItemsEndpoint {
    fn default() -> Self {
        Self {
            page: 0,
            count: 2,
            policy: Default::default(),
            pagination: PaginationVariant::OffsetLimit {
                offset: 0,
                limit: 2,
            },
        }
    }
}

impl RegressionEndpoint<TestCx> for PageOnlyItemsEndpoint {
    type Response = PageOnlyItems;

    fn execute<'a>(
        client: &'a ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        execute_buffered::<_, PageOnlyItemsCodec>(client, plan)
    }
}

impl RegressionReusableEndpoint<TestCx> for PageOnlyItemsEndpoint {
    fn plan(
        &self,
        _ctx: &RegressionPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "PageOnlyItems",
            Method::GET,
            "/page-only-items",
            self.policy.clone(),
            Some(PaginationMarker),
        );
        match &self.pagination {
            PaginationVariant::Paged { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("page".to_string(), self.page.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("per_page".to_string(), self.count.to_string()));
            }
            PaginationVariant::OffsetLimit { .. } => {
                plan.endpoint
                    .policy
                    .query
                    .push(("offset".to_string(), self.page.to_string()));
                plan.endpoint
                    .policy
                    .query
                    .push(("limit".to_string(), self.count.to_string()));
            }
            PaginationVariant::Cursor { .. } => {
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

impl RegressionPaginatedEndpoint<TestCx> for PageOnlyItemsEndpoint {
    type Pagination = PagedPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        match &self.pagination {
            PaginationVariant::Paged { .. } => {
                Some(Box::new(PaginationRuntimeAdapter::<PagedPagination>::new()))
            }
            PaginationVariant::OffsetLimit { .. } => Some(Box::new(PaginationRuntimeAdapter::<
                OffsetLimitPagination,
            >::new())),
            PaginationVariant::Cursor { .. } => None,
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

    fn item_count(&self) -> usize {
        self.items.len()
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

pub struct CommaSeparatedItems;

impl ResponseCodec for CommaSeparatedItems {
    type Value = Vec<String>;
    type Content = TextContentType;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        let text = std::str::from_utf8(&bytes)
            .map_err(|err| CodecError::with_source("text decode failed", err))?;
        Ok(if text.is_empty() {
            Vec::new()
        } else {
            text.split(',').map(ToOwned::to_owned).collect()
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InvalidJsonResponse;

impl ResponseCodec for InvalidJsonResponse {
    type Value = String;
    type Content = TextContentType;

    fn decode(_bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        Err(CodecError::new("invalid JSON payload"))
    }
}

pub struct PageOnlyItemsCodec;

impl ResponseCodec for PageOnlyItemsCodec {
    type Value = PageOnlyItems;
    type Content = TextContentType;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        let text = std::str::from_utf8(&bytes)
            .map_err(|err| CodecError::with_source("text decode failed", err))?;
        Ok(PageOnlyItems {
            items: if text.is_empty() {
                Vec::new()
            } else {
                text.split(',').map(ToOwned::to_owned).collect()
            },
        })
    }
}

pub struct CursorItemsCodec;

impl ResponseCodec for CursorItemsCodec {
    type Value = CursorItems;
    type Content = TextContentType;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        let text = std::str::from_utf8(&bytes)
            .map_err(|err| CodecError::with_source("text decode failed", err))?;
        let (items_text, next_text) = text.split_once('|').unwrap_or((text, ""));
        let items = if items_text.is_empty() {
            Vec::new()
        } else {
            items_text.split(',').map(ToOwned::to_owned).collect()
        };
        let next = next_text.strip_prefix("next=").map(ToOwned::to_owned);
        Ok(CursorItems { items, next })
    }
}

#[derive(Clone)]
pub struct CursorItemsEndpoint {
    pub cursor: Option<String>,
    pub count: u64,
    pub policy: ResolvedPolicy,
    pub pagination: PaginationVariant,
}

impl Default for CursorItemsEndpoint {
    fn default() -> Self {
        Self {
            cursor: Some("start".to_string()),
            count: 2,
            policy: Default::default(),
            pagination: PaginationVariant::Cursor {
                cursor: Some("start".to_string()),
                per_page: 2,
                send_cursor_on_first: false,
                stop_when_cursor_missing: true,
            },
        }
    }
}

impl RegressionEndpoint<TestCx> for CursorItemsEndpoint {
    type Response = CursorItems;

    fn execute<'a>(
        client: &'a ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        execute_buffered::<_, CursorItemsCodec>(client, plan)
    }
}

impl RegressionReusableEndpoint<TestCx> for CursorItemsEndpoint {
    fn plan(
        &self,
        _ctx: &RegressionPlanContext<'_, TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "CursorItems",
            Method::GET,
            "/cursor-items",
            self.policy.clone(),
            Some(PaginationMarker),
        );
        match &self.pagination {
            PaginationVariant::Cursor { .. } => {
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
            PaginationVariant::OffsetLimit { .. } => {
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
            PaginationVariant::Paged { .. } => {
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

impl RegressionPaginatedEndpoint<TestCx> for CursorItemsEndpoint {
    type Pagination = CursorPagination<String>;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        match &self.pagination {
            PaginationVariant::Cursor { .. } => Some(Box::new(PaginationRuntimeAdapter::<
                CursorPagination<String>,
            >::new())),
            PaginationVariant::OffsetLimit { .. } | PaginationVariant::Paged { .. } => None,
        }
    }
}

impl PaginateBinding<CursorPagination<String>> for CursorItemsEndpoint {
    fn load_pagination(&self) -> CursorPagination<String> {
        let (send_cursor_on_first, stop_when_cursor_missing) = match &self.pagination {
            PaginationVariant::Cursor {
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
    pagination: Option<PaginationMarker>,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method,
                idempotent: true,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTP, "example.com", path),
            policy,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: crate::regression_tests::test_api::Format::Text,
            },
            pagination,
        },
        body: PreparedBody::empty(),
        overrides: RequestOverrides::default(),
    }
}

pub fn execute_buffered<'a, Cx, C>(
    client: &'a ApiClient<Cx>,
    plan: RequestPlan,
) -> Pin<Box<dyn Future<Output = Result<C::Value, ApiClientError>> + Send + 'a>>
where
    Cx: ClientContext,
    C: ResponseCodec,
{
    BufferedResponse::<C>::execute(client, plan)
}

macro_rules! buffered_endpoint_execute {
    ($cx:ty, $codec:ty) => {
        fn execute<'a>(
            client: &'a concord_core::prelude::ApiClient<$cx>,
            plan: crate::regression_tests::test_api::RequestPlan,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Self::Response, concord_core::prelude::ApiClientError>,
                    > + Send
                    + 'a,
            >,
        > {
            $crate::regression_tests::common::execute_buffered::<_, $codec>(client, plan)
        }
    };
}

pub(crate) use buffered_endpoint_execute;

macro_rules! buffered_endpoint_response_terminal {
    ($endpoint:ty, $cx:ty, $codec:ty) => {
        impl $crate::regression_tests::test_api::RegressionResponseTerminal<$cx> for $endpoint {
            fn execute_response<'a>(
                client: &'a concord_core::prelude::ApiClient<$cx>,
                plan: crate::regression_tests::test_api::RequestPlan,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<
                                ::concord_core::prelude::DecodedResponse<Self::Response>,
                                concord_core::prelude::ApiClientError,
                            >,
                        > + Send
                        + 'a,
                >,
            >
            {
                <crate::regression_tests::test_api::BufferedResponse<$codec> as $crate::regression_tests::test_api::ResponseEntityWithMeta>::execute_with_meta(
                    client,
                    plan,
                )
            }
        }
    };
}

pub(crate) use buffered_endpoint_response_terminal;

#[derive(Clone)]
pub struct ObservationAuthVars {
    pub token: Option<String>,
    pub replacement_token: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub identity: &'static str,
    pub events: Arc<Mutex<Vec<String>>>,
    pub binding_resolutions: Arc<AtomicUsize>,
}

impl ObservationAuthVars {
    pub fn bearer(
        token: impl Into<String>,
        identity: &'static str,
        events: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            token: Some(token.into()),
            replacement_token: None,
            username: None,
            password: None,
            identity,
            events,
            binding_resolutions: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn bearer_replacing(
        token: impl Into<String>,
        replacement_token: impl Into<String>,
        identity: &'static str,
        events: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            token: Some(token.into()),
            replacement_token: Some(replacement_token.into()),
            username: None,
            password: None,
            identity,
            events,
            binding_resolutions: Arc::new(AtomicUsize::new(0)),
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
            replacement_token: None,
            username: Some(username.into()),
            password: Some(password.into()),
            identity,
            events,
            binding_resolutions: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[derive(Clone)]
pub struct ObservationAuthCx;

#[derive(Clone)]
struct ObservationSecretProvider;

impl CredentialProvider<ObservationAuthCx> for ObservationSecretProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "token")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, ObservationAuthCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let refreshed = {
                let mut events = ctx.auth.events.lock().await;
                let refreshed = events.iter().any(|event| event == "auth_retry");
                events.push("auth_prepare".to_string());
                refreshed
            };
            refreshed
                .then_some(ctx.auth.replacement_token.as_deref())
                .flatten()
                .or(ctx.auth.token.as_deref())
                .map(|token| ApiKey::new(token.to_string()))
                .ok_or_else(|| {
                    AuthError::new(
                        AuthErrorKind::MissingCredential,
                        "missing credential `test/token`; acquire or configure it before sending request",
                    )
                })
        })
    }

    fn invalidate<'a>(
        &'a self,
        ctx: CredentialContext<'a, ObservationAuthCx>,
        _current: Option<&'a Self::Credential>,
        reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            let _ = reason;
            let mut events = ctx.auth.events.lock().await;
            events.push("provider_refresh".to_string());
            events.push("auth_retry".to_string());
            Ok(())
        })
    }
}

#[derive(Clone)]
struct ObservationBasicProvider;

impl CredentialProvider<ObservationAuthCx> for ObservationBasicProvider {
    type Credential = BasicCredential;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "token")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, ObservationAuthCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            ctx.auth
                .events
                .lock()
                .await
                .push("auth_prepare".to_string());
            let username = ctx.auth.username.as_deref().ok_or_else(|| {
                AuthError::new(AuthErrorKind::MissingCredential, "missing basic username")
            })?;
            let password = ctx.auth.password.as_deref().ok_or_else(|| {
                AuthError::new(AuthErrorKind::MissingCredential, "missing basic password")
            })?;
            Ok(BasicCredential::new(
                username.to_string(),
                password.to_string(),
            ))
        })
    }
}

pub struct ObservationAuthState {
    secret: Arc<CredentialProviderState<ObservationAuthCx, ObservationSecretProvider>>,
    basic: Arc<CredentialProviderState<ObservationAuthCx, ObservationBasicProvider>>,
    use_basic: bool,
    refresh_on_challenge: bool,
    binding_resolutions: Arc<AtomicUsize>,
}

impl Clone for ObservationAuthState {
    fn clone(&self) -> Self {
        Self {
            secret: self.secret.clone(),
            basic: self.basic.clone(),
            use_basic: self.use_basic,
            refresh_on_challenge: self.refresh_on_challenge,
            binding_resolutions: self.binding_resolutions.clone(),
        }
    }
}

impl ClientContext for ObservationAuthCx {
    type Vars = ();
    type AuthVars = ObservationAuthVars;
    type AuthState = ObservationAuthState;
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        let secret = Arc::new(CredentialProviderState::new(ObservationSecretProvider));
        let basic = Arc::new(CredentialProviderState::new(ObservationBasicProvider));
        #[cfg(feature = "dangerous-dev-tools")]
        {
            let events = auth.events.clone();
            concord_core::__development::observe_credential_provider_state(
                secret.as_ref(),
                Arc::new(move |event| {
                    let rendered = match event {
                        concord_core::__development::CredentialLifecycleEvent::ChallengeClassified { status } => {
                            format!("auth_classification:{status}")
                        }
                        concord_core::__development::CredentialLifecycleEvent::ResponseReleased => {
                            "auth_response_released".to_string()
                        }
                        concord_core::__development::CredentialLifecycleEvent::GenerationInvalidated { requested, current, applied } => {
                            format!("auth_invalidation:identity_match={}:applied={applied}", requested == current)
                        }
                    };
                    events
                        .try_lock()
                        .expect("auth observation event lock")
                        .push(rendered);
                }),
            );

            let events = auth.events.clone();
            concord_core::__development::observe_credential_provider_state(
                basic.as_ref(),
                Arc::new(move |event| {
                    let name = match event {
                        concord_core::__development::CredentialLifecycleEvent::ChallengeClassified { .. } => "unrelated_auth_classification",
                        concord_core::__development::CredentialLifecycleEvent::ResponseReleased => "unrelated_auth_response_released",
                        concord_core::__development::CredentialLifecycleEvent::GenerationInvalidated { .. } => "unrelated_auth_invalidation",
                    };
                    events
                        .try_lock()
                        .expect("unrelated auth observation event lock")
                        .push(name.to_string());
                }),
            );
        }
        ObservationAuthState {
            secret,
            basic,
            use_basic: auth.username.is_some(),
            refresh_on_challenge: auth.identity == "refresh",
            binding_resolutions: auth.binding_resolutions.clone(),
        }
    }

    fn auth_provider_binding<'a>(
        credential: &CredentialId,
        auth_state: &'a Self::AuthState,
    ) -> Option<concord_core::advanced::AuthProviderBinding<'a, Self>> {
        auth_state
            .binding_resolutions
            .fetch_add(1, AtomicOrdering::SeqCst);
        if credential != &CredentialId::new("test", "token") {
            return None;
        }
        let challenge = if auth_state.refresh_on_challenge {
            concord_core::advanced::AuthChallengeMode::Refresh
        } else {
            concord_core::advanced::AuthChallengeMode::InvalidateOnly
        };
        Some(if auth_state.use_basic {
            auth_state.basic.basic_binding(
                concord_core::advanced::AuthPreparationMode::RequestLocal,
                challenge,
            )
        } else {
            auth_state.secret.secret_binding(
                concord_core::advanced::AuthPreparationMode::RequestLocal,
                challenge,
            )
        })
    }
}

impl RegressionEndpoint<ObservationAuthCx> for TextEndpoint {
    type Response = String;

    buffered_endpoint_execute!(ObservationAuthCx, concord_core::prelude::Text<String>);
}

buffered_endpoint_response_terminal!(
    TextEndpoint,
    ObservationAuthCx,
    concord_core::prelude::Text<String>
);

impl RegressionReusableEndpoint<ObservationAuthCx> for TextEndpoint {
    fn plan(
        &self,
        _ctx: &RegressionPlanContext<'_, ObservationAuthCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.as_ref().map(|_| PaginationMarker),
        ))
    }
}

pub fn auth_policy(placement: AuthPlacement) -> ResolvedPolicy {
    ResolvedPolicy {
        auth: crate::regression_tests::test_api::AuthPlan {
            requirements: vec![AuthRequirement {
                credential: crate::regression_tests::test_api::CredentialRef {
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

#[derive(Clone)]
pub struct NativeMockHarness {
    server: MockServer,
    handle: Arc<StdMutex<NativeMockHandle>>,
    events: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
pub enum NativeMockOutcome {
    Response(MockResponse),
    DisconnectAfterRequest,
}

impl From<MockResponse> for NativeMockOutcome {
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
}

impl NativeMockHarness {
    pub fn new(events: Arc<Mutex<Vec<String>>>, responses: Vec<MockResponse>) -> Self {
        Self::with_outcomes(events, responses.into_iter().map(Into::into).collect())
    }

    pub fn with_outcomes(
        events: Arc<Mutex<Vec<String>>>,
        outcomes: Vec<NativeMockOutcome>,
    ) -> Self {
        let replies = outcomes.into_iter().map(|outcome| match outcome {
            NativeMockOutcome::Response(response) => native_reply(response),
            NativeMockOutcome::DisconnectAfterRequest => {
                NativeMockReply::disconnect_after_request()
            }
        });
        let head_events = events.clone();
        let body_events = events.clone();
        let (server, handle) = native_mock::mock()
            .replies(replies)
            .on_request_head(move || {
                head_events.blocking_lock().push("request_head".to_string());
            })
            .on_request_body_complete(move || {
                body_events
                    .blocking_lock()
                    .push("request_body_complete".to_string());
            })
            .build();
        Self {
            server,
            handle: Arc::new(StdMutex::new(handle)),
            events,
        }
    }

    pub fn from_native_replies(
        events: Arc<Mutex<Vec<String>>>,
        replies: impl IntoIterator<Item = NativeMockReply>,
    ) -> Self {
        let head_events = events.clone();
        let body_events = events.clone();
        let (server, handle) = native_mock::mock()
            .replies(replies)
            .on_request_head(move || {
                head_events.blocking_lock().push("request_head".to_string());
            })
            .on_request_body_complete(move || {
                body_events
                    .blocking_lock()
                    .push("request_body_complete".to_string());
            })
            .build();
        Self {
            server,
            handle: Arc::new(StdMutex::new(handle)),
            events,
        }
    }

    pub fn from_native_replies_with_head_action(
        events: Arc<Mutex<Vec<String>>>,
        replies: impl IntoIterator<Item = NativeMockReply>,
        head_action: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let head_events = events.clone();
        let body_events = events.clone();
        let (server, handle) = native_mock::mock()
            .replies(replies)
            .on_request_head(move || {
                head_events.blocking_lock().push("request_head".to_string());
                head_action();
            })
            .on_request_body_complete(move || {
                body_events
                    .blocking_lock()
                    .push("request_body_complete".to_string());
            })
            .build();
        Self {
            server,
            handle: Arc::new(StdMutex::new(handle)),
            events,
        }
    }

    pub fn from_native_repeating(events: Arc<Mutex<Vec<String>>>, reply: NativeMockReply) -> Self {
        let head_events = events.clone();
        let body_events = events.clone();
        let (server, handle) = native_mock::mock()
            .repeating(reply)
            .on_request_head(move || {
                head_events.blocking_lock().push("request_head".to_string());
            })
            .on_request_body_complete(move || {
                body_events
                    .blocking_lock()
                    .push("request_body_complete".to_string());
            })
            .build();
        Self {
            server,
            handle: Arc::new(StdMutex::new(handle)),
            events,
        }
    }

    pub async fn sent_count(&self) -> usize {
        self.handle
            .lock()
            .expect("native handle lock")
            .completed_len()
    }

    pub async fn wait_for_sends(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.sent_count().await < expected {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {expected} native requests"));
    }

    pub async fn requests(&self) -> Vec<CapturedWireRequest> {
        let requests = self.handle.lock().expect("native handle lock").recorded();
        requests.into_iter().map(captured_native_request).collect()
    }

    pub fn configure_reqwest(
        &self,
        builder: concord_core::advanced::SafeReqwestBuilder,
    ) -> concord_core::advanced::SafeReqwestBuilder {
        self.server.configure_reqwest(builder)
    }
}

#[derive(Clone)]
pub struct GatedNativeMockHarness {
    inner: NativeMockHarness,
    gate: NativeReplyGate,
}

impl GatedNativeMockHarness {
    pub fn new(events: Arc<Mutex<Vec<String>>>, responses: Vec<MockResponse>) -> Self {
        let gate = NativeReplyGate::new();
        let replies = responses
            .into_iter()
            .map(|response| native_reply(response).with_gate(gate.clone()));
        Self {
            inner: NativeMockHarness::from_native_replies(events, replies),
            gate,
        }
    }

    pub async fn sent_count(&self) -> usize {
        self.inner.sent_count().await
    }

    pub async fn wait_for_sends(&self, expected: usize) {
        self.inner.wait_for_sends(expected).await;
    }

    pub fn release_all(&self) {
        self.gate.release();
    }

    pub fn configure_reqwest(
        &self,
        builder: concord_core::advanced::SafeReqwestBuilder,
    ) -> concord_core::advanced::SafeReqwestBuilder {
        self.inner.configure_reqwest(builder)
    }
}

fn native_reply(response: MockResponse) -> NativeMockReply {
    let mut reply = NativeMockReply::status(response.status);
    for (name, value) in response.headers {
        if let Some(name) = name {
            reply = reply.with_header(name, value);
        }
    }
    if let Some(length) = response.content_length {
        reply = reply.with_header(
            http::header::CONTENT_LENGTH,
            HeaderValue::from_str(&length.to_string()).expect("content length"),
        );
    }
    match response.chunks {
        Some(chunks) if response.content_length.is_none() => reply.with_chunks(chunks),
        Some(chunks) => reply.with_body(Bytes::from(chunks.concat())),
        None => reply.with_body(response.body),
    }
}

fn captured_native_request(request: native_mock::RecordedRequest) -> CapturedWireRequest {
    let body = if request.body.is_empty() {
        CapturedBody::Empty
    } else {
        CapturedBody::Bytes(request.body)
    };
    CapturedWireRequest {
        meta: CapturedRequestExecutionMeta {
            endpoint: request.endpoint,
            method: request.method.clone(),
            page_index: request.page_index,
        },
        url: request.url,
        headers: request.headers,
        body,
        timeout: request.timeout,
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
                "rate_meta:{}:{}:{}:{}:{}",
                meta.endpoint,
                meta.method,
                meta.url,
                meta.url_host.unwrap_or("<none>"),
                meta.page_index
            ));
            events.push(format!("rate_idempotent:{}", meta.idempotent));
            events.push(format!("rate_status:{status}"));
            events.push(format!("rate_headers:{headers:?}"));
            Ok(RateLimitResponseAction::Continue)
        })
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
                "hook_meta:{}:{}:{}:{}",
                meta.endpoint, meta.method, meta.url, meta.page_index
            ));
            events.push(format!("hook_idempotent:{}", meta.idempotent));
            events.push(format!("hook_status:{status}"));
            events.push(format!("hook_headers:{headers:?}"));
        })
    }

    fn request_error<'a>(
        &'a self,
        ctx: RequestErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .await
                .push(format!("request_error:{:?}:{ctx:?}", ctx.category));
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

    fn request_error<'a>(
        &'a self,
        _ctx: RequestErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().await.push("request_error".to_string());
        })
    }
}

#[derive(Clone)]
pub struct RegressionClient<Cx: ClientContext> {
    inner: ApiClient<Cx>,
    _harness: Option<NativeMockHarness>,
}

impl<Cx: ClientContext> RegressionClient<Cx> {
    pub fn from_inner(inner: ApiClient<Cx>, harness: Option<NativeMockHarness>) -> Self {
        Self {
            inner,
            _harness: harness,
        }
    }

    pub fn request<E>(
        &self,
        endpoint: E,
    ) -> crate::regression_tests::test_api::PendingRequest<'_, Cx, E>
    where
        E: crate::regression_tests::test_api::RegressionIntoPlan<Cx>,
    {
        crate::regression_tests::test_api::PendingRequest::new(&self.inner, endpoint)
    }
}

impl<Cx: ClientContext> std::ops::Deref for RegressionClient<Cx> {
    type Target = ApiClient<Cx>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Cx: ClientContext> std::ops::DerefMut for RegressionClient<Cx> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub type TestClient = RegressionClient<TestCx>;

pub fn client(auth: TestAuthVars, harness: NativeMockHarness) -> TestClient {
    let inner = ApiClient::with_safe_reqwest_builder((), auth, |builder| {
        harness.configure_reqwest(builder)
    })
    .expect("native mock client");
    TestClient::from_inner(inner, Some(harness))
}

pub fn observation_client(
    auth: ObservationAuthVars,
    harness: &NativeMockHarness,
) -> RegressionClient<ObservationAuthCx> {
    let inner = ApiClient::with_safe_reqwest_builder((), auth, |builder| {
        harness.configure_reqwest(builder)
    })
    .expect("native mock client");
    RegressionClient::from_inner(inner, None)
}

pub fn configure_runtime<Cx: ClientContext>(
    client: &mut ApiClient<Cx>,
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

    fn request_error<'a>(
        &'a self,
        _ctx: RequestErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let drop_token = self.drop_probe.as_ref().map(DropProbe::token);
            let _drop_token = drop_token;
            self.events
                .lock()
                .await
                .push("hook_request_error".to_string());
            self.gate.enter("hook_request_error").await;
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

    fn request_headers(
        &self,
        _dbg: concord_core::prelude::DebugLevel,
        headers: concord_core::advanced::SanitizedHeaders<'_>,
    ) {
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

    fn response_headers(
        &self,
        _dbg: concord_core::prelude::DebugLevel,
        headers: concord_core::advanced::SanitizedHeaders<'_>,
    ) {
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
