use std::fmt::Display;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use http::{HeaderMap, HeaderName, HeaderValue, Method};
use url::Url;

use super::{
    EndpointSpec, HeadBuilder, PreparedHead, RequestMeta, validate_public_header,
    validate_public_header_name,
};
use crate::auth::{Auth, AuthPlan};
use crate::body::{
    EmptyBody, PreparedBodySpec, PreparedRequestBody, RequestBody, RequestBodyContext,
    RequestBodyMetadata, RequestBodySpec, Stream,
};
use crate::codec::{EncodeBody, MediaType};
use crate::construction::{ErrorLatch, StructuralBudget, StructuralLimitError, format_bounded};
use crate::endpoint::{
    EndpointCapabilities, EndpointDescription, EndpointId, MAX_FORMATTED_PUBLIC_VALUE_BYTES,
    MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS, MAX_RUNTIME_ENDPOINT_COMPONENTS,
    MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES, RequestBodyCategory, TimeoutSelection,
};
use crate::error::{ApiError, ConfigurationError, ConfigurationErrorKind};
use crate::rate_limit::{RateLimitPlan, RateLimitRule};
use crate::response::{NoContent, ResponseBodySpec, ResponseMode};

struct RuntimeEndpointParts {
    endpoint_id: EndpointId,
    method: Method,
    path_segments: Vec<String>,
    query: Vec<(Arc<str>, String)>,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: PreparedRequestBody,
    body_category: RequestBodyCategory,
    body_specification: Option<&'static str>,
    auth: Vec<Auth>,
    timeout: TimeoutSelection,
    rate_limits: Vec<RateLimitRule>,
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
pub struct RuntimeEndpointBuilder<R = NoContent> {
    state: ErrorLatch<RuntimeEndpointParts, ConfigurationErrorKind>,
    response: PhantomData<R>,
}

/// One immutable hand-written endpoint accepted by every generated client's sealed request path.
///
/// The selected generated client supplies its base URL and complete managed runtime. Only
/// authentication explicitly attached through [`RuntimeEndpointBuilder::auth`] belongs to this
/// endpoint.
/// Fields are intentionally private.
pub struct RuntimeEndpoint<R = NoContent> {
    description: EndpointDescription,
    path_segments: Box<[String]>,
    query: Box<[(Arc<str>, String)]>,
    headers: HeaderMap,
    body: PreparedRequestBody,
    auth: Arc<AuthPlan>,
    rate_limit_plan: Arc<RateLimitPlan>,
    response: PhantomData<R>,
}

impl RuntimeEndpoint<NoContent> {
    /// Starts a no-content runtime endpoint with the supplied stable identity and HTTP method.
    #[must_use]
    pub fn builder(endpoint_id: EndpointId, method: Method) -> RuntimeEndpointBuilder<NoContent> {
        RuntimeEndpointBuilder {
            state: ErrorLatch::from_value(RuntimeEndpointParts {
                endpoint_id,
                method,
                path_segments: Vec::new(),
                query: Vec::new(),
                headers: Vec::new(),
                body: PreparedRequestBody::new::<EmptyBody>(()),
                body_category: RequestBodyCategory::Empty,
                body_specification: None,
                auth: Vec::new(),
                timeout: TimeoutSelection::InheritClient,
                rate_limits: Vec::new(),
                budget: StructuralBudget::new(),
            }),
            response: PhantomData,
        }
    }
}

impl<R> RuntimeEndpointBuilder<R> {
    /// Appends one formatted value as exactly one URL-encoded path segment.
    #[must_use]
    pub fn path_segment<T>(self, value: T) -> Self
    where
        T: Display,
    {
        self.update(move |mut parts| {
            let mut budget = reserve_runtime_structure(parts.budget, 1, 0)
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let maximum =
                runtime_format_limit(budget).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let value = format_bounded(&value, maximum)
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            budget = reserve_runtime_structure(budget, 0, value.len())
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            parts.budget = budget;
            parts.path_segments.push(value);
            Ok(parts)
        })
    }

