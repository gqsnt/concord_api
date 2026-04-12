use crate::auth::{
    AuthBuildContext, AuthController, AuthError, AuthErrorKind, AuthHttpExecutor, AuthHttpRequest,
    AuthHttpResponse, AuthMode, AuthPart, AuthPrepareContext as EndpointAuthPrepareContext,
    AuthResponseAction, AuthResponseContext as EndpointAuthResponseContext,
};
use crate::auth_provider::{AuthMeta, AuthPrepareContext, AuthProvider, AuthResponseContext};
use crate::cache::CacheStore;
use crate::codec::FormatType;
use crate::codec::{ContentType, Decodes, Encodes};
use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::endpoint::{BodyPart, Endpoint, PolicyPart, ResponseSpec, RoutePart};
use crate::error::{ApiClientError, ErrorContext};
use crate::inflight::{InflightPolicy, RequestKey, SharedSendError, SharedSendResult};
use crate::pagination::Caps;
use crate::policy::{Policy, PolicyLayer, PolicyPatch};
use crate::rate_limit::{RateLimitContext, RateLimitResponseContext, RateLimiter};
use crate::request::PendingRequest;
use crate::response_classify::{ResponseClass, classify_status};
use crate::retry::{RetryContext, RetryDecision, RetryOutcome, RetryPolicy};
use crate::runtime_hooks::{
    HookMeta, PostResponseHookContext, PreSendHookContext, RuntimeHooks, TransportErrorHookContext,
};
use crate::runtime_state::ClientRuntimeState;
use crate::transport::{BuiltRequest, BuiltResponse, DecodedResponse, RequestMeta};
use crate::transport::{
    ReqwestTransport, Transport, TransportBody, TransportError, TransportResponse,
};
use crate::types::RouteParts;
use bytes::Bytes;
use http::StatusCode;
use http::header::CONTENT_TYPE;
use http::uri::Scheme;
use std::sync::Arc;

pub trait ClientContext: Sized + Send + Sync + 'static {
    type Vars: Clone + Send + Sync + 'static;
    type AuthVars: Clone + Send + Sync + 'static;
    type AuthState: Clone + Send + Sync + 'static;
    const SCHEME: Scheme;
    const DOMAIN: &'static str;

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState;

    fn base_route(_vars: &Self::Vars, _auth: &Self::AuthVars) -> RouteParts {
        RouteParts::new()
    }

    fn base_policy(
        _vars: &Self::Vars,
        _auth: &Self::AuthVars,
        _ctx: &ErrorContext,
    ) -> Result<Policy, ApiClientError> {
        Ok(Policy::new())
    }
}

#[derive(Clone)]
pub struct ApiClient<Cx: ClientContext, T: Transport + Clone = ReqwestTransport> {
    transport: T,
    vars: Cx::Vars,
    auth_vars: Cx::AuthVars,
    auth_state: Cx::AuthState,
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
            auth_state,
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
    pub fn auth_state(&self) -> &Cx::AuthState {
        &self.auth_state
    }

