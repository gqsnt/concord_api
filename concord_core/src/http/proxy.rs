use url::Url;

use crate::error::{ConfigurationError, ConfigurationErrorKind};

/// A credential-free HTTP(S) proxy origin.
pub struct SafeProxy {
    url: Url,
}

impl SafeProxy {
    /// Parses and validates one credential-free HTTP(S) origin.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ConfigurationError> {
        let value = value.as_ref();
        let url = Url::parse(value).map_err(|_| invalid_proxy())?;
        if value.bytes().any(|byte| byte.is_ascii_whitespace())
            || !matches!(url.scheme(), "http" | "https")
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || has_userinfo(value)
            || has_empty_port(value)
            || !has_only_root_path(value)
        {
            return Err(invalid_proxy());
        }
        Ok(Self { url })
    }

    #[allow(
        dead_code,
        reason = "used by Phase 1 managed Reqwest client construction"
    )]
    pub(crate) fn as_str(&self) -> &str {
        self.url.as_str()
    }
}

fn invalid_proxy() -> ConfigurationError {
    ConfigurationError::new(ConfigurationErrorKind::InvalidProxy)
}

fn authority_and_suffix(value: &str) -> Option<(&str, &str)> {
    let scheme_end = value.find("://")?;
    let after_scheme = &value[scheme_end + 3..];
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    Some(after_scheme.split_at(authority_end))
}

fn has_userinfo(value: &str) -> bool {
    authority_and_suffix(value).is_some_and(|(authority, _)| authority.contains('@'))
}

fn has_empty_port(value: &str) -> bool {
    authority_and_suffix(value).is_some_and(|(authority, _)| authority.ends_with(':'))
}

fn has_only_root_path(value: &str) -> bool {
    authority_and_suffix(value).is_some_and(|(_, suffix)| matches!(suffix, "" | "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_credential_free_http_origins() {
        for value in [
            "http://proxy.example",
            "https://proxy.example/",
            "https://proxy.example:8443",
            "http://127.0.0.1:8080/",
            "http://[::1]:8080",
        ] {
            assert!(SafeProxy::parse(value).is_ok(), "should accept {value:?}");
        }
    }

    #[test]
    fn rejects_credentials_schemes_paths_queries_fragments_and_malformed_values() {
        for value in [
            "http://user@proxy.example",
            "https://user:password@proxy.example",
            "http://@proxy.example",
            "http://user%40name@proxy.example",
            "ftp://proxy.example",
            "http:///missing-host",
            "http://proxy.example/path",
            "http://proxy.example/.",
            "http://proxy.example//",
            "http://proxy.example?mode=forward",
            "http://proxy.example?",
            "http://proxy.example#fragment",
            "http://proxy.example#",
            "http://proxy.example:",
            "http://[::1]:",
            " http://proxy.example",
            "http://proxy.example ",
            "proxy.example:8080",
            "not a URL",
            "",
        ] {
            let result = SafeProxy::parse(value);
            assert!(result.is_err(), "should reject {value:?}");
            let error = match result {
                Ok(_) => unreachable!("checked above"),
                Err(error) => error,
            };
            assert_eq!(error.kind(), ConfigurationErrorKind::InvalidProxy);
        }
    }
}
