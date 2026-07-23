use std::collections::HashSet;
use std::fmt::Display;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use url::{Host, Url};

use super::RequestOptions;
use crate::auth::{Auth, AuthPlan};
use crate::error::{ApiError, ConfigurationErrorKind};
use crate::hooks::DebugLevel;
use crate::rate_limit::{
    RateLimitError, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitPlan, RateLimitRule,
    RateLimitWindow,
};
use crate::redaction::render_safe_url;

/// Static generated metadata required before public-head preparation.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct RequestMeta {
    endpoint: &'static str,
    method: Method,
    inherited_timeout: Option<Duration>,
    inherit_client_timeout: bool,
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
            inherit_client_timeout: false,
        }
    }

    /// Creates metadata for a runtime endpoint that inherits the built client's
    /// operation timeout.
    #[doc(hidden)]
    #[must_use]
    pub const fn inheriting_client_timeout(endpoint: &'static str, method: Method) -> Self {
        Self {
            endpoint,
            method,
            inherited_timeout: None,
            inherit_client_timeout: true,
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

    pub(crate) const fn resolve_inherited_timeout(
        &self,
        client_timeout: Option<Duration>,
    ) -> Option<Duration> {
        if self.inherit_client_timeout {
            client_timeout
        } else {
            self.inherited_timeout
        }
    }
}

/// Generated-private structured builder for one public request head.
#[doc(hidden)]
pub struct HeadBuilder {
    meta: RequestMeta,
    logical_url: Url,
    safe_logical_url: Url,
    base_host: String,
    host_labels: Vec<String>,
    safe_host_labels: Vec<String>,
    public_headers: HeaderMap,
    pagination_sensitive_headers: HashSet<HeaderName>,
    timeout: Option<Duration>,
    debug: DebugLevel,
    appended_segment: bool,
    auth: Arc<AuthPlan>,
    rate_limit_rules: Vec<RateLimitRule>,
    prepared_rate_limit_plan: Option<Arc<RateLimitPlan>>,
}

impl HeadBuilder {
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        base: &Url,
        meta: RequestMeta,
        options: &RequestOptions,
        inherited_debug: DebugLevel,
        client_timeout: Option<Duration>,
    ) -> Self {
        let base_host = base
            .host_str()
            .expect("generated clients validate that fixed bases have a host")
            .to_owned();
        Self {
            timeout: options.resolve_timeout(meta.resolve_inherited_timeout(client_timeout)),
            debug: options.resolve_debug(inherited_debug),
            meta,
            logical_url: base.clone(),
            safe_logical_url: base.clone(),
            base_host,
            host_labels: Vec::new(),
            safe_host_labels: Vec::new(),
            public_headers: HeaderMap::new(),
            pagination_sensitive_headers: HashSet::new(),
            appended_segment: false,
            auth: Arc::new(AuthPlan::default()),
            rate_limit_rules: Vec::new(),
            prepared_rate_limit_plan: None,
        }
    }

    /// Appends one normalized generated authentication placement.
    #[doc(hidden)]
    pub fn push_auth(&mut self, auth: Auth) -> Result<(), ApiError> {
        Arc::get_mut(&mut self.auth)
            .expect("generated authentication is composed before a prepared plan is installed")
            .push(auth, &self.logical_url, &self.public_headers)
            .map_err(|error| {
                drop(error);
                ApiError::configuration(
                    self.meta.endpoint(),
                    self.meta.method().clone(),
                    ConfigurationErrorKind::InvalidAuthentication,
                )
            })
    }

    /// Appends one generated, structurally validated effective rate-limit rule.
    #[doc(hidden)]
    pub fn push_rate_limit_rule<I, W>(
        &mut self,
        id: &str,
        parts: I,
        cost: u32,
        windows: W,
    ) -> Result<(), ApiError>
    where
        I: IntoIterator<Item = RateLimitKeyPart>,
        W: IntoIterator<Item = (u32, Duration)>,
    {
        let rule = (|| -> Result<RateLimitRule, RateLimitError> {
            let id = RateLimitId::new(id)?;
            let key = RateLimitKey::from_parts(parts)?;
            let cost = NonZeroU32::new(cost).ok_or_else(|| {
                RateLimitError::new(crate::error::RateLimitErrorKind::InvalidConfiguration)
            })?;
            let windows = windows
                .into_iter()
                .map(|(capacity, duration)| {
                    let capacity = NonZeroU32::new(capacity).ok_or_else(|| {
                        RateLimitError::new(crate::error::RateLimitErrorKind::InvalidConfiguration)
                    })?;
                    RateLimitWindow::new(capacity, duration)
                })
                .collect::<Result<Vec<_>, _>>()?;
            RateLimitRule::new(id, key, cost, windows)
        })()
        .map_err(|error| {
            drop(error);
            ApiError::configuration(
                self.meta.endpoint(),
                self.meta.method().clone(),
                ConfigurationErrorKind::InvalidRateLimit,
            )
        })?;
        self.rate_limit_rules.push(rule);
        Ok(())
    }

    /// Adds one generated lexical rule with a declaration-scoped global key.
    #[doc(hidden)]
    pub fn push_generated_rate_limit_rule<W>(
        &mut self,
        id: &str,
        cost: u32,
        windows: W,
    ) -> Result<(), ApiError>
    where
        W: IntoIterator<Item = (u32, Duration)>,
    {
        let rule = (|| -> Result<RateLimitRule, RateLimitError> {
            let id = RateLimitId::new(id)?;
            let cost = NonZeroU32::new(cost).ok_or_else(|| {
                RateLimitError::new(crate::error::RateLimitErrorKind::InvalidConfiguration)
            })?;
            let windows = windows
                .into_iter()
                .map(|(capacity, duration)| {
                    let capacity = NonZeroU32::new(capacity).ok_or_else(|| {
                        RateLimitError::new(crate::error::RateLimitErrorKind::InvalidConfiguration)
                    })?;
                    RateLimitWindow::new(capacity, duration)
                })
                .collect::<Result<Vec<_>, _>>()?;
            RateLimitRule::new(id, RateLimitKey::global(), cost, windows)
        })()
        .map_err(|error| {
            drop(error);
            ApiError::configuration(
                self.meta.endpoint(),
                self.meta.method().clone(),
                ConfigurationErrorKind::InvalidRateLimit,
            )
        })?;
        self.rate_limit_rules.push(rule);
        Ok(())
    }

    pub(crate) fn use_prepared_auth_plan(&mut self, auth: Arc<AuthPlan>) {
        debug_assert!(self.auth.is_empty());
        self.auth = auth;
    }

    pub(crate) fn use_prepared_rate_limit_plan(&mut self, plan: Arc<RateLimitPlan>) {
        debug_assert!(self.rate_limit_rules.is_empty());
        self.prepared_rate_limit_plan = Some(plan);
    }

    /// Adds one macro-validated static host-prefix label.
    #[doc(hidden)]
    pub fn push_host_static(&mut self, label: &str) -> Result<(), ApiError> {
        self.push_host_label(label)
    }

    /// Formats, validates, and adds one dynamic host-prefix label.
    #[doc(hidden)]
    pub fn push_host_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        self.push_host_label(&value.to_string())
    }

    /// Formats and adds one pagination-sensitive dynamic host-prefix label.
    #[doc(hidden)]
    pub fn push_pagination_host_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        self.push_host_label_with_safe(&value.to_string(), "x")
    }

    /// Returns the final logical host for advanced rate-key construction.
    #[doc(hidden)]
    pub fn rate_limit_host(&self) -> &str {
        self.logical_url
            .host_str()
            .expect("logical URLs retain a host")
    }

    fn push_host_label(&mut self, label: &str) -> Result<(), ApiError> {
        self.push_host_label_with_safe(label, label)
    }

    fn push_host_label_with_safe(&mut self, label: &str, safe_label: &str) -> Result<(), ApiError> {
        if !valid_dns_label(label) {
            return Err(invalid_route(self.meta.endpoint(), self.meta.method()));
        }
        if !matches!(self.logical_url.host(), Some(Host::Domain(_))) {
            return Err(invalid_route(self.meta.endpoint(), self.meta.method()));
        }

        self.host_labels.push(label.to_owned());
        let mut host = self.host_labels.join(".");
        host.push('.');
        host.push_str(&self.base_host);
        if host.len() > 253 || self.logical_url.set_host(Some(&host)).is_err() {
            self.host_labels.pop();
            return Err(invalid_route(self.meta.endpoint(), self.meta.method()));
        }
        self.safe_host_labels.push(safe_label.to_owned());
        let mut safe_host = self.safe_host_labels.join(".");
        safe_host.push('.');
        safe_host.push_str(&self.base_host);
        self.safe_logical_url
            .set_host(Some(&safe_host))
            .expect("validated pagination-safe host labels preserve a valid host");
        Ok(())
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

    /// Formats and appends one pagination-sensitive dynamic path segment.
    #[doc(hidden)]
    pub fn push_pagination_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let segment = value.to_string();
        self.push_segment_with_safe(&segment, "<redacted>")
    }

    fn push_segment(&mut self, segment: &str) -> Result<(), ApiError> {
        self.push_segment_with_safe(segment, segment)
    }

    fn push_segment_with_safe(
        &mut self,
        segment: &str,
        safe_segment: &str,
    ) -> Result<(), ApiError> {
        if segment.is_empty()
            || matches!(segment, "." | "..")
            || segment.contains('/')
            || segment.contains('\\')
        {
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
        drop(path);
        let mut safe_path = self
            .safe_logical_url
            .path_segments_mut()
            .map_err(|()| invalid_route(self.meta.endpoint(), self.meta.method()))?;
        if !self.appended_segment {
            safe_path.pop_if_empty();
        }
        safe_path.push(safe_segment);
        self.appended_segment = true;
        Ok(())
    }

    /// Appends an arbitrary formatted prepared value as exactly one encoded path segment.
    pub(crate) fn push_prepared_segment(&mut self, segment: &str) -> Result<(), ApiError> {
        // `url` normalizes literal single- and double-dot path segments. Feed those two values as
        // percent text so the path-segment encoder escapes the percent signs and retains one inert
        // segment instead of changing the base route.
        let segment = match segment {
            "." => "%2E",
            ".." => "%2E%2E",
            segment => segment,
        };
        let mut path = self
            .logical_url
            .path_segments_mut()
            .map_err(|()| invalid_route(self.meta.endpoint(), self.meta.method()))?;
        if !self.appended_segment {
            path.pop_if_empty();
        }
        path.push(segment);
        drop(path);
        let mut safe_path = self
            .safe_logical_url
            .path_segments_mut()
            .map_err(|()| invalid_route(self.meta.endpoint(), self.meta.method()))?;
        if !self.appended_segment {
            safe_path.pop_if_empty();
        }
        safe_path.push(segment);
        self.appended_segment = true;
        Ok(())
    }

    /// Appends one public query pair after formatting its value.
    #[doc(hidden)]
    pub fn push_query_value<T>(&mut self, key: &str, value: &T)
    where
        T: Display + ?Sized,
    {
        let value = value.to_string();
        self.logical_url.query_pairs_mut().append_pair(key, &value);
        self.safe_logical_url
            .query_pairs_mut()
            .append_pair(key, &value);
    }

    /// Appends one pagination-sensitive public query pair.
    #[doc(hidden)]
    pub fn push_pagination_query_value<T>(&mut self, key: &str, value: &T)
    where
        T: Display + ?Sized,
    {
        self.logical_url
            .query_pairs_mut()
            .append_pair(key, &value.to_string());
        self.safe_logical_url
            .query_pairs_mut()
            .append_pair(key, "<redacted>");
    }

    /// Inserts or replaces one validated public header.
    #[doc(hidden)]
    pub fn insert_header_value<T>(&mut self, name: &str, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint(), self.meta.method()))?;
        let value = validate_public_header(&name, &value.to_string())
            .map_err(|_| invalid_header(self.meta.endpoint(), self.meta.method()))?;
        self.public_headers.insert(name, value);
        Ok(())
    }

    pub(crate) fn insert_prepared_header(&mut self, name: HeaderName, value: HeaderValue) {
        self.public_headers.insert(name, value);
    }

    /// Inserts one pagination-sensitive validated public header.
    #[doc(hidden)]
    pub fn insert_pagination_header_value<T>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint(), self.meta.method()))?;
        if reserved_public_header(&name) {
            return Err(invalid_header(self.meta.endpoint(), self.meta.method()));
        }
        let value = HeaderValue::from_str(&value.to_string())
            .map_err(|_| invalid_header(self.meta.endpoint(), self.meta.method()))?;
        self.pagination_sensitive_headers.insert(name.clone());
        self.public_headers.insert(name, value);
        Ok(())
    }

    /// Finishes one immutable body-free public head.
    #[doc(hidden)]
    #[must_use]
    pub fn finish(self) -> PreparedHead {
        let safe_url: Arc<str> = render_safe_url(&self.safe_logical_url, &HashSet::new()).into();
        PreparedHead {
            meta: self.meta,
            logical_url: self.logical_url,
            safe_url,
            public_headers: self.public_headers,
            pagination_sensitive_headers: self.pagination_sensitive_headers,
            timeout: self.timeout,
            debug: self.debug,
            auth: self.auth,
            rate_limit_plan: self
                .prepared_rate_limit_plan
                .unwrap_or_else(|| Arc::new(RateLimitPlan::from_rules(self.rate_limit_rules))),
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
    pagination_sensitive_headers: HashSet<HeaderName>,
    timeout: Option<Duration>,
    debug: DebugLevel,
    auth: Arc<AuthPlan>,
    rate_limit_plan: Arc<RateLimitPlan>,
}

impl PreparedHead {
    pub(crate) fn validate_auth(&self) -> Result<(), ApiError> {
        self.auth
            .validate_public(&self.logical_url, &self.public_headers)
            .map_err(|error| {
                drop(error);
                ApiError::configuration(
                    self.endpoint(),
                    self.method().clone(),
                    ConfigurationErrorKind::InvalidAuthentication,
                )
            })
    }

    pub(crate) fn apply_media_headers(
        &mut self,
        accept: Option<&'static str>,
        content_type: Option<&HeaderValue>,
        body_owns_content_type: bool,
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
        if body_owns_content_type && self.public_headers.contains_key(CONTENT_TYPE) {
            return Err(ApiError::configuration(
                self.endpoint(),
                self.method().clone(),
                ConfigurationErrorKind::InvalidHeader,
            ));
        }
        if let Some(content_type) = content_type {
            self.public_headers
                .insert(CONTENT_TYPE, content_type.clone());
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

    pub(crate) fn sanitized_public_headers(&self) -> crate::redaction::SanitizedHeaders<'_> {
        crate::redaction::SanitizedHeaders::with_sensitive(
            &self.public_headers,
            &self.pagination_sensitive_headers,
        )
    }

    pub(crate) const fn pagination_sensitive_headers(&self) -> &HashSet<HeaderName> {
        &self.pagination_sensitive_headers
    }

    pub(crate) fn rate_limit_host(&self) -> Option<&str> {
        self.logical_url.host_str()
    }

    #[doc(hidden)]
    pub const fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    #[doc(hidden)]
    pub const fn debug(&self) -> DebugLevel {
        self.debug
    }

    pub(crate) fn auth(&self) -> &AuthPlan {
        self.auth.as_ref()
    }

    pub(crate) fn rate_limit_plan(&self) -> &RateLimitPlan {
        self.rate_limit_plan.as_ref()
    }

    pub(crate) fn validate_rate_limit_key_limit(&self, limit: usize) -> Result<(), ApiError> {
        self.rate_limit_plan
            .validate_key_limit(limit)
            .map_err(|error| {
                drop(error);
                ApiError::configuration(
                    self.endpoint(),
                    self.method().clone(),
                    ConfigurationErrorKind::InvalidRateLimit,
                )
            })
    }
}

fn invalid_route(endpoint: &'static str, method: &Method) -> ApiError {
    ApiError::configuration(
        endpoint,
        method.clone(),
        ConfigurationErrorKind::InvalidRoute,
    )
}

fn invalid_header(endpoint: &'static str, method: &Method) -> ApiError {
    ApiError::configuration(
        endpoint,
        method.clone(),
        ConfigurationErrorKind::InvalidHeader,
    )
}

fn valid_dns_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    (1..=63).contains(&bytes.len())
        && bytes.iter().all(u8::is_ascii)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
}

pub(crate) fn reserved_public_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "te"
            | "trailer"
            | "upgrade"
            | "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
    )
}

