//! The org context: one instance URL, one pinned API version and one access token, shared by the
//! generated handle and every adapter; and the validation of links Salesforce returns.
//!
//! # Returned links
//!
//! Salesforce returns links to follow: a query's `nextRecordsUrl`, the `resultUrl`s and next link
//! of a result-pages manifest, an ingest job's `contentUrl`, each record's `attributes.url`. The
//! SDK never sends a request to a returned URL as such. It validates the link against this
//! context, extracts the typed parts it designates (a [`QueryLocator`], a [`ResultLink`], ...)
//! and sends the **declared** operation built from them, so the request keeps the declaration's
//! base, version, headers and decoder. A link is refused ([`LinkError`]) when it:
//!
//! - is not on the instance origin (another scheme, host or port, `//host/...`), carries user
//!   information or a fragment;
//! - is not canonical: dot segments, backslashes, whitespace, control characters, or anything the
//!   URL parser would rewrite;
//! - is under `/services/data/` with another API version than the context's, or designates
//!   another resource (another job, another kind of resource) than expected;
//! - carries query parameters other than the documented ones, repeats one, or carries a locator
//!   outside the conservative locator character set (refused, not re-encoded).
//!
//! Relative links are resolved against the instance origin; the documented forms are
//! `/services/data/v68.0/...` and, for `contentUrl`, `services/data/v68.0/...` (no leading slash).

use std::fmt;
use std::sync::Arc;

use concord::reqwest::{self, Url};

use crate::error::{LinkError, LinkErrorKind, LinkRole};
use crate::ids::{
    AccessToken, ApiVersion, IngestJobId, InstanceUrl, MaxRecords, QueryJobId, QueryLocator,
    RecordId, ResultLocator, SObjectName,
};
use crate::{SalesforceApi, SalesforceApiConfig};

/// One org at one API version with one access token. Cheap to clone (an `Arc`); immutable.
///
/// `Debug` shows the instance origin and the version, never the token.
#[derive(Clone)]
pub struct OrgContext {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    instance: InstanceUrl,
    version: ApiVersion,
    api: SalesforceApi,
}

/// Why a context could not be created.
#[derive(Debug)]
#[non_exhaustive]
pub enum ContextError {
    /// The version is older than [`ApiVersion::MINIMUM`].
    UnsupportedVersion(ApiVersion),
    /// Concord refused the instance URL as a base.
    Base(concord::Error),
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContextError::UnsupportedVersion(v) => write!(
                f,
                "API version {v} is older than {} (the oldest version with every operation of this SDK)",
                ApiVersion::MINIMUM
            ),
            ContextError::Base(e) => write!(f, "instance URL refused: {e}"),
        }
    }
}

impl std::error::Error for ContextError {}

