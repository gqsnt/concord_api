#![doc = include_str!("../README.md")]

use concord::{Endpoint, Json, SendWith, reqwest::header::AUTHORIZATION};
use concord_pagination::{Page, Pages, Resume};
use serde::Deserialize;
use std::{collections::BTreeMap, fmt, num::NonZeroUsize};

pub use concord::ResponseMeta;
pub use concord_pagination as pagination;

/// Maximum response size for one successful JSON page, in bytes.
pub const SUCCESS_BODY_LIMIT: usize = 16 * 1024 * 1024;
/// Maximum bytes retained from an error response.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// A record's user-defined fields are arbitrary JSON values. Missing keys are not defaults:
/// Airtable omits empty values, so absence does not establish a schema or a value.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Record {
    /// Record identifier.
    pub id: String,
    /// Provider creation timestamp, if supplied.
    #[serde(rename = "createdTime")]
    pub created_time: Option<String>,
    /// Field names and values as returned by the provider.
    pub fields: BTreeMap<String, serde_json::Value>,
}

/// One decoded list response. An absent offset means provider exhaustion.
#[derive(Clone, Debug, Deserialize)]
pub struct RecordPage {
    /// Records in provider order.
    pub records: Vec<Record>,
    /// Opaque continuation, if another page exists.
    #[serde(default, deserialize_with = "present_offset")]
    pub offset: Option<String>,
}

fn present_offset<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

/// List options applied unchanged to every page. Formula evaluation is Airtable's responsibility.
///
/// `Debug` withholds the formula, which the declaration sends as a `#[sensitive]` query value.
#[derive(Clone, Default)]
pub struct ListOptions {
    /// Optional filter expression passed verbatim as `filterByFormula`.
    pub filter_by_formula: Option<String>,
    /// Selected field names or field IDs, repeated as `fields[]`; empty means no selection
    /// parameter. Response keys are field names either way.
    pub fields: Vec<String>,
    /// Optional page size, from 1 through 100 inclusive.
    pub page_size: Option<u8>,
    /// Optional total record limit imposed by Airtable.
    pub max_records: Option<u32>,
    /// Optional view name or ID.
    pub view: Option<String>,
}

impl fmt::Debug for ListOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListOptions")
            .field(
                "filter_by_formula",
                &self.filter_by_formula.as_ref().map(|_| "<redacted>"),
            )
            .field("fields", &self.fields)
            .field("page_size", &self.page_size)
            .field("max_records", &self.max_records)
            .field("view", &self.view)
            .finish()
    }
}

/// Successful value and the response head, including status, headers and operation identity.
#[derive(Debug)]
pub struct Response<T> {
    /// Decoded page.
    pub value: T,
    /// Original response metadata.
    pub meta: ResponseMeta,
}

/// Error payload supplied by Airtable. Unknown error type strings are retained.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProviderError {
    /// Structured form, including optional message.
    Object {
        /// Provider error type string.
        #[serde(rename = "type")]
        kind: String,
        /// Provider explanation, if supplied.
        message: Option<String>,
    },
    /// The documented string form.
    String(String),
}
impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProviderError(<withheld>)")
    }
}
#[derive(Deserialize)]
struct Envelope {
    error: ProviderError,
}

/// Failures retain the HTTP head after it arrives, even when a body is invalid or interrupted.
pub enum Error {
    /// Invalid local input; no request was made.
    InvalidInput(&'static str),
    /// Construction, transport, or success body failure.
    Concord {
        /// Underlying error.
        source: concord::Error,
        /// Response metadata when a head arrived.
        meta: Option<Box<ResponseMeta>>,
    },
    /// Non-success HTTP status with a bounded response body.
    Http {
        /// Actual status, headers and operation.
        meta: Box<ResponseMeta>,
        /// Parsed provider error when the body matches a documented shape.
        provider: Option<ProviderError>,
        /// Complete raw body if read within the limit. It can contain user data.
        body: Option<concord::bytes::Bytes>,
        /// Transfer or size failure, if the body could not be read completely.
        body_error: Option<concord::Error>,
    },
    /// A present but empty continuation is invalid, not exhaustion.
    EmptyOffset {
        /// Metadata from the offending response.
        meta: Box<ResponseMeta>,
    },
}
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => f.debug_tuple("InvalidInput").field(message).finish(),
            Self::Concord { source, meta } => f
                .debug_struct("Concord")
                .field("source", source)
                .field("meta", meta)
                .finish(),
            Self::Http {
                meta,
                body,
                body_error,
                ..
            } => f
                .debug_struct("Http")
                .field("meta", meta)
                .field("body_len", &body.as_ref().map(|b| b.len()))
                .field("body_error", body_error)
                .finish_non_exhaustive(),
            Self::EmptyOffset { meta } => {
                f.debug_struct("EmptyOffset").field("meta", meta).finish()
            }
        }
    }
}
impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let meta = source.head_meta().map(Box::new);
        Self::Concord { source, meta }
    }
}
impl Error {
    /// Response metadata when headers were received.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::InvalidInput(_) => None,
            Self::Concord { meta, .. } => meta.as_deref(),
            Self::Http { meta, .. } | Self::EmptyOffset { meta } => Some(meta),
        }
    }
    /// Parsed provider error if available.
    pub fn provider(&self) -> Option<&ProviderError> {
        match self {
            Self::Http { provider, .. } => provider.as_ref(),
            _ => None,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(s) => write!(f, "invalid input: {s}"),
            Self::Concord { source, .. } => source.fmt(f),
            Self::Http { meta, .. } => write!(f, "Airtable HTTP {}", meta.status()),
            Self::EmptyOffset { .. } => f.write_str("Airtable returned an empty offset"),
        }
    }
}
impl std::error::Error for Error {}

