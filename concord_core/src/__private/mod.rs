//! Concord's current generated-code integration contract.
//!
//! This surface is public only so macro expansions can refer to it across
//! crate boundaries. It is generated-only, unstable implementation
//! integration. It is intentionally not a transport, middleware, runtime
//! configuration, or general reflection API.

/// Semantic identity for the current Reqwest-native generated contract.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct ReqwestNativeGeneratedContract(());

/// Current-contract value referenced by every generated API module.
#[doc(hidden)]
pub const GENERATED_CONTRACT: ReqwestNativeGeneratedContract = ReqwestNativeGeneratedContract(());

/// Validate the generated integration contract during constant evaluation.
#[doc(hidden)]
pub const fn assert_generated_contract(_: ReqwestNativeGeneratedContract) {}

/// Opaque typed provider binding used by generated client contexts.
///
/// This adapter only associates a generated credential identifier with an
/// existing core-owned provider slot. Authentication execution and mutable
/// cache state remain outside this generated integration module.
#[doc(hidden)]
pub use crate::auth::{
    AuthChallengeMode, AuthPreparationMode, AuthProvenance, AuthProviderBinding, AuthUsageId,
    CredentialId,
};

/// Opaque generated association between one provider and core-owned credential state.
#[doc(hidden)]
pub struct GeneratedCredentialBinding<Cx, P>
where
    Cx: crate::client::ClientContext,
    P: crate::auth::CredentialProvider<Cx>,
{
    state: crate::auth::CredentialProviderState<Cx, P>,
}

impl<Cx, P> GeneratedCredentialBinding<Cx, P>
where
    Cx: crate::client::ClientContext,
    P: crate::auth::CredentialProvider<Cx>,
{
    #[doc(hidden)]
    pub fn new(provider: P) -> Self {
        Self {
            state: crate::auth::CredentialProviderState::new(provider),
        }
    }

    #[doc(hidden)]
    pub fn new_result(
        id: crate::auth::CredentialId,
        provider: Result<P, crate::auth::AuthError>,
    ) -> Self {
        Self {
            state: crate::auth::CredentialProviderState::new_result(id, provider),
        }
    }

    #[doc(hidden)]
    pub fn secret_binding(
        &self,
        preparation: AuthPreparationMode,
        challenge: AuthChallengeMode,
    ) -> AuthProviderBinding<'_, Cx>
    where
        P::Credential: crate::auth::SecretCredential,
    {
        self.state.secret_binding(preparation, challenge)
    }

    #[doc(hidden)]
    pub fn basic_binding(
        &self,
        preparation: AuthPreparationMode,
        challenge: AuthChallengeMode,
    ) -> AuthProviderBinding<'_, Cx>
    where
        P: crate::auth::CredentialProvider<Cx, Credential = crate::auth::BasicCredential>,
    {
        self.state.basic_binding(preparation, challenge)
    }

    #[doc(hidden)]
    pub async fn set_manual(&self, value: P::Credential) -> Result<(), crate::auth::AuthError> {
        self.state.set_manual(value).await
    }

    #[doc(hidden)]
    pub async fn clear_manual(&self) -> Result<(), crate::auth::AuthError> {
        self.state.clear_manual().await
    }

    #[doc(hidden)]
    pub async fn has_value(&self) -> bool {
        self.state.has_value().await
    }
}

/// Obtain generated client-auth configuration for one short mutation.
#[doc(hidden)]
pub fn generated_auth_write<T>(
    lock: &std::sync::RwLock<T>,
) -> Result<std::sync::RwLockWriteGuard<'_, T>, crate::auth::AuthError> {
    crate::auth::write_auth_lock(lock, "generated auth configuration lock poisoned")
}

