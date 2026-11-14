//! The start of a URL, when the caller provides it.

use std::fmt;
use std::str::FromStr;

/// A scheme, a host, an optional port, and an optional leading part of a path.
///
/// Use it for a server that the caller hosts: `client Gitlab(url: Base, …) { base url; … }`,
/// and with `with_base` on any client.
///
/// ```
/// # use concord_core as concord;
/// let base: concord::Base = "https://example.com/gitlab".parse()?;
/// assert_eq!(base.to_string(), "https://example.com/gitlab");
/// # Ok::<(), concord::base::BaseError>(())
/// ```
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Base {
    text: String,
}

impl fmt::Debug for Base {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Base(..)")
    }
}

impl FromStr for Base {
    type Err = BaseError;

    /// Rejects user information, a query, a fragment, a backslash and ASCII control characters.
    /// The parsed scheme, host, port and decoded path must mean what the text says.
    /// An empty path is the root; equivalent host spellings and escapes are accepted.
    /// A trailing `/` is kept; only joining a following `path` removes the
    /// separator between the base and that path.
    fn from_str(text: &str) -> Result<Base, BaseError> {
        let error = |what| BaseError { what };
        if text.bytes().any(|b| b == b'\\' || b.is_ascii_control()) {
            return Err(error(
                "a backslash or ASCII control character is not allowed",
            ));
        }
        let (scheme, authority_and_path) = text
            .split_once("://")
            .filter(|(scheme, _)| {
                scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")
            })
            .ok_or_else(|| error("expected http:// or https://"))?;
        let authority = authority_and_path
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("");
        if authority.contains('@') {
            return Err(error("a user is not allowed"));
        }
        let url = reqwest::Url::parse(text).map_err(|_| error("invalid URL"))?;
        if url.host().is_none() {
            return Err(error("a host is required"));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(error("a query or fragment is not allowed"));
        }
        // Check the written prefix so parsing cannot hide invalid dot segments.
        let path = authority_and_path
            .get(authority.len()..)
            .ok_or_else(|| error("invalid URL"))?;
        if !valid_prefix(path) {
            return Err(error("invalid path prefix"));
        }
        let host_end = if authority.starts_with('[') {
            // PANIC-SAFETY: find returns an ASCII delimiter boundary within authority; adding one stays at or below its length.
            authority.find(']').ok_or_else(|| error("invalid URL"))? + 1
        } else {
            authority.find(':').unwrap_or(authority.len())
        };
        // PANIC-SAFETY: find returns an ASCII delimiter boundary within authority; adding one stays at or below its length.
        let (written_host, port_suffix) = authority.split_at(host_end);
        let host_url = reqwest::Url::parse(&format!("{scheme}://{written_host}/"))
            .map_err(|_| error("invalid URL"))?;
        let written_port = match port_suffix {
            "" | ":" => None,
            port => Some(
                port.strip_prefix(':')
                    .and_then(|port| port.parse::<u16>().ok())
                    .ok_or_else(|| error("invalid URL"))?,
            ),
        };
        let same_port = match written_port {
            Some(port) => url.port_or_known_default() == Some(port),
            None => url.port().is_none(),
        };
        let written_path = percent_decode(if path.is_empty() { "/" } else { path })
            .ok_or_else(|| error("invalid URL"))?;
        let parsed_path = percent_decode(url.path()).ok_or_else(|| error("invalid URL"))?;
        if !scheme.eq_ignore_ascii_case(url.scheme())
            || host_url.host() != url.host()
            || !same_port
            || written_path != parsed_path
        {
            return Err(error("invalid URL"));
        }
        Ok(Base { text: url.into() })
    }
}

impl Base {
    // Test-only reference implementation of the path separator rule.
    #[cfg(test)]
    pub(crate) fn join_path(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.text.strip_suffix('/').unwrap_or(&self.text),
            path
        )
    }
}

