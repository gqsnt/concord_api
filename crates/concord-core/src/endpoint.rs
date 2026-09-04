use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use http::{Method, StatusCode};

use crate::contract::{EndpointContractDescription, SharedEndpointContractDescription};
use crate::error::{ConfigurationError, ConfigurationErrorKind};
use crate::redaction::SanitizedHeaderMap;

pub use crate::limits::{
    MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES,
    MAX_REQUEST_HEADER_COUNT, MAX_REQUEST_URL_BYTES, MAX_RUNTIME_ENDPOINT_COMPONENTS,
    MAX_RUNTIME_ENDPOINT_METHOD_BYTES, MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
};

/// Maximum encoded length accepted for one endpoint contract identity.
pub const MAX_ENDPOINT_ID_BYTES: usize = 128;

const MAX_ENDPOINT_ID_SEGMENT_BYTES: usize = 63;

#[derive(Clone)]
enum EndpointIdStorage {
    Static(&'static str),
    Owned(Arc<str>),
}

/// A bounded, public contract identity for one generated or runtime endpoint.
///
/// Generated declarations construct validated static identities through
/// Concord's hidden generated-code bridge. Runtime endpoints use
/// [`EndpointId::new`], [`EndpointId::try_from`], or the standard
/// [`TryFrom`] implementations, which validate and own at most
/// [`MAX_ENDPOINT_ID_BYTES`] bytes. Contract identities are public metadata:
/// callers must never place credentials or other protected values in them.
#[derive(Clone)]
pub struct EndpointId {
    storage: EndpointIdStorage,
}

impl EndpointId {
    /// Creates one bounded owned runtime endpoint identity.
    ///
    /// IDs contain 1–128 ASCII bytes in dot-separated 1–63 byte segments.
    /// Every segment begins and ends with a lowercase letter or digit and may
    /// additionally contain lowercase letters, digits, `_`, or `-`.
    pub fn new(value: impl AsRef<str>) -> Result<Self, ConfigurationError> {
        let value = value.as_ref();
        validate_endpoint_id(value)?;
        Ok(Self {
            storage: EndpointIdStorage::Owned(Arc::from(value)),
        })
    }

    pub(crate) const fn from_static(value: &'static str) -> Self {
        assert!(
            valid_endpoint_id(value),
            "invalid static Concord endpoint identity"
        );
        Self {
            storage: EndpointIdStorage::Static(value),
        }
    }

    /// Returns the public contract identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.storage {
            EndpointIdStorage::Static(value) => value,
            EndpointIdStorage::Owned(value) => value,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_static(&self) -> bool {
        matches!(self.storage, EndpointIdStorage::Static(_))
    }
}

impl AsRef<str> for EndpointId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<String> for EndpointId {
    type Error = ConfigurationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_endpoint_id(&value)?;
        Ok(Self {
            storage: EndpointIdStorage::Owned(Arc::from(value)),
        })
    }
}

impl TryFrom<&str> for EndpointId {
    type Error = ConfigurationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for EndpointId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Debug for EndpointId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("EndpointId")
            .field(&self.as_str())
            .finish()
    }
}

impl PartialEq for EndpointId {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for EndpointId {}

impl PartialOrd for EndpointId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EndpointId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for EndpointId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialEq<str> for EndpointId {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for EndpointId {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Endpoint-level operation-timeout selection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TimeoutSelection {
    /// Resolve the timeout from the immutable built-client snapshot.
    InheritClient,
    /// Use one explicit finite operation duration.
    Explicit(Duration),
}

impl TimeoutSelection {
    pub(crate) const fn resolve(self, client: Option<Duration>) -> Option<Duration> {
        match self {
            Self::InheritClient => client,
            Self::Explicit(duration) => Some(duration),
        }
    }
}

/// Static request-body information safe to expose without invoking a body
/// specification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequestBodyCategory {
    /// The endpoint has no request body.
    Empty,
    /// The endpoint has one declared semantic request-body specification.
    Specification,
}

/// One core-owned accepted-response acquisition category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResponseAcquisition {
    /// Complete bytes are acquired under the configured finite byte and
    /// empty-transport-chunk limits.
    Buffered,
    /// A lazy byte stream is exposed behind core-owned byte and
    /// empty-transport-chunk accounting.
    Streaming,
    /// The accepted response body is discarded without polling.
    Discard,
}

/// The endpoint's declared application effect class.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Effect {
    /// The operation is read-only and may be used for ordinary pagination.
    Safe,
    /// Repetition may change representation state but has the same intended
    /// application effect as one execution.
    Idempotent,
    /// Repetition may apply an additional application effect.
    NonIdempotent,
}

