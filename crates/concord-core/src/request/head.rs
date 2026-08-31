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
use crate::construction::format_bounded;
use crate::endpoint::{EndpointDescription, EndpointId, TimeoutSelection};
use crate::error::{ApiError, ConfigurationErrorKind};
use crate::hooks::DebugLevel;
use crate::limits::{
    MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES,
    MAX_REQUEST_HEADER_COUNT, MAX_REQUEST_URL_BYTES,
};
use crate::rate_limit::{
    RateLimitError, RateLimitId, RateLimitKey, RateLimitPlan, RateLimitPolicy,
};
use crate::redaction::render_safe_url;

/// Static generated metadata required before public-head preparation.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct RequestMeta {
    endpoint: EndpointId,
    method: Method,
    timeout: TimeoutSelection,
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
            endpoint: EndpointId::from_static(endpoint),
            method,
            timeout: match inherited_timeout {
                Some(duration) => TimeoutSelection::Explicit(duration),
                None => TimeoutSelection::Disabled,
            },
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn from_description(description: &EndpointDescription) -> Self {
        Self {
            endpoint: description.endpoint_id().clone(),
            method: description.method().clone(),
            timeout: description.timeout(),
        }
    }

    #[doc(hidden)]
    pub fn endpoint(&self) -> &str {
        self.endpoint.as_str()
    }

    #[doc(hidden)]
    pub const fn endpoint_id(&self) -> &EndpointId {
        &self.endpoint
    }

    #[doc(hidden)]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    #[doc(hidden)]
    pub const fn timeout_selection(&self) -> TimeoutSelection {
        self.timeout
    }

    pub(crate) const fn resolve_inherited_timeout(
        &self,
        runtime_endpoint_timeout: Option<Duration>,
    ) -> Option<Duration> {
        self.timeout.resolve(runtime_endpoint_timeout)
    }
}

#[derive(Clone, Copy, Default)]
struct RequestHeadBudget {
    components: usize,
    header_count: usize,
    header_bytes: usize,
}

impl RequestHeadBudget {
    const fn new() -> Self {
        Self {
            components: 0,
            header_count: 0,
            header_bytes: 0,
        }
    }

    fn try_add_components(&mut self, count: usize) -> Result<(), RequestHeadLimitError> {
        let next = self
            .components
            .checked_add(count)
            .ok_or(RequestHeadLimitError::Arithmetic)?;
        if next > MAX_REQUEST_HEAD_COMPONENTS {
            return Err(RequestHeadLimitError::Components);
        }
        self.components = next;
        Ok(())
    }

    fn try_insert_header(
        &mut self,
        headers: &HeaderMap,
        name: &HeaderName,
        value: &HeaderValue,
    ) -> Result<(), RequestHeadLimitError> {
        let name_bytes = name.as_str().len();
        let value_bytes = value.as_bytes().len();
        let replacement_bytes = name_bytes
            .checked_add(value_bytes)
            .ok_or(RequestHeadLimitError::Arithmetic)?;

        let (next_components, next_count, next_bytes) = if let Some(previous) = headers.get(name) {
            let previous_bytes = name_bytes
                .checked_add(previous.as_bytes().len())
                .ok_or(RequestHeadLimitError::Arithmetic)?;
            let next_bytes = self
                .header_bytes
                .checked_sub(previous_bytes)
                .and_then(|bytes| bytes.checked_add(replacement_bytes))
                .ok_or(RequestHeadLimitError::Arithmetic)?;
            (self.components, self.header_count, next_bytes)
        } else {
            let next_components = self
                .components
                .checked_add(1)
                .ok_or(RequestHeadLimitError::Arithmetic)?;
            let next_count = self
                .header_count
                .checked_add(1)
                .ok_or(RequestHeadLimitError::Arithmetic)?;
            let next_bytes = self
                .header_bytes
                .checked_add(replacement_bytes)
                .ok_or(RequestHeadLimitError::Arithmetic)?;
            (next_components, next_count, next_bytes)
        };

        if next_components > MAX_REQUEST_HEAD_COMPONENTS {
            return Err(RequestHeadLimitError::Components);
        }
        if next_count > MAX_REQUEST_HEADER_COUNT {
            return Err(RequestHeadLimitError::HeaderCount);
        }
        if next_bytes > MAX_REQUEST_HEADER_BYTES {
            return Err(RequestHeadLimitError::HeaderBytes);
        }

        self.components = next_components;
        self.header_count = next_count;
        self.header_bytes = next_bytes;
        Ok(())
    }

    fn validate_headers(&self, headers: &HeaderMap) -> Result<(), RequestHeadLimitError> {
        let header_count = headers.len();
        if header_count > MAX_REQUEST_HEADER_COUNT || header_count != self.header_count {
            return Err(RequestHeadLimitError::HeaderCount);
        }
        let header_bytes = headers.iter().try_fold(0_usize, |total, (name, value)| {
            total
                .checked_add(name.as_str().len())
                .and_then(|bytes| bytes.checked_add(value.as_bytes().len()))
                .ok_or(RequestHeadLimitError::Arithmetic)
        })?;
        if header_bytes > MAX_REQUEST_HEADER_BYTES || header_bytes != self.header_bytes {
            return Err(RequestHeadLimitError::HeaderBytes);
        }
        Ok(())
    }

    #[cfg(test)]
    const fn components(self) -> usize {
        self.components
    }

    #[cfg(test)]
    const fn header_count(self) -> usize {
        self.header_count
    }

