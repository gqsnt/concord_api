//! The client, bucket handles, the four operation builders and the signing pipeline.
//!
//! Every operation goes through the same steps: the generated Concord request is prepared (no
//! I/O), taken apart (`Prepared::into_parts`), its target brought into S3's canonical form
//! ([`crate::sigv4::canonicalize_target`]), signed ([`crate::sigv4::sign_request`]), executed
//! once with the endpoint's own decoder (`Decoder::execute`), and the response mapped:
//! 2xx → the declared format; `304` → [`S3Error::NotModified`]; anything else → the bounded
//! `<Error>` document in [`S3Error::Api`].

use std::fmt;
use std::sync::Arc;

use concord::bytes::Bytes;
use concord::reqwest::header::{HeaderName, HeaderValue};
use concord::reqwest::{self, StatusCode, Url, redirect};
use concord::{Decoder, Prepared, Received};
use concord_pagination::{Page, Pages, Resume};

use crate::api::{
    S3Api, S3ApiBucket, S3ApiBucketGetObject, S3ApiBucketHeadObject, S3ApiBucketListObjectsV2,
    S3ApiBucketPutObject,
};
use crate::error::{ApiError, InputError, NotModified, ProtocolErrorKind, ResponseMeta, S3Error};
use crate::object::{GetObjectOutput, ObjectHead, PutObjectOutput};
use crate::sigv4::{Clock, Credentials, canonicalize_target, sign_request};
use crate::types::{
    AccountId, AnyObject, BucketName, ByteRange, ContentType, ContinuationToken, ETag, HttpDate,
    MaxKeys, Metadata, ObjectKey, PartNumber, Region, Requester, StorageClass, UrlEncoding,
    VersionId,
};
use crate::xml::{ListObjectsV2Output, ObjectSummary};

/// A `reqwest::ClientBuilder` with the two settings this SDK needs from the transport: no
/// redirect following (a `301`/`307` from S3 names the bucket's Region and must reach the caller
/// as an error, and a followed redirect would carry a signature for another host) and no
/// automatic decompression (object bytes must arrive as stored). Timeouts, proxies and TLS stay
/// yours to add.
pub fn http_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .redirect(redirect::Policy::none())
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
}

/// The signing context shared by a client's handles.
struct Context {
    region: Region,
    credentials: Credentials,
    clock: Clock,
}

/// An S3 client: endpoint, Region, credentials and clock, over your `reqwest::Client`.
///
/// Like Concord's handles it is an immutable snapshot, cheap to clone: `with_*` returns a new
/// client, and handles created earlier keep what they captured.
///
/// ```
/// use concord_sdk_amazon_s3::{BucketName, Credentials, ObjectKey, Region, S3Client, http_client_builder};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let http = http_client_builder().build()?;
/// let credentials = Credentials::new("SYNTHETIC-ACCESS-KEY-ID", "synthetic-secret")?;
/// let s3 = S3Client::new(http, Region::new("eu-west-3")?, credentials)?;
/// let bucket = s3.bucket(BucketName::new("my-bucket")?);
/// let request = bucket.get_object(ObjectKey::new("reports/2026 Q1.csv")?).prepare()?;
/// assert_eq!(
///     request.request().url().as_str(),
///     "https://my-bucket.s3.eu-west-3.amazonaws.com/reports/2026%20Q1.csv"
/// );
/// assert!(request.request().headers()["authorization"].is_sensitive());
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct S3Client {
    api: S3Api,
    endpoint: Url,
    context: Arc<Context>,
}

impl S3Client {
    /// A client for `region` at its default endpoint `https://s3.{region}.amazonaws.com`
    /// (the `aws` partition), signing with `credentials` at the system time.
    ///
    /// Build `http` with [`http_client_builder`] (or apply its two settings yourself).
    ///
    /// # Errors
    ///
    /// [`S3Error::Build`] if the endpoint URL is refused (not expected for a valid Region).
    pub fn new(
        http: reqwest::Client,
        region: Region,
        credentials: Credentials,
    ) -> Result<Self, S3Error> {
        let endpoint = format!("https://s3.{region}.amazonaws.com");
        let context = Arc::new(Context {
            region,
            credentials,
            clock: Clock::system(),
        });
        Self::at(S3Api::new(http), context, &endpoint)
    }

