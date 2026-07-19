use std::fmt::Display;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use http::{HeaderMap, HeaderName, Method};
use url::Url;

use super::{
    BufferedResponse, BytesResponse, EndpointSpec, HeadBuilder, NoContentResponse, PreparedHead,
    RequestMeta, ResponseHandler, StreamResponseHandler, validate_public_header,
};
use crate::auth::{Auth, AuthPlan};
use crate::body::{BodyMetadata, BodySpec, CodecBody, EmptyBody, PreparedRequestBody, RequestBody};
use crate::codec::{DecodeBody, EncodeBody, MediaType};
use crate::error::{ApiError, ConfigurationError, ConfigurationErrorKind};
use crate::rate_limit::{RateLimitPlan, RateLimitRule};

/// Selects a complete bounded successful response decoded by `C`.
/// Its tuple field is intentionally private.
pub struct PreparedBuffered<C>(PhantomData<C>);

/// Selects a complete bounded successful response returned as bytes.
pub struct PreparedBytes;

/// Selects a successful response whose body is intentionally ignored.
pub struct PreparedNoContent;

/// Selects a lazy bounded successful response stream with media marker `M`.
/// Its tuple field is intentionally private.
pub struct PreparedStream<M>(PhantomData<M>);

mod response_sealed {
    pub trait Sealed {}
}

/// Core-private mapping from a stable prepared response marker to the ordinary response handler.
#[doc(hidden)]
pub trait PreparedResponseMarker: response_sealed::Sealed + Send + Sync + 'static {
    type Handler: ResponseHandler;
}

impl<C> response_sealed::Sealed for PreparedBuffered<C> where C: DecodeBody {}

impl<C> PreparedResponseMarker for PreparedBuffered<C>
where
    C: DecodeBody,
{
    type Handler = BufferedResponse<C>;
}

impl response_sealed::Sealed for PreparedBytes {}

impl PreparedResponseMarker for PreparedBytes {
    type Handler = BytesResponse;
}

impl response_sealed::Sealed for PreparedNoContent {}

impl PreparedResponseMarker for PreparedNoContent {
    type Handler = NoContentResponse;
}

impl<M> response_sealed::Sealed for PreparedStream<M> where M: MediaType {}

impl<M> PreparedResponseMarker for PreparedStream<M>
where
    M: MediaType,
{
    type Handler = StreamResponseHandler<M>;
}

enum DeadlineSelection {
    Inherit,
    Explicit(Duration),
    Disabled,
}

/// Builds one owned hand-written endpoint for execution by a generated client.
///
/// Construction is side-effect free with respect to Concord execution: it does not allocate an
/// operation identity, invoke codecs or factories, acquire credentials or limiter capacity, call
/// hooks, construct a native request, or perform HTTP.
/// Fields are intentionally private.
pub struct PreparedEndpointBuilder<R = PreparedNoContent> {
    endpoint: &'static str,
    method: Method,
    path_segments: Vec<String>,
    query: Vec<(String, String)>,
    headers: Vec<(HeaderName, String)>,
    body: PreparedRequestBody,
    auth: Vec<Auth>,
    deadline: DeadlineSelection,
    rate_limits: Vec<RateLimitRule>,
    response: PhantomData<R>,
}

/// One immutable hand-written endpoint accepted by every generated client's sealed request path.
///
/// The selected generated client supplies its base URL and complete managed runtime. Only
/// authentication explicitly attached through [`PreparedEndpointBuilder::auth`] belongs to this
/// endpoint.
/// Fields are intentionally private.
pub struct PreparedEndpoint<R = PreparedNoContent> {
    endpoint: &'static str,
    method: Method,
    path_segments: Box<[String]>,
    query: Box<[(String, String)]>,
    headers: HeaderMap,
    body: PreparedRequestBody,
    auth: Arc<AuthPlan>,
    inherited_timeout: Option<Duration>,
    rate_limit_plan: Arc<RateLimitPlan>,
    response: PhantomData<R>,
}