/// Install a generated rate-limit response policy without exposing the
/// concrete limiter implementation to generated source.
#[doc(hidden)]
pub fn generated_rate_limiter<P>(policy: P) -> std::sync::Arc<dyn crate::rate_limit::RateLimiter>
where
    P: crate::rate_limit::RateLimitResponsePolicy,
{
    std::sync::Arc::new(
        crate::rate_limit::GovernorRateLimiter::new()
            .with_response_policy(std::sync::Arc::new(policy)),
    )
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedOriginScheme {
    Http,
    Https,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedFixedOriginDescriptor {
    scheme: GeneratedOriginScheme,
    authority: &'static str,
}

impl GeneratedFixedOriginDescriptor {
    #[doc(hidden)]
    pub const fn new(scheme: GeneratedOriginScheme, authority: &'static str) -> Self {
        Self { scheme, authority }
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedApiOriginDescriptor {
    FixedSingleOrigin(GeneratedFixedOriginDescriptor),
    DynamicOrigin,
    MultiOrigin,
}

impl GeneratedApiOriginDescriptor {
    fn into_runtime(self) -> crate::retry_mode::ApiOriginDescriptor {
        match self {
            Self::FixedSingleOrigin(origin) => {
                crate::retry_mode::ApiOriginDescriptor::FixedSingleOrigin(
                    crate::retry_mode::FixedOriginDescriptor {
                        scheme: match origin.scheme {
                            GeneratedOriginScheme::Http => crate::retry_mode::OriginScheme::Http,
                            GeneratedOriginScheme::Https => crate::retry_mode::OriginScheme::Https,
                        },
                        authority: origin.authority,
                    },
                )
            }
            Self::DynamicOrigin => crate::retry_mode::ApiOriginDescriptor::DynamicOrigin,
            Self::MultiOrigin => crate::retry_mode::ApiOriginDescriptor::MultiOrigin,
        }
    }
}

/// Static origin relationship for one endpoint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointOriginDescriptor {
    Fixed(GeneratedFixedOriginDescriptor),
    Dynamic,
}

/// HTTP method identity stored without a runtime request value.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Options,
    Patch,
}

impl HttpMethod {
    /// Adapt descriptor metadata to the current runtime method type.
    #[doc(hidden)]
    pub fn as_http_method(self) -> http::Method {
        match self {
            Self::Get => http::Method::GET,
            Self::Post => http::Method::POST,
            Self::Put => http::Method::PUT,
            Self::Delete => http::Method::DELETE,
            Self::Head => http::Method::HEAD,
            Self::Options => http::Method::OPTIONS,
            Self::Patch => http::Method::PATCH,
        }
    }
}

/// Request body contract resolved by the macro.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestBodyDescriptor {
    None,
    Buffered { codec: &'static str },
    Streaming { media: &'static str },
    Multipart,
}

/// Static request metadata; it never contains a body instance.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestDescriptor {
    body: RequestBodyDescriptor,
}

impl RequestDescriptor {
    #[doc(hidden)]
    pub const fn new(body: RequestBodyDescriptor) -> Self {
        Self { body }
    }
}

/// Response contract resolved by the macro.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseFormatDescriptor {
    Buffered { codec: &'static str },
    Bytes,
    NoContent,
    Streaming { media: &'static str },
}

/// Static response metadata; response processing remains in the current
/// runtime pipeline.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseDescriptor {
    format: ResponseFormatDescriptor,
}

impl ResponseDescriptor {
    #[doc(hidden)]
    pub const fn new(format: ResponseFormatDescriptor) -> Self {
        Self { format }
    }
}

/// One secret-free authentication requirement identity.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthRequirementDescriptor {
    credential: &'static str,
    usage_id: &'static str,
}

impl AuthRequirementDescriptor {
    #[doc(hidden)]
    pub const fn new(credential: &'static str, usage_id: &'static str) -> Self {
        Self {
            credential,
            usage_id,
        }
    }
}

/// Static authentication metadata. Providers, credentials, and caches are
/// deliberately absent.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthDescriptor {
    requirements: &'static [AuthRequirementDescriptor],
}

impl AuthDescriptor {
    #[doc(hidden)]
    pub const fn new(requirements: &'static [AuthRequirementDescriptor]) -> Self {
        Self { requirements }
    }
}

/// Pagination facts known before runtime execution.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaginationDescriptor {
    can_change_origin: bool,
}

impl PaginationDescriptor {
    #[doc(hidden)]
    pub const fn new(can_change_origin: bool) -> Self {
        Self { can_change_origin }
    }
}

/// Static endpoint descriptor emitted once for every generated endpoint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedEndpointDescriptor {
    name: &'static str,
    api_name: &'static str,
    method: HttpMethod,
    origin: EndpointOriginDescriptor,
    request: RequestDescriptor,
    response: ResponseDescriptor,
    auth: AuthDescriptor,
    pagination: Option<PaginationDescriptor>,
}

impl GeneratedEndpointDescriptor {
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        name: &'static str,
        api_name: &'static str,
        method: HttpMethod,
        origin: EndpointOriginDescriptor,
        request: RequestDescriptor,
        response: ResponseDescriptor,
        auth: AuthDescriptor,
        pagination: Option<PaginationDescriptor>,
    ) -> Self {
        Self {
            name,
            api_name,
            method,
            origin,
            request,
            response,
            auth,
            pagination,
        }
    }
}

/// Static API descriptor emitted once for every generated API.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedApiDescriptor {
    name: &'static str,
    origin: GeneratedApiOriginDescriptor,
    endpoints: &'static [&'static GeneratedEndpointDescriptor],
}

impl GeneratedApiDescriptor {
    #[doc(hidden)]
    pub const fn new(
        name: &'static str,
        origin: GeneratedApiOriginDescriptor,
        endpoints: &'static [&'static GeneratedEndpointDescriptor],
    ) -> Self {
        Self {
            name,
            origin,
            endpoints,
        }
    }
}

/// Construct a generated client from the macro-emitted API descriptor.
///
/// `__private` is a cross-crate macro integration surface, not a Rust privacy
/// or security boundary. Manually calling this unsupported function is outside
/// the application API contract. Status eligibility is derived only from the
/// descriptor emitted for the generated wrapper; there is no reusable global
/// capability and no generic-client status constructor.
#[doc(hidden)]
pub fn create_generated_client<Cx, F>(
    descriptor: &'static GeneratedApiDescriptor,
    vars: Cx::Vars,
    auth_vars: Cx::AuthVars,
    retry_mode: crate::retry_mode::RetryMode,
    configure: F,
) -> Result<crate::client::ApiClient<Cx>, crate::retry_mode::RetryModeError>
where
    Cx: crate::client::ClientContext,
    F: FnOnce(
        crate::transport::SafeReqwestBuilder,
    ) -> Result<
        crate::transport::SafeReqwestBuilder,
        crate::transport::ReqwestClientBuildError,
    >,
{
    crate::client::ApiClient::<Cx>::with_generated_descriptor_retry_mode(
        Some(descriptor.origin.into_runtime()),
        vars,
        auth_vars,
        retry_mode,
        configure,
    )
}

/// Construct a generated client through its required-value builder.
#[doc(hidden)]
pub fn create_generated_client_for_builder<Cx>(
    descriptor: &'static GeneratedApiDescriptor,
    vars: Cx::Vars,
    auth_vars: Cx::AuthVars,
    ctx: crate::error::ErrorContext,
) -> Result<crate::client::ApiClient<Cx>, crate::error::ApiClientError>
where
    Cx: crate::client::ClientContext,
{
    crate::client::ApiClient::<Cx>::with_generated_descriptor_builder(
        Some(descriptor.origin.into_runtime()),
        vars,
        auth_vars,
        ctx,
    )
}