fn percent_decode(text: &str) -> Option<Vec<u8>> {
    fn hex(byte: u8) -> Option<u8> {
        // PANIC-SAFETY: to_digit(16) returns only 0..15, so narrowing to u8 is lossless.
        (byte as char).to_digit(16).map(|n| n as u8)
    }
    let mut decoded = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next().and_then(hex)?;
            let low = bytes.next().and_then(hex)?;
            // PANIC-SAFETY: Each hex digit is 0..15; high * 16 + low is at most 255.
            decoded.push(high * 16 + low);
        } else {
            decoded.push(byte);
        }
    }
    Some(decoded)
}

fn valid_prefix(path: &str) -> bool {
    let prefix = path.strip_prefix('/').unwrap_or(path);
    let prefix = prefix.strip_suffix('/').unwrap_or(prefix);
    if prefix.is_empty() {
        return path.is_empty() || path == "/";
    }
    prefix.split('/').all(|segment| {
        let Some(decoded) = percent_decode(segment) else {
            return false;
        };
        !decoded.is_empty()
            && decoded != b"."
            && decoded != b".."
            && !decoded.iter().any(|b| {
                b.is_ascii_control() || matches!(b, b' ' | b'/' | b'\\' | b'?' | b'#' | b'%')
            })
    })
}

impl fmt::Display for Base {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// The error for a string that is not a valid [`Base`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseError {
    what: &'static str,
}

impl fmt::Display for BaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a base URL: {}", self.what)
    }
}

impl std::error::Error for BaseError {}

/// A parsed base, or a string, that a generated client's `with_base` accepts.
///
/// Accepts [`Base`], `&Base`, `&str`, [`String`] and `&String`. A string is checked by
/// [`Base::from_str`]. A generated client's `with_base` keeps any parse failure
/// until the request is built, where it becomes a Build error.
///
/// Sealed: only Concord implements this trait. To validate eagerly, parse a
/// [`Base`] first and pass that value to `with_base`.
pub trait IntoBase: sealed::Sealed {
    /// Parses a string, or keeps a base that is already checked.
    fn into_base(self) -> Result<Base, BaseError>;
}

impl IntoBase for Base {
    fn into_base(self) -> Result<Base, BaseError> {
        Ok(self)
    }
}

impl IntoBase for &Base {
    fn into_base(self) -> Result<Base, BaseError> {
        Ok(self.clone())
    }
}

impl IntoBase for &str {
    fn into_base(self) -> Result<Base, BaseError> {
        self.parse()
    }
}

impl IntoBase for String {
    fn into_base(self) -> Result<Base, BaseError> {
        self.parse()
    }
}

