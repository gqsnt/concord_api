//! The public client, its construction and the read operations.

use std::fmt;

use concord::csv::{CsvOptions, CsvReader, HeaderMode};
use concord::jsonl::{JsonLinesOptions, JsonLinesReader};
use concord::reqwest::{self, header::HeaderValue};
use concord::stream::ByteSource;
use concord::{Endpoint, IgnoreContentType, Incoming, Received, Text};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::{
    Error, LateException, LateSource, ResponseError, ResponseProblem, ServerException, code_of,
    name_of,
};
use crate::format::OutputFormat;
use crate::guard::{BodyException, Guarded, MAX_BLOCK_BYTES};
use crate::insert::Insert;
use crate::meta::ResponseMeta;
use crate::query::{Identifier, Query, QueryId, RequestError, Table, is_token};
use crate::rows::{Reader, Rows};
use crate::wire::ClickHouseHttp;

/// The most bytes of a non-2xx body (the server's exception text) the SDK reads.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// How requests authenticate. `Debug` never prints the password.
#[derive(Clone)]
pub struct Credentials(Auth);

#[derive(Clone)]
enum Auth {
    None,
    Basic(HeaderValue),
    Headers { user: String, key: String },
}

impl Credentials {
    /// HTTP Basic authentication (`Authorization: Basic base64(user:password)`). reqwest drops
    /// `Authorization` when a redirect leaves the origin.
    ///
    /// # Errors
    ///
    /// [`RequestError::InvalidCredentials`] if `user` contains `:` or a control character, or
    /// `password` a control character.
    pub fn basic(user: &str, password: &str) -> Result<Self, RequestError> {
        concord_auth::basic(user, password)
            .map(|value| Self(Auth::Basic(value)))
            .map_err(|_| RequestError::InvalidCredentials)
    }

    /// The documented headers `X-ClickHouse-User` and `X-ClickHouse-Key`. A custom credential
    /// header is **not** dropped by reqwest on a cross-origin redirect: use a `reqwest::Client`
    /// whose redirect policy refuses them ([`ClickHouse::recommended_http_client`]).
    ///
    /// # Errors
    ///
    /// [`RequestError::InvalidCredentials`] if a value cannot be a header value.
    pub fn headers(user: &str, password: &str) -> Result<Self, RequestError> {
        let valid = |s: &str| HeaderValue::from_str(s).is_ok();
        if !valid(user) || !valid(password) {
            return Err(RequestError::InvalidCredentials);
        }
        Ok(Self(Auth::Headers {
            user: user.to_owned(),
            key: password.to_owned(),
        }))
    }

    /// No credentials: the server uses its `default` user with an empty password.
    pub fn none() -> Self {
        Self(Auth::None)
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Auth::None => f.write_str("Credentials::None"),
            Auth::Basic(_) => f.write_str("Credentials::Basic(<redacted>)"),
            Auth::Headers { user, .. } => f
                .debug_struct("Credentials::Headers")
                .field("user", user)
                .field("key", &"<redacted>")
                .finish(),
        }
    }
}

/// The caller's per-call context: the default database and the quota key of a request. Applied
/// with `.context(&ctx)`; a field left `None` keeps the client's value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallContext {
    database: Option<Identifier>,
    quota_key: Option<String>,
}

impl CallContext {
    /// An empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// The request's default database (`X-ClickHouse-Database`).
    #[must_use]
    pub fn database(mut self, database: Identifier) -> Self {
        self.database = Some(database);
        self
    }

    /// The quota key (`quota_key` URL parameter, "any string" without control characters).
    ///
    /// # Errors
    ///
    /// [`RequestError::InvalidToken`].
    pub fn quota_key(mut self, key: impl Into<String>) -> Result<Self, RequestError> {
        let key = key.into();
        if !is_token(&key) {
            return Err(RequestError::InvalidToken { what: "quota_key" });
        }
        self.quota_key = Some(key);
        Ok(self)
    }

    pub(crate) fn apply(&self, mut api: ClickHouseHttp) -> ClickHouseHttp {
        if let Some(database) = &self.database {
            api = api.set_database(database.clone());
        }
        if let Some(key) = &self.quota_key {
            api = api.set_quota_key(key.clone());
        }
        api
    }
}

