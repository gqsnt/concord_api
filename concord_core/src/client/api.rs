// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

#[derive(Clone)]
pub struct ApiClient<Cx: ClientContext, T: Transport + Clone = DefaultTransport> {
    pub(super) transport: T,
    pub(super) vars: Cx::Vars,
    pub(super) auth_vars: Cx::AuthVars,
    pub(super) auth_state: Arc<RwLock<Arc<Cx::AuthState>>>,
    pub(super) debug_level: DebugLevel,
    pub(super) pagination_detect_loops: bool,
    pub(super) debug_sink: Arc<dyn DebugSink>,
    pub(super) runtime_state: Arc<ClientRuntimeState>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::NoAuthState;
    use std::panic::AssertUnwindSafe;

    #[derive(Clone)]
    struct PoisonTransport;

    impl Transport for PoisonTransport {
        fn send(
            &self,
            _req: crate::transport::TransportRequest,
        ) -> ::std::pin::Pin<
            Box<
                dyn ::std::future::Future<
                        Output = Result<
                            crate::transport::TransportResponse,
                            crate::transport::TransportError,
                        >,
                    > + Send,
            >,
        > {
            Box::pin(async move {
                Err(crate::transport::TransportError::with_kind(
                    crate::transport::TransportErrorKind::Request,
                    std::io::Error::other("poison transport should not be used"),
                ))
            })
        }
    }

    #[derive(Clone)]
    struct PoisonCx;

    impl ClientContext for PoisonCx {
        type Vars = ();
        type AuthVars = ();
        type AuthState = NoAuthState;
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.com";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
            NoAuthState
        }
    }

    #[test]
    fn poisoned_auth_state_lock_returns_typed_error() {
        let client: ApiClient<PoisonCx, PoisonTransport> =
            ApiClient::with_transport((), (), PoisonTransport);
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = client
                .auth_state
                .write()
                .expect("test auth_state lock should be available");
            panic!("poison auth state lock");
        }));

        let err = match client.try_auth_state() {
            Ok(_) => panic!("poisoned auth state should return typed auth error"),
            Err(err) => err,
        };
        assert_eq!(err.kind, crate::auth::AuthErrorKind::StateUnavailable);
        assert!(err.to_string().contains("auth state lock poisoned"));
    }
}
#[cfg(feature = "transport-reqwest")]
impl<Cx: ClientContext> ApiClient<Cx, DefaultTransport>
where
    DefaultTransport: DefaultTransportMarker,
{
    pub fn new(vars: Cx::Vars, auth_vars: Cx::AuthVars) -> Self {
        Self::with_reqwest_client(vars, auth_vars, default_reqwest_client())
    }
    pub fn with_reqwest_client(
        vars: Cx::Vars,
        auth_vars: Cx::AuthVars,
        client: reqwest::Client,
    ) -> Self {
        Self::with_transport(vars, auth_vars, ReqwestTransport::new(client))
    }
}