    #[inline]
    pub fn set_auth_state(&mut self, auth_state: Cx::AuthState) {
        self.auth_state = auth_state;
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
    pub fn auth_provider(&self) -> &Arc<dyn AuthProvider> {
        self.runtime_state.auth_provider()
    }

    #[inline]
    pub fn set_auth_provider(&mut self, auth_provider: Arc<dyn AuthProvider>) {
        Arc::make_mut(&mut self.runtime_state).set_auth_provider(auth_provider);
    }

    #[inline]
    pub fn with_auth_provider(mut self, auth_provider: Arc<dyn AuthProvider>) -> Self {
        Arc::make_mut(&mut self.runtime_state).set_auth_provider(auth_provider);
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

    pub(crate) async fn execute_decoded_ref_with<E, F>(
        &self,
        ep: &E,
        meta: RequestMeta,
        dbg: DebugLevel,
        patch_policy: F,
    ) -> Result<DecodedResponse<<E::Response as ResponseSpec>::Output>, ApiClientError>
    where
        E: Endpoint<Cx>,
        F: for<'a> Fn(&mut PolicyPatch<'a>) -> Result<(), ApiClientError>,
    {
        let dbg_verbose = dbg.is_verbose();
        let dbg_vv = dbg.is_very_verbose();
        let ctx = Self::ctx_for::<E>(ep);
        let base_attempt = meta.attempt;
        let max_retries = self.runtime_state.retry_policy().max_retries();
        let auth_ctrl = <E::Auth as AuthPart<Cx, E>>::controller(
            AuthBuildContext {
                vars: self.vars(),
                auth: self.auth_vars(),
                auth_state: self.auth_state(),
            },
            ep,
        )?;
        let mut endpoint_auth_state = auth_ctrl.init(ep)?;
        let mut attempt_index: u32 = 0;
        let mut transport_retry_index: u32 = 0;

        loop {
            let mut attempt_meta = meta.clone();
            attempt_meta.attempt = base_attempt.saturating_add(attempt_index);

            let mut built = self.build_request::<E, F>(ep, attempt_meta, &patch_policy)?;
            let auth_http = ClientAuthHttpExecutor { client: self };
            let auth_request_meta = built.meta.clone();
            let auth_attempt = auth_ctrl
                .prepare(
                    &mut endpoint_auth_state,
                    EndpointAuthPrepareContext {
                        ep,
                        vars: self.vars(),
                        auth: self.auth_vars(),
                        auth_state: self.auth_state(),
                        executor: &auth_http,
                        meta: &auth_request_meta,
                        request: &mut built,
                    },
                )
                .await?;
            let auth_url = built.url.as_str().to_owned();
            let auth_method = built.meta.method.clone();
            let auth_meta = AuthMeta {
                endpoint: built.meta.endpoint,
                method: &auth_method,
                url: &auth_url,
                attempt: built.meta.attempt,
                page_index: built.meta.page_index,
                idempotent: built.meta.idempotent,
            };
            self.runtime_state
                .auth_provider()
                .prepare_request(AuthPrepareContext {
                    meta: auth_meta,
                    request: &mut built,
                })
                .await?;
            let url_str = built.url.as_str().to_string();
            let cache_key = self.runtime_state.cache_store().key_for(&built);
            if let Some(key) = cache_key.as_ref()
                && let Some(cached) = self.runtime_state.cache_store().get(key).await
            {
                return Self::decode_built_response::<E>(cached, ctx.clone());
            }

            if dbg_verbose {
                self.debug_sink.request_start(
                    dbg,
                    &E::METHOD,
                    &url_str,
                    ep.name(),
                    built.meta.page_index,
                );
            }

            if dbg_vv {
                self.debug_sink.request_headers(dbg, &built.headers);
                if let Some(body) = built.body.as_ref() {
                    const MAX_CHARS: usize = 32 * 1024;
                    let fmt = <<E::Body as BodyPart<E>>::Enc as FormatType>::FORMAT_TYPE;
                    self.debug_sink.request_body(dbg, body, fmt, MAX_CHARS);
                }
            }
            let pre_send_meta = HookMeta {
                endpoint: built.meta.endpoint,
                method: &built.meta.method,
                url: &url_str,
                attempt: built.meta.attempt,
                page_index: built.meta.page_index,
                idempotent: built.meta.idempotent,
            };
            let rate_limit_meta = RateLimitContext {
                endpoint: built.meta.endpoint,
                method: &built.meta.method,
                url: &url_str,
                attempt: built.meta.attempt,
                page_index: built.meta.page_index,
                idempotent: built.meta.idempotent,
            };
            let _permit = self
                .runtime_state
                .rate_limiter()
                .acquire(rate_limit_meta)
                .await?;
            self.runtime_state
                .hooks()
                .pre_send(PreSendHookContext {
                    meta: pre_send_meta,
                    headers: &built.headers,
                })
                .await?;
            let inflight_key = self.runtime_state.inflight_policy().key_for(&built);

            let send_result = self
                .send_and_classify_with_inflight(
                    built,
                    inflight_key,
                    dbg,
                    dbg_verbose,
                    dbg_vv,
                    &url_str,
                    &ctx,
                )
                .await;

            match send_result {
                Ok(resp) => {
                    let auth_action = auth_ctrl
                        .on_response(
                            &mut endpoint_auth_state,
                            EndpointAuthResponseContext {
                                ep,
                                vars: self.vars(),
                                auth: self.auth_vars(),
                                auth_state: self.auth_state(),
                                executor: &auth_http,
                                meta: &resp.meta,
                                status: resp.status,
                                headers: &resp.headers,
                                attempt: &auth_attempt,
                            },
                        )
                        .await?;
                    if matches!(auth_action, AuthResponseAction::Retry { .. }) {
                        attempt_index = attempt_index.saturating_add(1);
                        continue;
                    }
                    if let Some(key) = cache_key {
                        self.runtime_state
                            .cache_store()
                            .put(key, resp.clone())
                            .await;
                    }
                    if dbg_verbose {
                        self.debug_sink
                            .response_status(dbg, resp.status, &url_str, true);
                    }
                    if dbg_vv {
                        const MAX_CHARS: usize = 32 * 1024;
                        let fmt = <<E::Response as ResponseSpec>::Dec as FormatType>::FORMAT_TYPE;
                        self.debug_sink.response_headers(dbg, &resp.headers);
                        self.debug_sink
                            .response_body(dbg, &resp.body, fmt, MAX_CHARS);
                    }
                    return Self::decode_built_response::<E>(resp, ctx.clone());
                }
                Err(err) => {
                    if let ApiClientError::HttpStatus {
                        status, headers, ..
                    } = &err
                    {
                        let response_meta = RequestMeta {
                            endpoint: ep.name(),
                            method: E::METHOD.clone(),
                            idempotent: meta.idempotent,
                            attempt: base_attempt.saturating_add(attempt_index),
                            page_index: meta.page_index,
                        };
                        let auth_action = auth_ctrl
                            .on_response(
                                &mut endpoint_auth_state,
                                EndpointAuthResponseContext {
                                    ep,
                                    vars: self.vars(),
                                    auth: self.auth_vars(),
                                    auth_state: self.auth_state(),
                                    executor: &auth_http,
                                    meta: &response_meta,
                                    status: *status,
                                    headers,
                                    attempt: &auth_attempt,
                                },
                            )
                            .await?;
                        if matches!(auth_action, AuthResponseAction::Retry { .. }) {
                            attempt_index = attempt_index.saturating_add(1);
                            continue;
                        }
                    }
                    if transport_retry_index >= max_retries {
                        return Err(err);
                    }
                    let outcome = Self::retry_outcome_from_error(&err);
                    let retry_ctx = RetryContext {
                        endpoint: ep.name(),
                        method: &E::METHOD,
                        url: &url_str,
                        attempt: base_attempt.saturating_add(attempt_index),
                        page_index: meta.page_index,
                        idempotent: meta.idempotent,
                        outcome,
                    };
                    if self.runtime_state.retry_policy().should_retry(&retry_ctx)
                        != RetryDecision::Retry
                    {
                        return Err(err);
                    }
                    transport_retry_index = transport_retry_index.saturating_add(1);
                    attempt_index = attempt_index.saturating_add(1);
                }
            }
        }
    }
}

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    fn build_request<E, F>(
        &self,
        ep: &E,
        meta: RequestMeta,
        patch_policy: &F,
    ) -> Result<BuiltRequest, ApiClientError>
    where
        E: Endpoint<Cx>,
        F: for<'a> Fn(&mut PolicyPatch<'a>) -> Result<(), ApiClientError>,
    {
        let ctx = Self::ctx_for::<E>(ep);
        // Route = base + endpoint route part
        let mut route = Cx::base_route(self.vars(), self.auth_vars());
        <E::Route as RoutePart<Cx, E>>::apply(ep, self.vars(), self.auth_vars(), &mut route)?;

        // Policy layering model:
        // client (base_policy) -> (prefix/path) -> endpoint -> runtime injections
        let mut policy = Cx::base_policy(self.vars(), self.auth_vars(), &ctx)?;
        policy.set_layer(PolicyLayer::Endpoint);
        <E::Policy as PolicyPart<Cx, E>>::apply(ep, self.vars(), self.auth_vars(), &mut policy)?;

        // Runtime Accept injection (decoder-owned) after endpoint policy.
        policy.set_layer(PolicyLayer::Runtime);
        let is_head = E::METHOD == http::Method::HEAD;
        if !is_head && !E::response_is_no_content() {
            policy.ensure_accept(E::accept_content_type());
        }

        // Runtime patch (pagination controller, etc.)
        {
            let mut patch = PolicyPatch::new(ctx.clone(), &mut policy);
            patch_policy(&mut patch)?;
        }

        // Compute parts after patch (Content-Type may have been added/removed there).
        let (mut headers, query, timeout) = policy.into_parts();

        // URL
        route.host().validate(ctx.clone())?;
        let host = route.host().join(Cx::DOMAIN);
        let base = format!("{}://{}", Cx::SCHEME, host);
        let mut url = url::Url::parse(&base).map_err(|e| ApiClientError::BuildUrl {
            ctx: ctx.clone(),
            source: e,
        })?;
        url.set_path(route.path().as_str());
        {
            let mut qp = url.query_pairs_mut();
            for (k, v) in query.iter() {
                qp.append_pair(k, v);
            }
        }

        // Body (optional) + Content-Type injection if missing.
        let mut body_bytes: Option<Bytes> = None;
        if let Some(body) = <E::Body as BodyPart<E>>::body(ep) {
            let encoded = <<E::Body as BodyPart<E>>::Enc as Encodes<
                <E::Body as BodyPart<E>>::Body,
            >>::encode(body)
            .map_err(|e| ApiClientError::codec_error(ctx.clone(), e))?;

            if !headers.contains_key(CONTENT_TYPE) {
                let ct = <<E::Body as BodyPart<E>>::Enc as ContentType>::CONTENT_TYPE;
                if !ct.is_empty() {
                    headers.insert(CONTENT_TYPE, http::HeaderValue::from_static(ct));
                }
            }
            body_bytes = Some(encoded);
        }

        Ok(BuiltRequest {
            meta,
            url,
            headers,
            body: body_bytes,
            timeout,
            extensions: Default::default(),
        })
    }

    async fn send_built_request(
        &self,
        built: BuiltRequest,
        ctx: &ErrorContext,
    ) -> Result<TransportResponse, ApiClientError> {
        let endpoint = built.meta.endpoint;
        let method = built.meta.method.clone();
        let attempt = built.meta.attempt;
        let page_index = built.meta.page_index;
        let idempotent = built.meta.idempotent;
        let url = built.url.as_str().to_owned();

        match self.transport.send(built).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                let hook_meta = HookMeta {
                    endpoint,
                    method: &method,
                    url: &url,
                    attempt,
                    page_index,
                    idempotent,
                };
                self.runtime_state
                    .hooks()
                    .transport_error(TransportErrorHookContext {
                        meta: hook_meta,
                        error: &e,
                    })
                    .await;
                Err(ApiClientError::Transport {
                    ctx: ctx.clone(),
                    source: e,
                })
            }
        }
    }

    async fn classify_transport_response(
        &self,
        mut resp: TransportResponse,
        dbg: DebugLevel,
        dbg_verbose: bool,
        _dbg_vv: bool,
        url_str: &str,
        ctx: &ErrorContext,
    ) -> Result<BuiltResponse, ApiClientError> {
        let hook_meta = HookMeta {
            endpoint: resp.meta.endpoint,
            method: &resp.meta.method,
            url: resp.url.as_str(),
            attempt: resp.meta.attempt,
            page_index: resp.meta.page_index,
            idempotent: resp.meta.idempotent,
        };
        self.runtime_state
            .hooks()
            .post_response(PostResponseHookContext {
                meta: hook_meta,
                status: resp.status,
                headers: &resp.headers,
            })
            .await;
        let auth_meta = AuthMeta {
            endpoint: resp.meta.endpoint,
            method: &resp.meta.method,
            url: resp.url.as_str(),
            attempt: resp.meta.attempt,
            page_index: resp.meta.page_index,
            idempotent: resp.meta.idempotent,
        };
        self.runtime_state
            .auth_provider()
            .on_response(AuthResponseContext {
                meta: auth_meta,
                status: resp.status,
                headers: &resp.headers,
            })
            .await;
        let rate_limit_meta = RateLimitContext {
            endpoint: resp.meta.endpoint,
            method: &resp.meta.method,
            url: resp.url.as_str(),
            attempt: resp.meta.attempt,
            page_index: resp.meta.page_index,
            idempotent: resp.meta.idempotent,
        };
        self.runtime_state
            .rate_limiter()
            .on_response(RateLimitResponseContext {
                meta: rate_limit_meta,
                status: resp.status,
                headers: &resp.headers,
            })
            .await;
        match classify_status(resp.status) {
            ResponseClass::HttpStatusError => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status, url_str, false);
                    self.debug_sink.response_headers(dbg, &resp.headers);
                }
                Err(ApiClientError::HttpStatus {
                    ctx: ctx.clone(),
                    status: resp.status,
                    headers: resp.headers,
                })
            }
            ResponseClass::Success => {
                let bytes = read_body_all(resp.body.as_mut(), resp.content_length)
                    .await
                    .map_err(|e| ApiClientError::Transport {
                        ctx: ctx.clone(),
                        source: e,
                    })?;
                Ok(BuiltResponse {
                    meta: resp.meta,
                    url: resp.url,
                    status: resp.status,
                    headers: resp.headers,
                    body: bytes,
                })
            }
        }
    }

    async fn send_and_classify_with_inflight(
        &self,
        built: BuiltRequest,
        inflight_key: Option<RequestKey>,
        dbg: DebugLevel,
        dbg_verbose: bool,
        dbg_vv: bool,
        url_str: &str,
        ctx: &ErrorContext,
    ) -> Result<BuiltResponse, ApiClientError> {
        if let Some(key) = inflight_key {
            let join = self
                .runtime_state
                .inflight_registry()
                .join_or_lead(key)
                .await;
            if join.is_leader() {
                let result = self
                    .send_and_classify_once(built, dbg, dbg_verbose, dbg_vv, url_str, ctx)
                    .await;
                let shared = match &result {
                    Ok(resp) => SharedSendResult::Ok(resp.clone()),
                    Err(err) => SharedSendResult::Err(SharedSendError::from_api_error(err)),
                };
                join.complete(self.runtime_state.inflight_registry(), shared)
                    .await;
                result
            } else {
                match join.wait().await {
                    SharedSendResult::Ok(resp) => Ok(resp),
                    SharedSendResult::Err(err) => Err(err.into_api_error(ctx.clone())),
                }
            }
        } else {
            self.send_and_classify_once(built, dbg, dbg_verbose, dbg_vv, url_str, ctx)
                .await
        }
    }

    async fn send_and_classify_once(
        &self,
        built: BuiltRequest,
        dbg: DebugLevel,
        dbg_verbose: bool,
        dbg_vv: bool,
        url_str: &str,
        ctx: &ErrorContext,
    ) -> Result<BuiltResponse, ApiClientError> {
        let transport_resp = self.send_built_request(built, ctx).await?;
        self.classify_transport_response(transport_resp, dbg, dbg_verbose, dbg_vv, url_str, ctx)
            .await
    }

    fn decode_built_response<E>(
        resp: BuiltResponse,
        ctx: ErrorContext,
    ) -> Result<DecodedResponse<<E::Response as ResponseSpec>::Output>, ApiClientError>
    where
        E: Endpoint<Cx>,
    {
        // Enforce the documented constraints:
        // - HEAD must map to a NoContent decoder (body is empty by definition).
        if resp.meta.method == http::Method::HEAD && !E::response_is_no_content() {
            return Err(ApiClientError::HeadRequiresNoContent { ctx });
        }

        // - 204/205 are "no content" success statuses. If the endpoint expects content, fail early with a clear error.
        if matches!(
            resp.status,
            StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT
        ) && !E::response_is_no_content()
        {
            return Err(ApiClientError::NoContentStatusRequiresNoContent {
                ctx: ctx.clone(),
                status: resp.status,
            });
        }

        let decoded = <<E::Response as ResponseSpec>::Dec as Decodes<
            <E::Response as ResponseSpec>::Decoded,
        >>::decode(&resp.body)
        .map_err(|e| ApiClientError::Decode {
            ctx: ctx.clone(),
            source: e.into(),
        })?;

        let decoded_resp = DecodedResponse {
            meta: resp.meta,
            url: resp.url,
            status: resp.status,
            headers: resp.headers,
            value: decoded,
        };
        <E::Response as ResponseSpec>::map_response(decoded_resp)
            .map_err(|e| ApiClientError::Transform { ctx, source: e })
    }

    fn retry_outcome_from_error(err: &ApiClientError) -> RetryOutcome<'_> {
        match err {
            ApiClientError::Transport { source, .. } => RetryOutcome::Transport(source),
            ApiClientError::HttpStatus { status, .. } => RetryOutcome::HttpStatus(*status),
            ApiClientError::Decode { .. } => RetryOutcome::Decode,
            ApiClientError::Transform { .. } => RetryOutcome::Transform,
            _ => RetryOutcome::Other,
        }
    }
}

