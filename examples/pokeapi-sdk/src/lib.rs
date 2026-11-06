//! A small PokéAPI v2 client for Pokémon details and manually followed list pages.
//!
//! PokéAPI requires no authentication. Construct [`Client`] with a configured
//! [`concord::reqwest::Client`]. Cache responses as requested by PokéAPI's fair-use policy.

use concord::reqwest::{self, Url, header::HeaderMap};
use concord::{Json, Prepared};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;

/// A named resource reference, including the provider's absolute URL.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Resource {
    /// Resource name.
    pub name: String,
    /// Provider-provided URL.
    pub url: String,
}

/// Pokémon detail. Fields not projected into named properties remain in `extra`.
#[derive(Clone, Debug, Deserialize)]
pub struct Pokemon {
    /// Numeric identifier.
    pub id: u64,
    /// Pokémon name.
    pub name: String,
    /// Base experience, when provided.
    pub base_experience: Option<u64>,
    /// Height in decimetres.
    pub height: u64,
    /// Weight in hectograms.
    pub weight: u64,
    /// Default form flag.
    pub is_default: bool,
    /// Sort order.
    pub order: u64,
    /// All other provider fields, including nested abilities, moves and sprites.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A list response. `next: None` means the traversal is complete, even if `results` is empty.
#[derive(Clone, Debug, Deserialize)]
pub struct PokemonPage {
    /// Total number of Pokémon.
    pub count: u64,
    /// Absolute URL of the next page, or `None` at the end.
    #[serde(deserialize_with = "required_nullable")]
    pub next: Option<String>,
    /// Absolute URL of the previous page, if any.
    #[serde(deserialize_with = "required_nullable")]
    pub previous: Option<String>,
    /// References on this page.
    pub results: Vec<Resource>,
}

fn required_nullable<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

concord::api! {
    /// Low-level generated operations. Prefer [`Client`] for metadata and URL following.
    pub client PokeApi() {
        base "https://pokeapi.co/api/v2";
        format Json;
        /// Retrieve one Pokémon by name or numeric ID.
        GET pokemon(id: String) -> Pokemon { path "pokemon/{id}/"; }
        /// Request one list page.
        GET list(query limit?: u64, query offset?: u64) -> PokemonPage { path "pokemon/"; }
    }
}

/// Response status, headers and final URL, captured before decoding the body.
#[derive(Clone, Debug)]
pub struct ResponseMeta {
    /// HTTP status.
    pub status: reqwest::StatusCode,
    /// All response headers.
    pub headers: HeaderMap,
    /// Final response URL after redirects.
    pub url: Url,
}

/// A fully decoded response and its head.
#[derive(Debug)]
pub struct Response<T> {
    /// Decoded response body.
    pub value: T,
    /// HTTP response metadata.
    pub meta: ResponseMeta,
}

/// An SDK failure, with response metadata when a response head arrived.
#[derive(Debug)]
pub enum Error {
    /// Concord's build, transport, HTTP status, metadata, or body decoding error.
    Concord {
        /// Original error. For status and metadata errors its unread response can be recovered.
        error: concord::Error,
        /// Response head, absent for build and transport errors.
        meta: Option<Box<ResponseMeta>>,
    },
    /// The supplied URL is malformed, foreign, or outside the Pokémon endpoint.
    InvalidUrl,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord { error, .. } => error.fmt(f),
            Self::InvalidUrl => f.write_str("invalid or out-of-scope PokéAPI URL"),
        }
    }
}
impl std::error::Error for Error {}
impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord { error, meta: None }
    }
}

