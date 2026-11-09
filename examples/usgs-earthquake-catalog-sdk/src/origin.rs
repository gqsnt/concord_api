//! The local destination of this fixture-only capsule.
use crate::Error;
use concord::BaseUrl;

/// A checked loopback fixture base, with an optional path prefix.
#[derive(Clone, Debug)]
pub struct FixtureOrigin(BaseUrl);

impl FixtureOrigin {
    /// Accept HTTP(S) on localhost, a reserved .localhost name or a literal loopback IP.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let base = BaseUrl::parse(input)?;
        let host = base.url().host_str().unwrap_or_default();
        let local = host == "localhost"
            || host.ends_with(".localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
        if !local {
            return Err(Error::InvalidInput(
                "catalog capsule requires a loopback fixture",
            ));
        }
        Ok(Self(base))
    }
}

impl AsRef<BaseUrl> for FixtureOrigin {
    fn as_ref(&self) -> &BaseUrl {
        &self.0
    }
}
