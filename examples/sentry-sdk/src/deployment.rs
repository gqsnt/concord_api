//! The checked deployment of one Sentry caller.
use crate::Error;
use concord::BaseUrl;

/// An HTTPS deployment, or an HTTP loopback fixture; an optional proxy path is preserved.
#[derive(Clone, Debug)]
pub struct Deployment(BaseUrl);

impl Deployment {
    /// Sentry's hosted deployment.
    pub fn hosted() -> Self {
        Self(BaseUrl::from_static("https://sentry.io"))
    }

    /// Validate a self-hosted deployment or local fixture before attaching a credential.
    pub fn parse(input: impl AsRef<str>) -> Result<Self, Error> {
        let base = BaseUrl::parse(input.as_ref())?;
        let url = base.url();
        let local = url.host_str().is_some_and(|h| {
            h == "localhost"
                || h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if url.scheme() != "https" && !local {
            return Err(Error::Configuration(
                "deployment requires HTTPS or loopback",
            ));
        }
        Ok(Self(base))
    }
}

impl AsRef<BaseUrl> for Deployment {
    fn as_ref(&self) -> &BaseUrl {
        &self.0
    }
}
