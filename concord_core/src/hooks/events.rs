use std::time::Duration;

use http::{Method, StatusCode};

use crate::operation::OperationId;
use crate::redaction::SanitizedHeaders;

/// Structural category of a request body visible to lifecycle observers.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyKind {
    /// No request body.
    Empty,
    /// A complete buffered request body.
    Buffered,
    /// A lazy streaming request body.
    Stream,
    /// A multipart request body.
    Multipart,
}

/// Sanitized metadata supplied before expensive request work begins.
#[derive(Debug)]
pub struct BeforeSend<'a> {
    pub operation_id: OperationId,
    pub parent_operation_id: Option<OperationId>,
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub safe_url: &'a str,
    pub page_index: u32,
    pub recovery_index: u8,
    pub headers: SanitizedHeaders<'a>,
    pub body_kind: BodyKind,
    pub known_body_length: Option<u64>,
}

/// Sanitized metadata supplied for every received final response head.
#[derive(Debug)]
pub struct ResponseHeadEvent<'a> {
    pub operation_id: OperationId,
    pub parent_operation_id: Option<OperationId>,
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub safe_url: &'a str,
    pub page_index: u32,
    pub recovery_index: u8,
    pub status: StatusCode,
    pub headers: SanitizedHeaders<'a>,
}

/// Outside-deadline observation of one already-fixed request or page result.
#[derive(Debug)]
pub struct CompletionEvent<'a> {
    pub operation_id: OperationId,
    pub parent_operation_id: Option<OperationId>,
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub safe_url: &'a str,
    pub page_index: u32,
    pub recovered_authentication: bool,
    pub elapsed: Duration,
    pub final_status: Option<StatusCode>,
    pub outcome: CompletionOutcome,
}

/// Outside-deadline observation of one already-fixed pagination result.
#[derive(Debug)]
pub struct PaginationCompletionEvent<'a> {
    pub operation_id: OperationId,
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub pages_completed: u32,
    pub items_collected: usize,
    pub recovered_authentication: bool,
    pub elapsed: Duration,
    pub final_status: Option<StatusCode>,
    pub outcome: CompletionOutcome,
}

/// Safe structural category of an already-fixed operation result.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionOutcome {
    Success,
    StreamOpened,
    Configuration,
    RequestEncoding,
    RequestBody,
    AuthenticationUnavailable,
    AuthenticationProvider,
    AuthenticationRejected,
    RateLimit,
    Hook,
    Timeout,
    Connect,
    Transport,
    HttpStatus,
    ResponseBody,
    ResponseDecoding,
    ResponseContract,
    Pagination,
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
