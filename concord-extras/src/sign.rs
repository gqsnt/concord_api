//! Request signing: [`SigV4`], a middleware layer for AWS Signature Version 4 (feature `sign`).
//!
//! Put the signer last in the description's middleware: it sees the method, URL,
//! headers and body after `edit_request` and after the middleware before it. Every
//! attempt and every page is signed again. `request()` and `into_request()` return the
//! request as it is before middleware. A transport must keep the signed fields unchanged.
//!
//! ```no_run
//! use concord::{Base, Body};
//! use concord::sign::{CanonicalPath, Credentials, SigV4};
//! concord::api! {
//!     client Objects(origin: Base, signer: SigV4) {
//!         base origin;
//!         middleware signer.clone();
//!         PUT put(key: String, bytes: Body) { path "{*key}"; body bytes; }
//!     }
//! }
//! async fn upload(credentials: Credentials) -> Result<(), Box<dyn std::error::Error>> {
//!     let base: Base = "https://photos.s3.eu-west-1.amazonaws.com".parse()?;
//!     let signer = SigV4::new(&base, credentials, "eu-west-1", "s3", CanonicalPath::Preserve);
//!     Objects::new(base, signer).put("reports/november.txt", Body::new("hello", "text/plain")).await?;
//!     Ok(())
//! }
//! ```
//!
//! A stream body is refused without being polled, unless unsigned payloads are
//! explicitly allowed, for S3 over HTTPS. Allowing them does not make a stream
//! repeatable. The module implements no chunk signing, presigned URLs, lookup of
//! credentials, OAuth flow or HTTP message signatures. For a provider's own proof, use
//! `hmac` and `sha2`, with [`body_bytes`] and [`check_origin`] to check the request.

use concord_core::http::header::{HeaderName, HeaderValue, InvalidHeaderValue, ToStrError};
use concord_core::http::HeaderMap;
use concord_core::reqwest::{Request, Response, Url};
use concord_core::transport::{TransportError, TryInfo};
use concord_core::{Base, Middleware, Next, Secret};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error as StdError;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::SystemTime;

// PANIC-SAFETY: 64 KiB fits usize on every supported target.
const HEAD_LIMIT: usize = 64 * 1024;
// PANIC-SAFETY: 16 KiB fits usize on every supported target.
const FIELD_LIMIT: usize = 16 * 1024;

/// Returns the body bytes, or empty bytes when the request has no body.
///
/// # Errors
///
/// A stream body is refused with `SignError::StreamingBody`, without being polled, even
/// when the caller has set a `Content-Length` header on the request; that header does
/// not make its bytes available.
///
/// ```
/// let request = concord::reqwest::Request::new(concord::http::Method::GET,
///     "https://example.test".parse()?);
/// assert_eq!(concord::sign::body_bytes(&request)?, b"");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn body_bytes(request: &Request) -> Result<&[u8], SignError> {
    match request.body() {
        None => Ok(&[]),
        Some(body) => body.as_bytes().ok_or(SignError::StreamingBody),
    }
}

/// Checks the scheme, host and effective port of a request against a base, before a key is used.
///
/// A path prefix in the base does not affect the check.
///
/// # Errors
///
/// Returns `SignError::WrongOrigin` when the origins differ.
///
/// ```
/// let base = "https://example.test/v1".parse()?;
/// let request = concord::reqwest::Request::new(concord::http::Method::GET,
///     "https://example.test/v2".parse()?);
/// concord::sign::check_origin(&request, &base)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn check_origin(request: &Request, base: &Base) -> Result<(), SignError> {
    let base = Url::parse(&base.to_string()).map_err(|_| SignError::WrongOrigin)?;
    if same_origin(request.url(), &base) {
        Ok(())
    } else {
        Err(SignError::WrongOrigin)
    }
}

fn same_origin(url: &Url, base: &Url) -> bool {
    url.scheme() == base.scheme()
        && url.host() == base.host()
        && url.port_or_known_default() == base.port_or_known_default()
}

/// Why a request could not be signed; the underlying cause is kept but never printed.
///
/// ```no_run
/// fn inspect(error: &concord::Error) {
///     if let Some(signing) = concord::sign::SignError::find(error) {
///         let _cause = std::error::Error::source(signing);
///     }
/// }
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub enum SignError {
    /// A stream cannot be hashed without consuming it. Unsigned payload is only
    /// supported for S3, so other services return this even when it is enabled.
    StreamingBody,
    /// An unsigned stream requires HTTPS.
    InsecureUnsignedPayload,
    /// The request leaves the configured origin.
    WrongOrigin,
    /// A request head or credential field exceeds its documented bound, or a
    /// region or service exceeds 256 bytes.
    TooLarge,
    /// The URL, Host, path encoding, framing or Connection options cannot be signed.
    InvalidRequest,
    /// The access key identifier, region or service is empty or contains a byte
    /// other than an ASCII letter, digit, hyphen or underscore; or a secret key
    /// or supplied session token is empty.
    InvalidCredentials,
    /// Constructing or inserting a generated header failed.
    InvalidHeader(Arc<dyn StdError + Send + Sync>),
    /// A signed input header is not valid text.
    InvalidHeaderText(Arc<ToStrError>),
    /// The signing time is outside years 0001 through 9999 in UTC.
    InvalidTime,
    /// An application proof failed; its cause is retained.
    Failed(Arc<dyn StdError + Send + Sync>),
}