/// Construct a generated client through the safe managed Reqwest builder.
#[doc(hidden)]
pub fn create_generated_client_with_safe_reqwest_builder<Cx, F>(
    descriptor: &'static GeneratedApiDescriptor,
    vars: Cx::Vars,
    auth_vars: Cx::AuthVars,
    configure: F,
) -> Result<crate::client::ApiClient<Cx>, crate::transport::ReqwestClientBuildError>
where
    Cx: crate::client::ClientContext,
    F: FnOnce(
        crate::transport::SafeReqwestBuilder,
    ) -> Result<
        crate::transport::SafeReqwestBuilder,
        crate::transport::ReqwestClientBuildError,
    >,
{
    crate::client::ApiClient::<Cx>::with_generated_descriptor_safe_reqwest_builder_fallible(
        Some(descriptor.origin.into_runtime()),
        vars,
        auth_vars,
        configure,
    )
}

#[doc(hidden)]
pub struct PreparedEndpointRoute(crate::endpoint::ResolvedRoute);

#[doc(hidden)]
pub struct PreparedEndpointPolicy(crate::policy::ResolvedPolicy);

#[doc(hidden)]
pub struct GeneratedRequestBody(crate::io::PreparedBody);

/// Borrowed typed inputs used only while a generated endpoint is prepared.
#[doc(hidden)]
pub struct GeneratedPlanContext<'a, Cx: crate::client::ClientContext> {
    vars: &'a Cx::Vars,
    auth_vars: &'a Cx::AuthVars,
}

impl<'a, Cx: crate::client::ClientContext> GeneratedPlanContext<'a, Cx> {
    pub(crate) fn new(vars: &'a Cx::Vars, auth_vars: &'a Cx::AuthVars) -> Self {
        Self { vars, auth_vars }
    }

    #[doc(hidden)]
    pub fn vars(&self) -> &'a Cx::Vars {
        self.vars
    }

    #[doc(hidden)]
    pub fn auth_vars(&self) -> &'a Cx::AuthVars {
        self.auth_vars
    }
}

