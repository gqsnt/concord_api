//! Configured client, immutable query identity and one-execution response handling.
use crate::api::{SqlApi, SqlApiConfig, StatementBody};
use crate::partitions::FirstResult;
use crate::{
    Error, ProtocolError, RequestContext, RequestId, ServerError, ServerMessage, Statement,
    StatementHandle,
};
use concord::reqwest::{self, Url, header::HeaderValue};
use concord::{Prepared, ResponseMeta};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// Authentication token classification. Token acquisition/renewal is caller-owned.
#[derive(Clone, Copy, Debug)]
pub enum TokenType {
    /// An already-issued OAuth access token.
    OAuth,
    /// An already-signed key-pair JWT; this SDK does not sign it.
    KeyPairJwt,
    /// A programmatic access token.
    ProgrammaticAccessToken,
}
impl TokenType {
    fn wire(self) -> &'static str {
        match self {
            Self::OAuth => "OAUTH",
            Self::KeyPairJwt => "KEYPAIR_JWT",
            Self::ProgrammaticAccessToken => "PROGRAMMATIC_ACCESS_TOKEN",
        }
    }
}

/// Bounded HTTP configuration. Redirects and automatic retries are disabled.
#[derive(Clone, Debug)]
pub struct ClientOptions {
    /// HTTP request deadline, including the body (default 30 seconds).
    pub request_timeout: Duration,
    /// Bytes per successful response, and independently per inflated partition (default 16 MiB).
    pub max_response_bytes: usize,
    /// Bytes per provider error (default 64 KiB).
    pub max_error_bytes: usize,
    /// Application/version sent as User-Agent.
    pub user_agent: String,
}
impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            max_response_bytes: 16 * 1024 * 1024,
            max_error_bytes: 64 * 1024,
            user_agent: "concord-sdk-snowflake/0.0.0".into(),
        }
    }
}

/// Successful response value together with its original response metadata.
#[derive(Clone, Debug)]
pub struct Reply<T> {
    /// Decoded response body.
    pub value: T,
    /// HTTP status, headers and generated operation identity.
    pub meta: ResponseMeta,
}

/// Sends `authorization: Bearer <token>` from the constructor token and `user-agent` from
/// `ClientOptions::user_agent`. `x-snowflake-authorization-token-type` is `OAUTH`,
/// `KEYPAIR_JWT` or `PROGRAMMATIC_ACCESS_TOKEN`, according to `TokenType`.
///
/// An account-bound client. Clones share the HTTP connection pool.
#[derive(Clone)]
pub struct Client {
    pub(crate) api: SqlApi,
    pub(crate) options: Arc<ClientOptions>,
}
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SnowflakeClient { .. }")
    }
}
impl Client {
    /// Constructs a client for an HTTPS account origin, or an HTTP loopback origin for tests.
    /// The token is marked sensitive; Debug never includes it. No network request occurs here.
    /// Partition gzip decoding uses flate2 inside this SDK. No reqwest decompression feature is
    /// enabled; both encoded and inflated bytes are bounded, and wire headers are preserved.
    pub fn new(
        base: &str,
        token: &str,
        token_type: TokenType,
        options: ClientOptions,
    ) -> Result<Self, Error> {
        let url = reqwest::Url::parse(base).map_err(|_| Error::Config("invalid account origin"))?;
        let local = url.host_str().is_some_and(|h| {
            h == "localhost"
                || h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if !(url.scheme() == "https" || (url.scheme() == "http" && local))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(Error::Config(
                "expected an HTTPS account origin or HTTP loopback origin",
            ));
        }
        if token.is_empty() || token.bytes().any(|b| b.is_ascii_whitespace()) {
            return Err(Error::Config(
                "token must be nonempty and contain no whitespace",
            ));
        }
        if options.request_timeout.is_zero()
            || options.max_response_bytes == 0
            || options.max_error_bytes == 0
        {
            return Err(Error::Config("timeouts and body limits must be positive"));
        }
        let mut authorization = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| Error::Config("invalid auth header"))?;
        authorization.set_sensitive(true);
        let user_agent = HeaderValue::from_str(&options.user_agent)
            .map_err(|_| Error::Config("invalid user agent"))?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(options.request_timeout)
            // Keep our wire decoder stable even when an application enables these features.
            .no_gzip()
            .no_brotli()
            .no_zstd()
            .no_deflate()
            .build()
            .map_err(|_| Error::Config("HTTP client construction failed"))?;
        let api = SqlApi::new(
            http,
            SqlApiConfig {
                authorization,
                token_type: token_type.wire().into(),
                user_agent,
            },
        )
        .with_base_url(base)?;
        Ok(Self {
            api,
            options: Arc::new(options),
        })
    }