    /// Replaces the endpoint (another partition, a FIPS or dual-stack endpoint, an S3-compatible
    /// server, a test server). Buckets are addressed virtual-hosted style under it:
    /// `http://s3.test:9000` reaches bucket `b` at `http://b.s3.test:9000`.
    ///
    /// # Errors
    ///
    /// [`S3Error::Input`] unless the URL is `http`/`https` with a domain-name host (subdomains
    /// need one) and no path, query, fragment or user information; [`S3Error::Build`] if
    /// Concord refuses it.
    pub fn with_endpoint(self, url: &str) -> Result<Self, S3Error> {
        Self::at(self.api, self.context, url)
    }

    fn at(api: S3Api, context: Arc<Context>, url: &str) -> Result<Self, S3Error> {
        let parsed = Url::parse(url).map_err(|_| InputError::new("endpoint", "is not a URL"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(InputError::new("endpoint", "must be an http or https URL").into());
        }
        let host = parsed.host_str().unwrap_or("");
        if host.is_empty() || host.starts_with('[') || host.parse::<std::net::Ipv4Addr>().is_ok() {
            return Err(InputError::new(
                "endpoint",
                "must have a domain-name host (buckets are host labels)",
            )
            .into());
        }
        if parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(InputError::new(
                "endpoint",
                "must have no path, query, fragment or user information",
            )
            .into());
        }
        let api = api.with_base_url(parsed.as_str()).map_err(S3Error::Build)?;
        Ok(Self {
            api,
            endpoint: parsed,
            context,
        })
    }

    /// The same client signing with other credentials (rotated or refreshed by the application).
    pub fn with_credentials(self, credentials: Credentials) -> Self {
        let context = Arc::new(Context {
            region: self.context.region.clone(),
            credentials,
            clock: self.context.clock.clone(),
        });
        Self { context, ..self }
    }

    /// The same client signing with another clock (a fixed time in tests).
    pub fn with_clock(self, clock: Clock) -> Self {
        let context = Arc::new(Context {
            region: self.context.region.clone(),
            credentials: self.context.credentials.clone(),
            clock,
        });
        Self { context, ..self }
    }

    /// The Region of the credential scope.
    pub fn region(&self) -> &Region {
        &self.context.region
    }

    /// The endpoint.
    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    /// A bucket handle.
    pub fn bucket(&self, name: BucketName) -> Bucket {
        Bucket {
            scope: self.api.bucket(name.clone()),
            context: Arc::clone(&self.context),
            name,
        }
    }

    /// Signs a native request you built for an S3 operation this SDK does not declare, with this
    /// client's Region, credentials and clock (see [`crate::sigv4::sign_request`]; the target is
    /// signed as it is).
    ///
    /// # Errors
    ///
    /// [`S3Error::Signing`].
    pub fn sign(&self, request: &mut reqwest::Request) -> Result<(), S3Error> {
        let context = &self.context;
        sign_request(
            request,
            &context.credentials,
            &context.region,
            context.clock.now(),
        )
        .map_err(S3Error::Signing)
    }
}

impl fmt::Debug for S3Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Client")
            .field("endpoint", &self.endpoint.as_str())
            .field("region", &self.context.region)
            .field("credentials", &self.context.credentials)
            .field("clock", &self.context.clock)
            .finish()
    }
}

/// One bucket of an [`S3Client`].
#[derive(Clone)]
pub struct Bucket {
    scope: S3ApiBucket,
    context: Arc<Context>,
    name: BucketName,
}

impl Bucket {
    /// The bucket name.
    pub fn name(&self) -> &BucketName {
        &self.name
    }

    /// Sends `x-amz-expected-bucket-owner` on every request of this handle: S3 answers `403`
    /// when another account owns the bucket.
    pub fn expected_owner(self, account: AccountId) -> Self {
        Self {
            scope: self.scope.set_expected_owner(account),
            ..self
        }
    }

    /// Sends `x-amz-request-payer: requester` on every request of this handle (Requester Pays
    /// buckets).
    pub fn requester_pays(self) -> Self {
        Self {
            scope: self.scope.set_requester_pays(Requester),
            ..self
        }
    }

    fn signing(&self) -> Signing {
        Signing {
            context: Arc::clone(&self.context),
            credentials: None,
        }
    }

    /// `ListObjectsV2`: one page, or every page through [`ListObjectsV2::pages`].
    pub fn list_objects_v2(&self) -> ListObjectsV2 {
        ListObjectsV2 {
            request: self.scope.list_objects_v2(),
            signing: self.signing(),
            token: None,
        }
    }