impl PreparedEndpoint<PreparedNoContent> {
    /// Starts a no-content prepared endpoint with the supplied stable identity and HTTP method.
    #[must_use]
    pub fn builder(
        endpoint: &'static str,
        method: Method,
    ) -> PreparedEndpointBuilder<PreparedNoContent> {
        PreparedEndpointBuilder {
            endpoint,
            method,
            path_segments: Vec::new(),
            query: Vec::new(),
            headers: Vec::new(),
            body: PreparedRequestBody::new(EmptyBody::new()),
            auth: Vec::new(),
            deadline: DeadlineSelection::Inherit,
            rate_limits: Vec::new(),
            response: PhantomData,
        }
    }
}

impl<R> PreparedEndpointBuilder<R> {
    /// Appends one formatted value as exactly one URL-encoded path segment.
    #[must_use]
    pub fn path_segment<T>(mut self, value: T) -> Self
    where
        T: Display,
    {
        self.path_segments.push(value.to_string());
        self
    }

    /// Appends one ordered public query pair.
    #[must_use]
    pub fn query<T>(mut self, name: impl Into<String>, value: T) -> Self
    where
        T: Display,
    {
        self.query.push((name.into(), value.to_string()));
        self
    }

    /// Appends one ordered query pair when `value` is present.
    #[must_use]
    pub fn query_optional<T>(self, name: impl Into<String>, value: Option<T>) -> Self
    where
        T: Display,
    {
        match value {
            Some(value) => self.query(name, value),
            None => self,
        }
    }

    /// Appends repeated ordered query pairs while preserving iterator order.
    #[must_use]
    pub fn query_values<T, I>(mut self, name: impl Into<String>, values: I) -> Self
    where
        T: Display,
        I: IntoIterator<Item = T>,
    {
        let name = name.into();
        self.query.extend(
            values
                .into_iter()
                .map(|value| (name.clone(), value.to_string())),
        );
        self
    }

    /// Inserts one public header, replacing an earlier case-insensitively equal name.
    #[must_use]
    pub fn header<T>(mut self, name: HeaderName, value: T) -> Self
    where
        T: Display,
    {
        let value = value.to_string();
        if let Some((_, previous)) = self
            .headers
            .iter_mut()
            .find(|(candidate, _)| candidate == name)
        {
            *previous = value;
        } else {
            self.headers.push((name, value));
        }
        self
    }

    /// Replaces the selected request body with one existing advanced body value.
    #[must_use]
    pub fn body(mut self, body: RequestBody) -> Self {
        self.body = PreparedRequestBody::new(body);
        self
    }

    /// Replaces the selected request body with one lazily encoded codec input.
    #[must_use]
    pub fn body_codec<C>(mut self, value: C::Input) -> PreparedEndpointBuilder<R>
    where
        C: EncodeBody,
    {
        self.body = PreparedRequestBody::new(CodecBody::<C>::new(value));
        self
    }

