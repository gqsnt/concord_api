//! [`BaseUrl`]: the checked base URL every request of a client starts from.

use std::fmt;

use reqwest::Url;

use crate::error::Error;

/// A checked base URL: `http`/`https`, a host, an optional port and path, nothing else (no user
/// information, port 0, query or fragment). Declared paths are appended to its path.
///
/// A client declared with `base "https://…";` holds one. A client declared with `base x;` reads
/// it from its value `x`, whose type implements `AsRef<BaseUrl>`: the provider's base is then a
/// value the caller supplies (an instance, a region, an environment), checked once when that
/// value is made.
#[derive(Clone)]
pub struct BaseUrl {
    url: Url,
    /// The base path with at most one trailing slash removed; generated segments follow it.
    pub(crate) prefix: String,
}

impl BaseUrl {
    /// Checks a base URL.
    ///
    /// # Errors
    ///
    /// `concord::Error` (`ErrorKind::Build`) naming the rule the input breaks, never the input.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let fail = |reason| Error::build(None, "base URL", reason);
        let url = Url::parse(input).map_err(|_| fail("is not an absolute URL"))?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(fail("must use http or https"));
        }
        if url.host().is_none() {
            return Err(fail("must have a host"));
        }
        if url.port() == Some(0) {
            return Err(fail("must not use port 0"));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(fail("must not contain user information"));
        }
        if url.query().is_some() {
            return Err(fail("must not contain a query"));
        }
        if url.fragment().is_some() {
            return Err(fail("must not contain a fragment"));
        }
        let path = url.path();
        let prefix = path.strip_suffix('/').unwrap_or(path).to_owned();
        Ok(Self { url, prefix })
    }

    /// The base declared in `api!`, already validated by the macro.
    #[doc(hidden)]
    pub fn from_static(input: &'static str) -> Self {
        match Self::parse(input) {
            Ok(base) => base,
            Err(e) => panic!("api! accepted an invalid base URL {input:?}: {e}"),
        }
    }

    /// The checked URL.
    pub fn url(&self) -> &Url {
        &self.url
    }
}

impl AsRef<BaseUrl> for BaseUrl {
    fn as_ref(&self) -> &BaseUrl {
        self
    }
}

impl PartialEq for BaseUrl {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl Eq for BaseUrl {}

impl fmt::Debug for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.url.as_str(), f)
    }
}

impl fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.url.as_str())
    }
}
