use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use url::Url;

use super::{
    EndpointSpec, HeadBuilder, PreparedHead, RequestMeta, validate_public_header,
    validate_public_header_name,
};
use crate::atom::{OrderedValues, RequestAtom, encode_atom};
use crate::auth::{Auth, AuthPlan};
use crate::body::{
    EmptyBody, PreparedBodySpec, PreparedRequestBody, RequestBody, RequestBodyContext,
    RequestBodyMetadata, RequestRepresentation,
};
use crate::codec::Binary;
use crate::codec::EncodeBody;
use crate::construction::{ErrorLatch, StructuralBudget, StructuralLimitError};
use crate::endpoint::{
    AcceptedStatus, Effect, EffectSource, EndpointCapabilities, EndpointDescription, EndpointId,
    MAX_FORMATTED_PUBLIC_VALUE_BYTES, MAX_RUNTIME_ENDPOINT_COMPONENTS,
    MAX_RUNTIME_ENDPOINT_METHOD_BYTES, MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory,
    StatusSource, TimeoutSelection,
};
use crate::error::{ApiError, ConfigurationError, ConfigurationErrorKind};
use crate::limits::MAX_AUTH_PLACEMENTS;
use crate::rate_limit::{RateLimitPlan, RateLimitPolicy};
use crate::response::{Discard, Empty, ResponseDriver, ResponseRepresentation};

struct RuntimeEndpointParts {
    endpoint_id: EndpointId,
    method: Method,
    path_segments: Vec<PreparedRuntimeValue>,
    path_inherits: bool,
    query: Vec<(Arc<str>, PreparedRuntimeValue)>,
    query_replacements: Vec<Arc<str>>,
    query_off: bool,
    headers: Vec<(HeaderName, HeaderValue, bool)>,
    header_replacements: Vec<HeaderName>,
    headers_off: bool,
    body: PreparedRequestBody,
    body_category: RequestBodyCategory,
    body_specification: Option<&'static str>,
    auth: Vec<Auth>,
    auth_inherits: bool,
    timeout: TimeoutSelection,
    accepted_status: Option<AcceptedStatus>,
    effect: Option<Effect>,
    rate_limits: Vec<RateLimitPolicy>,
    budget: StructuralBudget,
}

/// Builds one owned hand-written endpoint for execution by a generated client.
///
/// Construction is side-effect free with respect to Concord execution: it does not allocate an
/// operation identity, invoke codecs or factories, acquire credentials or limiter capacity, call
/// hooks, construct a native request, or perform HTTP.
///
/// Public request shape is accounted incrementally against the canonical runtime endpoint limits.
/// The first structural failure is latched: later mutations do not format values or consume
/// iterators, and [`RuntimeEndpointBuilder::build`] returns only its safe configuration category.
/// Fields are intentionally private.
pub struct RuntimeEndpointBuilder<R = Empty, Output = ()> {
    state: ErrorLatch<RuntimeEndpointParts, ConfigurationErrorKind>,
    response: PhantomData<fn() -> (R, Output)>,
}

/// One runtime-defined ordinary public value and its observation class.
///
/// This wrapper deliberately has no `Debug` implementation. Sensitivity only
/// controls sanitized operational evidence; it does not make the value a
/// credential or hide it from the server request.
pub struct RuntimeValue<T> {
    value: T,
    sensitive: bool,
}

impl<T> RuntimeValue<T> {
    /// Classifies a runtime public value as visible in sanitized evidence.
    #[must_use]
    pub const fn visible(value: T) -> Self {
        Self {
            value,
            sensitive: false,
        }
    }

    /// Classifies a runtime public value as redacted in sanitized evidence.
    #[must_use]
    pub const fn sensitive(value: T) -> Self {
        Self {
            value,
            sensitive: true,
        }
    }

    fn into_parts(self) -> (T, bool) {
        (self.value, self.sensitive)
    }
}

impl<T> From<T> for RuntimeValue<T> {
    fn from(value: T) -> Self {
        Self::visible(value)
    }
}

struct PreparedRuntimeValue {
    value: String,
    sensitive: bool,
}

/// One immutable hand-written endpoint accepted by generated clients' sealed request paths.
///
/// The selected generated client supplies its base URL and complete managed runtime. Only
/// authentication explicitly attached through [`RuntimeEndpointBuilder::auth`] belongs to this
/// endpoint. A credential reference binds the endpoint's authentication to the client build that
/// created the reference; clones share that context, while independent client builds are rejected.
/// Fields are intentionally private.
pub struct RuntimeEndpoint<R = Empty, Output = ()> {
    description: EndpointDescription,
    path_segments: Box<[PreparedRuntimeValue]>,
    path_inherits: bool,
    query: Box<[(Arc<str>, PreparedRuntimeValue)]>,
    query_replacements: Box<[Arc<str>]>,
    query_off: bool,
    headers: Box<[(HeaderName, HeaderValue, bool)]>,
    header_replacements: Box<[HeaderName]>,
    headers_off: bool,
    body: PreparedRequestBody,
    auth: Arc<AuthPlan>,
    auth_inherits: bool,
    rate_limit_plan: Arc<RateLimitPlan>,
    head_prefix: Option<RuntimeHeadPrefix>,
    response: PhantomData<fn() -> (R, Output)>,
}

type RuntimeHeadPrefix =
    Arc<dyn Fn(HeadBuilder) -> Result<HeadBuilder, ApiError> + Send + Sync + 'static>;

/// Generated binding authority for a runtime-defined endpoint.
///
/// Implementations are emitted only for a concrete generated client or scope
/// owner. Runtime binding is deliberately fallible and remains separate from
/// statically matched generated-intent binding.
#[doc(hidden)]
pub trait RuntimeEndpointBinding<R, Output>
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
{
    /// Core endpoint shape retained by the returned lazy request.
    type BoundEndpoint: EndpointSpec<Output = Output, Response = R>;

    /// Validates and binds one runtime endpoint under this owner.
    fn try_bind_runtime_endpoint(
        &self,
        endpoint: RuntimeEndpoint<R, Output>,
    ) -> Result<crate::Request<Self::BoundEndpoint>, ApiError>;

    /// Builds the same inert plan used by binding validation.
    fn plan_runtime_endpoint(
        &self,
        endpoint: &RuntimeEndpoint<R, Output>,
    ) -> Result<crate::endpoint::RequestPlan, ApiError>;
}