struct ClientAuthHttpExecutor<'a, Cx: ClientContext, T: Transport> {
    client: &'a ApiClient<Cx, T>,
}

impl<Cx: ClientContext, T: Transport> AuthHttpExecutor for ClientAuthHttpExecutor<'_, Cx, T> {
    fn send<'a>(
        &'a self,
        req: AuthHttpRequest,
    ) -> crate::auth::AuthFuture<'a, Result<AuthHttpResponse, AuthError>> {
        Box::pin(async move {
            if !matches!(req.mode, AuthMode::SkipAuth) {
                return Err(AuthError::new(
                    AuthErrorKind::UnsupportedScheme,
                    "internal auth requests currently support SkipAuth only",
                ));
            }

            let meta = RequestMeta {
                endpoint: "<auth>",
                method: req.method,
                idempotent: false,
                attempt: 0,
                page_index: 0,
            };
            let built = BuiltRequest {
                meta,
                url: req.url,
                headers: req.headers,
                body: req.body,
                timeout: req.policy.timeout,
                extensions: Default::default(),
            };
            let mut resp = self.client.transport.send(built).await.map_err(|source| {
                AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
            })?;
            let body = read_body_all(resp.body.as_mut(), resp.content_length)
                .await
                .map_err(|source| {
                    AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
                })?;
            Ok(AuthHttpResponse {
                status: resp.status,
                headers: resp.headers,
                body,
            })
        })
    }
}

