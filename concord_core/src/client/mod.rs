use crate::auth::{
    AuthDecision, AuthError, AuthErrorKind, AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse,
    AuthMode, AuthRequirementId,
};
use crate::cache::{CacheAfter, CacheBefore, CacheRequestMode, CacheRevalidation, CacheStore};
use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::endpoint::{BodyPlan, ClientPlanContext, Endpoint, RequestPlan};
use crate::error::{ApiClientError, ErrorContext};
use crate::inflight::{InflightPolicy, RequestKey, SharedSendError, SharedSendResult};
use crate::pagination::Caps;
use crate::policy::Policy;
use crate::rate_limit::{
    RateLimitContext, RateLimitPlan, RateLimitResponseAction, RateLimitResponseContext, RateLimiter,
};
use crate::request::PendingRequest;
use crate::response_classify::{ResponseClass, classify_status};
use crate::retry::{RetryContext, RetryDecision, RetryOutcome, RetryPolicy, RetrySetting};
use crate::runtime_hooks::{
    HookMeta, PostResponseHookContext, PreSendHookContext, RuntimeHooks, TransportErrorHookContext,
};
use crate::runtime_state::ClientRuntimeState;
use crate::transport::{BuiltRequest, BuiltResponse, DecodedResponse, RequestMeta};
use crate::transport::{
    ReqwestTransport, Transport, TransportBody, TransportError, TransportResponse,
};
use crate::types::RouteBuilder;
use bytes::Bytes;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use http::uri::Scheme;
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

// Request lifecycle is kept in phase files while preserving one private client namespace.
include!("context.rs");
include!("api.rs");
include!("execute.rs");
include!("build.rs");
include!("send_flow.rs");
include!("retry_flow.rs");
include!("auth_http.rs");