/// A ClickHouse server's HTTP interface.
///
/// Cheap to clone (one reference count); every operation takes `&self`.
///
/// ```
/// use concord_sdk_clickhouse::{ClickHouse, Credentials, Identifier};
///
/// let http = ClickHouse::recommended_http_client()?;
/// let credentials = Credentials::basic("analyst", "fixture-password")?;
/// let clickhouse = ClickHouse::new(http, credentials)
///     .with_base_url("http://127.0.0.1:8123")?
///     .with_database(Identifier::new("default")?);
/// assert!(!format!("{clickhouse:?}").contains("fixture-password"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
pub struct ClickHouse {
    api: ClickHouseHttp,
}

impl ClickHouse {
    /// A client of the server at `http://localhost:8123` (the documented default port) using
    /// `http` for every request; change the address with [`with_base_url`](Self::with_base_url).
    pub fn new(http: reqwest::Client, credentials: Credentials) -> Self {
        let api = ClickHouseHttp::new(http);
        let api = match credentials.0 {
            Auth::None => api,
            Auth::Basic(value) => api.set_authorization(value),
            Auth::Headers { user, key } => api.set_user(user).set_key(key),
        };
        Self { api }
    }

    /// A `reqwest::Client` that follows no redirect: the credentials are only ever sent to the
    /// configured server. Timeouts are the application's to add (`reqwest::ClientBuilder`).
    ///
    /// # Errors
    ///
    /// reqwest's, if the client cannot be built (TLS backend initialisation).
    pub fn recommended_http_client() -> Result<reqwest::Client, reqwest::Error> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
    }

    /// The server's address (`https://host:8443`, a proxy path).
    ///
    /// # Errors
    ///
    /// Concord's `Build` error for a URL that is not `http`/`https` with a host.
    pub fn with_base_url(self, url: &str) -> Result<Self, Error> {
        Ok(Self {
            api: self.api.with_base_url(url)?,
        })
    }

    /// The default database of every request (`X-ClickHouse-Database`), unless a call's
    /// [`CallContext`] names another.
    #[must_use]
    pub fn with_database(self, database: Identifier) -> Self {
        Self {
            api: self.api.set_database(database),
        }
    }

    /// A read-only query whose rows arrive in `format` (`GET`, so the server refuses a statement
    /// that modifies data).
    pub fn select(&self, query: Query, format: OutputFormat) -> Select {
        Select {
            api: self.api.clone(),
            query,
            format,
            query_id: None,
            wait_end_of_query: false,
            progress: false,
        }
    }

    /// An insert into `table`; choose the body with
    /// [`json_each_row`](Insert::json_each_row) or [`csv_with_names`](Insert::csv_with_names).
    pub fn insert(&self, table: Table) -> Insert {
        Insert::new(self.api.clone(), table)
    }

    /// Asks the server to stop the queries with this ID: `KILL QUERY WHERE query_id =
    /// {id:String} ASYNC` (the ID is a bound value, not SQL text). Returns the server's rows
    /// (`kill_status`, `query_id`), possibly none; `ASYNC` does not wait for the query to stop.
    ///
    /// Stopping to read (dropping [`Rows`]) does not cancel the query on the server; this does,
    /// within the server's rules (a read-only user can stop only its own queries).
    ///
    /// # Errors
    ///
    /// [`Error::Server`] when the server refuses (with its default `kill_throw_if_noop`, an ID
    /// that matches nothing is the `NOTHING_TO_KILL` exception), and the errors of a query.
    pub async fn cancel(&self, query_id: &QueryId) -> Result<Vec<KillStatus>, Error> {
        let query = Query::new("KILL QUERY WHERE query_id = {id:String} ASYNC")
            .bind("id", query_id.as_str())?;
        let params = query.url_params()?;
        let endpoint = self
            .api
            .command(query.sql().to_owned(), OutputFormat::JsonEachRow)
            .set_params(params);
        let operation = endpoint_name::<Incoming, _>(&endpoint);
        let received = endpoint.prepare()?.receive().await?;
        let rows = open_rows(received, OutputFormat::JsonEachRow, operation).await?;
        Ok(rows.collect().await?.rows)
    }

    /// `GET /ping`: `Ok(())` when the server answers 2xx.
    ///
    /// # Errors
    ///
    /// [`Error::Server`] for another status; [`Error::Concord`] for a transport failure.
    pub async fn ping(&self) -> Result<(), Error> {
        let endpoint = self.api.ping();
        let operation = endpoint_name::<String, _>(&endpoint);
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            return Err(server_exception(received, operation).await);
        }
        received.decode().await?;
        Ok(())
    }
}