    /// Appends one typed authentication placement.
    #[must_use]
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth.push(auth);
        self
    }

    /// Replaces the Concord operation deadline with one explicit duration.
    #[must_use]
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.deadline = DeadlineSelection::Explicit(duration);
        self
    }

    /// Replaces the Concord operation deadline selection with explicit disablement.
    #[must_use]
    pub fn disable_timeout(mut self) -> Self {
        self.deadline = DeadlineSelection::Disabled;
        self
    }

    /// Appends one structurally validated rate-limit rule.
    #[must_use]
    pub fn rate_limit(mut self, rule: RateLimitRule) -> Self {
        self.rate_limits.push(rule);
        self
    }

    /// Selects a complete bounded successful response decoded by `C`.
    #[must_use]
    pub fn response<C>(self) -> PreparedEndpointBuilder<PreparedBuffered<C>>
    where
        C: DecodeBody,
    {
        self.with_response()
    }

    /// Selects a complete bounded successful bytes response.
    #[must_use]
    pub fn response_bytes(self) -> PreparedEndpointBuilder<PreparedBytes> {
        self.with_response()
    }

    /// Selects a successful response whose body is intentionally ignored.
    #[must_use]
    pub fn response_no_content(self) -> PreparedEndpointBuilder<PreparedNoContent> {
        self.with_response()
    }

    /// Selects a lazy bounded successful response stream with media marker `M`.
    #[must_use]
    pub fn response_stream<M>(self) -> PreparedEndpointBuilder<PreparedStream<M>>
    where
        M: MediaType,
    {
        self.with_response()
    }

    /// Validates structural configuration and returns one immutable endpoint value.
    pub fn build(self) -> Result<PreparedEndpoint<R>, ConfigurationError> {
        let Self {
            endpoint,
            method,
            path_segments,
            query,
            headers,
            body,
            auth,
            deadline,
            rate_limits,
            response: _,
        } = self;

        if endpoint.is_empty() {
            return Err(ConfigurationError::new(
                ConfigurationErrorKind::InvalidValue,
            ));
        }

        let mut validated_headers = HeaderMap::new();
        for (name, value) in headers {
            let value = validate_public_header(&name, &value).map_err(ConfigurationError::new)?;
            validated_headers.insert(name, value);
        }

        let mut validation_url = Url::parse("https://prepared.invalid/")
            .expect("the fixed prepared validation origin is valid");
        validation_url
            .query_pairs_mut()
            .extend_pairs(query.iter().map(|(name, value)| (name, value)));
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

        let inherited_timeout = match deadline {
            DeadlineSelection::Inherit | DeadlineSelection::Disabled => None,
            DeadlineSelection::Explicit(duration) => Some(duration),
        };

        Ok(PreparedEndpoint {
            endpoint,
            method,
            path_segments: path_segments.into_boxed_slice(),
            query: query.into_boxed_slice(),
            headers: validated_headers,
            body,
            auth: Arc::new(auth_plan),
            inherited_timeout,
            rate_limit_plan: Arc::new(RateLimitPlan::from_rules(rate_limits)),
            response: PhantomData,
        })
    }

    fn with_response<Next>(self) -> PreparedEndpointBuilder<Next> {
        PreparedEndpointBuilder {
            endpoint: self.endpoint,
            method: self.method,
            path_segments: self.path_segments,
            query: self.query,
            headers: self.headers,
            body: self.body,
            auth: self.auth,
            deadline: self.deadline,
            rate_limits: self.rate_limits,
            response: PhantomData,
        }
    }
}