pub(crate) fn validate_public_header(
    name: &HeaderName,
    value: &str,
) -> Result<HeaderValue, ConfigurationErrorKind> {
    if reserved_public_header(name) {
        return Err(ConfigurationErrorKind::InvalidHeader);
    }
    HeaderValue::from_str(value).map_err(|_| ConfigurationErrorKind::InvalidHeader)
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
            None,
        )
        .finish()
    }

    fn builder(base: &str) -> HeadBuilder {
        HeadBuilder::new(
            &Url::parse(base).expect("base URL"),
            RequestMeta::new("HeadApi::shape", Method::GET, None),
            &RequestOptions::new(),
            DebugLevel::None,
            None,
        )
    }

    #[test]
    fn host_prefixes_preserve_order_fixed_port_and_base_path() {
        let mut builder = builder("http://example.test:4317/v1/");
        builder.push_host_static("eu").expect("first label");
        builder
            .push_host_value(&"api")
            .expect("dynamic second label");
        let head = builder.finish();
        assert_eq!(head.logical_url().host_str(), Some("eu.api.example.test"));
        assert_eq!(head.logical_url().port(), Some(4317));
        assert_eq!(head.logical_url().path(), "/v1/");
    }

    #[test]
    fn pagination_host_is_actual_for_rate_limiting_and_redacted_for_safe_surfaces() {
        const SENTINEL: &str = "page-host-sentinel-7319";
        let expected_host = format!("{SENTINEL}.example.test");
        let mut builder = builder("https://example.test/v1/");
        builder
            .push_pagination_host_value(&SENTINEL)
            .expect("pagination host label");
        assert_eq!(builder.rate_limit_host(), expected_host);

        let head = builder.finish();
        assert_eq!(head.rate_limit_host(), Some(expected_host.as_str()));
        assert_eq!(
            Url::parse(head.safe_url()).expect("safe URL").host_str(),
            Some("x.example.test")
        );
        assert!(!head.safe_url().contains(SENTINEL));
    }

    #[test]
    fn dns_label_boundaries_and_injections_fail_closed() {
        let valid = "a".repeat(63);
        let mut valid_builder = builder("https://example.test/");
        valid_builder
            .push_host_value(&valid)
            .expect("63-byte label");

        for invalid in [
            "".to_owned(),
            "a".repeat(64),
            "-leading".to_owned(),
            "trailing-".to_owned(),
            "under_score".to_owned(),
            "two.labels".to_owned(),
            "port:443".to_owned(),
            "é".to_owned(),
        ] {
            let mut builder = builder("https://example.test/");
            let error = builder
                .push_host_value(&invalid)
                .expect_err("invalid label");
            assert_eq!(
                error.configuration_kind(),
                Some(ConfigurationErrorKind::InvalidRoute)
            );
        }

        let mut ip_builder = builder("http://127.0.0.1:8080/");
        ip_builder
            .push_host_static("api")
            .expect_err("an IP base cannot receive a DNS prefix");
    }

    #[test]
    fn path_segments_reject_structure_and_encode_percent_once() {
        for invalid in ["", ".", "..", "a/b", "a\\b"] {
            let mut builder = builder("https://example.test/root/");
            builder
                .push_value(&invalid)
                .expect_err("structural segment must fail");
        }

        let mut builder = builder("https://example.test/root/");
        builder.push_value(&"50%done").expect("percent as data");
        builder.push_static("tail").expect("tail");
        assert_eq!(
            builder.finish().logical_url().as_str(),
            "https://example.test/root/50%25done/tail"
        );
    }

    #[test]
    fn path_segmentation_property_preserves_one_owned_segment() {
        const SEED: u64 = 0x7061_7468_5f32_3032;
        let mut state = SEED;
        for case_index in 0..256_u32 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let segment = format!("item-{case_index}-{state:016x} %?#");
            let mut builder = builder("https://example.test/root/");
            builder
                .push_value(&segment)
                .unwrap_or_else(|_| panic!("seed={SEED:#x} case={case_index}"));
            let url = builder.finish().logical_url().clone();
            assert_eq!(url.query(), None, "seed={SEED:#x} case={case_index}");
            assert_eq!(url.fragment(), None, "seed={SEED:#x} case={case_index}");
            assert!(
                url.path().starts_with("/root/item-"),
                "seed={SEED:#x} case={case_index}"
            );
            assert!(
                url.path().ends_with("%20%25%3F%23"),
                "seed={SEED:#x} case={case_index} url={url}"
            );
        }
    }

    #[test]
    fn ordered_query_and_safe_redaction_are_finalized_together() {
        let mut builder = builder("https://example.test/v1");
        builder.push_query_value("keep", &"first");
        builder.push_query_value("tag", &"one two");
        builder.push_query_value("tag", &"three");
        builder.push_query_value("api_key", &"sensitive");
        let head = builder.finish();
        assert_eq!(
            head.logical_url().query(),
            Some("keep=first&tag=one+two&tag=three&api_key=sensitive")
        );
        assert_eq!(
            Url::parse(head.safe_url()).expect("safe URL").query(),
            Some("keep=first&tag=one+two&tag=three&api_key=%3Credacted%3E")
        );
    }

    #[test]
    fn public_headers_are_case_insensitive_validated_and_reserved() {
        let mut header_builder = builder("https://example.test/");
        header_builder
            .insert_header_value("X-Tenant", &"first")
            .expect("header");
        header_builder
            .insert_header_value("x-tenant", &"second")
            .expect("replacement");
        header_builder
            .insert_header_value("Accept", &"application/custom")
            .expect("public Accept");
        let mut head = header_builder.finish();
        assert_eq!(
            head.public_headers().get("X-Tenant"),
            Some(&HeaderValue::from_static("second"))
        );
        head.apply_media_headers(Some("application/json"), None, false)
            .expect("public Accept wins");
        assert_eq!(
            head.public_headers().get(ACCEPT),
            Some(&HeaderValue::from_static("application/custom"))
        );

        for reserved in [
            "Host",
            "Content-Length",
            "Authorization",
            "Proxy-Authorization",
            "Cookie",
        ] {
            let mut builder = builder("https://example.test/");
            let error = builder
                .insert_header_value(reserved, &"value")
                .expect_err("reserved public header");
            assert_eq!(
                error.configuration_kind(),
                Some(ConfigurationErrorKind::InvalidHeader)
            );
        }

        let mut builder = builder("https://example.test/");
        builder
            .insert_header_value("bad name", &"value")
            .expect_err("invalid header name");
        builder
            .insert_header_value("x-safe", &"bad\r\nvalue")
            .expect_err("invalid header value");
    }

    #[test]
    fn codec_media_uses_the_single_public_header_map_with_owned_precedence() {
        let mut head = prepared();
        head.apply_media_headers(
            Some("application/vnd.example+json"),
            Some(&HeaderValue::from_static("application/json")),
            true,
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

        head.apply_media_headers(Some("text/plain"), None, false)
            .expect("an existing Accept value wins");
        assert_eq!(
            head.public_headers().get(ACCEPT),
            Some(&HeaderValue::from_static("application/vnd.example+json"))
        );

        let error = head
            .apply_media_headers(None, Some(&HeaderValue::from_static("text/plain")), true)
            .expect_err("a request codec owns Content-Type");
        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
    }

    #[test]
    fn invalid_static_accept_value_fails_during_public_preparation() {
        let error = prepared()
            .apply_media_headers(Some("invalid\r\naccept"), None, false)
            .expect_err("invalid codec Accept value");
        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
    }

    #[test]
    fn bodyless_public_content_type_is_allowed_but_multipart_ownership_conflicts() {
        let mut bodyless_builder = builder("https://example.test/");
        bodyless_builder
            .insert_header_value("Content-Type", &"application/example")
            .expect("bodyless public Content-Type");
        let mut head = bodyless_builder.finish();
        head.apply_media_headers(None, None, false)
            .expect("no body source owns Content-Type");

        let mut multipart_builder = builder("https://example.test/");
        multipart_builder
            .insert_header_value("Content-Type", &"application/example")
            .expect("public header is structurally valid");
        let mut head = multipart_builder.finish();
        let error = head
            .apply_media_headers(None, None, true)
            .expect_err("multipart owns its top-level Content-Type boundary");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
    }
}