impl fmt::Debug for ClickHouse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ClickHouse").field(&self.api).finish()
    }
}

/// One row of `KILL QUERY`'s answer.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct KillStatus {
    /// `finished`, `waiting`, or the reason the query cannot be stopped.
    pub kill_status: String,
    /// The query's ID.
    pub query_id: String,
}

/// A prepared `SELECT`: options, then [`rows`](Self::rows).
#[derive(Clone)]
#[must_use = "a select does nothing until `rows()` is awaited"]
pub struct Select {
    api: ClickHouseHttp,
    query: Query,
    format: OutputFormat,
    query_id: Option<QueryId>,
    wait_end_of_query: bool,
    progress: bool,
}

impl Select {
    /// The query ID (`query_id`); without one the server assigns it.
    pub fn query_id(mut self, id: QueryId) -> Self {
        self.query_id = Some(id);
        self
    }

    /// The caller's context (database, quota key) for this call.
    pub fn context(mut self, context: &CallContext) -> Self {
        self.api = context.apply(self.api);
        self
    }

    /// `wait_end_of_query=1`: the server buffers the whole result (in memory up to
    /// `http_response_buffer_size`, then in a temporary file) and sends the head only when the
    /// query has finished, so most execution errors become a non-2xx [`Error::Server`] instead of
    /// a late exception. Not a guarantee: the HTTP interface documents that this "doesn't
    /// completely solve the problem", and that progress headers can interfere with the delay. The
    /// SDK checks the body for a late exception either way. Costs: no row before the query
    /// ends, and server memory or disk for the whole result.
    pub fn wait_end_of_query(mut self, yes: bool) -> Self {
        self.wait_end_of_query = yes;
        self
    }

    /// `send_progress_in_http_headers=1`: `X-ClickHouse-Progress` headers before the body
    /// ([`ResponseMeta::progress`]). The HTTP interface page notes that they can interfere with
    /// the header delay of [`wait_end_of_query`](Self::wait_end_of_query); with them, an
    /// exception can also be reported by `X-ClickHouse-Exception-Code` on a 2xx head
    /// ([`Error::LateException`] with [`LateSource::Head`](crate::LateSource::Head) or a block).
    pub fn progress_headers(mut self, yes: bool) -> Self {
        self.progress = yes;
        self
    }

    /// Sends the query and opens its rows. Nothing of the body is read yet.
    ///
    /// # Errors
    ///
    /// [`Error::Request`] (an unbound placeholder, nothing sent), [`Error::Concord`] (build or
    /// transport), [`Error::Server`] (non-2xx), [`Error::LateException`] (a 2xx head reporting an
    /// exception), [`Error::Response`] (another format, a refused `Content-Type`).
    pub async fn rows<T: DeserializeOwned>(self) -> Result<Rows<T>, Error> {
        let params = self.query.url_params()?;
        let endpoint = self
            .api
            .select(self.query.sql().to_owned(), self.format)
            .maybe_query_id(self.query_id)
            .maybe_wait_end_of_query(self.wait_end_of_query.then_some(1))
            .maybe_progress(self.progress.then_some(1))
            .set_params(params);
        let operation = endpoint_name::<Incoming, _>(&endpoint);
        let received = endpoint.prepare()?.receive().await?;
        open_rows(received, self.format, operation).await
    }
}

impl fmt::Debug for Select {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Select")
            .field("query", &self.query)
            .field("format", &self.format)
            .field("query_id", &self.query_id)
            .field("wait_end_of_query", &self.wait_end_of_query)
            .field("progress", &self.progress)
            .finish()
    }
}

/// The declared name of an endpoint (`ClickHouseHttp.select`), without building a request.
pub(crate) fn endpoint_name<T, E: Endpoint<T>>(_: &E) -> &'static str {
    E::operation().name()
}