    #[cfg(test)]
    const fn header_bytes(self) -> usize {
        self.header_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestHeadLimitError {
    Components,
    UrlBytes,
    HeaderCount,
    HeaderBytes,
    Arithmetic,
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
    rate_limit_policies: Vec<RateLimitPolicy>,
    prepared_rate_limit_plan: Option<Arc<RateLimitPlan>>,
    budget: RequestHeadBudget,
}

impl HeadBuilder {
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        base: &Url,
        meta: RequestMeta,
        options: &RequestOptions,
        inherited_debug: DebugLevel,
        runtime_endpoint_timeout: Option<Duration>,
    ) -> Self {
        let base_host = base
            .host_str()
            .expect("generated clients validate that fixed bases have a host")
            .to_owned();
        Self {
            timeout: options
                .resolve_timeout(meta.resolve_inherited_timeout(runtime_endpoint_timeout)),
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
            rate_limit_policies: Vec::new(),
            prepared_rate_limit_plan: None,
            budget: RequestHeadBudget::new(),
        }
    }

    /// Appends one normalized generated authentication placement.
    #[doc(hidden)]
    pub fn push_auth(&mut self, auth: Auth) -> Result<(), ApiError> {
        let mut budget = self.budget;
        budget
            .try_add_components(auth.placement_count())
            .map_err(|_| invalid_auth(self.meta.endpoint_id(), self.meta.method()))?;
        Arc::get_mut(&mut self.auth)
            .expect("generated authentication is composed before a prepared plan is installed")
            .push(auth, &self.logical_url, &self.public_headers)
            .map_err(|error| {
                drop(error);
                invalid_auth(self.meta.endpoint_id(), self.meta.method())
            })?;
        self.budget = budget;
        Ok(())
    }

    /// Validates and appends one fallibly constructed named authentication
    /// placement without exposing its value in diagnostics.
    #[doc(hidden)]
    pub fn push_auth_result(
        &mut self,
        auth: Result<Auth, crate::auth::AuthError>,
    ) -> Result<(), ApiError> {
        let auth = auth.map_err(|error| {
            drop(error);
            ApiError::configuration(
                self.meta.endpoint_id().clone(),
                self.meta.method().clone(),
                ConfigurationErrorKind::InvalidAuthentication,
            )
        })?;
        self.push_auth(auth)
    }

    /// Appends one generated, structurally validated effective rate-limit policy.
    ///
    /// Key names are validated and values are formatted into the remaining
    /// aggregate key budget incrementally inside the builder. The first
    /// excess or duplicate part stops iteration.
    #[doc(hidden)]
    pub fn push_rate_limit_policy<I, N, V, W>(
        &mut self,
        id: &str,
        parts: I,
        cost: u32,
        windows: W,
    ) -> Result<(), ApiError>
    where
        I: IntoIterator<Item = (N, V)>,
        N: AsRef<str>,
        V: Display,
        W: IntoIterator<Item = (u32, Duration)>,
    {
        let policy = (|| -> Result<RateLimitPolicy, RateLimitError> {
            RateLimitPlan::validate_policy_id_append(&self.rate_limit_policies, id)?;
            let id = RateLimitId::new(id)?;
            let cost = NonZeroU32::new(cost).ok_or_else(|| {
                RateLimitError::new(crate::error::RateLimitErrorKind::InvalidConfiguration)
            })?;
            let key = RateLimitKey::from_display_values(parts)?;
            RateLimitPolicy::from_window_values(id, key, cost, windows)
        })()
        .map_err(|error| {
            drop(error);
            ApiError::configuration(
                self.meta.endpoint_id().clone(),
                self.meta.method().clone(),
                ConfigurationErrorKind::InvalidRateLimit,
            )
        })?;
        RateLimitPlan::push_validated_policy(&mut self.rate_limit_policies, policy).map_err(
            |error| {
                drop(error);
                ApiError::configuration(
                    self.meta.endpoint_id().clone(),
                    self.meta.method().clone(),
                    ConfigurationErrorKind::InvalidRateLimit,
                )
            },
        )
    }

    /// Adds one generated policy with a declaration-scoped global key.
    #[doc(hidden)]
    pub fn push_generated_rate_limit_policy<W>(
        &mut self,
        id: &str,
        cost: u32,
        windows: W,
    ) -> Result<(), ApiError>
    where
        W: IntoIterator<Item = (u32, Duration)>,
    {
        let policy = (|| -> Result<RateLimitPolicy, RateLimitError> {
            RateLimitPlan::validate_policy_id_append(&self.rate_limit_policies, id)?;
            let id = RateLimitId::new(id)?;
            let cost = NonZeroU32::new(cost).ok_or_else(|| {
                RateLimitError::new(crate::error::RateLimitErrorKind::InvalidConfiguration)
            })?;
            RateLimitPolicy::from_window_values(id, RateLimitKey::global(), cost, windows)
        })()
        .map_err(|error| {
            drop(error);
            ApiError::configuration(
                self.meta.endpoint_id().clone(),
                self.meta.method().clone(),
                ConfigurationErrorKind::InvalidRateLimit,
            )
        })?;
        RateLimitPlan::push_validated_policy(&mut self.rate_limit_policies, policy).map_err(
            |error| {
                drop(error);
                ApiError::configuration(
                    self.meta.endpoint_id().clone(),
                    self.meta.method().clone(),
                    ConfigurationErrorKind::InvalidRateLimit,
                )
            },
        )
    }

    pub(crate) fn use_prepared_auth_plan(&mut self, auth: Arc<AuthPlan>) -> Result<(), ApiError> {
        debug_assert!(self.auth.is_empty());
        let mut budget = self.budget;
        budget
            .try_add_components(auth.placement_count())
            .map_err(|_| invalid_auth(self.meta.endpoint_id(), self.meta.method()))?;
        self.auth = auth;
        self.budget = budget;
        Ok(())
    }

    pub(crate) fn use_prepared_rate_limit_plan(&mut self, plan: Arc<RateLimitPlan>) {
        debug_assert!(self.rate_limit_policies.is_empty());
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
        let value = format_bounded(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_host_label(&value)
    }

    /// Formats and adds one pagination-sensitive dynamic host-prefix label.
    #[doc(hidden)]
    pub fn push_pagination_host_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let value = format_bounded(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_host_label_with_safe(&value, "x")
    }

    /// Returns the final logical host for custom rate-key construction.
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
            return Err(invalid_route(self.meta.endpoint_id(), self.meta.method()));
        }
        if !matches!(self.logical_url.host(), Some(Host::Domain(_))) {
            return Err(invalid_route(self.meta.endpoint_id(), self.meta.method()));
        }

        let mut budget = self.budget;
        budget
            .try_add_components(1)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let host = prefixed_host(&self.host_labels, label, &self.base_host);
        let mut logical_url = self.logical_url.clone();
        if host.len() > 253 || logical_url.set_host(Some(&host)).is_err() {
            return Err(invalid_route(self.meta.endpoint_id(), self.meta.method()));
        }
        let safe_host = prefixed_host(&self.safe_host_labels, safe_label, &self.base_host);
        let mut safe_logical_url = self.safe_logical_url.clone();
        safe_logical_url
            .set_host(Some(&safe_host))
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        validate_url_pair(&logical_url, &safe_logical_url)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        self.host_labels.push(label.to_owned());
        self.safe_host_labels.push(safe_label.to_owned());
        self.logical_url = logical_url;
        self.safe_logical_url = safe_logical_url;
        self.budget = budget;
        Ok(())
    }