impl<R, Output> RuntimeEndpoint<R, Output>
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
{
    /// Fallibly binds this runtime-defined endpoint under a generated client
    /// or selected scope without performing body, provider, limiter, observer,
    /// or HTTP work.
    pub fn try_bind<B>(self, owner: &B) -> Result<crate::Request<B::BoundEndpoint>, ApiError>
    where
        B: RuntimeEndpointBinding<R, Output>,
    {
        owner.try_bind_runtime_endpoint(self)
    }

    /// Builds an inert credential-free, redaction-aware plan under a generated
    /// client or selected scope without binding or executing the endpoint.
    pub fn plan<B>(&self, owner: &B) -> Result<crate::endpoint::RequestPlan, ApiError>
    where
        B: RuntimeEndpointBinding<R, Output>,
    {
        owner.plan_runtime_endpoint(self)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn __concord_inherits_path(&self) -> bool {
        self.path_inherits
    }

    #[doc(hidden)]
    #[must_use]
    pub fn __concord_inherits_auth(&self) -> bool {
        self.auth_inherits
    }

    #[doc(hidden)]
    #[must_use]
    pub fn __concord_description_for_owner(
        &self,
        timeout: Option<Duration>,
        authentication: bool,
        rate_limits: bool,
    ) -> EndpointDescription {
        self.description.with_runtime_owner(
            timeout,
            authentication && self.auth_inherits,
            rate_limits,
        )
    }

    #[doc(hidden)]
    #[must_use]
    pub fn __concord_bind_owner<F>(
        mut self,
        timeout: Option<Duration>,
        authentication: bool,
        rate_limits: bool,
        prefix: F,
    ) -> Self
    where
        F: Fn(HeadBuilder) -> Result<HeadBuilder, ApiError> + Send + Sync + 'static,
    {
        self.description = self.description.with_runtime_owner(
            timeout,
            authentication && self.auth_inherits,
            rate_limits,
        );
        self.head_prefix = Some(Arc::new(prefix));
        self
    }
}

impl RuntimeEndpoint<Empty> {
    /// Starts a no-content runtime endpoint with the supplied stable identity and HTTP method.
    ///
    /// The method token is retained only when it is at most
    /// [`MAX_RUNTIME_ENDPOINT_METHOD_BYTES`] bytes. Its bytes and one component
    /// are charged to the runtime endpoint structural budget.
    #[must_use]
    pub fn builder(endpoint_id: EndpointId, method: Method) -> RuntimeEndpointBuilder<Empty> {
        let method_bytes = method.as_str().len();
        if method_bytes > MAX_RUNTIME_ENDPOINT_METHOD_BYTES {
            return RuntimeEndpointBuilder {
                state: ErrorLatch::from_error(ConfigurationErrorKind::InvalidValue),
                response: PhantomData,
            };
        }
        let Ok(budget) = reserve_runtime_structure(StructuralBudget::new(), 1, method_bytes) else {
            return RuntimeEndpointBuilder {
                state: ErrorLatch::from_error(ConfigurationErrorKind::InvalidValue),
                response: PhantomData,
            };
        };
        RuntimeEndpointBuilder {
            state: ErrorLatch::from_value(RuntimeEndpointParts {
                endpoint_id,
                method,
                path_segments: Vec::new(),
                path_inherits: true,
                query: Vec::new(),
                query_replacements: Vec::new(),
                query_off: false,
                headers: Vec::new(),
                header_replacements: Vec::new(),
                headers_off: false,
                body: PreparedRequestBody::new::<EmptyBody, _>(()),
                body_category: RequestBodyCategory::Empty,
                body_specification: None,
                auth: Vec::new(),
                auth_inherits: true,
                timeout: TimeoutSelection::InheritClient,
                accepted_status: None,
                effect: None,
                rate_limits: Vec::new(),
                budget,
            }),
            response: PhantomData,
        }
    }
}

impl<R, Output> RuntimeEndpointBuilder<R, Output> {
    /// Appends one formatted value as exactly one validated, URL-encoded path
    /// segment.
    #[must_use]
    pub fn path<T>(self, value: T) -> Self
    where
        T: RequestAtom,
    {
        self.path_value(RuntimeValue::visible(value))
    }

    /// Appends one runtime public value with an explicit observation class.
    #[must_use]
    pub fn path_value<T>(self, value: RuntimeValue<T>) -> Self
    where
        T: RequestAtom,
    {
        self.update(move |mut parts| {
            let (value, sensitive) = value.into_parts();
            let mut budget = reserve_runtime_structure(parts.budget, 1, 0)
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let maximum =
                runtime_format_limit(budget).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let value =
                encode_atom(&value, maximum).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            if !runtime_path_segment_is_valid(&value) {
                return Err(ConfigurationErrorKind::InvalidRoute);
            }
            budget = reserve_runtime_structure(budget, 0, value.len())
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            parts.budget = budget;
            parts
                .path_segments
                .push(PreparedRuntimeValue { value, sensitive });
            Ok(parts)
        })
    }

    /// Replaces every generated runtime path segment with the supplied
    /// ordered segments while retaining the client's fixed base path.
    ///
    /// Every value is validated as one decoded segment and is encoded exactly
    /// once during public-head preparation.
    #[must_use]
    pub fn replace_path<V>(self, values: V) -> Self
    where
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.replace_path_values(RuntimeValue::visible(values))
    }

    /// Replaces runtime path values with an explicit shared observation class.
    #[must_use]
    pub fn replace_path_values<V>(self, values: RuntimeValue<V>) -> Self
    where
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.update(move |mut parts| {
            let (values, sensitive) = values.into_parts();
            let removed_bytes = parts
                .path_segments
                .iter()
                .map(|value| value.value.len())
                .sum();
            let mut budget = parts.budget;
            budget
                .try_replace(
                    parts.path_segments.len(),
                    removed_bytes,
                    0,
                    0,
                    MAX_RUNTIME_ENDPOINT_COMPONENTS,
                    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                )
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let mut segments = Vec::new();
            for value in values.ordered_values() {
                budget = reserve_runtime_structure(budget, 1, 0)
                    .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
                let maximum = runtime_format_limit(budget)
                    .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
                let value = encode_atom(value, maximum)
                    .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
                if !runtime_path_segment_is_valid(&value) {
                    return Err(ConfigurationErrorKind::InvalidRoute);
                }
                budget = reserve_runtime_structure(budget, 0, value.len())
                    .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
                segments.push(PreparedRuntimeValue { value, sensitive });
            }
            parts.path_segments = segments;
            parts.path_inherits = false;
            parts.budget = budget;
            Ok(parts)
        })
    }

    /// Clears every generated runtime path segment, retaining the client's
    /// fixed base path.
    #[must_use]
    pub fn path_off(self) -> Self {
        self.update(|mut parts| {
            let bytes = parts
                .path_segments
                .iter()
                .map(|value| value.value.len())
                .sum();
            parts
                .budget
                .try_replace(
                    parts.path_segments.len(),
                    bytes,
                    0,
                    0,
                    MAX_RUNTIME_ENDPOINT_COMPONENTS,
                    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                )
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            parts.path_segments.clear();
            parts.path_inherits = false;
            Ok(parts)
        })
    }

    /// Replaces every exact-key occurrence and appends one ordered public query
    /// pair at this operation's position.
    ///
    /// The name is borrowed through [`AsRef<str>`] so its byte length can be rejected before
    /// Concord copies it.
    #[must_use]
    pub fn query<N, T>(self, name: N, value: T) -> Self
    where
        N: AsRef<str>,
        T: RequestAtom,
    {
        self.query_value(name, RuntimeValue::visible(value))
    }

    /// Assigns one query value with an explicit observation class.
    #[must_use]
    pub fn query_value<N, T>(self, name: N, value: RuntimeValue<T>) -> Self
    where
        N: AsRef<str>,
        T: RequestAtom,
    {
        self.update(move |mut parts| {
            let (value, sensitive) = value.into_parts();
            let name = name.as_ref();
            let (removed_components, removed_bytes) = runtime_query_usage(&parts.query, name);
            let mut budget = parts.budget;
            budget
                .try_replace(
                    removed_components,
                    removed_bytes,
                    0,
                    0,
                    MAX_RUNTIME_ENDPOINT_COMPONENTS,
                    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                )
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            budget = reserve_runtime_structure(budget, 1, name.len())
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let maximum =
                runtime_format_limit(budget).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let value =
                encode_atom(&value, maximum).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            budget = reserve_runtime_structure(budget, 0, value.len())
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let name: Arc<str> = Arc::from(name);
            parts.budget = budget;
            parts
                .query
                .retain(|(candidate, _)| candidate.as_ref() != name.as_ref());
            mark_query_replacement(&mut parts, &name)?;
            parts
                .query
                .push((name, PreparedRuntimeValue { value, sensitive }));
            Ok(parts)
        })
    }

    /// Applies query assignment when `value` is present; absence removes every
    /// exact-key occurrence.
    #[must_use]
    pub fn query_optional<N, T>(self, name: N, value: Option<T>) -> Self
    where
        N: AsRef<str>,
        T: RequestAtom,
    {
        self.query_optional_value(name, RuntimeValue::visible(value))
    }

    /// Applies optional query assignment with an explicit observation class.
    #[must_use]
    pub fn query_optional_value<N, T>(self, name: N, value: RuntimeValue<Option<T>>) -> Self
    where
        N: AsRef<str>,
        T: RequestAtom,
    {
        let (value, sensitive) = value.into_parts();
        match value {
            Some(value) => self.query_value(name, RuntimeValue { value, sensitive }),
            None => self.remove_query(name),
        }
    }

    /// Replaces the exact query key only when the outer value is present.
    /// Absence preserves every existing occurrence.
    #[must_use]
    pub fn query_when_present<N, T>(self, name: N, value: Option<T>) -> Self
    where
        N: AsRef<str>,
        T: RequestAtom,
    {
        self.query_when_present_value(name, RuntimeValue::visible(value))
    }

    /// Applies a conditional query assignment with an observation class.
    #[must_use]
    pub fn query_when_present_value<N, T>(self, name: N, value: RuntimeValue<Option<T>>) -> Self
    where
        N: AsRef<str>,
        T: RequestAtom,
    {
        let (value, sensitive) = value.into_parts();
        match value {
            Some(value) => self.query_value(name, RuntimeValue { value, sensitive }),
            None => self,
        }
    }

    /// Replaces every exact-key occurrence with repeated ordered query pairs
    /// while preserving iterator order.
    ///
    /// Concord consumes only the values that fit plus one withheld first-excess value. An empty
    /// iterator retains no copy of `name`.
    #[must_use]
    pub fn query_each<N, V>(self, name: N, values: V) -> Self
    where
        N: AsRef<str>,
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.query_each_values(name, RuntimeValue::visible(values))
    }

    /// Assigns repeated query values with one shared observation class.
    #[must_use]
    pub fn query_each_values<N, V>(self, name: N, values: RuntimeValue<V>) -> Self
    where
        N: AsRef<str>,
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.update(move |mut parts| {
            let (values, sensitive) = values.into_parts();
            let name = name.as_ref();
            if name.len() > MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            let (removed_components, removed_bytes) = runtime_query_usage(&parts.query, name);
            let mut budget = parts.budget;
            budget
                .try_replace(
                    removed_components,
                    removed_bytes,
                    0,
                    0,
                    MAX_RUNTIME_ENDPOINT_COMPONENTS,
                    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                )
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let name: Arc<str> = Arc::from(name);
            let mut replacements = Vec::new();
            for value in values.ordered_values() {
                budget = reserve_runtime_structure(budget, 1, name.len())
                    .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
                let maximum = runtime_format_limit(budget)
                    .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
                let value = encode_atom(value, maximum)
                    .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
                budget = reserve_runtime_structure(budget, 0, value.len())
                    .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
                replacements.push((Arc::clone(&name), PreparedRuntimeValue { value, sensitive }));
            }
            parts
                .query
                .retain(|(candidate, _)| candidate.as_ref() != name.as_ref());
            mark_query_replacement(&mut parts, &name)?;
            parts.query.extend(replacements);
            parts.budget = budget;
            Ok(parts)
        })
    }

    /// Replaces the exact query key with a present ordered sequence while
    /// preserving existing occurrences when the outer value is absent.
    #[must_use]
    pub fn query_each_when_present<N, V>(self, name: N, values: Option<V>) -> Self
    where
        N: AsRef<str>,
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.query_each_when_present_values(name, RuntimeValue::visible(values))
    }

    /// Conditionally assigns repeated query values with an observation class.
    #[must_use]
    pub fn query_each_when_present_values<N, V>(
        self,
        name: N,
        values: RuntimeValue<Option<V>>,
    ) -> Self
    where
        N: AsRef<str>,
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        let (values, sensitive) = values.into_parts();
        match values {
            Some(values) => self.query_each_values(
                name,
                RuntimeValue {
                    value: values,
                    sensitive,
                },
            ),
            None => self,
        }
    }

    /// Appends one ordered public query pair without removing prior values.
    #[must_use]
    pub fn append_query<N, T>(self, name: N, value: T) -> Self
    where
        N: AsRef<str>,
        T: RequestAtom,
    {
        self.append_query_value(name, RuntimeValue::visible(value))
    }

    /// Appends one query value with an explicit observation class.
    #[must_use]
    pub fn append_query_value<N, T>(self, name: N, value: RuntimeValue<T>) -> Self
    where
        N: AsRef<str>,
        T: RequestAtom,
    {
        self.update(move |mut parts| {
            let name = name.as_ref();
            let name: Arc<str> = Arc::from(name);
            append_runtime_query_value(&mut parts, &name, value)?;
            Ok(parts)
        })
    }

    /// Appends repeated ordered public query pairs without removing prior
    /// values.
    #[must_use]
    pub fn append_query_each<N, V>(self, name: N, values: V) -> Self
    where
        N: AsRef<str>,
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.append_query_each_values(name, RuntimeValue::visible(values))
    }

    /// Appends repeated query values with one shared observation class.
    #[must_use]
    pub fn append_query_each_values<N, V>(self, name: N, values: RuntimeValue<V>) -> Self
    where
        N: AsRef<str>,
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.update(move |mut parts| {
            let (values, sensitive) = values.into_parts();
            let name: Arc<str> = Arc::from(name.as_ref());
            for value in values.ordered_values() {
                append_runtime_query_value(&mut parts, &name, RuntimeValue { value, sensitive })?;
            }
            Ok(parts)
        })
    }

    /// Removes every exact, case-sensitive query key occurrence.
    #[must_use]
    pub fn remove_query<N>(self, name: N) -> Self
    where
        N: AsRef<str>,
    {
        self.update(move |mut parts| {
            let name = name.as_ref();
            let (components, bytes) = runtime_query_usage(&parts.query, name);
            parts
                .budget
                .try_replace(
                    components,
                    bytes,
                    0,
                    0,
                    MAX_RUNTIME_ENDPOINT_COMPONENTS,
                    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                )
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            parts
                .query
                .retain(|(candidate, _)| candidate.as_ref() != name);
            let name: Arc<str> = Arc::from(name);
            mark_query_replacement(&mut parts, &name)?;
            Ok(parts)
        })
    }

    /// Clears the complete runtime public query patch.
    #[must_use]
    pub fn query_off(self) -> Self {
        self.update(|mut parts| {
            let components = parts.query.len();
            let bytes = parts
                .query
                .iter()
                .map(|(name, value)| name.len() + value.value.len())
                .sum();
            parts
                .budget
                .try_replace(
                    components,
                    bytes,
                    0,
                    0,
                    MAX_RUNTIME_ENDPOINT_COMPONENTS,
                    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                )
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            parts.query.clear();
            parts.query_replacements.clear();
            parts.query_off = true;
            Ok(parts)
        })
    }

    /// Inserts one public header, replacing an earlier case-insensitively equal name.
    #[must_use]
    pub fn header<T>(self, name: HeaderName, value: T) -> Self
    where
        T: RequestAtom,
    {
        self.header_value(name, RuntimeValue::visible(value))
    }

    /// Assigns one header value with an explicit observation class.
    #[must_use]
    pub fn header_value<T>(self, name: HeaderName, value: RuntimeValue<T>) -> Self
    where
        T: RequestAtom,
    {
        self.update(move |mut parts| {
            remove_runtime_headers(&mut parts, &name)?;
            mark_header_replacement(&mut parts, &name)?;
            append_runtime_header(&mut parts, name, value)?;
            Ok(parts)
        })
    }

    /// Applies header assignment; outer absence clears every occurrence.
    #[must_use]
    pub fn header_optional<T>(self, name: HeaderName, value: Option<T>) -> Self
    where
        T: RequestAtom,
    {
        self.header_optional_value(name, RuntimeValue::visible(value))
    }

    /// Applies optional header assignment with an observation class.
    #[must_use]
    pub fn header_optional_value<T>(self, name: HeaderName, value: RuntimeValue<Option<T>>) -> Self
    where
        T: RequestAtom,
    {
        let (value, sensitive) = value.into_parts();
        match value {
            Some(value) => self.header_value(name, RuntimeValue { value, sensitive }),
            None => self.remove_header(name),
        }
    }

    /// Applies header assignment only when the outer value is present.
    #[must_use]
    pub fn header_when_present<T>(self, name: HeaderName, value: Option<T>) -> Self
    where
        T: RequestAtom,
    {
        self.header_when_present_value(name, RuntimeValue::visible(value))
    }

    /// Applies conditional header assignment with an observation class.
    #[must_use]
    pub fn header_when_present_value<T>(
        self,
        name: HeaderName,
        value: RuntimeValue<Option<T>>,
    ) -> Self
    where
        T: RequestAtom,
    {
        let (value, sensitive) = value.into_parts();
        match value {
            Some(value) => self.header_value(name, RuntimeValue { value, sensitive }),
            None => self,
        }
    }

    /// Replaces every occurrence with one ordered sequence. An empty sequence
    /// explicitly clears the field.
    #[must_use]
    pub fn header_each<V>(self, name: HeaderName, values: V) -> Self
    where
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.header_each_values(name, RuntimeValue::visible(values))
    }

    /// Assigns repeated header values with one shared observation class.
    #[must_use]
    pub fn header_each_values<V>(self, name: HeaderName, values: RuntimeValue<V>) -> Self
    where
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.update(move |mut parts| {
            let (values, sensitive) = values.into_parts();
            remove_runtime_headers(&mut parts, &name)?;
            mark_header_replacement(&mut parts, &name)?;
            for value in values.ordered_values() {
                append_runtime_header(&mut parts, name.clone(), RuntimeValue { value, sensitive })?;
            }
            Ok(parts)
        })
    }

    /// Replaces every occurrence with a present ordered sequence and preserves
    /// the field when the outer sequence is absent.
    #[must_use]
    pub fn header_each_when_present<V>(self, name: HeaderName, values: Option<V>) -> Self
    where
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.header_each_when_present_values(name, RuntimeValue::visible(values))
    }

    /// Conditionally assigns repeated headers with an observation class.
    #[must_use]
    pub fn header_each_when_present_values<V>(
        self,
        name: HeaderName,
        values: RuntimeValue<Option<V>>,
    ) -> Self
    where
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        let (values, sensitive) = values.into_parts();
        match values {
            Some(values) => self.header_each_values(
                name,
                RuntimeValue {
                    value: values,
                    sensitive,
                },
            ),
            None => self,
        }
    }

    /// Appends one public header occurrence without removing prior values.
    #[must_use]
    pub fn append_header<T>(self, name: HeaderName, value: T) -> Self
    where
        T: RequestAtom,
    {
        self.append_header_value(name, RuntimeValue::visible(value))
    }

    /// Appends one header value with an explicit observation class.
    #[must_use]
    pub fn append_header_value<T>(self, name: HeaderName, value: RuntimeValue<T>) -> Self
    where
        T: RequestAtom,
    {
        self.update(move |mut parts| {
            append_runtime_header(&mut parts, name, value)?;
            Ok(parts)
        })
    }

    /// Appends an ordered sequence of public header occurrences.
    #[must_use]
    pub fn append_header_each<V>(self, name: HeaderName, values: V) -> Self
    where
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.append_header_each_values(name, RuntimeValue::visible(values))
    }

    /// Appends repeated headers with one shared observation class.
    #[must_use]
    pub fn append_header_each_values<V>(self, name: HeaderName, values: RuntimeValue<V>) -> Self
    where
        V: OrderedValues,
        V::Item: RequestAtom,
    {
        self.update(move |mut parts| {
            let (values, sensitive) = values.into_parts();
            for value in values.ordered_values() {
                append_runtime_header(&mut parts, name.clone(), RuntimeValue { value, sensitive })?;
            }
            Ok(parts)
        })
    }

    /// Removes one public header via case-insensitive HTTP field identity.
    #[must_use]
    pub fn remove_header(self, name: HeaderName) -> Self {
        self.update(move |mut parts| {
            remove_runtime_headers(&mut parts, &name)?;
            mark_header_replacement(&mut parts, &name)?;
            Ok(parts)
        })
    }

    /// Clears all runtime public headers.
    #[must_use]
    pub fn headers_off(self) -> Self {
        self.update(|mut parts| {
            let components = parts.headers.len();
            let bytes = parts
                .headers
                .iter()
                .map(|(name, value, _)| name.as_str().len() + value.as_bytes().len())
                .sum();
            parts
                .budget
                .try_replace(
                    components,
                    bytes,
                    0,
                    0,
                    MAX_RUNTIME_ENDPOINT_COMPONENTS,
                    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                )
                .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
            parts.headers.clear();
            parts.header_replacements.clear();
            parts.headers_off = true;
            Ok(parts)
        })
    }

    /// Replaces the selected request body with one existing explicit body value.
    #[must_use]
    pub fn body(self, body: RequestBody) -> Self {
        self.body_spec::<RequestBody, _>(body)
    }

    /// Replaces the selected request body with an arbitrary request specification.
    ///
    /// Concord stores the application input without invoking body metadata,
    /// encoding, factories, or streams. The input's own construction and
    /// retained state are an application extension trust boundary.
    #[must_use]
    pub fn body_spec<B, Input>(self, input: Input) -> Self
    where
        B: RequestRepresentation<Input>,
        Input: Send + 'static,
    {
        self.update(move |mut parts| {
            parts.body = PreparedRequestBody::new::<B, Input>(input);
            parts.body_category = RequestBodyCategory::Specification;
            parts.body_specification = Some(std::any::type_name::<B>());
            Ok(parts)
        })
    }

    /// Replaces the selected request body with one lazily encoded codec input.
    #[must_use]
    pub fn body_codec<C>(self, value: C::Input) -> RuntimeEndpointBuilder<R, Output>
    where
        C: EncodeBody,
    {
        self.body_spec::<C, C::Input>(value)
    }

    /// Appends one typed authentication placement.
    #[must_use]
    pub fn auth(self, auth: Auth) -> Self {
        self.update(move |mut parts| {
            let placements = auth.placement_count();
            let count = parts
                .auth
                .len()
                .checked_add(placements)
                .ok_or(ConfigurationErrorKind::InvalidAuthentication)?;
            if count > MAX_AUTH_PLACEMENTS {
                return Err(ConfigurationErrorKind::InvalidAuthentication);
            }
            let budget =
                reserve_runtime_structure(parts.budget, placements, auth.public_name_bytes())
                    .map_err(|_| ConfigurationErrorKind::InvalidAuthentication)?;
            parts.budget = budget;
            parts.auth.push(auth);
            Ok(parts)
        })
    }

    /// Replaces every prior runtime authentication placement with `auth`.
    #[must_use]
    pub fn replace_auth(self, auth: Auth) -> Self {
        self.auth_off().auth(auth)
    }

    /// Clears every runtime authentication placement.
    #[must_use]
    pub fn auth_off(self) -> Self {
        self.update(|mut parts| {
            let components = parts.auth.iter().map(Auth::placement_count).sum();
            let bytes = parts.auth.iter().map(Auth::public_name_bytes).sum();
            parts
                .budget
                .try_replace(
                    components,
                    bytes,
                    0,
                    0,
                    MAX_RUNTIME_ENDPOINT_COMPONENTS,
                    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                )
                .map_err(|_| ConfigurationErrorKind::InvalidAuthentication)?;
            parts.auth.clear();
            parts.auth_inherits = false;
            Ok(parts)
        })
    }

    /// Replaces the Concord operation deadline with one explicit duration.
    #[must_use]
    pub fn timeout(self, duration: Duration) -> Self {
        self.update(move |mut parts| {
            parts.timeout = TimeoutSelection::Explicit(duration);
            Ok(parts)
        })
    }

    /// Resets timeout inheritance to the immutable built-client value.
    #[must_use]
    pub fn reset_timeout(self) -> Self {
        self.update(|mut parts| {
            parts.timeout = TimeoutSelection::InheritClient;
            Ok(parts)
        })
    }

    /// Selects the endpoint's semantic resend permission explicitly.
    #[must_use]
    pub fn effect(self, effect: Effect) -> Self {
        self.update(move |mut parts| {
            parts.effect = Some(effect);
            Ok(parts)
        })
    }

    /// Selects the endpoint-owned accepted final status policy.
    #[must_use]
    pub fn status(self, accepted_status: AcceptedStatus) -> Self {
        self.update(move |mut parts| {
            parts.accepted_status = Some(accepted_status);
            Ok(parts)
        })
    }

    /// Selects and validates explicit accepted final status codes.
    #[must_use]
    pub fn status_codes(self, codes: impl IntoIterator<Item = StatusCode>) -> Self {
        match AcceptedStatus::codes(codes) {
            Ok(status) => self.status(status),
            Err(_) => self.update(|_| Err(ConfigurationErrorKind::InvalidValue)),
        }
    }

    /// Appends one structurally validated rate-limit policy.
    #[must_use]
    pub fn rate_limit(self, policy: RateLimitPolicy) -> Self {
        self.update(move |mut parts| {
            RateLimitPlan::validate_policy_append(&parts.rate_limits, &policy).map_err(
                |error| {
                    drop(error);
                    ConfigurationErrorKind::InvalidRateLimit
                },
            )?;
            let retained_bytes = policy
                .retained_structure_bytes()
                .ok_or(ConfigurationErrorKind::InvalidRateLimit)?;
            let budget = reserve_runtime_structure(parts.budget, 1, retained_bytes)
                .map_err(|_| ConfigurationErrorKind::InvalidRateLimit)?;
            parts.budget = budget;
            parts.rate_limits.push(policy);
            Ok(parts)
        })
    }

    /// Selects an arbitrary accepted response specification.
    #[must_use]
    pub fn response<S, NextOutput>(self) -> RuntimeEndpointBuilder<S, NextOutput>
    where
        S: ResponseRepresentation<NextOutput>,
        NextOutput: Send + 'static,
    {
        self.with_response()
    }

    /// Selects a complete bounded accepted bytes response.
    #[must_use]
    pub fn accepted_bytes(self) -> RuntimeEndpointBuilder<Binary, bytes::Bytes> {
        self.with_response()
    }

    /// Selects an accepted response that must contain zero decoded body bytes.
    #[must_use]
    pub fn response_empty(self) -> RuntimeEndpointBuilder<Empty, ()> {
        self.with_response()
    }

    /// Selects an accepted response whose bounded body is intentionally ignored.
    #[must_use]
    pub fn response_discard(self) -> RuntimeEndpointBuilder<Discard, ()> {
        self.with_response()
    }

    /// Validates structural configuration and returns one immutable endpoint value.
    pub fn build(self) -> Result<RuntimeEndpoint<R, Output>, ConfigurationError>
    where
        R: ResponseRepresentation<Output>,
        Output: Send + 'static,
    {
        let RuntimeEndpointParts {
            endpoint_id,
            method,
            path_segments,
            path_inherits,
            query,
            query_replacements,
            query_off,
            headers,
            header_replacements,
            headers_off,
            body,
            body_category,
            body_specification,
            auth,
            auth_inherits,
            timeout,
            accepted_status,
            effect,
            rate_limits,
            budget: _,
        } = self.state.into_result().map_err(ConfigurationError::new)?;

        let mut validated_headers = HeaderMap::new();
        for (name, value, _) in &headers {
            validated_headers.append(name.clone(), value.clone());
        }

        let mut validation_url = Url::parse("https://runtime.invalid/")
            .expect("the fixed runtime validation origin is valid");
        validation_url.query_pairs_mut().extend_pairs(
            query
                .iter()
                .map(|(name, value)| (name.as_ref(), value.value.as_str())),
        );
        let mut auth_plan = AuthPlan::default();
        for auth in auth {
            auth_plan
                .push(auth, &validation_url, &validated_headers)
                .map_err(|error| {
                    drop(error);
                    ConfigurationError::new(ConfigurationErrorKind::InvalidAuthentication)
                })?;
        }
        auth_plan
            .validate_public(&validation_url, &validated_headers)
            .map_err(|error| {
                drop(error);
                ConfigurationError::new(ConfigurationErrorKind::InvalidAuthentication)
            })?;

        let response_acquisition = <R::Driver as ResponseDriver>::ACQUISITION;
        let effect_source = if effect.is_some() {
            EffectSource::Explicit
        } else {
            EffectSource::MethodDefault
        };
        let effect = effect.unwrap_or_else(|| Effect::for_method(&method));
        let status_source = if accepted_status.is_some() {
            StatusSource::Explicit
        } else {
            StatusSource::DefaultSuccess
        };
        let accepted_status = accepted_status.unwrap_or_default();
        let description = EndpointDescription::runtime(
            endpoint_id,
            method,
            Arc::from("runtime"),
            runtime_route_shape(path_segments.len()),
            body_category,
            body_specification,
            response_acquisition,
            EndpointCapabilities::generated(
                false,
                false,
                false,
                !auth_plan.is_empty(),
                !rate_limits.is_empty(),
            ),
            timeout,
            accepted_status,
            status_source,
            effect,
            effect_source,
        );

        Ok(RuntimeEndpoint {
            description,
            path_segments: path_segments.into_boxed_slice(),
            path_inherits,
            query: query.into_boxed_slice(),
            query_replacements: query_replacements.into_boxed_slice(),
            query_off,
            headers: headers.into_boxed_slice(),
            header_replacements: header_replacements.into_boxed_slice(),
            headers_off,
            body,
            auth: Arc::new(auth_plan),
            auth_inherits,
            rate_limit_plan: Arc::new(RateLimitPlan::from_validated_policies(rate_limits)),
            head_prefix: None,
            response: PhantomData,
        })
    }

    fn with_response<Next, NextOutput>(self) -> RuntimeEndpointBuilder<Next, NextOutput> {
        RuntimeEndpointBuilder {
            state: self.state,
            response: PhantomData,
        }
    }

    fn update<F>(self, update: F) -> Self
    where
        F: FnOnce(RuntimeEndpointParts) -> Result<RuntimeEndpointParts, ConfigurationErrorKind>,
    {
        Self {
            state: self.state.then(update),
            response: PhantomData,
        }
    }
}