/// A non-2xx answer: the exception text (bounded), its code and name.
pub(crate) async fn server_exception<T>(received: Received<T>, operation: &'static str) -> Error {
    // Decode exception text as UTF-8 regardless of the advertised media or charset.
    let (meta, document) = received
        .document::<IgnoreContentType<Text>, String>(ERROR_BODY_LIMIT)
        .await;
    let meta = ResponseMeta::new(meta);
    let status = meta.status();
    let message = document
        .ok()
        .map(|text| text.trim_end().to_owned())
        .filter(|text| !text.is_empty());
    let code = meta
        .exception_code()
        .or_else(|| message.as_deref().and_then(code_of));
    let name = message.as_deref().and_then(name_of);
    Error::Server(Box::new(ServerException {
        operation,
        status,
        code,
        name,
        message,
        meta,
    }))
}

/// A 2xx head carrying `X-ClickHouse-Exception-Code`: the body holds the exception (a tagged
/// block, or plain text from a server without tags), read up to the block bound.
pub(crate) async fn head_exception<S: ByteSource>(
    body: S,
    operation: &'static str,
    meta: ResponseMeta,
) -> Error {
    let mut guard = Guarded::new(body, meta.exception_tag());
    let mut text = Vec::new();
    loop {
        match guard.next_chunk().await {
            Ok(Some(chunk)) => {
                let room = MAX_BLOCK_BYTES.saturating_sub(text.len());
                text.extend_from_slice(&chunk[..chunk.len().min(room)]);
                if room <= chunk.len() {
                    break;
                }
            }
            Ok(None) => break,
            Err(error) => {
                if let Some(block) = error
                    .source_unredacted()
                    .and_then(|source| source.downcast_ref::<BodyException>())
                {
                    return Error::LateException(Box::new(LateException::from_block(
                        operation, block, 0, meta,
                    )));
                }
                break;
            }
        }
    }
    let message = String::from_utf8_lossy(&text).trim().to_owned();
    Error::LateException(Box::new(LateException {
        operation,
        code: meta.exception_code(),
        name: name_of(&message),
        message,
        source: LateSource::Head,
        rows_before: 0,
        meta,
    }))
}

fn refused(operation: &'static str, problem: ResponseProblem, meta: ResponseMeta) -> Error {
    Error::Response(Box::new(ResponseError {
        operation,
        problem,
        meta,
    }))
}

/// Status, format and head checks, then the rows reader over the guarded body.
pub(crate) async fn open_rows<T: DeserializeOwned>(
    received: Received<Incoming>,
    format: OutputFormat,
    operation: &'static str,
) -> Result<Rows<T>, Error> {
    if !received.status().is_success() {
        return Err(server_exception(received, operation).await);
    }
    let meta = ResponseMeta::new(received.meta());
    if let Some(served) = meta.format()
        && !served.eq_ignore_ascii_case(format.name())
    {
        let problem = ResponseProblem::FormatMismatch {
            requested: format.name(),
            served: served.to_owned(),
        };
        return Err(refused(operation, problem, meta));
    }
    let incoming = received.into_incoming()?;
    if meta.exception_code().is_some() {
        return Err(head_exception(incoming, operation, meta).await);
    }
    // The guard cannot recognize a block without a nonempty tag. Keep the completion
    // classification consistent with the guard, including malformed empty headers.
    let marked = meta.exception_tag().is_some_and(|tag| !tag.is_empty());
    // Each reader checks the head as its `open` would; it then reads the guarded body (the
    // `Incoming` itself is the guard's source), naming the operation in its errors.
    let reader = match format {
        OutputFormat::CsvWithNames => {
            let csv = CsvReader::new(CsvOptions::new(HeaderMode::Present, b','))
                .expect("the CSVWithNames dialect is valid");
            if let Err(e) = csv.check_head(&incoming.head()) {
                let problem = ResponseProblem::ContentType { label: e.label() };
                return Err(refused(operation, problem, meta));
            }
            let body = Guarded::new(incoming, meta.exception_tag());
            Reader::Csv(Box::new(csv.read_for(body, operation)))
        }
        OutputFormat::JsonEachRow => {
            let json = JsonLinesReader::new(JsonLinesOptions::ndjson());
            if let Err(e) = json.check_head(&incoming.head()) {
                let problem = ResponseProblem::ContentType { label: e.label() };
                return Err(refused(operation, problem, meta));
            }
            let body = Guarded::new(incoming, meta.exception_tag());
            Reader::Json(Box::new(json.read_for(body, operation)))
        }
    };
    Ok(Rows {
        reader,
        operation,
        format,
        marked,
        meta,
    })
}