    /// `HeadObject`: the object's metadata without its body.
    pub fn head_object(&self, key: ObjectKey) -> HeadObject {
        HeadObject {
            request: self.scope.head_object(key),
            signing: self.signing(),
        }
    }

    /// `GetObject`: the whole object or one byte range, conditionally.
    pub fn get_object(&self, key: ObjectKey) -> GetObject {
        GetObject {
            request: self.scope.get_object(key),
            signing: self.signing(),
            range: None,
        }
    }

    /// `PutObject` of a finite body with an explicit content type, in one request.
    pub fn put_object(
        &self,
        key: ObjectKey,
        body: impl Into<Bytes>,
        content_type: ContentType,
    ) -> PutObject {
        PutObject {
            request: self
                .scope
                .put_object(key, body.into(), content_type.as_str()),
            signing: self.signing(),
            metadata: Metadata::new(),
        }
    }
}

impl fmt::Debug for Bucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bucket")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The context and the per-request credentials override of one operation.
#[derive(Clone)]
struct Signing {
    context: Arc<Context>,
    credentials: Option<Credentials>,
}

impl Signing {
    fn sign<T>(
        &self,
        prepared: Result<Prepared<T>, concord::Error>,
        edit: impl FnOnce(&mut reqwest::Request) -> Result<(), S3Error>,
    ) -> Result<SignedRequest<T>, S3Error> {
        let (client, mut request, decoder) = prepared.map_err(S3Error::Build)?.into_parts();
        edit(&mut request)?;
        canonicalize_target(request.url_mut());
        let credentials = self
            .credentials
            .as_ref()
            .unwrap_or(&self.context.credentials);
        sign_request(
            &mut request,
            credentials,
            &self.context.region,
            self.context.clock.now(),
        )?;
        Ok(SignedRequest {
            client,
            request,
            decoder,
            range: None,
        })
    }
}

/// A signed request, not yet sent: the native request (for inspection) and the operation's
/// response handling. `send()` executes it exactly once.
///
/// The native request's own `Debug` prints the full URL; the `authorization` and
/// `x-amz-security-token` values are flagged sensitive (reqwest prints `Sensitive`). This type's
/// `Debug` prints the operation, the method and the header names only.
pub struct SignedRequest<T> {
    client: reqwest::Client,
    request: reqwest::Request,
    decoder: Decoder<T>,
    range: Option<ByteRange>,
}

impl<T> SignedRequest<T> {
    /// The signed native request.
    pub fn request(&self) -> &reqwest::Request {
        &self.request
    }

    /// The operation's name (`S3Api.bucket.get_object`).
    pub fn operation(&self) -> &'static str {
        operation_name(self.decoder.operation())
    }

    /// The native exit: client, signed request and the operation's decoder. Any change to a
    /// signed part of the request (target, a header present when signing, the body) invalidates
    /// the signature; S3 then answers `403 SignatureDoesNotMatch`.
    pub fn into_parts(self) -> (reqwest::Client, reqwest::Request, Decoder<T>) {
        (self.client, self.request, self.decoder)
    }

    async fn receive(self) -> Result<(Received<T>, Option<ByteRange>), S3Error> {
        let range = self.range;
        let received = self
            .decoder
            .execute(&self.client, self.request)
            .await
            .map_err(S3Error::Transport)?;
        Ok((received, range))
    }
}

impl<T> fmt::Debug for SignedRequest<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedRequest")
            .field("operation", &self.operation())
            .field("method", &self.request.method().as_str())
            .field(
                "headers",
                &self
                    .request
                    .headers()
                    .keys()
                    .map(HeaderName::as_str)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

fn operation_name(operation: Option<concord::Operation>) -> &'static str {
    // Every generated endpoint attaches its identity; the fallback is for completeness only.
    operation.map_or("S3", |op| op.name())
}