impl<R> EndpointSpec for PreparedEndpoint<R>
where
    R: PreparedResponseMarker,
{
    type Body = PreparedRequestBody;
    type Response = R::Handler;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(self.endpoint, self.method.clone(), self.inherited_timeout)
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        for segment in &self.path_segments {
            builder.push_prepared_segment(segment)?;
        }
        for (name, value) in &self.query {
            builder.push_query_value(name, value);
        }
        for (name, value) in &self.headers {
            builder.insert_prepared_header(name.clone(), value.clone());
        }
        builder.use_prepared_auth_plan(Arc::clone(&self.auth));
        builder.use_prepared_rate_limit_plan(Arc::clone(&self.rate_limit_plan));
        let head = builder.finish();
        let body_metadata = self.body.metadata(self.endpoint, &self.method)?;
        Ok((head, body_metadata))
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use bytes::Bytes;
    use http::header::AUTHORIZATION;

    use super::*;
    use crate::codec::{OctetStream, Text};
    use crate::hooks::DebugLevel;
    use crate::rate_limit::{RateLimitId, RateLimitKey, RateLimitWindow};
    use crate::request::RequestOptions;

    fn head<R>(endpoint: &PreparedEndpoint<R>, base: &str) -> PreparedHead
    where
        R: PreparedResponseMarker,
    {
        let builder = HeadBuilder::new(
            &Url::parse(base).expect("base URL"),
            endpoint.request_meta(),
            &RequestOptions::new(),
            DebugLevel::None,
        );
        endpoint
            .prepare_head_and_body_metadata(builder)
            .expect("prepared head")
            .0
    }

    fn rule(id: &str, value: &str) -> RateLimitRule {
        RateLimitRule::new(
            RateLimitId::new(id).expect("rule ID"),
            RateLimitKey::from_parts([crate::rate_limit::RateLimitKeyPart::new("tenant", value)])
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

    #[test]
    fn exact_response_markers_replace_the_previous_selection() {
        fn buffered(_: PreparedEndpointBuilder<PreparedBuffered<Text>>) {}
        fn bytes(_: PreparedEndpointBuilder<PreparedBytes>) {}
        fn no_content(_: PreparedEndpointBuilder<PreparedNoContent>) {}
        fn stream(_: PreparedEndpointBuilder<PreparedStream<OctetStream>>) {}

        buffered(
            PreparedEndpoint::builder("Prepared::buffered", Method::GET)
                .response_bytes()
                .response::<Text>(),
        );
        bytes(
            PreparedEndpoint::builder("Prepared::bytes", Method::GET)
                .response::<Text>()
                .response_bytes(),
        );
        no_content(
            PreparedEndpoint::builder("Prepared::none", Method::GET)
                .response_stream::<OctetStream>()
                .response_no_content(),
        );
        stream(
            PreparedEndpoint::builder("Prepared::stream", Method::GET)
                .response_no_content()
                .response_stream::<OctetStream>(),
        );
    }

    #[test]
    fn composition_replaces_body_header_and_deadline_but_appends_query_and_rate_rules() {
        let endpoint = PreparedEndpoint::builder("Prepared::compose", Method::POST)
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
            .expect("valid prepared endpoint");

        assert_eq!(endpoint.inherited_timeout, None);
        assert_eq!(endpoint.rate_limit_plan.rules().len(), 2);
        assert_eq!(endpoint.rate_limit_plan.rules()[0].id().as_str(), "first");
        assert_eq!(endpoint.rate_limit_plan.rules()[1].id().as_str(), "second");
        let metadata = endpoint
            .body
            .metadata(endpoint.endpoint, &endpoint.method)
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

        let explicit = PreparedEndpoint::builder("Prepared::timeout", Method::GET)
            .timeout(Duration::from_secs(1))
            .disable_timeout()
            .timeout(Duration::from_secs(7))
            .build()
            .expect("last timeout selection wins");
        assert_eq!(explicit.inherited_timeout, Some(Duration::from_secs(7)));
    }

    #[test]
    fn prepared_path_values_are_one_ordered_encoded_segment_each() {
        let endpoint = PreparedEndpoint::builder("Prepared::route", Method::GET)
            .path_segment("ordinary")
            .path_segment("")
            .path_segment("slash/value")
            .path_segment("..")
            .path_segment("query?value")
            .path_segment("fragment#value")
            .path_segment("percent%2Fvalue")
            .build()
            .expect("valid prepared route");
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
        let error = PreparedEndpoint::builder("Prepared::invalid", Method::GET)
            .header(AUTHORIZATION, SENTINEL)
            .build()
            .err()
            .expect("reserved header must fail");
        assert_eq!(error.kind(), ConfigurationErrorKind::InvalidHeader);
        assert!(!error.to_string().contains(SENTINEL));
        assert!(!format!("{error:?}").contains(SENTINEL));

        let empty = PreparedEndpoint::builder("", Method::GET)
            .build()
            .err()
            .expect("empty identity must fail");
        assert_eq!(empty.kind(), ConfigurationErrorKind::InvalidValue);
    }
}
