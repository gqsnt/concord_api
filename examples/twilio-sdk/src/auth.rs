//! Basic credentials, encoded once and never rendered by SDK diagnostics.

use concord::reqwest::header::HeaderValue;

/// Configured Basic authentication, independent of the account in the request path.
///
/// The username can represent an API key SID or account SID; the password its secret or
/// auth token. Offline examples deliberately use invalid, visibly synthetic credentials.
#[derive(Clone, concord::Debug)]
pub struct BasicAuth(#[sensitive] HeaderValue);

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
        concord_auth::basic(username, password)
            .map(Self)
            .map_err(|_| AuthError::Header)
    }

    pub(crate) fn into_header(self) -> HeaderValue {
        self.0
    }
}

/// Invalid Basic credentials. Diagnostics never include the supplied values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, concord::derive::Error)]
#[display(with = display_auth_error)]
pub enum AuthError {
    /// Empty username, a colon, or a control character.
    Username,
    /// Empty password or a control character.
    Password,
    /// The encoded header could not be represented by HTTP's header type.
    Header,
}

fn display_auth_error(value: &AuthError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match value {
        AuthError::Username => "invalid Basic username",
        AuthError::Password => "invalid Basic password",
        AuthError::Header => "invalid Basic header",
    })
}
