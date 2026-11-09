//! Checked DigitalOcean destination, selected before a credential is attached.
use crate::Error;
use concord::BaseUrl;

/// The official API origin or an explicit HTTP loopback fixture origin.
#[derive(Clone, Debug)]
pub struct Destination(BaseUrl);

impl Destination {
    /// The production API origin.
    pub fn production() -> Self {
        Self(BaseUrl::from_static("https://api.digitalocean.com"))
    }

    /// A literal loopback IP over HTTP, without a path prefix.
    pub fn for_fixture(origin: &str) -> Result<Self, Error> {
        let base = BaseUrl::parse(origin).map_err(|_| Error::FixtureOrigin)?;
        let url = base.url();
        let local = url
            .host_str()
            .and_then(|h| h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if url.scheme() != "http" || !local || url.path() != "/" {
            return Err(Error::FixtureOrigin);
        }
        Ok(Self(base))
    }
}

impl AsRef<BaseUrl> for Destination {
    fn as_ref(&self) -> &BaseUrl {
        &self.0
    }
}
