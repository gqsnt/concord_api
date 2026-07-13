use crate::auth::{
    AuthError, AuthErrorKind, AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse, AuthMode,
};
use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::endpoint::{ClientPlanContext, RequestPlan};
use crate::error::{ApiClientError, ErrorContext};
use crate::execution_meta::RequestExecutionMeta;
use crate::policy::Policy;
use crate::rate_limit::{
    RateLimitContext, RateLimitPlan, RateLimitResponseAction, RateLimitResponseContext, RateLimiter,
};
use crate::request::PendingRequest;
use crate::response_classify::{ResponseClass, classify_status};
use crate::runtime_hooks::{
    HookMeta, PostResponseHookContext, PreSendHookContext, RequestErrorHookContext, RuntimeHooks,
};
use crate::runtime_state::ClientRuntimeState;
use crate::transport::{BuiltRequest, BuiltResponse, DecodedResponse, ExecutionResponse};
use crate::types::RouteBuilder;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use http::uri::Scheme;
use std::sync::Arc;
use std::sync::RwLock;

// Request lifecycle is kept in phase modules while preserving one private client namespace.
mod api;
mod auth_http;
mod build;
mod context;
mod execute;
mod send_flow;

pub use self::api::*;
pub use self::context::*;

use self::auth_http::*;