    /// Appends one ordered public query pair.
    ///
    /// The name is borrowed through [`AsRef<str>`] so its byte length can be rejected before
    /// Concord copies it.
    #[must_use]
    pub fn query<N, T>(self, name: N, value: T) -> Self
    where
        N: AsRef<str>,
        T: Display,
    {
        self.update(move |mut parts| {
            let name = name.as_ref();
            let mut budget = reserve_runtime_structure(parts.budget, 1, name.len())
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let maximum =
                runtime_format_limit(budget).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let value = format_bounded(&value, maximum)
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            budget = reserve_runtime_structure(budget, 0, value.len())
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let name: Arc<str> = Arc::from(name);
            parts.budget = budget;
            parts.query.push((name, value));
            Ok(parts)
        })
    }

    /// Appends one ordered query pair when `value` is present.
    #[must_use]
    pub fn query_optional<N, T>(self, name: N, value: Option<T>) -> Self
    where
        N: AsRef<str>,
        T: Display,
    {
        match value {
            Some(value) => self.query(name, value),
            None => self,
        }
    }

    /// Appends repeated ordered query pairs while preserving iterator order.
    ///
    /// Concord consumes only the values that fit plus one withheld first-excess value. An empty
    /// iterator retains no copy of `name`.
    #[must_use]
    pub fn query_values<N, T, I>(self, name: N, values: I) -> Self
    where
        N: AsRef<str>,
        T: Display,
        I: IntoIterator<Item = T>,
    {
        self.update(move |mut parts| {
            let name = name.as_ref();
            if name.len() > MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES {
                return Err(ConfigurationErrorKind::InvalidValue);
            }
            let mut values = values.into_iter();
            let Some(first) = values.next() else {
                return Ok(parts);
            };

            let mut budget = reserve_runtime_structure(parts.budget, 1, name.len())
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let maximum =
                runtime_format_limit(budget).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let first = format_bounded(&first, maximum)
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            budget = reserve_runtime_structure(budget, 0, first.len())
                .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
            let name: Arc<str> = Arc::from(name);
            parts.budget = budget;
            parts.query.push((Arc::clone(&name), first));

            for value in values {
                append_runtime_query_value(&mut parts, &name, value)?;
            }
            Ok(parts)
        })
    }

    /// Inserts one public header, replacing an earlier case-insensitively equal name.
    #[must_use]
    pub fn header<T>(self, name: HeaderName, value: T) -> Self
    where
        T: Display,
    {
        self.update(move |mut parts| {
            validate_public_header_name(&name)?;
            let existing = parts
                .headers
                .iter()
                .position(|(candidate, _)| candidate == name);
            let (mut budget, maximum) = match existing {
                Some(index) => {
                    let mut formatting_budget = parts.budget;
                    formatting_budget
                        .try_replace_public_bytes(
                            parts.headers[index].1.as_bytes().len(),
                            0,
                            MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                        )
                        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
                    let maximum = runtime_format_limit(formatting_budget)
                        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
                    (parts.budget, maximum)
                }
                None => {
                    let budget = reserve_runtime_structure(parts.budget, 1, name.as_str().len())
                        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
                    let maximum = runtime_format_limit(budget)
                        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
                    (budget, maximum)
                }
            };
            let formatted = format_bounded(&value, maximum)
                .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
            let value = validate_public_header(&name, &formatted)?;

            match existing {
                Some(index) => {
                    budget
                        .try_replace_public_bytes(
                            parts.headers[index].1.as_bytes().len(),
                            value.as_bytes().len(),
                            MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
                        )
                        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
                    parts.headers[index].1 = value;
                }
                None => {
                    budget = reserve_runtime_structure(budget, 0, value.as_bytes().len())
                        .map_err(|_| ConfigurationErrorKind::InvalidHeader)?;
                    parts.headers.push((name, value));
                }
            }
            parts.budget = budget;
            Ok(parts)
        })
    }