impl Effect {
    /// Returns the conservative method-derived v2.1 default.
    #[must_use]
    pub fn for_method(method: &Method) -> Self {
        match method.as_str() {
            "GET" | "HEAD" | "OPTIONS" => Self::Safe,
            "PUT" | "DELETE" => Self::Idempotent,
            _ => Self::NonIdempotent,
        }
    }
}

/// Provenance of the resolved effect declaration.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EffectSource {
    /// Derived from the exact HTTP method.
    MethodDefault,
    /// Written explicitly on the endpoint declaration or runtime endpoint.
    Explicit,
}

/// The endpoint-owned finite set of final response statuses accepted as an
/// ordinary result.
///
/// Authentication challenge attribution is evaluated before this set. Status
/// policy never belongs to a response representation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum AcceptedStatus {
    /// Accept every final status in `200..=299`.
    #[default]
    Success,
    /// Accept one canonical sorted finite set.
    Codes(Arc<[StatusCode]>),
}

impl AcceptedStatus {
    /// The default final-success policy.
    #[must_use]
    pub const fn success() -> Self {
        Self::Success
    }

    /// Validates and canonicalizes an explicit nonempty finite status set.
    pub fn codes(codes: impl IntoIterator<Item = StatusCode>) -> Result<Self, ConfigurationError> {
        let mut codes = codes.into_iter().collect::<Vec<_>>();
        codes.sort_unstable();
        codes.dedup();
        if codes.is_empty() {
            return Err(ConfigurationError::new(
                ConfigurationErrorKind::InvalidValue,
            ));
        }
        Ok(Self::Codes(codes.into()))
    }

    /// Returns whether a final response status is accepted.
    #[must_use]
    pub fn contains(&self, status: StatusCode) -> bool {
        match self {
            Self::Success => status.is_success(),
            Self::Codes(codes) => codes.binary_search(&status).is_ok(),
        }
    }

    /// Returns explicit canonical codes, or `None` for the success range.
    #[must_use]
    pub fn explicit_codes(&self) -> Option<&[StatusCode]> {
        match self {
            Self::Success => None,
            Self::Codes(codes) => Some(codes),
        }
    }
}

/// Provenance of the endpoint's status policy.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StatusSource {
    /// The endpoint omitted `status`; final 2xx was inferred.
    DefaultSuccess,
    /// The endpoint wrote a `status` clause or a runtime caller selected one.
    Explicit,
}

/// Compile-time endpoint capabilities and declared policy presence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EndpointCapabilities {
    pagination: bool,
    accepted_bytes: bool,
    credential_storage: bool,
    authentication: bool,
    rate_limits: bool,
}

impl EndpointCapabilities {
    #[doc(hidden)]
    #[must_use]
    pub const fn generated(
        pagination: bool,
        accepted_bytes: bool,
        credential_storage: bool,
        authentication: bool,
        rate_limits: bool,
    ) -> Self {
        Self {
            pagination,
            accepted_bytes,
            credential_storage,
            authentication,
            rate_limits,
        }
    }

    /// Returns whether the endpoint supports the generated pagination terminal.
    #[must_use]
    pub const fn pagination(self) -> bool {
        self.pagination
    }

    /// Returns whether the endpoint declares the bounded accepted-bytes
    /// terminal.
    #[must_use]
    pub const fn accepted_bytes(self) -> bool {
        self.accepted_bytes
    }

