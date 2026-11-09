//! AWS Signature Version 4 for S3, with the `Authorization` header.
//!
//! The signature itself is computed by [`aws-sigv4`](https://docs.rs/aws-sigv4) (AWS's own Rust
//! signer, the one the AWS SDK for Rust uses), configured as AWS documents for S3: the request
//! path is used as sent (encoded once, never normalized), the payload hash is the hex SHA-256 of
//! the body sent as `x-amz-content-sha256`, and every header present when signing is signed
//! (except `user-agent`, `authorization`, `x-amzn-trace-id` and `transfer-encoding`, which the
//! signer excludes). Region, service (`s3`), credentials and time are explicit inputs.
//!
//! [`sign_request`] signs a native request **as it is**. The SDK's operations first bring the
//! request target into the form S3 canonicalizes ([`canonicalize_target`]): `/` inside an object
//! key unencoded, every other byte outside the unreserved set percent-encoded once (`%20`, never
//! `+`), in the path and in the query.

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use aws_sigv4::http_request::{
    PayloadChecksumKind, PercentEncodingMode, SessionTokenMode, SignableBody, SignableRequest,
    SignatureLocation, SigningSettings, UriPathNormalizationMode, sign,
};
use concord::reqwest::header::{HOST, HeaderName, HeaderValue};
use concord::reqwest::{self, Url};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::error::{InputError, SigningError};
use crate::types::Region;

/// The SigV4 service name of Amazon S3.
pub const SERVICE: &str = "s3";

/// AWS credentials: an access key id, its secret and, for temporary credentials, a session
/// token (sent as `x-amz-security-token`, signed and flagged sensitive).
///
/// `Debug` shows the access key id (it is printed in every `Authorization` header) and never the
/// secret or the token. Obtaining and refreshing credentials is the application's: a new value
/// is passed to [`crate::S3Client::with_credentials`] (for one request too: a client snapshot).
#[derive(Clone)]
pub struct Credentials {
    access_key_id: Arc<str>,
    secret_access_key: Arc<str>,
    session_token: Option<Arc<str>>,
}

fn visible_ascii(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| (0x21..0x7f).contains(&b))
}

impl Credentials {
    /// Long-term credentials.
    ///
    /// # Errors
    ///
    /// [`InputError`] unless both parts are non-empty visible ASCII without spaces.
    pub fn new(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Result<Self, InputError> {
        let access_key_id = access_key_id.into();
        let secret_access_key = secret_access_key.into();
        if !visible_ascii(&access_key_id) {
            return Err(InputError::new(
                "access key id",
                "must be non-empty visible ASCII without spaces",
            ));
        }
        if !visible_ascii(&secret_access_key) {
            return Err(InputError::new(
                "secret access key",
                "must be non-empty visible ASCII without spaces",
            ));
        }
        Ok(Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
        })
    }

    /// Temporary credentials: adds the session token.
    ///
    /// # Errors
    ///
    /// [`InputError`] unless the token is non-empty visible ASCII without spaces.
    pub fn with_session_token(mut self, token: impl Into<String>) -> Result<Self, InputError> {
        let token = token.into();
        if !visible_ascii(&token) {
            return Err(InputError::new(
                "session token",
                "must be non-empty visible ASCII without spaces",
            ));
        }
        self.session_token = Some(token.into());
        Ok(self)
    }

    /// The access key id.
    pub fn access_key_id(&self) -> &str {
        &self.access_key_id
    }

    /// Whether a session token is attached.
    pub fn has_session_token(&self) -> bool {
        self.session_token.is_some()
    }

