//! The Bearer token flow of the registry's token authentication spec.
//!
//! 1. A request without (or with an expired) token gets `401 Unauthorized` with
//!    `WWW-Authenticate: Bearer realm="...",service="...",scope="..."`.
//! 2. The SDK asks the token service at `realm` for a token: `GET <realm>?service=<service>&
//!    scope=<scope>&scope=<scope>...` (one `scope` parameter per resource scope), with Basic
//!    credentials when configured.
//! 3. It retries the original request **once** with `Authorization: Bearer <token>`. A second
//!    `401` is the registry's answer ([`RegistryError::Api`](crate::RegistryError::Api)), never
//!    another round.
//!
//! **Scopes.** The token request asks for the challenge's `scope` values as sent; a challenge
//! without one gets the scope of the operation (`repository:<name>:pull` for reads,
//! `repository:<name>:pull,push` for writes, `registry:catalog:*` for the catalog). Tokens are
//! cached per operation access — pull, push, catalog, none — never shared between them: a pull
//! token is never sent with a push request. A cached token is sent pre-emptively until its
//! `expires_in` (60 s when absent, counted from when the token request was sent) has passed. An
//! `expires_in` too large for the platform's clock is [`AuthProblem::TokenResponse`]: that token is
//! neither cached nor sent.
//!
//! **Origins.** Concord's `Prepared::follow` and the SDK's `Location` and `Link` checks keep
//! server-provided URLs on the registry's origin. Token requests go to a realm whose origin is the
//! registry's or was trusted with [`TokenAuth::trust_realm`]; another realm is
//! [`AuthProblem::UntrustedRealm`] and nothing is sent to it. The caller's HTTP client must also
//! disable redirects or reject cross-origin redirects: it can follow a redirect before the SDK
//! sees the response, including for token requests and URLs containing opaque state.

use std::fmt;
use std::time::Instant;

use concord::reqwest::Url;
use concord::reqwest::header::{HeaderMap, HeaderValue, WWW_AUTHENTICATE};

use crate::error::{AuthProblem, InputError};
use crate::reference::RepositoryName;

/// How the client answers a Bearer challenge.
///
/// ```
/// use concord_sdk_distribution_registry::{BasicCredentials, TokenAuth};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let anonymous = TokenAuth::anonymous().trust_realm("https://auth.example.test")?;
/// let credentials = BasicCredentials::new("ci-bot", "synthetic-password-for-docs")?;
/// let signed_in = TokenAuth::basic(credentials).trust_realm("https://auth.example.test")?;
/// assert!(!format!("{signed_in:?}").contains("synthetic-password-for-docs"));
/// # let _ = anonymous; Ok(()) }
/// ```
#[derive(Clone)]
pub struct TokenAuth {
    trusted: Vec<Url>,
    credentials: Option<BasicCredentials>,
    client_id: Option<String>,
}

impl TokenAuth {
    /// Token requests without credentials (anonymous pulls, where the token service allows them).
    pub fn anonymous() -> Self {
        Self {
            trusted: Vec::new(),
            credentials: None,
            client_id: None,
        }
    }

    /// Token requests with HTTP Basic credentials, sent to trusted realms only.
    pub fn basic(credentials: BasicCredentials) -> Self {
        Self {
            credentials: Some(credentials),
            ..Self::anonymous()
        }
    }

    /// Trusts the origin (scheme, host, port) of `realm`: challenges naming a realm there are
    /// answered. The registry's own origin is always trusted.
    ///
    /// # Errors
    ///
    /// [`InputError::RealmUrl`] unless `realm` is an `http`/`https` URL with a host.
    pub fn trust_realm(mut self, realm: &str) -> Result<Self, InputError> {
        let url = Url::parse(realm).map_err(|_| InputError::RealmUrl)?;
        if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
            return Err(InputError::RealmUrl);
        }
        self.trusted.push(url);
        Ok(self)
    }

    /// The `client_id` sent with token requests (an audit label; the spec says it need not be
    /// registered).
    pub fn client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = Some(client_id.into());
        self
    }

    pub(crate) fn credentials(&self) -> Option<HeaderValue> {
        self.credentials.as_ref().map(|c| c.0.clone())
    }

    pub(crate) fn client_id_value(&self) -> Option<String> {
        self.client_id.clone()
    }

    /// The realm as a token endpoint, if it may be asked.
    pub(crate) fn check_realm(&self, realm: &str, registry: &Url) -> Result<Url, AuthProblem> {
        let url = Url::parse(realm).map_err(|_| AuthProblem::InvalidRealm)?;
        if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
            return Err(AuthProblem::InvalidRealm);
        }
        let origin = url.origin();
        let trusted =
            origin == registry.origin() || self.trusted.iter().any(|t| t.origin() == origin);
        if !trusted {
            return Err(AuthProblem::UntrustedRealm);
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AuthProblem::InvalidRealm);
        }
        Ok(url)
    }
}

