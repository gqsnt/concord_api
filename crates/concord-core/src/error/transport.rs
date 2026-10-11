use std::error::Error;
use std::fmt;
use std::time::Duration;

use http::{Method, StatusCode};

use crate::endpoint::EndpointId;
use crate::redaction::SanitizedHeaderMap;

use super::{
    AttemptFacts, AuthErrorKind, ConfigurationErrorKind, DispatchKnowledge, ErrorCategory,
    ErrorJournal, FailureStage, HttpResponseFailure, OperationCounts, PaginationErrorKind,
    ProviderAcquisitionFacts, RateLimitErrorKind,
};

/// A structured safe failure from an endpoint operation.
///
/// **Journal contract (EXEC-01/EXEC-02).** Every error that escapes a managed
/// operation (direct request, traversal page, traversal aggregate, or
/// credential-storage execution) is contextualized by that operation's
/// journal: its [`failure_stage`](Self::failure_stage), aggregate
/// [`dispatch_knowledge`](Self::dispatch_knowledge), optional
/// [`last_attempt`](Self::last_attempt), [`last_response_status`](Self::last_response_status),
/// [`operation_id`](Self::operation_id) / [`parent_operation_id`](Self::parent_operation_id)
/// and [`operation_counts`](Self::operation_counts) are settled facts, not
/// defaults chosen by the code that first produced the failure. Low-level
/// failures carry only a category and safe details; they never assert that a
/// request was not dispatched. An error fixed before any operation identity
/// existed (or during planning, which never dispatches) reports
/// [`FailureStage::Preparation`], [`DispatchKnowledge::NotDispatched`] and no
/// operation identity.
///
/// **Write reconciliation.** For a failed write, aggregate
/// `ResponseObserved` or `PossiblyDispatched` means the application may need
/// reconciliation with its durable command identifier. It does not by itself
/// mean the operation is retryable, and even an observed success status
/// followed by a body or decoder failure can have committed the write.
///
/// **Stream summaries.** Consumption failures of an already acquired managed
/// stream are reported with [`FailureStage::StreamConsumption`]. A value-free
/// per-stream summary accessor is an integration point owned by the stream
/// subsystem; it attaches to this type through a crate-private constructor
/// and is not part of this version's public surface.
///
/// **Formatting.** `Display` and `Debug` are value-free: they contain the
/// endpoint ID, method, category, structured kinds, statuses and opaque
/// identities, never URLs, headers, bodies, credentials, raw native causes or
/// arbitrary user error text. [`Error::source`] exposes only Concord's own
/// structured safe subsystem errors.
pub struct ApiError {
    endpoint: EndpointId,
    method: Method,
    category: ErrorCategory,
    configuration_kind: Option<ConfigurationErrorKind>,
    auth_kind: Option<AuthErrorKind>,
    rate_limit_kind: Option<RateLimitErrorKind>,
    pagination_kind: Option<PaginationErrorKind>,
    detail: Option<Box<ApiErrorDetail>>,
    /// Precise stage named by the code that fixed the failure, if any.
    stage_hint: Option<FailureStage>,
    /// Positive dispatch fact known where the failure was produced (for
    /// example `ResponseObserved` for a failure after a received head). It
    /// only ever raises knowledge; it is never a no-send claim.
    knowledge_floor: DispatchKnowledge,
    journal: Option<Box<ErrorJournal>>,
    traversal: Option<Box<crate::pagination::TraversalSummary>>,
}

enum ApiErrorDetail {
    Response(HttpResponseFailure),
    Authentication(crate::auth::AuthError),
    RateLimit(crate::rate_limit::RateLimitError),
    Pagination(crate::pagination::PaginationError),
    /// Value-free journal snapshot of a managed stream whose consumption
    /// produced this error. Stream errors carry no other detail.
    Stream(crate::response::StreamSummary),
}

impl ApiError {
    pub(crate) fn from_http_response(response: HttpResponseFailure) -> Self {
        Self::with_response(ErrorCategory::HttpStatus, None, response)
    }

    pub(crate) fn authentication_rejected(response: HttpResponseFailure) -> Self {
        Self::with_response(
            ErrorCategory::AuthenticationRejected,
            Some(AuthErrorKind::Rejected),
            response,
        )
    }

    fn with_response(
        category: ErrorCategory,
        auth_kind: Option<AuthErrorKind>,
        response: HttpResponseFailure,
    ) -> Self {
        let endpoint = response.endpoint_id().clone();
        let method = response.method().clone();
        let mut error = Self::classified(endpoint, method, category);
        error.auth_kind = auth_kind;
        error.detail = Some(Box::new(ApiErrorDetail::Response(response)));
        error.stage_hint = Some(FailureStage::RejectedResponse);
        error.knowledge_floor = DispatchKnowledge::ResponseObserved;
        error
    }