#[cfg(feature = "transport-reqwest")]
fn default_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest default client should build with redirects disabled")
}

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub fn with_transport(vars: Cx::Vars, auth_vars: Cx::AuthVars, transport: T) -> Self {
        let auth_state = Cx::init_auth_state(&vars, &auth_vars);
        Self {
            transport,
            vars,
            auth_vars,
            auth_state: Arc::new(RwLock::new(Arc::new(auth_state))),
            debug_level: DebugLevel::default(),
            pagination_detect_loops: true,
            debug_sink: Arc::new(StderrDebugSink),
            runtime_state: Arc::new(ClientRuntimeState::default()),
        }
    }

    #[inline]
    pub fn vars(&self) -> &Cx::Vars {
        &self.vars
    }

    #[inline]
    pub fn vars_mut(&mut self) -> &mut Cx::Vars {
        &mut self.vars
    }

    #[inline]
    pub fn set_vars(&mut self, vars: Cx::Vars) {
        self.vars = vars;
    }

    #[inline]
    pub fn update_vars(&mut self, f: impl FnOnce(&mut Cx::Vars)) {
        f(&mut self.vars);
    }

    #[inline]
    pub fn auth_vars(&self) -> &Cx::AuthVars {
        &self.auth_vars
    }
    #[inline]
    pub fn auth_vars_mut(&mut self) -> &mut Cx::AuthVars {
        &mut self.auth_vars
    }
    #[inline]
    pub fn set_auth_vars(&mut self, auth_vars: Cx::AuthVars) {
        self.auth_vars = auth_vars;
    }
    #[inline]
    pub fn update_auth_vars(&mut self, f: impl FnOnce(&mut Cx::AuthVars)) {
        f(&mut self.auth_vars);
    }

    #[inline]
    pub fn auth_state(&self) -> Arc<Cx::AuthState> {
        match self.auth_state.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    #[inline]
    pub fn try_auth_state(&self) -> Result<Arc<Cx::AuthState>, crate::auth::AuthError> {
        self.auth_state
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| crate::auth::AuthError::state_unavailable("auth state lock poisoned"))
    }

    #[inline]
    pub fn set_auth_state(&mut self, auth_state: Cx::AuthState) {
        match self.auth_state.write() {
            Ok(mut guard) => *guard = Arc::new(auth_state),
            Err(poisoned) => *poisoned.into_inner() = Arc::new(auth_state),
        }
    }

    #[inline]
    pub fn try_set_auth_state(
        &mut self,
        auth_state: Cx::AuthState,
    ) -> Result<(), crate::auth::AuthError> {
        *self
            .auth_state
            .write()
            .map_err(|_| crate::auth::AuthError::state_unavailable("auth state lock poisoned"))? =
            Arc::new(auth_state);
        Ok(())
    }

    #[inline]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    #[inline]
    pub fn debug_level(&self) -> DebugLevel {
        self.debug_level
    }

    #[inline]
    pub fn set_debug_level(&mut self, level: DebugLevel) {
        self.debug_level = level;
    }

    #[inline]
    pub fn debug_sink(&self) -> &Arc<dyn DebugSink> {
        &self.debug_sink
    }

    #[inline]
    pub fn set_debug_sink(&mut self, sink: Arc<dyn DebugSink>) {
        self.debug_sink = sink;
    }

    #[inline]
    pub fn with_debug_sink(mut self, sink: Arc<dyn DebugSink>) -> Self {
        self.debug_sink = sink;
        self
    }

    #[inline]
    pub fn runtime_hooks(&self) -> &Arc<dyn RuntimeHooks> {
        self.runtime_state.hooks()
    }

    #[inline]
    pub fn set_runtime_hooks(&mut self, hooks: Arc<dyn RuntimeHooks>) {
        Arc::make_mut(&mut self.runtime_state).set_hooks(hooks);
    }

    #[inline]
    pub fn with_runtime_hooks(mut self, hooks: Arc<dyn RuntimeHooks>) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_hooks(hooks);
        self
    }

    #[inline]
    pub fn retry_policy(&self) -> &Arc<dyn RetryPolicy> {
        self.runtime_state.retry_policy()
    }

    #[inline]
    pub fn set_retry_policy(&mut self, retry_policy: Arc<dyn RetryPolicy>) {
        Arc::make_mut(&mut self.runtime_state).set_retry_policy(retry_policy);
    }

    #[inline]
    pub fn with_retry_policy(mut self, retry_policy: Arc<dyn RetryPolicy>) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_retry_policy(retry_policy);
        self
    }

    #[inline]
    pub fn max_auth_retries(&self) -> u32 {
        self.runtime_state.max_auth_retries()
    }

    #[inline]
    pub fn set_max_auth_retries(&mut self, max_auth_retries: u32) {
        Arc::make_mut(&mut self.runtime_state).set_max_auth_retries(max_auth_retries);
    }

    #[inline]
    pub fn with_max_auth_retries(mut self, max_auth_retries: u32) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_max_auth_retries(max_auth_retries);
        self
    }

    #[inline]
    pub fn rate_limiter(&self) -> &Arc<dyn RateLimiter> {
        self.runtime_state.rate_limiter()
    }

    #[inline]
    pub fn set_rate_limiter(&mut self, rate_limiter: Arc<dyn RateLimiter>) {
        Arc::make_mut(&mut self.runtime_state).set_rate_limiter(rate_limiter);
    }

    #[inline]
    pub fn with_rate_limiter(mut self, rate_limiter: Arc<dyn RateLimiter>) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_rate_limiter(rate_limiter);
        self
    }

    #[inline]
    pub fn runtime_state(&self) -> &Arc<ClientRuntimeState> {
        &self.runtime_state
    }

    #[inline]
    pub fn pagination_detect_loops(&self) -> bool {
        self.pagination_detect_loops
    }

    #[inline]
    pub fn set_pagination_detect_loops(&mut self, enabled: bool) {
        self.pagination_detect_loops = enabled;
    }

    #[inline]
    pub fn with_pagination_detect_loops(mut self, enabled: bool) -> Self {
        self.pagination_detect_loops = enabled;
        self
    }

    #[inline]
    pub fn with_debug_level(mut self, level: DebugLevel) -> Self {
        self.debug_level = level;
        self
    }

    #[inline]
    pub fn configure(&mut self, f: impl FnOnce(&mut crate::runtime::RuntimeConfig)) -> &mut Self {
        let mut config = crate::runtime::RuntimeConfig {
            hooks: self.runtime_state.hooks().clone(),
            rate_limiter: self.runtime_state.rate_limiter().clone(),
            retry_policy: self.runtime_state.retry_policy().clone(),
            auth: crate::runtime::AuthRuntimeConfig {
                max_retries: self.runtime_state.max_auth_retries(),
                max_retry_delay: self.runtime_state.max_retry_delay(),
            },
            max_rate_limit_cooldown: self.runtime_state.max_rate_limit_cooldown(),
            pagination_detect_loops: self.pagination_detect_loops,
            debug: crate::runtime::DebugConfig {
                level: self.debug_level,
                sink: self.debug_sink.clone(),
            },
            max_response_body_bytes: self.runtime_state.max_response_body_bytes(),
            max_stream_request_body_bytes: self.runtime_state.max_stream_request_body_bytes(),
            max_stream_response_body_bytes: self.runtime_state.max_stream_response_body_bytes(),
            dev_body_capture: self.runtime_state.dev_body_capture().cloned(),
        };
        f(&mut config);
        self.debug_level = config.debug.level;
        self.debug_sink = config.debug.sink.clone();
        self.pagination_detect_loops = config.pagination_detect_loops;
        Arc::make_mut(&mut self.runtime_state).apply_config(config);
        self
    }

    #[inline]
    pub fn request<E>(&self, ep: E) -> PendingRequest<'_, Cx, E, T>
    where
        E: crate::endpoint::IntoEndpointPlan<Cx>,
    {
        PendingRequest::new(self, ep)
    }

    #[inline]
    pub fn plan_context(&self) -> ClientPlanContext<'_, Cx> {
        ClientPlanContext {
            vars: self.vars(),
            auth_vars: self.auth_vars(),
        }
    }
}