/// Resolved authentication placement emitted by the macro.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub enum GeneratedAuthPlacement {
    Bearer,
    Basic,
    Header(&'static str),
    Query(&'static str),
}

/// Opaque authentication descriptor builder. Generated code can add resolved
/// requirements but cannot construct or inspect Core's runtime auth plan.
#[doc(hidden)]
#[derive(Default)]
pub struct GeneratedAuthBuilder {
    requirements: Vec<crate::auth::AuthRequirement>,
}

#[doc(hidden)]
pub struct GeneratedRateLimitDescriptor(crate::rate_limit::RateLimitPlan);
#[doc(hidden)]
pub struct GeneratedRateLimitBucketDescriptor(crate::rate_limit::RateLimitBucketUse);
#[doc(hidden)]
pub struct GeneratedRateLimitKeyDescriptor(crate::rate_limit::RateLimitKey);
#[doc(hidden)]
pub struct GeneratedRateLimitKeyPartDescriptor(crate::rate_limit::RateLimitKeyPart);
#[doc(hidden)]
pub struct GeneratedRateLimitWindowDescriptor(crate::rate_limit::RateLimitWindow);

impl GeneratedRateLimitDescriptor {
    #[doc(hidden)]
    pub fn from_buckets(buckets: Vec<GeneratedRateLimitBucketDescriptor>) -> Self {
        Self(crate::rate_limit::RateLimitPlan::from_buckets(
            buckets.into_iter().map(|bucket| bucket.0).collect(),
        ))
    }

    pub(crate) fn into_plan(self) -> crate::rate_limit::RateLimitPlan {
        self.0
    }
}

impl GeneratedRateLimitBucketDescriptor {
    #[doc(hidden)]
    pub fn new(
        kind: &'static str,
        name: &'static str,
        key: GeneratedRateLimitKeyDescriptor,
    ) -> Self {
        Self(crate::rate_limit::RateLimitBucketUse::new(
            kind, name, key.0,
        ))
    }

    #[doc(hidden)]
    pub fn with_cost(mut self, cost: std::num::NonZeroU32) -> Self {
        self.0 = self.0.with_cost(cost);
        self
    }

    #[doc(hidden)]
    pub fn with_windows(mut self, windows: Vec<GeneratedRateLimitWindowDescriptor>) -> Self {
        self.0 = self
            .0
            .with_windows(windows.into_iter().map(|window| window.0).collect());
        self
    }
}

impl GeneratedRateLimitKeyDescriptor {
    #[doc(hidden)]
    pub fn new(parts: Vec<GeneratedRateLimitKeyPartDescriptor>) -> Self {
        Self(crate::rate_limit::RateLimitKey::new(
            parts.into_iter().map(|part| part.0).collect(),
        ))
    }
}

impl GeneratedRateLimitKeyPartDescriptor {
    #[doc(hidden)]
    pub fn static_value(
        name: &'static str,
        value: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Self {
        Self(crate::rate_limit::RateLimitKeyPart::static_value(
            name, value,
        ))
    }

    #[doc(hidden)]
    pub fn endpoint() -> Self {
        Self(crate::rate_limit::RateLimitKeyPart::endpoint())
    }

    #[doc(hidden)]
    pub fn method() -> Self {
        Self(crate::rate_limit::RateLimitKeyPart::method())
    }

    #[doc(hidden)]
    pub fn url_host() -> Self {
        Self(crate::rate_limit::RateLimitKeyPart::url_host())
    }
}

impl GeneratedRateLimitWindowDescriptor {
    #[doc(hidden)]
    pub fn new(max: std::num::NonZeroU32, per: std::time::Duration) -> Self {
        Self(crate::rate_limit::RateLimitWindow::new(max, per))
    }
}

impl GeneratedAuthBuilder {
    #[doc(hidden)]
    pub fn new() -> Self {
        Self::default()
    }

    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn require(
        &mut self,
        client_namespace: &'static str,
        credential: &'static str,
        placement: GeneratedAuthPlacement,
        usage_id: &'static str,
        step_id: &'static str,
        provenance: &'static str,
        challenge: crate::auth::AuthChallengePolicy,
    ) {
        let placement = match placement {
            GeneratedAuthPlacement::Bearer => crate::auth::AuthPlacement::Bearer,
            GeneratedAuthPlacement::Basic => crate::auth::AuthPlacement::Basic,
            GeneratedAuthPlacement::Header(name) => crate::auth::AuthPlacement::Header(name),
            GeneratedAuthPlacement::Query(name) => crate::auth::AuthPlacement::Query(name),
        };
        self.requirements.push(crate::auth::AuthRequirement {
            credential: crate::auth::CredentialRef {
                id: crate::auth::CredentialId::new(client_namespace, credential),
            },
            placement,
            usage_id: crate::auth::AuthUsageId::new(usage_id),
            step_id: Some(step_id),
            provenance: crate::auth::AuthProvenance::new(provenance),
            challenge,
        });
    }

    fn into_plan(self) -> crate::auth::AuthPlan {
        crate::auth::AuthPlan {
            requirements: self.requirements,
        }
    }
}

mod generated_request_sealed {
    pub trait Adapter {
        type Input;
        fn prepare(
            input: Self::Input,
            ctx: crate::error::ErrorContext,
        ) -> Result<crate::io::PreparedRequestEntity, crate::error::ApiClientError>;
    }
}

#[doc(hidden)]
pub struct GeneratedNoRequestBody;
#[doc(hidden)]
pub struct GeneratedEncodedRequest<C>(std::marker::PhantomData<fn() -> C>);
#[doc(hidden)]
pub struct GeneratedRawStreamRequest<M>(std::marker::PhantomData<fn() -> M>);
#[cfg(feature = "multipart")]
#[doc(hidden)]
pub struct GeneratedMultipartRequest;

impl generated_request_sealed::Adapter for GeneratedNoRequestBody {
    type Input = ();
    fn prepare(
        input: Self::Input,
        ctx: crate::error::ErrorContext,
    ) -> Result<crate::io::PreparedRequestEntity, crate::error::ApiClientError> {
        <crate::io::NoRequestBody as crate::io::RequestEntity>::prepare(input, ctx)
    }
}

impl<C> generated_request_sealed::Adapter for GeneratedEncodedRequest<C>
where
    C: crate::codec::BodyCodec,
{
    type Input = C::Value;
    fn prepare(
        input: Self::Input,
        ctx: crate::error::ErrorContext,
    ) -> Result<crate::io::PreparedRequestEntity, crate::error::ApiClientError> {
        <crate::io::EncodedRequest<C> as crate::io::RequestEntity>::prepare(input, ctx)
    }
}

impl<M> generated_request_sealed::Adapter for GeneratedRawStreamRequest<M>
where
    M: crate::codec::ContentType,
{
    type Input = crate::stream_body::StreamBody;
    fn prepare(
        input: Self::Input,
        ctx: crate::error::ErrorContext,
    ) -> Result<crate::io::PreparedRequestEntity, crate::error::ApiClientError> {
        <crate::io::RawStreamRequest<M> as crate::io::RequestEntity>::prepare(input, ctx)
    }
}

#[cfg(feature = "multipart")]
impl generated_request_sealed::Adapter for GeneratedMultipartRequest {
    type Input = crate::multipart::MultipartBody;
    fn prepare(
        input: Self::Input,
        ctx: crate::error::ErrorContext,
    ) -> Result<crate::io::PreparedRequestEntity, crate::error::ApiClientError> {
        <crate::io::MultipartRequest as crate::io::RequestEntity>::prepare(input, ctx)
    }
}

#[doc(hidden)]
pub fn prepare_generated_request_body<A>(
    input: <A as generated_request_sealed::Adapter>::Input,
    ctx: crate::error::ErrorContext,
) -> Result<GeneratedRequestBody, crate::error::ApiClientError>
where
    A: generated_request_sealed::Adapter,
{
    Ok(GeneratedRequestBody(A::prepare(input, ctx)?.body))
}

type GeneratedExecute<Cx, Output> = for<'a> fn(
    &'a crate::client::ApiClient<Cx>,
    crate::endpoint::RequestPlan,
) -> crate::endpoint::EndpointFuture<'a, Output>;

type GeneratedExecuteWithMeta<Cx, Output> =
    for<'a> fn(
        &'a crate::client::ApiClient<Cx>,
        crate::endpoint::RequestPlan,
    )
        -> crate::endpoint::EndpointFuture<'a, crate::transport::DecodedResponse<Output>>;

#[doc(hidden)]
pub struct GeneratedPreparedCall<Cx, Output>
where
    Cx: crate::client::ClientContext,
{
    plan: crate::endpoint::RequestPlan,
    execute: GeneratedExecute<Cx, Output>,
    execute_with_meta: Option<GeneratedExecuteWithMeta<Cx, Output>>,
}

#[cfg(test)]
pub(crate) fn prepared_call_for_core_regression<Cx, Output>(
    plan: crate::endpoint::RequestPlan,
    execute: GeneratedExecute<Cx, Output>,
) -> GeneratedPreparedCall<Cx, Output>
where
    Cx: crate::client::ClientContext,
{
    GeneratedPreparedCall {
        plan,
        execute,
        execute_with_meta: None,
    }
}

