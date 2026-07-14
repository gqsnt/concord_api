use crate::client::{ApiClient, ClientContext};
use crate::codec::{BodyCodec, ContentType, DecodeContext, EncodeContext, ResponseCodec};
use crate::endpoint::{RequestPlan, ResponsePlan};
use crate::error::{ApiClientError, ErrorContext};
#[cfg(feature = "multipart")]
use crate::multipart::MultipartBody;
use crate::stream_body::StreamBody;
use crate::stream_response::StreamResponse;
use crate::transport::{BuiltResponse, DecodedResponse};
use bytes::Bytes;
use http::{HeaderValue, Method};
use http_body::{Body as _, SizeHint};
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::time::Duration;

// A factory deliberately returns a terminal body, never another recipe. This
// makes recursive/factory-bearing output structurally impossible.
type BodyFactory = dyn Fn() -> Result<TerminalBody, crate::body::BodyError> + Send + Sync;

enum PreparedBodyMediaType {
    Fixed(HeaderValue),
    #[allow(dead_code)]
    Dynamic,
}

impl PreparedBodyMediaType {
    fn as_fixed(&self) -> Option<&HeaderValue> {
        match self {
            Self::Fixed(value) => Some(value),
            Self::Dynamic => None,
        }
    }
}

enum ReplayFactory {
    Terminal(std::sync::Arc<BodyFactory>),
}

/// Per-execution, non-factory body result. Each variant maps directly to a
/// native Reqwest request capability and cannot contain another factory.
pub(crate) enum TerminalBody {
    Empty,
    ReusableBytes(Bytes),
    OneShotByteStream(StreamBody),
    OneShotHttpBody(crate::body::DynBody),
    #[cfg(feature = "multipart")]
    MultipartRecipe(MultipartBody),
}

impl TerminalBody {
    fn reqwest_cloneable(&self) -> bool {
        matches!(self, Self::Empty | Self::ReusableBytes(_))
    }

    fn apply_to_reqwest(
        self,
        builder: reqwest::RequestBuilder,
        exact_length: Option<u64>,
        body_errors: &crate::body::RequestBodyErrorSlot,
    ) -> Result<reqwest::RequestBuilder, crate::body::BodyError> {
        match self {
            Self::Empty => {
                validate_reusable_exact_length(0, exact_length)?;
                Ok(builder)
            }
            Self::ReusableBytes(bytes) => {
                validate_reusable_exact_length(bytes.len() as u64, exact_length)?;
                Ok(builder.body(bytes))
            }
            Self::OneShotByteStream(stream) => {
                let body = reqwest::Body::wrap_stream(stream.into_byte_stream());
                Ok(builder.body(observe_native_request_body(
                    body,
                    exact_length,
                    body_errors.clone(),
                )))
            }
            Self::OneShotHttpBody(body) => {
                let body = reqwest::Body::wrap(body);
                Ok(builder.body(observe_native_request_body(
                    body,
                    exact_length,
                    body_errors.clone(),
                )))
            }
            #[cfg(feature = "multipart")]
            Self::MultipartRecipe(recipe) => recipe
                .into_form()
                .map(|form| builder.multipart(form))
                .map_err(|_| crate::body::BodyError::invalid_configuration()),
        }
    }
}

/// The single logical authority for a request body.
///
/// Ordinary byte streams and multipart recipes remain in native-capability
/// form until a Reqwest request is constructed for a visible execution.
enum RequestBodyRecipe {
    Empty,
    ReusableBytes(Bytes),
    OneShotByteStream(Option<StreamBody>),
    OneShotHttpBody(Option<crate::body::DynBody>),
    RequestBodyFactory(ReplayFactory),
    #[cfg(feature = "multipart")]
    MultipartRecipe(Option<MultipartBody>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BodyProductionErrorKind {
    AlreadyConsumed,
    FactoryFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BodyProductionError {
    kind: BodyProductionErrorKind,
    body_error: Option<crate::body::BodyError>,
}

impl BodyProductionError {
    pub(crate) fn kind(&self) -> BodyProductionErrorKind {
        self.kind
    }

    #[cfg(test)]
    pub(crate) fn body_error_kind(&self) -> Option<crate::body::BodyErrorKind> {
        self.body_error.map(|error| error.kind())
    }

    pub(crate) fn body_error(&self) -> Option<crate::body::BodyError> {
        self.body_error
    }

    fn already_consumed() -> Self {
        Self {
            kind: BodyProductionErrorKind::AlreadyConsumed,
            body_error: None,
        }
    }

    fn factory_failure(error: crate::body::BodyError) -> Self {
        Self {
            kind: BodyProductionErrorKind::FactoryFailure,
            body_error: Some(error),
        }
    }
}

impl fmt::Display for BodyProductionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BodyProductionErrorKind::AlreadyConsumed => {
                f.write_str("one-shot request body has already been consumed")
            }
            BodyProductionErrorKind::FactoryFailure => f.write_str("request body factory failed"),
        }
    }
}

impl std::error::Error for BodyProductionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.body_error
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

/// Request-local owner of request body production, metadata, and replayability.
pub struct PreparedBody {
    recipe: RequestBodyRecipe,
    media_type: Option<PreparedBodyMediaType>,
    size_hint: SizeHint,
}

pub(crate) struct ProducedBody {
    terminal: TerminalBody,
    media_type: Option<HeaderValue>,
    exact_length: Option<u64>,
}

/// Opaque request-only wrapper for a one-shot standard [`http_body::Body`].
///
/// This type is deliberately not a response-body abstraction. It preserves
/// frames and safe size metadata while keeping Concord's polling and native
/// Reqwest adaptation private.
pub struct AdvancedRequestBody {
    inner: crate::body::DynBody,
}

impl AdvancedRequestBody {
    /// Wrap any one-shot standard body producing [`bytes::Bytes`]. Producer
    /// diagnostics are converted to Concord's safe [`crate::body::BodyError`]
    /// taxonomy before they can reach hooks or callers.
    pub fn new<B>(body: B) -> Self
    where
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: Send + 'static,
    {
        Self {
            inner: crate::body::DynBody::from_body(body),
        }
    }

    /// Override body size metadata without polling the producer.
    pub fn with_size_hint(mut self, hint: SizeHint) -> Self {
        self.inner = self.inner.with_size_hint(hint);
        self
    }

    /// Apply a request-side byte limit without polling the producer.
    pub fn limited(mut self, limit: u64) -> Self {
        self.inner = self.inner.limited(limit);
        self
    }

    pub(crate) fn from_dyn(inner: crate::body::DynBody) -> Self {
        Self { inner }
    }

    fn into_dyn(self) -> crate::body::DynBody {
        self.inner
    }
}

impl fmt::Debug for AdvancedRequestBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdvancedRequestBody")
            .field("size_hint", &self.inner.size_hint())
            .finish_non_exhaustive()
    }
}

impl From<crate::body::DynBody> for AdvancedRequestBody {
    fn from(inner: crate::body::DynBody) -> Self {
        Self::from_dyn(inner)
    }
}

impl ProducedBody {
    #[cfg(test)]
    pub(crate) fn is_reqwest_cloneable(&self) -> bool {
        self.terminal.reqwest_cloneable()
    }

    #[cfg(test)]
    pub(crate) fn into_dyn_body(self) -> crate::body::DynBody {
        self.try_into_dyn_body()
            .expect("validated terminal body must materialize natively")
    }

