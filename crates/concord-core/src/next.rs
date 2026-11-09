//! A next page whose URL the provider issues (`page next link;`, `page next url <path>;`).
//!
//! The declaration says where the provider puts the URL; an endpoint that declares it can be
//! [`follow`](Follow::follow)ed: [`Followed`] is the same endpoint (method, headers, decoder,
//! identity, send policy) sent to that URL. Whether to trust a URL beyond its origin, what a
//! continuation belongs to and when to stop stay the SDK's.

use std::fmt;
use std::future::Future;

use reqwest::Url;

use crate::error::Error;
use crate::prepared::{Endpoint, Prepared, Sends};
use crate::response::Operation;

/// An absolute URL issued by the provider for the next page. It may carry cursors, filters or
/// signatures in its query, so `Debug` and `Display` withhold it; [`expose`](Self::expose) and
/// [`url`](Self::url) disclose it explicitly. Parsing proves syntax only, not trust.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct NextUrl {
    text: Box<str>,
    url: Url,
}

impl NextUrl {
    /// Parses an absolute URL, keeping the original text for SDK validation.
    ///
    /// # Errors
    ///
    /// [`NextUrlError`] for empty text, a relative reference or a malformed URL.
    pub fn parse(text: &str) -> Result<Self, NextUrlError> {
        if text.is_empty() {
            return Err(NextUrlError::Empty);
        }
        match Url::parse(text) {
            Ok(url) => Ok(Self {
                text: text.into(),
                url,
            }),
            Err(url::ParseError::RelativeUrlWithoutBase) => Err(NextUrlError::Relative),
            Err(_) => Err(NextUrlError::Malformed),
        }
    }

    /// A URL already parsed (a resolved `Link` target).
    pub fn from_url(url: Url) -> Self {
        Self {
            text: url.as_str().into(),
            url,
        }
    }

    /// The original text, unredacted.
    pub fn expose(&self) -> &str {
        &self.text
    }

    /// The parsed URL, unredacted.
    pub fn url(&self) -> &Url {
        &self.url
    }
}

impl fmt::Debug for NextUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NextUrl(<withheld>)")
    }
}

impl fmt::Display for NextUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<withheld next URL>")
    }
}

/// Why a provider's next URL is not usable. Never contains the URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NextUrlError {
    /// The field is present and empty.
    Empty,
    /// A relative reference where an absolute URL is required.
    Relative,
    /// Not a URL.
    Malformed,
}

impl fmt::Display for NextUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "the next URL is empty",
            Self::Relative => "the next URL is not absolute",
            Self::Malformed => "the next URL does not parse",
        })
    }
}

impl std::error::Error for NextUrlError {}

/// An endpoint whose declaration says where the provider issues its next page URL
/// (`page next link;` or `page next url <path>;`).
pub trait Follow<T>: Endpoint<T> {
    /// The same endpoint sent to `target` instead of its own URL. Nothing is sent here.
    fn follow(self, target: NextUrl) -> Followed<Self> {
        Followed {
            endpoint: self,
            target,
        }
    }
}

/// `page next link;`: the next page is the response's `Link: <..>; rel="next"` target.
pub trait LinkContinuation<T>: Follow<T> {}

/// `page next url <path>;`: the next page is a URL field of the decoded output.
pub trait UrlContinuation<T>: Follow<T> {
    /// The declared field: `Ok(None)` when absent or null, an error when present but unusable.
    ///
    /// # Errors
    ///
    /// [`NextUrlError`] for a present empty, relative or malformed URL.
    fn next_url(value: &T) -> Result<Option<NextUrl>, NextUrlError>;
}

/// An endpoint sent to a provider-issued URL ([`Follow::follow`]): the endpoint's method,
/// headers, decoder, limit, identity and send policy, with only the URL replaced. Preparing it
/// refuses a URL with another origin than the endpoint's own. `Debug` withholds the URL.
#[derive(Clone)]
#[must_use = "a followed endpoint does nothing until `send` or `prepare` is called"]
pub struct Followed<E> {
    endpoint: E,
    target: NextUrl,
}

impl<E> Followed<E> {
    /// Sends exactly as the endpoint's own `send()` does: through the client's send policy, or,
    /// without one, once with the endpoint's declared decoding (`Result<T, concord::Error>`).
    pub fn send<O>(self) -> impl Future<Output = O> + Send
    where
        E: Sends<O>,
    {
        Sends::send(self)
    }
}

impl<T, E: Endpoint<T>> Endpoint<T> for Followed<E> {
    fn prepare(self) -> Result<Prepared<T>, Error> {
        self.endpoint.prepare()?.follow_url(self.target.url)
    }

    fn operation() -> Operation {
        E::operation()
    }
}

impl<O, E: Sends<O>> Sends<O> for Followed<E> {
    fn send(self) -> impl Future<Output = O> + Send {
        self.endpoint.__send_followed(self.target)
    }

    fn __send_followed(self, target: NextUrl) -> impl Future<Output = O> + Send {
        self.endpoint.__send_followed(target)
    }
}

impl<E: fmt::Debug> fmt::Debug for Followed<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Followed")
            .field("endpoint", &self.endpoint)
            .field("target", &self.target)
            .finish()
    }
}

/// `api!` support: a followed endpoint built by a generated send.
#[doc(hidden)]
pub fn followed<E>(endpoint: E, target: NextUrl) -> Followed<E> {
    Followed { endpoint, target }
}

/// `api!` support: the declared field's text (`None` when absent) to a next URL.
#[doc(hidden)]
pub fn next_url(text: Option<&str>) -> Result<Option<NextUrl>, NextUrlError> {
    text.map(NextUrl::parse).transpose()
}
