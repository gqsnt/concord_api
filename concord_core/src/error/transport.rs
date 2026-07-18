use std::error::Error;
use std::fmt;
use std::time::Duration;

use http::{Method, StatusCode};

use crate::redaction::SanitizedHeaderMap;

use super::{
    AuthErrorKind, ConfigurationErrorKind, ErrorCategory, HttpResponseFailure, PaginationErrorKind,
    RateLimitErrorKind,
};

/// A structured safe failure from an endpoint operation.
pub struct ApiError {
    endpoint: &'static str,
    method: Method,
    category: ErrorCategory,
    configuration_kind: Option<ConfigurationErrorKind>,
    auth_kind: Option<AuthErrorKind>,
    rate_limit_kind: Option<RateLimitErrorKind>,
    pagination_kind: Option<PaginationErrorKind>,
    response: Option<Box<HttpResponseFailure>>,
    authentication_error: Option<Box<crate::auth::AuthError>>,
    rate_limit_error: Option<Box<crate::rate_limit::RateLimitError>>,
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
        let endpoint = response.endpoint();
        let method = response.method().clone();
        Self {
            endpoint,
            method,
            category,
            configuration_kind: None,
            auth_kind,
            rate_limit_kind: None,
            pagination_kind: None,
            response: Some(Box::new(response)),
            authentication_error: None,
            rate_limit_error: None,
        }
    }

    pub(crate) fn from_reqwest(
        endpoint: &'static str,
        method: Method,
        error: reqwest::Error,
        managed_body_failed: bool,
    ) -> Self {
        let category = reqwest_category(error, managed_body_failed);
        Self::classified(endpoint, method, category)
    }

    pub(crate) fn classified(
        endpoint: &'static str,
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
            response: None,
            authentication_error: None,
            rate_limit_error: None,
        }
    }

    pub(crate) fn configuration(
        endpoint: &'static str,
        method: Method,
        kind: ConfigurationErrorKind,
    ) -> Self {
        let mut error = Self::classified(endpoint, method, ErrorCategory::Configuration);
        error.configuration_kind = Some(kind);
        error
    }

    pub(crate) fn authentication(
        endpoint: &'static str,
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
        result.authentication_error = Some(Box::new(error));
        result
    }

    pub(crate) fn rate_limit(
        endpoint: &'static str,
        method: Method,
        error: crate::rate_limit::RateLimitError,
    ) -> Self {
        let kind = error.kind();
        let mut result = Self::classified(endpoint, method, ErrorCategory::RateLimit);
        result.rate_limit_kind = Some(kind);
        result.rate_limit_error = Some(Box::new(error));
        result
    }

    #[cfg(test)]
    fn with_auth_kind(endpoint: &'static str, method: Method, kind: AuthErrorKind) -> Self {
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
        endpoint: &'static str,
        method: Method,
        kind: RateLimitErrorKind,
    ) -> Self {
        let mut error = Self::classified(endpoint, method, ErrorCategory::RateLimit);
        error.rate_limit_kind = Some(kind);
        error
    }

    #[cfg(test)]
    fn with_pagination_kind(
        endpoint: &'static str,
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

    /// Returns the stable generated endpoint identity.
    #[must_use]
    pub const fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    /// Returns the application HTTP method.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns a final application HTTP status when a response head existed.
    #[must_use]
    pub fn http_status(&self) -> Option<StatusCode> {
        self.response.as_deref().map(HttpResponseFailure::status)
    }

    /// Returns sanitized final response headers when a response head existed.
    #[must_use]
    pub fn http_headers(&self) -> Option<&SanitizedHeaderMap> {
        self.response.as_deref().map(HttpResponseFailure::headers)
    }

    /// Returns the complete terminal response wrapper when one exists.
    #[must_use]
    pub fn http_response(&self) -> Option<&HttpResponseFailure> {
        self.response.as_deref()
    }

    /// Consumes this error and extracts its terminal response wrapper.
    pub fn into_http_response(mut self) -> Result<HttpResponseFailure, Self> {
        match self.response.take() {
            Some(response) => Ok(*response),
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

    /// Returns safe parsed terminal `Retry-After` metadata when present.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        self.response
            .as_deref()
            .and_then(HttpResponseFailure::retry_after)
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
        if let Some(message) = self
            .authentication_error
            .as_deref()
            .and_then(crate::auth::AuthError::safe_message)
        {
            write!(formatter, ": {message}")?;
        }
        if let Some(message) = self
            .rate_limit_error
            .as_deref()
            .and_then(crate::rate_limit::RateLimitError::safe_message)
        {
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
            .field("http_status", &self.http_status())
            .field("authentication_error", &self.authentication_error)
            .field("rate_limit_error", &self.rate_limit_error)
            .finish()
    }
}

impl Error for ApiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.authentication_error
            .as_deref()
            .map(|error| error as &(dyn Error + 'static))
            .or_else(|| {
                self.rate_limit_error
                    .as_deref()
                    .map(|error| error as &(dyn Error + 'static))
            })
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
            "widgets.fetch",
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
        let error = ApiError::classified("widgets.fetch", Method::POST, ErrorCategory::Internal);
        let error = error
            .into_http_response()
            .expect_err("non-response error should be returned");
        assert_eq!(error.category(), ErrorCategory::Internal);
        assert_eq!(error.method(), Method::POST);
    }

    #[test]
    fn subsystem_kinds_are_machine_readable_without_messages() {
        let auth =
            ApiError::with_auth_kind("widgets.fetch", Method::GET, AuthErrorKind::Acquisition);
        let rate = ApiError::with_rate_limit_kind(
            "widgets.fetch",
            Method::GET,
            RateLimitErrorKind::Acquire,
        );
        let pagination = ApiError::with_pagination_kind(
            "widgets.fetch",
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
            "widgets.fetch",
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
            PaginationErrorKind::PageCeilingExceeded,
            PaginationErrorKind::CollectedItemCeilingExceeded,
            PaginationErrorKind::IdentityStorageCeilingExceeded,
        ] {
            let error = ApiError::with_pagination_kind("widgets.list", Method::GET, kind);
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
        let reduced = ApiError::from_reqwest("widgets.fetch", Method::GET, native, false);
        assert!(matches!(
            reduced.category(),
            ErrorCategory::Connect | ErrorCategory::Transport
        ));
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
        let reduced = ApiError::from_reqwest("widgets.create", Method::POST, native, true);
        assert_eq!(reduced.category(), ErrorCategory::RequestBody);
        assert_recursive_sources_are_safe(&reduced, &[BODY_SENTINEL]);
    }
}