    /// Appends one macro-validated static path segment.
    #[doc(hidden)]
    pub fn push_static(&mut self, segment: &str) -> Result<(), ApiError> {
        if segment.contains('/') || segment.contains('\\') {
            return Err(invalid_route(self.meta.endpoint_id(), self.meta.method()));
        }
        self.push_segment(segment)
    }

    /// Formats and appends one dynamic value as exactly one path segment.
    #[doc(hidden)]
    pub fn push_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let segment = format_bounded(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_segment(&segment)
    }

    /// Formats and appends one pagination-sensitive dynamic path segment.
    #[doc(hidden)]
    pub fn push_pagination_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let segment = format_bounded(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
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
            return Err(invalid_route(self.meta.endpoint_id(), self.meta.method()));
        }

        let mut budget = self.budget;
        budget
            .try_add_components(1)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let mut logical_url = self.logical_url.clone();
        push_url_segment(&mut logical_url, segment, self.appended_segment)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let mut safe_logical_url = self.safe_logical_url.clone();
        push_url_segment(&mut safe_logical_url, safe_segment, self.appended_segment)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        validate_url_pair(&logical_url, &safe_logical_url)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        self.logical_url = logical_url;
        self.safe_logical_url = safe_logical_url;
        self.appended_segment = true;
        self.budget = budget;
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

