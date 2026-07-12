use crate::auth::{
    AuthError, AuthErrorKind, AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse, AuthMode,
    AuthRequirementId,
};
use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::endpoint::{ClientPlanContext, RequestPlan};
use crate::error::{ApiClientError, ErrorContext};
use crate::policy::Policy;
use crate::rate_limit::{
    RateLimitContext, RateLimitPlan, RateLimitResponseAction, RateLimitResponseContext, RateLimiter,
};
use crate::request::PendingRequest;
use crate::response_classify::{ResponseClass, classify_status};
use crate::retry::{
    RetryContext, RetryDecision, RetryOutcome, RetryPolicy, RetrySetting,
    validate_capped_retry_delay,
};
use crate::retry_admission::{AdmissionPermit, OriginHandle, OriginKey};
use crate::runtime_hooks::{
    HookMeta, PostResponseHookContext, PreSendHookContext, RuntimeHooks, TransportErrorHookContext,
};
use crate::runtime_state::ClientRuntimeState;
use crate::transport::DefaultTransportMarker;
use crate::transport::ReqwestTransport;
use crate::transport::{
    AttemptResponse, BuiltRequest, BuiltResponse, DecodedResponse, RequestMeta,
};
use crate::transport::{DefaultTransport, Transport};
use crate::types::RouteBuilder;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use http::uri::Scheme;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

// Request lifecycle is kept in phase modules while preserving one private client namespace.
mod api;
mod auth_http;
mod build;
mod context;
mod execute;
mod retry_flow;
mod send_flow;

pub use self::api::*;
pub use self::context::*;

use self::auth_http::*;