    fn to_aws(&self) -> aws_credential_types::Credentials {
        aws_credential_types::Credentials::new(
            &*self.access_key_id,
            &*self.secret_access_key,
            self.session_token.as_deref().map(str::to_owned),
            None,
            "concord-sdk-amazon-s3",
        )
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("access_key_id", &&*self.access_key_id)
            .field("secret_access_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Where the signing time comes from: the system clock, or a fixed instant (tests, replays).
#[derive(Clone)]
pub struct Clock(ClockSource);

#[derive(Clone)]
enum ClockSource {
    System,
    Fixed(SystemTime),
    Custom(Arc<dyn Fn() -> SystemTime + Send + Sync>),
}

impl Clock {
    /// `SystemTime::now()` at signing.
    pub fn system() -> Self {
        Self(ClockSource::System)
    }

    /// Always `time`.
    pub fn fixed(time: SystemTime) -> Self {
        Self(ClockSource::Fixed(time))
    }

    /// Your own time source.
    pub fn from_fn(now: impl Fn() -> SystemTime + Send + Sync + 'static) -> Self {
        Self(ClockSource::Custom(Arc::new(now)))
    }

    /// The current signing time.
    pub fn now(&self) -> SystemTime {
        match &self.0 {
            ClockSource::System => SystemTime::now(),
            ClockSource::Fixed(time) => *time,
            ClockSource::Custom(now) => now(),
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::system()
    }
}

impl fmt::Debug for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ClockSource::System => f.write_str("Clock::System"),
            ClockSource::Fixed(time) => f.debug_tuple("Clock::Fixed").field(time).finish(),
            ClockSource::Custom(_) => f.write_str("Clock::Custom"),
        }
    }
}

/// SigV4's `UriEncode`: everything but `A-Z a-z 0-9 - . _ ~`, uppercase hex.
const URI_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Brings a request target built by Concord into the form S3 signs and expects:
///
/// - **path**: Concord writes an object key as *one* path segment, so a `/` inside the key
///   arrives as `%2F`. S3 requires the key's `/` unencoded. Concord percent-encodes every byte
///   outside the unreserved set (a literal `%` becomes `%25`), so `%2F` in its output can only
///   come from a `/`: it is turned back into `/`, and nothing else changes.
/// - **query**: Concord writes `application/x-www-form-urlencoded` (a space is `+`); SigV4
///   requires `UriEncode` (`%20`, `*` as `%2A`). The pairs are decoded and re-encoded in order.
///
/// The endpoint must have no path of its own (checked by [`crate::S3Client::with_endpoint`]).
pub fn canonicalize_target(url: &mut Url) {
    if url.path().contains("%2F") {
        let path = url.path().replace("%2F", "/");
        url.set_path(&path);
    }
    if let Some(query) = url.query() {
        if query.is_empty() {
            url.set_query(None);
            return;
        }
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let encoded = pairs
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    utf8_percent_encode(k, URI_ENCODE),
                    utf8_percent_encode(v, URI_ENCODE)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        url.set_query(Some(&encoded));
    }
}

/// The `Host` value hyper would send for `url` (the port only when it is not the scheme's
/// default), set explicitly so the signed value is the sent value.
fn host_value(url: &Url) -> Result<HeaderValue, SigningError> {
    let host = url
        .host_str()
        .ok_or_else(|| SigningError::new("the request URL has no host"))?;
    let text = match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    };
    HeaderValue::from_str(&text)
        .map_err(|e| SigningError::new("the host is not a header value").with_source(e))
}