impl<R, Output> RuntimeEndpoint<R, Output>
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
{
    /// Returns the safe bounded runtime endpoint description.
    #[must_use]
    pub const fn description(&self) -> &EndpointDescription {
        &self.description
    }
}

impl<R, Output> EndpointSpec for RuntimeEndpoint<R, Output>
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
{
    type Input = PreparedRequestBody;
    type Body = PreparedBodySpec;
    type Output = Output;
    type Response = R;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::from_description(&self.description)
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        if let Some(prefix) = &self.head_prefix {
            builder = prefix(builder)?;
        }
        for segment in &self.path_segments {
            builder.push_prepared_segment(&segment.value, segment.sensitive)?;
        }
        if self.query_off {
            builder.clear_query_values()?;
        }
        for name in &self.query_replacements {
            builder.remove_query_values(name)?;
        }
        for (name, value) in &self.query {
            builder.push_prepared_query_value(name.as_ref(), &value.value, value.sensitive)?;
        }
        if self.headers_off {
            builder.clear_header_values()?;
        }
        for name in &self.header_replacements {
            builder.remove_header_values(name.as_str())?;
        }
        for (name, value, sensitive) in &self.headers {
            builder.append_prepared_header(name.clone(), value.clone(), *sensitive)?;
        }
        if !self.auth_inherits {
            builder.clear_auth()?;
        }
        builder.append_prepared_auth_plan(&self.auth)?;
        builder.append_prepared_rate_limit_plan(&self.rate_limit_plan)?;
        builder.finish()
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, crate::body::BodyError> {
        PreparedBodySpec::metadata(&self.body, context)
    }

    fn into_body(self) -> Self::Input {
        self.body
    }
}

