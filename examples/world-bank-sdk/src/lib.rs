//! A small, unauthenticated World Bank Indicators API v2 client.
//! See the crate README for construction, examples and completion semantics.
use concord::{Endpoint, Json, SendWith};
use concord_pagination::{Page, Pages, Resume};
use serde::{Deserialize, Deserializer};
use std::num::NonZeroU32;

pub use concord::ResponseMeta;
pub use concord_pagination;

/// Maximum bytes read for each response, including error documents.
pub const BODY_LIMIT: usize = 8 * 1024 * 1024;

/// A provider code and display name.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct NamedCode {
    /// Provider code.
    pub id: String,
    /// Provider display name.
    pub value: String,
}

/// A country/indicator observation; a missing value is distinct from zero.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Observation {
    /// Indicator identity.
    pub indicator: NamedCode,
    /// Country identity.
    pub country: NamedCode,
    /// Three-letter country code, when provided.
    #[serde(rename = "countryiso3code")]
    pub country_iso3_code: Option<String>,
    /// Observation year as returned by the provider.
    pub date: String,
    /// Numeric value, or `None` for JSON null. The field must be present.
    #[serde(deserialize_with = "required_nullable")]
    pub value: Option<f64>,
    /// Units, when provided.
    pub unit: Option<String>,
    /// Observation status, when provided.
    pub obs_status: Option<String>,
    /// Provider decimal precision, when provided.
    pub decimal: Option<u32>,
}
fn required_nullable<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Option::deserialize(d)
}
fn number_or_string<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Number(u32),
        String(String),
    }
    match Number::deserialize(d)? {
        Number::Number(n) => Ok(n),
        Number::String(s) => s.parse().map_err(serde::de::Error::custom),
    }
}

/// Metadata in the first element of the JSON response array.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PageMetadata {
    /// One-based page number.
    #[serde(deserialize_with = "number_or_string")]
    pub page: u32,
    /// Total page count.
    #[serde(deserialize_with = "number_or_string")]
    pub pages: u32,
    /// Page size, which may be a JSON number or a string.
    #[serde(deserialize_with = "number_or_string")]
    pub per_page: u32,
    /// Total record count.
    #[serde(deserialize_with = "number_or_string")]
    pub total: u32,
    /// Provider's last-update date, if supplied.
    pub lastupdated: Option<String>,
}

/// A provider error message.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Message {
    /// Provider error identifier.
    pub id: String,
    /// Provider error key.
    pub key: String,
    /// Provider's explanation.
    pub value: String,
}
/// Provider error envelope.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ErrorDocument {
    /// Messages supplied by the provider.
    pub message: Vec<Message>,
}
/// Wire response: either metadata plus nullable data, or a provider error envelope.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum WireResponse {
    /// The two-element success array.
    Data((PageMetadata, Option<Vec<Observation>>)),
    /// The one-element error array.
    Failure((ErrorDocument,)),
}

/// A fully decoded page and its HTTP metadata.
#[derive(Debug)]
pub struct IndicatorPage {
    /// Provider paging metadata.
    pub metadata: PageMetadata,
    /// Observations (a null data array becomes an empty vector).
    pub observations: Vec<Observation>,
    /// Status, headers, HTTP version and Concord operation.
    pub response: ResponseMeta,
}

/// An error from the SDK. HTTP heads are preserved where one arrived.
#[derive(Debug, concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Provider error document, including when sent with a successful HTTP status.
    Provider {
        /// HTTP metadata.
        meta: Box<ResponseMeta>,
        /// Provider messages.
        document: ErrorDocument,
    },
    /// Non-success status with an unparseable, unavailable or non-JSON error body.
    Http {
        /// HTTP metadata.
        meta: Box<ResponseMeta>,
        /// Diagnostic of the body read or parse.
        detail: String,
    },
    /// Concord request, transport, content-type, size, or decode failure.
    Concord {
        /// Original error.
        source: concord::Error,
        /// Response metadata if available.
        meta: Option<Box<ResponseMeta>>,
    },
    /// Inconsistent paging metadata, with the response head.
    InvalidPage {
        /// Response metadata.
        meta: Box<ResponseMeta>,
        /// Reason for rejection.
        reason: &'static str,
    },
    /// The requested year range is reversed.
    InvalidRange,
}
impl Error {
    /// HTTP metadata when a response head was received.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Provider { meta, .. }
            | Self::Http { meta, .. }
            | Self::InvalidPage { meta, .. } => Some(meta),
            Self::Concord { meta, .. } => meta.as_deref(),
            Self::InvalidRange => None,
        }
    }
}
impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let meta = source.head_meta().map(Box::new);
        Self::Concord { source, meta }
    }
}
fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Provider { meta, .. } => write!(f, "World Bank error (HTTP {})", meta.status()),
        Error::Http { meta, .. } => write!(f, "World Bank HTTP {}", meta.status()),
        Error::Concord { source, .. } => std::fmt::Display::fmt(source, f),
        Error::InvalidPage { reason, .. } => write!(f, "invalid page metadata: {reason}"),
        Error::InvalidRange => f.write_str("year range starts after it ends"),
    }
}