impl fmt::Debug for TokenAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenAuth")
            .field(
                "trusted",
                &self
                    .trusted
                    .iter()
                    .map(|url| url.origin().ascii_serialization())
                    .collect::<Vec<_>>(),
            )
            .field("credentials", &self.credentials)
            .field("client_id", &self.client_id)
            .finish()
    }
}

/// HTTP Basic credentials for the token service (`Authorization: Basic base64(user:password)`,
/// RFC 7617). Never printed; the header value is flagged sensitive.
#[derive(Clone)]
pub struct BasicCredentials(HeaderValue);

impl BasicCredentials {
    /// Encodes `username` and `password`.
    ///
    /// # Errors
    ///
    /// [`InputError::Credentials`] when the user name contains `:` or either contains a control
    /// character.
    pub fn new(username: &str, password: &str) -> Result<Self, InputError> {
        if username.contains(':')
            || format!("{username}{password}")
                .chars()
                .any(char::is_control)
        {
            return Err(InputError::Credentials);
        }
        let encoded = base64(format!("{username}:{password}").as_bytes());
        let mut value = HeaderValue::try_from(format!("Basic {encoded}"))
            .map_err(|_| InputError::Credentials)?;
        value.set_sensitive(true);
        Ok(Self(value))
    }
}

impl fmt::Debug for BasicCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BasicCredentials(<redacted>)")
    }
}

/// RFC 4648 base64 with padding.
fn base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 0x3f) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A parsed `WWW-Authenticate: Bearer ...` challenge.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Challenge {
    /// The token endpoint, as sent.
    pub realm: String,
    /// The `service` parameter, as sent.
    pub service: Option<String>,
    /// The `scope` parameter's resource scopes (space-separated in the header), in order.
    pub scopes: Vec<String>,
    /// The `error` parameter (RFC 6750 §3), if any.
    pub error: Option<String>,
}

impl Challenge {
    /// The Bearer challenge among the `WWW-Authenticate` fields: `Ok(None)` when no field offers
    /// the Bearer scheme.
    pub(crate) fn bearer(headers: &HeaderMap) -> Result<Option<Self>, AuthProblem> {
        for value in headers.get_all(WWW_AUTHENTICATE) {
            let text = value
                .to_str()
                .map_err(|_| AuthProblem::MalformedChallenge)?;
            let challenges = parse_challenges(text).ok_or(AuthProblem::MalformedChallenge)?;
            for (scheme, params) in challenges {
                if !scheme.eq_ignore_ascii_case("bearer") {
                    continue;
                }
                return Self::from_params(params).map(Some);
            }
        }
        Ok(None)
    }

    fn from_params(params: Vec<(String, String)>) -> Result<Self, AuthProblem> {
        let mut challenge = Self {
            realm: String::new(),
            service: None,
            scopes: Vec::new(),
            error: None,
        };
        let mut seen: Vec<String> = Vec::new();
        for (name, value) in params {
            let name = name.to_ascii_lowercase();
            if seen.contains(&name) {
                return Err(AuthProblem::MalformedChallenge); // RFC 7235: each name at most once
            }
            seen.push(name.clone());
            match name.as_str() {
                "realm" => challenge.realm = value,
                "service" => challenge.service = Some(value),
                "scope" => {
                    challenge.scopes = value
                        .split(' ')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                "error" => challenge.error = Some(value),
                _ => {}
            }
        }
        if challenge.realm.is_empty() {
            return Err(AuthProblem::MalformedChallenge);
        }
        Ok(challenge)
    }
}

type Params = Vec<(String, String)>;

/// RFC 7235 `#challenge`: `scheme [ auth-param *( "," auth-param ) ]`, challenges separated by
/// commas; a parameter value is a token or a quoted string. `token68` is not used by Bearer
/// challenges and is refused.
fn parse_challenges(text: &str) -> Option<Vec<(String, Params)>> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut challenges: Vec<(String, Params)> = Vec::new();
    let skip_ows = |at: &mut usize| {
        while *at < bytes.len() && matches!(bytes[*at], b' ' | b'\t') {
            *at += 1;
        }
    };
    loop {
        // Empty list elements are allowed (RFC 9110 §5.6.1).
        while at < bytes.len() && matches!(bytes[at], b' ' | b'\t' | b',') {
            at += 1;
        }
        if at == bytes.len() {
            break;
        }
        let scheme = read_token(bytes, &mut at)?;
        let mut params: Params = Vec::new();
        loop {
            let save = at;
            skip_ows(&mut at);
            if at == bytes.len() {
                break;
            }
            let Some(name) = read_token(bytes, &mut at) else {
                at = save;
                break;
            };
            skip_ows(&mut at);
            if bytes.get(at) != Some(&b'=') {
                // A token not followed by `=` starts the next challenge.
                at = save;
                break;
            }
            at += 1;
            skip_ows(&mut at);
            let value = if bytes.get(at) == Some(&b'"') {
                read_quoted(bytes, &mut at)?
            } else {
                read_token(bytes, &mut at)?
            };
            params.push((name, value));
            skip_ows(&mut at);
            match bytes.get(at) {
                None => break,
                Some(b',') => {
                    at += 1;
                    // Skip further commas; the next token decides parameter or challenge.
                    while at < bytes.len() && matches!(bytes[at], b' ' | b'\t' | b',') {
                        at += 1;
                    }
                }
                Some(_) => return None,
            }
        }
        if params.is_empty() && at < bytes.len() && !matches!(bytes[at], b' ' | b'\t' | b',') {
            return None;
        }
        challenges.push((scheme, params));
    }
    Some(challenges)
}