    /// Replaces the selected request body with one existing explicit body value.
    #[must_use]
    pub fn body(self, body: RequestBody) -> Self {
        self.body_spec::<RequestBody>(body)
    }

    /// Replaces the selected request body with an arbitrary request specification.
    ///
    /// Concord stores the application input without invoking body metadata,
    /// encoding, factories, or streams. The input's own construction and
    /// retained state are an application extension trust boundary.
    #[must_use]
    pub fn body_spec<B>(self, input: B::Input) -> Self
    where
        B: RequestBodySpec,
    {
        self.update(move |mut parts| {
            parts.body = PreparedRequestBody::new::<B>(input);
            parts.body_category = RequestBodyCategory::Specification;
            parts.body_specification = Some(std::any::type_name::<B>());
            Ok(parts)
        })
    }

    /// Replaces the selected request body with one lazily encoded codec input.
    #[must_use]
    pub fn body_codec<C>(self, value: C::Input) -> RuntimeEndpointBuilder<R>
    where
        C: EncodeBody,
    {
        self.body_spec::<C>(value)
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
            if count > MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS {
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

    /// Replaces the Concord operation deadline with one explicit duration.
    #[must_use]
    pub fn timeout(self, duration: Duration) -> Self {
        self.update(move |mut parts| {
            parts.timeout = TimeoutSelection::Explicit(duration);
            Ok(parts)
        })
    }

    /// Replaces the Concord operation deadline selection with explicit disablement.
    #[must_use]
    pub fn disable_timeout(self) -> Self {
        self.update(|mut parts| {
            parts.timeout = TimeoutSelection::Disabled;
            Ok(parts)
        })
    }

    /// Restores timeout inheritance from the immutable built client.
    #[must_use]
    pub fn inherit_timeout(self) -> Self {
        self.update(|mut parts| {
            parts.timeout = TimeoutSelection::InheritClient;
            Ok(parts)
        })
    }

    /// Appends one structurally validated rate-limit rule.
    #[must_use]
    pub fn rate_limit(self, rule: RateLimitRule) -> Self {
        self.update(move |mut parts| {
            RateLimitPlan::validate_rule_append(&parts.rate_limits, &rule).map_err(|error| {
                drop(error);
                ConfigurationErrorKind::InvalidRateLimit
            })?;
            let retained_bytes = rule
                .retained_structure_bytes()
                .ok_or(ConfigurationErrorKind::InvalidRateLimit)?;
            let budget = reserve_runtime_structure(parts.budget, 1, retained_bytes)
                .map_err(|_| ConfigurationErrorKind::InvalidRateLimit)?;
            parts.budget = budget;
            parts.rate_limits.push(rule);
            Ok(parts)
        })
    }

    /// Selects an arbitrary successful response specification.
    #[must_use]
    pub fn response<S>(self) -> RuntimeEndpointBuilder<S>
    where
        S: ResponseBodySpec,
    {
        self.with_response()
    }

    /// Selects a complete bounded successful bytes response.
    #[must_use]
    pub fn response_bytes(self) -> RuntimeEndpointBuilder<bytes::Bytes> {
        self.with_response()
    }

    /// Selects a successful response whose body is intentionally ignored.
    #[must_use]
    pub fn response_no_content(self) -> RuntimeEndpointBuilder<NoContent> {
        self.with_response()
    }

    /// Selects a lazy bounded successful response stream with media marker `M`.
    #[must_use]
    pub fn response_stream<M>(self) -> RuntimeEndpointBuilder<Stream<M>>
    where
        M: MediaType,
    {
        self.with_response()
    }

    /// Validates structural configuration and returns one immutable endpoint value.
    pub fn build(self) -> Result<RuntimeEndpoint<R>, ConfigurationError>
    where
        R: ResponseBodySpec,
    {
        let RuntimeEndpointParts {
            endpoint_id,
            method,
            path_segments,
            query,
            headers,
            body,
            body_category,
            body_specification,
            auth,
            timeout,
            rate_limits,
            budget: _,
        } = self.state.into_result().map_err(ConfigurationError::new)?;

        let mut validated_headers = HeaderMap::new();
        for (name, value) in headers {
            validated_headers.insert(name, value);
        }

        let mut validation_url = Url::parse("https://runtime.invalid/")
            .expect("the fixed runtime validation origin is valid");
        validation_url.query_pairs_mut().extend_pairs(
            query
                .iter()
                .map(|(name, value)| (name.as_ref(), value.as_str())),
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

        let response_acquisition = <R::Mode as ResponseMode>::ACQUISITION;
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
        );

        Ok(RuntimeEndpoint {
            description,
            path_segments: path_segments.into_boxed_slice(),
            query: query.into_boxed_slice(),
            headers: validated_headers,
            body,
            auth: Arc::new(auth_plan),
            rate_limit_plan: Arc::new(RateLimitPlan::from_validated_rules(rate_limits)),
            response: PhantomData,
        })
    }

    fn with_response<Next>(self) -> RuntimeEndpointBuilder<Next> {
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

impl<R> RuntimeEndpoint<R>
where
    R: ResponseBodySpec,
{
    /// Returns the safe bounded runtime endpoint description.
    #[must_use]
    pub const fn description(&self) -> &EndpointDescription {
        &self.description
    }
}

impl<R> EndpointSpec for RuntimeEndpoint<R>
where
    R: ResponseBodySpec,
{
    type Body = PreparedBodySpec;
    type Response = R;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::from_description(&self.description)
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        for segment in &self.path_segments {
            builder.push_prepared_segment(segment)?;
        }
        for (name, value) in &self.query {
            builder.push_query_value(name.as_ref(), value)?;
        }
        for (name, value) in &self.headers {
            builder.insert_prepared_header(name.clone(), value.clone());
        }
        builder.use_prepared_auth_plan(Arc::clone(&self.auth));
        builder.use_prepared_rate_limit_plan(Arc::clone(&self.rate_limit_plan));
        Ok(builder.finish())
    }

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, crate::body::BodyError> {
        PreparedBodySpec::metadata(&self.body, context)
    }

    fn into_body(self) -> <Self::Body as RequestBodySpec>::Input {
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
    value: T,
) -> Result<(), ConfigurationErrorKind>
where
    T: Display,
{
    let mut budget = reserve_runtime_structure(parts.budget, 1, name.len())
        .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
    let maximum = runtime_format_limit(budget).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
    let value =
        format_bounded(&value, maximum).map_err(|_| ConfigurationErrorKind::InvalidValue)?;
    budget = reserve_runtime_structure(budget, 0, value.len())
        .map_err(|_| ConfigurationErrorKind::InvalidValue)?;
    parts.budget = budget;
    parts.query.push((Arc::clone(name), value));
    Ok(())
}

fn runtime_route_shape(path_segment_count: usize) -> Arc<str> {
    if path_segment_count == 0 {
        return Arc::from("<client-base>");
    }
    let mut shape = String::from("/<client-base>");
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
    use crate::codec::{OctetStream, Text};
    use crate::hooks::DebugLevel;
    use crate::rate_limit::{
        MAX_RATE_LIMIT_RULES_PER_PLAN, RateLimitId, RateLimitKey, RateLimitWindow,
    };
    use crate::request::RequestOptions;

    const FORMAT_SENTINEL: &str = "runtime-format-first-excess-sentinel-3f4a";
    const RATE_ID_SENTINEL: &str = "runtime-rate-id-sentinel-527d";
    const RATE_KEY_SENTINEL: &str = "runtime-rate-key-sentinel-28b1";

    #[derive(Clone, Copy)]
    struct EmptyDisplay;

    impl Display for EmptyDisplay {
        fn fmt(&self, _formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Ok(())
        }
    }

    struct TrackedEmptyDisplay {
        formatted: Arc<AtomicUsize>,
    }

    impl Display for TrackedEmptyDisplay {
        fn fmt(&self, _formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.formatted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct PanicBeyondFirstExcess {
        next_calls: Arc<AtomicUsize>,
        formatted: Arc<AtomicUsize>,
    }

    impl Iterator for PanicBeyondFirstExcess {
        type Item = TrackedEmptyDisplay;

        fn next(&mut self) -> Option<Self::Item> {
            let call = self.next_calls.fetch_add(1, Ordering::SeqCst);
            assert!(call < 3, "iterator consumed beyond the first excess");
            Some(TrackedEmptyDisplay {
                formatted: Arc::clone(&self.formatted),
            })
        }
    }

    struct RetentionProbe {
        _retained: Arc<()>,
        formatted: Arc<AtomicUsize>,
        text: &'static str,
    }

    impl Display for RetentionProbe {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.formatted.fetch_add(1, Ordering::SeqCst);
            formatter.write_str(self.text)
        }
    }

    struct PanicDisplay;

    impl Display for PanicDisplay {
        fn fmt(&self, _formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("latched builder formatted a later value")
        }
    }

    struct PanicIntoIterator {
        into_iter_calls: Arc<AtomicUsize>,
    }

    impl IntoIterator for PanicIntoIterator {
        type Item = &'static str;
        type IntoIter = std::iter::Empty<&'static str>;

        fn into_iter(self) -> Self::IntoIter {
            self.into_iter_calls.fetch_add(1, Ordering::SeqCst);
            panic!("latched builder consumed a later iterator")
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

    impl Display for SentinelDisplay {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(&self.prefix)?;
            formatter.write_str(FORMAT_SENTINEL)
        }
    }

    struct InertBodyInput {
        calls: Arc<AtomicUsize>,
    }

    struct InertBodySpec;

    impl RequestBodySpec for InertBodySpec {
        type Input = InertBodyInput;

        fn metadata(
            input: &Self::Input,
            _context: RequestBodyContext<'_>,
        ) -> Result<RequestBodyMetadata, crate::body::BodyError> {
            input.calls.fetch_add(1, Ordering::SeqCst);
            panic!("runtime endpoint build invoked body metadata")
        }

        fn prepare(
            input: Self::Input,
            _context: RequestBodyContext<'_>,
        ) -> Result<RequestBody, crate::body::BodyError> {
            input.calls.fetch_add(1, Ordering::SeqCst);
            panic!("runtime endpoint build invoked body preparation")
        }
    }

    fn head<R>(endpoint: &RuntimeEndpoint<R>, base: &str) -> PreparedHead
    where
        R: ResponseBodySpec,
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

    fn rule(id: &str, value: &str) -> RateLimitRule {
        RateLimitRule::new(
            RateLimitId::new(id).expect("rule ID"),
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
            AccessToken::new("runtime-auth-secret-sentinel"),
            Duration::from_secs(1),
        ))
    }

    #[test]
    fn generic_response_specifications_replace_the_previous_selection() {
        fn buffered(_: RuntimeEndpointBuilder<Text>) {}
        fn bytes(_: RuntimeEndpointBuilder<Bytes>) {}
        fn no_content(_: RuntimeEndpointBuilder<NoContent>) {}
        fn stream(_: RuntimeEndpointBuilder<Stream<OctetStream>>) {}

        buffered(
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.buffered"), Method::GET)
                .response_bytes()
                .response::<Text>(),
        );
        bytes(
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.bytes"), Method::GET)
                .response::<Text>()
                .response_bytes(),
        );
        no_content(
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.none"), Method::GET)
                .response_stream::<OctetStream>()
                .response_no_content(),
        );
        stream(
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.stream"), Method::GET)
                .response_no_content()
                .response_stream::<OctetStream>(),
        );
    }

    #[test]
    fn composition_replaces_body_header_and_deadline_but_appends_query_and_rate_rules() {
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.compose"), Method::POST)
                .query("first", 1)
                .query_optional("omitted", Option::<u8>::None)
                .query_values("repeat", [2, 3])
                .header(HeaderName::from_static("x-choice"), "old")
                .header(
                    HeaderName::from_bytes(b"X-Choice").expect("mixed-case header"),
                    "new",
                )
                .body_codec::<Text>("discarded".to_owned())
                .body(RequestBody::bytes(Bytes::from_static(b"selected")))
                .timeout(Duration::from_secs(5))
                .disable_timeout()
                .rate_limit(rule("first", "a"))
                .rate_limit(rule("second", "b"))
                .build()
                .expect("valid runtime endpoint");

        assert_eq!(endpoint.description.timeout(), TimeoutSelection::Disabled);
        assert_eq!(endpoint.rate_limit_plan.rules().len(), 2);
        assert_eq!(endpoint.rate_limit_plan.rules()[0].id().as_str(), "first");
        assert_eq!(endpoint.rate_limit_plan.rules()[1].id().as_str(), "second");
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
                .disable_timeout()
                .timeout(Duration::from_secs(7))
                .build()
                .expect("last timeout selection wins");
        assert_eq!(
            explicit.description.timeout(),
            TimeoutSelection::Explicit(Duration::from_secs(7))
        );
    }

    #[test]
    fn runtime_path_values_are_one_ordered_encoded_segment_each() {
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.route"), Method::GET)
                .path_segment("ordinary")
                .path_segment("")
                .path_segment("slash/value")
                .path_segment("..")
                .path_segment("query?value")
                .path_segment("fragment#value")
                .path_segment("percent%2Fvalue")
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
                "",
                "slash%2Fvalue",
                "%252E%252E",
                "query%3Fvalue",
                "fragment%23value",
                "percent%252Fvalue",
            ]
        );
        assert_eq!(head.logical_url().query(), None);
        assert_eq!(head.logical_url().fragment(), None);
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
    fn runtime_endpoint_query_values_stops_at_first_excess() {
        let mut builder =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.query"), Method::GET);
        for _ in 0..(MAX_RUNTIME_ENDPOINT_COMPONENTS - 2) {
            builder = builder.query("", EmptyDisplay);
        }

        let next_calls = Arc::new(AtomicUsize::new(0));
        let formatted = Arc::new(AtomicUsize::new(0));
        let error = builder
            .query_values(
                "",
                PanicBeyondFirstExcess {
                    next_calls: Arc::clone(&next_calls),
                    formatted: Arc::clone(&formatted),
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
                .query_values("", std::iter::repeat(EmptyDisplay))
                .build()
                .err()
                .expect("an infinite query iterator must reach the finite component limit");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidValue);
    }

    #[test]
    fn runtime_endpoint_first_excess_component_is_not_retained() {
        let mut boundary =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.components"), Method::GET)
                .query_optional(PanicName, Option::<PanicDisplay>::None);
        for _ in 0..MAX_RUNTIME_ENDPOINT_COMPONENTS {
            boundary = boundary.query("", EmptyDisplay);
        }
        boundary.build().expect("component boundary");

        let retained = Arc::new(());
        let formatted = Arc::new(AtomicUsize::new(0));
        let mut excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.components"), Method::GET);
        for _ in 0..MAX_RUNTIME_ENDPOINT_COMPONENTS {
            excessive = excessive.query("", EmptyDisplay);
        }
        let excessive = excessive.path_segment(RetentionProbe {
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
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.bytes"), Method::GET)
            .path_segment("x".repeat(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES))
            .build()
            .expect("public-byte boundary");

        let retained = Arc::new(());
        let formatted = Arc::new(AtomicUsize::new(0));
        let excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.bytes"), Method::GET)
                .path_segment("x".repeat(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES))
                .path_segment(RetentionProbe {
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
                .response_bytes()
                .query_values(
                    PanicName,
                    PanicIntoIterator {
                        into_iter_calls: Arc::clone(&into_iter_calls),
                    },
                )
                .path_segment(PanicDisplay)
                .build()
                .err()
                .expect("the original structural failure remains latched");

        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidHeader);
        assert_eq!(into_iter_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn runtime_endpoint_header_replacement_updates_budget_exactly() {
        let name = HeaderName::from_static("x-budget");
        let initial = "a".repeat(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - name.as_str().len());
        let replacement = "b";
        let tail =
            "c".repeat(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - name.as_str().len() - replacement.len());

        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.header"), Method::GET)
                .header(name.clone(), initial)
                .header(name.clone(), replacement)
                .path_segment(tail)
                .build()
                .expect("replacement frees exactly the previous retained value bytes");

        assert_eq!(
            endpoint
                .headers
                .get(&name)
                .and_then(|value| value.to_str().ok()),
            Some(replacement)
        );

        let retained = Arc::new(());
        let formatted = Arc::new(AtomicUsize::new(0));
        let excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.header"), Method::GET)
                .header(name.clone(), "a")
                .path_segment(
                    "x".repeat(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - name.as_str().len() - 1),
                )
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
        for index in 0..MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS {
            boundary = boundary.auth(
                Auth::query(format!("runtime-auth-{index}"), credential.clone())
                    .expect("valid authentication placement"),
            );
        }
        boundary.build().expect("authentication placement boundary");

        let mut excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.auth"), Method::GET);
        for index in 0..=MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS {
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
            AccessToken::new(SECRET_SENTINEL),
            Duration::from_secs(1),
        ));
        let error = RuntimeEndpoint::builder(EndpointId::from_static("runtime.auth"), Method::GET)
            .path_segment("x".repeat(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - 1))
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
        for index in 0..MAX_RATE_LIMIT_RULES_PER_PLAN {
            boundary = boundary.rate_limit(rule(&format!("rule-{index}"), "tenant"));
        }
        boundary.build().expect("rate-rule boundary");

        let mut excessive =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.rate"), Method::GET);
        for index in 0..MAX_RATE_LIMIT_RULES_PER_PLAN {
            excessive = excessive.rate_limit(rule(&format!("rule-{index}"), "tenant"));
        }
        excessive = excessive.rate_limit(rule(RATE_ID_SENTINEL, RATE_KEY_SENTINEL));
        let error = excessive
            .build()
            .err()
            .expect("first excess rate-limit rule");
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
        .expect("duplicate runtime rate-limit rule ID");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidRateLimit);
        assert!(!error.to_string().contains(RATE_ID_SENTINEL));
        assert!(!error.to_string().contains(RATE_KEY_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_ID_SENTINEL));
        assert!(!format!("{error:?}").contains(RATE_KEY_SENTINEL));
    }

    #[test]
    fn runtime_endpoint_rate_key_bytes_are_bounded_and_redacted() {
        let error = RuntimeEndpoint::builder(EndpointId::from_static("runtime.rate"), Method::GET)
            .path_segment("x".repeat(MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES - 1))
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
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.format"), Method::GET)
                .path_segment(SentinelDisplay {
                    prefix: "x".repeat(MAX_FORMATTED_PUBLIC_VALUE_BYTES),
                    _retained: Arc::clone(&retained),
                });

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
                .body_spec::<InertBodySpec>(InertBodyInput {
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
                .path_segment(SENTINEL)
                .query("q", SENTINEL)
                .header(HeaderName::from_static("x-public"), SENTINEL)
                .body_codec::<Text>(SENTINEL.to_owned())
                .response_bytes()
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