fn reserve_runtime_structure(
    mut budget: StructuralBudget,
    components: usize,
    public_bytes: usize,
) -> Result<StructuralBudget, StructuralLimitError> {
    budget.try_add(
        components,
        public_bytes,
        MAX_RUNTIME_ENDPOINT_COMPONENTS,
        MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
    )?;
    Ok(budget)
}

fn runtime_format_limit(budget: StructuralBudget) -> Result<usize, StructuralLimitError> {
    budget
        .remaining_public_bytes(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES)
        .map(|remaining| remaining.min(MAX_FORMATTED_PUBLIC_VALUE_BYTES))
}

fn append_runtime_query_value<T>(
    parts: &mut RuntimeEndpointParts,
    name: &Arc<str>,
    value: RuntimeValue<T>,
) -> Result<(), ConfigurationErrorKind>
where
    T: RequestAtom,
{
    let (value, sensitive) = value.into_parts();
    let mut budget = reserve_runtime_structure(parts.budget, 1, name.len())
        .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
    let maximum = runtime_format_limit(budget).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
    let value = encode_atom(&value, maximum).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
    budget = reserve_runtime_structure(budget, 0, value.len())
        .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
    parts.budget = budget;
    parts
        .query
        .push((Arc::clone(name), PreparedRuntimeValue { value, sensitive }));
    Ok(())
}