async fn read_body_all(
    body: &mut dyn TransportBody,
    content_length: Option<u64>,
) -> Result<Bytes, TransportError> {
    // Sanity cap: au-delà, on évite toute pré-allocation “grosse” basée sur Content-Length.
    const SANITY_CAP: usize = 2 * 1024 * 1024; // 2MB
    const SMALL_START: usize = 8 * 1024;
    const LARGE_START: usize = 64 * 1024;

    let cap = match content_length {
        None => SMALL_START,
        Some(n) => {
            let n_usize = usize::try_from(n).unwrap_or(usize::MAX);
            if n_usize <= SANITY_CAP {
                n_usize.max(SMALL_START)
            } else {
                LARGE_START
            }
        }
    };
    let mut buf = bytes::BytesMut::with_capacity(cap);
    while let Some(chunk) = body.next_chunk().await? {
        buf.extend_from_slice(&chunk);
    }
    Ok(buf.freeze())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::codec::{Format, FormatType, NoContent, format_debug_body, text::Text};

    struct BinaryEncoding;
    impl FormatType for BinaryEncoding {
        const FORMAT_TYPE: Format = Format::Binary;
    }

    #[test]
    fn debug_preview_uses_request_encoder_and_response_decoder_formats() {
        // Request: binary => base64
        let req = Bytes::from_static(&[0x00, 0x01, 0x02]);
        let req_s = format_debug_body::<BinaryEncoding>(&req, 1024);
        assert_eq!(req_s, "AAEC");

        // Response: text => UTF-8
        let resp = Bytes::from_static(b"hello");
        let resp_s = format_debug_body::<Text>(&resp, 1024);
        assert_eq!(resp_s, "hello");

        // sanity: NoContentEncoding is text-format (empty)
        let empty = Bytes::new();
        let s = crate::codec::format_debug_body::<NoContent>(&empty, 1024);
        assert_eq!(s, "");
    }
}