impl OrgContext {
    /// A context for the org at `instance`, at `version`, authenticated with `token`. Sends
    /// nothing.
    ///
    /// # Errors
    ///
    /// [`ContextError`] for a version older than [`ApiVersion::MINIMUM`], or an instance URL
    /// Concord refuses as a base.
    pub fn new(
        http: reqwest::Client,
        instance: InstanceUrl,
        version: ApiVersion,
        token: AccessToken,
    ) -> Result<Self, ContextError> {
        if version < ApiVersion::MINIMUM {
            return Err(ContextError::UnsupportedVersion(version));
        }
        let config = SalesforceApiConfig {
            token: token.expose().to_owned(),
            version,
        };
        let api = SalesforceApi::new(http.clone(), config)
            .with_base_url(instance.as_str())
            .map_err(ContextError::Base)?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                instance,
                version,
                api,
            }),
        })
    }

    /// The generated handle, configured with this context's values.
    pub fn api(&self) -> &SalesforceApi {
        &self.inner.api
    }

    /// The `reqwest::Client`.
    pub fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    /// The instance origin.
    pub fn instance(&self) -> &InstanceUrl {
        &self.inner.instance
    }

    /// The API version every request uses.
    pub fn version(&self) -> ApiVersion {
        self.inner.version
    }

    /// Validates a REST query's `nextRecordsUrl` (`/services/data/v68.0/query/{locator}`) and
    /// returns its locator.
    ///
    /// # Errors
    ///
    /// [`LinkError`] (see the [module documentation](self)).
    pub fn query_link(&self, raw: &str) -> Result<QueryLocator, LinkError> {
        let role = LinkRole::QueryNext;
        let link = self.parse(role, raw)?;
        match link.rest.as_slice() {
            ["query", locator] => {
                if link.query.is_some() {
                    return Err(LinkError::new(role, LinkErrorKind::UnexpectedQuery));
                }
                QueryLocator::new(*locator)
                    .map_err(|_| LinkError::new(role, LinkErrorKind::InvalidLocator))
            }
            _ => Err(LinkError::new(role, LinkErrorKind::UnexpectedPath)),
        }
    }

    /// Validates a `resultUrl` of `job`'s result-pages manifest
    /// (`/services/data/v68.0/jobs/query/{job}/results?locator=..[&maxRecords=..]`).
    ///
    /// # Errors
    ///
    /// [`LinkError`] (see the [module documentation](self)).
    pub fn result_link(&self, job: &QueryJobId, raw: &str) -> Result<ResultLink, LinkError> {
        let role = LinkRole::ResultUrl;
        let link = self.parse(role, raw)?;
        match link.rest.as_slice() {
            ["jobs", "query", id, "results"] => {
                if *id != job.as_str() {
                    return Err(LinkError::new(role, LinkErrorKind::WrongJob));
                }
                let params = query_params(role, link.query, &["locator", "maxRecords"])?;
                let locator = params
                    .get("locator")
                    .map(|v| {
                        ResultLocator::new(*v)
                            .map_err(|_| LinkError::new(role, LinkErrorKind::InvalidLocator))
                    })
                    .transpose()?;
                let max_records = params
                    .get("maxRecords")
                    .map(|v| {
                        v.parse::<u32>()
                            .ok()
                            .and_then(|n| MaxRecords::new(n).ok())
                            .ok_or(LinkError::new(role, LinkErrorKind::InvalidMaxRecords))
                    })
                    .transpose()?;
                Ok(ResultLink {
                    locator,
                    max_records,
                })
            }
            ["jobs", "query", _, _] => Err(LinkError::new(role, LinkErrorKind::UnexpectedPath)),
            _ => Err(LinkError::new(role, LinkErrorKind::UnexpectedPath)),
        }
    }

    /// Validates the next-manifest link of `job`'s result pages
    /// (`/services/data/v68.0/jobs/query/{job}/resultPages?locator=..`) and returns its locator.
    ///
    /// The documented URI spells the resource `resultPages`; the documented example link spells
    /// it `resultpages`. Both are accepted here (ASCII case-insensitively); the request is then
    /// sent to the declared `resultPages` endpoint with the locator (decision SF-D2 in
    /// `PROVIDER.md`).
    ///
    /// # Errors
    ///
    /// [`LinkError`] (see the [module documentation](self)).
    pub fn result_pages_link(
        &self,
        job: &QueryJobId,
        raw: &str,
    ) -> Result<ResultLocator, LinkError> {
        let role = LinkRole::ResultPagesNext;
        let link = self.parse(role, raw)?;
        match link.rest.as_slice() {
            ["jobs", "query", id, resource] if resource.eq_ignore_ascii_case("resultPages") => {
                if *id != job.as_str() {
                    return Err(LinkError::new(role, LinkErrorKind::WrongJob));
                }
                let params = query_params(role, link.query, &["locator"])?;
                let locator = params
                    .get("locator")
                    .ok_or(LinkError::new(role, LinkErrorKind::UnexpectedQuery))?;
                ResultLocator::new(*locator)
                    .map_err(|_| LinkError::new(role, LinkErrorKind::InvalidLocator))
            }
            _ => Err(LinkError::new(role, LinkErrorKind::UnexpectedPath)),
        }
    }

    /// Validates an ingest job's `contentUrl` (`services/data/v68.0/jobs/ingest/{job}/batches`).
    ///
    /// # Errors
    ///
    /// [`LinkError`] (see the [module documentation](self)).
    pub fn content_url(&self, job: &IngestJobId, raw: &str) -> Result<(), LinkError> {
        let role = LinkRole::ContentUrl;
        let link = self.parse(role, raw)?;
        match link.rest.as_slice() {
            ["jobs", "ingest", id, "batches"] => {
                if *id != job.as_str() {
                    return Err(LinkError::new(role, LinkErrorKind::WrongJob));
                }
                if link.query.is_some() {
                    return Err(LinkError::new(role, LinkErrorKind::UnexpectedQuery));
                }
                Ok(())
            }
            _ => Err(LinkError::new(role, LinkErrorKind::UnexpectedPath)),
        }
    }

    /// Validates a record's `attributes.url` (`/services/data/v68.0/sobjects/{type}/{id}`) and
    /// returns the object and id it designates.
    ///
    /// # Errors
    ///
    /// [`LinkError`] (see the [module documentation](self)).
    pub fn record_link(&self, raw: &str) -> Result<(SObjectName, RecordId), LinkError> {
        let role = LinkRole::RecordUrl;
        let link = self.parse(role, raw)?;
        match link.rest.as_slice() {
            ["sobjects", object, id] => {
                if link.query.is_some() {
                    return Err(LinkError::new(role, LinkErrorKind::UnexpectedQuery));
                }
                let object = SObjectName::new(*object)
                    .map_err(|_| LinkError::new(role, LinkErrorKind::UnexpectedPath))?;
                let id = RecordId::new(*id)
                    .map_err(|_| LinkError::new(role, LinkErrorKind::UnexpectedPath))?;
                Ok((object, id))
            }
            _ => Err(LinkError::new(role, LinkErrorKind::UnexpectedPath)),
        }
    }

    /// Resolves and checks the common rules; returns the path segments after
    /// `/services/data/v{version}/` and the raw query.
    fn parse<'a>(&self, role: LinkRole, raw: &'a str) -> Result<ParsedLink<'a>, LinkError> {
        let err = |kind| LinkError::new(role, kind);
        if raw.is_empty()
            || raw.len() > 4096
            || raw
                .bytes()
                .any(|b| b <= b' ' || b == 0x7f || b == b'\\' || !b.is_ascii())
        {
            return Err(err(LinkErrorKind::Malformed));
        }
        if raw.starts_with("//") {
            return Err(err(LinkErrorKind::ForeignOrigin));
        }
        let absolute = has_scheme(raw);
        let url: Url = if absolute {
            Url::parse(raw).map_err(|_| err(LinkErrorKind::Malformed))?
        } else {
            self.inner
                .instance
                .url()
                .join(raw)
                .map_err(|_| err(LinkErrorKind::Malformed))?
        };
        if !url.username().is_empty() || url.password().is_some() {
            return Err(err(LinkErrorKind::UserInfo));
        }
        if !self.inner.instance.same_origin(&url) {
            return Err(err(LinkErrorKind::ForeignOrigin));
        }
        if raw.contains('#') {
            return Err(err(LinkErrorKind::Fragment));
        }
        // The raw path must be exactly what the parser kept (no dot segments, no rewriting).
        let after_origin = if absolute {
            let rest = &raw[raw.find("://").map_or(0, |i| i + 3)..];
            rest.find('/').map_or("", |i| &rest[i..])
        } else {
            raw
        };
        let (raw_path, raw_query) = match after_origin.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (after_origin, None),
        };
        let canonical = if raw_path.starts_with('/') {
            raw_path == url.path()
        } else {
            url.path().strip_prefix('/') == Some(raw_path)
        };
        if !canonical || url.query() != raw_query {
            return Err(err(LinkErrorKind::NonCanonicalPath));
        }
        let segments: Vec<&'a str> = raw_path.trim_start_matches('/').split('/').collect();
        if segments
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == "..")
        {
            return Err(err(LinkErrorKind::NonCanonicalPath));
        }
        match segments.as_slice() {
            ["services", "data", version, rest @ ..] => {
                if *version != self.inner.version.segment() {
                    return Err(err(LinkErrorKind::VersionMismatch));
                }
                Ok(ParsedLink {
                    rest: rest.to_vec(),
                    query: raw_query,
                })
            }
            _ => Err(err(LinkErrorKind::UnexpectedPath)),
        }
    }
}