    /// Reduces a native transport error to its safe category.
    ///
    /// The native error is dropped immediately. No dispatch fact is derived
    /// from the category: in particular a `Connect` classification is not
    /// proof that the application request was not submitted. The operation
    /// journal settles dispatch knowledge at the handoff boundary.
    pub(crate) fn from_reqwest(
        endpoint: EndpointId,
        method: Method,
        error: reqwest::Error,
        managed_body_failed: bool,
    ) -> Self {
        let category = reqwest_category(error, managed_body_failed);
        Self::classified(endpoint, method, category)
    }

    /// Creates a low-level categorized failure with no stage or dispatch
    /// claim. Until an operation journal (or the explicit pre-operation
    /// context) settles it, its public stage is `DuringDispatchOrUnknown` and
    /// its dispatch knowledge `PossiblyDispatched` — never a fabricated
    /// no-send fact.
    pub(crate) fn classified(
        endpoint: EndpointId,
        method: Method,
        category: ErrorCategory,
    ) -> Self {
        Self {
            endpoint,
            method,
            category,
            configuration_kind: None,
            auth_kind: None,
            rate_limit_kind: None,
            pagination_kind: None,
            detail: None,
            stage_hint: None,
            knowledge_floor: DispatchKnowledge::NotDispatched,
            journal: None,
            traversal: None,
        }
    }

    pub(crate) fn configuration(
        endpoint: EndpointId,
        method: Method,
        kind: ConfigurationErrorKind,
    ) -> Self {
        let mut error = Self::classified(endpoint, method, ErrorCategory::Configuration);
        error.configuration_kind = Some(kind);
        error
    }

    pub(crate) fn authentication(
        endpoint: EndpointId,
        method: Method,
        error: crate::auth::AuthError,
    ) -> Self {
        let kind = error.kind();
        let category = match kind {
            AuthErrorKind::Unavailable => ErrorCategory::AuthenticationUnavailable,
            AuthErrorKind::Rejected => ErrorCategory::AuthenticationRejected,
            _ => ErrorCategory::AuthenticationProvider,
        };
        let mut result = Self::classified(endpoint, method, category);
        result.auth_kind = Some(kind);
        result.detail = Some(Box::new(ApiErrorDetail::Authentication(error)));
        result
    }

    pub(crate) fn rate_limit(
        endpoint: EndpointId,
        method: Method,
        error: crate::rate_limit::RateLimitError,
    ) -> Self {
        let kind = error.kind();
        let mut result = Self::classified(endpoint, method, ErrorCategory::RateLimit);
        result.rate_limit_kind = Some(kind);
        result.detail = Some(Box::new(ApiErrorDetail::RateLimit(error)));
        result
    }

    pub(crate) fn pagination(
        endpoint: EndpointId,
        method: Method,
        error: crate::pagination::PaginationError,
    ) -> Self {
        let kind = error.kind();
        let mut result = Self::classified(endpoint, method, ErrorCategory::Pagination);
        result.pagination_kind = Some(kind);
        result.detail = Some(Box::new(ApiErrorDetail::Pagination(error)));
        result
    }

    #[cfg(test)]
    fn with_auth_kind(endpoint: EndpointId, method: Method, kind: AuthErrorKind) -> Self {
        let category = match kind {
            AuthErrorKind::Unavailable => ErrorCategory::AuthenticationUnavailable,
            AuthErrorKind::Rejected => ErrorCategory::AuthenticationRejected,
            _ => ErrorCategory::AuthenticationProvider,
        };
        let mut error = Self::classified(endpoint, method, category);
        error.auth_kind = Some(kind);
        error
    }

    #[cfg(test)]
    fn with_rate_limit_kind(
        endpoint: EndpointId,
        method: Method,
        kind: RateLimitErrorKind,
    ) -> Self {
        let mut error = Self::classified(endpoint, method, ErrorCategory::RateLimit);
        error.rate_limit_kind = Some(kind);
        error
    }

    #[cfg(test)]
    fn with_pagination_kind(
        endpoint: EndpointId,
        method: Method,
        kind: PaginationErrorKind,
    ) -> Self {
        let mut error = Self::classified(endpoint, method, ErrorCategory::Pagination);
        error.pagination_kind = Some(kind);
        error
    }

    /// Returns the stable top-level category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the stable configuration kind when applicable.
    #[must_use]
    pub const fn configuration_kind(&self) -> Option<ConfigurationErrorKind> {
        self.configuration_kind
    }