    #[cfg(test)]
    pub(crate) fn try_into_dyn_body(self) -> Result<crate::body::DynBody, crate::body::BodyError> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("test Reqwest client");
        let builder = client.request(
            http::Method::POST,
            url::Url::parse("http://example.test/").expect("static URL"),
        );
        let (builder, _, _) = self.apply_to_reqwest(builder)?;
        let mut request = builder.build().expect("native test request");
        Ok(request
            .body_mut()
            .take()
            .map(crate::body::DynBody::from_body)
            .unwrap_or_else(crate::body::DynBody::empty))
    }

    pub(crate) fn apply_to_reqwest(
        self,
        builder: reqwest::RequestBuilder,
    ) -> Result<
        (
            reqwest::RequestBuilder,
            Option<HeaderValue>,
            crate::body::RequestBodyErrorSlot,
        ),
        crate::body::BodyError,
    > {
        let body_errors = crate::body::RequestBodyErrorSlot::default();
        let builder = self
            .terminal
            .apply_to_reqwest(builder, self.exact_length, &body_errors)?;
        Ok((builder, self.media_type, body_errors))
    }
}

impl fmt::Debug for ProducedBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProducedBody")
            .field("reqwest_cloneable", &self.terminal.reqwest_cloneable())
            .field("has_media_type", &self.media_type.is_some())
            .finish()
    }
}

impl PreparedBody {
    pub fn empty() -> Self {
        Self {
            recipe: RequestBodyRecipe::Empty,
            media_type: None,
            size_hint: exact_size_hint(0),
        }
    }

    pub fn reusable_bytes(bytes: Bytes, media_type: Option<HeaderValue>) -> Self {
        let size_hint = exact_size_hint(bytes.len() as u64);
        Self {
            recipe: RequestBodyRecipe::ReusableBytes(bytes),
            media_type: media_type.map(PreparedBodyMediaType::Fixed),
            size_hint,
        }
    }

    /// Prepare a one-shot advanced body. It cannot be reconstructed for an
    /// authentication recovery execution.
    pub fn one_shot(body: impl Into<AdvancedRequestBody>, media_type: Option<HeaderValue>) -> Self {
        Self::one_shot_dyn(body.into().into_dyn(), media_type)
    }

    pub(crate) fn one_shot_dyn(
        body: crate::body::DynBody,
        media_type: Option<HeaderValue>,
    ) -> Self {
        let size_hint = body.size_hint();
        Self {
            recipe: RequestBodyRecipe::OneShotHttpBody(Some(body)),
            media_type: media_type.map(PreparedBodyMediaType::Fixed),
            size_hint,
        }
    }

    /// Prepare a reconstructible advanced body. The factory is invoked once
    /// for each visible execution and is never invoked by eligibility checks.
    pub fn factory<F>(size_hint: SizeHint, media_type: Option<HeaderValue>, factory: F) -> Self
    where
        F: Fn() -> Result<AdvancedRequestBody, crate::body::BodyError> + Send + Sync + 'static,
    {
        Self::replay_factory_terminal(size_hint, media_type, move || {
            factory().map(|body| TerminalBody::OneShotHttpBody(body.into_dyn()))
        })
    }

    #[cfg(test)]
    pub(crate) fn replay_factory<F>(
        size_hint: SizeHint,
        media_type: Option<HeaderValue>,
        factory: F,
    ) -> Self
    where
        F: Fn() -> Result<crate::body::DynBody, crate::body::BodyError> + Send + Sync + 'static,
    {
        Self::replay_factory_terminal(size_hint, media_type, move || {
            factory().map(TerminalBody::OneShotHttpBody)
        })
    }

    /// Prepare a reconstructible byte stream. Each factory result is a fresh
    /// terminal stream for one visible execution.
    pub fn stream_factory<F>(
        size_hint: SizeHint,
        media_type: Option<HeaderValue>,
        factory: F,
    ) -> Self
    where
        F: Fn() -> Result<StreamBody, crate::body::BodyError> + Send + Sync + 'static,
    {
        Self::replay_factory_terminal(size_hint, media_type, move || {
            factory().map(TerminalBody::OneShotByteStream)
        })
    }

    /// Private terminal-recipe factory used by core-owned body producers.
    pub(crate) fn replay_factory_terminal<F>(
        size_hint: SizeHint,
        media_type: Option<HeaderValue>,
        factory: F,
    ) -> Self
    where
        F: Fn() -> Result<TerminalBody, crate::body::BodyError> + Send + Sync + 'static,
    {
        Self {
            recipe: RequestBodyRecipe::RequestBodyFactory(ReplayFactory::Terminal(
                std::sync::Arc::new(factory),
            )),
            media_type: media_type.map(PreparedBodyMediaType::Fixed),
            size_hint,
        }
    }

    #[cfg(feature = "multipart")]
    pub(crate) fn replay_multipart_factory<F>(factory: F) -> Self
    where
        F: Fn() -> Result<MultipartBody, crate::body::BodyError> + Send + Sync + 'static,
    {
        Self {
            recipe: RequestBodyRecipe::RequestBodyFactory(ReplayFactory::Terminal(
                std::sync::Arc::new(move || factory().map(TerminalBody::MultipartRecipe)),
            )),
            media_type: Some(PreparedBodyMediaType::Dynamic),
            size_hint: SizeHint::new(),
        }
    }

    /// Prepare a reconstructible multipart body. The factory must return a
    /// complete fresh recipe; Reqwest creates a fresh form and boundary for
    /// each visible execution.
    #[cfg(feature = "multipart")]
    pub fn multipart_factory<F>(factory: F) -> Self
    where
        F: Fn() -> Result<MultipartBody, crate::multipart::MultipartBodyError>
            + Send
            + Sync
            + 'static,
    {
        Self::replay_multipart_factory(move || {
            factory().map_err(|_| crate::body::BodyError::invalid_configuration())
        })
    }

    pub fn from_stream_body(body: StreamBody, media_type: Option<HeaderValue>) -> Self {
        let size_hint = body.size_hint();
        Self {
            recipe: RequestBodyRecipe::OneShotByteStream(Some(body)),
            media_type: media_type.map(PreparedBodyMediaType::Fixed),
            size_hint,
        }
    }

    #[cfg(feature = "multipart")]
    pub(crate) fn multipart(recipe: MultipartBody) -> Self {
        Self {
            recipe: RequestBodyRecipe::MultipartRecipe(Some(recipe)),
            // Reqwest owns the complete multipart value including boundary.
            media_type: Some(PreparedBodyMediaType::Dynamic),
            size_hint: SizeHint::new(),
        }
    }

    pub fn media_type(&self) -> Option<&HeaderValue> {
        self.media_type
            .as_ref()
            .and_then(PreparedBodyMediaType::as_fixed)
    }

    #[doc(hidden)]
    pub fn reserves_content_type(&self) -> bool {
        matches!(self.media_type, Some(PreparedBodyMediaType::Dynamic))
    }

    pub fn size_hint(&self) -> SizeHint {
        self.size_hint.clone()
    }

    pub fn is_replayable(&self) -> bool {
        match &self.recipe {
            RequestBodyRecipe::Empty
            | RequestBodyRecipe::ReusableBytes(_)
            | RequestBodyRecipe::RequestBodyFactory(_) => true,
            #[cfg(feature = "multipart")]
            RequestBodyRecipe::MultipartRecipe(Some(recipe)) => recipe.is_reconstructible(),
            _ => false,
        }
    }