impl<Cx, Output> GeneratedPreparedCall<Cx, Output>
where
    Cx: crate::client::ClientContext,
{
    pub(crate) fn plan(&self) -> &crate::endpoint::RequestPlan {
        &self.plan
    }

    pub(crate) fn plan_mut(&mut self) -> &mut crate::endpoint::RequestPlan {
        &mut self.plan
    }

    #[allow(dead_code)]
    pub(crate) fn into_plan(self) -> crate::endpoint::RequestPlan {
        self.plan
    }

    #[doc(hidden)]
    pub fn execute<'a>(
        self,
        client: &'a crate::client::ApiClient<Cx>,
    ) -> crate::endpoint::EndpointFuture<'a, Output> {
        (self.execute)(client, self.plan)
    }

    #[doc(hidden)]
    pub fn execute_with_meta<'a>(
        self,
        client: &'a crate::client::ApiClient<Cx>,
    ) -> crate::endpoint::EndpointFuture<'a, crate::transport::DecodedResponse<Output>> {
        match self.execute_with_meta {
            Some(execute) => execute(client, self.plan),
            None => Box::pin(async move {
                Err(crate::error::ApiClientError::invalid_param(
                    crate::error::ErrorContext {
                        endpoint: self.plan.endpoint.meta.name,
                        method: self.plan.endpoint.meta.method.clone(),
                    },
                    "response_terminal",
                ))
            }),
        }
    }
}

mod generated_response_sealed {
    pub trait Adapter<Cx: crate::client::ClientContext> {
        type Output;
        fn plan(
            ctx: crate::error::ErrorContext,
        ) -> Result<crate::io::ResponseEntityPlan, crate::error::ApiClientError>;
        fn execute<'a>(
            client: &'a crate::client::ApiClient<Cx>,
            plan: crate::endpoint::RequestPlan,
        ) -> crate::endpoint::EndpointFuture<'a, Self::Output>;
        fn execute_with_meta() -> Option<super::GeneratedExecuteWithMeta<Cx, Self::Output>> {
            None
        }
    }
}

#[doc(hidden)]
pub struct GeneratedBufferedResponse<C>(std::marker::PhantomData<fn() -> C>);
#[doc(hidden)]
pub struct GeneratedBytesResponse;
#[doc(hidden)]
pub struct GeneratedNoContentResponse;
#[doc(hidden)]
pub struct GeneratedRawStreamResponse<M>(std::marker::PhantomData<fn() -> M>);

macro_rules! impl_generated_buffered_response {
    ($marker:ty, $runtime:ty) => {
        impl<Cx> generated_response_sealed::Adapter<Cx> for $marker
        where
            Cx: crate::client::ClientContext,
            $runtime: crate::io::ResponseEntity,
        {
            type Output = <$runtime as crate::io::ResponseEntity>::Output;
            fn plan(
                ctx: crate::error::ErrorContext,
            ) -> Result<crate::io::ResponseEntityPlan, crate::error::ApiClientError> {
                <$runtime as crate::io::ResponseEntity>::plan(ctx)
            }
            fn execute<'a>(
                client: &'a crate::client::ApiClient<Cx>,
                plan: crate::endpoint::RequestPlan,
            ) -> crate::endpoint::EndpointFuture<'a, Self::Output> {
                <$runtime as crate::io::ResponseEntity>::execute(client, plan)
            }
            fn execute_with_meta() -> Option<GeneratedExecuteWithMeta<Cx, Self::Output>> {
                Some(<$runtime as crate::io::ResponseEntityWithMeta>::execute_with_meta::<Cx>)
            }
        }
    };
}

impl_generated_buffered_response!(GeneratedBytesResponse, crate::io::BytesResponse);
impl_generated_buffered_response!(GeneratedNoContentResponse, crate::io::NoContentResponse);

impl<Cx, C> generated_response_sealed::Adapter<Cx> for GeneratedBufferedResponse<C>
where
    Cx: crate::client::ClientContext,
    C: crate::codec::ResponseCodec,
{
    type Output = C::Value;
    fn plan(
        ctx: crate::error::ErrorContext,
    ) -> Result<crate::io::ResponseEntityPlan, crate::error::ApiClientError> {
        <crate::io::BufferedResponse<C> as crate::io::ResponseEntity>::plan(ctx)
    }
    fn execute<'a>(
        client: &'a crate::client::ApiClient<Cx>,
        plan: crate::endpoint::RequestPlan,
    ) -> crate::endpoint::EndpointFuture<'a, Self::Output> {
        <crate::io::BufferedResponse<C> as crate::io::ResponseEntity>::execute(client, plan)
    }
    fn execute_with_meta() -> Option<GeneratedExecuteWithMeta<Cx, Self::Output>> {
        Some(<crate::io::BufferedResponse<C> as crate::io::ResponseEntityWithMeta>::execute_with_meta::<Cx>)
    }
}

impl<Cx, M> generated_response_sealed::Adapter<Cx> for GeneratedRawStreamResponse<M>
where
    Cx: crate::client::ClientContext,
    M: crate::codec::ContentType,
{
    type Output = crate::stream_response::StreamResponse<M>;
    fn plan(
        ctx: crate::error::ErrorContext,
    ) -> Result<crate::io::ResponseEntityPlan, crate::error::ApiClientError> {
        <crate::io::RawStreamResponse<M> as crate::io::ResponseEntity>::plan(ctx)
    }
    fn execute<'a>(
        client: &'a crate::client::ApiClient<Cx>,
        plan: crate::endpoint::RequestPlan,
    ) -> crate::endpoint::EndpointFuture<'a, Self::Output> {
        <crate::io::RawStreamResponse<M> as crate::io::ResponseEntity>::execute(client, plan)
    }
}

