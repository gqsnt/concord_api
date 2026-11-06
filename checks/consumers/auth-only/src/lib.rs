//! A macro-free consumer, also run against the extracted auth package.

#[cfg(test)]
mod tests {
    use concord_auth::{Token, TokenCache, authorization, basic, bearer};
    use std::{convert::Infallible, num::NonZeroUsize, time::Duration};

    #[test]
    fn basic_credentials_are_sensitive_and_preserve_password_colons() {
        let header = basic("user", "pass:word").unwrap();
        assert_eq!(header, "Basic dXNlcjpwYXNzOndvcmQ=");
        assert!(header.is_sensitive());
        assert!(!format!("{header:?}").contains("dXNlcjpwYXNzOndvcmQ="));
        assert_eq!(basic("user", "").unwrap(), "Basic dXNlcjo=");
        assert!(basic("user:name", "password").is_err());
    }

    #[test]
    fn token_headers_refuse_controls_and_redact() {
        let sentinel = "synthetic-sensitive-token";
        for header in [
            bearer(sentinel).unwrap(),
            authorization("Token", sentinel).unwrap(),
        ] {
            assert!(header.is_sensitive());
            assert!(!format!("{header:?}").contains(sentinel));
        }
        assert!(bearer("bad\r\nvalue").is_err());
        assert!(authorization("Token", "bad\tvalue").is_err());
    }

    #[tokio::test]
    async fn scoped_cache_and_generation_invalidation() {
        // The SDK selects the complete scope; auth never derives one from a URL or token.
        let cache = TokenCache::<(&str, &str, &str), Infallible>::new(
            NonZeroUsize::new(2).unwrap(),
            Duration::ZERO,
        );
        let key = ("service-a", "principal-a", "read");
        let old = cache
            .get_or_refresh(key, || async {
                Ok(Token::new(
                    bearer("first-token").unwrap(),
                    Duration::from_secs(60),
                ))
            })
            .await
            .unwrap();
        let cached = cache
            .get_or_refresh(key, || async { panic!("cached call must not acquire") })
            .await
            .unwrap();
        assert_eq!(old.generation(), cached.generation());
        assert!(cache.get(&("service-b", "principal-a", "read")).is_none());
        assert!(cache.invalidate(&key, &old.generation()));
        let current = cache
            .get_or_refresh(key, || async {
                Ok(Token::new(
                    bearer("replacement-token").unwrap(),
                    Duration::from_secs(60),
                ))
            })
            .await
            .unwrap();
        assert_ne!(old.generation(), current.generation());
        assert!(!cache.invalidate(&key, &old.generation()));
        assert_eq!(cache.get(&key).unwrap().header(), current.header());
        assert!(current.header().is_sensitive());
        assert!(!format!("{current:?}").contains("replacement-token"));
    }
}