concord::api! {
    pub(crate) client AirtableApi(#[sensitive] token: String) {
        base "https://api.airtable.com";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;
        limit SUCCESS_BODY_LIMIT;
        send Policy;
        GET list(base: String, table: String,
            query("fields[]") each fields: Vec<String> = Vec::new(),
            #[sensitive] query("filterByFormula") filter?: String,
            #[sensitive] query offset?: String,
            query("pageSize") page_size?: u8,
            query("maxRecords") max_records?: u32,
            query view?: String,
        ) -> RecordPage { path "v0/{base}/{table}"; }
    }
}

#[derive(Default)]
struct Policy;
impl SendWith<RecordPage> for Policy {
    type Output = Result<Response<RecordPage>, Error>;
    async fn send<E: Endpoint<RecordPage> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            // Keep the complete raw body even when the provider document decodes.
            let meta = received.meta();
            let (body, body_error) = match received.bytes_limited(ERROR_BODY_LIMIT).await {
                Ok(bytes) => (Some(bytes), None),
                Err(error) => (None, Some(error)),
            };
            let provider = body
                .as_ref()
                .and_then(|b| serde_json::from_slice::<Envelope>(b).ok())
                .map(|e| e.error);
            return Err(Error::Http {
                meta: Box::new(meta),
                provider,
                body,
                body_error,
            });
        }
        let (value, meta) = received.decode_with_meta().await?;
        if value.offset.as_deref() == Some("") {
            return Err(Error::EmptyOffset {
                meta: Box::new(meta),
            });
        }
        Ok(Response { value, meta })
    }
}

#[cfg(test)]
mod limit_tests {
    use super::*;
    use concord_example_testserver::{Reply, TestServer};

    #[tokio::test]
    async fn native_send_keeps_the_declared_success_body_limit() {
        let server =
            TestServer::start(|_| Reply::json(200, vec![b' '; SUCCESS_BODY_LIMIT + 1])).await;
        let api = AirtableApi::new(
            concord::reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap(),
            AirtableApiConfig {
                token: "synthetic-token".into(),
            },
        )
        .with_base_url(server.url())
        .unwrap();
        let error = api
            .list("app", "table")
            .prepare()
            .unwrap()
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.kind(), concord::ErrorKind::Limit);
        assert_eq!(error.head().unwrap().status().as_u16(), 200);
        server.shutdown().await;
    }
}

/// Lazy page traversal, with per-page HTTP metadata. `items()` exposes item traversal and streaming.
pub type RecordPages = Pages<Record, String, ResponseMeta, Error>;

/// Airtable client. The provided HTTP client controls timeouts and redirect policy.
#[derive(Clone, Debug)]
pub struct Client {
    api: AirtableApi,
}
impl Client {
    /// Construct with an existing HTTP client and a raw bearer token (without `Bearer `).
    pub fn new(http: concord::reqwest::Client, token: impl Into<String>) -> Self {
        Self {
            api: AirtableApi::new(
                http,
                AirtableApiConfig {
                    token: token.into(),
                },
            ),
        }
    }
    /// Replace the trusted API origin, e.g. for offline testing. This destination receives the token.
    pub fn with_base_url(self, base: impl AsRef<str>) -> Result<Self, concord::Error> {
        Ok(Self {
            api: self.api.with_base_url(base)?,
        })
    }
    /// Fetch one page. Use the same base, table and options when continuing an offset.
    pub async fn list_page(
        &self,
        base: &str,
        table: &str,
        options: &ListOptions,
        offset: Option<&str>,
    ) -> Result<Response<RecordPage>, Error> {
        if [base, table]
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == "..")
        {
            return Err(Error::InvalidInput(
                "base and table must be nonempty non-dot segments",
            ));
        }
        if options.page_size.is_some_and(|n| !(1..=100).contains(&n)) {
            return Err(Error::InvalidInput("page size must be 1 through 100"));
        }
        if options.max_records == Some(0) {
            return Err(Error::InvalidInput("max records must be positive"));
        }
        if offset == Some("") {
            return Err(Error::InvalidInput("offset must not be empty"));
        }
        self.api
            .list(base.to_owned(), table.to_owned())
            .set_fields(options.fields.clone())
            .maybe_filter(options.filter_by_formula.clone())
            .maybe_offset(offset.map(str::to_owned))
            .maybe_page_size(options.page_size)
            .maybe_max_records(options.max_records)
            .maybe_view(options.view.clone())
            .send()
            .await
    }
    /// Traverse lazily from the start or a saved offset, under a required page cap.
    /// Resume only with the same base, table, options, credentials and origin. Use `concord_sdk_airtable::pagination::Resume` for the position. `Resume::Start` repeats the table listing without an offset; `Resume::At(key)` takes the opaque `String` offset in a returned page's `next`.
    pub fn list_pages(
        &self,
        base: impl Into<String>,
        table: impl Into<String>,
        options: ListOptions,
        from: Resume<String>,
        max_pages: NonZeroUsize,
    ) -> RecordPages {
        let client = self.clone();
        let base = base.into();
        let table = table.into();
        Pages::new(from, move |offset: Option<String>| {
            let client = client.clone();
            let base = base.clone();
            let table = table.clone();
            let options = options.clone();
            async move {
                let response = client
                    .list_page(&base, &table, &options, offset.as_deref())
                    .await?;
                Ok(Page {
                    items: response.value.records,
                    next: response.value.offset,
                    meta: response.meta,
                })
            }
        })
        .max_pages(max_pages)
    }
}