    /// Returns whether accepted responses can update endpoint-backed
    /// credential storage.
    #[must_use]
    pub const fn credential_storage(self) -> bool {
        self.credential_storage
    }

    /// Returns whether the effective declaration includes authentication.
    #[must_use]
    pub const fn authentication(self) -> bool {
        self.authentication
    }

    /// Returns whether the effective declaration includes rate limiting.
    #[must_use]
    pub const fn rate_limits(self) -> bool {
        self.rate_limits
    }
}

#[derive(Clone)]
enum DescriptionText {
    Static(&'static str),
    Owned(Arc<str>),
}

impl DescriptionText {
    const fn from_static(value: &'static str) -> Self {
        Self::Static(value)
    }

    fn from_owned(value: impl Into<Arc<str>>) -> Self {
        Self::Owned(value.into())
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Static(value) => value,
            Self::Owned(value) => value,
        }
    }
}

/// A read-only, secret-free description shared by generated and runtime
/// endpoint values.
#[derive(Clone)]
pub struct EndpointDescription {
    id: EndpointId,
    method: Method,
    source_path: DescriptionText,
    route_shape: DescriptionText,
    body_category: RequestBodyCategory,
    body_specification: Option<&'static str>,
    response_acquisition: ResponseAcquisition,
    capabilities: EndpointCapabilities,
    timeout: TimeoutSelection,
    accepted_status: AcceptedStatus,
    status_source: StatusSource,
    effect: Effect,
    effect_source: EffectSource,
    contract: Option<SharedEndpointContractDescription>,
}

impl EndpointDescription {
    pub(crate) fn with_runtime_owner(
        &self,
        timeout: Option<Duration>,
        authentication: bool,
        rate_limits: bool,
    ) -> Self {
        let mut selected = self.clone();
        if matches!(selected.timeout, TimeoutSelection::InheritClient) {
            selected.timeout =
                timeout.map_or(TimeoutSelection::InheritClient, TimeoutSelection::Explicit);
        }
        selected.capabilities.authentication |= authentication;
        selected.capabilities.rate_limits |= rate_limits;
        selected
    }

