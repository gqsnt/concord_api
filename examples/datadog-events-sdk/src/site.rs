//! Datadog sites (regions) and the API host each one serves.

use std::fmt;
use std::str::FromStr;

/// A Datadog site: the region an organization (and therefore its API and application keys)
/// belongs to. Every request goes to that site's API host; keys of one site are not valid on
/// another.
///
/// The table is the one the cited Events reference pages print for every endpoint (sites,
/// selector labels and API hosts, read 2026-11-07). A site Datadog adds later is reached with
/// [`Destination::at`](crate::Destination::at) until it is listed here; that destination keeps
/// the caller's selected site label, while authentication errors name the host that answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Site {
    /// `app.datadoghq.com`, API host `api.datadoghq.com` (the default of Datadog's clients).
    Us1,
    /// `us3.datadoghq.com`, API host `api.us3.datadoghq.com`.
    Us3,
    /// `us5.datadoghq.com`, API host `api.us5.datadoghq.com`.
    Us5,
    /// `app.datadoghq.eu`, API host `api.datadoghq.eu`.
    Eu,
    /// `ap1.datadoghq.com`, API host `api.ap1.datadoghq.com`.
    Ap1,
    /// `ap2.datadoghq.com`, API host `api.ap2.datadoghq.com`.
    Ap2,
    /// `uk1.datadoghq.com`, API host `api.uk1.datadoghq.com`.
    Uk1,
    /// `app.ddog-gov.com`, API host `api.ddog-gov.com`.
    Us1Fed,
    /// `us2.ddog-gov.com`, API host `api.us2.ddog-gov.com`.
    Us2Fed,
}

impl Site {
    /// Every listed site, in the order of the documentation's table.
    pub const ALL: [Site; 9] = [
        Site::Us1,
        Site::Us3,
        Site::Us5,
        Site::Eu,
        Site::Ap1,
        Site::Ap2,
        Site::Uk1,
        Site::Us1Fed,
        Site::Us2Fed,
    ];

    /// The documentation's selector label (`US1`, `EU`, `US1-FED`, ...).
    pub fn label(self) -> &'static str {
        match self {
            Site::Us1 => "US1",
            Site::Us3 => "US3",
            Site::Us5 => "US5",
            Site::Eu => "EU",
            Site::Ap1 => "AP1",
            Site::Ap2 => "AP2",
            Site::Uk1 => "UK1",
            Site::Us1Fed => "US1-FED",
            Site::Us2Fed => "US2-FED",
        }
    }

    /// The `DD_SITE` value of the documentation's examples (`datadoghq.com`, `datadoghq.eu`,
    /// `us2.ddog-gov.com`, ...). The API host is `api.` followed by it.
    pub fn dd_site(self) -> &'static str {
        match self {
            Site::Us1 => "datadoghq.com",
            Site::Us3 => "us3.datadoghq.com",
            Site::Us5 => "us5.datadoghq.com",
            Site::Eu => "datadoghq.eu",
            Site::Ap1 => "ap1.datadoghq.com",
            Site::Ap2 => "ap2.datadoghq.com",
            Site::Uk1 => "uk1.datadoghq.com",
            Site::Us1Fed => "ddog-gov.com",
            Site::Us2Fed => "us2.ddog-gov.com",
        }
    }

    /// The API host this site serves (`api.datadoghq.eu`).
    pub fn api_host(self) -> &'static str {
        match self {
            Site::Us1 => "api.datadoghq.com",
            Site::Us3 => "api.us3.datadoghq.com",
            Site::Us5 => "api.us5.datadoghq.com",
            Site::Eu => "api.datadoghq.eu",
            Site::Ap1 => "api.ap1.datadoghq.com",
            Site::Ap2 => "api.ap2.datadoghq.com",
            Site::Uk1 => "api.uk1.datadoghq.com",
            Site::Us1Fed => "api.ddog-gov.com",
            Site::Us2Fed => "api.us2.ddog-gov.com",
        }
    }

    /// `https://` and the API host: the base every request of this site starts with.
    pub fn api_base_url(self) -> String {
        format!("https://{}", self.api_host())
    }

    /// The site whose `DD_SITE` value is `value` (ASCII case is ignored, surrounding
    /// whitespace is not accepted).
    ///
    /// # Errors
    ///
    /// [`UnknownSite`] for any other text.
    pub fn from_dd_site(value: &str) -> Result<Site, UnknownSite> {
        Site::ALL
            .into_iter()
            .find(|site| site.dd_site().eq_ignore_ascii_case(value))
            .ok_or(UnknownSite)
    }
}

impl Site {
    /// The listed site whose API host is `host` (ASCII case ignored), if any.
    pub fn from_api_host(host: &str) -> Option<Site> {
        Site::ALL
            .into_iter()
            .find(|site| site.api_host().eq_ignore_ascii_case(host))
    }
}

impl fmt::Display for Site {
    /// The selector label and API host: `EU (api.datadoghq.eu)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.label(), self.api_host())
    }
}

impl FromStr for Site {
    type Err = UnknownSite;

    /// As [`Site::from_dd_site`].
    fn from_str(value: &str) -> Result<Site, UnknownSite> {
        Site::from_dd_site(value)
    }
}

/// A `DD_SITE` value that is not one of the listed sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownSite;

impl fmt::Display for UnknownSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a listed Datadog site (expected a DD_SITE value such as `datadoghq.eu`)")
    }
}

impl std::error::Error for UnknownSite {}