impl SignError {
    /// Finds a `SignError` in this error or its chain of sources, stopping at a cycle.
    ///
    /// ```no_run
    /// # fn inspect(error: &concord::Error) {
    /// let signing = concord::sign::SignError::find(error);
    /// # }
    /// ```
    pub fn find<'a>(error: &'a (dyn StdError + 'static)) -> Option<&'a Self> {
        let mut current = Some(error);
        let mut slow = Some(error);
        let mut advance_slow = false;
        while let Some(error) = current {
            if let Some(found) = error.downcast_ref::<Self>() {
                return Some(found);
            }
            current = error.source();
            if advance_slow {
                slow = slow.and_then(StdError::source);
            }
            advance_slow = !advance_slow;
            if let (Some(fast), Some(slow)) = (current, slow) {
                if std::ptr::eq(fast, slow) {
                    break;
                }
            }
        }
        None
    }

    /// Wraps an error of the application as `Failed`, kept without being formatted.
    ///
    /// ```
    /// let error = concord::sign::SignError::failed(std::io::Error::other("private cause"));
    /// assert_eq!(error.to_string(), "the request proof failed");
    /// ```
    pub fn failed(cause: impl StdError + Send + Sync + 'static) -> Self {
        Self::Failed(Arc::new(cause))
    }
}

impl From<InvalidHeaderValue> for SignError {
    fn from(cause: InvalidHeaderValue) -> Self {
        Self::InvalidHeader(Arc::new(cause))
    }
}
impl From<ToStrError> for SignError {
    fn from(cause: ToStrError) -> Self {
        Self::InvalidHeaderText(Arc::new(cause))
    }
}
impl fmt::Debug for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::StreamingBody => "StreamingBody",
            Self::InsecureUnsignedPayload => "InsecureUnsignedPayload",
            Self::WrongOrigin => "WrongOrigin",
            Self::TooLarge => "TooLarge",
            Self::InvalidRequest => "InvalidRequest",
            Self::InvalidCredentials => "InvalidCredentials",
            Self::InvalidHeader(_) => "InvalidHeader(..)",
            Self::InvalidHeaderText(_) => "InvalidHeaderText(..)",
            Self::InvalidTime => "InvalidTime",
            Self::Failed(_) => "Failed(..)",
        })
    }
}
impl fmt::Display for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::StreamingBody => "the request body is a stream and cannot be hashed",
            Self::InsecureUnsignedPayload => "an unsigned request payload requires HTTPS",
            Self::WrongOrigin => "the request leaves the signing origin",
            Self::TooLarge => "the signing input exceeds its size limit",
            Self::InvalidRequest => "the request cannot be canonicalized for signing",
            Self::InvalidCredentials => "the signing credentials or scope are invalid",
            Self::InvalidHeader(_) => "a signing header could not be installed",
            Self::InvalidHeaderText(_) => "a signed header is not valid text",
            Self::InvalidTime => "the signing time is outside the supported range",
            Self::Failed(_) => "the request proof failed",
        })
    }
}
impl StdError for SignError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::InvalidHeader(cause) | Self::Failed(cause) => Some(cause.as_ref()),
            Self::InvalidHeaderText(cause) => Some(cause.as_ref()),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct CredentialSet {
    access: String,
    secret: String,
    token: Option<String>,
}

/// A set of AWS credentials, shared by its clones and replaced atomically as a whole.
///
/// A copy of each input is taken: changing an input `Secret` later has no effect. Use
/// [`Credentials::replace`] to install a complete new set. This type never looks up or
/// renews credentials. `Debug` hides every field.
///
/// Clones share one set: [`replace`](Self::replace) and
/// [`session_token`](Self::session_token), called on any clone, change every clone.
/// Configure the credentials fully, then clone.
///
/// ```
/// let credentials = concord::sign::Credentials::new("access", "secret").session_token("token");
/// credentials.replace(concord::sign::Credentials::new("next-access", "next-secret"));
/// ```
#[derive(Clone)]
#[must_use = "give the credentials to a signer"]
pub struct Credentials {
    inner: Arc<Mutex<Arc<CredentialSet>>>,
}
impl Credentials {
    /// Credentials from an access key ID and a secret key, each copied now and at most 16 KiB.
    ///
    /// They are checked when a request is signed.
    ///
    /// ```
    /// let credentials = concord::sign::Credentials::new("access", "secret");
    /// ```
    pub fn new(access_key_id: impl Into<Secret>, secret_access_key: impl Into<Secret>) -> Self {
        let set = CredentialSet {
            access: access_key_id.into().expose().to_string(),
            secret: secret_access_key.into().expose().to_string(),
            token: None,
        };
        Self {
            inner: Arc::new(Mutex::new(Arc::new(set))),
        }
    }

    /// Adds a session token, copied now; its limit of 16 KiB is checked when it is used.
    ///
    /// Configure the credentials fully before cloning them.
    ///
    /// ```
    /// let credentials = concord::sign::Credentials::new("access", "secret").session_token("token");
    /// ```
    #[must_use = "give the configured credentials to a signer"]
    pub fn session_token(self, token: impl Into<Secret>) -> Self {
        let mut set = (*self.snapshot()).clone();
        set.token = Some(token.into().expose().to_string());
        self.install(Arc::new(set));
        self
    }

    /// Replaces the whole set for every clone; each signature uses exactly one set.
    ///
    /// ```
    /// let credentials = concord::sign::Credentials::new("access", "secret");
    /// let shared = credentials.clone();
    /// credentials.replace(concord::sign::Credentials::new("new-access", "new-secret"));
    /// ```
    pub fn replace(&self, next: Credentials) {
        self.install(next.snapshot());
    }

    fn lock(&self) -> MutexGuard<'_, Arc<CredentialSet>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    fn snapshot(&self) -> Arc<CredentialSet> {
        self.lock().clone()
    }
    fn install(&self, next: Arc<CredentialSet>) {
        let retired = std::mem::replace(&mut *self.lock(), next);
        drop(retired);
    }
}
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credentials(..)")
    }
}

/// How the service expects the path to be written in the canonical request; you choose it explicitly.
///
/// Neither convention rewrites the URL. S3 preserves existing escapes; other
/// services encode them again in the canonical request.
/// Neither can restore dot segments already removed by the URL parser.
/// ```
/// let path = concord::sign::CanonicalPath::Preserve;
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum CanonicalPath {
    /// S3: preserves slashes and escapes. Requires AWS encoding: unreserved
    /// characters, slash separators and uppercase escapes for other bytes.
    /// For example, `/a%20b` remains `/a%20b`. Refuses other paths.
    Preserve,
    /// Services other than S3: normalizes redundant slashes and literal dot
    /// segments, then encodes the path again: `/a%20b` becomes `/a%2520b`.
    /// A trailing slash is kept; an encoded dot segment such as `%2E%2E` remains
    /// a segment and becomes `%252E%252E`, rather than acting as `..`. The URL
    /// parser may already have removed dot segments before signing.
    NormalizeAndEncode,
}

struct Clock {
    wall: SystemTime,
    tick: tokio::time::Instant,
}