    /// Returns the operational endpoint ID (an explicit `id` pin or navigation-derived).
    #[must_use]
    pub fn endpoint(&self) -> &str {
        self.endpoint.as_str()
    }

    /// Returns the typed operational endpoint ID (an explicit `id` pin or navigation-derived).
    #[must_use]
    pub const fn endpoint_id(&self) -> &EndpointId {
        &self.endpoint
    }

    /// Returns the application HTTP method.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns a final application HTTP status when a terminal rejected
    /// response wrapper exists.
    #[must_use]
    pub fn http_status(&self) -> Option<StatusCode> {
        self.http_response().map(HttpResponseFailure::status)
    }

    /// Returns sanitized final response headers when a terminal rejected
    /// response wrapper exists.
    #[must_use]
    pub fn http_headers(&self) -> Option<&SanitizedHeaderMap> {
        self.http_response().map(HttpResponseFailure::headers)
    }

    /// Returns the complete terminal response wrapper when one exists.
    #[must_use]
    pub fn http_response(&self) -> Option<&HttpResponseFailure> {
        match self.detail.as_deref() {
            Some(ApiErrorDetail::Response(response)) => Some(response),
            _ => None,
        }
    }

    /// Consumes this error and extracts its terminal response wrapper.
    pub fn into_http_response(mut self) -> Result<HttpResponseFailure, Self> {
        match self.detail.take().map(|detail| *detail) {
            Some(ApiErrorDetail::Response(response)) => Ok(response),
            Some(detail) => {
                self.detail = Some(Box::new(detail));
                Err(self)
            }
            None => Err(self),
        }
    }

    /// Returns the stable authentication kind when applicable.
    #[must_use]
    pub const fn auth_kind(&self) -> Option<AuthErrorKind> {
        self.auth_kind
    }

    /// Returns the stable rate-limit kind when applicable.
    #[must_use]
    pub const fn rate_limit_kind(&self) -> Option<RateLimitErrorKind> {
        self.rate_limit_kind
    }

    /// Returns the stable pagination kind when applicable.
    #[must_use]
    pub const fn pagination_kind(&self) -> Option<PaginationErrorKind> {
        self.pagination_kind
    }

    /// Returns the managed pipeline stage at which this outcome was fixed.
    ///
    /// For an error that escaped an operation this is the journal's settled
    /// stage. A low-level failure that was never contextualized reports the
    /// conservative `DuringDispatchOrUnknown`.
    #[must_use]
    pub fn failure_stage(&self) -> FailureStage {
        match self.journal.as_deref() {
            Some(journal) => journal.stage,
            None => self
                .stage_hint
                .unwrap_or(FailureStage::DuringDispatchOrUnknown),
        }
    }

    /// Alias of [`Self::failure_stage`].
    #[must_use]
    pub fn stage(&self) -> FailureStage {
        self.failure_stage()
    }

    /// Returns the operation's aggregate dispatch knowledge: the monotone
    /// join `NotDispatched < PossiblyDispatched < ResponseObserved` over every
    /// attempt (and, for a traversal error, every page).
    ///
    /// A low-level failure that was never contextualized conservatively
    /// reports at least `PossiblyDispatched`.
    #[must_use]
    pub fn dispatch_knowledge(&self) -> DispatchKnowledge {
        match self.journal.as_deref() {
            Some(journal) => journal.aggregate.join(self.knowledge_floor),
            None => self
                .knowledge_floor
                .join(DispatchKnowledge::PossiblyDispatched),
        }
    }

    /// Returns whether an application request of this operation may have
    /// reached the server, i.e. aggregate knowledge is not
    /// `NotDispatched`. This is not evidence of a business effect or success.
    #[must_use]
    pub fn may_have_reached_server(&self) -> bool {
        self.dispatch_knowledge().may_have_reached_server()
    }

    /// Returns the operation identity when execution allocated one.
    ///
    /// For a traversal page failure this is the page operation and
    /// [`Self::parent_operation_id`] is the traversal; for a traversal
    /// decision failure it is the traversal itself.
    #[must_use]
    pub fn operation_id(&self) -> Option<crate::operation::OperationId> {
        self.journal
            .as_deref()
            .and_then(|journal| journal.operation_id)
    }

    /// Returns the parent aggregate (traversal) identity of a page operation.
    #[must_use]
    pub fn parent_operation_id(&self) -> Option<crate::operation::OperationId> {
        self.journal
            .as_deref()
            .and_then(|journal| journal.parent_operation_id)
    }

