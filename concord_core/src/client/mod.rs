use crate::auth::{
    AuthDecision, AuthError, AuthErrorKind, AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse,
    AuthMode, AuthRequirementId,
};
use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::endpoint::{BodyPlan, ClientPlanContext, RequestPlan};
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
use crate::runtime_hooks::{
    HookMeta, PostResponseHookContext, PreSendHookContext, RuntimeHooks, TransportErrorHookContext,
};
use crate::runtime_state::ClientRuntimeState;
use crate::sse::SseStream;
#[cfg(feature = "transport-reqwest")]
use crate::transport::DefaultTransportMarker;
#[cfg(feature = "transport-reqwest")]
use crate::transport::ReqwestTransport;
use crate::transport::{BuiltRequest, BuiltResponse, DecodedResponse, RequestMeta};
use crate::transport::{
    DefaultTransport, Transport, TransportBody, TransportError, TransportResponse,
};
use crate::types::RouteBuilder;
use bytes::Bytes;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use http::uri::Scheme;
use std::fmt;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

#[derive(Debug)]
enum BodyReadError {
    Transport(TransportError),
    ContentLengthTooLarge { limit: usize, actual: u64 },
    LimitExceeded { limit: usize },
}

impl fmt::Display for BodyReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BodyReadError::Transport(source) => write!(f, "{source}"),
            BodyReadError::ContentLengthTooLarge { limit, actual } => {
                write!(
                    f,
                    "response Content-Length {actual} exceeds limit {limit} bytes"
                )
            }
            BodyReadError::LimitExceeded { limit } => {
                write!(
                    f,
                    "response body exceeded limit {limit} bytes while reading"
                )
            }
        }
    }
}

impl std::error::Error for BodyReadError {}

impl From<TransportError> for BodyReadError {
    fn from(value: TransportError) -> Self {
        BodyReadError::Transport(value)
    }
}

async fn read_body_all_limited(
    body: &mut dyn TransportBody,
    content_length: Option<u64>,
    limit: Option<usize>,
) -> Result<Bytes, BodyReadError> {
    if let (Some(limit), Some(actual)) = (limit, content_length) {
        let actual_usize = usize::try_from(actual).unwrap_or(usize::MAX);
        if actual_usize > limit {
            return Err(BodyReadError::ContentLengthTooLarge { limit, actual });
        }
    }

    const SMALL_START: usize = 8 * 1024;
    const LARGE_START: usize = 64 * 1024;
    // When the response body limit is disabled, we still must not let an unverified
    // server Content-Length drive pre-read allocation. Honest large bodies grow the
    // buffer via normal amortized reallocation; this only caps the INITIAL guess.
    const NO_LIMIT_INITIAL_CAP: usize = 1 << 20; // 1 MiB
    let cap = match content_length {
        Some(n) => {
            let n_usize = usize::try_from(n).unwrap_or(usize::MAX);
            match limit {
                Some(limit) => n_usize.min(limit),
                None => n_usize.clamp(SMALL_START, NO_LIMIT_INITIAL_CAP),
            }
        }
        None => limit.map_or(SMALL_START, |limit| limit.min(LARGE_START)),
    };

    let mut buf = bytes::BytesMut::with_capacity(cap);
    while let Some(chunk) = body.next_chunk().await? {
        if let Some(limit) = limit {
            let next_len = buf
                .len()
                .checked_add(chunk.len())
                .ok_or(BodyReadError::LimitExceeded { limit })?;
            if next_len > limit {
                return Err(BodyReadError::LimitExceeded { limit });
            }
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf.freeze())
}

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