    pub(crate) fn produce_for_execution(&mut self) -> Result<ProducedBody, BodyProductionError> {
        let size_hint = self.size_hint.clone();
        match &mut self.recipe {
            RequestBodyRecipe::Empty => Ok(ProducedBody {
                terminal: TerminalBody::Empty,
                media_type: None,
                exact_length: None,
            }),
            RequestBodyRecipe::ReusableBytes(bytes) => Ok(ProducedBody {
                terminal: TerminalBody::ReusableBytes(bytes.clone()),
                media_type: self.media_type().cloned(),
                exact_length: None,
            }),
            RequestBodyRecipe::OneShotByteStream(body) => body
                .take()
                .map(|stream| ProducedBody {
                    terminal: TerminalBody::OneShotByteStream(stream),
                    media_type: self.media_type().cloned(),
                    exact_length: size_hint.exact(),
                })
                .ok_or_else(BodyProductionError::already_consumed),
            RequestBodyRecipe::OneShotHttpBody(body) => body
                .take()
                .map(|body| ProducedBody {
                    terminal: TerminalBody::OneShotHttpBody(body),
                    media_type: self.media_type().cloned(),
                    exact_length: size_hint.exact(),
                })
                .ok_or_else(BodyProductionError::already_consumed),
            RequestBodyRecipe::RequestBodyFactory(ReplayFactory::Terminal(factory)) => factory()
                .map(|terminal| ProducedBody {
                    exact_length: terminal_exact_length(&terminal, &size_hint),
                    terminal,
                    media_type: self.media_type().cloned(),
                })
                .map_err(BodyProductionError::factory_failure),
            #[cfg(feature = "multipart")]
            RequestBodyRecipe::MultipartRecipe(recipe) => {
                let produced = recipe
                    .as_ref()
                    .and_then(MultipartBody::clone_if_reconstructible)
                    .or_else(|| recipe.take());
                produced
                    .map(|recipe| {
                        Ok(ProducedBody {
                            terminal: TerminalBody::MultipartRecipe(recipe),
                            media_type: None,
                            exact_length: None,
                        })
                    })
                    .unwrap_or_else(|| Err(BodyProductionError::already_consumed()))
            }
        }
    }
}

pub(crate) fn apply_prepared_body_media_type(
    headers: &mut http::HeaderMap,
    body: &PreparedBody,
) -> Result<(), ()> {
    match body.media_type.as_ref() {
        None | Some(PreparedBodyMediaType::Fixed(_)) => {
            let media_type = body.media_type();
            if let Some(media_type) = media_type {
                if headers.contains_key(http::header::CONTENT_TYPE) {
                    return (headers.get(http::header::CONTENT_TYPE) == Some(media_type))
                        .then_some(())
                        .ok_or(());
                }
                headers.insert(http::header::CONTENT_TYPE, media_type.clone());
            }
            Ok(())
        }
        Some(PreparedBodyMediaType::Dynamic) => {
            if headers.contains_key(http::header::CONTENT_TYPE) {
                Err(())
            } else {
                Ok(())
            }
        }
    }
}

pub(crate) fn apply_execution_media_type(
    headers: &mut http::HeaderMap,
    media_type: Option<&HeaderValue>,
) -> Result<(), ()> {
    let Some(media_type) = media_type else {
        return Ok(());
    };
    match headers.get(http::header::CONTENT_TYPE) {
        Some(existing) if existing == media_type => Ok(()),
        Some(_) => Err(()),
        None => {
            headers.insert(http::header::CONTENT_TYPE, media_type.clone());
            Ok(())
        }
    }
}

impl fmt::Debug for PreparedBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category = match self.recipe {
            RequestBodyRecipe::Empty => "Empty",
            RequestBodyRecipe::ReusableBytes(_) => "ReusableBytes",
            RequestBodyRecipe::OneShotByteStream(Some(_)) => "OneShotByteStream",
            RequestBodyRecipe::OneShotByteStream(None) => "OneShotByteStreamConsumed",
            RequestBodyRecipe::OneShotHttpBody(Some(_)) => "OneShotHttpBody",
            RequestBodyRecipe::OneShotHttpBody(None) => "OneShotHttpBodyConsumed",
            RequestBodyRecipe::RequestBodyFactory(_) => "RequestBodyFactory",
            #[cfg(feature = "multipart")]
            RequestBodyRecipe::MultipartRecipe(Some(_)) => "MultipartRecipe",
            #[cfg(feature = "multipart")]
            RequestBodyRecipe::MultipartRecipe(None) => "MultipartRecipeConsumed",
        };
        f.debug_struct("PreparedBody")
            .field("category", &category)
            .field("has_media_type", &self.media_type.is_some())
            .field("size_hint", &self.size_hint)
            .field("replayable", &self.is_replayable())
            .finish()
    }
}

// Terminal native-body enforcement. Exact length remains inside the global
// request limiter installed by the managed execution path.
fn observe_native_request_body(
    body: reqwest::Body,
    exact_length: Option<u64>,
    errors: crate::body::RequestBodyErrorSlot,
) -> reqwest::Body {
    match exact_length {
        Some(length) => reqwest::Body::wrap(crate::body::ObservedRequestBody::new(
            crate::body::ExactLengthBody::new(body, length),
            errors,
        )),
        None => reqwest::Body::wrap(crate::body::ObservedRequestBody::new(body, errors)),
    }
}

fn validate_reusable_exact_length(
    actual: u64,
    exact_length: Option<u64>,
) -> Result<(), crate::body::BodyError> {
    let Some(expected) = exact_length else {
        return Ok(());
    };
    match actual.cmp(&expected) {
        std::cmp::Ordering::Less => Err(crate::body::BodyError::exact_length_underflow(
            expected, actual,
        )),
        std::cmp::Ordering::Greater => Err(crate::body::BodyError::exact_length_overflow(
            expected, actual,
        )),
        std::cmp::Ordering::Equal => Ok(()),
    }
}

fn terminal_exact_length(_terminal: &TerminalBody, hint: &SizeHint) -> Option<u64> {
    #[cfg(feature = "multipart")]
    if matches!(_terminal, TerminalBody::MultipartRecipe(_)) {
        return None;
    }
    hint.exact()
}

fn exact_size_hint(len: u64) -> SizeHint {
    let mut hint = SizeHint::new();
    hint.set_exact(len);
    hint
}

#[derive(Debug)]
pub struct PreparedRequestEntity {
    pub body: PreparedBody,
}

pub trait RequestEntity {
    type Input;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError>;
}