    #[doc(hidden)]
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        reason = "generated descriptions contain one field for every public contract fact"
    )]
    pub fn generated(
        id: EndpointId,
        method: Method,
        source_path: &'static str,
        route_shape: &'static str,
        body_category: RequestBodyCategory,
        body_specification: Option<&'static str>,
        response_acquisition: ResponseAcquisition,
        capabilities: EndpointCapabilities,
        timeout: TimeoutSelection,
        accepted_status: AcceptedStatus,
        status_source: StatusSource,
        effect: Effect,
        effect_source: EffectSource,
        contract: EndpointContractDescription,
    ) -> Self {
        Self {
            id,
            method,
            source_path: DescriptionText::from_static(source_path),
            route_shape: DescriptionText::from_static(route_shape),
            body_category,
            body_specification,
            response_acquisition,
            capabilities,
            timeout,
            accepted_status,
            status_source,
            effect,
            effect_source,
            contract: Some(Arc::new(contract)),
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "runtime descriptions freeze the same complete public contract shape"
    )]
    pub(crate) fn runtime(
        id: EndpointId,
        method: Method,
        source_path: Arc<str>,
        route_shape: Arc<str>,
        body_category: RequestBodyCategory,
        body_specification: Option<&'static str>,
        response_acquisition: ResponseAcquisition,
        capabilities: EndpointCapabilities,
        timeout: TimeoutSelection,
        accepted_status: AcceptedStatus,
        status_source: StatusSource,
        effect: Effect,
        effect_source: EffectSource,
    ) -> Self {
        Self {
            id,
            method,
            source_path: DescriptionText::from_owned(source_path),
            route_shape: DescriptionText::from_owned(route_shape),
            body_category,
            body_specification,
            response_acquisition,
            capabilities,
            timeout,
            accepted_status,
            status_source,
            effect,
            effect_source,
            contract: None,
        }
    }

    /// Returns the stable public contract identity.
    #[must_use]
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// Returns the typed endpoint identity.
    #[must_use]
    pub const fn endpoint_id(&self) -> &EndpointId {
        &self.id
    }

    /// Returns the application HTTP method.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the static declaration path or bounded runtime description path.
    #[must_use]
    pub fn source_path(&self) -> &str {
        self.source_path.as_str()
    }

    /// Returns a route template or structural shape without request values.
    #[must_use]
    pub fn route_shape(&self) -> &str {
        self.route_shape.as_str()
    }

    /// Returns the statically known request-body category.
    #[must_use]
    pub const fn body_category(&self) -> RequestBodyCategory {
        self.body_category
    }

    /// Returns the static body-specification identity when one is declared.
    #[must_use]
    pub const fn body_specification(&self) -> Option<&'static str> {
        self.body_specification
    }

    /// Returns the core-owned accepted-response acquisition category.
    #[must_use]
    pub const fn response_acquisition(&self) -> ResponseAcquisition {
        self.response_acquisition
    }

    /// Returns the endpoint's compile-time capabilities and declared policy
    /// presence.
    #[must_use]
    pub const fn capabilities(&self) -> EndpointCapabilities {
        self.capabilities
    }

    /// Returns the endpoint-level operation-timeout selection.
    #[must_use]
    pub const fn timeout(&self) -> TimeoutSelection {
        self.timeout
    }

    /// Returns the endpoint-owned accepted final status policy.
    #[must_use]
    pub const fn accepted_status(&self) -> &AcceptedStatus {
        &self.accepted_status
    }

    /// Returns whether status policy was explicit or defaulted.
    #[must_use]
    pub const fn status_source(&self) -> StatusSource {
        self.status_source
    }

    /// Returns the resolved semantic resend permission.
    #[must_use]
    pub const fn effect(&self) -> Effect {
        self.effect
    }

    /// Returns whether effect was explicit or method-derived.
    #[must_use]
    pub const fn effect_source(&self) -> EffectSource {
        self.effect_source
    }

    /// Returns detailed generated contract facts when this description belongs
    /// to a generated endpoint. Runtime-defined endpoints describe their
    /// runtime-bound facts through the common accessors and return `None` here.
    #[must_use]
    pub fn contract(&self) -> Option<&EndpointContractDescription> {
        self.contract.as_deref()
    }
}

impl fmt::Debug for EndpointDescription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EndpointDescription")
            .field("id", &self.id)
            .field("method", &self.method)
            .field("source_path", &self.source_path())
            .field("route_shape", &self.route_shape())
            .field("body_category", &self.body_category)
            .field("body_specification", &self.body_specification)
            .field("response_acquisition", &self.response_acquisition)
            .field("capabilities", &self.capabilities)
            .field("timeout", &self.timeout)
            .field("accepted_status", &self.accepted_status)
            .field("status_source", &self.status_source)
            .field("effect", &self.effect)
            .field("effect_source", &self.effect_source)
            .field("contract", &self.contract)
            .finish()
    }
}

/// A credential-sanitized, owned projection of real public-head preparation.
///
/// Planning never contains native requests, authentication material, protected
/// rate-limit keys, encoded body bytes, or response values. Public URL values
/// may still contain operational identifiers or personally identifiable
/// information.
pub struct RequestPlan {
    description: EndpointDescription,
    observed_url: Arc<str>,
    public_headers: SanitizedHeaderMap,
    resolved_timeout: Option<Duration>,
    page_index: u32,
    rate_limit_policy_count: usize,
}

impl RequestPlan {
    pub(crate) fn new(
        description: EndpointDescription,
        observed_url: Arc<str>,
        public_headers: SanitizedHeaderMap,
        resolved_timeout: Option<Duration>,
        page_index: u32,
        rate_limit_policy_count: usize,
    ) -> Self {
        Self {
            description,
            observed_url,
            public_headers,
            resolved_timeout,
            page_index,
            rate_limit_policy_count,
        }
    }