    /// Submits exactly once with async=true. Immediate completion is also accepted.
    /// Reuse the returned Query to poll, cancel or correlate partitions. No automatic resubmit.
    pub async fn submit(
        &self,
        context: RequestContext,
        statement: Statement,
    ) -> Result<Submission, Error> {
        if statement.statement.trim().is_empty() {
            return Err(Error::Config("empty statement"));
        }
        let prepared = self
            .api
            .submit(context.submission_id.clone(), statement)
            .prepare()?;
        let url = prepared.request().url().clone();
        let reply = self.execute(prepared, &[200, 202], None).await?;
        let handle = match &reply.value {
            StatementBody::Pending(s) => s.statement_handle.clone(),
            StatementBody::Complete(r) => r.status.statement_handle.clone(),
        }
        .ok_or_else(|| Error::protocol(ProtocolError::HandleMismatch, Some(&reply.meta)))?;
        let query = Query {
            client: self.clone(),
            handle,
            context: Arc::new(context),
            status_url: Arc::new(Mutex::new(None)),
        };
        let state = query.state(reply, &url)?;
        Ok(Submission { query, state })
    }

    pub(crate) async fn execute<T>(
        &self,
        prepared: Prepared<T>,
        statuses: &[u16],
        expected: Option<&StatementHandle>,
    ) -> Result<Reply<T>, Error> {
        let received = prepared.receive().await?;
        if !received.status().is_success() {
            let meta = received.meta();
            let mut error = ServerError {
                meta: meta.clone(),
                document: None,
                body: Default::default(),
                read_error: None,
                decode_error: None,
            };
            match received.bytes_limited(self.options.max_error_bytes).await {
                Ok(bytes) => {
                    match serde_json::from_slice::<ServerMessage>(&bytes) {
                        Ok(doc) => {
                            if let (Some(expected), Some(actual)) =
                                (expected, &doc.statement_handle)
                                && expected != actual
                            {
                                return Err(Error::protocol(
                                    ProtocolError::HandleMismatch,
                                    Some(&meta),
                                ));
                            }
                            error.document = Some(doc);
                        }
                        Err(e) => error.decode_error = Some(e),
                    }
                    error.body = bytes;
                }
                Err(e) => error.read_error = Some(e),
            }
            return Err(Error::Server(Box::new(error)));
        }
        if !statuses.contains(&received.status().as_u16()) {
            return Err(Error::protocol(
                ProtocolError::UnexpectedStatus,
                Some(&received.meta()),
            ));
        }
        let (value, meta) = received
            .decode_limited_with_meta(self.options.max_response_bytes)
            .await?;
        Ok(Reply { value, meta })
    }
}

/// The first observation and its durable, account-bound query handle.
#[derive(Debug)]
pub struct Submission {
    /// Use this handle even after a local deadline/stop; it remains a remote query.
    pub query: Query,
    /// Pending or already complete.
    pub state: StatementState,
}

/// An observation, not a promise of future state.
#[derive(Debug)]
pub enum StatementState {
    /// HTTP 202; the statement is still running.
    Pending(Reply<ServerMessage>),
    /// HTTP 200 with validated first partition and complete discovery metadata.
    Complete(FirstResult),
}

