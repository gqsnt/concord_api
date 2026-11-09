//! The logical site and the checked base of one Datadog deployment.

use concord::BaseUrl;

use crate::site::Site;

/// A site's API host, or an explicitly selected reverse proxy or fixture base.
///
/// Forward proxies belong to the supplied `reqwest::Client`. A reverse proxy is selected
/// here before credentials are attached. A custom base may include a path prefix.
/// `site()` retains the configured region; authentication errors identify the actual host.
/// This type is available to integration tests and applications, without `cfg(test)`.
#[derive(Clone, Debug)]
pub struct Destination {
    site: Site,
    base: BaseUrl,
}

impl Destination {
    /// A custom deployment for `site`. Accepts the same HTTP(S) bases as Concord:
    /// a host, optional port and path, without user information, query or fragment.
    /// Port zero is refused. Does not send anything or inspect the network.
    pub fn at(site: Site, base: impl AsRef<str>) -> Result<Self, concord::Error> {
        Ok(Self {
            site,
            base: BaseUrl::parse(base.as_ref())?,
        })
    }

    /// The configured region, including when this is a custom deployment.
    pub fn site(&self) -> Site {
        self.site
    }
}

impl From<Site> for Destination {
    fn from(site: Site) -> Self {
        Self {
            site,
            base: BaseUrl::parse(&site.api_base_url())
                .expect("every listed site's API base is an https URL with a domain host"),
        }
    }
}

impl AsRef<BaseUrl> for Destination {
    fn as_ref(&self) -> &BaseUrl {
        &self.base
    }
}