/// The shared response mapping: 2xx → the declared handling; `304` → [`S3Error::NotModified`];
/// any other status → [`S3Error::Api`] with the bounded `<Error>` document.
async fn finish<T>(received: Received<T>) -> Result<(T, ResponseMeta), S3Error> {
    let operation = operation_name(received.operation());
    let status = received.status();
    let meta = ResponseMeta::from_headers(status, received.head().headers());
    if status.is_success() {
        match received.decode().await {
            Ok(value) => Ok((value, meta)),
            Err(error) => Err(S3Error::Response {
                error,
                meta: Box::new(meta),
            }),
        }
    } else if status == StatusCode::NOT_MODIFIED {
        let not_modified = NotModified::from_headers(operation, meta, received.head().headers());
        Err(S3Error::NotModified(Box::new(not_modified)))
    } else {
        Err(S3Error::Api(Box::new(
            ApiError::read(operation, received, meta).await,
        )))
    }
}

impl SignedRequest<ListObjectsV2Output> {
    /// Sends the request once.
    ///
    /// # Errors
    ///
    /// [`S3Error`].
    pub async fn send(self) -> Result<ListObjectsV2Output, S3Error> {
        let (received, _) = self.receive().await?;
        finish(received).await.map(|(value, _)| value)
    }
}

impl SignedRequest<ObjectHead> {
    /// Sends the request once.
    ///
    /// # Errors
    ///
    /// [`S3Error`]; `304` is [`S3Error::NotModified`], `412` an [`S3Error::Api`]
    /// ([`S3Error::is_precondition_failed`]). S3 sends no error document for `HEAD`.
    pub async fn send(self) -> Result<ObjectHead, S3Error> {
        let (received, _) = self.receive().await?;
        finish(received).await.map(|(value, _)| value)
    }
}

impl SignedRequest<GetObjectOutput> {
    /// Sends the request once and returns the head with the **unread** body.
    ///
    /// When a range was requested, a `206` must answer it ([`ProtocolErrorKind::RangeMismatch`]
    /// otherwise); a `200` means S3 sent the whole object (`head.is_partial()` is `false`).
    ///
    /// # Errors
    ///
    /// [`S3Error`]; `304` is [`S3Error::NotModified`], `412` an [`S3Error::Api`].
    pub async fn send(self) -> Result<GetObjectOutput, S3Error> {
        let (received, range) = self.receive().await?;
        let operation = operation_name(received.operation());
        let (output, meta) = finish(received).await?;
        if let (Some(requested), Some(answered)) = (range, output.head.content_range)
            && !requested.is_answered_by(&answered)
        {
            return Err(S3Error::protocol(
                operation,
                ProtocolErrorKind::RangeMismatch,
                meta,
            ));
        }
        Ok(output)
    }
}

impl SignedRequest<PutObjectOutput> {
    /// Sends the request once.
    ///
    /// # Errors
    ///
    /// [`S3Error`]; a failed `If-Match` / `If-None-Match: *` is a `412` [`S3Error::Api`].
    pub async fn send(self) -> Result<PutObjectOutput, S3Error> {
        let (received, _) = self.receive().await?;
        finish(received).await.map(|(value, _)| value)
    }
}

/// `ListObjectsV2` (`GET /?list-type=2`).
#[derive(Clone)]
#[must_use = "an operation does nothing until it is sent"]
pub struct ListObjectsV2 {
    request: S3ApiBucketListObjectsV2,
    signing: Signing,
    token: Option<ContinuationToken>,
}

/// What [`ListObjectsV2::pages`] keeps with each page besides its objects.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ListPage {
    /// `<CommonPrefixes>` of this page.
    pub common_prefixes: Vec<String>,
    /// `<KeyCount>`.
    pub key_count: Option<u32>,
    /// Status and request ids of the page's response.
    pub response: ResponseMeta,
}

/// The pages of a listing: objects as items, the continuation token as the key.
pub type ObjectPages = Pages<ObjectSummary, ContinuationToken, ListPage, S3Error>;

impl ListObjectsV2 {
    /// `prefix`: only keys that begin with it.
    pub fn prefix(self, prefix: impl Into<String>) -> Self {
        Self {
            request: self.request.set_prefix(prefix.into()),
            ..self
        }
    }

    /// `delimiter`: roll keys up to the first delimiter after the prefix into common prefixes.
    pub fn delimiter(self, delimiter: impl Into<String>) -> Self {
        Self {
            request: self.request.set_delimiter(delimiter.into()),
            ..self
        }
    }

    /// `max-keys`: at most this many keys and common prefixes per page.
    pub fn max_keys(self, max: MaxKeys) -> Self {
        Self {
            request: self.request.set_max_keys(max),
            ..self
        }
    }