/// Supported authentication placement for a hand-written prepared endpoint.
///
/// The value declares only resolved public facts. Core constructs and owns the
/// authentication plan, placement validation, credential lifecycle, and
/// challenge sequencing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestAuthentication {
    credential: crate::auth::CredentialId,
    placement: PreparedAuthenticationPlacement,
    challenge: crate::auth::AuthChallengePolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PreparedAuthenticationPlacement {
    Bearer,
    Basic,
    Header(&'static str),
    Query(&'static str),
}

impl RequestAuthentication {
    pub fn bearer(credential: crate::auth::CredentialId) -> Self {
        Self {
            credential,
            placement: PreparedAuthenticationPlacement::Bearer,
            challenge: crate::auth::AuthChallengePolicy::Unauthorized,
        }
    }

    pub fn basic(credential: crate::auth::CredentialId) -> Self {
        Self {
            credential,
            placement: PreparedAuthenticationPlacement::Basic,
            challenge: crate::auth::AuthChallengePolicy::Unauthorized,
        }
    }

    pub fn header(credential: crate::auth::CredentialId, name: &'static str) -> Self {
        Self {
            credential,
            placement: PreparedAuthenticationPlacement::Header(name),
            challenge: crate::auth::AuthChallengePolicy::Unauthorized,
        }
    }

    pub fn query(credential: crate::auth::CredentialId, name: &'static str) -> Self {
        Self {
            credential,
            placement: PreparedAuthenticationPlacement::Query(name),
            challenge: crate::auth::AuthChallengePolicy::Unauthorized,
        }
    }

    /// Selects the HTTP challenge statuses that may trigger the single
    /// bounded authentication recovery.
    pub fn challenge_policy(mut self, policy: crate::auth::AuthChallengePolicy) -> Self {
        self.challenge = policy;
        self
    }

    fn into_requirement(self) -> crate::auth::AuthRequirement {
        let placement = match self.placement {
            PreparedAuthenticationPlacement::Bearer => crate::auth::AuthPlacement::Bearer,
            PreparedAuthenticationPlacement::Basic => crate::auth::AuthPlacement::Basic,
            PreparedAuthenticationPlacement::Header(name) => {
                crate::auth::AuthPlacement::Header(name)
            }
            PreparedAuthenticationPlacement::Query(name) => crate::auth::AuthPlacement::Query(name),
        };
        crate::auth::AuthRequirement {
            credential: crate::auth::CredentialRef {
                id: self.credential,
            },
            placement,
            usage_id: crate::auth::AuthUsageId::new("prepared-endpoint"),
            step_id: None,
            provenance: crate::auth::AuthProvenance::new("advanced"),
            challenge: self.challenge,
        }
    }
}

#[cfg(test)]
mod request_authentication_tests {
    use super::*;

    #[test]
    fn challenge_policy_defaults_to_401_and_can_explicitly_include_403() {
        let credential = crate::auth::CredentialId::new("test", "credential");
        assert_eq!(
            RequestAuthentication::bearer(credential.clone())
                .into_requirement()
                .challenge,
            crate::auth::AuthChallengePolicy::Unauthorized
        );
        assert_eq!(
            RequestAuthentication::header(credential, "X-Key")
                .challenge_policy(crate::auth::AuthChallengePolicy::UnauthorizedOrForbidden)
                .into_requirement()
                .challenge,
            crate::auth::AuthChallengePolicy::UnauthorizedOrForbidden
        );
    }
}

struct PreparedEndpointCore {
    endpoint: &'static str,
    method: Method,
    path: &'static str,
    idempotent: bool,
    request: PreparedRequestEntity,
    authentication: Option<RequestAuthentication>,
    timeout: Option<Duration>,
}

impl fmt::Debug for PreparedEndpointCore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedEndpointCore")
            .field("endpoint", &self.endpoint)
            .field("method", &self.method)
            .field("path", &self.path)
            .field("idempotent", &self.idempotent)
            .field("authenticated", &self.authentication.is_some())
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl PreparedEndpointCore {
    fn new(
        endpoint: &'static str,
        method: Method,
        path: &'static str,
        request: PreparedRequestEntity,
    ) -> Self {
        let idempotent = matches!(
            method,
            Method::GET
                | Method::HEAD
                | Method::PUT
                | Method::DELETE
                | Method::OPTIONS
                | Method::TRACE
        );
        Self {
            endpoint,
            method,
            path,
            idempotent,
            request,
            authentication: None,
            timeout: None,
        }
    }

    fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.endpoint,
            method: self.method.clone(),
        }
    }

    fn into_plan<Cx: ClientContext>(
        self,
        client: &ApiClient<Cx>,
        response: ResponsePlan,
    ) -> Result<RequestPlan, ApiClientError> {
        let ctx = self.error_context();
        if !self.path.starts_with('/')
            || self.path.contains('?')
            || self.path.contains('#')
            || self.path.contains('\\')
        {
            return Err(ApiClientError::invalid_param(ctx, "path"));
        }

        let plan_ctx = client.plan_context();
        let mut route = Cx::base_route(plan_ctx.vars(), plan_ctx.auth_vars());
        route.path_mut().push_raw(self.path);
        route.host().validate(ctx.clone())?;
        let resolved_route = crate::endpoint::ResolvedRoute {
            scheme: Cx::SCHEME,
            host: route.host().join(Cx::DOMAIN),
            path: route.path().as_str().to_string(),
        };

        let mut policy = Cx::base_policy(plan_ctx.vars(), plan_ctx.auth_vars(), &ctx)?.into_inner();
        policy.set_layer(crate::policy::PolicyLayer::Runtime);
        if self.method != Method::HEAD
            && !response.no_content
            && let Some(accept) = response.accept.clone()
        {
            policy.ensure_accept(accept);
        }
        let (headers, query, timeout, mut rate_limit) = policy.into_parts();
        rate_limit.canonicalize();
        let auth = self
            .authentication
            .map(RequestAuthentication::into_requirement)
            .into_iter()
            .collect();

        Ok(RequestPlan {
            endpoint: crate::endpoint::EndpointPlan {
                meta: crate::endpoint::EndpointMeta {
                    name: self.endpoint,
                    method: self.method,
                    idempotent: self.idempotent,
                    facade_path: &[],
                },
                route: resolved_route,
                policy: crate::policy::ResolvedPolicy {
                    headers,
                    query,
                    timeout,
                    auth: crate::auth::AuthPlan { requirements: auth },
                    rate_limit,
                },
                response,
                pagination: None,
            },
            body: self.request.body,
            overrides: crate::endpoint::RequestOverrides {
                timeout: self.timeout,
                ..crate::endpoint::RequestOverrides::default()
            },
        })
    }
}

/// Core-owned execution adapter for a hand-written buffered endpoint.
///
/// Applications supply public endpoint facts and a prepared request entity;
/// Core retains ownership of route, policy, authentication, and runtime plans.
pub struct PreparedEndpoint<C> {
    core: PreparedEndpointCore,
    _codec: PhantomData<fn() -> C>,
}

impl<C> fmt::Debug for PreparedEndpoint<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedEndpoint")
            .field("core", &self.core)
            .finish_non_exhaustive()
    }
}

impl<C> PreparedEndpoint<C> {
    pub fn new(
        endpoint: &'static str,
        method: Method,
        path: &'static str,
        request: PreparedRequestEntity,
    ) -> Self {
        Self {
            core: PreparedEndpointCore::new(endpoint, method, path, request),
            _codec: PhantomData,
        }
    }

    pub fn from_request_entity<E>(
        endpoint: &'static str,
        method: Method,
        path: &'static str,
        input: E::Input,
    ) -> Result<Self, ApiClientError>
    where
        E: RequestEntity,
    {
        let ctx = ErrorContext {
            endpoint,
            method: method.clone(),
        };
        Ok(Self::new(endpoint, method, path, E::prepare(input, ctx)?))
    }

    pub fn authentication(mut self, authentication: RequestAuthentication) -> Self {
        self.core.authentication = Some(authentication);
        self
    }

    pub fn idempotent(mut self, idempotent: bool) -> Self {
        self.core.idempotent = idempotent;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.core.timeout = Some(timeout);
        self
    }
}

impl<C> PreparedEndpoint<C>
where
    C: ResponseCodec,
{
    pub async fn execute<Cx: ClientContext>(
        self,
        client: &ApiClient<Cx>,
    ) -> Result<C::Value, ApiClientError> {
        let ctx = self.core.error_context();
        let response = response_plan_from_codec::<C>(ctx)?;
        let plan = self.core.into_plan(client, response)?;
        execute_buffered_codec_response::<Cx, C>(client, plan).await
    }

    pub async fn response<Cx: ClientContext>(
        self,
        client: &ApiClient<Cx>,
    ) -> Result<DecodedResponse<C::Value>, ApiClientError> {
        let ctx = self.core.error_context();
        let response = response_plan_from_codec::<C>(ctx)?;
        let plan = self.core.into_plan(client, response)?;
        execute_buffered_codec_response_with_meta::<Cx, C>(client, plan).await
    }
}

/// Core-owned execution adapter for a hand-written streaming endpoint.
pub struct PreparedStreamEndpoint<M> {
    core: PreparedEndpointCore,
    _media: PhantomData<fn() -> M>,
}

impl<M> PreparedStreamEndpoint<M> {
    pub fn new(
        endpoint: &'static str,
        method: Method,
        path: &'static str,
        request: PreparedRequestEntity,
    ) -> Self {
        Self {
            core: PreparedEndpointCore::new(endpoint, method, path, request),
            _media: PhantomData,
        }
    }