fn read_token(bytes: &[u8], at: &mut usize) -> Option<String> {
    let start = *at;
    while *at < bytes.len()
        && (bytes[*at].is_ascii_alphanumeric()
            || matches!(
                bytes[*at],
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            ))
    {
        *at += 1;
    }
    (start < *at).then(|| String::from_utf8_lossy(&bytes[start..*at]).into_owned())
}

fn read_quoted(bytes: &[u8], at: &mut usize) -> Option<String> {
    *at += 1; // the opening quote
    let mut out = Vec::new();
    while *at < bytes.len() {
        match bytes[*at] {
            b'"' => {
                *at += 1;
                return String::from_utf8(out).ok();
            }
            b'\\' => {
                let escaped = *bytes.get(*at + 1)?;
                out.push(escaped);
                *at += 2;
            }
            byte => {
                out.push(byte);
                *at += 1;
            }
        }
    }
    None
}

/// The access an operation needs, which keys the token cache.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Access {
    /// `GET /v2/`: no resource.
    None,
    /// `GET /v2/_catalog`: `registry:catalog:*`.
    Catalog,
    /// Reads of one repository: `repository:<name>:pull`.
    Pull(RepositoryName),
    /// Writes of one repository: `repository:<name>:pull,push`.
    Push(RepositoryName),
}

impl Access {
    /// The resource scope a token request names when the challenge carries none.
    pub(crate) fn scope(&self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Catalog => Some("registry:catalog:*".to_owned()),
            Self::Pull(name) => Some(format!("repository:{name}:pull")),
            Self::Push(name) => Some(format!("repository:{name}:pull,push")),
        }
    }
}

/// A token and when it stops being sent pre-emptively.
#[derive(Clone)]
pub(crate) struct CachedToken {
    pub(crate) header: HeaderValue,
    pub(crate) expires: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(values: &[&str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for value in values {
            map.append(WWW_AUTHENTICATE, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn the_documented_challenge() {
        let map = headers(&[
            r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:samalba/my-app:pull,push""#,
        ]);
        let challenge = Challenge::bearer(&map).unwrap().unwrap();
        assert_eq!(challenge.realm, "https://auth.docker.io/token");
        assert_eq!(challenge.service.as_deref(), Some("registry.docker.io"));
        assert_eq!(challenge.scopes, ["repository:samalba/my-app:pull,push"]);
    }

    #[test]
    fn several_challenges_scopes_and_escapes() {
        let map = headers(&[
            r#"Basic realm="x", Bearer realm="https://a.test/t" , scope="repository:a:pull  registry:catalog:*", error="insufficient_scope", service=svc"#,
        ]);
        let challenge = Challenge::bearer(&map).unwrap().unwrap();
        assert_eq!(challenge.realm, "https://a.test/t");
        assert_eq!(
            challenge.scopes,
            ["repository:a:pull", "registry:catalog:*"]
        );
        assert_eq!(challenge.error.as_deref(), Some("insufficient_scope"));
        assert_eq!(challenge.service.as_deref(), Some("svc"));

        let map = headers(&[r#"bearer realm="https://a.test/\"q\"""#]);
        assert_eq!(
            Challenge::bearer(&map).unwrap().unwrap().realm,
            "https://a.test/\"q\""
        );
    }

    #[test]
    fn refusals() {
        assert_eq!(
            Challenge::bearer(&headers(&[r#"Basic realm="x""#])),
            Ok(None)
        );
        assert_eq!(Challenge::bearer(&HeaderMap::new()), Ok(None));
        for bad in [
            r#"Bearer realm="unterminated"#,
            r#"Bearer service="s""#,
            r#"Bearer realm="a",realm="b""#,
            r#"Bearer realm="a" junk"#,
        ] {
            assert_eq!(
                Challenge::bearer(&headers(&[bad])),
                Err(AuthProblem::MalformedChallenge),
                "{bad}"
            );
        }
    }

    #[test]
    fn base64_vectors() {
        // RFC 4648 §10.
        for (input, output) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), output);
        }
    }
}