    /// `start-after`: list keys after this one.
    pub fn start_after(self, key: ObjectKey) -> Self {
        Self {
            request: self.request.set_start_after(key),
            ..self
        }
    }

    /// `continuation-token`: continue an earlier listing.
    pub fn continuation_token(self, token: ContinuationToken) -> Self {
        self.maybe_token(Some(token))
    }

    fn maybe_token(self, token: Option<ContinuationToken>) -> Self {
        Self {
            request: self.request.maybe_continuation_token(token.clone()),
            token,
            ..self
        }
    }

    /// `fetch-owner=true`: include each object's `<Owner>`.
    pub fn fetch_owner(self, fetch: bool) -> Self {
        Self {
            request: self.request.set_fetch_owner(fetch),
            ..self
        }
    }

    /// `encoding-type=url`: S3 percent-encodes key fields (for keys XML 1.0 cannot carry); the
    /// SDK decodes them, so the output holds the real keys either way.
    pub fn url_encoded_keys(self) -> Self {
        Self {
            request: self.request.set_encoding_type(UrlEncoding),
            ..self
        }
    }

    /// Signs this request with `credentials` instead of the client's.
    pub fn credentials(mut self, credentials: Credentials) -> Self {
        self.signing.credentials = Some(credentials);
        self
    }

    /// Builds and signs the request; nothing is sent.
    ///
    /// # Errors
    ///
    /// [`S3Error::Build`] or [`S3Error::Signing`].
    pub fn prepare(self) -> Result<SignedRequest<ListObjectsV2Output>, S3Error> {
        self.signing.sign(self.request.prepare(), |_| Ok(()))
    }

    /// Sends one page.
    ///
    /// # Errors
    ///
    /// [`S3Error`].
    pub async fn send(self) -> Result<ListObjectsV2Output, S3Error> {
        self.prepare()?.send().await
    }

    /// Every page of this listing, as a `concord_pagination::Pages` chain: each page is this
    /// request (same prefix, delimiter, page size and credentials) with the previous page's
    /// `NextContinuationToken`. It starts at the token set with
    /// [`continuation_token`](Self::continuation_token), if any.
    ///
    /// `Ok(None)` from the chain means S3's last page (`IsTruncated` false) was read. A truncated
    /// page without a next token, or one returning the token it was requested with, is an
    /// [`S3Error::Protocol`] (no guess, no loop). A cap (`max_pages`) reached while S3 has more
    /// is `PagesError::Bounded` with the token to resume from. Nothing is retried. The listing is
    /// not a snapshot: objects written during the traversal may or may not appear.
    pub fn pages(self) -> ObjectPages {
        let from = match &self.token {
            Some(token) => Resume::At(token.clone()),
            None => Resume::Start,
        };
        let template = self;
        Pages::new(from, move |token: Option<ContinuationToken>| {
            let prepared = template.clone().maybe_token(token.clone()).prepare();
            async move {
                let signed = prepared?;
                let operation = signed.operation();
                let page = signed.send().await?;
                let next = if page.is_truncated {
                    let Some(next) = page.next_continuation_token.clone() else {
                        return Err(S3Error::protocol(
                            operation,
                            ProtocolErrorKind::TruncatedWithoutToken,
                            page.meta,
                        ));
                    };
                    if token.as_ref() == Some(&next) {
                        return Err(S3Error::protocol(
                            operation,
                            ProtocolErrorKind::RepeatedToken,
                            page.meta,
                        ));
                    }
                    Some(next)
                } else {
                    None
                };
                Ok(Page {
                    items: page.contents,
                    next,
                    meta: ListPage {
                        common_prefixes: page.common_prefixes,
                        key_count: page.key_count,
                        response: page.meta,
                    },
                })
            }
        })
    }
}

impl fmt::Debug for ListObjectsV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListObjectsV2")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