concord::api! {
    /// Unauthenticated Indicators client. `WorldBank::new(http)` accepts a caller-configured
    /// transport; `with_base_url` replaces the v2 base for testing or another deployment.
    pub client WorldBank() {
        base "https://api.worldbank.org/v2";
        error crate::WireResponse in concord::Json;
        send SendPolicy;
        format Json;
        limit BODY_LIMIT;
        /// Request an indicator page; use [`WorldBank::year`] or [`WorldBank::range`] for
        /// year-specific workflows. IDs are encoded as individual path segments.
        GET indicator(country: String, indicator: String, query date: String,
                      query page: u32 = 1, query per_page: u32 = 50) -> WireResponse {
            path "country/{country}/indicator/{indicator}";
            query { "format" = "json"; }
        }
    }
}
/// Normal public client name.
pub type Client = WorldBank;

/// Send policy that decodes the full bounded JSON response and keeps its HTTP head.
#[derive(Default)]
pub struct SendPolicy;
impl SendWith<WireResponse, crate::WireResponse> for SendPolicy {
    type Output = Result<IndicatorPage, Error>;
    async fn send<E: Endpoint<WireResponse, crate::WireResponse> + Send>(
        &self,
        endpoint: E,
    ) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            let (meta, body) = received.failure(BODY_LIMIT).await.into_document();
            return match body {
                Ok(WireResponse::Failure((document,))) => Err(Error::Provider {
                    meta: Box::new(meta),
                    document,
                }),
                Ok(WireResponse::Data(_)) => Err(Error::Http {
                    meta: Box::new(meta),
                    detail: "unexpected data document".into(),
                }),
                Err(error) => Err(Error::Http {
                    meta: Box::new(meta),
                    detail: error.to_string(),
                }),
            };
        }
        let (body, meta) = received.into_success()?.decode_with_meta().await?;
        match body {
            WireResponse::Failure((document,)) => Err(Error::Provider {
                meta: Box::new(meta),
                document,
            }),
            WireResponse::Data((metadata, data)) => {
                if metadata.page == 0
                    || metadata.per_page == 0
                    || (metadata.pages == 0 && metadata.total != 0)
                    || (metadata.pages > 0 && metadata.page > metadata.pages)
                    || (metadata.pages == 0 && metadata.page != 1)
                {
                    return Err(Error::InvalidPage {
                        meta: Box::new(meta),
                        reason: "page, pages, per_page, or total are inconsistent",
                    });
                }
                let observations = data.unwrap_or_default();
                if observations.len() > metadata.per_page as usize
                    || (metadata.total == 0 && !observations.is_empty())
                {
                    return Err(Error::InvalidPage {
                        meta: Box::new(meta),
                        reason: "data contradicts page metadata",
                    });
                }
                Ok(IndicatorPage {
                    metadata,
                    observations,
                    response: meta,
                })
            }
        }
    }
}

/// Pagination metadata and the HTTP response head retained for each page.
#[derive(Debug)]
pub struct TraversalMeta {
    /// Provider metadata.
    pub page: PageMetadata,
    /// HTTP metadata.
    pub response: ResponseMeta,
}
/// A lazy, sequential page traversal. Page-level metadata is retained until `.items()` is used.
pub type IndicatorPages = Pages<Observation, u32, TraversalMeta, Error>;
impl WorldBank {
    /// Read one year's indicator page. The result can be empty or contain a null value.
    pub async fn year(
        &self,
        country: &str,
        indicator: &str,
        year: u32,
    ) -> Result<IndicatorPage, Error> {
        let page = self
            .indicator(country, indicator, year.to_string())
            .send()
            .await?;
        if page.metadata.page != 1 {
            return Err(Error::InvalidPage {
                meta: Box::new(page.response),
                reason: "response page does not match request",
            });
        }
        Ok(page)
    }

    /// Traverse the inclusive year range using provider-indicated page counts.
    /// `per_page` must be nonzero. Use `.max_pages(...)` to cap requests, or
    /// `concord_pagination::Resume::At(n)` to resume the same query at page `n`. Use `concord_sdk_world_bank::concord_pagination::Resume` for the position. `Resume::Start` repeats page 1 of the same country, indicator and year range with `per_page`; `Resume::At(key)` takes the `u32` page number in a returned page's `next`.
    pub fn range(
        &self,
        country: &str,
        indicator: &str,
        start: u32,
        end: u32,
        per_page: NonZeroU32,
        resume: Resume<u32>,
    ) -> Result<IndicatorPages, Error> {
        if start > end {
            return Err(Error::InvalidRange);
        }
        let client = self.clone();
        let country = country.to_owned();
        let indicator = indicator.to_owned();
        let date = format!("{start}:{end}");
        Ok(Pages::new(resume, move |key: Option<u32>| {
            let client = client.clone();
            let country = country.clone();
            let indicator = indicator.clone();
            let date = date.clone();
            async move {
                let requested = key.unwrap_or(1);
                let page = client
                    .indicator(country, indicator, date)
                    .set_page(requested)
                    .set_per_page(per_page.get())
                    .send()
                    .await?;
                if page.metadata.page != requested {
                    return Err(Error::InvalidPage {
                        meta: Box::new(page.response),
                        reason: "response page does not match request",
                    });
                }
                let next =
                    (page.metadata.page < page.metadata.pages).then(|| page.metadata.page + 1);
                Ok(Page {
                    items: page.observations,
                    next,
                    meta: TraversalMeta {
                        page: page.metadata,
                        response: page.response,
                    },
                })
            }
        }))
    }
}

/// Examples in the README are checked as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