#[doc(hidden)]
pub struct GeneratedResponsePreparation<Cx, Output>
where
    Cx: crate::client::ClientContext,
{
    plan: crate::endpoint::ResponsePlan,
    execute: GeneratedExecute<Cx, Output>,
    execute_with_meta: Option<GeneratedExecuteWithMeta<Cx, Output>>,
}

impl<Cx, Output> GeneratedResponsePreparation<Cx, Output>
where
    Cx: crate::client::ClientContext,
{
    #[doc(hidden)]
    pub fn accept(&self) -> Option<&http::HeaderValue> {
        self.plan.accept.as_ref()
    }

    #[doc(hidden)]
    pub fn is_no_content(&self) -> bool {
        self.plan.no_content
    }
}

#[doc(hidden)]
pub fn prepare_generated_response<Cx, A>(
    ctx: crate::error::ErrorContext,
) -> Result<
    GeneratedResponsePreparation<Cx, <A as generated_response_sealed::Adapter<Cx>>::Output>,
    crate::error::ApiClientError,
>
where
    Cx: crate::client::ClientContext,
    A: generated_response_sealed::Adapter<Cx>,
{
    let prepared = A::plan(ctx)?;
    Ok(GeneratedResponsePreparation {
        plan: prepared.response_plan,
        execute: A::execute,
        execute_with_meta: A::execute_with_meta(),
    })
}

#[doc(hidden)]
pub fn prepare_generated_route(
    scheme: http::uri::Scheme,
    host: String,
    path: String,
) -> PreparedEndpointRoute {
    PreparedEndpointRoute(crate::endpoint::ResolvedRoute::new(scheme, host, path))
}

#[doc(hidden)]
pub fn prepare_generated_policy(
    policy: crate::policy::ClientPolicyBuilder,
    auth: GeneratedAuthBuilder,
) -> PreparedEndpointPolicy {
    let (headers, query, timeout, mut rate_limit) = policy.into_inner().into_parts();
    rate_limit.canonicalize();
    PreparedEndpointPolicy(crate::policy::ResolvedPolicy {
        headers,
        query,
        timeout,
        auth: auth.into_plan(),
        rate_limit,
    })
}

#[doc(hidden)]
/// Core-owned endpoint preparation entry point used by generated adapters.
/// Generated code supplies typed, already-materialized inputs; construction
/// of the executable request plan remains in Core.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn prepare_generated_endpoint<Cx, Output>(
    name: &'static str,
    method: http::Method,
    idempotent: bool,
    facade_path: &'static [&'static str],
    route: PreparedEndpointRoute,
    policy: PreparedEndpointPolicy,
    response: GeneratedResponsePreparation<Cx, Output>,
    body: GeneratedRequestBody,
    pagination: bool,
) -> Result<GeneratedPreparedCall<Cx, Output>, crate::error::ApiClientError>
where
    Cx: crate::client::ClientContext,
{
    let plan = crate::endpoint::RequestPlan {
        endpoint: crate::endpoint::EndpointPlan {
            meta: crate::endpoint::EndpointMeta {
                name,
                method,
                idempotent,
                facade_path,
            },
            route: route.0,
            policy: policy.0,
            response: response.plan,
            pagination: pagination.then_some(crate::endpoint::PaginationMarker),
        },
        body: body.0,
        overrides: crate::endpoint::RequestOverrides::default(),
    };
    Ok(GeneratedPreparedCall {
        plan,
        execute: response.execute,
        execute_with_meta: response.execute_with_meta,
    })
}

