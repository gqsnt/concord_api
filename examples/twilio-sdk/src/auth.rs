//! Basic credentials, encoded once and never rendered by SDK diagnostics.

use std::fmt;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use concord::reqwest::header::HeaderValue;

/// Configured Basic authentication, independent of the account in the request path.
///
/// The username can represent an API key SID or account SID; the password its secret or
/// auth token. Offline examples deliberately use invalid, visibly synthetic credentials.
#[derive(Clone)]
pub struct BasicAuth(HeaderValue);

impl BasicAuth {
    /// Encodes `username:password` using standard padded Base64.
    ///
    /// Rejects empty credentials, control characters and a colon in the username. A colon in
    /// the password is allowed. Values are not checked against Twilio credential formats.
    pub fn new(username: &str, password: &str) -> Result<Self, AuthError> {
        if username.is_empty() || username.contains(':') || username.chars().any(char::is_control) {
            return Err(AuthError::Username);
        }
        if password.is_empty() || password.chars().any(char::is_control) {
            return Err(AuthError::Password);
        }
        let encoded = STANDARD.encode(format!("{username}:{password}"));
        let mut header =
            HeaderValue::from_str(&format!("Basic {encoded}")).map_err(|_| AuthError::Header)?;
        header.set_sensitive(true);
        Ok(Self(header))
    }

    pub(crate) fn into_header(self) -> HeaderValue {
        self.0
    }
}

impl fmt::Debug for BasicAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BasicAuth(<redacted>)")
    }
}

/// Invalid Basic credentials. Diagnostics never include the supplied values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    /// Empty username, a colon, or a control character.
    Username,
    /// Empty password or a control character.
    Password,
    /// The encoded header could not be represented by HTTP's header type.
    Header,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Username => "invalid Basic username",
            Self::Password => "invalid Basic password",
            Self::Header => "invalid Basic header",
        })
    }
}

impl std::error::Error for AuthError {}
