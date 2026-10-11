//! Exact provider-helper HTTP origins (AUTH-02).

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use url::Url;

/// Maximum UTF-8 bytes accepted when parsing one provider origin.
pub const MAX_PROVIDER_ORIGIN_BYTES: usize = 2_048;

/// One validated, exact, normalized HTTP(S) origin that a credential provider
/// declares for Concord's provider HTTP helper.
///
/// An origin is the triple *(scheme, host, effective port)*. Construction
/// normalizes the scheme and host to lowercase ASCII (IDNA hosts become their
/// ASCII form), makes the default port explicit for comparison, and rejects
/// user information, paths other than `/`, queries and fragments. Two origins
/// are equal exactly when their normalized triples are equal; no wildcard,
/// suffix or subdomain matching exists.
///
/// Declared origins are **capabilities**: [`ProviderHttp`](crate::contracts::ProviderHttp)
/// refuses, before any transport work or body-factory call, every request
/// whose URL origin is not in the providing credential's declared set. The
/// set is read once when the client is built (or when a provider is
/// explicitly replaced) and cannot grow afterwards. The built-in helper is
/// HTTPS-only, so declaring an `http` origin is rejected when the set is
/// validated.
///
/// Origins are not credentials, but hosts can reveal private infrastructure:
/// `Debug` redacts the host, while [`ProviderOrigin::as_str`] is explicit
/// application-data access.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ProviderOrigin {
    scheme: OriginScheme,
    host: Box<str>,
    port: u16,
    serialized: Box<str>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum OriginScheme {
    Http,
    Https,
}

impl OriginScheme {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

impl ProviderOrigin {
    /// Parses one exact origin such as `https://auth.example.com` or
    /// `https://auth.example.com:8443/`.
    ///
    /// A single trailing `/` is accepted; any other path, a query, a fragment
    /// or user information is rejected. Errors are value-free.
    pub fn parse(origin: impl AsRef<str>) -> Result<Self, ProviderOriginError> {
        let origin = origin.as_ref();
        if origin.len() > MAX_PROVIDER_ORIGIN_BYTES {
            return Err(ProviderOriginError::new(ProviderOriginErrorKind::TooLong));
        }
        let url = Url::parse(origin)
            .map_err(|_| ProviderOriginError::new(ProviderOriginErrorKind::Syntax))?;
        if (url.path() != "/" && !url.path().is_empty())
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ProviderOriginError::new(
                ProviderOriginErrorKind::PathQueryOrFragment,
            ));
        }
        Self::from_url(&url)
    }

    /// Extracts the exact origin of one absolute HTTP(S) URL, for example a
    /// provider token endpoint. The URL's path and query are ignored; user
    /// information is rejected.
    pub fn from_url(url: &Url) -> Result<Self, ProviderOriginError> {
        let scheme = match url.scheme() {
            "https" => OriginScheme::Https,
            "http" => OriginScheme::Http,
            _ => {
                return Err(ProviderOriginError::new(
                    ProviderOriginErrorKind::UnsupportedScheme,
                ));
            }
        };
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ProviderOriginError::new(ProviderOriginErrorKind::UserInfo));
        }
        let Some(host) = url.host_str().filter(|host| !host.is_empty()) else {
            return Err(ProviderOriginError::new(
                ProviderOriginErrorKind::MissingHost,
            ));
        };
        let Some(port) = url.port_or_known_default() else {
            return Err(ProviderOriginError::new(ProviderOriginErrorKind::Syntax));
        };
        let serialized = url.origin().ascii_serialization();
        if serialized.len() > MAX_PROVIDER_ORIGIN_BYTES {
            return Err(ProviderOriginError::new(ProviderOriginErrorKind::TooLong));
        }
        Ok(Self {
            scheme,
            host: host.into(),
            port,
            serialized: serialized.into_boxed_str(),
        })
    }

    /// Returns the lowercase scheme, `http` or `https`.
    #[must_use]
    pub const fn scheme(&self) -> &'static str {
        self.scheme.as_str()
    }

    /// Returns the normalized ASCII host; IPv6 literals keep their brackets.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the effective port, including the scheme default.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns the normalized serialization `scheme://host[:port]`, with the
    /// scheme-default port omitted.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// Returns whether `url` belongs to exactly this origin.
    #[must_use]
    pub fn matches(&self, url: &Url) -> bool {
        url.scheme() == self.scheme.as_str()
            && url.host_str() == Some(&*self.host)
            && url.port_or_known_default() == Some(self.port)
    }

    pub(crate) const fn is_https(&self) -> bool {
        matches!(self.scheme, OriginScheme::Https)
    }
}

impl fmt::Debug for ProviderOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderOrigin")
            .field("scheme", &self.scheme.as_str())
            .field("host", &"<redacted>")
            .field("port", &self.port)
            .finish()
    }
}

/// Stable category for one invalid provider origin.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderOriginErrorKind {
    /// The text was not an absolute URL.
    Syntax,
    /// The input exceeded [`MAX_PROVIDER_ORIGIN_BYTES`].
    TooLong,
    /// The scheme was neither `http` nor `https`.
    UnsupportedScheme,
    /// The URL had no host.
    MissingHost,
    /// The URL carried user information.
    UserInfo,
    /// An origin string carried a path other than `/`, a query or a fragment.
    PathQueryOrFragment,
}

/// A value-free provider-origin validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderOriginError {
    kind: ProviderOriginErrorKind,
}