/// Configured client. The transport controls timeouts, redirects and connection pooling.
/// The default base is `https://pokeapi.co/api/v2`; there is no authentication.
#[derive(Clone)]
pub struct Client {
    api: PokeApi,
}
impl Client {
    /// Construct with a caller-configured HTTP transport. No authentication is required.
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            api: PokeApi::new(http),
        }
    }
    /// Override the API base for an offline server or an alternate installation.
    ///
    /// # Errors
    /// Returns Concord's build error for an invalid base URL.
    pub fn with_base_url(mut self, base: impl AsRef<str>) -> Result<Self, Error> {
        self.api = self.api.with_base_url(base)?;
        Ok(self)
    }
    /// Retrieve one named Pokémon (or numeric ID).
    ///
    /// # Errors
    /// Returns the original Concord error, with metadata once the response head arrives.
    pub async fn pokemon(&self, id: impl Into<String>) -> Result<Response<Pokemon>, Error> {
        send(self.api.pokemon(id.into()).prepare()?, None).await
    }
    /// Request a list page with optional `limit` and `offset`; omitted values use provider defaults.
    ///
    /// # Errors
    /// Returns the original Concord error, with metadata once the response head arrives.
    pub async fn list(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Response<PokemonPage>, Error> {
        let mut op = self.api.list();
        if let Some(limit) = limit {
            op = op.set_limit(limit);
        }
        if let Some(offset) = offset {
            op = op.set_offset(offset);
        }
        send(op.prepare()?, None).await
    }
    /// Follow a `next` URL exactly, retaining its query parameters and their order.
    ///
    /// # Errors
    /// Rejects malformed URLs and URLs outside the configured origin and Pokémon list path.
    /// Other failures preserve the Concord error and any response head.
    pub async fn follow(&self, next: &str) -> Result<Response<PokemonPage>, Error> {
        let prepared = self.api.list().prepare()?;
        let url = checked_url(prepared.request().url(), next, false)?;
        send(prepared, Some(url)).await
    }
    /// Follow a `next` URL with a different page size. The URL's `limit` pair is replaced in
    /// place (later duplicates are dropped; it is appended when absent); `offset` and every other
    /// pair keep their bytes and order, so the position stays the one the server returned.
    ///
    /// # Errors
    /// The same as [`Client::follow`].
    pub async fn follow_with_limit(
        &self,
        next: &str,
        limit: u64,
    ) -> Result<Response<PokemonPage>, Error> {
        let prepared = self.api.list().set_limit(limit).prepare()?;
        let declared = prepared.request().url();
        let mut url = checked_url(declared, next, false)?;
        // With only `limit` set, the declared query is exactly the encoded `limit` pair.
        replace_pair(&mut url, declared.query().unwrap_or_default());
        send(prepared, Some(url)).await
    }
    /// Retrieve a Pokémon by a URL supplied in a list result.
    ///
    /// # Errors
    /// Rejects malformed, foreign or out-of-scope URLs; otherwise returns a Concord error.
    pub async fn pokemon_url(&self, url: &str) -> Result<Response<Pokemon>, Error> {
        let prepared = self.api.pokemon("placeholder".to_owned()).prepare()?;
        let list = self.api.list().prepare()?;
        let url = checked_url(list.request().url(), url, true)?;
        send(prepared, Some(url)).await
    }
}

fn checked_url(list: &Url, value: &str, detail: bool) -> Result<Url, Error> {
    let url = Url::parse(value).map_err(|_| Error::InvalidUrl)?;
    let list_path = list.path().trim_end_matches('/');
    let path = url.path();
    let valid_path = if detail {
        path.strip_prefix(list_path)
            .and_then(|s| s.strip_prefix('/'))
            .is_some_and(|s| {
                let name = s.trim_end_matches('/');
                !name.is_empty() && !name.contains('/') && !name.contains('%')
            })
            && url.query().is_none()
    } else {
        path.trim_end_matches('/') == list_path
    };
    if url.origin() != list.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !valid_path
    {
        return Err(Error::InvalidUrl);
    }
    Ok(url)
}

/// Puts the encoded `key=value` `pair` at the first query pair with the same key, drops later ones,
/// or appends it. Other non-empty segments are kept byte for byte, in order.
fn replace_pair(url: &mut Url, pair: &str) {
    fn key_of(segment: &str) -> &str {
        segment.split_once('=').map_or(segment, |(key, _)| key)
    }
    let key = key_of(pair);
    let mut placed = false;
    let mut segments = Vec::new();
    for segment in url.query().unwrap_or_default().split('&') {
        if segment.is_empty() {
            continue;
        }
        if key_of(segment) != key {
            segments.push(segment);
        } else if !placed {
            segments.push(pair);
            placed = true;
        }
    }
    if !placed {
        segments.push(pair);
    }
    let query = segments.join("&");
    url.set_query(Some(&query));
}

async fn send<T>(prepared: Prepared<T>, url: Option<Url>) -> Result<Response<T>, Error> {
    let (client, mut request, decoder) = prepared.into_parts();
    if let Some(url) = url {
        *request.url_mut() = url;
    }
    let received = decoder.execute(&client, request).await?;
    let meta = ResponseMeta {
        status: received.status(),
        headers: received.head().headers().clone(),
        url: received.url().clone(),
    };
    let value = received.decode().await.map_err(|error| Error::Concord {
        error,
        meta: Some(Box::new(meta.clone())),
    })?;
    Ok(Response { value, meta })
}