/// Conditional-request setters shared by `HeadObject` and `GetObject`.
macro_rules! read_conditions {
    () => {
        /// `If-Match`: only if the current entity tag is this one (`412` otherwise).
        pub fn if_match(self, etag: ETag) -> Self {
            Self {
                request: self.request.set_if_match(etag),
                ..self
            }
        }

        /// `If-None-Match`: only if the current entity tag differs (`304` otherwise).
        pub fn if_none_match(self, etag: ETag) -> Self {
            Self {
                request: self.request.set_if_none_match(etag),
                ..self
            }
        }

        /// `If-Modified-Since`: only if modified after this date (`304` otherwise).
        pub fn if_modified_since(self, date: HttpDate) -> Self {
            Self {
                request: self.request.set_if_modified_since(date),
                ..self
            }
        }

        /// `If-Unmodified-Since`: only if not modified after this date (`412` otherwise).
        pub fn if_unmodified_since(self, date: HttpDate) -> Self {
            Self {
                request: self.request.set_if_unmodified_since(date),
                ..self
            }
        }

        /// `versionId`: a specific version of the object.
        pub fn version_id(self, version: VersionId) -> Self {
            Self {
                request: self.request.set_version_id(version),
                ..self
            }
        }

        /// `partNumber`: one part of an object uploaded in parts.
        pub fn part_number(self, part: PartNumber) -> Self {
            Self {
                request: self.request.set_part_number(part),
                ..self
            }
        }

        /// Signs this request with `credentials` instead of the client's.
        pub fn credentials(mut self, credentials: Credentials) -> Self {
            self.signing.credentials = Some(credentials);
            self
        }
    };
}

/// `HeadObject` (`HEAD /{key}`).
#[derive(Clone)]
#[must_use = "an operation does nothing until it is sent"]
pub struct HeadObject {
    request: S3ApiBucketHeadObject,
    signing: Signing,
}

impl HeadObject {
    read_conditions!();

    /// `Range`: S3 reports the length of this range (`416` when unsatisfiable).
    pub fn range(self, range: ByteRange) -> Self {
        Self {
            request: self.request.set_range(range),
            ..self
        }
    }

    /// Builds and signs the request; nothing is sent.
    ///
    /// # Errors
    ///
    /// [`S3Error::Build`] or [`S3Error::Signing`].
    pub fn prepare(self) -> Result<SignedRequest<ObjectHead>, S3Error> {
        self.signing.sign(self.request.prepare(), |_| Ok(()))
    }

    /// Sends the request once.
    ///
    /// # Errors
    ///
    /// [`S3Error`] (see [`SignedRequest::<ObjectHead>::send`]).
    pub async fn send(self) -> Result<ObjectHead, S3Error> {
        self.prepare()?.send().await
    }
}

impl fmt::Debug for HeadObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadObject")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

/// `GetObject` (`GET /{key}`).
#[derive(Clone)]
#[must_use = "an operation does nothing until it is sent"]
pub struct GetObject {
    request: S3ApiBucketGetObject,
    signing: Signing,
    range: Option<ByteRange>,
}

impl GetObject {
    read_conditions!();

    /// `Range`: one byte range (S3 does not serve several ranges in one `GET`).
    pub fn range(self, range: ByteRange) -> Self {
        Self {
            request: self.request.set_range(range),
            range: Some(range),
            ..self
        }
    }

    /// `response-cache-control`: overrides the response's `Cache-Control`.
    pub fn response_cache_control(self, value: impl Into<String>) -> Self {
        Self {
            request: self.request.set_response_cache_control(value.into()),
            ..self
        }
    }

    /// `response-content-disposition`: overrides the response's `Content-Disposition`.
    pub fn response_content_disposition(self, value: impl Into<String>) -> Self {
        Self {
            request: self.request.set_response_content_disposition(value.into()),
            ..self
        }
    }

    /// `response-content-encoding`: overrides the response's `Content-Encoding`.
    pub fn response_content_encoding(self, value: impl Into<String>) -> Self {
        Self {
            request: self.request.set_response_content_encoding(value.into()),
            ..self
        }
    }

    /// `response-content-language`: overrides the response's `Content-Language`.
    pub fn response_content_language(self, value: impl Into<String>) -> Self {
        Self {
            request: self.request.set_response_content_language(value.into()),
            ..self
        }
    }

    /// `response-content-type`: overrides the response's `Content-Type`.
    pub fn response_content_type(self, value: ContentType) -> Self {
        Self {
            request: self.request.set_response_content_type(value.as_str()),
            ..self
        }
    }

    /// `response-expires`: overrides the response's `Expires`.
    pub fn response_expires(self, date: HttpDate) -> Self {
        Self {
            request: self.request.set_response_expires(date),
            ..self
        }
    }

    /// Builds and signs the request; nothing is sent.
    ///
    /// # Errors
    ///
    /// [`S3Error::Build`] or [`S3Error::Signing`].
    pub fn prepare(self) -> Result<SignedRequest<GetObjectOutput>, S3Error> {
        let mut signed = self.signing.sign(self.request.prepare(), |_| Ok(()))?;
        signed.range = self.range;
        Ok(signed)
    }