struct ParsedLink<'a> {
    rest: Vec<&'a str>,
    query: Option<&'a str>,
}

fn has_scheme(raw: &str) -> bool {
    match raw.split_once(':') {
        Some((scheme, _)) => {
            !scheme.is_empty()
                && scheme.as_bytes()[0].is_ascii_alphabetic()
                && scheme
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
                && !scheme.contains('/')
        }
        None => false,
    }
}

/// Splits a raw query into the allowed parameters; each at most once, values unencoded (the
/// locator character set has nothing to percent-decode).
fn query_params<'a>(
    role: LinkRole,
    query: Option<&'a str>,
    allowed: &[&str],
) -> Result<std::collections::BTreeMap<&'a str, &'a str>, LinkError> {
    let mut params = std::collections::BTreeMap::new();
    let Some(query) = query else {
        return Ok(params);
    };
    for pair in query.split('&') {
        let (key, value) = pair
            .split_once('=')
            .ok_or(LinkError::new(role, LinkErrorKind::UnexpectedQuery))?;
        if !allowed.contains(&key) || value.is_empty() || params.insert(key, value).is_some() {
            return Err(LinkError::new(role, LinkErrorKind::UnexpectedQuery));
        }
    }
    Ok(params)
}

impl fmt::Debug for OrgContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrgContext")
            .field("instance", &self.inner.instance)
            .field("version", &self.inner.version)
            .finish_non_exhaustive()
    }
}