/// Sets `host` from the request URL (including a non-default port). The AWS SigV4
/// signer supplies `authorization` (AWS4-HMAC-SHA256 credentials, signed headers and signature),
/// `x-amz-date` from `time`, `x-amz-content-sha256` from the body's SHA-256, and
/// `x-amz-security-token` when the supplied credentials have a session token.
///
/// Signs `request` in place for `region` and the `s3` service at `time`: sets `host`,
/// `x-amz-date`, `x-amz-content-sha256`, `x-amz-security-token` (temporary credentials) and
/// `authorization` (the last two flagged sensitive). The target and headers are signed as they
/// are; see [`canonicalize_target`] for the form S3 expects.
///
/// The body must be finite and in memory (`reqwest::Body::from(bytes)`): its SHA-256 is the
/// signed payload hash.
///
/// # Errors
///
/// [`SigningError`] for a streaming body, a header value that is not visible ASCII (SigV4
/// signs header text; the error names the header, never the value) or a signer failure.
pub fn sign_request(
    request: &mut reqwest::Request,
    credentials: &Credentials,
    region: &Region,
    time: SystemTime,
) -> Result<(), SigningError> {
    let host = host_value(request.url())?;
    request.headers_mut().insert(HOST, host);

    let mut headers = Vec::with_capacity(request.headers().len());
    for (name, value) in request.headers() {
        let text = value.to_str().map_err(|_| {
            SigningError::header("a header value is not visible ASCII", name.as_str())
        })?;
        headers.push((name.as_str(), text));
    }
    let body = match request.body() {
        None => SignableBody::Bytes(&[]),
        Some(body) => match body.as_bytes() {
            Some(bytes) => SignableBody::Bytes(bytes),
            None => {
                return Err(SigningError::new(
                    "the body is a stream; S3 single-request signing needs the bytes",
                ));
            }
        },
    };

    let identity = credentials.to_aws().into();
    let mut settings = SigningSettings::default();
    settings.percent_encoding_mode = PercentEncodingMode::Single;
    settings.uri_path_normalization_mode = UriPathNormalizationMode::Disabled;
    settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;
    settings.signature_location = SignatureLocation::Headers;
    settings.session_token_mode = SessionTokenMode::Include;
    let params = aws_sigv4::sign::v4::SigningParams::builder()
        .identity(&identity)
        .region(region.as_str())
        .name(SERVICE)
        .time(time)
        .settings(settings)
        .build()
        .map_err(|e| SigningError::new("the signing parameters are incomplete").with_source(e))?
        .into();
    let signable = SignableRequest::new(
        request.method().as_str(),
        request.url().as_str(),
        headers.into_iter(),
        body,
    )
    .map_err(|e| SigningError::new("the request is not signable").with_source(e))?;
    let (instructions, _signature) = sign(signable, &params)
        .map_err(|e| SigningError::new("the signer failed").with_source(e))?
        .into_parts();
    let (signing_headers, _params) = instructions.into_parts();
    for header in signing_headers {
        let name = HeaderName::from_static(header.name());
        let mut value = HeaderValue::from_str(header.value()).map_err(|e| {
            SigningError::new("the signer produced an invalid header").with_source(e)
        })?;
        if header.sensitive() || name == reqwest::header::AUTHORIZATION {
            value.set_sensitive(true);
        }
        request.headers_mut().insert(name, value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_canonicalization() {
        let mut url: Url =
            "https://b.s3.test/photos%2F2006%2Fa%20b%25.txt?prefix=a+b%2Bc&list-type=2&t=x*y%7E"
                .parse()
                .unwrap();
        canonicalize_target(&mut url);
        assert_eq!(url.path(), "/photos/2006/a%20b%25.txt");
        assert_eq!(url.query(), Some("prefix=a%20b%2Bc&list-type=2&t=x%2Ay~"));
        let mut empty: Url = "https://b.s3.test/?".parse().unwrap();
        canonicalize_target(&mut empty);
        assert_eq!(empty.as_str(), "https://b.s3.test/");
    }

    #[test]
    fn credentials_debug_withholds_secrets() {
        let creds = Credentials::new("SYNTHETIC-ID", "synthetic-secret")
            .unwrap()
            .with_session_token("synthetic-token")
            .unwrap();
        let shown = format!("{creds:?}");
        assert!(shown.contains("SYNTHETIC-ID"));
        assert!(!shown.contains("synthetic-secret") && !shown.contains("synthetic-token"));
        assert!(Credentials::new("", "x").is_err());
        assert!(Credentials::new("a b", "x").is_err());
    }
}