    /// Returns the journal facts of the credential-provider acquisition this
    /// failure is linked to (EXEC-01).
    ///
    /// For a provider-caused authentication failure these are the facts of
    /// the acquisition that failed. Otherwise they belong to the latest
    /// acquisition this operation led or joined before it failed. The
    /// acquisition has its own identity,
    /// [`ProviderAcquisitionFacts::operation_id`], which differs from
    /// [`Self::operation_id`]. Its
    /// [`parent_operation_id`](ProviderAcquisitionFacts::parent_operation_id)
    /// names the operation that led it. For a follower that joined another
    /// operation's acquisition, that parent is the leader, not this
    /// operation. The provider's helper traffic is journaled separately, so
    /// this error's own [`Self::dispatch_knowledge`] never includes provider
    /// network activity. `None` when no provider acquisition was involved.
    #[must_use]
    pub fn provider_acquisition(&self) -> Option<ProviderAcquisitionFacts> {
        let from_failure = match self.detail.as_deref() {
            Some(ApiErrorDetail::Authentication(error)) => error.provider_acquisition(),
            _ => None,
        };
        from_failure.or_else(|| self.journal.as_deref().and_then(|journal| journal.provider))
    }

    /// Returns facts about the last application-visible attempt, separate
    /// from the aggregate knowledge.
    #[must_use]
    pub fn last_attempt(&self) -> Option<AttemptFacts> {
        self.journal
            .as_deref()
            .and_then(|journal| journal.last_attempt)
    }

    /// Returns the visible managed attempt identity when applicable.
    #[must_use]
    pub fn attempt(&self) -> Option<crate::hooks::AttemptInfo> {
        self.last_attempt().map(|facts| facts.attempt())
    }

    /// Returns the zero-based page index when this was a pagination page or
    /// traversal failure.
    #[must_use]
    pub fn page_index(&self) -> Option<u32> {
        self.journal
            .as_deref()
            .and_then(|journal| journal.page_index)
    }

    /// Returns the operation journal's value-free work counts, when the
    /// failure escaped a started operation.
    #[must_use]
    pub fn operation_counts(&self) -> Option<OperationCounts> {
        self.journal.as_deref().and_then(|journal| journal.counts)
    }

    /// Returns the status of the last application response head observed
    /// before this failure, if any.
    ///
    /// For a terminal rejected response this equals [`Self::http_status`].
    /// For a failure after an accepted head (a truncated body, a decode
    /// failure, a failed local credential installation) it retains the
    /// received status even though no terminal response wrapper exists. A
    /// later attempt's failure never clears it. A status is response
    /// metadata, not proof of a business effect.
    #[must_use]
    pub fn last_response_status(&self) -> Option<StatusCode> {
        self.journal
            .as_deref()
            .and_then(|journal| journal.last_status)
            .or_else(|| self.http_status())
    }

    /// Returns the value-free traversal summary when this error escaped a
    /// pagination traversal (`pages`, `collect_prefix` or `collect_all`).
    ///
    /// The summary is fixed at the moment the error was produced: it reports
    /// the stop reason (for example `Failed`, `Cancelled`, or `PageBound` for a
    /// `collect_all` completion-limit error), the pages received and delivered,
    /// the aggregate attempt/byte accounting and aggregate dispatch knowledge.
    #[must_use]
    pub fn traversal_summary(&self) -> Option<&crate::pagination::TraversalSummary> {
        self.traversal.as_deref()
    }

    /// Returns the value-free stream summary when this error was yielded by a
    /// managed `ByteStream` or `ItemStream` after its accepted head was
    /// handed to the caller.
    ///
    /// The summary is a snapshot fixed when the error was produced: it
    /// reports the terminal state (`Failed` or `Cancelled`), the structural
    /// error kind, the original operation, attempt and status, and the bytes
    /// and records accounted so far.
    #[must_use]
    pub fn stream_summary(&self) -> Option<&crate::response::StreamSummary> {
        match self.detail.as_deref() {
            Some(ApiErrorDetail::Stream(summary)) => Some(summary),
            _ => None,
        }
    }

    /// Attaches the stream journal snapshot to a stream-consumption error.
    ///
    /// Only called on errors built by the managed stream itself, which carry
    /// no other detail; an existing detail is never replaced.
    pub(crate) fn with_stream_summary(mut self, summary: crate::response::StreamSummary) -> Self {
        if self.detail.is_none() {
            self.detail = Some(Box::new(ApiErrorDetail::Stream(summary)));
        }
        self
    }

