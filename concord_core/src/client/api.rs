// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

#[derive(Clone)]
pub struct ApiClient<Cx: ClientContext> {
    pub(super) managed_client: crate::transport::ManagedReqwestClient,
    pub(super) vars: Cx::Vars,
    pub(super) auth_vars: Cx::AuthVars,
    pub(super) auth_state: Arc<RwLock<Arc<Cx::AuthState>>>,
    pub(super) debug_level: DebugLevel,
    pub(super) pagination_detect_loops: bool,
    pub(super) debug_sink: Arc<dyn DebugSink>,
    pub(super) runtime_state: Arc<ClientRuntimeState>,
    pub(super) api_headers: http::HeaderMap,
}

impl<Cx: ClientContext> ApiClient<Cx> {
    pub fn new(vars: Cx::Vars, auth_vars: Cx::AuthVars) -> Self {
        Self::with_managed_client(
            vars,
            auth_vars,
            crate::transport::ManagedReqwestClient::new(),
        )
    }

    /// Configures the managed Reqwest client through Concord's reviewed
    /// configuration surface. Raw Reqwest builders and clients stay private.
    pub fn with_reqwest_builder(
        vars: Cx::Vars,
        auth_vars: Cx::AuthVars,
        configure: impl FnOnce(
            crate::transport::SafeReqwestBuilder,
        ) -> crate::transport::SafeReqwestBuilder,
    ) -> Result<Self, crate::transport::ReqwestClientBuildError> {
        Self::with_reqwest_builder_fallible(vars, auth_vars, |builder| Ok(configure(builder)))
    }

    pub fn with_reqwest_builder_fallible(
        vars: Cx::Vars,
        auth_vars: Cx::AuthVars,
        configure: impl FnOnce(
            crate::transport::SafeReqwestBuilder,
        ) -> Result<
            crate::transport::SafeReqwestBuilder,
            crate::transport::ReqwestClientBuildError,
        >,
    ) -> Result<Self, crate::transport::ReqwestClientBuildError> {
        Ok(Self::with_managed_client(
            vars,
            auth_vars,
            crate::transport::ManagedReqwestClient::with_builder_fallible(configure)?,
        ))
    }

    /// Clearer spelling for [`Self::with_reqwest_builder`].
    pub fn with_safe_reqwest_builder(
        vars: Cx::Vars,
        auth_vars: Cx::AuthVars,
        configure: impl FnOnce(
            crate::transport::SafeReqwestBuilder,
        ) -> crate::transport::SafeReqwestBuilder,
    ) -> Result<Self, crate::transport::ReqwestClientBuildError> {
        Self::with_reqwest_builder(vars, auth_vars, configure)
    }

    pub fn with_safe_reqwest_builder_fallible(
        vars: Cx::Vars,
        auth_vars: Cx::AuthVars,
        configure: impl FnOnce(
            crate::transport::SafeReqwestBuilder,
        ) -> Result<
            crate::transport::SafeReqwestBuilder,
            crate::transport::ReqwestClientBuildError,
        >,
    ) -> Result<Self, crate::transport::ReqwestClientBuildError> {
        Self::with_reqwest_builder_fallible(vars, auth_vars, configure)
    }
    fn with_managed_client(
        vars: Cx::Vars,
        auth_vars: Cx::AuthVars,
        managed_client: crate::transport::ManagedReqwestClient,
    ) -> Self {
        let auth_state = Cx::init_auth_state(&vars, &auth_vars);
        Self {
            managed_client,
            vars,
            auth_vars,
            auth_state: Arc::new(RwLock::new(Arc::new(auth_state))),
            debug_level: DebugLevel::default(),
            pagination_detect_loops: true,
            debug_sink: Arc::new(StderrDebugSink),
            runtime_state: Arc::new(ClientRuntimeState::default()),
            api_headers: http::HeaderMap::new(),
        }
    }

    /// Installs client-wide origin API headers. Endpoint headers replace
    /// matching names during request preparation.
    pub fn set_api_headers(
        &mut self,
        headers: http::HeaderMap,
    ) -> Result<(), crate::header_ownership::HeaderOwnershipError> {
        crate::header_ownership::validate_public_headers(&headers)?;
        self.api_headers = headers;
        Ok(())
    }

    pub fn with_api_headers(
        mut self,
        headers: http::HeaderMap,
    ) -> Result<Self, crate::header_ownership::HeaderOwnershipError> {
        self.set_api_headers(headers)?;
        Ok(self)
    }

    pub fn api_headers(&self) -> &http::HeaderMap {
        &self.api_headers
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
    pub fn max_attempts(&self) -> u32 {
        self.runtime_state.max_attempts()
    }

    #[inline]
    pub fn set_max_attempts(&mut self, max_attempts: u32) {
        Arc::make_mut(&mut self.runtime_state).set_max_attempts(max_attempts);
    }

    #[inline]
    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_max_attempts(max_attempts);
        self
    }

    #[inline]
    pub fn respect_retry_after(&self) -> bool {
        self.runtime_state.respect_retry_after()
    }

    #[inline]
    pub fn set_respect_retry_after(&mut self, enabled: bool) {
        Arc::make_mut(&mut self.runtime_state).set_respect_retry_after(enabled);
    }

    #[inline]
    pub fn with_respect_retry_after(mut self, enabled: bool) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_respect_retry_after(enabled);
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
            retry_admission: self.runtime_state.retry_admission().clone(),
            max_attempts: self.runtime_state.max_attempts(),
            respect_retry_after: self.runtime_state.respect_retry_after(),
            max_rate_limit_cooldown: self.runtime_state.max_rate_limit_cooldown(),
            pagination_detect_loops: self.pagination_detect_loops,
            debug: crate::runtime::DebugConfig {
                level: self.debug_level,
                sink: self.debug_sink.clone(),
            },
            max_response_body_bytes: self.runtime_state.max_response_body_bytes(),
            max_stream_request_body_bytes: self.runtime_state.max_stream_request_body_bytes(),
            max_stream_response_body_bytes: self.runtime_state.max_stream_response_body_bytes(),
            #[cfg(feature = "dangerous-dev-tools")]
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
    pub fn request<E>(&self, ep: E) -> PendingRequest<'_, Cx, E>
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