fn append_runtime_header<T>(
    parts: &mut RuntimeEndpointParts,
    name: HeaderName,
    value: RuntimeValue<T>,
) -> Result<(), ConfigurationErrorKind>
where
    T: RequestAtom,
{
    let (value, sensitive) = value.into_parts();
    validate_public_header_name(&name)?;
    let mut budget = reserve_runtime_structure(parts.budget, 1, name.as_str().len())
        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
    let maximum =
        runtime_format_limit(budget).map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
    let value = encode_atom(&value, maximum).map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
    let value = validate_public_header(&name, &value)?;
    budget = reserve_runtime_structure(budget, 0, value.as_bytes().len())
        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
    parts.headers.push((name, value, sensitive));
    parts.budget = budget;
    Ok(())
}

fn remove_runtime_headers(
    parts: &mut RuntimeEndpointParts,
    name: &HeaderName,
) -> Result<(), ConfigurationErrorKind> {
    let (components, bytes) = parts
        .headers
        .iter()
        .filter(|(candidate, _, _)| candidate == name)
        .fold((0_usize, 0_usize), |(count, bytes), (name, value, _)| {
            (
                count + 1,
                bytes + name.as_str().len() + value.as_bytes().len(),
            )
        });
    parts
        .budget
        .try_replace(
            components,
            bytes,
            0,
            0,
            MAX_RUNTIME_ENDPOINT_COMPONENTS,
            MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
        )
        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
    parts.headers.retain(|(candidate, _, _)| candidate != name);
    Ok(())
}

fn runtime_query_usage(query: &[(Arc<str>, PreparedRuntimeValue)], name: &str) -> (usize, usize) {
    query
        .iter()
        .filter(|(candidate, _)| candidate.as_ref() == name)
        .fold((0_usize, 0_usize), |(components, bytes), (name, value)| {
            (components + 1, bytes + name.len() + value.value.len())
        })
}

fn mark_query_replacement(
    parts: &mut RuntimeEndpointParts,
    name: &Arc<str>,
) -> Result<(), ConfigurationErrorKind> {
    let already_retained = parts
        .query_replacements
        .iter()
        .any(|candidate| candidate == name);
    let retained_bytes = parts
        .query_replacements
        .iter()
        .map(|candidate| candidate.len())
        .sum::<usize>();
    if name.len() > MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES
        || (!already_retained
            && retained_bytes
                .checked_add(name.len())
                .is_none_or(|bytes| bytes > MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES))
        || (parts.query_replacements.len() >= MAX_RUNTIME_ENDPOINT_COMPONENTS && !already_retained)
    {
        return Err(ConfigurationErrorKind::InvalidValue);
    }
    if !already_retained {
        parts.query_replacements.push(Arc::clone(name));
    }
    Ok(())
}

fn mark_header_replacement(
    parts: &mut RuntimeEndpointParts,
    name: &HeaderName,
) -> Result<(), ConfigurationErrorKind> {
    let already_retained = parts
        .header_replacements
        .iter()
        .any(|candidate| candidate == name);
    let retained_bytes = parts
        .header_replacements
        .iter()
        .map(|candidate| candidate.as_str().len())
        .sum::<usize>();
    if (!already_retained
        && retained_bytes
            .checked_add(name.as_str().len())
            .is_none_or(|bytes| bytes > MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES))
        || (parts.header_replacements.len() >= MAX_RUNTIME_ENDPOINT_COMPONENTS && !already_retained)
    {
        return Err(ConfigurationErrorKind::InvalidHeader);
    }
    if !already_retained {
        parts.header_replacements.push(name.clone());
    }
    Ok(())
}

fn runtime_path_segment_is_valid(segment: &str) -> bool {
    !segment.is_empty()
        && !matches!(segment, "." | "..")
        && !segment.contains('/')
        && !segment.contains('\\')
        && !segment.as_bytes().windows(3).any(|window| {
            window[0] == b'%' && window[1].is_ascii_hexdigit() && window[2].is_ascii_hexdigit()
        })
}

