use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RedactionSentinels {
    pub auth: &'static str,
    pub body: &'static str,
    pub response: &'static str,
}

impl RedactionSentinels {
    pub const fn new(auth: &'static str, body: &'static str, response: &'static str) -> Self {
        Self {
            auth,
            body,
            response,
        }
    }

    pub const fn auth_body(self) -> [&'static str; 2] {
        [self.auth, self.body]
    }

    pub const fn all(self) -> [&'static str; 3] {
        [self.auth, self.body, self.response]
    }
}

impl fmt::Debug for RedactionSentinels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedactionSentinels")
            .field("auth", &"<redacted>")
            .field("body", &"<redacted>")
            .field("response", &"<redacted>")
            .finish()
    }
}

pub fn assert_text_does_not_contain_any(text: &str, sentinels: &[&str]) {
    for (idx, sentinel) in sentinels.iter().enumerate() {
        assert!(
            !text.contains(sentinel),
            "text leaked redaction sentinel at index {idx}"
        );
    }
}

pub fn assert_error_chain_does_not_contain_any(err: &(dyn Error + 'static), sentinels: &[&str]) {
    assert_text_does_not_contain_any(&err.to_string(), sentinels);
    assert_text_does_not_contain_any(&format!("{err:?}"), sentinels);
    assert_text_does_not_contain_any(&format!("{err:#?}"), sentinels);

    let mut current = err.source();
    while let Some(source) = current {
        assert_text_does_not_contain_any(&source.to_string(), sentinels);
        assert_text_does_not_contain_any(&format!("{source:?}"), sentinels);
        assert_text_does_not_contain_any(&format!("{source:#?}"), sentinels);
        current = source.source();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RedactionSentinels, assert_error_chain_does_not_contain_any,
        assert_text_does_not_contain_any,
    };

    #[test]
    fn redaction_sentinels_debug_hides_raw_values() {
        let sentinels = RedactionSentinels::new(
            "RAW_AUTH_SENTINEL_TEST",
            "RESPONSE_BODY_SENTINEL_TEST",
            "RESPONSE_SENTINEL_TEST",
        );

        let debug = format!("{sentinels:?}");
        assert!(debug.contains("<redacted>"));
        assert_text_does_not_contain_any(&debug, &sentinels.all());
    }

    #[test]
    fn assert_text_does_not_contain_any_rejects_present_sentinel() {
        let clean = "nothing sensitive here";
        assert_text_does_not_contain_any(clean, &["SENSITIVE"]);

        let leaked = std::panic::catch_unwind(|| {
            assert_text_does_not_contain_any("contains SENSITIVE data", &["SENSITIVE"]);
        });
        assert!(leaked.is_err());
    }

    #[test]
    fn assert_error_chain_does_not_contain_any_checks_sources() {
        #[derive(Debug)]
        struct SourceError;

        impl std::fmt::Display for SourceError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "clean source")
            }
        }

        impl std::error::Error for SourceError {}

        #[derive(Debug)]
        struct WrappedError(SourceError);

        impl std::fmt::Display for WrappedError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "wrapped clean error")
            }
        }

        impl std::error::Error for WrappedError {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let err = WrappedError(SourceError);
        assert_error_chain_does_not_contain_any(&err, &["SENSITIVE"]);
    }
}