/// Identity and submission context retained across polling, cancellation and result retrieval.
#[derive(Clone)]
pub struct Query {
    pub(crate) client: Client,
    pub(crate) handle: StatementHandle,
    pub(crate) context: Arc<RequestContext>,
    status_url: Arc<Mutex<Option<Url>>>,
}
impl std::fmt::Debug for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field("handle", &self.handle)
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}
impl Query {
    /// Server statement identity.
    pub fn handle(&self) -> &StatementHandle {
        &self.handle
    }
    /// Stable context of the original submission.
    pub fn context(&self) -> &RequestContext {
        &self.context
    }
    /// Fetches status once; completion includes partition zero, without a second request.
    /// Follows the latest correlated status URL, preserving its opaque query and request ID.
    /// Cloned queries share that target. Without a returned link, uses the declared handle path.
    pub async fn status(&self) -> Result<StatementState, Error> {
        let mut prepared = self
            .client
            .api
            .statement(self.handle.clone())
            .status()
            .prepare()?;
        let target = self.status_url.lock().unwrap().clone();
        if let Some(url) = target {
            prepared = prepared.follow(url.as_str())?;
        }
        let url = prepared.request().url().clone();
        self.state(self.execute(prepared, &[200, 202], &url).await?, &url)
    }
    pub(crate) fn correlate(
        &self,
        actual: Option<&StatementHandle>,
        meta: &ResponseMeta,
    ) -> Result<(), Error> {
        if actual != Some(&self.handle) {
            return Err(Error::protocol(ProtocolError::HandleMismatch, Some(meta)));
        }
        Ok(())
    }
    fn remember_status(
        &self,
        value: &ServerMessage,
        meta: &ResponseMeta,
        base: &Url,
    ) -> Result<(), Error> {
        self.correlate(value.statement_handle.as_ref(), meta)?;
        if let Some(raw) = &value.statement_status_url {
            let url = crate::links::resolve(self, base, raw, meta)?;
            if crate::links::partition_index(&url, meta)?.is_some_and(|index| index != 0) {
                return Err(Error::protocol(ProtocolError::InvalidLink, Some(meta)));
            }
            *self.status_url.lock().unwrap() = Some(url);
        }
        Ok(())
    }
    async fn execute<T>(
        &self,
        prepared: Prepared<T>,
        statuses: &[u16],
        url: &Url,
    ) -> Result<Reply<T>, Error> {
        let result = self
            .client
            .execute(prepared, statuses, Some(&self.handle))
            .await;
        if let Err(Error::Server(error)) = &result
            && let Some(doc) = &error.document
            && doc.statement_handle.as_ref() == Some(&self.handle)
        {
            self.remember_status(doc, &error.meta, url)?;
        }
        result
    }
    fn state(&self, reply: Reply<StatementBody>, url: &Url) -> Result<StatementState, Error> {
        match reply.value {
            StatementBody::Pending(value) => {
                self.remember_status(&value, &reply.meta, url)?;
                Ok(StatementState::Pending(Reply {
                    value,
                    meta: reply.meta,
                }))
            }
            StatementBody::Complete(value) => {
                self.correlate(value.status.statement_handle.as_ref(), &reply.meta)?;
                let first = FirstResult::new(
                    self.clone(),
                    Reply {
                        value,
                        meta: reply.meta,
                    },
                    url,
                )?;
                self.remember_status(&first.response().value.status, &first.response().meta, url)?;
                Ok(StatementState::Complete(first))
            }
        }
    }
    /// Explicit remote cancellation POST, exactly once. Local stop never calls this endpoint.
    /// Returns the documented 000604/57014 acknowledgement; use cancel_and_wait to observe a race.
    pub async fn cancel(&self, request_id: RequestId) -> Result<Reply<ServerMessage>, Error> {
        let prepared = self
            .client
            .api
            .statement(self.handle.clone())
            .cancel(request_id)
            .prepare()?;
        let url = prepared.request().url().clone();
        let reply = self.execute(prepared, &[200], &url).await?;
        self.correlate(reply.value.statement_handle.as_ref(), &reply.meta)?;
        if reply.value.code != "000604" || reply.value.sql_state.as_deref() != Some("57014") {
            return Err(Error::protocol(
                ProtocolError::UnexpectedCancellation,
                Some(&reply.meta),
            ));
        }
        self.remember_status(&reply.value, &reply.meta, &url)?;
        Ok(reply)
    }
}
