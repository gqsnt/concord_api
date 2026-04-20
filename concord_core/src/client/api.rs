#[derive(Clone)]
pub struct ApiClient<Cx: ClientContext, T: Transport + Clone = ReqwestTransport> {
    transport: T,
    vars: Cx::Vars,
    auth_vars: Cx::AuthVars,
    auth_state: Arc<RwLock<Arc<Cx::AuthState>>>,
    debug_level: DebugLevel,
    pagination_caps: Caps,
    debug_sink: Arc<dyn DebugSink>,
    runtime_state: Arc<ClientRuntimeState>,
}
impl<Cx: ClientContext> ApiClient<Cx, ReqwestTransport> {
    pub fn new(vars: Cx::Vars, auth_vars: Cx::AuthVars) -> Self {
        Self::with_reqwest_client(vars, auth_vars, reqwest::Client::new())
    }
    pub fn with_reqwest_client(
        vars: Cx::Vars,
        auth_vars: Cx::AuthVars,
        client: reqwest::Client,
    ) -> Self {
        Self::with_transport(vars, auth_vars, ReqwestTransport::new(client))
    }
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
            pagination_caps: Caps::default(),
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
        self.auth_state
            .read()
            .expect("auth_state lock poisoned")
            .clone()
    }

    #[inline]
    pub fn set_auth_state(&mut self, auth_state: Cx::AuthState) {
        *self.auth_state.write().expect("auth_state lock poisoned") = Arc::new(auth_state);
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
    pub fn cache_store(&self) -> &Arc<dyn CacheStore> {
        self.runtime_state.cache_store()
    }

    #[inline]
    pub fn set_cache_store(&mut self, cache_store: Arc<dyn CacheStore>) {
        Arc::make_mut(&mut self.runtime_state).set_cache_store(cache_store);
    }

    #[inline]
    pub fn with_cache_store(mut self, cache_store: Arc<dyn CacheStore>) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_cache_store(cache_store);
        self
    }

    #[inline]
    pub fn inflight_policy(&self) -> &Arc<dyn InflightPolicy> {
        self.runtime_state.inflight_policy()
    }

    #[inline]
    pub fn set_inflight_policy(&mut self, inflight_policy: Arc<dyn InflightPolicy>) {
        Arc::make_mut(&mut self.runtime_state).set_inflight_policy(inflight_policy);
    }

    #[inline]
    pub fn with_inflight_policy(mut self, inflight_policy: Arc<dyn InflightPolicy>) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_inflight_policy(inflight_policy);
        self
    }

    #[inline]
    pub fn runtime_state(&self) -> &Arc<ClientRuntimeState> {
        &self.runtime_state
    }

    #[inline]
    pub fn pagination_caps(&self) -> Caps {
        self.pagination_caps
    }

    #[inline]
    pub fn set_pagination_caps(&mut self, caps: Caps) {
        self.pagination_caps = caps;
    }

    #[inline]
    pub fn with_pagination_caps(mut self, caps: Caps) -> Self {
        self.pagination_caps = caps;
        self
    }

    #[inline]
    pub fn with_debug_level(mut self, level: DebugLevel) -> Self {
        self.debug_level = level;
        self
    }

    #[inline]
    pub fn request<E>(&self, ep: E) -> PendingRequest<'_, Cx, E, T>
    where
        E: Endpoint<Cx>,
    {
        PendingRequest::new(self, ep)
    }

    #[inline]
    fn ctx_for<E: Endpoint<Cx>>(ep: &E) -> ErrorContext {
        ErrorContext {
            endpoint: ep.name(),
            method: E::METHOD.clone(),
        }
    }

}
