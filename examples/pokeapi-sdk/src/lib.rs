//! A small PokéAPI v2 client for Pokémon details and manually followed list pages.
//!
//! PokéAPI requires no authentication. Construct [`Client`] with a configured
//! [`concord::reqwest::Client`]. Cache responses as requested by PokéAPI's fair-use policy.
//!
//! `use concord_sdk_pokeapi::Client;` is all a caller imports: a declared operation is sent with
//! its `send()` (`api.pokemon("clefairy").send().await?`, through the client's send policy
//! [`KeepHead`]), a URL the server returned with [`Client::follow`] or [`Client::pokemon_url`].
//! Each returns a [`Response`] with the response head, or an [`Error`] that keeps it.

use concord::reqwest::{self, Url, header::HeaderMap};
use concord::{Endpoint, Json, Received, SendWith};
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
    /// The PokéAPI client (also named [`Client`]); `PokeApi` is the name diagnostics and
    /// [`concord::Operation`] names use (`PokeApi.pokemon`, `PokeApi.list`).
    ///
    /// `PokeApi::new(http)` uses `https://pokeapi.co/api/v2` with the caller's transport
    /// (timeouts, redirects, pooling); `with_base_url` replaces it for an offline server or
    /// another installation. There is no authentication. Every operation's `send()` goes through
    /// [`KeepHead`]: it returns the value with the response head ([`Response`]) or this SDK's
    /// [`Error`]. `prepare()` builds the native request with Concord's own error.
    pub client PokeApi() {
        base "https://pokeapi.co/api/v2";
        send KeepHead -> Result<Response<_>, Error>;
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
        /// Response head, absent when no response arrived.
        meta: Option<Box<ResponseMeta>>,
    },
    /// The supplied URL was refused before any request, for the given reason. Neither `Display`
    /// nor `Debug` shows the URL.
    InvalidUrl(UrlRejection),
}

/// Why [`Client::follow`] or [`Client::pokemon_url`] refused a URL. The checks run in this
/// order and the first that fails is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UrlRejection {
    /// Not an absolute URL.
    Unparseable,
    /// Scheme, host or port differ from the configured base.
    ForeignOrigin,
    /// It carries a user name or password.
    Userinfo,
    /// It has a fragment (`#...`).
    Fragment,
    /// The path is not the Pokémon list (for `follow`), or not one Pokémon of it (for
    /// `pokemon_url`: one segment after the list path, without `/` or `%` in the name).
    WrongPath,
    /// A Pokémon detail URL with a query string.
    Query,
}

impl fmt::Display for UrlRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unparseable => "not an absolute URL",
            Self::ForeignOrigin => "foreign origin",
            Self::Userinfo => "user info in the URL",
            Self::Fragment => "fragment in the URL",
            Self::WrongPath => "path outside the Pokémon collection",
            Self::Query => "query on a Pokémon detail URL",
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord { error, .. } => error.fmt(f),
            Self::InvalidUrl(reason) => write!(f, "invalid or out-of-scope PokéAPI URL: {reason}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        // This SDK also promises the final URL, which core metadata does not retain.
        // Consumed-body failures are wrapped by `finish`, while it still has that URL.
        let meta = error.head().zip(error.response()).map(|(head, response)| {
            Box::new(ResponseMeta {
                status: head.status(),
                headers: head.headers().clone(),
                url: response.url().clone(),
            })
        });
        Self::Concord { error, meta }
    }
}

/// The configured client: the generated [`PokeApi`] under this SDK's documented name.
/// `Client::new(http)` takes the caller's transport, which controls timeouts, redirects and
/// connection pooling; the default base is `https://pokeapi.co/api/v2` and there is no
/// authentication.
pub type Client = PokeApi;

/// The send policy of [`PokeApi`] (`send KeepHead -> Result<Response<_>, Error>;`): every operation's `send()`
/// (`api.pokemon("clefairy").send()`, `api.list().set_limit(20).set_offset(0).send()`) executes
/// once, decodes the entire JSON body, keeps status, headers and final URL, and returns
/// `Result<Response<T>, Error>`.
///
/// Errors: [`Error::Concord`] with the original Concord error; `meta` is present once the
/// response head arrived (including a failed body read), absent for build and connection
/// errors before a head.
#[derive(Default)]
pub struct KeepHead;

impl<T: Send> SendWith<T> for KeepHead {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        finish(endpoint.prepare()?.receive().await?).await
    }
}

impl PokeApi {
    /// Follow a `next` URL exactly, retaining its query parameters and their order.
    ///
    /// # Errors
    /// [`Error::InvalidUrl`] for a malformed URL or one outside the configured origin and Pokémon
    /// list path (nothing is sent). Other failures are those of [`KeepHead`].
    pub async fn follow(&self, next: &str) -> Result<Response<PokemonPage>, Error> {
        let list = self.list().prepare()?;
        checked_url(list.request().url(), next, false)?;
        finish(list.follow(next)?.receive().await?).await
    }
    /// Retrieve a Pokémon by a URL supplied in a list result.
    ///
    /// # Errors
    /// [`Error::InvalidUrl`] for a malformed, foreign or out-of-scope URL (nothing is sent).
    /// Other failures are those of [`KeepHead`].
    pub async fn pokemon_url(&self, url: &str) -> Result<Response<Pokemon>, Error> {
        checked_url(self.list().prepare()?.request().url(), url, true)?;
        // The `pokemon` operation's request (decoder, identity) at the checked URL.
        let detail = self.pokemon("placeholder").prepare()?.follow(url)?;
        finish(detail.receive().await?).await
    }
}

/// The SDK's rules for a server-returned URL, checked against the list request's URL before
/// `Prepared::follow` (which itself refuses only another origin).
fn checked_url(list: &Url, value: &str, detail: bool) -> Result<(), Error> {
    let reject = |reason| Err(Error::InvalidUrl(reason));
    let Ok(url) = Url::parse(value) else {
        return reject(UrlRejection::Unparseable);
    };
    if url.origin() != list.origin() {
        return reject(UrlRejection::ForeignOrigin);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return reject(UrlRejection::Userinfo);
    }
    if url.fragment().is_some() {
        return reject(UrlRejection::Fragment);
    }
    let list_path = list.path().trim_end_matches('/');
    let path = url.path();
    let valid_path = if detail {
        path.strip_prefix(list_path)
            .and_then(|s| s.strip_prefix('/'))
            .is_some_and(|s| {
                let name = s.trim_end_matches('/');
                !name.is_empty() && !name.contains('/') && !name.contains('%')
            })
    } else {
        path.trim_end_matches('/') == list_path
    };
    if !valid_path {
        return reject(UrlRejection::WrongPath);
    }
    if detail && url.query().is_some() {
        return reject(UrlRejection::Query);
    }
    Ok(())
}

/// The README's examples, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// Captures the head, then decodes; a decode failure keeps the head.
async fn finish<T>(received: Received<T>) -> Result<Response<T>, Error> {
    // Keep the final URL as well as the head; `decode_with_meta` does not carry the URL.
    let meta = ResponseMeta {
        status: received.status(),
        headers: received.head().headers().clone(),
        url: received.url().clone(),
    };
    match received.decode().await {
        Ok(value) => Ok(Response { value, meta }),
        Err(error) => Err(Error::Concord {
            error,
            meta: Some(Box::new(meta)),
        }),
    }
}