        let mut budget = self.budget;
        budget
            .try_add_components(1)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let mut logical_url = self.logical_url.clone();
        push_url_segment(&mut logical_url, segment, self.appended_segment)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let mut safe_logical_url = self.safe_logical_url.clone();
        push_url_segment(&mut safe_logical_url, segment, self.appended_segment)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        validate_url_pair(&logical_url, &safe_logical_url)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        self.logical_url = logical_url;
        self.safe_logical_url = safe_logical_url;
        self.appended_segment = true;
        self.budget = budget;
        Ok(())
    }

    /// Appends one public query pair after formatting its value.
    #[doc(hidden)]
    pub fn push_query_value<T>(&mut self, key: &str, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let value = format_bounded(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        let mut budget = self.budget;
        budget
            .try_add_components(1)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let mut logical_url = self.logical_url.clone();
        logical_url.query_pairs_mut().append_pair(key, &value);
        let mut safe_logical_url = self.safe_logical_url.clone();
        safe_logical_url.query_pairs_mut().append_pair(key, &value);
        validate_url_pair(&logical_url, &safe_logical_url)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        self.logical_url = logical_url;
        self.safe_logical_url = safe_logical_url;
        self.budget = budget;
        Ok(())
    }

    /// Appends one pagination-sensitive public query pair.
    #[doc(hidden)]
    pub fn push_pagination_query_value<T>(&mut self, key: &str, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let value = format_bounded(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        let mut budget = self.budget;
        budget
            .try_add_components(1)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let mut logical_url = self.logical_url.clone();
        logical_url.query_pairs_mut().append_pair(key, &value);
        let mut safe_logical_url = self.safe_logical_url.clone();
        safe_logical_url
            .query_pairs_mut()
            .append_pair(key, "<redacted>");
        validate_url_pair(&logical_url, &safe_logical_url)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        self.logical_url = logical_url;
        self.safe_logical_url = safe_logical_url;
        self.budget = budget;
        Ok(())
    }

    /// Inserts or replaces one validated public header.
    #[doc(hidden)]
    pub fn insert_header_value<T>(&mut self, name: &str, value: &T) -> Result<(), ApiError>
    where
        T: Display + ?Sized,
    {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = format_bounded(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = validate_public_header(&name, &value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.insert_public_header(name, value)
    }

    pub(crate) fn insert_prepared_header(
        &mut self,
        name: HeaderName,
        value: HeaderValue,
    ) -> Result<(), ApiError> {
        self.insert_public_header(name, value)
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
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        if reserved_public_header(&name) {
            return Err(invalid_header(self.meta.endpoint_id(), self.meta.method()));
        }
        let value = format_bounded(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = HeaderValue::from_str(&value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.insert_public_header(name.clone(), value)?;
        self.pagination_sensitive_headers.insert(name.clone());
        Ok(())
    }

    fn insert_public_header(
        &mut self,
        name: HeaderName,
        value: HeaderValue,
    ) -> Result<(), ApiError> {
        let mut budget = self.budget;
        budget
            .try_insert_header(&self.public_headers, &name, &value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.public_headers.insert(name, value);
        self.budget = budget;
        Ok(())
    }

    /// Finishes one immutable body-free public head.
    #[doc(hidden)]
    pub fn finish(self) -> Result<PreparedHead, ApiError> {
        validate_url_pair(&self.logical_url, &self.safe_logical_url)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.budget
            .validate_headers(&self.public_headers)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let safe_url = render_safe_url(&self.safe_logical_url, &HashSet::new());
        if safe_url.len() > MAX_REQUEST_URL_BYTES {
            return Err(invalid_route(self.meta.endpoint_id(), self.meta.method()));
        }
        let safe_url: Arc<str> = safe_url.into();
        Ok(PreparedHead {
            meta: self.meta,
            logical_url: self.logical_url,
            safe_url,
            public_headers: self.public_headers,
            pagination_sensitive_headers: self.pagination_sensitive_headers,
            timeout: self.timeout,
            debug: self.debug,
            auth: self.auth,
            rate_limit_plan: self.prepared_rate_limit_plan.unwrap_or_else(|| {
                Arc::new(RateLimitPlan::from_validated_policies(
                    self.rate_limit_policies,
                ))
            }),
            budget: self.budget,
        })
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
    budget: RequestHeadBudget,
}

impl PreparedHead {
    pub(crate) fn validate_auth(&self) -> Result<(), ApiError> {
        self.auth
            .validate_public(&self.logical_url, &self.public_headers)
            .map_err(|error| {
                drop(error);
                ApiError::configuration(
                    self.meta.endpoint_id().clone(),
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
        if body_owns_content_type && self.public_headers.contains_key(CONTENT_TYPE) {
            return Err(invalid_header(self.meta.endpoint_id(), self.method()));
        }

        let mut headers = self.public_headers.clone();
        let mut budget = self.budget;
        if let Some(accept) = accept {
            if !headers.contains_key(ACCEPT) {
                let value = HeaderValue::from_str(accept)
                    .map_err(|_| invalid_header(self.meta.endpoint_id(), self.method()))?;
                budget
                    .try_insert_header(&headers, &ACCEPT, &value)
                    .map_err(|_| invalid_header(self.meta.endpoint_id(), self.method()))?;
                headers.insert(ACCEPT, value);
            }
        }
        if let Some(content_type) = content_type {
            budget
                .try_insert_header(&headers, &CONTENT_TYPE, content_type)
                .map_err(|_| invalid_header(self.meta.endpoint_id(), self.method()))?;
            headers.insert(CONTENT_TYPE, content_type.clone());
        }
        self.public_headers = headers;
        self.budget = budget;
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
    pub fn endpoint(&self) -> &str {
        self.meta.endpoint()
    }

    #[doc(hidden)]
    pub const fn endpoint_id(&self) -> &EndpointId {
        self.meta.endpoint_id()
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

    pub(crate) fn validate_auth_context(
        &self,
        expected: &crate::auth::CredentialContext,
    ) -> Result<(), ApiError> {
        self.auth.validate_context(expected).map_err(|error| {
            drop(error);
            invalid_auth(self.meta.endpoint_id(), self.meta.method())
        })
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
                    self.meta.endpoint_id().clone(),
                    self.method().clone(),
                    ConfigurationErrorKind::InvalidRateLimit,
                )
            })
    }
}

fn prefixed_host(labels: &[String], next: &str, base_host: &str) -> String {
    let prefix_bytes = labels
        .iter()
        .try_fold(next.len(), |total, label| {
            total
                .checked_add(label.len())
                .and_then(|bytes| bytes.checked_add(1))
        })
        .unwrap_or(usize::MAX);
    let capacity = prefix_bytes
        .checked_add(1)
        .and_then(|bytes| bytes.checked_add(base_host.len()))
        .unwrap_or(0);
    let mut host = String::with_capacity(capacity);
    for label in labels {
        host.push_str(label);
        host.push('.');
    }
    host.push_str(next);
    host.push('.');
    host.push_str(base_host);
    host
}

fn push_url_segment(url: &mut Url, segment: &str, appended_segment: bool) -> Result<(), ()> {
    let mut path = url.path_segments_mut()?;
    if !appended_segment {
        path.pop_if_empty();
    }
    path.push(segment);
    Ok(())
}

fn validate_url_pair(
    logical_url: &Url,
    safe_logical_url: &Url,
) -> Result<(), RequestHeadLimitError> {
    if logical_url.as_str().len() > MAX_REQUEST_URL_BYTES
        || safe_logical_url.as_str().len() > MAX_REQUEST_URL_BYTES
    {
        Err(RequestHeadLimitError::UrlBytes)
    } else {
        Ok(())
    }
}

fn invalid_route(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::configuration(
        endpoint.clone(),
        method.clone(),
        ConfigurationErrorKind::InvalidRoute,
    )
}

fn invalid_auth(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::configuration(
        endpoint.clone(),
        method.clone(),
        ConfigurationErrorKind::InvalidAuthentication,
    )
}

fn invalid_header(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::configuration(
        endpoint.clone(),
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
    validate_public_header_name(name)?;
    HeaderValue::from_str(value).map_err(|_| ConfigurationErrorKind::InvalidHeader)
}

pub(crate) fn validate_public_header_name(name: &HeaderName) -> Result<(), ConfigurationErrorKind> {
    if reserved_public_header(name) {
        Err(ConfigurationErrorKind::InvalidHeader)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::error::{ConfigurationErrorKind, ErrorCategory};

    const RATE_ID_SENTINEL: &str = "generated-rate-id-sentinel-c31f";
    const RATE_KEY_SENTINEL: &str = "generated-rate-key-sentinel-a592";
    const FORMAT_SENTINEL: &str = "generated-format-sentinel-a4f1";

    struct FirstExcessDisplay {
        ignore_format_error: bool,
    }

    impl fmt::Display for FirstExcessDisplay {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            let boundary = "x".repeat(MAX_FORMATTED_PUBLIC_VALUE_BYTES);
            if self.ignore_format_error {
                let _ = formatter.write_str(&boundary);
                let _ = formatter.write_str(FORMAT_SENTINEL);
                Ok(())
            } else {
                formatter.write_str(&boundary)?;
                formatter.write_str(FORMAT_SENTINEL)
            }
        }
    }

    struct GuardedRateWindows {
        calls: Arc<AtomicUsize>,
    }

    impl Iterator for GuardedRateWindows {
        type Item = (u32, Duration);

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(
                call <= crate::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
                "generated window iterator consumed beyond the first excess"
            );
            Some((
                1,
                Duration::from_secs(u64::try_from(call + 1).expect("test index")),
            ))
        }
    }

    struct UnpolledRateWindows;

    impl IntoIterator for UnpolledRateWindows {
        type Item = (u32, Duration);
        type IntoIter = std::iter::Empty<Self::Item>;

        fn into_iter(self) -> Self::IntoIter {
            panic!("rejected generated rule converted its window iterator")
        }
    }

    struct GuardedRateText {
        text: String,
        may_inspect: bool,
    }

    impl AsRef<str> for GuardedRateText {
        fn as_ref(&self) -> &str {
            assert!(
                self.may_inspect,
                "generated key inspected the first excess part"
            );
            &self.text
        }
    }

    impl fmt::Display for GuardedRateText {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            assert!(
                self.may_inspect,
                "generated key formatted the first excess part"
            );
            formatter.write_str(&self.text)
        }
    }

    struct GuardedRateParts {
        calls: Arc<AtomicUsize>,
    }

    impl Iterator for GuardedRateParts {
        type Item = (GuardedRateText, GuardedRateText);

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(
                call <= crate::rate_limit::MAX_RATE_LIMIT_KEY_PARTS,
                "generated key iterator consumed beyond the first excess"
            );
            let may_inspect = call < crate::rate_limit::MAX_RATE_LIMIT_KEY_PARTS;
            Some((
                GuardedRateText {
                    text: format!("part-{call}"),
                    may_inspect,
                },
                GuardedRateText {
                    text: RATE_KEY_SENTINEL.to_owned(),
                    may_inspect,
                },
            ))
        }
    }

    fn prepared() -> PreparedHead {
        HeadBuilder::new(
            &Url::parse("https://headers.example.test/").expect("base URL"),
            RequestMeta::new("header_api.records.create", Method::POST, None),
            &RequestOptions::new(),
            DebugLevel::None,
            None,
        )
        .finish()
        .expect("bounded prepared head")
    }

    fn builder(base: &str) -> HeadBuilder {
        HeadBuilder::new(
            &Url::parse(base).expect("base URL"),
            RequestMeta::new("head_api.shape", Method::GET, None),
            &RequestOptions::new(),
            DebugLevel::None,
            None,
        )
    }

    fn assert_format_error_is_safe(error: &ApiError, expected: ConfigurationErrorKind) {
        assert_eq!(error.configuration_kind(), Some(expected));
        assert!(!error.to_string().contains(FORMAT_SENTINEL));
        assert!(!format!("{error:?}").contains(FORMAT_SENTINEL));
    }

    fn base_with_serialized_len(length: usize) -> String {
        const PREFIX: &str = "https://example.test/";
        assert!(length >= PREFIX.len());
        format!("{PREFIX}{}", "b".repeat(length - PREFIX.len()))
    }

    fn base_for_appended_segment(final_length: usize, segment_length: usize) -> String {
        let mut base = base_with_serialized_len(final_length - segment_length - 1);
        base.push('/');
        assert_eq!(base.len() + segment_length, final_length);
        base
    }

    #[test]
    fn generated_head_exact_component_limit() {
        let mut builder = builder("https://example.test/");
        for _ in 0..MAX_REQUEST_HEAD_COMPONENTS {
            builder.push_static("x").expect("component at boundary");
        }
        assert_eq!(builder.budget.components(), MAX_REQUEST_HEAD_COMPONENTS);
        let head = builder.finish().expect("exact component boundary");
        assert_eq!(
            head.logical_url()
                .path_segments()
                .expect("hierarchical URL")
                .count(),
            MAX_REQUEST_HEAD_COMPONENTS
        );
    }

    #[test]
    fn generated_head_first_excess_component_not_retained() {
        const SENTINEL: &str = "head-component-first-excess-sentinel-384d";
        let mut builder = builder("https://example.test/");
        for _ in 0..MAX_REQUEST_HEAD_COMPONENTS {
            builder.push_static("x").expect("component at boundary");
        }
        let retained = builder.logical_url.clone();
        let error = builder
            .push_query_value(SENTINEL, &"withheld")
            .expect_err("first excess component");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRoute)
        );
        assert_eq!(builder.budget.components(), MAX_REQUEST_HEAD_COMPONENTS);
        assert_eq!(builder.logical_url, retained);
        assert!(!builder.logical_url.as_str().contains(SENTINEL));
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(SENTINEL));
        }
    }

    #[test]
    fn generated_head_exact_header_count() {
        let mut builder = builder("https://example.test/");
        for index in 0..MAX_REQUEST_HEADER_COUNT {
            builder
                .insert_header_value(&format!("x-head-{index}"), &"x")
                .expect("header at boundary");
        }
        assert_eq!(builder.budget.header_count(), MAX_REQUEST_HEADER_COUNT);
        assert_eq!(builder.budget.components(), MAX_REQUEST_HEAD_COMPONENTS);
        assert_eq!(
            builder
                .finish()
                .expect("exact header-count boundary")
                .public_headers()
                .len(),
            MAX_REQUEST_HEADER_COUNT
        );
    }

    #[test]
    fn generated_head_first_excess_header() {
        const SENTINEL: &str = "x-head-first-excess-sentinel-921e";
        let mut builder = builder("https://example.test/");
        for index in 0..MAX_REQUEST_HEADER_COUNT {
            builder
                .insert_header_value(&format!("x-head-{index}"), &"x")
                .expect("header at boundary");
        }
        let error = builder
            .insert_header_value(SENTINEL, &"withheld")
            .expect_err("first excess header");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
        assert_eq!(builder.budget.header_count(), MAX_REQUEST_HEADER_COUNT);
        assert!(!builder.public_headers.contains_key(SENTINEL));
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(SENTINEL));
        }
    }

    #[test]
    fn generated_head_header_replacement_updates_budget() {
        let mut builder = builder("https://example.test/");
        let name = "x-a";
        let exact = "a".repeat(MAX_REQUEST_HEADER_BYTES - name.len());
        builder
            .insert_header_value(name, &exact)
            .expect("exact aggregate header bytes");
        assert_eq!(builder.budget.header_count(), 1);
        assert_eq!(builder.budget.header_bytes(), MAX_REQUEST_HEADER_BYTES);

        builder
            .insert_header_value(name, &"short")
            .expect("smaller replacement");
        assert_eq!(builder.budget.header_count(), 1);
        assert_eq!(builder.budget.header_bytes(), name.len() + "short".len());

        builder
            .insert_header_value(name, &exact)
            .expect("replacement restores exact boundary");
        let first_excess = "z".repeat(exact.len() + 1);
        let error = builder
            .insert_header_value(name, &first_excess)
            .expect_err("first excess replacement");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
        assert_eq!(builder.budget.header_count(), 1);
        assert_eq!(builder.budget.header_bytes(), MAX_REQUEST_HEADER_BYTES);
        assert_eq!(
            builder.public_headers.get(name).map(HeaderValue::as_bytes),
            Some(exact.as_bytes())
        );
    }

    #[test]
    fn logical_url_exact_limit() {
        let segment = "s".repeat(MAX_FORMATTED_PUBLIC_VALUE_BYTES);
        let base = base_for_appended_segment(MAX_REQUEST_URL_BYTES, segment.len());
        let mut builder = builder(&base);
        builder
            .push_static(&segment)
            .expect("logical URL exact boundary");
        let head = builder.finish().expect("logical URL exact boundary");
        assert_eq!(head.logical_url().as_str().len(), MAX_REQUEST_URL_BYTES);
    }

    #[test]
    fn logical_url_first_excess() {
        const SENTINEL: &str = "logical-url-first-excess-sentinel-7da4";
        let segment = format!(
            "{}{SENTINEL}",
            "s".repeat(MAX_FORMATTED_PUBLIC_VALUE_BYTES - SENTINEL.len())
        );
        let base = base_for_appended_segment(MAX_REQUEST_URL_BYTES + 1, segment.len());
        let mut builder = builder(&base);
        let retained = builder.logical_url.clone();
        let error = builder
            .push_static(&segment)
            .expect_err("first excess logical URL");
        assert_eq!(builder.logical_url, retained);
        assert!(!builder.logical_url.as_str().contains(SENTINEL));
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(SENTINEL));
        }
    }

    #[test]
    fn safe_url_exact_limit() {
        const SAFE_SUFFIX: &str = "?token=%3Credacted%3E";
        let base = base_with_serialized_len(MAX_REQUEST_URL_BYTES - SAFE_SUFFIX.len());
        let mut builder = builder(&base);
        builder
            .push_query_value("token", &"x")
            .expect("raw URLs remain bounded");
        let head = builder.finish().expect("safe URL exact boundary");
        assert_eq!(head.safe_url().len(), MAX_REQUEST_URL_BYTES);
        assert!(head.logical_url().as_str().len() < MAX_REQUEST_URL_BYTES);
    }

    #[test]
    fn safe_url_first_excess() {
        const SAFE_SUFFIX: &str = "?token=%3Credacted%3E";
        let base = base_with_serialized_len(MAX_REQUEST_URL_BYTES - SAFE_SUFFIX.len() + 1);
        let mut builder = builder(&base);
        builder
            .push_query_value("token", &"x")
            .expect("raw URLs remain bounded");
        let error = builder
            .finish()
            .err()
            .expect("credential-sanitized URL first excess");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRoute)
        );
    }

    #[test]
    fn aggregate_header_bytes_exact_limit() {
        let mut builder = builder("https://example.test/");
        let name = "x-b";
        let exact = "a".repeat(MAX_REQUEST_HEADER_BYTES - name.len());
        builder
            .insert_header_value(name, &exact)
            .expect("aggregate header byte boundary");
        let head = builder.finish().expect("exact aggregate header bytes");
        assert_eq!(head.budget.header_bytes(), MAX_REQUEST_HEADER_BYTES);
    }

    #[test]
    fn aggregate_header_bytes_first_excess() {
        let mut builder = builder("https://example.test/");
        let name = "x-b";
        let exact = "a".repeat(MAX_REQUEST_HEADER_BYTES - name.len());
        builder
            .insert_header_value(name, &exact)
            .expect("aggregate header byte boundary");
        let error = builder
            .insert_header_value(name, &"z".repeat(exact.len() + 1))
            .expect_err("aggregate header first excess");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
        assert_eq!(builder.budget.header_bytes(), MAX_REQUEST_HEADER_BYTES);
        assert_eq!(
            builder.public_headers.get(name).map(HeaderValue::as_bytes),
            Some(exact.as_bytes())
        );
    }

    #[test]
    fn media_header_first_excess_is_transactional() {
        let mut builder = builder("https://example.test/");
        for _ in 0..(MAX_REQUEST_HEAD_COMPONENTS - 1) {
            builder.push_static("x").expect("component before media");
        }
        let mut head = builder.finish().expect("head before media");
        let error = head
            .apply_media_headers(
                Some("application/json"),
                Some(&HeaderValue::from_static("application/json")),
                true,
            )
            .expect_err("Content-Type is the first excess after Accept");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
        assert!(!head.public_headers().contains_key(ACCEPT));
        assert!(!head.public_headers().contains_key(CONTENT_TYPE));
        assert_eq!(head.budget.components(), MAX_REQUEST_HEAD_COMPONENTS - 1);
    }

    #[test]
    fn pagination_mutation_respects_head_budget() {
        const SENTINEL: &str = "pagination-head-first-excess-sentinel-41a6";
        let mut builder = builder("https://example.test/");
        for _ in 0..(MAX_REQUEST_HEAD_COMPONENTS - 1) {
            builder.push_static("x").expect("component before boundary");
        }
        builder
            .push_pagination_query_value("cursor", &"boundary")
            .expect("pagination component at boundary");
        let retained = builder.logical_url.clone();
        let error = builder
            .insert_pagination_header_value(SENTINEL, &"withheld")
            .expect_err("pagination first excess");
        assert_eq!(builder.logical_url, retained);
        assert!(!builder.public_headers.contains_key(SENTINEL));
        assert!(!builder.pagination_sensitive_headers.contains(SENTINEL));
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(SENTINEL));
        }
    }

    #[test]
    fn runtime_and_generated_head_limits_match() {
        use crate::request::{EndpointSpec, RuntimeEndpoint};
        use crate::response::NoContent;

        let public_components = MAX_REQUEST_HEAD_COMPONENTS - 1;
        let mut runtime =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.head"), Method::GET);
        for _ in 0..public_components {
            runtime = runtime.path_segment("x");
        }
        let runtime: RuntimeEndpoint<NoContent> = runtime
            .build()
            .expect("runtime boundary after method charge");
        let base = Url::parse("https://example.test/").expect("base URL");
        let runtime_head = runtime
            .prepare_head(HeadBuilder::new(
                &base,
                runtime.request_meta(),
                &RequestOptions::new(),
                DebugLevel::None,
                None,
            ))
            .expect("runtime final head boundary");

        let mut generated = builder("https://example.test/");
        for _ in 0..public_components {
            generated.push_static("x").expect("generated component");
        }
        let generated_head = generated.finish().expect("matching generated head");
        assert_eq!(runtime_head.logical_url(), generated_head.logical_url());
    }

    #[test]
    fn head_limit_errors_are_redacted() {
        const SENTINEL: &str = "head-limit-error-sentinel-6b27";
        let segment = format!(
            "{}{SENTINEL}",
            "x".repeat(MAX_FORMATTED_PUBLIC_VALUE_BYTES - SENTINEL.len())
        );
        let base = base_for_appended_segment(MAX_REQUEST_URL_BYTES + 1, segment.len());
        let mut builder = builder(&base);
        let error = builder.push_static(&segment).expect_err("URL limit error");
        assert_eq!(error.category(), ErrorCategory::Configuration);
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(SENTINEL));
        }
    }

    #[test]
    fn generated_path_formatted_value_first_excess() {
        let mut builder = builder("https://example.test/root/");
        let error = builder
            .push_value(&FirstExcessDisplay {
                ignore_format_error: false,
            })
            .expect_err("first excess generated path value");
        assert_format_error_is_safe(&error, ConfigurationErrorKind::InvalidRoute);
        let head = builder.finish().expect("unchanged bounded head");
        assert_eq!(head.logical_url().path(), "/root/");
        assert!(!head.safe_url().contains(FORMAT_SENTINEL));
    }

    #[test]
    fn generated_query_formatted_value_first_excess() {
        let mut builder = builder("https://example.test/root/");
        let error = builder
            .push_query_value(
                "value",
                &FirstExcessDisplay {
                    ignore_format_error: false,
                },
            )
            .expect_err("first excess generated query value");
        assert_format_error_is_safe(&error, ConfigurationErrorKind::InvalidRoute);
        let head = builder.finish().expect("unchanged bounded head");
        assert_eq!(head.logical_url().query(), None);
        assert!(!head.safe_url().contains(FORMAT_SENTINEL));
    }

    #[test]
    fn generated_header_formatted_value_first_excess() {
        let mut builder = builder("https://example.test/root/");
        let error = builder
            .insert_header_value(
                "x-bounded",
                &FirstExcessDisplay {
                    ignore_format_error: false,
                },
            )
            .expect_err("first excess generated header value");
        assert_format_error_is_safe(&error, ConfigurationErrorKind::InvalidHeader);
        assert!(
            builder
                .finish()
                .expect("unchanged bounded head")
                .public_headers()
                .is_empty()
        );
    }

    #[test]
    fn pagination_formatted_value_first_excess() {
        let mut builder = builder("https://example.test/root/");
        let error = builder
            .push_pagination_query_value(
                "cursor",
                &FirstExcessDisplay {
                    ignore_format_error: false,
                },
            )
            .expect_err("first excess pagination query");
        assert_format_error_is_safe(&error, ConfigurationErrorKind::InvalidRoute);
        let error = builder
            .insert_pagination_header_value(
                "x-cursor",
                &FirstExcessDisplay {
                    ignore_format_error: false,
                },
            )
            .expect_err("first excess pagination header");
        assert_format_error_is_safe(&error, ConfigurationErrorKind::InvalidHeader);
        let error = builder
            .push_pagination_value(&FirstExcessDisplay {
                ignore_format_error: false,
            })
            .expect_err("first excess pagination path");
        assert_format_error_is_safe(&error, ConfigurationErrorKind::InvalidRoute);
        let error = builder
            .push_pagination_host_value(&FirstExcessDisplay {
                ignore_format_error: false,
            })
            .expect_err("first excess pagination host");
        assert_format_error_is_safe(&error, ConfigurationErrorKind::InvalidRoute);

        let head = builder.finish().expect("unchanged bounded head");
        assert_eq!(head.logical_url().query(), None);
        assert_eq!(head.logical_url().path(), "/root/");
        assert!(head.public_headers().is_empty());
        assert!(!head.safe_url().contains(FORMAT_SENTINEL));
    }

    #[test]
    fn bounded_formatter_does_not_retain_protected_sentinel() {
        let mut builder = builder("https://example.test/root/");
        let error = builder
            .push_pagination_query_value(
                "cursor",
                &FirstExcessDisplay {
                    ignore_format_error: true,
                },
            )
            .expect_err("ignored formatting error still fails closed");
        assert_format_error_is_safe(&error, ConfigurationErrorKind::InvalidRoute);
        let head = builder.finish().expect("unchanged bounded head");
        assert_eq!(head.logical_url().query(), None);
        assert!(!head.safe_url().contains(FORMAT_SENTINEL));
    }

    #[test]
    fn generated_rate_limits_obey_same_structural_limits() {
        let mut window_boundary = builder("https://example.test/");
        window_boundary
            .push_generated_rate_limit_policy(
                "window-boundary",
                1,
                (0..crate::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_POLICY).map(|index| {
                    (
                        1,
                        Duration::from_secs(u64::try_from(index + 1).expect("test index")),
                    )
                }),
            )
            .expect("maximum generated window count");
        assert_eq!(
            window_boundary
                .finish()
                .expect("bounded rate-limit head")
                .rate_limit_plan()
                .policies()[0]
                .windows()
                .len(),
            crate::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_POLICY
        );

        let calls = Arc::new(AtomicUsize::new(0));
        let mut excessive_windows = builder("https://example.test/");
        let error = excessive_windows
            .push_generated_rate_limit_policy(
                RATE_ID_SENTINEL,
                1,
                GuardedRateWindows {
                    calls: Arc::clone(&calls),
                },
            )
            .expect_err("first excess generated window");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            crate::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_POLICY + 1
        );
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));

        let calls = Arc::new(AtomicUsize::new(0));
        let mut excessive_parts = builder("https://example.test/");
        let error = excessive_parts
            .push_rate_limit_policy(
                RATE_ID_SENTINEL,
                GuardedRateParts {
                    calls: Arc::clone(&calls),
                },
                1,
                UnpolledRateWindows,
            )
            .expect_err("first excess generated key part");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            crate::rate_limit::MAX_RATE_LIMIT_KEY_PARTS + 1
        );
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!error.to_string().contains(RATE_KEY_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_KEY_SENTINEL));

        let first = "x".repeat(crate::rate_limit::MAX_RATE_LIMIT_KEY_BYTES - 1);
        let second = RATE_KEY_SENTINEL.to_owned();
        let mut excessive_key = builder("https://example.test/");
        let error = excessive_key
            .push_rate_limit_policy(
                RATE_ID_SENTINEL,
                [("a", first), ("b", second)],
                1,
                [(1, Duration::from_secs(1))],
            )
            .expect_err("aggregate generated key above the hard cap");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!error.to_string().contains(RATE_KEY_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_KEY_SENTINEL));

        let mut excessive_formatted_key = builder("https://example.test/");
        let error = excessive_formatted_key
            .push_rate_limit_policy(
                RATE_ID_SENTINEL,
                [(
                    "ep.resource",
                    &FirstExcessDisplay {
                        ignore_format_error: true,
                    },
                )],
                1,
                UnpolledRateWindows,
            )
            .expect_err("bounded generated key formatting must fail closed");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!error.to_string().contains(FORMAT_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(FORMAT_SENTINEL));

        let mut plan_boundary = builder("https://example.test/");
        for index in 0..crate::rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN {
            plan_boundary
                .push_generated_rate_limit_policy(
                    &format!("generated-rule-{index}"),
                    1,
                    [(1, Duration::from_secs(1))],
                )
                .expect("maximum generated rule count");
        }
        let error = plan_boundary
            .push_generated_rate_limit_policy(RATE_ID_SENTINEL, 1, UnpolledRateWindows)
            .expect_err("first excess generated rule");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));
        assert_eq!(
            plan_boundary
                .finish()
                .expect("bounded rate-limit head")
                .rate_limit_plan()
                .policies()
                .len(),
            crate::rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN
        );

        let mut duplicate = builder("https://example.test/");
        duplicate
            .push_generated_rate_limit_policy(RATE_ID_SENTINEL, 1, [(1, Duration::from_secs(1))])
            .expect("first generated policy ID");
        let error = duplicate
            .push_generated_rate_limit_policy(RATE_ID_SENTINEL, 1, UnpolledRateWindows)
            .expect_err("duplicate generated policy ID");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));
    }

    #[test]
    fn host_prefixes_preserve_order_fixed_port_and_base_path() {
        let mut builder = builder("http://example.test:4317/v1/");
        builder.push_host_static("eu").expect("first label");
        builder
            .push_host_value(&"api")
            .expect("dynamic second label");
        let head = builder.finish().expect("bounded host head");
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

        let head = builder.finish().expect("bounded pagination host head");
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
            builder
                .finish()
                .expect("bounded path head")
                .logical_url()
                .as_str(),
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
            let url = builder
                .finish()
                .expect("bounded property head")
                .logical_url()
                .clone();
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
        builder
            .push_query_value("keep", &"first")
            .expect("public query");
        builder
            .push_query_value("tag", &"one two")
            .expect("public query");
        builder
            .push_query_value("tag", &"three")
            .expect("public query");
        builder
            .push_query_value("api_key", &"sensitive")
            .expect("public query");
        let head = builder.finish().expect("bounded query head");
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
        let mut head = header_builder.finish().expect("bounded header head");
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
        let mut head = bodyless_builder.finish().expect("bounded bodyless head");
        head.apply_media_headers(None, None, false)
            .expect("no body source owns Content-Type");

        let mut multipart_builder = builder("https://example.test/");
        multipart_builder
            .insert_header_value("Content-Type", &"application/example")
            .expect("public header is structurally valid");
        let mut head = multipart_builder.finish().expect("bounded multipart head");
        let error = head
            .apply_media_headers(None, None, true)
            .expect_err("multipart owns its top-level Content-Type boundary");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
    }
}