    /// Returns the endpoint's static contract description.
    #[must_use]
    pub const fn description(&self) -> &EndpointDescription {
        &self.description
    }

    /// Returns the stable endpoint ID.
    #[must_use]
    pub fn endpoint_id(&self) -> &str {
        self.description.id()
    }

    /// Returns the application HTTP method.
    #[must_use]
    pub const fn method(&self) -> &Method {
        self.description.method()
    }

    /// Returns the redaction-aware, credential-free logical URL produced by
    /// real public-head shaping.
    ///
    /// Visible public values can still contain operational identifiers or
    /// personally identifiable information.
    #[must_use]
    pub fn observed_url(&self) -> &str {
        &self.observed_url
    }

    /// Returns owned sanitized public headers as explicit caller authority.
    ///
    /// Non-credential public values can still contain personally or
    /// operationally sensitive application data. They are omitted from this
    /// plan's `Debug` implementation and automatic observation.
    #[must_use]
    pub const fn public_headers(&self) -> &SanitizedHeaderMap {
        &self.public_headers
    }

    /// Returns the endpoint-level timeout selection.
    #[must_use]
    pub const fn timeout_selection(&self) -> TimeoutSelection {
        self.description.timeout()
    }

    /// Returns the statically known request-body category without invoking the
    /// request-body specification.
    #[must_use]
    pub const fn body_category(&self) -> RequestBodyCategory {
        self.description.body_category()
    }

    /// Returns the static request-body specification identity when one exists.
    #[must_use]
    pub const fn body_specification(&self) -> Option<&'static str> {
        self.description.body_specification()
    }

    /// Returns the core-owned accepted-response acquisition category.
    #[must_use]
    pub const fn response_acquisition(&self) -> ResponseAcquisition {
        self.description.response_acquisition()
    }

    /// Returns compile-time endpoint capabilities and declared policy presence.
    #[must_use]
    pub const fn capabilities(&self) -> EndpointCapabilities {
        self.description.capabilities()
    }

    /// Returns the timeout resolved against the immutable client snapshot.
    #[must_use]
    pub const fn resolved_timeout(&self) -> Option<Duration> {
        self.resolved_timeout
    }

    /// Returns the zero-based page index represented by this plan.
    #[must_use]
    pub const fn page_index(&self) -> u32 {
        self.page_index
    }

    /// Returns the number of declared effective rate-limit policies without
    /// exposing their resolved keys.
    #[must_use]
    pub const fn rate_limit_policy_count(&self) -> usize {
        self.rate_limit_policy_count
    }

    /// Returns false because planning deliberately never invokes body metadata
    /// or materialization callbacks.
    #[must_use]
    pub const fn dynamic_body_metadata_available(&self) -> bool {
        false
    }
}

impl fmt::Debug for RequestPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestPlan")
            .field("description", &self.description)
            .field("observed_url", &"<explicit accessor only>")
            .field("public_header_count", &self.public_headers.len())
            .field("resolved_timeout", &self.resolved_timeout)
            .field("page_index", &self.page_index)
            .field("rate_limit_policy_count", &self.rate_limit_policy_count)
            .field("dynamic_body_metadata_available", &false)
            .finish()
    }
}

const fn valid_endpoint_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_ENDPOINT_ID_BYTES {
        return false;
    }

    let mut index = 0;
    let mut segment_start = 0;
    while index <= bytes.len() {
        if index == bytes.len() || bytes[index] == b'.' {
            let segment_len = index - segment_start;
            if segment_len == 0 || segment_len > MAX_ENDPOINT_ID_SEGMENT_BYTES {
                return false;
            }
            if !is_lower_alphanumeric(bytes[segment_start])
                || !is_lower_alphanumeric(bytes[index - 1])
            {
                return false;
            }
            segment_start = index + 1;
        } else if !is_endpoint_id_byte(bytes[index]) {
            return false;
        }
        index += 1;
    }
    true
}