/// A validated result link: the locator (absent for the first result set) and the
/// `maxRecords` the link carries. `Debug` never shows the locator.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ResultLink {
    /// The `locator`.
    pub locator: Option<ResultLocator>,
    /// The `maxRecords`.
    pub max_records: Option<MaxRecords>,
}

impl fmt::Debug for ResultLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResultLink")
            .field("locator", &self.locator.as_ref().map(|_| ".."))
            .field("max_records", &self.max_records)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(instance: &str) -> OrgContext {
        OrgContext::new(
            reqwest::Client::new(),
            InstanceUrl::parse(instance).unwrap(),
            ApiVersion::PINNED,
            AccessToken::new("00Dsynthetic!token").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn query_links() {
        let c = ctx("https://org-a.my.salesforce.com");
        let ok = c
            .query_link("/services/data/v68.0/query/01gD0000002HU6KIAW-2000")
            .unwrap();
        assert_eq!(ok.expose(), "01gD0000002HU6KIAW-2000");
        let abs = c.query_link("https://org-a.my.salesforce.com/services/data/v68.0/query/01g-1");
        assert!(abs.is_ok());
        let kind = |raw: &str| c.query_link(raw).unwrap_err().kind();
        assert_eq!(
            kind("https://evil.example/services/data/v68.0/query/01g-1"),
            LinkErrorKind::ForeignOrigin
        );
        assert_eq!(
            kind("//evil.example/services/data/v68.0/query/01g-1"),
            LinkErrorKind::ForeignOrigin
        );
        assert_eq!(
            kind("http://org-a.my.salesforce.com/services/data/v68.0/query/01g-1"),
            LinkErrorKind::ForeignOrigin
        );
        assert_eq!(
            kind("https://org-a.my.salesforce.com:8443/services/data/v68.0/query/01g-1"),
            LinkErrorKind::ForeignOrigin
        );
        assert_eq!(
            kind("https://u@org-a.my.salesforce.com/services/data/v68.0/query/01g-1"),
            LinkErrorKind::UserInfo
        );
        assert_eq!(
            kind("/services/data/v67.0/query/01g-1"),
            LinkErrorKind::VersionMismatch
        );
        assert_eq!(
            kind("/services/data/v68.0/sobjects/x/../../query/01g-1"),
            LinkErrorKind::NonCanonicalPath
        );
        assert_eq!(
            kind("/services/data/v68.0/query/%2e%2e"),
            LinkErrorKind::NonCanonicalPath
        );
        assert_eq!(
            kind("/services/data/v68.0/query/01g-1#x"),
            LinkErrorKind::Fragment
        );
        assert_eq!(
            kind("/services/data/v68.0/query/01g-1?x=1"),
            LinkErrorKind::UnexpectedQuery
        );
        assert_eq!(
            kind("/services/data/v68.0/query/a%2Fb"),
            LinkErrorKind::InvalidLocator
        );
        assert_eq!(
            kind("/services/data/v68.0/queryAll/01g-1"),
            LinkErrorKind::UnexpectedPath
        );
        assert_eq!(
            kind("/services/data/v68.0/query/a b"),
            LinkErrorKind::Malformed
        );
        assert_eq!(
            kind("/services/data/v68.0/query\\01g"),
            LinkErrorKind::Malformed
        );
    }

    #[test]
    fn result_links() {
        let c = ctx("https://org-a.my.salesforce.com");
        let job = QueryJobId::new("750R0000000zxr8IAA").unwrap();
        let link = c
            .result_link(
                &job,
                "/services/data/v68.0/jobs/query/750R0000000zxr8IAA/results?locator=aBcDeFg4N",
            )
            .unwrap();
        assert_eq!(link.locator.unwrap().expose(), "aBcDeFg4N");
        let link = c
            .result_link(
                &job,
                "/services/data/v68.0/jobs/query/750R0000000zxr8IAA/results?locator=x&maxRecords=500",
            )
            .unwrap();
        assert_eq!(link.max_records, Some(MaxRecords::new(500).unwrap()));
        let kind = |raw: &str| c.result_link(&job, raw).unwrap_err().kind();
        assert_eq!(
            kind("/services/data/v68.0/jobs/query/750R0000000zxr9IAA/results?locator=x"),
            LinkErrorKind::WrongJob
        );
        assert_eq!(
            kind("/services/data/v68.0/jobs/query/750R0000000zxr8IAA/results?locator=x&locator=y"),
            LinkErrorKind::UnexpectedQuery
        );
        assert_eq!(
            kind("/services/data/v68.0/jobs/query/750R0000000zxr8IAA/results?locator=x&q=1"),
            LinkErrorKind::UnexpectedQuery
        );
        assert_eq!(
            kind("/services/data/v68.0/jobs/query/750R0000000zxr8IAA/results?maxRecords=0"),
            LinkErrorKind::InvalidMaxRecords
        );
        assert_eq!(
            kind("/services/data/v68.0/jobs/query/750R0000000zxr8IAA/results?locator=a%3D"),
            LinkErrorKind::InvalidLocator
        );
        let next = c
            .result_pages_link(
                &job,
                "/services/data/v68.0/jobs/query/750R0000000zxr8IAA/resultpages?locator=YcApWm4N",
            )
            .unwrap();
        assert_eq!(next.expose(), "YcApWm4N");
        assert!(
            c.result_pages_link(
                &job,
                "/services/data/v68.0/jobs/query/750R0000000zxr8IAA/resultPages?locator=Y"
            )
            .is_ok()
        );
        assert_eq!(
            c.result_pages_link(
                &job,
                "/services/data/v68.0/jobs/query/750R0000000zxr8IAA/resultPages"
            )
            .unwrap_err()
            .kind(),
            LinkErrorKind::UnexpectedQuery
        );
    }

    #[test]
    fn content_and_record_links() {
        let c = ctx("https://org-a.my.salesforce.com");
        let job = IngestJobId::new("7506g00000DhRA2AAN").unwrap();
        assert!(
            c.content_url(
                &job,
                "services/data/v68.0/jobs/ingest/7506g00000DhRA2AAN/batches"
            )
            .is_ok()
        );
        assert!(
            c.content_url(
                &job,
                "/services/data/v68.0/jobs/ingest/7506g00000DhRA2AAN/batches"
            )
            .is_ok()
        );
        assert_eq!(
            c.content_url(
                &job,
                "services/data/v68.0/jobs/ingest/7506g00000DhRA3AAN/batches"
            )
            .unwrap_err()
            .kind(),
            LinkErrorKind::WrongJob
        );
        let (object, id) = c
            .record_link("/services/data/v68.0/sobjects/Contact/003RO0000035WQgYAM")
            .unwrap();
        assert_eq!(object.as_str(), "Contact");
        assert_eq!(id.as_str(), "003RO0000035WQgYAM");
    }

    #[test]
    fn debug_hides_the_token() {
        let c = ctx("https://org-a.my.salesforce.com");
        let shown = format!("{c:?} {:?}", c.api());
        assert!(!shown.contains("synthetic"), "{shown}");
    }
}