    /// Returns safe parsed terminal `Retry-After` metadata when present.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        self.http_response()
            .and_then(HttpResponseFailure::retry_after)
    }

    /// Records the precise stage named by the code fixing this failure and a
    /// positive dispatch fact it knows (`ResponseObserved` after a received
    /// head). `NotDispatched` adds nothing: knowledge only ever rises. When a
    /// journal is already attached, its stage is replaced and its aggregate
    /// joined.
    pub(crate) fn with_stage(mut self, stage: FailureStage, known: DispatchKnowledge) -> Self {
        self.stage_hint = Some(stage);
        self.knowledge_floor = self.knowledge_floor.join(known);
        if let Some(journal) = self.journal.as_deref_mut() {
            journal.stage = stage;
            journal.aggregate = journal.aggregate.join(known);
        }
        self
    }

    /// Returns the precise stage hint recorded by the code that fixed this
    /// failure, if any.
    pub(crate) const fn stage_hint(&self) -> Option<FailureStage> {
        self.stage_hint
    }

    /// Settles journal context. A precise stage hint recorded by the failing
    /// code wins over the journal's coarser current phase; the error's known
    /// positive dispatch fact is joined into the aggregate.
    pub(crate) fn with_journal(mut self, mut journal: ErrorJournal) -> Self {
        if let Some(stage) = self.stage_hint {
            journal.stage = stage;
        }
        journal.aggregate = journal.aggregate.join(self.knowledge_floor);
        self.journal = Some(Box::new(journal));
        self
    }

    /// Settles the context of a failure fixed before any operation identity
    /// existed, or during planning (which never dispatches).
    pub(crate) fn before_operation(self) -> Self {
        let stage = self.stage_hint.unwrap_or(FailureStage::Preparation);
        self.with_journal(ErrorJournal::before_operation(stage))
    }

    /// Returns whether journal context was already settled.
    pub(crate) const fn is_contextualized(&self) -> bool {
        self.journal.is_some()
    }

    pub(crate) fn with_traversal_summary(
        mut self,
        summary: crate::pagination::TraversalSummary,
    ) -> Self {
        let aggregate = summary.dispatch_knowledge();
        self.knowledge_floor = self.knowledge_floor.join(aggregate);
        if let Some(journal) = self.journal.as_deref_mut() {
            journal.aggregate = journal.aggregate.join(aggregate);
        }
        self.traversal = Some(Box::new(summary));
        self
    }
}

pub(crate) fn reqwest_category(error: reqwest::Error, managed_body_failed: bool) -> ErrorCategory {
    let category = if managed_body_failed {
        ErrorCategory::RequestBody
    } else if error.is_timeout() {
        ErrorCategory::Timeout
    } else if error.is_connect() {
        ErrorCategory::Connect
    } else if error.is_body() {
        ErrorCategory::RequestBody
    } else {
        ErrorCategory::Transport
    };
    drop(error);
    category
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "endpoint {} {} failed ({:?})",
            self.method, self.endpoint, self.category
        )?;
        if let Some(status) = self.http_status() {
            write!(formatter, " with HTTP status {status}")?;
        }
        let safe_message = match self.detail.as_deref() {
            Some(ApiErrorDetail::Authentication(error)) => error.safe_message(),
            Some(ApiErrorDetail::RateLimit(error)) => error.safe_message(),
            Some(ApiErrorDetail::Pagination(error)) => error.safe_message(),
            Some(ApiErrorDetail::Response(_) | ApiErrorDetail::Stream(_)) | None => None,
        };
        if let Some(message) = safe_message {
            write!(formatter, ": {message}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiError")
            .field("endpoint", &self.endpoint)
            .field("method", &self.method)
            .field("category", &self.category)
            .field("configuration_kind", &self.configuration_kind)
            .field("auth_kind", &self.auth_kind)
            .field("rate_limit_kind", &self.rate_limit_kind)
            .field("pagination_kind", &self.pagination_kind)
            .field("failure_stage", &self.failure_stage())
            .field("dispatch_knowledge", &self.dispatch_knowledge())
            .field("operation_id", &self.operation_id())
            .field("parent_operation_id", &self.parent_operation_id())
            .field("provider_acquisition", &self.provider_acquisition())
            .field("last_attempt", &self.last_attempt())
            .field("operation_counts", &self.operation_counts())
            .field("http_status", &self.http_status())
            .field("last_response_status", &self.last_response_status())
            .field(
                "authentication_error",
                &match self.detail.as_deref() {
                    Some(ApiErrorDetail::Authentication(error)) => Some(error),
                    _ => None,
                },
            )
            .field(
                "rate_limit_error",
                &match self.detail.as_deref() {
                    Some(ApiErrorDetail::RateLimit(error)) => Some(error),
                    _ => None,
                },
            )
            .field(
                "pagination_error",
                &match self.detail.as_deref() {
                    Some(ApiErrorDetail::Pagination(error)) => Some(error),
                    _ => None,
                },
            )
            .field("traversal", &self.traversal)
            .field("stream", &self.stream_summary())
            .finish()
    }
}