fn validate_endpoint_id(value: &str) -> Result<(), ConfigurationError> {
    if valid_endpoint_id(value) {
        Ok(())
    } else {
        Err(ConfigurationError::new(
            ConfigurationErrorKind::InvalidValue,
        ))
    }
}

const fn is_lower_alphanumeric(byte: u8) -> bool {
    (byte >= b'a' && byte <= b'z') || (byte >= b'0' && byte <= b'9')
}

const fn is_endpoint_id_byte(byte: u8) -> bool {
    is_lower_alphanumeric(byte) || byte == b'_' || byte == b'-'
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::*;

    const STATIC_ID: EndpointId = EndpointId::from_static("users.get_by_id");

    #[test]
    fn static_and_owned_ids_share_bounded_value_semantics() {
        let owned = EndpointId::new("users.get_by_id").expect("owned ID");
        let borrowed =
            EndpointId::try_from("users.get_by_id").expect("fallible borrowed construction");
        let transferred = EndpointId::try_from(String::from("users.get_by_id"))
            .expect("fallible owned construction");
        assert!(STATIC_ID.is_static());
        assert!(!owned.is_static());
        assert_eq!(STATIC_ID, owned);
        assert_eq!(STATIC_ID, borrowed);
        assert_eq!(STATIC_ID, transferred);
        assert_eq!(STATIC_ID, "users.get_by_id");
        assert_eq!(STATIC_ID.to_string(), "users.get_by_id");
        assert_eq!(format!("{STATIC_ID:?}"), "EndpointId(\"users.get_by_id\")");

        let mut hash = HashSet::new();
        assert!(hash.insert(STATIC_ID.clone()));
        assert!(!hash.insert(owned.clone()));
        let mut ordered = BTreeSet::new();
        assert!(ordered.insert(STATIC_ID.clone()));
        assert!(!ordered.insert(owned));
    }

    #[test]
    fn runtime_ids_enforce_the_complete_static_grammar_without_echoing_input() {
        for invalid in [
            "",
            ".leading",
            "trailing.",
            "two..dots",
            "Upper.case",
            "under._edge",
            "dash.-edge",
            "space value",
            "é",
        ] {
            let error = EndpointId::new(invalid).expect_err("invalid endpoint identity");
            assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
            if !invalid.is_empty() {
                assert!(!error.to_string().contains(invalid));
                assert!(!format!("{error:?}").contains(invalid));
            }
        }
        let maximum = format!("{}.b.{}", "a".repeat(63), "a".repeat(62));
        assert_eq!(maximum.len(), MAX_ENDPOINT_ID_BYTES);
        assert!(EndpointId::new(maximum).is_ok());
        let excessive = format!("{}.b.{}", "a".repeat(63), "a".repeat(63));
        assert_eq!(excessive.len(), MAX_ENDPOINT_ID_BYTES + 1);
        assert!(EndpointId::new(excessive).is_err());
        assert!(EndpointId::new(format!("{}.b", "a".repeat(63))).is_ok());
        assert!(EndpointId::new(format!("{}.b", "a".repeat(64))).is_err());
    }

    #[test]
    fn borrowed_endpoint_id_first_excess_is_rejected_before_ownership() {
        let maximum = format!("{}.b.{}", "a".repeat(63), "a".repeat(62));
        let exact = EndpointId::new(maximum.as_str()).expect("exact endpoint identity limit");
        assert_eq!(exact.as_str(), maximum);

        let first_excess = format!("{}.b.{}", "a".repeat(63), "a".repeat(63));
        assert_eq!(first_excess.len(), MAX_ENDPOINT_ID_BYTES + 1);
        EndpointId::new(first_excess.as_str()).expect_err("first excess must fail");

        let sentinel = "endpoint-identity-sentinel-427c".repeat(4_096);
        let error = EndpointId::new(sentinel.as_str()).expect_err("huge borrowed input must fail");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
        assert!(!error.to_string().contains("endpoint-identity-sentinel"));
        assert!(!format!("{error:?}").contains("endpoint-identity-sentinel"));
    }
}
