//! The two Datadog keys every SDK request carries.

use std::fmt;

use concord::reqwest::header::HeaderValue;

use crate::error::{AuthError, Key};

/// An API key and an application key, checked before any request exists.
///
/// Datadog requires an API key for every request and an application key for reads. This SDK
/// deliberately uses the same pair for posting as for reads. Both are site-specific. They are sent
/// in their own headers (`DD-API-KEY`, `DD-APPLICATION-KEY`), flagged sensitive, and never
/// printed: `Debug` shows `<redacted>` for each, and no error of this crate contains either.
///
/// ```
/// use concord_sdk_datadog_events::{AuthError, Credentials};
///
/// let credentials = Credentials::new("synthetic-api-key", "synthetic-app-key").unwrap();
/// assert!(!format!("{credentials:?}").contains("synthetic"));
///
/// // An application key that is missing (an unset environment variable, say) is an
/// // authentication error before anything is sent.
/// let error = Credentials::new("synthetic-api-key", "").unwrap_err();
/// assert_eq!(error, AuthError::MissingApplicationKey);
/// ```
#[derive(Clone)]
pub struct Credentials {
    pub(crate) api_key: String,
    pub(crate) application_key: String,
}

impl Credentials {
    /// Checks and keeps both keys.
    ///
    /// The SDK does not check the keys' format (the cited pages do not define one); it refuses
    /// what cannot be a key at all.
    ///
    /// # Errors
    ///
    /// - [`AuthError::MissingApiKey`] / [`AuthError::MissingApplicationKey`] when a key is
    ///   empty or only whitespace;
    /// - [`AuthError::MalformedKey`] when a key holds bytes an HTTP header cannot carry (a line
    ///   break from a file or an environment variable, a control character).
    ///
    /// The error names the key, never its value.
    pub fn new(
        api_key: impl Into<String>,
        application_key: impl Into<String>,
    ) -> Result<Self, AuthError> {
        let api_key = api_key.into();
        let application_key = application_key.into();
        check(&api_key, Key::Api)?;
        check(&application_key, Key::Application)?;
        Ok(Self {
            api_key,
            application_key,
        })
    }
}

fn check(value: &str, key: Key) -> Result<(), AuthError> {
    if value.trim().is_empty() {
        return Err(match key {
            Key::Api => AuthError::MissingApiKey,
            Key::Application => AuthError::MissingApplicationKey,
        });
    }
    // `http`'s own rule for a header value: the same check Concord applies when it builds the
    // request, made here so that a bad key is an authentication error, not a build error.
    HeaderValue::from_str(value).map_err(|_| AuthError::MalformedKey(key))?;
    Ok(())
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("api_key", &format_args!("<redacted>"))
            .field("application_key", &format_args!("<redacted>"))
            .finish()
    }
}
