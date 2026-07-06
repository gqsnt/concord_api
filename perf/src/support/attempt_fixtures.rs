use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthDecision, AuthError, AuthErrorKind,
    AuthPlacement, AuthProvenance, AuthRequirement, AuthUsageId, NoopDebugSink, NoopRateLimiter,
    PreparedAuthCredential, RateLimitContext, RateLimitFuture, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimiter, Transport,
};
use concord_core::auth::{ApiKey, CredentialId, CredentialRef, apply_secret_credential};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, Replayability, RequestArgs, RequestOverrides,
    RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ClientContext, DebugLevel};
use http::{HeaderValue, Method, StatusCode};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct PerfAuthVars {
    pub token: Option<String>,
}

#[derive(Clone)]
pub struct PerfCx;

impl ClientContext for PerfCx {
    type Vars = ();
    type AuthVars = PerfAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a concord_core::advanced::RequestMeta,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<PreparedAuthCredential, AuthError>,
    > {
        Box::pin(async move {
            let token = auth.token.as_deref().ok_or_else(|| {
                AuthError::new(AuthErrorKind::MissingCredential, "missing perf auth token")
            })?;
            let application = match requirement.placement {
                AuthPlacement::Bearer | AuthPlacement::Header(_) | AuthPlacement::Query(_) => {
                    let material = ApiKey::new(token.to_string());
                    apply_secret_credential(request, requirement, &material)?
                }
                AuthPlacement::Basic | AuthPlacement::Certificate => {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "perf context only uses bearer/header/query auth",
                    ));
                }
            };
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        _requirement: &'a AuthRequirement,
        _applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a concord_core::advanced::RequestMeta,
        _status: StatusCode,
        _headers: &'a http::HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async move { Ok(AuthDecision::Continue) })
    }
}

#[derive(Clone, Default)]
pub struct CountingRateLimiter {
    acquire_calls: Arc<AtomicUsize>,
    response_calls: Arc<AtomicUsize>,
}

impl CountingRateLimiter {
    pub fn acquire_calls(&self) -> usize {
        self.acquire_calls.load(Ordering::Relaxed)
    }

    pub fn response_calls(&self) -> usize {
        self.response_calls.load(Ordering::Relaxed)
    }
}

impl fmt::Debug for CountingRateLimiter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CountingRateLimiter")
            .field("acquire_calls", &self.acquire_calls())
            .field("response_calls", &self.response_calls())
            .finish()
    }
}

impl RateLimiter for CountingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, concord_core::prelude::ApiClientError>> {
        let acquire_calls = self.acquire_calls.clone();
        Box::pin(async move {
            acquire_calls.fetch_add(1, Ordering::Relaxed);
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, concord_core::prelude::ApiClientError>>
    {
        let response_calls = self.response_calls.clone();
        Box::pin(async move {
            response_calls.fetch_add(1, Ordering::Relaxed);
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

pub fn client<T: Transport>(transport: T) -> ApiClient<PerfCx, T> {
    configured_client(transport, DebugLevel::None, Arc::new(NoopRateLimiter::new()))
}

pub fn configured_client<T: Transport>(
    transport: T,
    debug_level: DebugLevel,
    rate_limiter: Arc<dyn RateLimiter>,
) -> ApiClient<PerfCx, T> {
    let mut client = ApiClient::with_transport(
        (),
        PerfAuthVars {
            token: Some("BENCH_FAKE_TOKEN".to_string()),
        },
        transport,
    );
    client.configure(|cfg| {
        cfg.debug_sink(Arc::new(NoopDebugSink));
        cfg.debug_level(debug_level);
        cfg.rate_limiter(rate_limiter);
    });
    client
}

pub fn auth_requirement(placement: AuthPlacement, label: &'static str) -> AuthRequirement {
    AuthRequirement {
        credential: CredentialRef {
            id: CredentialId::new("bench", label),
        },
        placement,
        usage_id: AuthUsageId::new(label),
        step_id: Some("bench"),
        provenance: AuthProvenance::new("perf"),
        challenge: Default::default(),
    }
}

pub fn request_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: BodyPlan,
    args: RequestArgs,
    replayability: Replayability,
) -> RequestPlan {
    let idempotent = method == Method::GET || method == Method::HEAD;
    let meta_method = method.clone();
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: meta_method,
                idempotent,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        args,
        overrides: RequestOverrides::default(),
        replayability,
    }
}
