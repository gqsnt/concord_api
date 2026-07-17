use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue, Method};
use url::Url;

use super::RequestOptions;
use crate::error::{ApiError, ConfigurationErrorKind};
use crate::hooks::DebugLevel;
use crate::redaction::render_safe_url;

/// Static generated metadata required before public-head preparation.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct RequestMeta {
    endpoint: &'static str,
    method: Method,
    inherited_timeout: Option<Duration>,
}

impl RequestMeta {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        endpoint: &'static str,
        method: Method,
        inherited_timeout: Option<Duration>,
    ) -> Self {
        Self {
            endpoint,
            method,
            inherited_timeout,
        }
    }

    #[doc(hidden)]
    pub const fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    #[doc(hidden)]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    #[doc(hidden)]
    pub const fn inherited_timeout(&self) -> Option<Duration> {
        self.inherited_timeout
    }
}

/// Generated-private structured builder for one public request head.
#[doc(hidden)]
pub struct HeadBuilder {
    meta: RequestMeta,
    logical_url: Url,
    timeout: Option<Duration>,
    debug: DebugLevel,
    appended_segment: bool,
}

impl HeadBuilder {
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        base: &Url,
        meta: RequestMeta,
        options: &RequestOptions,
        inherited_debug: DebugLevel,
    ) -> Self {
        Self {
            timeout: options.resolve_timeout(meta.inherited_timeout()),
            debug: options.resolve_debug(inherited_debug),
            meta,
            logical_url: base.clone(),
            appended_segment: false,
        }
    }

    /// Appends one macro-validated static path segment.
    #[doc(hidden)]
    pub fn push_static(&mut self, segment: &str) -> Result<(), ApiError> {
        if segment.contains('/') || segment.contains('\\') {
            return Err(invalid_route(self.meta.endpoint(), self.meta.method()));
        }
        self.push_segment(segment)
    }

    /// Formats and appends one dynamic value as exactly one path segment.
    #[doc(hidden)]
    pub fn push_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let segment = value.to_string();
        self.push_segment(&segment)
    }

    fn push_segment(&mut self, segment: &str) -> Result<(), ApiError> {
        if segment.is_empty() || matches!(segment, "." | "..") {
            return Err(invalid_route(self.meta.endpoint(), self.meta.method()));
        }
        let mut path = self
            .logical_url
            .path_segments_mut()
            .map_err(|()| invalid_route(self.meta.endpoint(), self.meta.method()))?;
        if !self.appended_segment {
            path.pop_if_empty();
        }
        path.push(segment);
        self.appended_segment = true;
        Ok(())
    }

    /// Finishes one immutable body-free public head.
    #[doc(hidden)]
    #[must_use]
    pub fn finish(self) -> PreparedHead {
        let safe_url: Arc<str> = render_safe_url(&self.logical_url, &HashSet::new()).into();
        PreparedHead {
            meta: self.meta,
            logical_url: self.logical_url,
            safe_url,
            public_headers: HeaderMap::new(),
            timeout: self.timeout,
            debug: self.debug,
        }
    }
}

/// Immutable public request facts prepared only after first poll.
#[doc(hidden)]
pub struct PreparedHead {
    meta: RequestMeta,
    logical_url: Url,
    safe_url: Arc<str>,
    public_headers: HeaderMap,
    timeout: Option<Duration>,
    debug: DebugLevel,
}

impl PreparedHead {
    pub(crate) fn apply_media_headers(
        &mut self,
        accept: Option<&'static str>,
        content_type: Option<&'static str>,
    ) -> Result<(), ApiError> {
        if let Some(accept) = accept {
            if !self.public_headers.contains_key(ACCEPT) {
                let value = HeaderValue::from_str(accept).map_err(|_| {
                    ApiError::configuration(
                        self.endpoint(),
                        self.method().clone(),
                        ConfigurationErrorKind::InvalidHeader,
                    )
                })?;
                self.public_headers.insert(ACCEPT, value);
            }
        }
        if let Some(content_type) = content_type {
            if self.public_headers.contains_key(CONTENT_TYPE) {
                return Err(ApiError::configuration(
                    self.endpoint(),
                    self.method().clone(),
                    ConfigurationErrorKind::InvalidHeader,
                ));
            }
            let value = HeaderValue::from_str(content_type).map_err(|_| {
                ApiError::configuration(
                    self.endpoint(),
                    self.method().clone(),
                    ConfigurationErrorKind::InvalidHeader,
                )
            })?;
            self.public_headers.insert(CONTENT_TYPE, value);
        }
        Ok(())
    }

    #[doc(hidden)]
    pub const fn meta(&self) -> &RequestMeta {
        &self.meta
    }

    #[doc(hidden)]
    pub const fn method(&self) -> &Method {
        self.meta.method()
    }

    #[doc(hidden)]
    pub const fn endpoint(&self) -> &'static str {
        self.meta.endpoint()
    }

    #[doc(hidden)]
    pub const fn logical_url(&self) -> &Url {
        &self.logical_url
    }

    #[doc(hidden)]
    pub fn safe_url(&self) -> &str {
        &self.safe_url
    }

    #[doc(hidden)]
    pub const fn public_headers(&self) -> &HeaderMap {
        &self.public_headers
    }

    #[doc(hidden)]
    pub const fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    #[doc(hidden)]
    pub const fn debug(&self) -> DebugLevel {
        self.debug
    }
}

fn invalid_route(endpoint: &'static str, method: &Method) -> ApiError {
    ApiError::configuration(
        endpoint,
        method.clone(),
        ConfigurationErrorKind::InvalidRoute,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{ConfigurationErrorKind, ErrorCategory};

    fn prepared() -> PreparedHead {
        HeadBuilder::new(
            &Url::parse("https://headers.example.test/").expect("base URL"),
            RequestMeta::new("HeaderApi::records::create", Method::POST, None),
            &RequestOptions::new(),
            DebugLevel::None,
        )
        .finish()
    }

    #[test]
    fn codec_media_uses_the_single_public_header_map_with_owned_precedence() {
        let mut head = prepared();
        head.apply_media_headers(
            Some("application/vnd.example+json"),
            Some("application/json"),
        )
        .expect("codec headers");
        assert_eq!(
            head.public_headers().get(ACCEPT),
            Some(&HeaderValue::from_static("application/vnd.example+json"))
        );
        assert_eq!(
            head.public_headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );

        head.apply_media_headers(Some("text/plain"), None)
            .expect("an existing Accept value wins");
        assert_eq!(
            head.public_headers().get(ACCEPT),
            Some(&HeaderValue::from_static("application/vnd.example+json"))
        );

        let error = head
            .apply_media_headers(None, Some("text/plain"))
            .expect_err("a request codec owns Content-Type");
        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
    }

    #[test]
    fn invalid_static_codec_header_values_fail_during_public_preparation() {
        for (accept, content_type) in [
            (Some("invalid\r\naccept"), None),
            (None, Some("invalid\r\ncontent-type")),
        ] {
            let error = prepared()
                .apply_media_headers(accept, content_type)
                .expect_err("invalid codec header value");
            assert_eq!(error.category(), ErrorCategory::Configuration);
            assert_eq!(
                error.configuration_kind(),
                Some(ConfigurationErrorKind::InvalidHeader)
            );
        }
    }
}