impl ProviderOriginError {
    const fn new(kind: ProviderOriginErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ProviderOriginErrorKind {
        self.kind
    }
}

impl fmt::Display for ProviderOriginError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self.kind {
            ProviderOriginErrorKind::Syntax => "not an absolute origin",
            ProviderOriginErrorKind::TooLong => "origin exceeds the supported byte limit",
            ProviderOriginErrorKind::UnsupportedScheme => "origin scheme must be http or https",
            ProviderOriginErrorKind::MissingHost => "origin requires a host",
            ProviderOriginErrorKind::UserInfo => "origin must not contain user information",
            ProviderOriginErrorKind::PathQueryOrFragment => {
                "origin must not contain a path, query or fragment"
            }
        };
        write!(formatter, "invalid provider origin: {detail}")
    }
}

impl Error for ProviderOriginError {}

/// Why a provider's declared origin set was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OriginSetError {
    /// More entries were declared than the configured capacity allows.
    TooMany,
    /// A cleartext origin was declared for the HTTPS-only helper.
    Cleartext,
}

/// Validates and snapshots one provider's declared origin set.
///
/// Every declared entry counts toward `capacity` (duplicates included, so the
/// work is bounded by the capacity); exact duplicates are then collapsed.
pub(crate) fn validated_origin_set(
    declared: &[ProviderOrigin],
    capacity: usize,
) -> Result<Arc<[ProviderOrigin]>, OriginSetError> {
    if declared.len() > capacity {
        return Err(OriginSetError::TooMany);
    }
    let mut unique: Vec<ProviderOrigin> = Vec::with_capacity(declared.len());
    for origin in declared {
        if !origin.is_https() {
            return Err(OriginSetError::Cleartext);
        }
        if !unique.contains(origin) {
            unique.push(origin.clone());
        }
    }
    Ok(unique.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_normalize_and_compare_exactly() {
        let origin = ProviderOrigin::parse("HTTPS://Auth.Example.TEST:443/").expect("origin");
        assert_eq!(origin.scheme(), "https");
        assert_eq!(origin.host(), "auth.example.test");
        assert_eq!(origin.port(), 443);
        assert_eq!(origin.as_str(), "https://auth.example.test");
        assert_eq!(
            origin,
            ProviderOrigin::parse("https://auth.example.test").expect("same origin")
        );
        let explicit = ProviderOrigin::parse("https://auth.example.test:8443").expect("port");
        assert_ne!(origin, explicit);
        assert_eq!(explicit.as_str(), "https://auth.example.test:8443");

        let token = Url::parse("https://auth.example.test/oauth/token?x=1").expect("url");
        assert!(origin.matches(&token));
        assert!(!explicit.matches(&token));
        assert!(!origin.matches(&Url::parse("https://evil.auth.example.test/token").expect("url")));
        assert!(!origin.matches(&Url::parse("http://auth.example.test/token").expect("url")));
        assert_eq!(ProviderOrigin::from_url(&token).expect("from url"), origin);

        let ipv6 = ProviderOrigin::parse("https://[::1]:9443").expect("ipv6");
        assert_eq!(ipv6.host(), "[::1]");
        assert!(ipv6.matches(&Url::parse("https://[::1]:9443/token").expect("url")));
    }

    #[test]
    fn invalid_origins_fail_with_value_free_errors() {
        const SENTINEL: &str = "origin-secret-sentinel-4b1c";
        for (input, kind) in [
            ("not a url", ProviderOriginErrorKind::Syntax),
            (
                "ftp://host.test",
                ProviderOriginErrorKind::UnsupportedScheme,
            ),
            (
                "https://user:origin-secret-sentinel-4b1c@host.test",
                ProviderOriginErrorKind::UserInfo,
            ),
            (
                "https://host.test/origin-secret-sentinel-4b1c",
                ProviderOriginErrorKind::PathQueryOrFragment,
            ),
            (
                "https://host.test/?q=origin-secret-sentinel-4b1c",
                ProviderOriginErrorKind::PathQueryOrFragment,
            ),
            (
                "https://host.test/#origin-secret-sentinel-4b1c",
                ProviderOriginErrorKind::PathQueryOrFragment,
            ),
        ] {
            let error = ProviderOrigin::parse(input).expect_err("invalid origin");
            assert_eq!(error.kind(), kind, "{input}");
            assert!(!error.to_string().contains(SENTINEL));
            assert!(!format!("{error:?}").contains(SENTINEL));
        }
        let too_long = format!("https://{}.test", "a".repeat(MAX_PROVIDER_ORIGIN_BYTES));
        assert_eq!(
            ProviderOrigin::parse(too_long)
                .expect_err("too long")
                .kind(),
            ProviderOriginErrorKind::TooLong
        );
        let origin = ProviderOrigin::parse("https://private-host-sentinel.test").expect("ok");
        assert!(!format!("{origin:?}").contains("private-host-sentinel"));
    }

    #[test]
    fn origin_sets_are_bounded_https_only_and_deduplicated() {
        let first = ProviderOrigin::parse("https://a.test").expect("a");
        let second = ProviderOrigin::parse("https://b.test").expect("b");
        let set = validated_origin_set(&[first.clone(), second.clone(), first.clone()], 3)
            .expect("bounded set");
        assert_eq!(&*set, &[first.clone(), second.clone()]);
        assert_eq!(
            validated_origin_set(&[first.clone(), second, first.clone()], 2),
            Err(OriginSetError::TooMany)
        );
        let cleartext = ProviderOrigin::parse("http://a.test").expect("http");
        assert_eq!(
            validated_origin_set(&[first, cleartext], 8),
            Err(OriginSetError::Cleartext)
        );
        assert!(validated_origin_set(&[], 8).expect("empty").is_empty());
    }
}
