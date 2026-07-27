use std::time::Duration;

use http::{Method, StatusCode};

use crate::body::BodyKind;
use crate::operation::OperationId;
use crate::redaction::SanitizedHeaders;

/// Safe structural identity of one visible managed attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttemptInfo {
    /// Zero-based index across all visible attempts for this request or page.
    pub index: u32,
    /// Number of policy retries started through this attempt.
    pub policy_retry_index: u32,
    /// Whether this attempt is the single authentication recovery.
    pub authentication_recovery: bool,
}

impl AttemptInfo {
    pub(crate) const INITIAL: Self = Self {
        index: 0,
        policy_retry_index: 0,
        authentication_recovery: false,
    };
}

/// Sanitized metadata supplied before expensive request work begins.
#[derive(Debug)]
pub struct BeforeSend<'a> {
    /// Identity of this request or page operation.
    pub operation_id: OperationId,
    /// Aggregate pagination identity for a page operation.
    pub parent_operation_id: Option<OperationId>,
    /// Stable generated or runtime endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Credential-sanitized logical URL. Public values may still contain
    /// operational identifiers or personally identifiable information.
    pub safe_url: &'a str,
    /// Zero-based page index; direct requests use zero.
    pub page_index: u32,
    /// Structural identity of this visible managed attempt.
    pub attempt: AttemptInfo,
    /// Sanitized public request headers.
    pub headers: SanitizedHeaders<'a>,
    /// Structural request-body category.
    pub body_kind: BodyKind,
    /// Known request-body length when available before materialization.
    pub known_body_length: Option<u64>,
}

/// Sanitized metadata supplied for every received final response head.
#[derive(Debug)]
pub struct ResponseHeadEvent<'a> {
    /// Identity of this request or page operation.
    pub operation_id: OperationId,
    /// Aggregate pagination identity for a page operation.
    pub parent_operation_id: Option<OperationId>,
    /// Stable generated or runtime endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Credential-sanitized logical URL. Public values may still contain
    /// operational identifiers or personally identifiable information.
    pub safe_url: &'a str,
    /// Zero-based page index; direct requests use zero.
    pub page_index: u32,
    /// Structural identity of the attempt that produced this head.
    pub attempt: AttemptInfo,
    /// Final status for this Concord attempt.
    pub status: StatusCode,
    /// Sanitized actual response headers.
    pub headers: SanitizedHeaders<'a>,
}

/// Outside-deadline observation of one already-fixed request or page result.
#[derive(Debug)]
pub struct CompletionEvent<'a> {
    /// Identity of this request or page operation.
    pub operation_id: OperationId,
    /// Aggregate pagination identity for a page operation.
    pub parent_operation_id: Option<OperationId>,
    /// Stable generated or runtime endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Credential-sanitized logical URL. Public values may still contain
    /// operational identifiers or personally identifiable information.
    pub safe_url: &'a str,
    /// Zero-based page index; direct requests use zero.
    pub page_index: u32,
    /// Number of visible attempts whose before-send stage began.
    pub attempts_started: u32,
    /// Number of policy retries started for this operation.
    pub policy_retries: u32,
    /// Whether this result followed one authentication recovery.
    pub recovered_authentication: bool,
    /// Elapsed Concord operation time before completion observation began.
    pub elapsed: Duration,
    /// Final received status when a response head was available.
    pub final_status: Option<StatusCode>,
    /// Structural category of the already-fixed result.
    pub outcome: CompletionOutcome,
}

/// Outside-deadline observation of one already-fixed pagination result.
#[derive(Debug)]
pub struct PaginationCompletionEvent<'a> {
    /// Identity of the aggregate pagination operation.
    pub operation_id: OperationId,
    /// Stable generated endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method used for every page.
    pub method: &'a Method,
    /// Number of pages whose direct completion finished.
    pub pages_completed: u32,
    /// Number of items retained for the caller.
    pub items_collected: usize,
    /// Total visible attempts started across all page operations.
    pub attempts_started: u32,
    /// Total policy retries started across all page operations.
    pub policy_retries: u32,
    /// Whether any completed page used authentication recovery.
    pub recovered_authentication: bool,
    /// Elapsed aggregate time before pagination completion observation began.
    pub elapsed: Duration,
    /// Final received status when the last result had a response head.
    pub final_status: Option<StatusCode>,
    /// Structural category of the already-fixed aggregate result.
    pub outcome: CompletionOutcome,
}

/// Safe structural category of an already-fixed operation result.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionOutcome {
    /// A buffered or no-content operation succeeded.
    Success,
    /// An accepted response stream was opened for the caller.
    StreamOpened,
    /// Request or runtime configuration was invalid.
    Configuration,
    /// A buffered request encoder failed.
    RequestEncoding,
    /// Request body materialization or streaming failed.
    RequestBody,
    /// No usable credential material was available.
    AuthenticationUnavailable,
    /// Credential-provider work failed.
    AuthenticationProvider,
    /// The protected service rejected attributed credential material.
    AuthenticationRejected,
    /// Rate-limit preparation or acquisition failed.
    RateLimit,
    /// A result-determining lifecycle hook rejected the operation.
    Hook,
    /// The operation deadline expired.
    Timeout,
    /// Native connection establishment failed.
    Connect,
    /// Another native transport operation failed.
    Transport,
    /// The final response status was not accepted.
    HttpStatus,
    /// Accepted response body collection or streaming failed.
    ResponseBody,
    /// An accepted buffered response decoder failed.
    ResponseDecoding,
    /// Accepted response metadata violated the declared contract.
    ResponseContract,
    /// Pagination preparation or controller work failed.
    Pagination,
    /// Concord reduced an otherwise unclassified internal failure.
    Internal,
}

impl CompletionOutcome {
    pub(crate) const fn from_error_category(category: crate::error::ErrorCategory) -> Self {
        use crate::error::ErrorCategory;

        match category {
            ErrorCategory::Configuration => Self::Configuration,
            ErrorCategory::RequestEncoding => Self::RequestEncoding,
            ErrorCategory::RequestBody => Self::RequestBody,
            ErrorCategory::AuthenticationUnavailable => Self::AuthenticationUnavailable,
            ErrorCategory::AuthenticationProvider => Self::AuthenticationProvider,
            ErrorCategory::AuthenticationRejected => Self::AuthenticationRejected,
            ErrorCategory::RateLimit => Self::RateLimit,
            ErrorCategory::Hook => Self::Hook,
            ErrorCategory::Timeout => Self::Timeout,
            ErrorCategory::Connect => Self::Connect,
            ErrorCategory::Transport => Self::Transport,
            ErrorCategory::HttpStatus => Self::HttpStatus,
            ErrorCategory::ResponseBody => Self::ResponseBody,
            ErrorCategory::ResponseDecoding => Self::ResponseDecoding,
            ErrorCategory::ResponseContract => Self::ResponseContract,
            ErrorCategory::Pagination => Self::Pagination,
            ErrorCategory::Internal => Self::Internal,
        }
    }
}