#[cfg(feature = "json")]
#[doc(hidden)]
pub use crate::auth::OAuth2ClientCredentialsProvider;
#[doc(hidden)]
pub use crate::auth::{
    AuthChallengePolicy as GeneratedChallengePolicy,
    ManualCredentialProvider as GeneratedManualCredentialProvider,
    NoAuthState as GeneratedNoAuthState, StaticApiKeyProvider as GeneratedStaticApiKeyProvider,
    StaticBasicProvider as GeneratedStaticBasicProvider,
    StaticBearerProvider as GeneratedStaticBearerProvider,
};
#[doc(hidden)]
pub use crate::codec::{
    BodyCodec, CodecError, ContentType, DecodeContext, Decodes, EncodeContext, EncodedBody,
    Encodes, Format, FormatType, ResponseCodec,
};
#[doc(hidden)]
pub use crate::endpoint::{
    GeneratedEndpoint, GeneratedIntoPreparedCall, GeneratedPaginatedEndpoint,
    GeneratedResponseTerminalEndpoint, GeneratedReusableEndpoint,
    PaginationMarker as GeneratedPaginationMarker,
};
#[doc(hidden)]
pub use crate::error::ErrorContext;
#[doc(hidden)]
pub use crate::pagination::{
    Control as GeneratedPageControl, CursorPagination as GeneratedCursorPagination,
    EndpointPagination as GeneratedEndpointPagination, HasNextCursor as GeneratedHasNextCursor,
    OffsetLimitPagination as GeneratedOffsetPagination, PageAdvance as GeneratedPageAdvance,
    PageApply as GeneratedPageApply, PageDecision as GeneratedPageDecision,
    PageItems as GeneratedPageItems, PagedPagination as GeneratedPagedPagination,
    PaginateBinding as GeneratedPaginateBinding, PaginationCaps as GeneratedPaginationCaps,
    PaginationTermination as GeneratedPaginationTermination, ProgressKey as GeneratedProgressKey,
};
#[doc(hidden)]
#[doc(hidden)]
pub use crate::types::HostLabelSource;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const FIXED_AUTHORITY: &str = "fixed-tls-secret-sentinel.example.test";
    #[cfg(not(feature = "default-tls"))]
    const FIXED_HTTPS_URL: &str =
        "https://fixed-tls-secret-sentinel.example.test/?query=QUERY_SECRET_SENTINEL";
    #[cfg(not(feature = "default-tls"))]
    const QUERY_MATERIAL: &str = "query=QUERY_SECRET_SENTINEL";
    #[cfg(not(feature = "default-tls"))]
    const PROXY_TARGET: &str = "http://proxy-secret-sentinel.example.test";
    #[cfg(not(feature = "default-tls"))]
    const AUTH_MATERIAL: &str = "Bearer AUTH_SECRET_SENTINEL";

    struct ConstructionCx;

    impl crate::client::ClientContext for ConstructionCx {
        type Vars = ();
        type AuthVars = Arc<AtomicUsize>;
        type AuthState = ();
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
        const DOMAIN: &'static str = "runtime.invalid";

        fn init_auth_state(_: &Self::Vars, calls: &Self::AuthVars) -> Self::AuthState {
            calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[cfg(not(feature = "default-tls"))]
    #[derive(Clone)]
    struct RetryConstructionAuthVars {
        initializations: Arc<AtomicUsize>,
        query_material: String,
        auth_material: String,
    }

    #[cfg(not(feature = "default-tls"))]
    struct RetryConstructionCx;

    #[cfg(not(feature = "default-tls"))]
    impl crate::client::ClientContext for RetryConstructionCx {
        type Vars = ();
        type AuthVars = RetryConstructionAuthVars;
        type AuthState = ();
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = FIXED_AUTHORITY;

        fn init_auth_state(_: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
            let _ = (&auth.query_material, &auth.auth_material);
            auth.initializations.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[cfg(not(feature = "default-tls"))]
    static FIXED_HTTP_DESCRIPTOR: GeneratedApiDescriptor = GeneratedApiDescriptor::new(
        "FixedHttpConstruction",
        GeneratedApiOriginDescriptor::FixedSingleOrigin(GeneratedFixedOriginDescriptor::new(
            GeneratedOriginScheme::Http,
            "http.example.test",
        )),
        &[],
    );

    static FIXED_SECRET_HTTPS_DESCRIPTOR: GeneratedApiDescriptor = GeneratedApiDescriptor::new(
        "FixedHttpsConstruction",
        GeneratedApiOriginDescriptor::FixedSingleOrigin(GeneratedFixedOriginDescriptor::new(
            GeneratedOriginScheme::Https,
            FIXED_AUTHORITY,
        )),
        &[],
    );

    #[cfg(not(feature = "default-tls"))]
    static DYNAMIC_DESCRIPTOR: GeneratedApiDescriptor = GeneratedApiDescriptor::new(
        "DynamicConstruction",
        GeneratedApiOriginDescriptor::DynamicOrigin,
        &[],
    );

    fn construction_context() -> crate::error::ErrorContext {
        crate::error::ErrorContext {
            endpoint: "ConstructionClient::builder",
            method: http::Method::GET,
        }
    }

    #[cfg(not(feature = "default-tls"))]
    fn retry_auth_vars(initializations: Arc<AtomicUsize>) -> RetryConstructionAuthVars {
        RetryConstructionAuthVars {
            initializations,
            query_material: QUERY_MATERIAL.to_string(),
            auth_material: AUTH_MATERIAL.to_string(),
        }
    }

    #[cfg(not(feature = "default-tls"))]
    fn error_chain_diagnostics(error: &(dyn std::error::Error + 'static)) -> String {
        let mut diagnostics = format!("{error}\n{error:?}");
        let mut source = error.source();
        while let Some(current) = source {
            diagnostics.push_str(&format!("\n{current}\n{current:?}"));
            source = current.source();
        }
        diagnostics
    }

    #[cfg(not(feature = "default-tls"))]
    fn assert_tls_diagnostics_sanitized(error: &(dyn std::error::Error + 'static)) {
        let diagnostics = error_chain_diagnostics(error);
        assert!(diagnostics.contains("HTTPS requires an available TLS capability"));
        for secret in [
            FIXED_AUTHORITY,
            FIXED_HTTPS_URL,
            "fixed-tls-secret-sentinel",
            "FIXED_TLS_SECRET_SENTINEL",
            QUERY_MATERIAL,
            "QUERY_SECRET_SENTINEL",
            PROXY_TARGET,
            "proxy-secret-sentinel",
            AUTH_MATERIAL,
            "AUTH_SECRET_SENTINEL",
        ] {
            assert!(
                !diagnostics.contains(secret),
                "leaked {secret}: {diagnostics}"
            );
        }
    }

    #[cfg(not(feature = "default-tls"))]
    fn retry_construction_error(
        retry_mode: crate::retry_mode::RetryMode,
        initializations: Arc<AtomicUsize>,
        configure: impl FnOnce(
            crate::transport::SafeReqwestBuilder,
        ) -> Result<
            crate::transport::SafeReqwestBuilder,
            crate::transport::ReqwestClientBuildError,
        >,
    ) -> crate::retry_mode::RetryModeError {
        match create_generated_client::<RetryConstructionCx, _>(
            &FIXED_SECRET_HTTPS_DESCRIPTOR,
            (),
            retry_auth_vars(initializations),
            retry_mode,
            configure,
        ) {
            Ok(_) => panic!("fixed HTTPS retry-mode construction must fail without TLS"),
            Err(error) => error,
        }
    }

    #[test]
    fn descriptor_method_adapter_is_metadata_only() {
        assert_eq!(HttpMethod::Get.as_http_method(), http::Method::GET);
        const _: () = assert_generated_contract(GENERATED_CONTRACT);
    }

    #[test]
    fn generated_surface_contains_no_auth_engine_or_mutable_cache_implementation() {
        let source = include_str!("mod.rs");
        for forbidden in [
            concat!("Credential", "SlotState"),
            concat!("get_or_", "refresh"),
            concat!("invalidate_", "generation"),
            concat!("AuthHttp", "Executor"),
            concat!("req", "west::"),
            concat!("Trans", "port"),
            concat!("Dyn", "Body"),
        ] {
            assert!(
                !source.contains(forbidden),
                "__private contained {forbidden}"
            );
        }
    }

    #[cfg(not(feature = "default-tls"))]
    #[test]
    fn no_tls_fixed_http_and_dynamic_generated_construction_remain_available() {
        for descriptor in [&FIXED_HTTP_DESCRIPTOR, &DYNAMIC_DESCRIPTOR] {
            let auth_initializations = Arc::new(AtomicUsize::new(0));
            create_generated_client_for_builder::<ConstructionCx>(
                descriptor,
                (),
                auth_initializations.clone(),
                construction_context(),
            )
            .expect("HTTP-fixed and dynamic clients do not require TLS at construction");
            assert_eq!(auth_initializations.load(Ordering::SeqCst), 1);
        }
    }

    #[cfg(not(feature = "default-tls"))]
    #[test]
    fn no_tls_fixed_https_generated_builder_fails_before_auth_state_initialization() {
        let auth_initializations = Arc::new(AtomicUsize::new(0));
        let error = match create_generated_client_for_builder::<ConstructionCx>(
            &FIXED_SECRET_HTTPS_DESCRIPTOR,
            (),
            auth_initializations.clone(),
            construction_context(),
        ) {
            Ok(_) => panic!("fixed HTTPS must fail during fallible generated construction"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            crate::error::ApiClientError::TlsCapabilityUnavailable { .. }
        ));
        assert_eq!(auth_initializations.load(Ordering::SeqCst), 0);
        assert_eq!(error.category(), crate::error::ErrorCategory::Config);
        assert!(std::error::Error::source(&error).is_none());

        assert_tls_diagnostics_sanitized(&error);
    }

    #[cfg(not(feature = "default-tls"))]
    #[test]
    fn no_tls_fixed_https_safe_reqwest_construction_has_sanitized_source_chain() {
        let auth_initializations = Arc::new(AtomicUsize::new(0));
        let error = match create_generated_client_with_safe_reqwest_builder::<ConstructionCx, _>(
            &FIXED_SECRET_HTTPS_DESCRIPTOR,
            (),
            auth_initializations.clone(),
            Ok,
        ) {
            Ok(_) => panic!("fixed HTTPS safe construction must reject unavailable TLS"),
            Err(error) => error,
        };

        assert_eq!(auth_initializations.load(Ordering::SeqCst), 0);
        assert!(std::error::Error::source(&error).is_some());
        assert_tls_diagnostics_sanitized(&error);
    }

    #[cfg(not(feature = "default-tls"))]
    #[test]
    fn no_tls_fixed_https_protocol_recovery_constructor_fails_before_auth_state_initialization() {
        let initializations = Arc::new(AtomicUsize::new(0));
        let error = retry_construction_error(
            crate::retry_mode::RetryMode::ProtocolRecovery,
            initializations.clone(),
            Ok,
        );

        assert!(matches!(error, crate::retry_mode::RetryModeError::Build(_)));
        assert_eq!(initializations.load(Ordering::SeqCst), 0);
        assert_tls_diagnostics_sanitized(&error);
    }

    #[cfg(not(feature = "default-tls"))]
    #[test]
    fn no_tls_fixed_https_disabled_constructor_fails_before_auth_state_initialization() {
        let initializations = Arc::new(AtomicUsize::new(0));
        let error = retry_construction_error(
            crate::retry_mode::RetryMode::Disabled,
            initializations.clone(),
            Ok,
        );

        assert!(matches!(error, crate::retry_mode::RetryModeError::Build(_)));
        assert_eq!(initializations.load(Ordering::SeqCst), 0);
        assert_tls_diagnostics_sanitized(&error);
    }

    #[cfg(not(feature = "default-tls"))]
    #[test]
    fn no_tls_fixed_https_safe_builder_retry_constructor_is_fully_sanitized() {
        let initializations = Arc::new(AtomicUsize::new(0));
        let proxy = crate::transport::SafeProxy::all(PROXY_TARGET).expect("safe HTTP proxy");
        let error = retry_construction_error(
            crate::retry_mode::RetryMode::Disabled,
            initializations.clone(),
            move |builder| Ok(builder.proxy(proxy)),
        );

        assert!(matches!(error, crate::retry_mode::RetryModeError::Build(_)));
        assert_eq!(initializations.load(Ordering::SeqCst), 0);
        assert_tls_diagnostics_sanitized(&error);
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn tls_enabled_fixed_https_generated_builder_construction_succeeds() {
        let auth_initializations = Arc::new(AtomicUsize::new(0));
        create_generated_client_for_builder::<ConstructionCx>(
            &FIXED_SECRET_HTTPS_DESCRIPTOR,
            (),
            auth_initializations.clone(),
            construction_context(),
        )
        .expect("compiled TLS permits fixed HTTPS construction without network I/O");
        assert_eq!(auth_initializations.load(Ordering::SeqCst), 1);
    }
}