impl IntoBase for &String {
    fn into_base(self) -> Result<Base, BaseError> {
        self.as_str().parse()
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Base {}
    impl Sealed for &super::Base {}
    impl Sealed for &str {}
    impl Sealed for String {}
    impl Sealed for &String {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_slash_distinguishes_bases_until_a_path_is_joined() {
        let with_slash: Base = "https://example.test/prefix/".parse().unwrap();
        let without_slash: Base = "https://example.test/prefix".parse().unwrap();
        assert_ne!(with_slash, without_slash);
        assert_eq!(with_slash.to_string(), "https://example.test/prefix/");
        assert_eq!(without_slash.to_string(), "https://example.test/prefix");
        for base in [with_slash, without_slash] {
            assert_eq!(base.join_path("path"), "https://example.test/prefix/path");
            assert_eq!(base.join_path("path/"), "https://example.test/prefix/path/");
        }
    }

    #[test]
    fn base_rejects_user_query_fragment_and_invalid_prefixes() {
        for text in [
            "ftp://example.test/prefix/",
            "https://user@example.test/prefix/",
            "https://example.test/prefix/?query",
            "https://example.test/prefix/#fragment",
            "https://example.test/prefix/../",
            "https://example.test/%2e/",
            "https://example.test/prefix//",
            "https://example.test/a%2fb/",
        ] {
            assert!(text.parse::<Base>().is_err(), "{text}");
        }
    }

    #[test]
    fn base_must_reject_missing_written_authority_and_recovered_credentials() {
        for text in [
            "https:///user:pass@example.test/prefix/",
            "https:///user@example.test/prefix/",
            "http:///@example.test/prefix/",
            "https:///example.test/prefix/",
        ] {
            assert_eq!(
                text.parse::<Base>().unwrap_err().to_string(),
                "not a base URL: invalid URL",
                "{text}"
            );
        }
    }

    #[test]
    fn decoded_base_prefix_must_obey_the_backslash_ban() {
        for piece in ["%5c", "%5C", "a%5Cb", "prefix%5c..%5cadmin"] {
            for ending in ["", "/"] {
                let text = format!("https://example.test/{piece}{ending}");
                assert_eq!(
                    text.parse::<Base>().unwrap_err().to_string(),
                    "not a base URL: invalid path prefix",
                    "{text}"
                );
            }
        }
    }

    #[test]
    fn base_preserves_trailing_slash_and_joins_simple_path() {
        // Encoded backslashes are refused in a base URL.
        for prefix in ["prefix", "prefix%41", "caf%C3%A9", "a%5Cb"] {
            for ending in ["", "/"] {
                let text = format!("https://example.test/{prefix}{ending}");
                let parsed = text.parse::<Base>();
                if prefix == "a%5Cb" {
                    assert!(parsed.is_err(), "{text}");
                } else {
                    let base = parsed.unwrap();
                    assert_eq!(base.to_string(), text);
                    assert_eq!(
                        base.join_path("path/"),
                        format!("https://example.test/{prefix}/path/")
                    );
                }
            }
        }
    }

    #[test]
    fn ordinary_canonicalization_and_valid_prefixes_still_work() {
        for (text, expected) in [
            (
                "https://EXAMPLE.test:443/Prefix/",
                "https://example.test/Prefix/",
            ),
            (
                "http://EXAMPLE.test:80/prefix",
                "http://example.test/prefix",
            ),
            (
                "HtTpS://EXAMPLE.test:443/Prefix/",
                "https://example.test/Prefix/",
            ),
            ("HTTP://example.test/", "http://example.test/"),
            (
                "https://[2001:DB8::1]:443/prefix/",
                "https://[2001:db8::1]/prefix/",
            ),
            (
                "https://[2001:0DB8:0:0:0:0:0:1]:443/prefix/",
                "https://[2001:db8::1]/prefix/",
            ),
            ("http://[::1]:8080/prefix", "http://[::1]:8080/prefix"),
            (
                "https://example.test:8443/prefix",
                "https://example.test:8443/prefix",
            ),
            (
                "https://example.test/prefix%41/%7e/caf%C3%A9/",
                "https://example.test/prefix%41/%7e/caf%C3%A9/",
            ),
            (
                "https://example.test/café/",
                "https://example.test/caf%C3%A9/",
            ),
            (
                "https://example.test/caf%c3%a9/%FF/",
                "https://example.test/caf%c3%a9/%FF/",
            ),
            ("https://example.test/v../", "https://example.test/v../"),
            ("https://example.test/", "https://example.test/"),
            ("https://example.test", "https://example.test/"),
            (
                "https://%65xample.test/prefix/",
                "https://example.test/prefix/",
            ),
            ("https://127.1/prefix/", "https://127.0.0.1/prefix/"),
            ("https://0x7f000001/prefix/", "https://127.0.0.1/prefix/"),
            (
                "https://éxample.test/prefix/",
                "https://xn--xample-9ua.test/prefix/",
            ),
            (
                "https://example.test:/prefix/",
                "https://example.test/prefix/",
            ),
            (
                "https://example.test:0443/prefix/",
                "https://example.test/prefix/",
            ),
            (
                "https://example.test:08443/prefix/",
                "https://example.test:8443/prefix/",
            ),
        ] {
            assert_eq!(
                text.parse::<Base>().unwrap().to_string(),
                expected,
                "{text}"
            );
        }
    }

    #[test]
    fn base_rejects_other_url_normalization() {
        for text in ["https://example.test ", "https://example.test:443 "] {
            let url = reqwest::Url::parse(text).unwrap();
            assert_ne!(url.as_str(), text, "{text}");
            assert_eq!(
                text.parse::<Base>().unwrap_err().to_string(),
                "not a base URL: invalid URL",
                "{text}"
            );
        }
    }

    #[test]
    fn named_base_errors_precede_the_round_trip_check() {
        for (text, expected) in [
            ("ftp://example.test/", "expected http:// or https://"),
            ("https://user@example.test/", "a user is not allowed"),
            ("https://example.test/prefix/./", "invalid path prefix"),
            ("https://example.test/prefix/%2e%2E/", "invalid path prefix"),
            ("https://example.test/prefix/ ", "invalid path prefix"),
            (
                "https://example.test/prefix/?",
                "a query or fragment is not allowed",
            ),
            (
                "https://example.test/prefix/#",
                "a query or fragment is not allowed",
            ),
            (
                "https://example.test\\prefix/",
                "a backslash or ASCII control character is not allowed",
            ),
            (
                "https://example.test/pre\tfix/",
                "a backslash or ASCII control character is not allowed",
            ),
        ] {
            assert_eq!(
                text.parse::<Base>().unwrap_err().to_string(),
                format!("not a base URL: {expected}"),
                "{text:?}"
            );
        }
    }

    #[test]
    fn base_must_not_normalize_away_a_prefix_through_backslashes() {
        for text in [
            r"https://example.test/prefix\..\admin/",
            r"https://example.test\prefix\..\admin/",
            r"https://example.test/prefix\.\admin/",
        ] {
            assert!(text.parse::<Base>().is_err(), "{text:?}");
        }
    }

    #[test]
    fn base_must_reject_ascii_controls_before_parsing() {
        for control in (0u8..=127).filter(u8::is_ascii_control).map(char::from) {
            for text in [
                format!("https://exam{control}ple.test/prefix/"),
                format!("https://example.test/pre{control}fix/"),
                format!("https://example.test/prefix/{control}"),
            ] {
                assert!(text.parse::<Base>().is_err(), "{text:?}");
            }
        }
    }

    #[test]
    fn join_keeps_empty_first_splat_segments() {
        for text in [
            "https://example.test/prefix",
            "https://example.test/prefix/",
        ] {
            let base: Base = text.parse().unwrap();
            // path "{*file}", with file = "/object", has an empty first segment.
            for path in ["/object", "//object", "/object/"] {
                assert_eq!(
                    base.join_path(path),
                    format!("https://example.test/prefix/{path}")
                );
            }
        }
    }

    #[test]
    fn joining_preserves_every_empty_rendered_piece_and_last_slash() {
        for stem in [
            "https://example.test/prefix",
            "https://example.test",
            "http://[::1]:8080/prefix",
        ] {
            for ending in ["", "/"] {
                let text = format!("{stem}{ending}");
                let base: Base = text.parse().unwrap();
                for path in [
                    "object", "object/", "/object", "//object", "/object/", "/", "//", "///",
                    "/a//b///",
                ] {
                    let expected = format!("{stem}/{path}");
                    let joined = base.join_path(path);
                    assert_eq!(joined, expected);
                    assert_eq!(reqwest::Url::parse(&joined).unwrap().as_str(), expected);
                }
            }
        }
    }

    #[test]
    fn bare_host_is_accepted_and_joins_a_path() {
        let base: Base = "https://api.example.com".parse().unwrap();
        // Base stored the library's serialized URL before the round-trip check.
        assert_eq!(base.to_string(), "https://api.example.com/");
        assert_eq!(
            base.join_path("v1/items"),
            "https://api.example.com/v1/items"
        );
    }

    #[test]
    fn debug_hides_the_base_url() {
        let base = Base {
            text: "https://private.example/secret-path".into(),
        };
        assert_eq!(format!("{base:?}"), "Base(..)");
        assert_eq!(format!("{base:#?}"), "Base(..)");
    }
}