/// A middleware layer that signs requests with AWS Signature Version 4, for one origin,
/// service and region.
///
/// Put it last in the description. The origin is checked before credentials are
/// read. Host comes from the URL; a conflicting Host is refused. S3 receives
/// `x-amz-content-sha256`; other services use the digest in the canonical request.
/// Session tokens and Authorization are sensitive. Stale signing fields are
/// replaced, and a stale session token is removed when none is configured.
///
/// Signs every header on the request except Authorization, Connection, Expect,
/// Transfer-Encoding, User-Agent, X-Amzn-Trace-Id, Keep-Alive, Proxy-Connection,
/// TE, Trailer, Upgrade, Proxy-Authorization, Proxy-Authenticate and fields named
/// by Connection. A Connection option naming Authorization, Host or an
/// `x-amz-*` field is refused. Excluded headers are sent as the caller supplied them,
/// except Authorization, which the signer replaces.
/// Signed values are trimmed, spaces and tabs collapsed, duplicates joined with
/// commas. Conflicting Content-Length and Transfer-Encoding, duplicate or invalid
/// lengths, and lengths that disagree with a buffered body are refused.
///
/// Query names and values are decoded and AWS-encoded, then sorted, keeping
/// duplicate keys and bare flags as empty values. Raw `+` means space, `%2B`
/// means plus, empty fields between separators are skipped, lowercase escapes
/// become uppercase, and unescaped reserved characters are escaped.
/// A query or path with a malformed percent escape, or a query whose decoded
/// bytes are not UTF-8, is refused with [`SignError::InvalidRequest`].
///
/// Request heads are limited to 64 KiB before and after signing (method, full URL,
/// and every header name and value). Credential fields are at most 16 KiB, region
/// and service 256 bytes each; a longer region or service returns
/// [`SignError::TooLarge`]. The canonical text grows with the bounded request head; body
/// bytes are borrowed. The middleware hashes at most 64 KiB of payload per poll and
/// yields between chunks. It starts no task and never resends; only the engine retries
/// a request.
///
/// Time is anchored to the wall clock on the first valid use and advances with
/// Tokio's clock, including paused time in tests. Each attempt and each page reads
/// the time and is signed again.
///
/// Clones share the credentials and the time anchor; the origin, region, service, path
/// convention and unsigned-payload flag are copied, so `allow_unsigned_payload` changes
/// only the value it is called on, not its clones.
///
/// ```no_run
/// use concord::sign::{CanonicalPath, Credentials, SigV4};
/// let base = "https://iam.amazonaws.com".parse()?;
/// let signer = SigV4::new(&base, Credentials::new("access", "secret"),
///     "us-east-1", "iam", CanonicalPath::NormalizeAndEncode);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
#[must_use = "attach the signer as middleware or sign a request"]
pub struct SigV4 {
    origin: Option<Url>,
    credentials: Credentials,
    region: String,
    service: String,
    path: CanonicalPath,
    unsigned: bool,
    clock: Arc<OnceLock<Clock>>,
}
impl SigV4 {
    /// A signer for the origin of `base`, with its credentials, region, service and path convention.
    ///
    /// It cannot fail here; the settings are checked before sending.
    ///
    /// ```no_run
    /// # use concord::sign::{CanonicalPath, Credentials, SigV4};
    /// let base = "https://example.test".parse()?;
    /// let signer = SigV4::new(&base, Credentials::new("access", "secret"),
    ///     "us-east-1", "s3", CanonicalPath::Preserve);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(
        base: &Base,
        credentials: Credentials,
        region: impl Into<String>,
        service: impl Into<String>,
        path: CanonicalPath,
    ) -> Self {
        Self {
            origin: Url::parse(&base.to_string()).ok(),
            credentials,
            region: region.into(),
            service: service.into(),
            path,
            unsigned: false,
            clock: Arc::new(OnceLock::new()),
        }
    }

    /// Allows stream bodies to be sent as unsigned payloads, for S3 over HTTPS only.
    ///
    /// Other services still refuse a stream with `SignError::StreamingBody`; S3 requests
    /// over plain HTTP return `SignError::InsecureUnsignedPayload`. Buffered bodies are
    /// still hashed. `Body::stream` is sent once; a repeatable stream is signed again, as
    /// an unsigned payload, at each attempt.
    ///
    /// ```no_run
    /// # fn configure(signer: concord::sign::SigV4) {
    /// let signer = signer.allow_unsigned_payload();
    /// # }
    /// ```
    #[must_use = "attach the configured signer as middleware"]
    pub fn allow_unsigned_payload(mut self) -> Self {
        self.unsigned = true;
        self
    }

    /// Signs the request for the given region, without checking its origin.
    ///
    /// The adapter must check the origin first, with [`check_origin`] or its own list of
    /// allowed hosts; only the middleware checks the origin against this signer's base.
    /// This synchronous form hashes the whole buffered body before it returns; the
    /// middleware yields between chunks. The headers change only on success.
    ///
    /// ```no_run
    /// # fn sign(signer: &concord::sign::SigV4, request: &mut concord::reqwest::Request,
    /// #     bucket: &concord::Base) -> Result<(), concord::sign::SignError> {
    /// concord::sign::check_origin(request, bucket)?;
    /// signer.sign(request, "eu-west-1")?;
    /// # Ok(()) }
    /// ```
    pub fn sign(&self, request: &mut Request, region: &str) -> Result<(), SignError> {
        self.validate(request, region)?;
        let payload = match self.payload(request)? {
            Some(bytes) => hex::encode(Sha256::digest(bytes)),
            None => "UNSIGNED-PAYLOAD".into(),
        };
        self.finish(request, region, &payload)
    }

    fn payload<'a>(&self, request: &'a Request) -> Result<Option<&'a [u8]>, SignError> {
        match body_bytes(request) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(SignError::StreamingBody) if self.unsigned && self.service == "s3" => {
                if request.url().scheme() != "https" {
                    Err(SignError::InsecureUnsignedPayload)
                } else {
                    Ok(None)
                }
            }
            Err(error) => Err(error),
        }
    }

    fn validate(&self, request: &Request, region: &str) -> Result<(), SignError> {
        head_bound(request, request.headers())?;
        let url = request.url();
        if !matches!(url.scheme(), "https" | "http")
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(SignError::InvalidRequest);
        }
        scope(region)?;
        scope(&self.service)?;
        let mut lengths = request.headers().get_all("content-length").iter();
        if let Some(length) = lengths.next() {
            if lengths.next().is_some() || request.headers().contains_key("transfer-encoding") {
                return Err(SignError::InvalidRequest);
            }
            let text = length.to_str()?;
            if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(SignError::InvalidRequest);
            }
            let length = text
                .parse::<usize>()
                .map_err(|_| SignError::InvalidRequest)?;
            if body_bytes(request).is_ok_and(|bytes| bytes.len() != length) {
                return Err(SignError::InvalidRequest);
            }
        }
        canonical_path(url.path(), &self.path)?;
        canonical_query(url.query().unwrap_or(""))?;
        exclusions(request.headers())?;
        let host = authority(url)?;
        for value in request.headers().get_all("host") {
            if value.to_str()? != host {
                return Err(SignError::InvalidRequest);
            }
        }
        Ok(())
    }

    fn now(&self) -> Result<SystemTime, SignError> {
        let clock = self.clock.get_or_init(|| Clock {
            wall: SystemTime::now(),
            tick: tokio::time::Instant::now(),
        });
        clock
            .wall
            .checked_add(clock.tick.elapsed())
            .ok_or(SignError::InvalidTime)
    }

    fn finish(&self, request: &mut Request, region: &str, payload: &str) -> Result<(), SignError> {
        let credentials = self.credentials.snapshot();
        for field in [&credentials.access, &credentials.secret]
            .into_iter()
            .chain(credentials.token.iter())
        {
            if field.len() > FIELD_LIMIT {
                return Err(SignError::TooLarge);
            }
            if field.is_empty() {
                return Err(SignError::InvalidCredentials);
            }
        }
        if !scope_text(&credentials.access) {
            return Err(SignError::InvalidCredentials);
        }
        let nanos = match self.now()?.duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => {
                i128::try_from(duration.as_nanos()).map_err(|_| SignError::InvalidTime)?
            }
            Err(error) => i128::try_from(error.duration().as_nanos())
                .ok()
                .and_then(i128::checked_neg)
                .ok_or(SignError::InvalidTime)?,
        };
        let now = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map_err(|_| SignError::InvalidTime)?;
        if !(1..=9999).contains(&now.year()) {
            return Err(SignError::InvalidTime);
        }
        let date = format!(
            "{:04}{:02}{:02}",
            now.year(),
            u8::from(now.month()),
            now.day()
        );
        let timestamp = format!(
            "{date}T{:02}{:02}{:02}Z",
            now.hour(),
            now.minute(),
            now.second()
        );
        let mut headers = request.headers().clone();
        for name in [
            "authorization",
            "x-amz-date",
            "x-amz-content-sha256",
            "x-amz-security-token",
        ] {
            // PANIC-SAFETY: HeaderMap removal by a valid static name does not index a sequence.
            headers.remove(name);
        }
        put(&mut headers, "host", &authority(request.url())?, false)?;
        put(&mut headers, "x-amz-date", &timestamp, false)?;
        if self.service == "s3" || payload == "UNSIGNED-PAYLOAD" {
            put(&mut headers, "x-amz-content-sha256", payload, false)?;
        }
        if let Some(token) = &credentials.token {
            put(&mut headers, "x-amz-security-token", token, true)?;
        }
        let (canonical_headers, signed_headers) = canonical_headers(&headers)?;
        let canonical = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            request.method(),
            canonical_path(request.url().path(), &self.path)?,
            canonical_query(request.url().query().unwrap_or(""))?,
            canonical_headers,
            signed_headers,
            payload
        );
        let scope = format!("{date}/{region}/{}/aws4_request", self.service);
        let text = format!(
            "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical.as_bytes()))
        );
        let key = mac(
            format!("AWS4{}", credentials.secret).as_bytes(),
            date.as_bytes(),
        )?;
        let key = mac(&key, region.as_bytes())?;
        let key = mac(&key, self.service.as_bytes())?;
        let key = mac(&key, b"aws4_request")?;
        let signature = hex::encode(mac(&key, text.as_bytes())?);
        let authorization = format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}", credentials.access);
        put(&mut headers, "authorization", &authorization, true)?;
        head_bound(request, &headers)?;
        *request.headers_mut() = headers;
        Ok(())
    }

    #[cfg(test)]
    fn at(mut self, wall: SystemTime) -> Self {
        self.clock = Arc::new(OnceLock::from(Clock {
            wall,
            tick: tokio::time::Instant::now(),
        }));
        self
    }
}
impl fmt::Debug for SigV4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SigV4(..)")
    }
}
impl Middleware for SigV4 {
    async fn handle(
        &self,
        mut request: Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<Response, TransportError> {
        let result = async {
            if !self
                .origin
                .as_ref()
                .is_some_and(|base| same_origin(request.url(), base))
            {
                return Err(SignError::WrongOrigin);
            }
            self.validate(&request, &self.region)?;
            let payload = match self.payload(&request)? {
                None => "UNSIGNED-PAYLOAD".into(),
                Some(bytes) => {
                    let mut hash = Sha256::new();
                    hash_payload(bytes, |chunk| hash.update(chunk)).await;
                    hex::encode(hash.finalize())
                }
            };
            yield_once().await;
            self.finish(&mut request, &self.region, &payload)
        }
        .await;
        result.map_err(|error| TransportError::blocked().with_source(error))?;
        next.send(request, info).await
    }
}

async fn hash_payload(bytes: &[u8], mut update: impl FnMut(&[u8])) {
    // PANIC-SAFETY: HEAD_LIMIT is the nonzero constant 65536.
    for chunk in bytes.chunks(HEAD_LIMIT) {
        update(chunk);
        yield_once().await;
    }
}

async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if std::mem::replace(&mut yielded, true) {
            std::task::Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await
}

fn scope_text(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
fn scope(text: &str) -> Result<(), SignError> {
    if text.len() > 256 {
        Err(SignError::TooLarge)
    } else if !scope_text(text) {
        Err(SignError::InvalidCredentials)
    } else {
        Ok(())
    }
}
fn head_bound(request: &Request, headers: &HeaderMap) -> Result<(), SignError> {
    let size = request
        .method()
        .as_str()
        .len()
        .saturating_add(request.url().as_str().len());
    head_size(
        size,
        headers
            .iter()
            .map(|(name, value)| (name.as_str().len(), value.len())),
    )
}

fn head_size(
    mut size: usize,
    headers: impl IntoIterator<Item = (usize, usize)>,
) -> Result<(), SignError> {
    if size > HEAD_LIMIT {
        return Err(SignError::TooLarge);
    }
    for (name, value) in headers {
        size = size.saturating_add(name).saturating_add(value);
        if size > HEAD_LIMIT {
            return Err(SignError::TooLarge);
        }
    }
    Ok(())
}
fn authority(url: &Url) -> Result<String, SignError> {
    let mut host = url.host_str().ok_or(SignError::InvalidRequest)?.to_owned();
    if let Some(port) = url.port() {
        host.push_str(&format!(":{port}"));
    }
    Ok(host)
}
fn put(
    headers: &mut HeaderMap,
    name: &'static str,
    text: &str,
    sensitive: bool,
) -> Result<(), SignError> {
    let mut value = HeaderValue::from_str(text)?;
    value.set_sensitive(sensitive);
    headers
        .try_insert(name, value)
        .map_err(|cause| SignError::InvalidHeader(Arc::new(cause)))?;
    Ok(())
}
fn mac(key: &[u8], text: &[u8]) -> Result<Vec<u8>, SignError> {
    // HMAC accepts keys of every length; retain the error rather than asserting it.
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(SignError::failed)?;
    mac.update(text);
    Ok(mac.finalize().into_bytes().to_vec())
}
fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._~".contains(&byte)
}
fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &byte in bytes {
        if unreserved(byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
fn decode(text: &str) -> Result<Vec<u8>, SignError> {
    let mut bytes = text.bytes();
    let mut out = Vec::with_capacity(text.len());
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes
                .next()
                .and_then(|b| char::from(b).to_digit(16))
                .ok_or(SignError::InvalidRequest)?;
            let low = bytes
                .next()
                .and_then(|b| char::from(b).to_digit(16))
                .ok_or(SignError::InvalidRequest)?;
            // PANIC-SAFETY: both digits are at most 15; the result fits in u8.
            out.push((high * 16 + low) as u8);
        } else {
            out.push(byte);
        }
    }
    Ok(out)
}
fn canonical_path(path: &str, convention: &CanonicalPath) -> Result<String, SignError> {
    let mut segments = Vec::new();
    for piece in path.split('/') {
        match convention {
            CanonicalPath::Preserve => {
                let encoded = encode(&decode(piece)?);
                if encoded != piece {
                    return Err(SignError::InvalidRequest);
                }
                segments.push(encoded);
            }
            CanonicalPath::NormalizeAndEncode => {
                // Validate escapes without decoding: an encoded dot is not a dot segment.
                decode(piece)?;
                match piece {
                    "" | "." => {}
                    ".." => {
                        segments.pop();
                    }
                    _ => segments.push(encode(piece.as_bytes())),
                }
            }
        }
    }
    match convention {
        CanonicalPath::Preserve => Ok(segments.join("/")),
        CanonicalPath::NormalizeAndEncode => {
            let mut result = format!("/{}", segments.join("/"));
            // PANIC-SAFETY: This is an array literal, not indexing; its string iteration cannot panic.
            if ["/", "/.", "/.."]
                .iter()
                .any(|suffix| path.ends_with(suffix))
                && result != "/"
            {
                result.push('/');
            }
            Ok(result)
        }
    }
}

fn canonical_query(query: &str) -> Result<String, SignError> {
    if query.is_empty() {
        return Ok(String::new());
    }
    let mut pairs = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = decode(&name.replace('+', " "))?;
        let value = decode(&value.replace('+', " "))?;
        if std::str::from_utf8(&name).is_err() || std::str::from_utf8(&value).is_err() {
            return Err(SignError::InvalidRequest);
        }
        pairs.push((encode(&name), encode(&value)));
    }
    pairs.sort_unstable();
    Ok(pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&"))
}
fn exclusions(headers: &HeaderMap) -> Result<BTreeSet<String>, SignError> {
    let mut names: BTreeSet<String> = [
        "authorization",
        "connection",
        "expect",
        "transfer-encoding",
        "user-agent",
        "x-amzn-trace-id",
        "keep-alive",
        "proxy-connection",
        "te",
        "trailer",
        "upgrade",
        "proxy-authorization",
        "proxy-authenticate",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for value in headers.get_all("connection") {
        for name in value.to_str()?.split(',') {
            let name = name.trim().to_ascii_lowercase();
            if matches!(name.as_str(), "host" | "authorization") || name.starts_with("x-amz-") {
                return Err(SignError::InvalidRequest);
            }
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| SignError::InvalidRequest)?;
            // PANIC-SAFETY: BTreeSet insertion does not index a sequence.
            names.insert(name);
        }
    }
    Ok(names)
}
fn canonical_headers(headers: &HeaderMap) -> Result<(String, String), SignError> {
    let excluded = exclusions(headers)?;
    let mut fields: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (name, value) in headers {
        if !excluded.contains(name.as_str()) {
            let text = value
                .to_str()?
                .split([' ', '\t'])
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            fields.entry(name.as_str()).or_default().push(text);
        }
    }
    let signed = fields.keys().copied().collect::<Vec<_>>().join(";");
    let canonical = fields
        .into_iter()
        .map(|(name, values)| format!("{name}:{}\n", values.join(",")))
        .collect();
    Ok((canonical, signed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord_core::http::Method;
    use std::time::{Duration, UNIX_EPOCH};

    fn fixed(service: &str, credentials: Credentials, timestamp: i64) -> SigV4 {
        let base: Base = format!("https://{service}.amazonaws.com").parse().unwrap();
        SigV4::new(
            &base,
            credentials,
            "us-east-1",
            service,
            CanonicalPath::Preserve,
        )
        .at(UNIX_EPOCH + Duration::from_secs(timestamp.try_into().unwrap()))
    }

    #[test]
    fn c12_9_iam_list_users_signs_the_action_query_and_content_type() {
        // AWS General Reference, "Examples of the complete Signature Version 4
        // signing process", IAM ListUsers GET, 2015-08-30T12:36:00Z.
        // https://docs.aws.amazon.com/general/latest/gr/sigv4-signed-request-examples.html
        // This is a recalled published signature, not an output blessed by this signer.
        let signer = fixed(
            "iam",
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY"),
            1_440_938_160,
        );
        let mut request = Request::new(
            Method::GET,
            "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08"
                .parse()
                .unwrap(),
        );
        request.headers_mut().insert(
            "content-type",
            HeaderValue::from_static("application/x-www-form-urlencoded; charset=utf-8"),
        );
        signer.sign(&mut request, "us-east-1").unwrap();
        assert_eq!(request.headers()["x-amz-date"], "20150830T123600Z");
        assert_eq!(
            request.headers()["authorization"].to_str().unwrap(),
            concat!(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, ",
                "SignedHeaders=content-type;host;x-amz-date, ",
                "Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
            )
        );
        assert!(!request.headers().contains_key("x-amz-content-sha256"));
    }

    struct S3Example {
        method: Method,
        target: &'static str,
        body: &'static str,
        headers: &'static [(&'static str, &'static str)],
        canonical_path: &'static str,
        canonical_query: &'static str,
        payload_hash: &'static str,
        signed_headers: &'static str,
        canonical_hash: &'static str,
        signature: &'static str,
    }

    // AWS S3 developer guide, "Signature Calculations for the Authorization
    // Header: Transferring Payload in a Single Chunk (AWS Signature Version 4)",
    // "Examples: Signature Calculations" supplies the inputs, canonical-request
    // hashes and signatures, so a caller's S3 requests match AWS's examples.
    // https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sig-v4-header-based-auth.html
    fn assert_s3_signature(example: &S3Example, path: CanonicalPath) {
        let signer = fixed(
            "s3",
            Credentials::new(
                "AKIAIOSFODNN7EXAMPLE",
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            ),
            1_369_353_600,
        );
        let signer = SigV4 { path, ..signer };
        let mut request = Request::new(
            example.method.clone(),
            format!("https://examplebucket.s3.amazonaws.com{}", example.target)
                .parse()
                .unwrap(),
        );
        if !example.body.is_empty() {
            *request.body_mut() = Some(example.body.into());
        }
        for &(name, value) in example.headers {
            request.headers_mut().insert(
                HeaderName::from_static(name),
                HeaderValue::from_static(value),
            );
        }
        signer.sign(&mut request, "us-east-1").unwrap();
        assert_eq!(request.headers()["x-amz-date"], "20130524T000000Z");
        assert_eq!(
            request.headers()["x-amz-content-sha256"],
            example.payload_hash
        );
        let path = canonical_path(request.url().path(), &signer.path).unwrap();
        let query = canonical_query(request.url().query().unwrap_or("")).unwrap();
        let (headers, signed_headers) = canonical_headers(request.headers()).unwrap();
        assert_eq!(path, example.canonical_path);
        assert_eq!(query, example.canonical_query);
        // Pin the exact set as well as the signature: Range, Date and
        // x-amz-storage-class must not enter the transport-header exclusions.
        assert_eq!(signed_headers, example.signed_headers);
        let canonical = format!(
            "{}\n{path}\n{query}\n{headers}\n{signed_headers}\n{}",
            request.method(),
            example.payload_hash,
        );
        assert_eq!(
            hex::encode(Sha256::digest(canonical)),
            example.canonical_hash
        );
        assert_eq!(
            request.headers()["authorization"].to_str().unwrap(),
            format!(
                "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders={}, Signature={}",
                example.signed_headers, example.signature,
            ),
        );
    }

    #[test]
    fn c12_9_s3_get_object_signs_the_range_header() {
        assert_s3_signature(
            &S3Example {
                method: Method::GET,
                target: "/test.txt",
                body: "",
                headers: &[("range", "bytes=0-9")],
                canonical_path: "/test.txt",
                canonical_query: "",
                payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                signed_headers: "host;range;x-amz-content-sha256;x-amz-date",
                canonical_hash: "7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972",
                signature: "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
            },
            CanonicalPath::Preserve,
        );
    }

    #[test]
    fn c12_9_s3_put_object_signs_the_encoded_path_date_and_storage_class() {
        let mut example = S3Example {
            method: Method::PUT,
            // Preserve requires an already AWS-encoded URL path. AWS prints
            // /test$file.text and uses /test%24file.text in its canonical request.
            target: "/test%24file.text",
            body: "Welcome to Amazon S3.",
            headers: &[
                ("date", "Fri, 24 May 2013 00:00:00 GMT"),
                ("x-amz-storage-class", "REDUCED_REDUNDANCY"),
            ],
            canonical_path: "/test%24file.text",
            canonical_query: "",
            payload_hash: "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072",
            signed_headers: "date;host;x-amz-content-sha256;x-amz-date;x-amz-storage-class",
            canonical_hash: "9e0e90d9c76de8fa5b200d8c849cd5b8dc7a3be3951ddb7f6a76b4158342019d",
            signature: "98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd",
        };
        assert_s3_signature(&example, CanonicalPath::Preserve);
        // A literal dollar sign has no existing escape to encode again, so the
        // other convention also produces %24 and the same published signature.
        example.target = "/test$file.text";
        assert_s3_signature(&example, CanonicalPath::NormalizeAndEncode);
    }

    #[test]
    fn c12_9_s3_bucket_lifecycle_signs_an_empty_query_value() {
        assert_s3_signature(
            &S3Example {
                method: Method::GET,
                target: "/?lifecycle",
                body: "",
                headers: &[],
                canonical_path: "/",
                canonical_query: "lifecycle=",
                payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                signed_headers: "host;x-amz-content-sha256;x-amz-date",
                canonical_hash: "9766c798316ff2757b517bc739a67f6213b4ab36dd5da2f94eaebf79c77395ca",
                signature: "fea454ca298b7da1c68078a5d1bdbfbbe0d65c699e0f91ac7a200a0136783543",
            },
            CanonicalPath::Preserve,
        );
    }

    #[test]
    fn c12_9_s3_list_objects_signs_sorted_query_parameters() {
        assert_s3_signature(
            &S3Example {
                method: Method::GET,
                target: "/?max-keys=2&prefix=J",
                body: "",
                headers: &[],
                canonical_path: "/",
                canonical_query: "max-keys=2&prefix=J",
                payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                signed_headers: "host;x-amz-content-sha256;x-amz-date",
                canonical_hash: "df57d21db20da04d7fa30298dd4488ba3a2b47ca3a489c74750e0f1e7df1b9b7",
                signature: "34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7",
            },
            CanonicalPath::Preserve,
        );
    }

    #[test]
    fn c12_9_queries_keep_duplicates_empty_values_and_escaped_plus() {
        // Raw '+' means space; %2B remains plus.
        // Decoded query bytes must be valid UTF-8.
        assert_eq!(
            canonical_query("z=&a=+&a=%2B&a=%20&%61=%C3%A9&flag&eq=a=b").unwrap(),
            "a=%20&a=%20&a=%2B&a=%C3%A9&eq=a%3Db&flag=&z="
        );
        for query in ["q=%FF", "%FF=q", "q=%C3%28"] {
            assert!(matches!(
                canonical_query(query),
                Err(SignError::InvalidRequest)
            ));
        }
        assert!(matches!(
            canonical_query("a=%GG"),
            Err(SignError::InvalidRequest)
        ));
        assert!(matches!(
            canonical_query("a=%1"),
            Err(SignError::InvalidRequest)
        ));
    }

    #[test]
    fn c12_9_s3_preserves_escapes_and_other_services_encode_them_again() {
        for path in ["/a//b/%2F/%FF/", "/%20/%24"] {
            assert_eq!(
                canonical_path(path, &CanonicalPath::Preserve).unwrap(),
                path
            );
        }
        for path in ["/a+b", "/a%2fb", "/a%", "/%41"] {
            assert!(matches!(
                canonical_path(path, &CanonicalPath::Preserve),
                Err(SignError::InvalidRequest)
            ));
        }
        assert_eq!(
            canonical_path("/a//b/../%2f/+", &CanonicalPath::NormalizeAndEncode).unwrap(),
            "/a/%252f/%2B"
        );
    }

    #[test]
    fn c12_10_time_bounds_fail_without_panicking() {
        let signer = fixed("s3", Credentials::new("access", "secret"), 253_402_300_800);
        let mut request = Request::new(Method::GET, "https://s3.amazonaws.com".parse().unwrap());
        assert!(matches!(
            signer.sign(&mut request, "us-east-1"),
            Err(SignError::InvalidTime)
        ));
        assert!(request.headers().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn c12_10_wrong_origin_is_checked_before_clock_or_credentials() {
        let base: Base = "https://s3.amazonaws.com".parse().unwrap();
        let signer = SigV4::new(
            &base,
            Credentials::new("", ""),
            "bad/scope",
            "s3",
            CanonicalPath::Preserve,
        );
        let script = concord_testkit::Script::new();
        let request = Request::new(Method::GET, "https://foreign.test".parse().unwrap());
        let error = signer
            .handle(request, TryInfo::new("sign.get", 1), Next::to(&script))
            .await
            .unwrap_err();
        assert!(matches!(
            SignError::find(&error),
            Some(SignError::WrongOrigin)
        ));
        assert!(signer.clock.get().is_none());
        assert!(script.requests().is_empty());
    }
    #[test]
    fn c12_9_replacement_publishes_one_immutable_set_during_concurrent_reads() {
        let credentials = Credentials::new("old-access", "old-secret").session_token("old-token");
        let original = credentials.snapshot();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for i in 0..1000 {
                    let label = if i % 2 == 0 { "new" } else { "old" };
                    credentials.replace(
                        Credentials::new(format!("{label}-access"), format!("{label}-secret"))
                            .session_token(format!("{label}-token")),
                    );
                }
            });
            for _ in 0..1000 {
                let set = credentials.snapshot();
                let label = if set.access == "old-access" {
                    "old"
                } else {
                    "new"
                };
                assert!(set.access == format!("{label}-access"));
                assert!(set.secret == format!("{label}-secret"));
                assert!(set.token.as_deref() == Some(format!("{label}-token").as_str()));
            }
        });
        assert!(
            original.access == "old-access"
                && original.secret == "old-secret"
                && original.token.as_deref() == Some("old-token")
        );
        credentials.replace(credentials.clone()); // publishing the same handle does not deadlock
    }

    #[test]
    fn c12_10_input_and_generated_head_bounds_are_enforced() {
        let signer = fixed("s3", Credentials::new("access", "secret"), 1_369_353_600);
        let mut request = Request::new(Method::GET, "https://s3.amazonaws.com/".parse().unwrap());
        let overhead =
            request.method().as_str().len() + request.url().as_str().len() + "large".len();
        request.headers_mut().insert(
            "large",
            HeaderValue::from_str(&"x".repeat(HEAD_LIMIT - overhead)).unwrap(),
        );
        head_bound(&request, request.headers()).unwrap();
        // Input fits exactly, but generated headers push it over the same bound.
        assert!(matches!(
            signer.sign(&mut request, "us-east-1"),
            Err(SignError::TooLarge)
        ));
        assert!(!request.headers().contains_key("authorization"));
        request.headers_mut().insert(
            "large",
            HeaderValue::from_str(&"x".repeat(HEAD_LIMIT - overhead + 1)).unwrap(),
        );
        assert!(matches!(
            head_bound(&request, request.headers()),
            Err(SignError::TooLarge)
        ));
        request.headers_mut().clear();
        for (access, secret, token, expected) in [
            ("a".repeat(FIELD_LIMIT + 1), "s".into(), None, "large"),
            ("a".into(), "s".repeat(FIELD_LIMIT + 1), None, "large"),
            (
                "a".into(),
                "s".into(),
                Some("t".repeat(FIELD_LIMIT + 1)),
                "large",
            ),
            ("invalid/scope".into(), "s".into(), None, "invalid"),
            ("a".into(), String::new(), None, "invalid"),
            (
                "a".into(),
                "s".into(),
                Some("private\ntoken".into()),
                "header",
            ),
        ] {
            let credentials = Credentials::new(access, secret);
            let credentials = if let Some(token) = token {
                credentials.session_token(token)
            } else {
                credentials
            };
            let signer = fixed("s3", credentials, 1_369_353_600);
            let error = signer.sign(&mut request, "us-east-1").unwrap_err();
            match expected {
                "large" => assert!(matches!(error, SignError::TooLarge)),
                "header" => {
                    assert!(matches!(error, SignError::InvalidHeader(_)));
                    assert!(error.source().is_some());
                }
                _ => assert!(matches!(error, SignError::InvalidCredentials)),
            }
            assert!(request.headers().is_empty());
        }
        let maximum = fixed(
            "s3",
            Credentials::new("a".repeat(FIELD_LIMIT), "s".repeat(FIELD_LIMIT))
                .session_token("t".repeat(FIELD_LIMIT)),
            1_369_353_600,
        );
        maximum.sign(&mut request, "r").unwrap();
        assert!(matches!(
            maximum.sign(&mut request, &"r".repeat(257)),
            Err(SignError::TooLarge)
        ));
    }
    #[test]
    fn c12_10_head_size_check_stops_at_the_first_excess_byte() {
        let visited = std::cell::Cell::new(0);
        let entries =
            std::iter::repeat_n((1, 0), 200_000).inspect(|_| visited.set(visited.get() + 1));
        assert!(matches!(head_size(0, entries), Err(SignError::TooLarge)));
        assert_eq!(visited.get(), HEAD_LIMIT + 1);
    }

    #[test]
    fn c12_10_head_size_check_skips_headers_when_the_target_is_already_too_large() {
        let visited = std::cell::Cell::new(0);
        let entries = std::iter::repeat_n((1, 0), 100).inspect(|_| visited.set(visited.get() + 1));
        assert!(matches!(
            head_size(HEAD_LIMIT + 1, entries),
            Err(SignError::TooLarge)
        ));
        assert_eq!(visited.get(), 0);
    }

    #[test]
    fn c12_9_non_s3_paths_encode_existing_escapes_again() {
        // Derived by hand from AWS's published aws-sigv4 1.6.4 rule:
        // PercentEncodingMode::Double re-encodes '%' as '%25' after normalizing
        // the still-encoded path. This is a derived request, not an AWS worked example.
        // https://docs.rs/aws-sigv4/1.6.4/aws_sigv4/http_request/enum.PercentEncodingMode.html
        // Space, UTF-8 e-acute and an escaped slash therefore become the following
        // canonical URI. No published final signature is claimed for this request.
        let mut signer = fixed(
            "es",
            Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY"),
            1_440_938_160,
        );
        signer.path = CanonicalPath::NormalizeAndEncode;
        let mut request = Request::new(
            Method::GET,
            "https://search.es.amazonaws.com/index/_doc/a%20b/%C3%A9/%2F"
                .parse()
                .unwrap(),
        );
        signer.sign(&mut request, "us-east-1").unwrap();
        let (headers, names) = canonical_headers(request.headers()).unwrap();
        let path = canonical_path(request.url().path(), &signer.path).unwrap();
        let canonical = format!("GET\n{path}\n\n{headers}\n{names}\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(
            canonical,
            concat!(
                "GET\n/index/_doc/a%2520b/%25C3%25A9/%252F\n\n",
                "host:search.es.amazonaws.com\nx-amz-date:20150830T123600Z\n\n",
                "host;x-amz-date\n",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            )
        );
        // Calculated independently with Python's hashlib/hmac from the literal
        // canonical request above, timestamp and public example credentials.
        assert_eq!(
            hex::encode(Sha256::digest(canonical)),
            "043301802c4f734bb03debe0950afb3c4493d49c1be8b4e2a35afb83a1aba06c"
        );
        assert_eq!(
            request.headers()["authorization"].to_str().unwrap(),
            concat!(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/es/aws4_request, ",
                "SignedHeaders=host;x-amz-date, ",
                "Signature=2864de1f74ee3ba716a9e4519faa5c741d15f60817d15d5e7dc46b0f2f9f323a"
            )
        );
    }

    #[test]
    fn c12_9_normalized_paths_keep_trailing_slashes_and_encoded_dot_segments() {
        for (path, expected) in [
            ("/a//b/", "/a/b/"),
            ("/a/b/..", "/a/"),
            ("/a/b/.", "/a/b/"),
            ("/a/%2E%2E/b/", "/a/%252E%252E/b/"),
        ] {
            assert_eq!(
                canonical_path(path, &CanonicalPath::NormalizeAndEncode).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn c12_10_scope_limits_and_invalid_scope_syntax_have_distinct_errors() {
        for field in ["region", "service"] {
            for size in [256, 257] {
                let region = if field == "region" {
                    "r".repeat(size)
                } else {
                    "r".into()
                };
                let service = if field == "service" {
                    "s".repeat(size)
                } else {
                    "s".into()
                };
                let signer = fixed(
                    &service,
                    Credentials::new("access", "secret"),
                    1_369_353_600,
                );
                let mut request =
                    Request::new(Method::GET, "https://service.test/".parse().unwrap());
                let result = signer.sign(&mut request, &region);
                if size == 256 {
                    result.unwrap();
                } else {
                    assert!(matches!(result, Err(SignError::TooLarge)));
                }
            }
        }
        for invalid in [
            "",
            "us/east",
            "us east",
            "us:east",
            "région",
            "region+suffix",
        ] {
            assert!(matches!(scope(invalid), Err(SignError::InvalidCredentials)));
        }
    }

    #[test]
    fn c12_10_payload_hashing_processes_at_most_64_kib_per_poll() {
        use futures_util::FutureExt;
        let bytes = vec![0; HEAD_LIMIT * 4];
        let processed = std::cell::Cell::new(0);
        let mut hash = Sha256::new();
        let mut hashing = Box::pin(hash_payload(&bytes, |chunk| {
            processed.set(processed.get() + chunk.len());
            hash.update(chunk);
        }));
        for chunks in 1..=4 {
            assert!(hashing.as_mut().now_or_never().is_none());
            assert_eq!(processed.get(), chunks * HEAD_LIMIT);
        }
        assert!(hashing.as_mut().now_or_never().is_some());
        drop(hashing);
        assert_eq!(processed.get(), bytes.len());
        assert_eq!(hash.finalize(), Sha256::digest(&bytes));
    }
}
