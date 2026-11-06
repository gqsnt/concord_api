//! Read-only Linear issue queries. See the README for response and completion policy.
use concord::{
    CodecError, Encode, Json, Prepared,
    reqwest::{
        self,
        header::{AUTHORIZATION, HeaderMap, HeaderValue},
    },
};
use concord_pagination::{Page, Pages, Resume};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

const LIST: &str = "query Issues($first: Int!, $after: String) { issues(first: $first, after: $after) { nodes { id title description dueDate } pageInfo { hasNextPage endCursor } } }";
const ISSUE: &str = "query Issue($id: String!) { issue(id: $id) { id title description dueDate } }";

/// A request body for a fixed SDK query and its variables.
#[derive(Clone, Debug, Serialize)]
pub struct GraphqlRequest<V> {
    query: &'static str,
    #[serde(rename = "operationName")]
    operation_name: &'static str,
    variables: V,
}
/// Encodes a fixed query and variables as JSON.
pub struct Graphql;
impl<V: Serialize> Encode<GraphqlRequest<V>> for Graphql {
    const CONTENT_TYPE: Option<&'static str> = Some("application/json");
    fn encode(value: GraphqlRequest<V>) -> Result<reqwest::Body, CodecError> {
        serde_json::to_vec(&value)
            .map(reqwest::Body::from)
            .map_err(|e| CodecError::new("GraphQL request encoding failed").with_source(e))
    }
}
/// Variables for the issue connection query.
#[derive(Clone, Debug, Serialize)]
pub struct ListVariables {
    first: u32,
    after: Option<String>,
}
/// Variables for the issue lookup query.
#[derive(Clone, Debug, Serialize)]
pub struct IssueVariables {
    id: String,
}

concord::api! {
    /// Wire operations; applications normally use [`Linear`].
    pub client LinearApi(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "https://api.linear.app";
        path "graphql";
        request Graphql;
        response Json;
        /// A page of issues.
        POST issues(body request: GraphqlRequest<ListVariables>) -> Envelope<IssuesData>;
        /// One issue.
        POST issue(body request: GraphqlRequest<IssueVariables>) -> Envelope<IssueData>;
    }
}

fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// A selected issue. Description and due date are nullable, including on a partial GraphQL
/// response; only an error `path` ending at the field tells a failed field from a null value.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Issue {
    /// Linear's identifier.
    pub id: String,
    /// Issue title.
    pub title: String,
    /// Selected nullable description.
    #[serde(deserialize_with = "required_nullable")]
    pub description: Option<String>,
    /// Selected nullable `dueDate` (`TimelessDate`), passed through as returned, not parsed.
    #[serde(rename = "dueDate", deserialize_with = "required_nullable")]
    pub due_date: Option<String>,
}
/// GraphQL issue connection.
#[derive(Clone, Debug, Deserialize)]
pub struct Connection {
    /// Issues in this page.
    pub nodes: Vec<Issue>,
    /// The continuation information.
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
}
/// Forward pagination state.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// Whether another page exists.
    pub has_next_page: bool,
    /// Cursor for the next request, if available.
    #[serde(deserialize_with = "required_nullable")]
    pub end_cursor: Option<String>,
}
/// Issue connection query data.
#[derive(Clone, Debug, Deserialize)]
pub struct IssuesData {
    /// The requested connection.
    pub issues: Connection,
}
/// Issue query data.
#[derive(Clone, Debug, Deserialize)]
pub struct IssueData {
    /// The issue, if Linear returned one.
    #[serde(deserialize_with = "required_nullable")]
    pub issue: Option<Issue>,
}
/// A GraphQL diagnostic, including its provider-defined extension data.
#[derive(Clone, Debug, Deserialize)]
pub struct GraphqlError {
    /// Human-readable provider message.
    pub message: String,
    /// Path of the failed field, including list indices.
    #[serde(default)]
    pub path: Option<Vec<Value>>,
    /// Source locations, if supplied.
    #[serde(default)]
    pub locations: Option<Vec<Value>>,
    /// Provider-defined diagnostics, including codes.
    #[serde(default)]
    pub extensions: Option<Value>,
    /// Other provider fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
/// GraphQL envelope. Data and errors are retained together even on HTTP 200.
#[derive(Clone, Debug, Deserialize)]
pub struct Envelope<T> {
    /// Typed data; absent and null are both `None`.
    pub data: Option<T>,
    /// GraphQL errors (empty if omitted).
    #[serde(default)]
    pub errors: Vec<GraphqlError>,
    /// Top-level provider extensions.
    #[serde(default)]
    pub extensions: Option<Value>,
}
/// HTTP response metadata.
#[derive(Clone, Debug)]
pub struct ResponseMeta {
    /// HTTP status.
    pub status: reqwest::StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Concord operation identity.
    pub operation: Option<concord::Operation>,
}
/// A decoded GraphQL answer and its HTTP metadata.
#[derive(Clone, Debug)]
pub struct Response<T> {
    /// Data, errors, and extensions.
    pub envelope: Envelope<T>,
    /// HTTP metadata.
    pub meta: ResponseMeta,
}
impl<T> Response<T> {
    /// True only when data exists and there are no GraphQL errors.
    pub fn is_success(&self) -> bool {
        self.envelope.data.is_some() && self.envelope.errors.is_empty()
    }
}
/// Request or connection failure. HTTP and decode failures retain response metadata when received.
#[derive(Debug)]
pub enum Error {
    /// Request construction or transport failure before the response head.
    Concord(concord::Error),
    /// Non-success HTTP status, with bounded raw body and optional GraphQL diagnostic envelope.
    Http {
        /// Response head.
        meta: Box<ResponseMeta>,
        /// Response bytes (at most 64 KiB).
        body: Vec<u8>,
        /// Diagnostics if the body was a GraphQL envelope.
        errors: Vec<GraphqlError>,
    },
    /// Body read, content-type, size, or decoding failure after the head.
    Response {
        /// Response head.
        meta: Box<ResponseMeta>,
        /// Concord's error, including an unread response for metadata failures.
        source: concord::Error,
    },
    /// A connection response had no usable data or had invalid continuation information.
    Page {
        /// The entire response, including any partial data and diagnostics.
        response: Box<Response<IssuesData>>,
        /// Reason the page could not be followed.
        reason: &'static str,
    },
    /// An invalid client or operation input.
    Invalid(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(e) => write!(f, "Linear request: {e}"),
            Self::Http { meta, .. } => write!(f, "Linear HTTP {}", meta.status),
            Self::Response { source, .. } => write!(f, "Linear response: {source}"),
            Self::Page { reason, .. } | Self::Invalid(reason) => write!(f, "Linear: {reason}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<concord::Error> for Error {
    fn from(e: concord::Error) -> Self {
        Self::Concord(e)
    }
}

const BODY_LIMIT: usize = 1024 * 1024;
async fn send<T: serde::de::DeserializeOwned>(
    prepared: Result<Prepared<Envelope<T>>, concord::Error>,
) -> Result<Response<T>, Error> {
    let received = prepared?.receive().await?;
    let meta = ResponseMeta {
        status: received.status(),
        headers: received.head().headers().clone(),
        operation: received.operation(),
    };
    if !meta.status.is_success() {
        let body = received
            .bytes_limited(65536)
            .await
            .map_err(|source| Error::Response {
                meta: Box::new(meta.clone()),
                source,
            })?
            .to_vec();
        let errors = serde_json::from_slice::<Envelope<Value>>(&body)
            .map(|e| e.errors)
            .unwrap_or_default();
        return Err(Error::Http {
            meta: Box::new(meta),
            body,
            errors,
        });
    }
    let envelope = received
        .decode_limited(BODY_LIMIT)
        .await
        .map_err(|source| Error::Response {
            meta: Box::new(meta.clone()),
            source,
        })?;
    Ok(Response { envelope, meta })
}

/// An authenticated Linear client over a caller-configured HTTP transport.
#[derive(Clone, Debug)]
pub struct Linear {
    api: LinearApi,
}
impl Linear {
    /// Construct with a personal API key, which is sent verbatim in Authorization.
    pub fn api_key(http: reqwest::Client, key: &str) -> Result<Self, Error> {
        Self::authorization(http, key)
    }
    /// Construct with an OAuth access token, sent with the Bearer scheme.
    pub fn oauth(http: reqwest::Client, token: &str) -> Result<Self, Error> {
        Self::authorization(http, &format!("Bearer {token}"))
    }
    fn authorization(http: reqwest::Client, value: &str) -> Result<Self, Error> {
        if value.is_empty() || value == "Bearer " {
            return Err(Error::Invalid("empty authorization"));
        }
        let mut authorization = HeaderValue::from_str(value)
            .map_err(|_| Error::Invalid("invalid authorization header"))?;
        authorization.set_sensitive(true);
        Ok(Self {
            api: LinearApi::new(http, LinearApiConfig { authorization }),
        })
    }
    /// Replace the base URL, for example with a loopback fixture server.
    pub fn with_base_url(mut self, url: &str) -> Result<Self, Error> {
        self.api = self.api.with_base_url(url)?;
        Ok(self)
    }
    /// Fetch selected fields for a single issue ID or identifier.
    pub async fn issue(&self, id: &str) -> Result<Response<IssueData>, Error> {
        if id.is_empty() {
            return Err(Error::Invalid("empty issue id"));
        }
        send(
            self.api
                .issue(GraphqlRequest {
                    query: ISSUE,
                    operation_name: "Issue",
                    variables: IssueVariables { id: id.to_owned() },
                })
                .prepare(),
        )
        .await
    }
    /// Fetch one issue connection page. This SDK restricts `first` to 1..=250.
    pub async fn issue_page(
        &self,
        first: u32,
        after: Option<String>,
    ) -> Result<Page<Issue, String, PageMeta>, Error> {
        if !(1..=250).contains(&first) {
            return Err(Error::Invalid("first must be between 1 and 250"));
        }
        let response = send(
            self.api
                .issues(GraphqlRequest {
                    query: LIST,
                    operation_name: "Issues",
                    variables: ListVariables {
                        first,
                        after: after.clone(),
                    },
                })
                .prepare(),
        )
        .await?;
        let reason = match &response.envelope.data {
            None => Some("connection data missing"),
            Some(data)
                if data.issues.page_info.has_next_page
                    && data
                        .issues
                        .page_info
                        .end_cursor
                        .as_deref()
                        .is_none_or(str::is_empty) =>
            {
                Some("next page has no cursor")
            }
            Some(data)
                if data.issues.page_info.has_next_page
                    && data.issues.page_info.end_cursor == after =>
            {
                Some("cursor did not advance")
            }
            _ => None,
        };
        if let Some(reason) = reason {
            return Err(Error::Page {
                response: Box::new(response),
                reason,
            });
        }
        let Response { envelope, meta } = response;
        let data = envelope.data.expect("checked above");
        let next = if data.issues.page_info.has_next_page {
            data.issues.page_info.end_cursor
        } else {
            None
        };
        Ok(Page {
            items: data.issues.nodes,
            next,
            meta: PageMeta {
                http: meta,
                errors: envelope.errors,
                extensions: envelope.extensions,
            },
        })
    }
    /// Traverse issue pages from the start or a saved cursor. Inspect every page's diagnostics.
    /// A page cap can be set with `max_pages`; its error includes a resume point.
    pub fn issues(
        &self,
        first: u32,
        from: Resume<String>,
    ) -> Pages<Issue, String, PageMeta, Error> {
        let client = self.clone();
        Pages::new(from, move |after| {
            let client = client.clone();
            async move { client.issue_page(first, after).await }
        })
    }
}
/// Diagnostics and HTTP metadata retained alongside a connection page.
#[derive(Clone, Debug)]
pub struct PageMeta {
    /// Response head.
    pub http: ResponseMeta,
    /// GraphQL errors: a nonempty list means the page is partial, not application success.
    pub errors: Vec<GraphqlError>,
    /// Provider extensions.
    pub extensions: Option<Value>,
}
