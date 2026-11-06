use std::fmt;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::HeaderValue;

/// Invalid credential syntax. Contains no input, even when formatted as an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialError {
    /// A Basic username contains a colon or a control character.
    Username,
    /// A Basic password contains a control character.
    Password,
    /// An authentication scheme is empty or is not an HTTP token.
    Scheme,
    /// A token is empty, contains whitespace/control characters or is not visible ASCII.
    Token,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Username => "invalid Basic username",
            Self::Password => "invalid Basic password",
            Self::Scheme => "invalid authentication scheme",
            Self::Token => "invalid credential token",
        })
    }
}

impl std::error::Error for CredentialError {}

/// Sensitive HTTP Basic credentials using standard padded Base64 of UTF-8 `user:password`.
///
/// Empty usernames and passwords are allowed; the separating colon is always encoded.
/// Colons in passwords are allowed. Username colons and Unicode control characters in either
/// input are refused. Provider-specific nonempty/charset rules belong in the SDK.
pub fn basic(user: &str, password: &str) -> Result<HeaderValue, CredentialError> {
    if user.contains(':') || user.chars().any(char::is_control) {
        return Err(CredentialError::Username);
    }
    if password.chars().any(char::is_control) {
        return Err(CredentialError::Password);
    }
    let encoded = STANDARD.encode(format!("{user}:{password}"));
    // The fixed prefix and Base64 alphabet are valid visible ASCII.
    let mut header = HeaderValue::from_str(&format!("Basic {encoded}")).expect("Base64 header");
    header.set_sensitive(true);
    Ok(header)
}

/// Sensitive Bearer credentials. Rejects empty tokens and non-visible ASCII, including controls.
/// Provider-specific token grammar is deliberately left to the SDK.
pub fn bearer(token: &str) -> Result<HeaderValue, CredentialError> {
    authorization("Bearer", token)
}

/// Sensitive `scheme token` credentials for an SDK-selected HTTP authentication scheme.
///
/// `scheme` must be a nonempty HTTP token. `token` must be nonempty visible ASCII without spaces;
/// this function does not construct multi-parameter authentication documents or signatures.
pub fn authorization(scheme: &str, token: &str) -> Result<HeaderValue, CredentialError> {
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    {
        return Err(CredentialError::Scheme);
    }
    if token.is_empty() || !token.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(CredentialError::Token);
    }
    let mut header =
        HeaderValue::from_str(&format!("{scheme} {token}")).expect("validated ASCII header");
    header.set_sensitive(true);
    Ok(header)
}