    /// Sends the request once; the body is returned unread.
    ///
    /// # Errors
    ///
    /// [`S3Error`] (see [`SignedRequest::<GetObjectOutput>::send`]).
    pub async fn send(self) -> Result<GetObjectOutput, S3Error> {
        self.prepare()?.send().await
    }
}

impl fmt::Debug for GetObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GetObject")
            .field("request", &self.request)
            .field("range", &self.range)
            .finish_non_exhaustive()
    }
}

/// `PutObject` (`PUT /{key}`), the whole body in one request.
#[derive(Clone)]
#[must_use = "an operation does nothing until it is sent"]
pub struct PutObject {
    request: S3ApiBucketPutObject,
    signing: Signing,
    metadata: Metadata,
}

impl PutObject {
    /// User-defined metadata (`x-amz-meta-*`), replacing any set before.
    pub fn metadata(self, metadata: Metadata) -> Self {
        Self { metadata, ..self }
    }

    /// `Cache-Control` stored with the object.
    pub fn cache_control(self, value: impl Into<String>) -> Self {
        Self {
            request: self.request.set_cache_control(value.into()),
            ..self
        }
    }

    /// `Content-Disposition` stored with the object.
    pub fn content_disposition(self, value: impl Into<String>) -> Self {
        Self {
            request: self.request.set_content_disposition(value.into()),
            ..self
        }
    }

    /// `Content-Encoding` stored with the object (the body is sent as given).
    pub fn content_encoding(self, value: impl Into<String>) -> Self {
        Self {
            request: self.request.set_content_encoding(value.into()),
            ..self
        }
    }

    /// `Content-Language` stored with the object.
    pub fn content_language(self, value: impl Into<String>) -> Self {
        Self {
            request: self.request.set_content_language(value.into()),
            ..self
        }
    }

    /// `Expires` stored with the object.
    pub fn expires(self, date: HttpDate) -> Self {
        Self {
            request: self.request.set_expires(date),
            ..self
        }
    }

    /// `x-amz-storage-class`.
    pub fn storage_class(self, class: StorageClass) -> Self {
        Self {
            request: self.request.set_storage_class(class),
            ..self
        }
    }

    /// `If-Match`: write only if the current object has this entity tag (`412` otherwise,
    /// `409 ConditionalRequestConflict` on a concurrent write).
    pub fn if_match(self, etag: ETag) -> Self {
        Self {
            request: self.request.set_if_match(etag),
            ..self
        }
    }

    /// `If-None-Match: *`: write only if no object has this key (`412` otherwise).
    pub fn if_absent(self) -> Self {
        Self {
            request: self.request.set_if_none_match(AnyObject),
            ..self
        }
    }

    /// Signs this request with `credentials` instead of the client's.
    pub fn credentials(mut self, credentials: Credentials) -> Self {
        self.signing.credentials = Some(credentials);
        self
    }

    /// Builds and signs the request (the payload hash is the SHA-256 of the body); nothing is
    /// sent.
    ///
    /// # Errors
    ///
    /// [`S3Error::Build`] or [`S3Error::Signing`].
    pub fn prepare(self) -> Result<SignedRequest<PutObjectOutput>, S3Error> {
        let metadata = self.metadata;
        self.signing.sign(self.request.prepare(), move |request| {
            // User-defined metadata names are data, so they cannot be declared header keys: they
            // are added to the native request before signing.
            for (name, value) in metadata.iter() {
                let name = HeaderName::from_bytes(format!("x-amz-meta-{name}").as_bytes())
                    .map_err(|_| InputError::new("metadata name", "is not a header name"))?;
                let value = HeaderValue::from_str(value)
                    .map_err(|_| InputError::new("metadata value", "is not a header value"))?;
                request.headers_mut().append(name, value);
            }
            Ok(())
        })
    }

    /// Sends the request once.
    ///
    /// # Errors
    ///
    /// [`S3Error`] (see [`SignedRequest::<PutObjectOutput>::send`]).
    pub async fn send(self) -> Result<PutObjectOutput, S3Error> {
        self.prepare()?.send().await
    }
}

impl fmt::Debug for PutObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PutObject")
            .field("request", &self.request)
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}