impl Error for ApiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.detail.as_deref() {
            Some(ApiErrorDetail::Authentication(error)) => Some(error),
            Some(ApiErrorDetail::RateLimit(error)) => Some(error),
            Some(ApiErrorDetail::Pagination(error)) => Some(error),
            Some(ApiErrorDetail::Response(_) | ApiErrorDetail::Stream(_)) | None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;
    use std::net::TcpListener;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll};

    use bytes::Bytes;
    use concord_test_support::server::{ResponseScript, ScriptedServer};
    use http::{HeaderMap, HeaderValue};

    use super::*;
    use crate::error::StatusBody;

    const BODY_SENTINEL: &str = "terminal-body-private-01e08b";
    const HEADER_SENTINEL: &str = "auth-header-private-d9012a";
    const URL_SENTINEL: &str = "native-url-private-732fee";

    fn response_error() -> ApiError {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_static(HEADER_SENTINEL),
        );
        let response = HttpResponseFailure::new(
            EndpointId::from_static("widgets.fetch"),
            Method::GET,
            StatusCode::BAD_GATEWAY,
            headers,
            StatusBody::complete(Bytes::from_static(BODY_SENTINEL.as_bytes())),
            Duration::from_secs(60),
        );
        ApiError::from_http_response(response)
    }

    fn assert_recursive_sources_are_safe(error: &(dyn Error + 'static), sentinels: &[&str]) {
        let mut current = Some(error);
        while let Some(error) = current {
            for formatted in [error.to_string(), format!("{error:?}")] {
                for sentinel in sentinels {
                    assert!(!formatted.contains(sentinel));
                }
            }
            current = error.source();
        }
    }

    #[test]
    fn structured_accessors_and_owning_extraction_preserve_response() {
        let error = response_error();
        assert_eq!(error.category(), ErrorCategory::HttpStatus);
        assert_eq!(error.endpoint(), "widgets.fetch");
        assert_eq!(error.method(), Method::GET);
        assert_eq!(error.http_status(), Some(StatusCode::BAD_GATEWAY));
        assert!(error.http_headers().is_some());
        assert!(error.http_response().is_some());
        assert_eq!(error.auth_kind(), None);
        assert_eq!(error.rate_limit_kind(), None);
        assert_eq!(error.pagination_kind(), None);
        assert_recursive_sources_are_safe(&error, &[BODY_SENTINEL, HEADER_SENTINEL]);

        let response = error
            .into_http_response()
            .expect("HTTP response should extract");
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            response.into_body().into_bytes().as_deref(),
            Some(BODY_SENTINEL.as_bytes())
        );
    }

    #[test]
    fn non_response_error_is_returned_unchanged_by_owning_extraction() {
        let error = ApiError::classified(
            EndpointId::from_static("widgets.fetch"),
            Method::POST,
            ErrorCategory::Internal,
        );
        let error = error
            .into_http_response()
            .expect_err("non-response error should be returned");
        assert_eq!(error.category(), ErrorCategory::Internal);
        assert_eq!(error.method(), Method::POST);
    }

    #[test]
    fn subsystem_kinds_are_machine_readable_without_messages() {
        let auth = ApiError::with_auth_kind(
            EndpointId::from_static("widgets.fetch"),
            Method::GET,
            AuthErrorKind::Acquisition,
        );
        let rate = ApiError::with_rate_limit_kind(
            EndpointId::from_static("widgets.fetch"),
            Method::GET,
            RateLimitErrorKind::Acquire,
        );
        let pagination = ApiError::with_pagination_kind(
            EndpointId::from_static("widgets.fetch"),
            Method::GET,
            PaginationErrorKind::RepeatedRequest,
        );
        assert_eq!(auth.auth_kind(), Some(AuthErrorKind::Acquisition));
        assert_eq!(rate.rate_limit_kind(), Some(RateLimitErrorKind::Acquire));
        assert_eq!(
            pagination.pagination_kind(),
            Some(PaginationErrorKind::RepeatedRequest)
        );
    }

    #[test]
    fn provider_owned_safe_message_remains_application_visible() {
        const SAFE_MESSAGE: &str = "operator-approved credential broker outage";
        let error = ApiError::authentication(
            EndpointId::from_static("widgets.fetch"),
            Method::GET,
            crate::auth::AuthError::with_safe_message(AuthErrorKind::Acquisition, SAFE_MESSAGE),
        );

        assert!(error.to_string().contains(SAFE_MESSAGE));
        assert!(format!("{error:?}").contains(SAFE_MESSAGE));
        let source = error
            .source()
            .expect("authentication source should remain visible");
        assert!(source.to_string().contains(SAFE_MESSAGE));
        assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    }

    #[test]
    fn pagination_ceiling_kinds_keep_structural_category_and_safe_formatting() {
        for kind in [
            PaginationErrorKind::CompletionLimitExceeded,
            PaginationErrorKind::AttemptCeilingExceeded,
            PaginationErrorKind::CollectedItemCeilingExceeded,
            PaginationErrorKind::CollectedResponseByteCeilingExceeded,
            PaginationErrorKind::IdentityStorageCeilingExceeded,
        ] {
            let error = ApiError::with_pagination_kind(
                EndpointId::from_static("widgets.list"),
                Method::GET,
                kind,
            );
            assert_eq!(error.category(), ErrorCategory::Pagination);
            assert_eq!(error.pagination_kind(), Some(kind));
            assert_eq!(
                error.to_string(),
                "endpoint GET widgets.list failed (Pagination)"
            );
            assert!(format!("{error:?}").contains(&format!("{kind:?}")));
        }
    }

    #[tokio::test]
    async fn raw_reqwest_url_error_is_reduced_without_retaining_native_error() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let reserved = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("closed-port reservation should bind");
        let address = reserved.local_addr().expect("address should exist");
        drop(reserved);
        let native_url = format!(
            "http://user-{URL_SENTINEL}:password-{URL_SENTINEL}@{address}/private?token={URL_SENTINEL}"
        );
        let native = reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .build()
            .expect("client should build")
            .get(native_url)
            .send()
            .await
            .expect_err("closed port should fail");
        assert!(
            native
                .url()
                .is_some_and(|url| url.as_str().contains(URL_SENTINEL)),
            "the native Reqwest error must carry the authenticated URL sentinel before reduction"
        );
        let reduced = ApiError::from_reqwest(
            EndpointId::from_static("widgets.fetch"),
            Method::GET,
            native,
            false,
        );
        assert!(matches!(
            reduced.category(),
            ErrorCategory::Connect | ErrorCategory::Transport
        ));
        // A native connect classification is not proof of no submission.
        assert_ne!(
            reduced.dispatch_knowledge(),
            DispatchKnowledge::NotDispatched
        );
        assert_recursive_sources_are_safe(&reduced, &[URL_SENTINEL]);
    }

    #[derive(Debug)]
    struct FailingBodyError;

    impl fmt::Display for FailingBodyError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(BODY_SENTINEL)
        }
    }

    impl Error for FailingBodyError {}

    struct FailingBody {
        failed: Arc<AtomicBool>,
    }

    impl http_body::Body for FailingBody {
        type Data = Bytes;
        type Error = FailingBodyError;

        fn poll_frame(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            self.failed.store(true, Ordering::SeqCst);
            Poll::Ready(Some(Err(FailingBodyError)))
        }

        fn size_hint(&self) -> http_body::SizeHint {
            let mut hint = http_body::SizeHint::new();
            hint.set_lower(1);
            hint
        }
    }

    #[tokio::test]
    async fn raw_reqwest_body_error_is_reduced_without_retaining_source_text() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let server = ScriptedServer::start([ResponseScript::ok(b"unused".to_vec())])
            .expect("body-error server should start");
        let body_failed = Arc::new(AtomicBool::new(false));
        let native = reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .build()
            .expect("client should build")
            .post(server.url("/body"))
            .body(reqwest::Body::wrap(FailingBody {
                failed: Arc::clone(&body_failed),
            }))
            .send()
            .await
            .expect_err("failing request body should fail");
        assert!(
            body_failed.load(Ordering::SeqCst),
            "the sentinel-bearing body source must actually have failed"
        );
        assert!(FailingBodyError.to_string().contains(BODY_SENTINEL));
        let reduced = ApiError::from_reqwest(
            EndpointId::from_static("widgets.create"),
            Method::POST,
            native,
            true,
        );
        assert_eq!(reduced.category(), ErrorCategory::RequestBody);
        assert_recursive_sources_are_safe(&reduced, &[BODY_SENTINEL]);
    }

    fn widget_error(category: ErrorCategory) -> ApiError {
        ApiError::classified(
            EndpointId::from_static("widgets.write"),
            Method::POST,
            category,
        )
    }

    #[test]
    fn exec_02_low_level_errors_never_claim_no_dispatch_before_contextualization() {
        for category in [
            ErrorCategory::Connect,
            ErrorCategory::Transport,
            ErrorCategory::Timeout,
            ErrorCategory::Internal,
        ] {
            let error = widget_error(category);
            assert!(!error.is_contextualized());
            assert_eq!(
                error.dispatch_knowledge(),
                DispatchKnowledge::PossiblyDispatched
            );
            assert!(error.may_have_reached_server());
            assert_eq!(error.failure_stage(), FailureStage::DuringDispatchOrUnknown);
            assert_eq!(error.operation_id(), None);
            assert_eq!(error.last_attempt(), None);
        }
        // A rejected response wrapper carries its positive observed fact.
        let rejected = response_error();
        assert_eq!(
            rejected.dispatch_knowledge(),
            DispatchKnowledge::ResponseObserved
        );
        assert_eq!(rejected.failure_stage(), FailureStage::RejectedResponse);
    }

    #[test]
    fn exec_01_journal_context_is_settled_and_precise_stage_hints_win() {
        let operation_id = crate::operation::OperationId::allocate().expect("operation");
        let parent = crate::operation::OperationId::allocate().expect("parent");
        let attempt = AttemptFacts::new(crate::hooks::AttemptInfo {
            index: 1,
            policy_retry_index: 1,
            authentication_recovery: false,
        });
        let journal = ErrorJournal {
            operation_id: Some(operation_id),
            parent_operation_id: Some(parent),
            page_index: Some(3),
            stage: FailureStage::DuringDispatchOrUnknown,
            aggregate: DispatchKnowledge::ResponseObserved,
            last_attempt: Some(attempt),
            last_status: Some(StatusCode::SERVICE_UNAVAILABLE),
            counts: Some(OperationCounts {
                attempts_started: 2,
                application_handoffs: 2,
                quota_admissions: 2,
                policy_retries: 1,
                authentication_recoveries: 0,
                freshness_restarts: 0,
            }),
            provider: None,
        };
        let error = widget_error(ErrorCategory::Connect).with_journal(journal);
        assert!(error.is_contextualized());
        assert_eq!(error.operation_id(), Some(operation_id));
        assert_eq!(error.parent_operation_id(), Some(parent));
        assert_eq!(error.page_index(), Some(3));
        assert_eq!(
            error.dispatch_knowledge(),
            DispatchKnowledge::ResponseObserved
        );
        assert_eq!(
            error.last_attempt().map(|facts| facts.dispatch_knowledge()),
            Some(DispatchKnowledge::NotDispatched)
        );
        assert_eq!(
            error.last_response_status(),
            Some(StatusCode::SERVICE_UNAVAILABLE)
        );
        assert_eq!(
            error.operation_counts().map(|c| c.automatic_replays()),
            Some(1)
        );

        // A precise hint from the failing code wins over the journal phase,
        // and its positive knowledge is joined into the aggregate.
        let converted = widget_error(ErrorCategory::ResponseDecoding)
            .with_stage(
                FailureStage::ResponseConversion,
                DispatchKnowledge::ResponseObserved,
            )
            .with_journal(ErrorJournal {
                stage: FailureStage::ResponseAcquisition,
                aggregate: DispatchKnowledge::PossiblyDispatched,
                ..journal
            });
        assert_eq!(converted.failure_stage(), FailureStage::ResponseConversion);
        assert_eq!(
            converted.dispatch_knowledge(),
            DispatchKnowledge::ResponseObserved
        );

        // Formatting remains value-free and shows opaque identities only.
        let rendered = format!("{error:?}");
        assert!(rendered.contains("OperationId(<opaque>)"));
        assert!(rendered.contains("ResponseObserved"));
    }

    #[test]
    fn pre_operation_context_is_preparation_not_dispatched_without_identity() {
        let error = widget_error(ErrorCategory::Configuration).before_operation();
        assert_eq!(error.failure_stage(), FailureStage::Preparation);
        assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
        assert!(!error.may_have_reached_server());
        assert_eq!(error.operation_id(), None);
        assert_eq!(error.operation_counts(), None);

        let cancelled = widget_error(ErrorCategory::Cancelled)
            .with_stage(
                FailureStage::BeforeDispatch,
                DispatchKnowledge::NotDispatched,
            )
            .before_operation();
        assert_eq!(cancelled.failure_stage(), FailureStage::BeforeDispatch);
        assert_eq!(
            cancelled.dispatch_knowledge(),
            DispatchKnowledge::NotDispatched
        );
    }

    #[test]
    fn dispatch_knowledge_join_is_monotone() {
        use DispatchKnowledge::{NotDispatched, PossiblyDispatched, ResponseObserved};
        let all = [NotDispatched, PossiblyDispatched, ResponseObserved];
        for (left_rank, left) in all.iter().enumerate() {
            for (right_rank, right) in all.iter().enumerate() {
                assert_eq!(left.join(*right), all[left_rank.max(right_rank)]);
            }
        }
        assert!(!NotDispatched.may_have_reached_server());
        assert!(PossiblyDispatched.may_have_reached_server());
        assert!(ResponseObserved.may_have_reached_server());
    }
}
