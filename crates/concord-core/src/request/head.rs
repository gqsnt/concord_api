use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use url::{Host, Url};

use super::RequestOptions;
use crate::atom::{
    AtomEncoder, AtomError, BoundedAtomWriter, OrderedValues, RequestAtom, encode_atom,
    encode_atom_with,
};
use crate::auth::{Auth, AuthPlan};
use crate::controls::{CallOptions, CancellationToken};
use crate::endpoint::{AcceptedStatus, Effect, EndpointDescription, EndpointId, TimeoutSelection};
use crate::error::{ApiError, ConfigurationErrorKind};
use crate::hooks::DebugLevel;
use crate::limits::{
    MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES,
    MAX_REQUEST_HEADER_COUNT, MAX_REQUEST_URL_BYTES,
};
use crate::rate_limit::{RateLimitDeclaration, RateLimitPlan};
use crate::redaction::render_observed_url;

/// Static generated metadata required before public-head preparation.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct RequestMeta {
    etag_revalidation: bool,
    endpoint: EndpointId,
    method: Method,
    timeout: TimeoutSelection,
    accepted_status: AcceptedStatus,
    effect: Effect,
}

impl RequestMeta {
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        endpoint: &'static str,
        method: Method,
        inherited_timeout: Option<Duration>,
    ) -> Self {
        let effect = Effect::for_method(&method);
        Self::new_with_contract(
            endpoint,
            method,
            inherited_timeout,
            AcceptedStatus::Success,
            effect,
        )
    }

    #[doc(hidden)]
    #[must_use]
    pub fn new_with_effect(
        endpoint: &'static str,
        method: Method,
        inherited_timeout: Option<Duration>,
        effect: Effect,
    ) -> Self {
        Self::new_with_contract(
            endpoint,
            method,
            inherited_timeout,
            AcceptedStatus::Success,
            effect,
        )
    }

    #[doc(hidden)]
    #[must_use]
    pub fn new_with_contract(
        endpoint: &'static str,
        method: Method,
        inherited_timeout: Option<Duration>,
        accepted_status: AcceptedStatus,
        effect: Effect,
    ) -> Self {
        Self {
            etag_revalidation: false,
            endpoint: EndpointId::from_static(endpoint),
            method,
            timeout: inherited_timeout
                .map_or(TimeoutSelection::InheritClient, TimeoutSelection::Explicit),
            accepted_status,
            effect,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn from_description(description: &EndpointDescription) -> Self {
        Self {
            etag_revalidation: false,
            endpoint: description.endpoint_id().clone(),
            method: description.method().clone(),
            timeout: description.timeout(),
            accepted_status: description.accepted_status().clone(),
            effect: description.effect(),
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn with_etag_revalidation(mut self) -> Self {
        self.etag_revalidation = true;
        self
    }

    #[doc(hidden)]
    pub const fn etag_revalidation(&self) -> bool {
        self.etag_revalidation
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

    #[doc(hidden)]
    pub const fn effect(&self) -> Effect {
        self.effect
    }

    #[doc(hidden)]
    pub const fn accepted_status(&self) -> &AcceptedStatus {
        &self.accepted_status
    }

    #[cfg(test)]
    pub(crate) fn with_accepted_status(mut self, accepted_status: AcceptedStatus) -> Self {
        self.accepted_status = accepted_status;
        self
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

    fn remove_components(&mut self, count: usize) -> Result<(), RequestHeadLimitError> {
        self.components = self
            .components
            .checked_sub(count)
            .ok_or(RequestHeadLimitError::Arithmetic)?;
        Ok(())
    }

    fn remove_headers(
        &mut self,
        name: &HeaderName,
        values: impl Iterator<Item = HeaderValue>,
    ) -> Result<(), RequestHeadLimitError> {
        let mut count = 0_usize;
        let mut bytes = 0_usize;
        for value in values {
            count = count
                .checked_add(1)
                .ok_or(RequestHeadLimitError::Arithmetic)?;
            bytes = bytes
                .checked_add(name.as_str().len())
                .and_then(|total| total.checked_add(value.as_bytes().len()))
                .ok_or(RequestHeadLimitError::Arithmetic)?;
        }
        self.components = self
            .components
            .checked_sub(count)
            .ok_or(RequestHeadLimitError::Arithmetic)?;
        self.header_count = self
            .header_count
            .checked_sub(count)
            .ok_or(RequestHeadLimitError::Arithmetic)?;
        self.header_bytes = self
            .header_bytes
            .checked_sub(bytes)
            .ok_or(RequestHeadLimitError::Arithmetic)?;
        Ok(())
    }

    fn try_append_header(
        &mut self,
        name: &HeaderName,
        value: &HeaderValue,
    ) -> Result<(), RequestHeadLimitError> {
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
            .checked_add(name.as_str().len())
            .and_then(|total| total.checked_add(value.as_bytes().len()))
            .ok_or(RequestHeadLimitError::Arithmetic)?;
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
    /// PRIVACY-01: a sensitive generated host component withholds the actual
    /// full host from automatic classifiers.
    host_withheld: bool,
    public_headers: HeaderMap,
    pagination_sensitive_headers: HashSet<HeaderName>,
    timeout: Option<Duration>,
    debug: DebugLevel,
    call_caps: CallOptions,
    call_cancellation: Vec<CancellationToken>,
    appended_segment: bool,
    auth: Arc<AuthPlan>,
    rate_limit_declarations: Vec<RateLimitDeclaration>,
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
            call_caps: options.controls().caps().clone(),
            call_cancellation: options.controls().cancellation().to_vec(),
            meta,
            logical_url: base.clone(),
            safe_logical_url: base.clone(),
            base_host,
            host_labels: Vec::new(),
            safe_host_labels: Vec::new(),
            host_withheld: false,
            public_headers: HeaderMap::new(),
            pagination_sensitive_headers: HashSet::new(),
            appended_segment: false,
            auth: Arc::new(AuthPlan::default()),
            rate_limit_declarations: Vec::new(),
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

    /// Attaches one declared effective rate-limit policy (generated-code bridge).
    ///
    /// The `api!` macro lowers every resolved `rate_limit` attachment to one
    /// [`RateLimitDeclaration`] and calls this method once per effective
    /// attachment in effective declaration order. The declaration's latched
    /// construction error, if any, is reported here. An exact repetition of an
    /// already attached policy (same ID, cost, canonical windows and ordered
    /// dimensions including custom value bytes) is deduplicated; any other
    /// repetition of the ID is rejected. At most
    /// [`crate::rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN`] distinct
    /// policies are retained. The built-in `origin` and `endpoint` dimensions
    /// are resolved from the finished head, so the logical origin always
    /// includes every host label. Errors are safe configuration errors that
    /// never contain IDs or key text. No limiter work happens here.
    #[doc(hidden)]
    pub fn push_rate_limit(&mut self, declaration: RateLimitDeclaration) -> Result<(), ApiError> {
        crate::rate_limit::push_declaration(&mut self.rate_limit_declarations, declaration).map_err(
            |error| {
                drop(error);
                self.invalid_rate_limit()
            },
        )
    }

    fn invalid_rate_limit(&self) -> ApiError {
        ApiError::configuration(
            self.meta.endpoint_id().clone(),
            self.meta.method().clone(),
            ConfigurationErrorKind::InvalidRateLimit,
        )
    }
    /// The safe target identities of the effective authentication
    /// placements composed so far (runtime binding validation).
    pub(crate) fn auth_targets(&self) -> Vec<crate::auth::AuthTarget> {
        self.auth.targets()
    }

    /// Validates that every composed placement belongs to `expected`.
    pub(crate) fn validate_auth_context(
        &self,
        expected: &crate::auth::CredentialContext,
    ) -> Result<(), ApiError> {
        self.auth.validate_context(expected).map_err(|error| {
            drop(error);
            invalid_auth(self.meta.endpoint_id(), self.meta.method())
        })
    }

    /// Whether an effective placement attributes `status` as a challenge.
    pub(crate) fn auth_classifies_challenge(&self, status: http::StatusCode) -> bool {
        self.auth.classifies_challenge(status)
    }

    /// Removes the one inherited placement owning `target` (RUNTIME-02).
    /// Returns whether a placement owned it.
    pub(crate) fn remove_auth_target(
        &mut self,
        target: &crate::auth::AuthTarget,
    ) -> Result<bool, ApiError> {
        let removed = Arc::get_mut(&mut self.auth)
            .ok_or_else(|| invalid_auth(self.meta.endpoint_id(), self.meta.method()))?
            .remove_target(target);
        if removed {
            self.budget
                .remove_components(1)
                .map_err(|_| invalid_auth(self.meta.endpoint_id(), self.meta.method()))?;
        }
        Ok(removed)
    }

    /// Number of distinct effective rate-limit declarations composed so far.
    pub(crate) fn rate_limit_declaration_count(&self) -> usize {
        self.rate_limit_declarations.len()
    }

    pub(crate) fn append_rate_limit_declarations(
        &mut self,
        declarations: &[RateLimitDeclaration],
    ) -> Result<(), ApiError> {
        for declaration in declarations {
            self.push_rate_limit(declaration.clone())?;
        }
        Ok(())
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
        T: RequestAtom + ?Sized,
    {
        let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_host_label(&value)
    }

    /// Adapts, validates, and adds one dynamic host-prefix label.
    #[doc(hidden)]
    pub fn push_host_value_with<A, T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        A: AtomEncoder<T>,
        T: ?Sized,
    {
        let value = encode_atom_with::<A, T>(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_host_label(&value)
    }

    /// Encodes one possibly concatenated host atom in a single bounded pass.
    #[doc(hidden)]
    pub fn push_host_atoms<F>(&mut self, redacted: bool, encode: F) -> Result<(), ApiError>
    where
        F: FnOnce(&mut BoundedAtomWriter<'_>) -> Result<(), AtomError>,
    {
        let value = encode_atoms(encode)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        if redacted {
            self.push_host_label_with_safe(&value, "x")?;
            self.host_withheld = true;
            Ok(())
        } else {
            self.push_host_label(&value)
        }
    }

    /// Formats and adds one pagination-sensitive dynamic host-prefix label.
    #[doc(hidden)]
    pub fn push_pagination_host_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: RequestAtom + ?Sized,
    {
        let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_host_label_with_safe(&value, "x")?;
        self.host_withheld = true;
        Ok(())
    }

    /// Returns the canonical `scheme://host:effective_port` rate-key origin
    /// of the current logical URL (the payload of the built-in `origin`
    /// dimension).
    pub(crate) fn rate_limit_origin(&self) -> Option<String> {
        crate::rate_limit::normalized_origin(&self.logical_url)
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
        T: RequestAtom + ?Sized,
    {
        let segment = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_segment(&segment)
    }

    /// Adapts and appends one dynamic value as exactly one path segment.
    #[doc(hidden)]
    pub fn push_value_with<A, T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        A: AtomEncoder<T>,
        T: ?Sized,
    {
        let segment = encode_atom_with::<A, T>(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_segment(&segment)
    }

    /// Encodes one possibly concatenated path atom in a single bounded pass.
    #[doc(hidden)]
    pub fn push_path_atoms<F>(&mut self, redacted: bool, encode: F) -> Result<(), ApiError>
    where
        F: FnOnce(&mut BoundedAtomWriter<'_>) -> Result<(), AtomError>,
    {
        let value = encode_atoms(encode)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        if redacted {
            self.push_segment_with_safe(&value, "<redacted>")
        } else {
            self.push_segment(&value)
        }
    }

    /// Formats and appends one pagination-sensitive dynamic path segment.
    #[doc(hidden)]
    pub fn push_pagination_value<T>(&mut self, value: &T) -> Result<(), ApiError>
    where
        T: RequestAtom + ?Sized,
    {
        let segment = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
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
        if !valid_path_segment(segment) {
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

    /// Appends one public query pair after formatting its value.
    #[doc(hidden)]
    pub fn push_query_value<T>(&mut self, key: &str, value: &T) -> Result<(), ApiError>
    where
        T: RequestAtom + ?Sized,
    {
        let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        self.push_query_atom(key, &value, false)
    }

    /// Appends one explicitly adapted public query pair.
    #[doc(hidden)]
    pub fn push_query_value_with<A, T>(&mut self, key: &str, value: &T) -> Result<(), ApiError>
    where
        A: AtomEncoder<T>,
        T: ?Sized,
    {
        let value = encode_atom_with::<A, T>(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_query_atom(key, &value, false)
    }

    /// Encodes one possibly concatenated query atom in a single bounded pass.
    #[doc(hidden)]
    pub fn push_query_atoms<F>(
        &mut self,
        key: &str,
        redacted: bool,
        encode: F,
    ) -> Result<(), ApiError>
    where
        F: FnOnce(&mut BoundedAtomWriter<'_>) -> Result<(), AtomError>,
    {
        let value = encode_atoms(encode)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_query_atom(key, &value, redacted)
    }

    /// Appends an explicitly ordered sequence as repeated query occurrences.
    #[doc(hidden)]
    pub fn push_query_each<V>(
        &mut self,
        key: &str,
        values: &V,
        redacted: bool,
    ) -> Result<(), ApiError>
    where
        V: OrderedValues + ?Sized,
        V::Item: RequestAtom,
    {
        for value in values.ordered_values() {
            let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
                .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
            self.push_query_atom(key, &value, redacted)?;
        }
        Ok(())
    }

    /// Appends an adapted ordered sequence as repeated query occurrences.
    #[doc(hidden)]
    pub fn push_query_each_with<A, V>(
        &mut self,
        key: &str,
        values: &V,
        redacted: bool,
    ) -> Result<(), ApiError>
    where
        V: OrderedValues + ?Sized,
        A: AtomEncoder<V::Item>,
    {
        for value in values.ordered_values() {
            let value = encode_atom_with::<A, V::Item>(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
                .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
            self.push_query_atom(key, &value, redacted)?;
        }
        Ok(())
    }

    fn push_query_atom(&mut self, key: &str, value: &str, redacted: bool) -> Result<(), ApiError> {
        let mut budget = self.budget;
        budget
            .try_add_components(1)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let mut logical_url = self.logical_url.clone();
        logical_url.query_pairs_mut().append_pair(key, value);
        let mut safe_logical_url = self.safe_logical_url.clone();
        safe_logical_url
            .query_pairs_mut()
            .append_pair(key, if redacted { "<redacted>" } else { value });
        validate_url_pair(&logical_url, &safe_logical_url)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;

        self.logical_url = logical_url;
        self.safe_logical_url = safe_logical_url;
        self.budget = budget;
        Ok(())
    }

    /// Removes every generated public query occurrence with this exact key.
    #[doc(hidden)]
    pub fn remove_query_values(&mut self, key: &str) -> Result<(), ApiError> {
        let removed = self
            .logical_url
            .query_pairs()
            .filter(|(candidate, _)| candidate == key)
            .count();
        if removed == 0 {
            return Ok(());
        }
        remove_query_key(&mut self.logical_url, key);
        remove_query_key(&mut self.safe_logical_url, key);
        self.budget
            .remove_components(removed)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))
    }

    pub(crate) fn push_prepared_query_value(
        &mut self,
        key: &str,
        value: &str,
        sensitive: bool,
    ) -> Result<(), ApiError> {
        self.push_query_atom(key, value, sensitive)
    }

    /// Appends one pagination-sensitive public query pair.
    #[doc(hidden)]
    pub fn push_pagination_query_value<T>(&mut self, key: &str, value: &T) -> Result<(), ApiError>
    where
        T: RequestAtom + ?Sized,
    {
        let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.push_query_atom(key, &value, true)
    }

    /// Inserts or replaces one validated public header.
    #[doc(hidden)]
    pub fn insert_header_value<T>(&mut self, name: &str, value: &T) -> Result<(), ApiError>
    where
        T: RequestAtom + ?Sized,
    {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = validate_public_header(&name, &value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.insert_public_header(name, value)
    }

    /// Inserts or replaces one explicitly adapted public header.
    #[doc(hidden)]
    pub fn insert_header_value_with<A, T>(&mut self, name: &str, value: &T) -> Result<(), ApiError>
    where
        A: AtomEncoder<T>,
        T: ?Sized,
    {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = encode_atom_with::<A, T>(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = validate_public_header(&name, &value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.insert_public_header(name, value)
    }

    /// Encodes one possibly concatenated public header atom in one bounded pass.
    #[doc(hidden)]
    pub fn insert_header_atoms<F>(
        &mut self,
        name: &str,
        redacted: bool,
        encode: F,
    ) -> Result<(), ApiError>
    where
        F: FnOnce(&mut BoundedAtomWriter<'_>) -> Result<(), AtomError>,
    {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = encode_atoms(encode)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = validate_public_header(&name, &value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.insert_public_header(name.clone(), value)?;
        if redacted {
            self.pagination_sensitive_headers.insert(name);
        }
        Ok(())
    }

    /// Encodes and appends one public header occurrence in one bounded pass.
    #[doc(hidden)]
    pub fn append_header_atoms<F>(
        &mut self,
        name: &str,
        redacted: bool,
        encode: F,
    ) -> Result<(), ApiError>
    where
        F: FnOnce(&mut BoundedAtomWriter<'_>) -> Result<(), AtomError>,
    {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = encode_atoms(encode)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = validate_public_header(&name, &value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let mut budget = self.budget;
        budget
            .try_append_header(&name, &value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.public_headers.append(name.clone(), value);
        if redacted {
            self.pagination_sensitive_headers.insert(name);
        }
        self.budget = budget;
        Ok(())
    }

    /// Inserts an ordered sequence as repeated public header occurrences.
    #[doc(hidden)]
    pub fn insert_header_each<V>(
        &mut self,
        name: &str,
        values: &V,
        redacted: bool,
    ) -> Result<(), ApiError>
    where
        V: OrderedValues + ?Sized,
        V::Item: RequestAtom,
    {
        self.remove_header_values(name)?;
        for value in values.ordered_values() {
            let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
                .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
            self.append_header_atom(name, &value, redacted)?;
        }
        Ok(())
    }

    /// Inserts an adapted ordered sequence as repeated public headers.
    #[doc(hidden)]
    pub fn insert_header_each_with<A, V>(
        &mut self,
        name: &str,
        values: &V,
        redacted: bool,
    ) -> Result<(), ApiError>
    where
        V: OrderedValues + ?Sized,
        A: AtomEncoder<V::Item>,
    {
        self.remove_header_values(name)?;
        self.append_header_each_with::<A, V>(name, values, redacted)
    }

    /// Appends an ordered sequence as repeated public headers.
    #[doc(hidden)]
    pub fn append_header_each<V>(
        &mut self,
        name: &str,
        values: &V,
        redacted: bool,
    ) -> Result<(), ApiError>
    where
        V: OrderedValues + ?Sized,
        V::Item: RequestAtom,
    {
        for value in values.ordered_values() {
            let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
                .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
            self.append_header_atom(name, &value, redacted)?;
        }
        Ok(())
    }

    /// Appends an adapted ordered sequence as repeated public headers.
    #[doc(hidden)]
    pub fn append_header_each_with<A, V>(
        &mut self,
        name: &str,
        values: &V,
        redacted: bool,
    ) -> Result<(), ApiError>
    where
        V: OrderedValues + ?Sized,
        A: AtomEncoder<V::Item>,
    {
        for value in values.ordered_values() {
            let value = encode_atom_with::<A, V::Item>(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
                .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
            self.append_header_atom(name, &value, redacted)?;
        }
        Ok(())
    }

    fn append_header_atom(
        &mut self,
        name: &str,
        value: &str,
        redacted: bool,
    ) -> Result<(), ApiError> {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let value = validate_public_header(&name, value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let mut budget = self.budget;
        budget
            .try_append_header(&name, &value)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.public_headers.append(name.clone(), value);
        if redacted {
            self.pagination_sensitive_headers.insert(name);
        }
        self.budget = budget;
        Ok(())
    }

    /// Removes every generated public header occurrence under case-insensitive identity.
    #[doc(hidden)]
    pub fn remove_header_values(&mut self, name: &str) -> Result<(), ApiError> {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let values = self
            .public_headers
            .get_all(&name)
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        if values.is_empty() {
            return Ok(());
        }
        let mut budget = self.budget;
        budget
            .remove_headers(&name, values.into_iter())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        self.public_headers.remove(&name);
        self.pagination_sensitive_headers.remove(&name);
        self.budget = budget;
        Ok(())
    }

    /// Validates and appends one already encoded public header occurrence
    /// selected by a patch program.
    pub(crate) fn append_prepared_header_value(
        &mut self,
        name: &str,
        value: &str,
        sensitive: bool,
    ) -> Result<(), ApiError> {
        self.append_header_atom(name, value, sensitive)
    }

    /// The safe error for a failed public value encoder in one family.
    pub(crate) fn invalid_public_value(&self, family: super::program::PatchFamily) -> ApiError {
        match family {
            super::program::PatchFamily::Query => {
                invalid_route(self.meta.endpoint_id(), self.meta.method())
            }
            super::program::PatchFamily::Header => {
                invalid_header(self.meta.endpoint_id(), self.meta.method())
            }
        }
    }

    /// Inserts one pagination-sensitive validated public header.
    #[doc(hidden)]
    pub fn insert_pagination_header_value<T>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), ApiError>
    where
        T: RequestAtom + ?Sized,
    {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        if reserved_public_header(&name) {
            return Err(invalid_header(self.meta.endpoint_id(), self.meta.method()));
        }
        let value = encode_atom(value, MAX_FORMATTED_PUBLIC_VALUE_BYTES)
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

    fn resolve_rate_limits(&self) -> Result<(Arc<RateLimitPlan>, Option<Arc<str>>), ApiError> {
        if self.rate_limit_declarations.is_empty() {
            return Ok((Arc::new(RateLimitPlan::empty()), None));
        }
        let origin = self
            .rate_limit_origin()
            .ok_or_else(|| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        let mut policies = Vec::with_capacity(self.rate_limit_declarations.len());
        for declaration in &self.rate_limit_declarations {
            declaration
                .resolve(&origin, self.meta.endpoint_id().as_str())
                .and_then(|policy| RateLimitPlan::push_policy(&mut policies, policy))
                .map_err(|error| {
                    drop(error);
                    self.invalid_rate_limit()
                })?;
        }
        let plan = RateLimitPlan::from_validated_policies(policies);
        let logical_origin = plan.uses_origin().then(|| Arc::from(origin));
        Ok((Arc::new(plan), logical_origin))
    }

    /// Finishes one immutable body-free public head.
    #[doc(hidden)]
    pub fn finish(self) -> Result<PreparedHead, ApiError> {
        validate_url_pair(&self.logical_url, &self.safe_logical_url)
            .map_err(|_| invalid_route(self.meta.endpoint_id(), self.meta.method()))?;
        self.budget
            .validate_headers(&self.public_headers)
            .map_err(|_| invalid_header(self.meta.endpoint_id(), self.meta.method()))?;
        let observed_url = render_observed_url(&self.safe_logical_url, &HashSet::new());
        if observed_url.len() > MAX_REQUEST_URL_BYTES {
            return Err(invalid_route(self.meta.endpoint_id(), self.meta.method()));
        }
        let observed_url: Arc<str> = observed_url.into();
        let (rate_limit_plan, rate_limit_logical_origin) = self.resolve_rate_limits()?;
        let cooldown_origin = self
            .rate_limit_origin()
            .map(crate::rate_limit::CooldownOrigin::new);
        Ok(PreparedHead {
            meta: self.meta,
            logical_url: self.logical_url,
            host_withheld: self.host_withheld,
            observed_url,
            rate_limit_logical_origin,
            cooldown_origin,
            public_headers: self.public_headers,
            pagination_sensitive_headers: self.pagination_sensitive_headers,
            timeout: self.timeout,
            debug: self.debug,
            call_caps: self.call_caps,
            call_cancellation: self.call_cancellation,
            auth: self.auth,
            rate_limit_plan,
            budget: self.budget,
        })
    }
}

/// Immutable public request facts prepared only after first poll.
#[doc(hidden)]
pub struct PreparedHead {
    meta: RequestMeta,
    logical_url: Url,
    host_withheld: bool,
    observed_url: Arc<str>,
    rate_limit_logical_origin: Option<Arc<str>>,
    cooldown_origin: Option<crate::rate_limit::CooldownOrigin>,
    public_headers: HeaderMap,
    pagination_sensitive_headers: HashSet<HeaderName>,
    timeout: Option<Duration>,
    debug: DebugLevel,
    call_caps: CallOptions,
    call_cancellation: Vec<CancellationToken>,
    auth: Arc<AuthPlan>,
    rate_limit_plan: Arc<RateLimitPlan>,
    budget: RequestHeadBudget,
}

impl PreparedHead {
    /// The logical host offered to automatic classifiers: withheld entirely
    /// when any generated host component is sensitive (PRIVACY-01).
    pub(crate) fn classifier_host(&self) -> Option<&str> {
        if self.host_withheld {
            None
        } else {
            self.logical_url.host_str()
        }
    }

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
        if accept.is_none() && content_type.is_none() {
            // Nothing to place (for example the body-media step of a
            // body-free request): the headers and budget stay as they are,
            // without copying the header map (V3-O07 attribution).
            return Ok(());
        }

        let mut headers = self.public_headers.clone();
        let mut budget = self.budget;
        let selected_accept = accept
            .map(|value| {
                crate::response::validate_accept(value)
                    .map_err(|_| invalid_header(self.meta.endpoint_id(), self.method()))?;
                HeaderValue::from_str(value)
                    .map_err(|_| invalid_header(self.meta.endpoint_id(), self.method()))
            })
            .transpose()?;
        if let Some(value) = selected_accept {
            budget
                .try_insert_header(&headers, &ACCEPT, &value)
                .map_err(|_| invalid_header(self.meta.endpoint_id(), self.method()))?;
            headers.insert(ACCEPT, value);
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

    /// Places the one body-owned `Content-Type` taken from the prepared body.
    ///
    /// Called once per operation or page after body preparation and before
    /// credentials, so every attempt of that page shares the same header.
    /// A public head can never own `Content-Type` (it is reserved), so a
    /// collision is an ownership error rather than an overwrite.
    pub(crate) fn apply_body_content_type(
        &mut self,
        content_type: Option<&HeaderValue>,
    ) -> Result<(), ApiError> {
        self.apply_media_headers(None, content_type, content_type.is_some())
    }

    #[doc(hidden)]
    pub const fn meta(&self) -> &RequestMeta {
        &self.meta
    }

    #[doc(hidden)]
    pub const fn method(&self) -> &Method {
        self.meta.method()
    }

    pub(crate) const fn effect(&self) -> Effect {
        self.meta.effect()
    }

    /// Caller caps captured from the request controls when this head was
    /// prepared (send/replay caps apply to this direct request or page).
    pub(crate) const fn call_caps(&self) -> &CallOptions {
        &self.call_caps
    }

    /// External cancellation signals captured from the request controls when
    /// this head was prepared (bounded and deduplicated by the controls). A
    /// managed stream opened by this request keeps observing them.
    pub(crate) fn call_cancellation(&self) -> &[CancellationToken] {
        &self.call_cancellation
    }

    pub(crate) const fn accepted_status(&self) -> &AcceptedStatus {
        self.meta.accepted_status()
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
    pub fn observed_url(&self) -> &str {
        &self.observed_url
    }

    #[doc(hidden)]
    pub const fn public_headers(&self) -> &HeaderMap {
        &self.public_headers
    }

    pub(crate) fn request_target(&self) -> crate::hooks::RequestTarget {
        crate::hooks::RequestTarget::from_url(&self.logical_url, self.public_headers.len())
    }

    pub(crate) const fn pagination_sensitive_headers(&self) -> &HashSet<HeaderName> {
        &self.pagination_sensitive_headers
    }

    pub(crate) fn rate_limit_logical_origin(&self) -> Option<&str> {
        self.rate_limit_logical_origin.as_deref()
    }

    pub(crate) fn cooldown_origin(&self) -> Option<&crate::rate_limit::CooldownOrigin> {
        self.cooldown_origin.as_ref()
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

fn encode_atoms<F>(encode: F) -> Result<String, AtomError>
where
    F: FnOnce(&mut BoundedAtomWriter<'_>) -> Result<(), AtomError>,
{
    let mut output = String::new();
    encode(&mut BoundedAtomWriter::new(
        &mut output,
        MAX_FORMATTED_PUBLIC_VALUE_BYTES,
    ))?;
    Ok(output)
}

fn remove_query_key(url: &mut Url, key: &str) {
    let retained = url
        .query_pairs()
        .filter(|(candidate, _)| candidate != key)
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !retained.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (name, value) in retained {
            pairs.append_pair(&name, &value);
        }
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

/// PATH §5.3: the complete unencoded segment is nonempty, neither `.` nor
/// `..`, and contains no `/` or backslash. Percent signs are ordinary data,
/// not a raw-encoding escape: a value such as `a%2Fb` is accepted and encoded
/// exactly once (`a%252Fb`), never decoded into a slash nor preserved as a
/// raw path fragment.
fn valid_path_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !matches!(segment, "." | "..")
        && !segment.contains('/')
        && !segment.contains('\\')
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
            | "content-type"
            | "accept"
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

    impl RequestAtom for FirstExcessDisplay {
        fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
            let boundary = "x".repeat(MAX_FORMATTED_PUBLIC_VALUE_BYTES);
            output.push_str(&boundary)?;
            output.push_str(FORMAT_SENTINEL)
        }
    }

    impl crate::rate_limit::RateKeyPart for FirstExcessDisplay {
        const ENCODING_ID: &'static str = "test.first-excess/1";

        fn write_canonical(
            &self,
            out: &mut crate::rate_limit::CanonicalKeyWriter,
        ) -> Result<(), crate::rate_limit::RateLimitError> {
            let boundary = vec![b'x'; crate::rate_limit::MAX_RATE_LIMIT_KEY_BYTES];
            out.write_tagged_bytes("first-excess", &boundary)?;
            out.write_tagged_bytes("sentinel", FORMAT_SENTINEL.as_bytes())
        }
    }

    struct GuardedRateText {
        text: String,
        may_inspect: bool,
    }

    impl crate::rate_limit::RateKeyPart for GuardedRateText {
        const ENCODING_ID: &'static str = "test.guarded/1";

        fn write_canonical(
            &self,
            out: &mut crate::rate_limit::CanonicalKeyWriter,
        ) -> Result<(), crate::rate_limit::RateLimitError> {
            assert!(
                self.may_inspect,
                "generated key encoded a part after the first excess"
            );
            out.write_tagged_bytes("guarded", self.text.as_bytes())
        }
    }

    fn one_window(id: &str) -> RateLimitDeclaration {
        RateLimitDeclaration::new(id).window(1, Duration::from_secs(1))
    }

    fn assert_rate_error_is_safe(error: &ApiError, sentinels: &[&str]) {
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidRateLimit)
        );
        for sentinel in sentinels {
            assert!(!error.to_string().contains(sentinel));
            assert!(!format!("{error:?}").contains(sentinel));
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
    fn observed_url_exact_limit() {
        const SAFE_SUFFIX: &str = "?token=%3Credacted%3E";
        let base = base_with_serialized_len(MAX_REQUEST_URL_BYTES - SAFE_SUFFIX.len());
        let mut builder = builder(&base);
        builder
            .push_query_value("token", &"x")
            .expect("raw URLs remain bounded");
        let head = builder.finish().expect("safe URL exact boundary");
        assert_eq!(head.observed_url().len(), MAX_REQUEST_URL_BYTES);
        assert!(head.logical_url().as_str().len() < MAX_REQUEST_URL_BYTES);
    }

    #[test]
    fn observed_url_first_excess() {
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
    fn absent_media_headers_leave_the_head_unchanged() {
        let mut builder = builder("https://example.test/");
        builder
            .insert_header_value("x-trace", "one")
            .expect("public header");
        let mut head = builder.finish().expect("head");
        let before = head.public_headers().clone();
        let budget = (head.budget.components(), head.budget.header_count());
        // The body-media step of a body-free request places nothing.
        head.apply_body_content_type(None).expect("no body media");
        head.apply_media_headers(None, None, false)
            .expect("no media headers");
        assert_eq!(head.public_headers(), &before);
        assert_eq!(
            (head.budget.components(), head.budget.header_count()),
            budget
        );
        // An existing Content-Type still conflicts with a body-owned one.
        let mut owned = builder_with_content_type();
        let error = owned
            .apply_media_headers(None, None, true)
            .expect_err("public Content-Type conflicts with a body-owned one");
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidHeader)
        );
    }

    fn builder_with_content_type() -> PreparedHead {
        let mut head = builder("https://example.test/").finish().expect("head");
        head.public_headers
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        head
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
        use crate::request::{RuntimeEndpoint, test_owner};
        use crate::response::Empty;

        // The method is charged as one recorded runtime operation, so 255
        // runtime path atoms reach the runtime operation boundary; the bound
        // plan then composes them through the same head budget (255 path
        // components plus the representation's Accept header).
        let public_components = MAX_REQUEST_HEAD_COMPONENTS - 1;
        let mut runtime =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.head"), Method::GET);
        for _ in 0..public_components {
            runtime = runtime.path("x");
        }
        let runtime: RuntimeEndpoint<Empty> = runtime
            .response::<Empty, ()>()
            .build()
            .expect("runtime boundary after method charge");
        let executor = concord_test_support::executor::DeterministicExecutor::new();
        let core = crate::client::ClientCore::build_with_development(
            "http://example.test/",
            crate::http::HttpConfigBuilder::new().http1_only(),
            crate::runtime::RuntimeConfigBuilder::new(),
            &executor,
        )
        .expect("development core");
        let runtime_plan = test_owner::root(&core, runtime)
            .plan()
            .expect("runtime final head boundary");

        let mut generated = builder("http://example.test/");
        for _ in 0..public_components {
            generated.push_static("x").expect("generated component");
        }
        let generated_head = generated.finish().expect("matching generated head");
        assert_eq!(runtime_plan.observed_url(), generated_head.observed_url());

        let mut excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.head"), Method::GET);
        for _ in 0..=public_components {
            excessive = excessive.path("x");
        }
        assert_eq!(
            excessive
                .response::<Empty, ()>()
                .build()
                .expect_err("first excess runtime operation")
                .kind(),
            ConfigurationErrorKind::InvalidRoute
        );
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
        assert!(!head.observed_url().contains(FORMAT_SENTINEL));
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
        assert!(!head.observed_url().contains(FORMAT_SENTINEL));
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
        assert!(!head.observed_url().contains(FORMAT_SENTINEL));
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
        assert!(!head.observed_url().contains(FORMAT_SENTINEL));
    }

    #[test]
    fn generated_rate_limits_obey_same_structural_limits() {
        let mut window_boundary = builder("https://example.test/");
        let mut declaration = RateLimitDeclaration::new("window-boundary").origin();
        for index in 0..crate::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_POLICY {
            declaration = declaration.window(
                1,
                Duration::from_secs(u64::try_from(index + 1).expect("test index")),
            );
        }
        window_boundary
            .push_rate_limit(declaration.clone())
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

        let mut excessive_windows = builder("https://example.test/");
        let error = excessive_windows
            .push_rate_limit(declaration.window(1, Duration::from_secs(10_000)))
            .expect_err("first excess generated window");
        assert_rate_error_is_safe(&error, &[RATE_ID_SENTINEL]);

        let mut excessive_parts = builder("https://example.test/");
        let mut parts = one_window(RATE_ID_SENTINEL);
        for index in 0..=crate::rate_limit::MAX_RATE_LIMIT_KEY_PARTS {
            parts = parts.part(
                &format!("part-{index}"),
                &GuardedRateText {
                    text: RATE_KEY_SENTINEL.to_owned(),
                    may_inspect: index < crate::rate_limit::MAX_RATE_LIMIT_KEY_PARTS,
                },
            );
        }
        let error = excessive_parts
            .push_rate_limit(parts)
            .expect_err("first excess generated key part");
        assert_rate_error_is_safe(&error, &[RATE_ID_SENTINEL, RATE_KEY_SENTINEL]);

        let first = "x".repeat(crate::rate_limit::MAX_RATE_LIMIT_KEY_BYTES / 2);
        let second = format!(
            "{}{RATE_KEY_SENTINEL}",
            "y".repeat(crate::rate_limit::MAX_RATE_LIMIT_KEY_BYTES / 2)
        );
        let mut excessive_key = builder("https://example.test/");
        let error = excessive_key
            .push_rate_limit(
                one_window(RATE_ID_SENTINEL)
                    .part("a", &first)
                    .part("b", &second),
            )
            .expect_err("aggregate generated key above the hard cap");
        assert_rate_error_is_safe(&error, &[RATE_ID_SENTINEL, RATE_KEY_SENTINEL]);

        let mut excessive_formatted_key = builder("https://example.test/");
        let error = excessive_formatted_key
            .push_rate_limit(one_window(RATE_ID_SENTINEL).part(
                "ep.resource",
                &FirstExcessDisplay {
                    ignore_format_error: true,
                },
            ))
            .expect_err("bounded generated key encoding must fail closed");
        assert_rate_error_is_safe(&error, &[RATE_ID_SENTINEL, FORMAT_SENTINEL]);

        let mut plan_boundary = builder("https://example.test/");
        for index in 0..crate::rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN {
            plan_boundary
                .push_rate_limit(one_window(&format!("generated-rule-{index}")).origin())
                .expect("maximum generated rule count");
        }
        let error = plan_boundary
            .push_rate_limit(one_window(RATE_ID_SENTINEL).origin())
            .expect_err("first excess generated rule");
        assert_rate_error_is_safe(&error, &[RATE_ID_SENTINEL]);
        assert_eq!(
            plan_boundary
                .finish()
                .expect("bounded rate-limit head")
                .rate_limit_plan()
                .policies()
                .len(),
            crate::rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN
        );
    }

    #[test]
    fn generated_rate_limit_repetition_dedups_only_exact_duplicates() {
        let base = || {
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(10, Duration::from_secs(1))
                .window(100, Duration::from_secs(60))
                .origin()
                .part("tenant", RATE_KEY_SENTINEL)
        };
        let mut exact = builder("https://example.test/");
        exact.push_rate_limit(base()).expect("first attachment");
        exact
            .push_rate_limit(base())
            .expect("exact repeated attachment deduplicates");
        let reordered = RateLimitDeclaration::new(RATE_ID_SENTINEL)
            .window(100, Duration::from_secs(60))
            .window(10, Duration::from_secs(1))
            .origin()
            .part("tenant", RATE_KEY_SENTINEL);
        exact
            .push_rate_limit(reordered)
            .expect("canonical window order is the schema identity");
        let head = exact.finish().expect("deduplicated head");
        assert_eq!(head.rate_limit_plan().policies().len(), 1);
        assert_eq!(
            head.rate_limit_plan().policies()[0].windows()[0].duration(),
            Duration::from_secs(1),
            "the first attachment's source order is retained as provenance"
        );

        let conflicts = [
            base().cost(2),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(10, Duration::from_secs(1))
                .window(100, Duration::from_secs(60))
                .origin()
                .part("tenant", "a-different-tenant"),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(10, Duration::from_secs(1))
                .window(100, Duration::from_secs(60))
                .origin()
                .part("client.tenant", RATE_KEY_SENTINEL),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(10, Duration::from_secs(1))
                .window(100, Duration::from_secs(60))
                .part("tenant", RATE_KEY_SENTINEL)
                .origin(),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(10, Duration::from_secs(1))
                .window(99, Duration::from_secs(60))
                .origin()
                .part("tenant", RATE_KEY_SENTINEL),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(10, Duration::from_secs(1))
                .window(100, Duration::from_secs(60))
                .origin()
                .part("tenant", &RATE_KEY_SENTINEL.len()),
        ];
        for conflict in conflicts {
            let mut conflicting = builder("https://example.test/");
            conflicting
                .push_rate_limit(base())
                .expect("first attachment");
            let error = conflicting
                .push_rate_limit(conflict)
                .expect_err("a differing repetition of one ID is a conflict");
            assert_rate_error_is_safe(&error, &[RATE_ID_SENTINEL, RATE_KEY_SENTINEL]);
        }
    }

    #[test]
    fn generated_rate_limit_structural_validation_is_latched_and_safe() {
        let invalid = [
            RateLimitDeclaration::new(RATE_ID_SENTINEL).origin(),
            RateLimitDeclaration::new(RATE_ID_SENTINEL).window(1, Duration::from_secs(1)),
            one_window(RATE_ID_SENTINEL).origin().origin(),
            one_window(RATE_ID_SENTINEL).endpoint().endpoint(),
            one_window(RATE_ID_SENTINEL).part("origin", "reserved"),
            one_window(RATE_ID_SENTINEL).part("endpoint", "reserved"),
            one_window(RATE_ID_SENTINEL).part("", "empty"),
            one_window(RATE_ID_SENTINEL)
                .part("tenant", "a")
                .part("tenant", "b"),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(0, Duration::from_secs(1))
                .origin(),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(1, Duration::ZERO)
                .origin(),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(1, Duration::from_secs(1))
                .window(2, Duration::from_secs(1))
                .origin(),
            RateLimitDeclaration::new(RATE_ID_SENTINEL)
                .window(5, Duration::from_secs(1))
                .window(1, Duration::from_secs(2))
                .cost(2)
                .origin(),
            one_window(RATE_ID_SENTINEL).cost(0).origin(),
            one_window(RATE_ID_SENTINEL).cost(1).cost(1).origin(),
            RateLimitDeclaration::new("").window(1, Duration::from_secs(1)),
            RateLimitDeclaration::new("i".repeat(crate::rate_limit::MAX_RATE_LIMIT_ID_BYTES + 1))
                .window(1, Duration::from_secs(1))
                .origin(),
            one_window(RATE_ID_SENTINEL).part(
                &"l".repeat(crate::rate_limit::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES + 1),
                "overlong label",
            ),
        ];
        for declaration in invalid {
            let mut head = builder("https://example.test/");
            let error = head
                .push_rate_limit(declaration)
                .expect_err("invalid generated declaration");
            assert_rate_error_is_safe(&error, &[RATE_ID_SENTINEL]);
            assert!(
                head.finish()
                    .expect("rejected declaration retains nothing")
                    .rate_limit_plan()
                    .is_empty()
            );
        }
        let label = "l".repeat(crate::rate_limit::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES);
        let mut boundary = builder("https://example.test/");
        boundary
            .push_rate_limit(
                RateLimitDeclaration::new("i".repeat(crate::rate_limit::MAX_RATE_LIMIT_ID_BYTES))
                    .window(3, Duration::from_secs(1))
                    .cost(3)
                    .part(&label, "maximum label"),
            )
            .expect("maximum ID, label and admissible cost");
    }

    #[test]
    fn rate_limit_origin_and_endpoint_resolve_from_the_finished_head() {
        let mut head_builder = HeadBuilder::new(
            &Url::parse("HTTPS://API.Example.TEST/root/").expect("base URL"),
            RequestMeta::new("catalog.item.update", Method::PUT, None),
            &RequestOptions::new(),
            DebugLevel::None,
            None,
        );
        // Attached before the host label: resolution must still observe the
        // complete logical destination.
        head_builder
            .push_rate_limit(
                one_window("catalog.item.write")
                    .origin()
                    .endpoint()
                    .part("client.tenant", "tenant-a"),
            )
            .expect("origin/endpoint declaration");
        head_builder.push_host_static("eu").expect("host label");
        let head = head_builder.finish().expect("resolved head");
        let origin = "https://eu.api.example.test:443";
        assert_eq!(head.rate_limit_logical_origin(), Some(origin));
        let policy = &head.rate_limit_plan().policies()[0];
        let parts = policy.key().parts();
        assert_eq!(parts[0].name(), "origin");
        assert_eq!(
            parts[0].kind(),
            crate::rate_limit::RateDimensionKind::Origin
        );
        assert_eq!(parts[0].encoding_id(), "concord.utf8/1");
        assert_eq!(
            parts[0].value(),
            crate::rate_limit::encode_canonical(origin, 1_024)
                .expect("origin bytes")
                .as_bytes()
        );
        assert_eq!(parts[1].name(), "endpoint");
        assert_eq!(
            parts[1].value(),
            crate::rate_limit::encode_canonical("catalog.item.update", 1_024)
                .expect("endpoint bytes")
                .as_bytes()
        );
        assert_eq!(parts[2].name(), "client.tenant");

        let mut no_origin = builder("https://example.test/");
        no_origin
            .push_rate_limit(one_window("tenant.only").part("tenant", "tenant-a"))
            .expect("custom-only declaration");
        assert_eq!(
            no_origin
                .finish()
                .expect("custom-only head")
                .rate_limit_logical_origin(),
            None,
            "the logical origin is disclosed only for an origin dimension"
        );
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
    fn pagination_origin_is_actual_for_rate_keys_and_redacted_for_safe_surfaces() {
        const SENTINEL: &str = "page-host-sentinel-7319";
        let expected_host = format!("{SENTINEL}.example.test");
        let mut builder = builder("https://example.test/v1/");
        builder
            .push_pagination_host_value(&SENTINEL)
            .expect("pagination host label");
        assert_eq!(
            builder.rate_limit_origin(),
            Some(format!("https://{expected_host}:443"))
        );

        let head = builder.finish().expect("bounded pagination host head");
        assert_eq!(head.rate_limit_logical_origin(), None);
        assert_eq!(
            Url::parse(head.observed_url())
                .expect("safe URL")
                .host_str(),
            Some("x.example.test")
        );
        assert!(!head.observed_url().contains(SENTINEL));
        assert_eq!(head.classifier_host(), None);
    }

    /// PRIVACY-01 regression: a sensitive generated host component withholds
    /// the actual full host from automatic retry classifiers, while public
    /// labels keep the host available.
    #[test]
    fn sensitive_host_components_withhold_the_classifier_host() {
        const SENTINEL: &str = "classifier-host-sentinel-5d2e";
        let mut public = builder("https://example.test/v1/");
        public.push_host_static("eu").expect("public label");
        let public = public.finish().expect("public host head");
        assert_eq!(public.classifier_host(), Some("eu.example.test"));

        let mut sensitive = builder("https://example.test/v1/");
        sensitive.push_host_static("eu").expect("public label");
        sensitive
            .push_host_atoms(true, |writer| writer.push_str(SENTINEL))
            .expect("sensitive label");
        let sensitive = sensitive.finish().expect("sensitive host head");
        assert_eq!(
            sensitive.logical_url().host_str(),
            Some(format!("eu.{SENTINEL}.example.test").as_str())
        );
        assert_eq!(sensitive.classifier_host(), None);
        assert!(!sensitive.observed_url().contains(SENTINEL));
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
    fn percent_triples_in_path_values_are_data_encoded_exactly_once() {
        // Regression (PATH-01 / V3-S07): a `%XX`-shaped value was rejected as
        // "pre-encoded" (a v2 rule). Percent signs are ordinary data: never
        // decoded into a separator, never preserved raw, encoded once.
        let mut builder = builder("https://example.test/root/");
        builder
            .push_value(&"team alpha%2F")
            .expect("percent triple as data");
        builder
            .push_static("100%25")
            .expect("static percent triple");
        let head = builder.finish().expect("bounded path head");
        assert_eq!(
            head.logical_url().as_str(),
            "https://example.test/root/team%20alpha%252F/100%2525"
        );
        assert_eq!(
            head.logical_url()
                .path_segments()
                .expect("hierarchical URL")
                .count(),
            3
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
            Url::parse(head.observed_url()).expect("safe URL").query(),
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
        let mut head = header_builder.finish().expect("bounded header head");
        assert_eq!(
            head.public_headers().get("X-Tenant"),
            Some(&HeaderValue::from_static("second"))
        );
        head.apply_media_headers(Some("application/json"), None, false)
            .expect("response specification owns Accept");
        assert_eq!(
            head.public_headers().get(ACCEPT),
            Some(&HeaderValue::from_static("application/json"))
        );

        for reserved in [
            "Host",
            "Content-Length",
            "Authorization",
            "Proxy-Authorization",
            "Cookie",
            "Content-Type",
            "Accept",
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
            .expect("the selected response specification replaces Accept");
        assert_eq!(
            head.public_headers().get(ACCEPT),
            Some(&HeaderValue::from_static("text/plain"))
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
    fn media_headers_are_reserved_even_when_an_endpoint_has_no_body() {
        for name in ["Content-Type", "Accept"] {
            let mut builder = builder("https://example.test/");
            let error = builder
                .insert_header_value(name, &"application/example")
                .expect_err("media headers have dedicated owners");
            assert_eq!(
                error.configuration_kind(),
                Some(ConfigurationErrorKind::InvalidHeader)
            );
        }

        let mut head = prepared();
        head.apply_media_headers(
            Some("application/example"),
            Some(&HeaderValue::from_static("application/example")),
            true,
        )
        .expect("response and body specifications may place their owned media headers");
    }
}