fn runtime_route_shape(path_segment_count: usize) -> Arc<str> {
    if path_segment_count == 0 {
        return Arc::from("<client-base>");
    }
    let mut shape = String::new();
    shape.push_str("<client-base-host>/<client-base>");
    for _ in 0..path_segment_count {
        shape.push_str("/<runtime-segment>");
    }
    Arc::from(shape)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bytes::Bytes;
    use http::header::AUTHORIZATION;

    use super::*;
    use crate::auth::{AccessToken, CredentialRef, SlotSelector};
    use crate::codec::Text;
    use crate::hooks::DebugLevel;
    use crate::rate_limit::{
        MAX_RATE_LIMIT_POLICIES_PER_PLAN, RateLimitId, RateLimitKey, RateLimitWindow,
    };
    use crate::request::RequestOptions;
    use crate::response::ByteStream;

    const FORMAT_SENTINEL: &str = "runtime-format-first-excess-sentinel-3f4a";
    const RATE_ID_SENTINEL: &str = "runtime-rate-id-sentinel-527d";
    const RATE_KEY_SENTINEL: &str = "runtime-rate-key-sentinel-28b1";

    fn access_token(value: impl AsRef<str>) -> AccessToken {
        AccessToken::try_new(value).expect("bounded test access token")
    }

    #[derive(Clone, Copy)]
    struct EmptyDisplay;

    impl RequestAtom for EmptyDisplay {
        fn write_atom(
            &self,
            _output: &mut crate::atom::BoundedAtomWriter<'_>,
        ) -> Result<(), crate::atom::AtomError> {
            Ok(())
        }
    }

    struct TrackedEmptyDisplay {
        formatted: Arc<AtomicUsize>,
    }

    impl RequestAtom for TrackedEmptyDisplay {
        fn write_atom(
            &self,
            _output: &mut crate::atom::BoundedAtomWriter<'_>,
        ) -> Result<(), crate::atom::AtomError> {
            self.formatted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct PanicBeyondFirstExcess {
        next_calls: Arc<AtomicUsize>,
        values: Vec<TrackedEmptyDisplay>,
    }

    struct CheckedOrderedIter<'a> {
        values: std::slice::Iter<'a, TrackedEmptyDisplay>,
        next_calls: &'a AtomicUsize,
    }

    impl<'a> Iterator for CheckedOrderedIter<'a> {
        type Item = &'a TrackedEmptyDisplay;

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.next_calls.fetch_add(1, Ordering::SeqCst);
            assert!(call < 3, "iterator consumed beyond the first excess");
            self.values.next()
        }
    }

    impl OrderedValues for PanicBeyondFirstExcess {
        type Item = TrackedEmptyDisplay;
        type Iter<'a> = CheckedOrderedIter<'a>;

        fn ordered_values(&self) -> Self::Iter<'_> {
            CheckedOrderedIter {
                values: self.values.iter(),
                next_calls: &self.next_calls,
            }
        }
    }

    struct RetentionProbe {
        _retained: Arc<()>,
        formatted: Arc<AtomicUsize>,
        text: &'static str,
    }

    impl RequestAtom for RetentionProbe {
        fn write_atom(
            &self,
            output: &mut crate::atom::BoundedAtomWriter<'_>,
        ) -> Result<(), crate::atom::AtomError> {
            self.formatted.fetch_add(1, Ordering::SeqCst);
            output.push_str(self.text)
        }
    }

    struct PanicDisplay;

    impl RequestAtom for PanicDisplay {
        fn write_atom(
            &self,
            _output: &mut crate::atom::BoundedAtomWriter<'_>,
        ) -> Result<(), crate::atom::AtomError> {
            panic!("latched builder encoded a later value")
        }
    }

    struct PanicIntoIterator {
        into_iter_calls: Arc<AtomicUsize>,
    }

    impl OrderedValues for PanicIntoIterator {
        type Item = &'static str;
        type Iter<'a> = std::iter::Empty<&'a &'static str>;

        fn ordered_values(&self) -> Self::Iter<'_> {
            self.into_iter_calls.fetch_add(1, Ordering::SeqCst);
            panic!("latched builder consumed a later ordered sequence")
        }
    }

    struct PanicName;

    impl AsRef<str> for PanicName {
        fn as_ref(&self) -> &str {
            panic!("latched builder inspected a later query name")
        }
    }

    struct SentinelDisplay {
        prefix: String,
        _retained: Arc<()>,
    }

    impl RequestAtom for SentinelDisplay {
        fn write_atom(
            &self,
            output: &mut crate::atom::BoundedAtomWriter<'_>,
        ) -> Result<(), crate::atom::AtomError> {
            output.push_str(&self.prefix)?;
            output.push_str(FORMAT_SENTINEL)
        }
    }

    struct InertBodyInput {
        calls: Arc<AtomicUsize>,
    }

    struct InertBodySpec;

    impl RequestRepresentation<InertBodyInput> for InertBodySpec {
        fn metadata(
            input: &InertBodyInput,
            _context: RequestBodyContext<'_>,
        ) -> Result<RequestBodyMetadata, crate::body::BodyError> {
            input.calls.fetch_add(1, Ordering::SeqCst);
            panic!("runtime endpoint build invoked body metadata")
        }

        fn prepare(
            input: InertBodyInput,
            _context: RequestBodyContext<'_>,
        ) -> Result<RequestBody, crate::body::BodyError> {
            input.calls.fetch_add(1, Ordering::SeqCst);
            panic!("runtime endpoint build invoked body preparation")
        }
    }

    fn head<R, Output>(endpoint: &RuntimeEndpoint<R, Output>, base: &str) -> PreparedHead
    where
        R: ResponseRepresentation<Output>,
        Output: Send + 'static,
    {
        let builder = HeadBuilder::new(
            &Url::parse(base).expect("base URL"),
            endpoint.request_meta(),
            &RequestOptions::new(),
            DebugLevel::None,
            None,
        );
        endpoint.prepare_head(builder).expect("prepared head")
    }

    fn rule(id: &str, value: &str) -> RateLimitPolicy {
        RateLimitPolicy::new(
            RateLimitId::new(id).expect("policy ID"),
            RateLimitKey::from_parts([
                crate::rate_limit::RateLimitKeyPart::new("tenant", value).expect("tenant key part")
            ])
            .expect("rate key"),
            NonZeroU32::new(1).expect("cost"),
            [RateLimitWindow::new(
                NonZeroU32::new(10).expect("capacity"),
                Duration::from_secs(1),
            )
            .expect("window")],
        )
        .expect("rule")
    }

    fn credential() -> CredentialRef<AccessToken> {
        CredentialRef::new(SlotSelector::static_value(
            access_token("runtime-auth-secret-sentinel"),
            Duration::from_secs(1),
        ))
    }

    #[test]
    fn generic_response_specifications_replace_the_previous_selection() {
        fn buffered(_: RuntimeEndpointBuilder<Text, String>) {}
        fn bytes(_: RuntimeEndpointBuilder<Binary, Bytes>) {}
        fn no_content(_: RuntimeEndpointBuilder<Empty>) {}
        fn stream(_: RuntimeEndpointBuilder<Binary, ByteStream>) {}

        buffered(
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.buffered"), Method::GET)
                .accepted_bytes()
                .response::<Text, String>(),
        );
        bytes(
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.bytes"), Method::GET)
                .response::<Text, String>()
                .accepted_bytes(),
        );
        no_content(
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.none"), Method::GET)
                .response::<Binary, ByteStream>()
                .response_empty(),
        );
        stream(
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.stream"), Method::GET)
                .response_empty()
                .response::<Binary, ByteStream>(),
        );
    }

    #[test]
    fn composition_applies_v2_query_header_body_deadline_and_rate_algebras() {
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.compose"), Method::POST)
                .query("first", 1)
                .query_optional("omitted", Option::<u8>::None)
                .query_each("repeat", [2, 3])
                .header(HeaderName::from_static("x-choice"), "old")
                .header(
                    HeaderName::from_bytes(b"X-Choice").expect("mixed-case header"),
                    "new",
                )
                .body_codec::<Text>("discarded".to_owned())
                .body(RequestBody::bytes(Bytes::from_static(b"selected")))
                .timeout(Duration::from_secs(5))
                .reset_timeout()
                .rate_limit(rule("first", "a"))
                .rate_limit(rule("second", "b"))
                .build()
                .expect("valid runtime endpoint");

        assert_eq!(
            endpoint.description.timeout(),
            TimeoutSelection::InheritClient
        );
        assert_eq!(endpoint.rate_limit_plan.policies().len(), 2);
        assert_eq!(
            endpoint.rate_limit_plan.policies()[0].id().as_str(),
            "first"
        );
        assert_eq!(
            endpoint.rate_limit_plan.policies()[1].id().as_str(),
            "second"
        );
        let metadata = PreparedBodySpec::metadata(
            &endpoint.body,
            RequestBodyContext {
                endpoint: endpoint.description.id(),
                method: endpoint.description.method(),
                body_limit: std::num::NonZeroUsize::new(1024).expect("positive body limit"),
            },
        )
        .expect("body metadata");
        assert_eq!(metadata.known_length(), Some(8));

        let head = head(&endpoint, "https://example.test/base/");
        let query = head
            .logical_url()
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        assert_eq!(
            query,
            [("first", "1"), ("repeat", "2"), ("repeat", "3")]
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
        );
        assert_eq!(
            head.public_headers()
                .get("x-choice")
                .and_then(|value| value.to_str().ok()),
            Some("new")
        );

        let explicit =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.timeout"), Method::GET)
                .timeout(Duration::from_secs(1))
                .reset_timeout()
                .timeout(Duration::from_secs(7))
                .build()
                .expect("last timeout selection wins");
        assert_eq!(
            explicit.description.timeout(),
            TimeoutSelection::Explicit(Duration::from_secs(7))
        );
    }

    #[test]
    fn runtime_route_and_public_head_patch_operations_are_explicit() {
        let removed = HeaderName::from_static("x-removed");
        let retained = HeaderName::from_static("x-retained");
        let endpoint = RuntimeEndpoint::builder(
            EndpointId::from_static("runtime.patch_algebra"),
            Method::GET,
        )
        .path("discarded")
        .replace_path(["kept"])
        .append_query("tag", 1)
        .append_query("tag", 2)
        .query("tag", 3)
        .append_query("tag", 4)
        .query("gone", 5)
        .query_optional("gone", Option::<u8>::None)
        .header(removed.clone(), "discarded")
        .remove_header(removed)
        .header(retained.clone(), "kept")
        .effect(Effect::NonIdempotent)
        .build()
        .expect("valid runtime patch algebra");

        assert_eq!(endpoint.description.effect(), Effect::NonIdempotent);
        assert_eq!(endpoint.description.effect_source(), EffectSource::Explicit);
        assert!(!endpoint.description.route_shape().contains("runtime-label"));
        let mut prepared = head(&endpoint, "https://example.test/fixed/");
        assert_eq!(prepared.logical_url().host_str(), Some("example.test"));
        assert_eq!(prepared.logical_url().path(), "/fixed/kept");
        assert_eq!(
            prepared
                .logical_url()
                .query_pairs()
                .map(|(name, value)| (name.into_owned(), value.into_owned()))
                .collect::<Vec<_>>(),
            [
                ("tag".to_owned(), "3".to_owned()),
                ("tag".to_owned(), "4".to_owned())
            ]
        );
        assert_eq!(
            prepared
                .public_headers()
                .get(&retained)
                .and_then(|value| value.to_str().ok()),
            Some("kept")
        );
        assert!(!prepared.public_headers().contains_key("x-removed"));
        prepared
            .apply_media_headers(Some("text/plain"), None, false)
            .expect("response representation negotiation");
        assert_eq!(
            prepared
                .public_headers()
                .get(http::header::ACCEPT)
                .and_then(|value| value.to_str().ok()),
            Some("text/plain")
        );

        let cleared =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.route_off"), Method::GET)
                .path("discarded")
                .path_off()
                .build()
                .expect("valid route suppression");
        let cleared = head(&cleared, "https://example.test/fixed/");
        assert_eq!(cleared.logical_url().host_str(), Some("example.test"));
        assert_eq!(cleared.logical_url().path(), "/fixed/");
    }

    #[test]
    fn runtime_path_values_are_validated_then_encoded_once() {
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.route"), Method::GET)
                .path("ordinary")
                .path("space value")
                .path("query?value")
                .path("fragment#value")
                .path("percent%value")
                .build()
                .expect("valid runtime route");
        let head = head(&endpoint, "https://example.test/fixed/base/");

        assert_eq!(
            head.logical_url()
                .path_segments()
                .unwrap()
                .collect::<Vec<_>>(),
            [
                "fixed",
                "base",
                "ordinary",
                "space%20value",
                "query%3Fvalue",
                "fragment%23value",
                "percent%25value",
            ]
        );
        assert_eq!(head.logical_url().query(), None);
        assert_eq!(head.logical_url().fragment(), None);

        for invalid in [
            "",
            ".",
            "..",
            "slash/value",
            "back\\value",
            "encoded%2Fvalue",
        ] {
            let error = RuntimeEndpoint::builder(
                EndpointId::from_static("runtime.invalid_path"),
                Method::GET,
            )
            .path(invalid)
            .build()
            .err()
            .expect("invalid or pre-encoded path segment");
            assert_eq!(error.kind(), ConfigurationErrorKind::InvalidRoute);
        }
    }

    #[test]
    fn build_uses_safe_typed_configuration_errors() {
        const SENTINEL: &str = "prepared-sensitive-header-56e8c97f";
        let error =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.invalid"), Method::GET)
                .header(AUTHORIZATION, SENTINEL)
                .build()
                .err()
                .expect("reserved header must fail");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidHeader);
        assert!(!error.to_string().contains(SENTINEL));
        assert!(!format!("{error:?}").contains(SENTINEL));

        let empty = EndpointId::new("").expect_err("empty identity must fail");
        assert_eq!(empty.kind(), ConfigurationErrorKind::InvalidValue);
    }

    #[test]
    fn runtime_endpoint_method_limit_is_checked_before_later_shape_and_charged_to_budget() {
        let exact_text = "M".repeat(MAX_RUNTIME_ENDPOINT_METHOD_BYTES);
        let exact = Method::from_bytes(exact_text.as_bytes()).expect("exact method token");
        let remaining = MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - exact_text.len();
        let endpoint = RuntimeEndpoint::builder(
            EndpointId::from_static("runtime.method.exact"),
            exact.clone(),
        )
        .path("x".repeat(remaining))
        .build()
        .expect("exact method and aggregate byte limits");
        assert_eq!(endpoint.description().method(), &exact);
        let rendered = format!("{:?}", endpoint.description());
        assert!(rendered.contains(&exact_text));
        assert!(rendered.len() < exact_text.len() + 768);

        let aggregate_error =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.method.aggregate"), exact)
                .path("x".repeat(remaining + 1))
                .build()
                .err()
                .expect("method bytes consume the aggregate budget");
        assert_eq!(aggregate_error.kind(), ConfigurationErrorKind::InvalidValue);

        let excessive_text = "N".repeat(MAX_RUNTIME_ENDPOINT_METHOD_BYTES + 1);
        let excessive = Method::from_bytes(excessive_text.as_bytes()).expect("excess method token");
        let error =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.method.excess"), excessive)
                .path(PanicDisplay)
                .build()
                .err()
                .expect("first excess method must remain latched");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
        assert!(!error.to_string().contains(&excessive_text));
        assert!(!format!("{error:?}").contains(&excessive_text));
    }

    #[test]
    fn runtime_endpoint_query_values_stops_at_first_excess() {
        let mut builder =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.query"), Method::GET);
        for _ in 0..(MAX_RUNTIME_ENDPOINT_COMPONENTS - 3) {
            builder = builder.append_query("", EmptyDisplay);
        }

        let next_calls = Arc::new(AtomicUsize::new(0));
        let formatted = Arc::new(AtomicUsize::new(0));
        let error = builder
            .append_query_each(
                "",
                PanicBeyondFirstExcess {
                    next_calls: Arc::clone(&next_calls),
                    values: (0..3)
                        .map(|_| TrackedEmptyDisplay {
                            formatted: Arc::clone(&formatted),
                        })
                        .collect(),
                },
            )
            .build()
            .err()
            .expect("third repeated query value is the first excess");

        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
        assert_eq!(next_calls.load(Ordering::SeqCst), 3);
        assert_eq!(formatted.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn runtime_endpoint_infinite_query_values_returns_configuration_error() {
        let error =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.infinite"), Method::GET)
                .query_each("", vec![EmptyDisplay; MAX_RUNTIME_ENDPOINT_COMPONENTS + 1])
                .build()
                .err()
                .expect("an infinite query iterator must reach the finite component limit");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
    }

    #[test]
    fn runtime_endpoint_first_excess_component_is_not_retained() {
        let mut boundary =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.components"), Method::GET)
                .query_optional("absent", Option::<PanicDisplay>::None);
        for _ in 0..(MAX_RUNTIME_ENDPOINT_COMPONENTS - 1) {
            boundary = boundary.append_query("", EmptyDisplay);
        }
        boundary.build().expect("component boundary");

        let retained = Arc::new(());
        let formatted = Arc::new(AtomicUsize::new(0));
        let mut excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.components"), Method::GET);
        for _ in 0..(MAX_RUNTIME_ENDPOINT_COMPONENTS - 1) {
            excessive = excessive.append_query("", EmptyDisplay);
        }
        let excessive = excessive.path(RetentionProbe {
            _retained: Arc::clone(&retained),
            formatted: Arc::clone(&formatted),
            text: FORMAT_SENTINEL,
        });

        assert_eq!(Arc::strong_count(&retained), 1);
        assert_eq!(formatted.load(Ordering::SeqCst), 0);
        let error = excessive.build().err().expect("first excess component");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
        assert!(!error.to_string().contains(FORMAT_SENTINEL));
        assert!(!format!("{error:?}").contains(FORMAT_SENTINEL));
    }

    #[test]
    fn runtime_endpoint_first_excess_public_byte_is_not_retained() {
        let remaining = MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - Method::GET.as_str().len();
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.bytes"), Method::GET)
            .path("x".repeat(remaining))
            .build()
            .expect("public-byte boundary");

        let retained = Arc::new(());
        let formatted = Arc::new(AtomicUsize::new(0));
        let excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.bytes"), Method::GET)
                .path("x".repeat(remaining))
                .path(RetentionProbe {
                    _retained: Arc::clone(&retained),
                    formatted: Arc::clone(&formatted),
                    text: FORMAT_SENTINEL,
                });

        assert_eq!(Arc::strong_count(&retained), 1);
        assert_eq!(formatted.load(Ordering::SeqCst), 1);
        let error = excessive.build().err().expect("first excess public byte");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
        assert!(!error.to_string().contains(FORMAT_SENTINEL));
        assert!(!format!("{error:?}").contains(FORMAT_SENTINEL));
    }

    #[test]
    fn runtime_endpoint_errored_builder_does_not_consume_later_iterator() {
        let into_iter_calls = Arc::new(AtomicUsize::new(0));
        let error =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.latched"), Method::GET)
                .header(AUTHORIZATION, "reserved")
                .accepted_bytes()
                .query_each(
                    PanicName,
                    PanicIntoIterator {
                        into_iter_calls: Arc::clone(&into_iter_calls),
                    },
                )
                .path(PanicDisplay)
                .build()
                .err()
                .expect("the original structural failure remains latched");

        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidHeader);
        assert_eq!(into_iter_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn runtime_endpoint_header_replacement_updates_budget_exactly() {
        let name = HeaderName::from_static("x-budget");
        let remaining = MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - Method::GET.as_str().len();
        let initial = "a".repeat(remaining - name.as_str().len());
        let replacement = "b";
        let tail = "c".repeat(remaining - name.as_str().len() - replacement.len());

        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.header"), Method::GET)
                .header(name.clone(), initial)
                .header(name.clone(), replacement)
                .path(tail)
                .build()
                .expect("replacement frees exactly the previous retained value bytes");

        assert_eq!(
            endpoint
                .headers
                .iter()
                .find(|(candidate, _, _)| candidate == name)
                .and_then(|(_, value, _)| value.to_str().ok()),
            Some(replacement)
        );

        let retained = Arc::new(());
        let formatted = Arc::new(AtomicUsize::new(0));
        let excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.header"), Method::GET)
                .header(name.clone(), "a")
                .path("x".repeat(remaining - name.as_str().len() - 1))
                .header(
                    name,
                    RetentionProbe {
                        _retained: Arc::clone(&retained),
                        formatted: Arc::clone(&formatted),
                        text: FORMAT_SENTINEL,
                    },
                );

        assert_eq!(Arc::strong_count(&retained), 1);
        assert_eq!(formatted.load(Ordering::SeqCst), 1);
        assert_eq!(
            excessive
                .build()
                .err()
                .expect("oversized replacement")
                .kind(),
            ConfigurationErrorKind::InvalidHeader
        );
    }

    #[test]
    fn runtime_endpoint_auth_first_excess_is_rejected_before_push() {
        let credential = credential();
        let mut boundary =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.auth"), Method::GET);
        for index in 0..MAX_AUTH_PLACEMENTS {
            boundary = boundary.auth(
                Auth::query(format!("runtime-auth-{index}"), credential.clone())
                    .expect("valid authentication placement"),
            );
        }
        boundary.build().expect("authentication placement boundary");

        let mut excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.auth"), Method::GET);
        for index in 0..=MAX_AUTH_PLACEMENTS {
            excessive = excessive.auth(
                Auth::query(format!("runtime-auth-{index}"), credential.clone())
                    .expect("valid authentication placement"),
            );
        }
        let error = excessive
            .build()
            .err()
            .expect("first excess authentication placement");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidAuthentication);
    }

    #[test]
    fn runtime_endpoint_auth_public_name_bytes_are_bounded_and_redacted() {
        const SECRET_SENTINEL: &str = "runtime-auth-material-sentinel-8a20";
        let credential = CredentialRef::new(SlotSelector::static_value(
            access_token(SECRET_SENTINEL),
            Duration::from_secs(1),
        ));
        let remaining = MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - Method::GET.as_str().len();
        let error = RuntimeEndpoint::builder(EndpointId::from_static("runtime.auth"), Method::GET)
            .path("x".repeat(remaining - 1))
            .auth(Auth::query(FORMAT_SENTINEL, credential).expect("valid authentication placement"))
            .build()
            .err()
            .expect("authentication placement exceeds remaining shape bytes");

        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidAuthentication);
        assert!(!error.to_string().contains(FORMAT_SENTINEL));
        assert!(!error.to_string().contains(SECRET_SENTINEL));
        assert!(!format!("{error:?}").contains(FORMAT_SENTINEL));
        assert!(!format!("{error:?}").contains(SECRET_SENTINEL));
    }

    #[test]
    fn runtime_endpoint_rate_limits_obey_same_structural_limits() {
        let mut boundary =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.rate"), Method::GET);
        for index in 0..MAX_RATE_LIMIT_POLICIES_PER_PLAN {
            boundary = boundary.rate_limit(rule(&format!("rule-{index}"), "tenant"));
        }
        boundary.build().expect("rate-rule boundary");

        let mut excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.rate"), Method::GET);
        for index in 0..MAX_RATE_LIMIT_POLICIES_PER_PLAN {
            excessive = excessive.rate_limit(rule(&format!("rule-{index}"), "tenant"));
        }
        excessive = excessive.rate_limit(rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL));
        let error = excessive
            .build()
            .err()
            .expect("first excess rate-limit policy");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidRateLimit);
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!error.to_string().contains(RATE_KEY_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_KEY_SENTINEL));

        let error = RuntimeEndpoint::builder(
            EndpointId::from_static("runtime.rate.duplicate"),
            Method::GET,
        )
        .rate_limit(rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL))
        .rate_limit(rule(RATE_ID_SENTINEL, "second-value"))
        .build()
        .err()
        .expect("duplicate runtime rate-limit policy ID");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidRateLimit);
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!error.to_string().contains(RATE_KEY_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_KEY_SENTINEL));
    }

    #[test]
    fn runtime_endpoint_rate_key_bytes_are_bounded_and_redacted() {
        let remaining = MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - Method::GET.as_str().len();
        let error = RuntimeEndpoint::builder(EndpointId::from_static("runtime.rate"), Method::GET)
            .path("x".repeat(remaining - 1))
            .rate_limit(rule("r", FORMAT_SENTINEL))
            .build()
            .err()
            .expect("rate key exceeds remaining retained-shape bytes");

        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidRateLimit);
        assert!(!error.to_string().contains(FORMAT_SENTINEL));
        assert!(!format!("{error:?}").contains(FORMAT_SENTINEL));
    }

    #[test]
    fn runtime_endpoint_bounded_format_does_not_retain_sentinel() {
        let retained = Arc::new(());
        let builder =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.format"), Method::GET).path(
                SentinelDisplay {
                    prefix: "x".repeat(MAX_FORMATTED_PUBLIC_VALUE_BYTES),
                    _retained: Arc::clone(&retained),
                },
            );

        assert_eq!(Arc::strong_count(&retained), 1);
        let error = builder
            .build()
            .err()
            .expect("the sentinel chunk is the first formatted excess");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
        assert!(!error.to_string().contains(FORMAT_SENTINEL));
        assert!(!format!("{error:?}").contains(FORMAT_SENTINEL));
    }

    #[test]
    fn runtime_endpoint_build_remains_inert() {
        let calls = Arc::new(AtomicUsize::new(0));
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.inert"), Method::POST)
                .body_spec::<InertBodySpec, _>(InertBodyInput {
                    calls: Arc::clone(&calls),
                })
                .build()
                .expect("inert application body input");

        assert_eq!(calls.load(Ordering::SeqCst), 0);
        drop(endpoint);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn runtime_description_contains_shape_but_no_request_values() {
        const SENTINEL: &str = "runtime-description-value-sentinel-74ad";
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.description"), Method::POST)
                .path(SENTINEL)
                .query("q", SENTINEL)
                .header(HeaderName::from_static("x-public"), SENTINEL)
                .body_codec::<Text>(SENTINEL.to_owned())
                .accepted_bytes()
                .build()
                .expect("runtime description endpoint");
        let rendered = format!("{:?}", endpoint.description());
        assert!(!rendered.contains(SENTINEL));
        assert_eq!(endpoint.description().id(), "runtime.description");
        assert_eq!(
            endpoint.description().body_category(),
            RequestBodyCategory::Specification
        );
        assert!(
            endpoint
                .description()
                .body_specification()
                .is_some_and(|name| name.ends_with("Text"))
        );
    }
}
