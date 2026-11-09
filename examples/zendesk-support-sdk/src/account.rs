//! The checked origin of one Zendesk account.

use crate::Error;
use concord::BaseUrl;

/// One account's API origin. Select it before attaching the account's OAuth credential.
#[derive(Clone, Debug)]
pub struct Account(BaseUrl);

impl Account {
    /// The production origin of `subdomain` (`https://acme.zendesk.com`).
    pub fn new(subdomain: &str) -> Result<Self, Error> {
        Self::at(subdomain, "https://zendesk.com")
    }

    /// An account on a reserved fixture domain (`localhost`, `.localhost` or `.test`).
    /// Map the resulting account host to loopback with `reqwest::ClientBuilder::resolve`.
    pub fn for_fixture(subdomain: &str, base: &str) -> Result<Self, Error> {
        let base = BaseUrl::parse(base)?;
        let host = base.url().host_str().unwrap_or_default();
        if !(host == "localhost" || host.ends_with(".localhost") || host.ends_with(".test")) {
            return Err(Error::InvalidInput {
                what: "fixture account base",
            });
        }
        Self::at(subdomain, base.url().as_str())
    }

    fn at(subdomain: &str, base: &str) -> Result<Self, Error> {
        let bytes = subdomain.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 63
            || !bytes[0].is_ascii_alphanumeric()
            || !bytes[bytes.len() - 1].is_ascii_alphanumeric()
            || !bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        {
            return Err(Error::InvalidInput {
                what: "account subdomain",
            });
        }
        let mut url = BaseUrl::parse(base)?.url().clone();
        let host = format!("{subdomain}.{}", url.host_str().unwrap_or_default());
        url.set_host(Some(&host)).map_err(|_| Error::InvalidInput {
            what: "account host",
        })?;
        Ok(Self(BaseUrl::parse(url.as_str())?))
    }
}

impl AsRef<BaseUrl> for Account {
    fn as_ref(&self) -> &BaseUrl {
        &self.0
    }
}