    pub fn authentication(mut self, authentication: RequestAuthentication) -> Self {
        self.core.authentication = Some(authentication);
        self
    }

    pub fn idempotent(mut self, idempotent: bool) -> Self {
        self.core.idempotent = idempotent;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.core.timeout = Some(timeout);
        self
    }
}

impl<M> PreparedStreamEndpoint<M>
where
    M: ContentType,
{
    pub async fn execute<Cx: ClientContext>(
        self,
        client: &ApiClient<Cx>,
    ) -> Result<StreamResponse<M>, ApiClientError> {
        let ctx = self.core.error_context();
        let response = ResponsePlan {
            accept: Some(
                M::try_header_value()
                    .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
            ),
            no_content: false,
            format: crate::codec::Format::Binary,
        };
        let plan = self.core.into_plan(client, response)?;
        client.execute_stream_response::<M>(plan).await
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoRequestBody;

impl RequestEntity for NoRequestBody {
    type Input = ();

    fn prepare(
        _: Self::Input,
        _ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        Ok(PreparedRequestEntity {
            body: PreparedBody::empty(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EncodedRequest<C>(PhantomData<fn() -> C>);

impl<C> RequestEntity for EncodedRequest<C>
where
    C: BodyCodec,
{
    type Input = C::Value;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        let encoded = C::encode(input, EncodeContext::new(ctx.endpoint, &ctx.method))
            .map_err(|_| ApiClientError::request_body_codec_error(ctx.clone()))?;
        let (bytes, _format) = encoded.into_parts();
        let content_type = C::try_content_type()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?;
        Ok(PreparedRequestEntity {
            body: PreparedBody::reusable_bytes(bytes, content_type),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawStreamRequest<M>(PhantomData<fn() -> M>);

impl<M> RequestEntity for RawStreamRequest<M>
where
    M: ContentType,
{
    type Input = StreamBody;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        let content_type = M::try_header_value()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?;
        Ok(PreparedRequestEntity {
            body: PreparedBody::from_stream_body(input, Some(content_type)),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(feature = "multipart")]
pub struct MultipartRequest;

#[cfg(feature = "multipart")]
impl RequestEntity for MultipartRequest {
    type Input = MultipartBody;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        // Validate syntax now, but defer native Form/Part construction until
        // after auth collision preflight and hooks have completed.
        input
            .validate()
            .map_err(|source| ApiClientError::codec_error(ctx.clone(), source))?;
        Ok(PreparedRequestEntity {
            body: PreparedBody::multipart(input),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResponseEntityCapabilities {
    pub supports_pagination: bool,
    pub is_streaming: bool,
    pub is_no_content: bool,
}

#[derive(Debug, Clone)]
pub struct ResponseEntityPlan {
    pub response_plan: ResponsePlan,
    pub capabilities: ResponseEntityCapabilities,
}

#[doc(hidden)]
pub type ResponseEntityFuture<'a, Output> =
    Pin<Box<dyn Future<Output = Result<Output, ApiClientError>> + Send + 'a>>;

pub trait ResponseEntity {
    type Output;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError>;

    fn execute<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> ResponseEntityFuture<'a, Self::Output>
    where
        Cx: ClientContext;
}

#[doc(hidden)]
pub trait ResponseEntityWithMeta: ResponseEntity {
    fn execute_with_meta<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> ResponseEntityFuture<'a, DecodedResponse<Self::Output>>
    where
        Cx: ClientContext;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BufferedResponse<C>(PhantomData<fn() -> C>);

impl<C> ResponseEntity for BufferedResponse<C>
where
    C: ResponseCodec,
{
    type Output = C::Value;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: response_plan_from_codec::<C>(ctx)?,
            capabilities: ResponseEntityCapabilities {
                supports_pagination: !C::is_no_content(),
                is_streaming: false,
                is_no_content: C::is_no_content(),
            },
        })
    }

    fn execute<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
    where
        Cx: ClientContext,
    {
        Box::pin(execute_buffered_codec_response::<Cx, C>(client, plan))
    }
}

impl<C> ResponseEntityWithMeta for BufferedResponse<C>
where
    C: ResponseCodec,
{
    fn execute_with_meta<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<DecodedResponse<Self::Output>, ApiClientError>> + Send + 'a>,
    >
    where
        Cx: ClientContext,
    {
        Box::pin(execute_buffered_codec_response_with_meta::<Cx, C>(
            client, plan,
        ))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BytesResponse;

impl ResponseEntity for BytesResponse {
    type Output = Bytes;

    fn plan(_ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: None,
                no_content: false,
                format: crate::codec::Format::Binary,
            },
            capabilities: ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: false,
                is_no_content: false,
            },
        })
    }

    fn execute<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
    where
        Cx: ClientContext,
    {
        Box::pin(execute_bytes_response(client, plan))
    }
}

impl ResponseEntityWithMeta for BytesResponse {
    fn execute_with_meta<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<DecodedResponse<Self::Output>, ApiClientError>> + Send + 'a>,
    >
    where
        Cx: ClientContext,
    {
        Box::pin(execute_bytes_response_with_meta(client, plan))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoContentResponse;

impl ResponseEntity for NoContentResponse {
    type Output = ();

    fn plan(_ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: None,
                no_content: true,
                format: crate::codec::Format::Text,
            },
            capabilities: ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: false,
                is_no_content: true,
            },
        })
    }

    fn execute<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
    where
        Cx: ClientContext,
    {
        Box::pin(execute_no_content_response(client, plan))
    }
}

impl ResponseEntityWithMeta for NoContentResponse {
    fn execute_with_meta<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<DecodedResponse<Self::Output>, ApiClientError>> + Send + 'a>,
    >
    where
        Cx: ClientContext,
    {
        Box::pin(execute_no_content_response_with_meta(client, plan))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawStreamResponse<M>(PhantomData<fn() -> M>);

impl<M> ResponseEntity for RawStreamResponse<M>
where
    M: ContentType,
{
    type Output = StreamResponse<M>;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: Some(
                    M::try_header_value()
                        .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
                ),
                no_content: false,
                format: crate::codec::Format::Binary,
            },
            capabilities: ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: true,
                is_no_content: false,
            },
        })
    }

    fn execute<'a, Cx>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
    where
        Cx: ClientContext,
    {
        Box::pin(async move { client.execute_stream_response::<M>(plan).await })
    }
}

async fn execute_buffered_codec_response<Cx, C>(
    client: &ApiClient<Cx>,
    plan: RequestPlan,
) -> Result<C::Value, ApiClientError>
where
    Cx: ClientContext,
    C: ResponseCodec,
{
    execute_buffered_codec_response_with_meta::<Cx, C>(client, plan)
        .await
        .map(|decoded| decoded.value)
}

async fn execute_buffered_codec_response_with_meta<Cx, C>(
    client: &ApiClient<Cx>,
    plan: RequestPlan,
) -> Result<DecodedResponse<C::Value>, ApiClientError>
where
    Cx: ClientContext,
    C: ResponseCodec,
{
    let resp = if C::is_no_content() {
        client.execute_plan_raw_skip_body(plan).await?
    } else {
        client.execute_plan_raw(plan).await?
    };
    decode_buffered_response_with_meta::<C>(resp)
}

async fn execute_bytes_response<Cx>(
    client: &ApiClient<Cx>,
    plan: RequestPlan,
) -> Result<Bytes, ApiClientError>
where
    Cx: ClientContext,
{
    execute_bytes_response_with_meta(client, plan)
        .await
        .map(|decoded| decoded.value)
}

async fn execute_no_content_response<Cx>(
    client: &ApiClient<Cx>,
    plan: RequestPlan,
) -> Result<(), ApiClientError>
where
    Cx: ClientContext,
{
    execute_no_content_response_with_meta(client, plan)
        .await
        .map(|decoded| decoded.value)
}

async fn execute_bytes_response_with_meta<Cx>(
    client: &ApiClient<Cx>,
    plan: RequestPlan,
) -> Result<DecodedResponse<Bytes>, ApiClientError>
where
    Cx: ClientContext,
{
    let resp = client.execute_plan_raw(plan).await?;
    decode_bytes_response_with_meta(resp)
}

async fn execute_no_content_response_with_meta<Cx>(
    client: &ApiClient<Cx>,
    plan: RequestPlan,
) -> Result<DecodedResponse<()>, ApiClientError>
where
    Cx: ClientContext,
{
    let resp = client.execute_plan_raw_skip_body(plan).await?;
    decode_no_content_response_with_meta(resp)
}

fn response_plan_from_codec<C>(ctx: ErrorContext) -> Result<ResponsePlan, ApiClientError>
where
    C: ResponseCodec,
{
    Ok(ResponsePlan {
        accept: C::try_accept()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
        no_content: C::is_no_content(),
        format: C::format(),
    })
}

fn decode_buffered_response_with_meta<C>(
    resp: BuiltResponse,
) -> Result<DecodedResponse<C::Value>, ApiClientError>
where
    C: ResponseCodec,
{
    let ctx = validate_buffered_response(&resp, C::is_no_content())?;
    let content_type = resp
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let status = resp.status();
    let (message, context) = resp.into_parts();
    let (parts, body) = message.into_parts();
    let value = if C::is_no_content() {
        C::decode(
            Bytes::new(),
            DecodeContext::new(ctx.endpoint, &ctx.method, status, content_type.as_deref()),
        )
        .map_err(|_| {
            ApiClientError::response_body_decode_error(ctx.clone(), status, content_type.as_deref())
        })?
    } else {
        C::decode(
            body,
            DecodeContext::new(ctx.endpoint, &ctx.method, status, content_type.as_deref()),
        )
        .map_err(|_| {
            ApiClientError::response_body_decode_error(ctx.clone(), status, content_type.as_deref())
        })?
    };
    Ok(DecodedResponse {
        meta: context.meta,
        url: context.logical_url,
        status,
        headers: parts.headers,
        value,
    })
}

fn decode_bytes_response_with_meta(
    resp: BuiltResponse,
) -> Result<DecodedResponse<Bytes>, ApiClientError> {
    let _ctx = validate_buffered_response(&resp, false)?;
    let status = resp.status();
    let (message, context) = resp.into_parts();
    let (parts, body) = message.into_parts();
    Ok(DecodedResponse {
        meta: context.meta,
        url: context.logical_url,
        status,
        headers: parts.headers,
        value: body,
    })
}

fn decode_no_content_response_with_meta(
    resp: BuiltResponse,
) -> Result<DecodedResponse<()>, ApiClientError> {
    let _ctx = validate_buffered_response(&resp, true)?;
    let status = resp.status();
    let (message, context) = resp.into_parts();
    let (parts, _) = message.into_parts();
    Ok(DecodedResponse {
        meta: context.meta,
        url: context.logical_url,
        status,
        headers: parts.headers,
        value: (),
    })
}

fn validate_buffered_response(
    resp: &BuiltResponse,
    no_content: bool,
) -> Result<ErrorContext, ApiClientError> {
    let ctx = ErrorContext {
        endpoint: resp.meta().endpoint,
        method: resp.meta().method.clone(),
    };
    if resp.meta().method == http::Method::HEAD && !no_content {
        return Err(ApiClientError::HeadRequiresNoContent { ctx });
    }
    if matches!(
        resp.status(),
        http::StatusCode::NO_CONTENT | http::StatusCode::RESET_CONTENT
    ) && !no_content
    {
        return Err(ApiClientError::NoContentStatusRequiresNoContent {
            ctx: ctx.clone(),
            status: resp.status(),
        });
    }
    Ok(ctx)
}

#[cfg(test)]
#[allow(dead_code, unused_imports)]
mod tests {
    use super::*;
    use crate::codec::text::Text;
    use crate::codec::{BodyCodec, ContentType, EncodeContext, EncodedBody};
    use crate::media::{OctetStream, TextContentType};
    #[cfg(feature = "multipart")]
    use crate::multipart::MultipartBody;
    use http::{HeaderMap, Method, StatusCode};
    use http_body_util::BodyExt;
    #[cfg(feature = "json")]
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    struct TestStream<T> {
        items: std::collections::VecDeque<T>,
        polls: Option<Arc<AtomicUsize>>,
    }

    impl<T> TestStream<T> {
        fn new(items: impl IntoIterator<Item = T>) -> Self {
            Self {
                items: items.into_iter().collect(),
                polls: None,
            }
        }

        fn with_polls(mut self, polls: Arc<AtomicUsize>) -> Self {
            self.polls = Some(polls);
            self
        }
    }

    impl<T: Unpin> futures_core::Stream for TestStream<T> {
        type Item = T;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            if let Some(polls) = &self.polls {
                polls.fetch_add(1, Ordering::SeqCst);
            }
            Poll::Ready(self.items.pop_front())
        }
    }

    #[cfg(feature = "json")]
    use crate::media::JsonContentType;

    fn ctx() -> ErrorContext {
        ErrorContext {
            endpoint: "Example",
            method: Method::POST,
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct CustomEncodedContentType;

    impl ContentType for CustomEncodedContentType {
        const CONTENT_TYPE: &'static str = "application/x-custom-encoded";
    }

    struct TextOverrideCodec;

    impl BodyCodec for TextOverrideCodec {
        type Value = String;
        type Content = CustomEncodedContentType;

        fn encode(
            value: Self::Value,
            _ctx: EncodeContext<'_>,
        ) -> Result<EncodedBody, crate::codec::CodecError> {
            Ok(EncodedBody::from_bytes(value.into_bytes()).text())
        }
    }

    #[derive(Clone)]
    struct TestCx;

    impl ClientContext for TestCx {
        type Vars = ();
        type AuthVars = ();
        type AuthState = crate::auth::NoAuthState;
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.test";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
            crate::auth::NoAuthState
        }
    }

    fn request_plan(response_plan: ResponsePlan) -> RequestPlan {
        RequestPlan {
            endpoint: crate::endpoint::EndpointPlan {
                meta: crate::endpoint::EndpointMeta {
                    name: "Example",
                    method: Method::POST,
                    idempotent: true,
                    facade_path: &[],
                },
                route: crate::endpoint::ResolvedRoute::new(
                    http::uri::Scheme::HTTPS,
                    "example.test",
                    "/items",
                ),
                policy: crate::policy::ResolvedPolicy::default(),
                response: response_plan,
                pagination: None,
            },
            body: PreparedBody::empty(),
            overrides: crate::endpoint::RequestOverrides::default(),
        }
    }

    fn response_headers(content_type: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(content_type) = content_type {
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_str(content_type).expect("valid content type"),
            );
        }
        headers
    }

    #[test]
    fn no_request_body_is_empty_and_replayable() {
        let prepared = NoRequestBody::prepare((), ctx()).expect("no request body");
        assert!(prepared.body.is_replayable());
        assert_eq!(prepared.body.size_hint().exact(), Some(0));
    }

    #[tokio::test]
    async fn encoded_request_matches_buffered_body_path() {
        let input = String::from("hello");
        let mut prepared =
            EncodedRequest::<Text<String>>::prepare(input.clone(), ctx()).expect("encoded request");
        let expected =
            Text::<String>::encode(input.clone(), EncodeContext::new("Example", &Method::POST))
                .expect("encode")
                .into_parts()
                .0;
        assert_eq!(
            prepared.body.media_type(),
            Some(&TextContentType::try_header_value().expect("text content type"))
        );
        let bytes = prepared
            .body
            .produce_for_execution()
            .expect("body")
            .into_dyn_body()
            .collect()
            .await
            .expect("collect")
            .to_bytes();
        assert_eq!(bytes, expected);
    }

    #[tokio::test]
    async fn encoded_request_preserves_returned_media_type() {
        let prepared = EncodedRequest::<TextOverrideCodec>::prepare(String::from("hello"), ctx())
            .expect("encoded request");
        assert_eq!(
            prepared.body.media_type(),
            Some(&CustomEncodedContentType::try_header_value().expect("custom content type"))
        );
        assert!(prepared.body.is_replayable());
    }

    #[test]
    fn raw_stream_request_is_non_replayable() {
        let prepared = RawStreamRequest::<OctetStream>::prepare(
            StreamBody::from_bytes(Bytes::from_static(b"abc")),
            ctx(),
        )
        .expect("stream request");
        assert!(!prepared.body.is_replayable());
        assert_eq!(prepared.body.size_hint().exact(), Some(3));
    }

    #[test]
    fn factory_replayability_is_recipe_level() {
        let mut advanced = PreparedBody::one_shot(
            crate::body::DynBody::from_body(crate::body::ExactLengthBody::new(
                crate::body::DynBody::from_bytes(Bytes::from_static(b"advanced")),
                8,
            )),
            None,
        );
        assert!(!advanced.is_replayable());
        assert!(
            !advanced
                .produce_for_execution()
                .expect("direct advanced terminal")
                .is_reqwest_cloneable()
        );

        let factory = PreparedBody::replay_factory_terminal(SizeHint::new(), None, || {
            Ok(TerminalBody::OneShotByteStream(StreamBody::from_bytes(
                Bytes::from_static(b"fresh"),
            )))
        });
        assert!(factory.is_replayable());
    }

    #[test]
    fn rebuildability_inspection_never_invokes_stream_factory_and_terminals_are_uncloneable() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let mut factory = PreparedBody::replay_factory(exact_size_hint(2), None, move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(crate::body::DynBody::from_bytes(Bytes::from_static(b"ok")))
        });

        assert!(factory.is_replayable());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        for expected_calls in 1..=2 {
            assert!(
                !factory
                    .produce_for_execution()
                    .expect("factory terminal")
                    .is_reqwest_cloneable()
            );
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        }
    }

    #[test]
    fn one_shot_rematerialization_fails_after_consuming_direct_stream() {
        let mut body = PreparedBody::from_stream_body(
            StreamBody::from_bytes(Bytes::from_static(b"one-shot")),
            None,
        );
        assert!(!body.is_replayable());
        assert!(
            !body
                .produce_for_execution()
                .expect("first stream terminal")
                .is_reqwest_cloneable()
        );
        assert_eq!(
            body.produce_for_execution()
                .expect_err("consumed direct stream cannot rematerialize")
                .kind(),
            BodyProductionErrorKind::AlreadyConsumed
        );
    }

    #[test]
    #[cfg(feature = "multipart")]
    fn multipart_request_prepares_stream_body() {
        let multipart = MultipartRequest::prepare(
            MultipartBody::new().bytes("payload", Bytes::from_static(b"abc")),
            ctx(),
        )
        .expect("multipart request");
        assert!(multipart.body.is_replayable());
        assert!(multipart.body.reserves_content_type());
        assert!(multipart.body.media_type().is_none());
    }

    #[test]
    #[cfg(feature = "multipart")]
    fn multipart_replayability_is_recipe_level() {
        let mut direct_stream = PreparedBody::multipart(MultipartBody::new().stream(
            "stream",
            StreamBody::from_bytes(Bytes::from_static(b"part")),
        ));
        assert!(!direct_stream.is_replayable());
        assert!(
            !direct_stream
                .produce_for_execution()
                .expect("direct streamed multipart terminal")
                .is_reqwest_cloneable()
        );

        let mut direct_reusable = PreparedBody::multipart(
            MultipartBody::new().bytes("bytes", Bytes::from_static(b"part")),
        );
        assert!(direct_reusable.is_replayable());
        assert!(
            !direct_reusable
                .produce_for_execution()
                .expect("direct multipart terminal")
                .is_reqwest_cloneable()
        );

        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let mut factory = PreparedBody::replay_multipart_factory(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(MultipartBody::new().bytes("bytes", Bytes::from_static(b"part")))
        });
        assert!(factory.is_replayable());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        for expected_calls in 1..=2 {
            assert!(
                !factory
                    .produce_for_execution()
                    .expect("factory multipart terminal")
                    .is_reqwest_cloneable()
            );
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        }
    }

    #[tokio::test]
    async fn one_shot_production_is_unpolled_and_second_use_fails_structurally() {
        let polls = Arc::new(AtomicUsize::new(0));
        let stream = TestStream::new(Vec::<Result<Bytes, crate::body::BodyError>>::new())
            .with_polls(polls.clone());
        let mut prepared =
            PreparedBody::one_shot(crate::body::DynBody::from_byte_stream(stream), None);

        assert!(!prepared.is_replayable());
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        let first = prepared
            .produce_for_execution()
            .expect("first body")
            .into_dyn_body();
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        let error = prepared
            .produce_for_execution()
            .expect_err("second body production must fail");
        assert_eq!(error.kind(), BodyProductionErrorKind::AlreadyConsumed);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        drop(first);
    }

    #[tokio::test]
    async fn declared_stream_exact_length_is_structurally_enforced() {
        let stream =
            StreamBody::from_bytes(Bytes::from_static(b"abc")).with_size_hint(exact_size_hint(4));
        let mut prepared = PreparedBody::from_stream_body(stream, None);
        let error = prepared
            .produce_for_execution()
            .expect("terminal body")
            .into_dyn_body()
            .collect()
            .await
            .expect_err("short stream must fail its exact contract");
        assert_eq!(
            error.kind(),
            crate::body::BodyErrorKind::ExactLengthUnderflow
        );
        assert!(!format!("{error:?}").contains("abc"));
    }

    #[tokio::test]
    async fn factory_terminal_exact_lengths_are_enforced_without_payload_diagnostics() {
        for (expected, bytes, kind) in [
            (0, None, None),
            (
                1,
                None,
                Some(crate::body::BodyErrorKind::ExactLengthUnderflow),
            ),
            (3, Some(b"abc".as_slice()), None),
            (
                4,
                Some(b"abc".as_slice()),
                Some(crate::body::BodyErrorKind::ExactLengthUnderflow),
            ),
            (
                2,
                Some(b"abc".as_slice()),
                Some(crate::body::BodyErrorKind::ExactLengthOverflow),
            ),
        ] {
            let mut body =
                PreparedBody::replay_factory_terminal(exact_size_hint(expected), None, move || {
                    Ok(bytes.map_or(TerminalBody::Empty, |bytes| {
                        TerminalBody::ReusableBytes(Bytes::copy_from_slice(bytes))
                    }))
                });
            let result = match body.produce_for_execution().unwrap().try_into_dyn_body() {
                Ok(body) => body.collect().await,
                Err(error) => Err(error),
            };
            match kind {
                Some(kind) => {
                    let error = result.expect_err("mismatch must fail");
                    assert_eq!(error.kind(), kind);
                    assert!(!format!("{error:?}").contains("abc"));
                }
                None => assert!(result.is_ok()),
            }
        }
    }

    #[tokio::test]
    async fn exact_length_factory_executions_keep_byte_counters_independent() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let mut factory = PreparedBody::replay_factory(exact_size_hint(3), None, move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(crate::body::DynBody::from_frame_stream(TestStream::new(
                vec![
                    Ok::<_, crate::body::BodyError>(http_body::Frame::data(Bytes::from_static(
                        b"a",
                    ))),
                    Ok(http_body::Frame::data(Bytes::from_static(b"bc"))),
                ],
            )))
        });

        for expected_calls in 1..=2 {
            let bytes = factory
                .produce_for_execution()
                .expect("fresh factory execution")
                .into_dyn_body()
                .collect()
                .await
                .expect("independent exact counter")
                .to_bytes();
            assert_eq!(bytes, Bytes::from_static(b"abc"));
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        }
    }

    #[tokio::test]
    async fn reusable_bytes_and_replay_factory_produce_fresh_execution_bodies() {
        let mut bytes_body = PreparedBody::reusable_bytes(Bytes::from_static(b"repeat"), None);
        for _ in 0..2 {
            let bytes = bytes_body
                .produce_for_execution()
                .expect("reusable body")
                .into_dyn_body()
                .collect()
                .await
                .expect("collect")
                .to_bytes();
            assert_eq!(bytes, Bytes::from_static(b"repeat"));
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let mut factory = PreparedBody::replay_factory(exact_size_hint(5), None, move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(crate::body::DynBody::from_bytes(Bytes::from_static(
                b"fresh",
            )))
        });
        assert!(factory.is_replayable());
        assert!(
            bytes_body
                .produce_for_execution()
                .expect("reusable terminal")
                .is_reqwest_cloneable()
        );
        assert!(
            !factory
                .produce_for_execution()
                .expect("factory terminal")
                .is_reqwest_cloneable()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        for expected_calls in 2..=3 {
            let body = factory
                .produce_for_execution()
                .expect("factory body")
                .into_dyn_body();
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
            assert_eq!(
                body.collect().await.expect("collect").to_bytes(),
                Bytes::from_static(b"fresh")
            );
        }
    }

    #[test]
    fn factory_failure_is_distinct_from_one_shot_exhaustion_and_is_redacted() {
        let sentinel = "FACTORY_PRODUCER_SENTINEL_MUST_NOT_APPEAR";
        let mut factory = PreparedBody::replay_factory(exact_size_hint(0), None, move || {
            let _ = sentinel;
            Err(crate::body::BodyError::invalid_configuration())
        });
        let error = factory
            .produce_for_execution()
            .expect_err("factory should fail");
        assert_eq!(error.kind(), BodyProductionErrorKind::FactoryFailure);
        assert_eq!(
            error.body_error_kind(),
            Some(crate::body::BodyErrorKind::InvalidConfiguration)
        );
        assert!(!format!("{error}").contains(sentinel));
        assert!(!format!("{error:?}").contains(sentinel));
        let source = std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<crate::body::BodyError>())
            .expect("safe structured factory source is retained");
        assert_eq!(
            source.kind(),
            crate::body::BodyErrorKind::InvalidConfiguration
        );
    }

    #[test]
    fn recipe_replayability_check_does_not_invoke_factory() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let factory = PreparedBody::replay_factory(exact_size_hint(0), None, move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(crate::body::DynBody::empty())
        });
        assert!(factory.is_replayable());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let one_shot = PreparedBody::one_shot(crate::body::DynBody::empty(), None);
        assert!(!one_shot.is_replayable());
    }

    fn native_request_from_produced(produced: ProducedBody) -> reqwest::Request {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("test Reqwest client");
        let builder = client.request(
            http::Method::POST,
            url::Url::parse("http://example.test/native").expect("static URL"),
        );
        let (builder, media_type, _body_errors) = produced
            .apply_to_reqwest(builder)
            .expect("native body materialization");
        let mut request = builder.build().expect("native request");
        apply_execution_media_type(request.headers_mut(), media_type.as_ref()).expect("media type");
        request
    }

    #[test]
    fn reusable_bytes_remain_a_direct_reqwest_byte_body() {
        let mut prepared = PreparedBody::reusable_bytes(Bytes::from_static(b"direct"), None);
        let request =
            native_request_from_produced(prepared.produce_for_execution().expect("produced bytes"));
        assert_eq!(
            request.body().and_then(reqwest::Body::as_bytes),
            Some(&b"direct"[..])
        );
    }

    #[test]
    fn empty_stream_and_advanced_terminals_map_to_native_reqwest_capabilities() {
        let empty = native_request_from_produced(
            PreparedBody::empty()
                .produce_for_execution()
                .expect("empty terminal"),
        );
        assert!(empty.body().is_none());

        let stream = StreamBody::from_byte_stream(TestStream::new([Ok::<
            _,
            crate::stream_body::StreamBodyError,
        >(Bytes::from_static(
            b"stream",
        ))]));
        let streamed = native_request_from_produced(
            PreparedBody::from_stream_body(stream, None)
                .produce_for_execution()
                .expect("stream terminal"),
        );
        assert!(
            streamed
                .body()
                .is_some_and(|body| body.as_bytes().is_none())
        );

        let advanced = native_request_from_produced(
            PreparedBody::one_shot(
                crate::body::DynBody::from_bytes(Bytes::from_static(b"body")),
                None,
            )
            .produce_for_execution()
            .expect("advanced terminal"),
        );
        assert!(
            advanced
                .body()
                .is_some_and(|body| body.as_bytes().is_none())
        );
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn multipart_factories_build_fresh_native_forms_and_boundaries() {
        let mut prepared = PreparedBody::replay_multipart_factory(|| {
            Ok(MultipartBody::new().text("field", "value"))
        });
        let first = native_request_from_produced(
            prepared.produce_for_execution().expect("first multipart"),
        );
        let second = native_request_from_produced(
            prepared.produce_for_execution().expect("second multipart"),
        );
        let first_type = first
            .headers()
            .get(http::header::CONTENT_TYPE)
            .expect("first Content-Type");
        let second_type = second
            .headers()
            .get(http::header::CONTENT_TYPE)
            .expect("second Content-Type");
        assert!(
            first_type
                .to_str()
                .expect("header")
                .starts_with("multipart/form-data; boundary=")
        );
        assert!(
            second_type
                .to_str()
                .expect("header")
                .starts_with("multipart/form-data; boundary=")
        );
        assert_ne!(first_type, second_type);
        assert!(first.body().is_some_and(|body| body.as_bytes().is_none()));
        assert!(second.body().is_some_and(|body| body.as_bytes().is_none()));
    }

    #[tokio::test]
    async fn prepared_one_shot_preserves_frames_and_trailers_during_native_adaptation() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-trailer", http::HeaderValue::from_static("present"));
        let frames = TestStream::new(vec![
            Ok::<_, crate::body::BodyError>(http_body::Frame::data(Bytes::from_static(b"data"))),
            Ok(http_body::Frame::trailers(trailers.clone())),
        ]);
        let mut prepared =
            PreparedBody::one_shot(crate::body::DynBody::from_frame_stream(frames), None);
        let mut body = prepared
            .produce_for_execution()
            .expect("body")
            .into_dyn_body();
        let data = body
            .frame()
            .await
            .expect("data frame")
            .expect("data")
            .into_data()
            .expect("data frame");
        assert_eq!(data, Bytes::from_static(b"data"));
        let actual_trailers = body
            .frame()
            .await
            .expect("trailer frame")
            .expect("trailers")
            .into_trailers()
            .expect("trailer frame");
        assert_eq!(actual_trailers, trailers);
    }

    #[test]
    fn streaming_response_adapter_reports_streaming_capabilities() {
        let plan = RawStreamResponse::<OctetStream>::plan(ctx()).expect("stream response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: true,
                is_no_content: false,
            }
        );
        assert_eq!(
            plan.response_plan.accept,
            Some(OctetStream::try_header_value().expect("octet-stream"))
        );
        assert_eq!(plan.response_plan.format, crate::codec::Format::Binary);
    }
}
