//! The client, the signing send policy, the signed-request type and the two domain methods the
//! declaration cannot express (`ListObjectsV2::pages`, `PutObject::metadata`).
//!
//! Every operation goes through the same steps, in [`Signer`] (the client's send policy, run by
//! each generated `send()`): the generated request is prepared (no I/O), taken apart
//! (`Prepared::into_parts`), its target brought into S3's canonical form
//! ([`crate::sigv4::canonicalize_target`]), signed ([`crate::sigv4::sign_request`]), executed
//! once with the endpoint's own decoder (`Decoder::execute`), and the response mapped:
//! 2xx → the declared format; `304` → [`S3Error::NotModified`]; anything else → the bounded
//! `<Error>` document in [`S3Error::Api`].

use std::fmt;
use std::sync::Arc;

use concord::reqwest::header::{HeaderName, HeaderValue, RANGE};
use concord::reqwest::{self, StatusCode, Url, redirect};
use concord::{Decoder, Endpoint, Prepared, Received, SendWith};
use concord_pagination::{Page, Pages, Resume};

use crate::api::{S3Api, S3ApiBucket, S3ApiBucketListObjectsV2, S3ApiBucketPutObject, S3ApiConfig};
use crate::error::{ApiError, InputError, NotModified, ProtocolErrorKind, ResponseMeta, S3Error};
use crate::object::{GetObjectOutput, ObjectHead, PutObjectOutput};
use crate::sigv4::{Clock, Credentials, canonicalize_target, sign_request};
use crate::types::{BucketName, ByteRange, ContinuationToken, Metadata, Region};
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

/// Each send applies the headers documented by `crate::sigv4::sign_request`: `host`,
/// `authorization`, `x-amz-date`, `x-amz-content-sha256`, and optional `x-amz-security-token`.
/// Generated `prepare()` bypasses signing.
///
/// A client's SigV4 context — Region, credentials, clock — and the **send policy** of every
/// operation: each generated `send()` hands its request to it (`send signer;` in the
/// declaration), and it signs the built request, sends it once and maps the response to
/// [`S3Error`]. Created by [`S3Client`]; cheap to clone (an `Arc`); `Debug` never shows the
/// secret or the session token.
#[derive(Clone)]
pub struct Signer(Arc<Context>);

impl Signer {
    /// Takes the prepared request apart, applies `edit`, canonicalizes the target and signs it.
    fn sign<T>(
        &self,
        prepared: Result<Prepared<T>, concord::Error>,
        edit: impl FnOnce(&mut reqwest::Request) -> Result<(), S3Error>,
    ) -> Result<SignedRequest<T>, S3Error> {
        let (client, mut request, decoder) = prepared.map_err(S3Error::Build)?.into_parts();
        edit(&mut request)?;
        canonicalize_target(request.url_mut());
        // The requested range, read back from the built request: a `206` must answer it.
        let range = match request.headers().get(RANGE) {
            Some(value) => Some(
                value
                    .to_str()
                    .ok()
                    .and_then(ByteRange::from_header)
                    .ok_or(InputError::new("range", "is not one byte range"))?,
            ),
            None => None,
        };
        let context = &*self.0;
        sign_request(
            &mut request,
            &context.credentials,
            &context.region,
            context.clock.now(),
        )?;
        Ok(SignedRequest {
            client,
            request,
            decoder,
            range,
        })
    }
}

impl fmt::Debug for Signer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signer")
            .field("region", &self.0.region)
            .field("credentials", &self.0.credentials)
            .field("clock", &self.0.clock)
            .finish()
    }
}

/// The policy for each declared output: sign, send once, apply the output's checks
/// ([`SignedRequest::send`]).
macro_rules! signed_send {
    ($($output:ty),* $(,)?) => {$(
        impl SendWith<$output> for Signer {
            type Output = Result<$output, S3Error>;

            async fn send<E: Endpoint<$output> + Send>(&self, endpoint: E) -> Self::Output {
                self.sign(endpoint.prepare(), |_| Ok(()))?.send().await
            }
        }
    )*};
}

signed_send!(
    ListObjectsV2Output,
    ObjectHead,
    GetObjectOutput,
    PutObjectOutput
);

/// An S3 client: endpoint, Region, credentials and clock, over your `reqwest::Client`.
///
/// Like Concord's handles it is an immutable snapshot, cheap to clone: `with_*` returns a new
/// client, and handles created earlier keep what they captured. Operations are reached through
/// [`bucket`](Self::bucket) and sent with their own `send()`, signed with this client's context.
///
/// ```
/// use concord_sdk_amazon_s3::{BucketName, Credentials, ObjectKey, Region, S3Client, http_client_builder};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let http = http_client_builder().build()?;
/// let credentials = Credentials::new("SYNTHETIC-ACCESS-KEY-ID", "synthetic-secret")?;
/// let s3 = S3Client::new(http, Region::new("eu-west-3")?, credentials)?;
/// let bucket = s3.bucket(BucketName::new("my-bucket")?);
/// let request = s3.prepare(bucket.get_object(ObjectKey::new("reports/2026 Q1.csv")?))?;
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
    signer: Signer,
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
        let signer = Signer(Arc::new(Context {
            region,
            credentials,
            clock: Clock::system(),
        }));
        let api = S3Api::new(
            http,
            S3ApiConfig {
                signer: signer.clone(),
            },
        );
        Self::at(api, signer, &endpoint)
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
        Self::at(self.api, self.signer, url)
    }

    fn at(api: S3Api, signer: Signer, url: &str) -> Result<Self, S3Error> {
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
            signer,
        })
    }

    /// The same client signing with other credentials (rotated or refreshed by the application,
    /// or the credentials of one request: `s3.clone().with_credentials(c).bucket(..)`).
    pub fn with_credentials(self, credentials: Credentials) -> Self {
        let context = Context {
            region: self.signer.0.region.clone(),
            credentials,
            clock: self.signer.0.clock.clone(),
        };
        self.with_context(context)
    }

    /// The same client signing with another clock (a fixed time in tests).
    pub fn with_clock(self, clock: Clock) -> Self {
        let context = Context {
            region: self.signer.0.region.clone(),
            credentials: self.signer.0.credentials.clone(),
            clock,
        };
        self.with_context(context)
    }

    fn with_context(self, context: Context) -> Self {
        let signer = Signer(Arc::new(context));
        Self {
            api: self.api.set_signer(signer.clone()),
            signer,
            ..self
        }
    }

    /// The Region of the credential scope.
    pub fn region(&self) -> &Region {
        &self.signer.0.region
    }

    /// The endpoint.
    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    /// A bucket handle: [`Bucket`](crate::Bucket), the generated scope (`set_expected_owner`,
    /// `set_requester_pays`, the four operations).
    pub fn bucket(&self, name: BucketName) -> S3ApiBucket {
        self.api.bucket(name)
    }

    /// Builds and signs an operation's request with this client's context, without sending it
    /// (the generated `prepare()` is the unsigned request). Pass a request created from this
    /// client: its target was built for this client's endpoint.
    ///
    /// # Errors
    ///
    /// [`S3Error::Build`], [`S3Error::Input`] or [`S3Error::Signing`].
    pub fn prepare<T, E: Endpoint<T>>(&self, operation: E) -> Result<SignedRequest<T>, S3Error> {
        self.signer.sign(operation.prepare(), |_| Ok(()))
    }

    /// Signs a native request you built for an S3 operation this SDK does not declare, with this
    /// client's Region, credentials and clock (see [`crate::sigv4::sign_request`]; the target is
    /// signed as it is).
    ///
    /// # Errors
    ///
    /// [`S3Error::Signing`].
    pub fn sign(&self, request: &mut reqwest::Request) -> Result<(), S3Error> {
        let context = &*self.signer.0;
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
        let context = &*self.signer.0;
        f.debug_struct("S3Client")
            .field("endpoint", &self.endpoint.as_str())
            .field("region", &context.region)
            .field("credentials", &context.credentials)
            .field("clock", &context.clock)
            .finish()
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

/// What [`ListObjectsV2::pages`](crate::ListObjectsV2::pages) keeps with each page besides its
/// objects.
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

impl S3ApiBucketListObjectsV2 {
    /// Every page of this listing, as a `concord_pagination::Pages` chain: each page is this
    /// request (same prefix, delimiter, page size and client) with the previous page's
    /// `NextContinuationToken`. It starts at the token set with `set_continuation_token`, if
    /// any.
    ///
    /// `Ok(None)` from the chain means S3's last page (`IsTruncated` false) was read. A truncated
    /// page without a next token, or one returning the token it was requested with, is an
    /// [`S3Error::Protocol`] (no guess, no loop). A cap (`max_pages`) reached while S3 has more
    /// is `PagesError::Bounded` with the token to resume from. Nothing is retried. The listing is
    /// not a snapshot: objects written during the traversal may or may not appear.
    pub fn pages(self) -> ObjectPages {
        // The starting token, read back from this request's built target (a request that does
        // not build fails on its first page with the same error).
        let start = self.clone().prepare().ok().and_then(|prepared| {
            prepared
                .request()
                .url()
                .query_pairs()
                .find(|(name, _)| name == "continuation-token")
                .and_then(|(_, token)| ContinuationToken::new(token).ok())
        });
        let from = match start {
            Some(token) => Resume::At(token),
            None => Resume::Start,
        };
        let operation = <Self as Endpoint<ListObjectsV2Output>>::operation().name();
        let template = self;
        Pages::new(from, move |token: Option<ContinuationToken>| {
            let request = template.clone().maybe_continuation_token(token.clone());
            async move {
                let page = request.send().await?;
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

impl S3ApiBucketPutObject {
    /// User-defined metadata (`x-amz-meta-*`): this request with its metadata, sent with
    /// [`PutObjectWithMetadata::send`] and the client it was created from. Set every other value
    /// before this call.
    ///
    /// Metadata names are data, which a declaration cannot place, and the send policy sees only
    /// the built request; the headers are added to it before signing.
    pub fn metadata(self, metadata: Metadata) -> PutObjectWithMetadata {
        PutObjectWithMetadata {
            request: self,
            metadata,
        }
    }
}

/// Appends each supplied metadata value verbatim under `x-amz-meta-<name>` before signing;
/// the signing headers follow `crate::sigv4::sign_request`.
///
/// A [`PutObject`](crate::PutObject) with user-defined metadata (`x-amz-meta-*`), from
/// [`PutObject::metadata`](crate::PutObject::metadata). Sent with the client:
/// `put.metadata(m).send(&s3)`.
#[derive(Clone)]
#[must_use = "an operation does nothing until it is sent"]
#[derive(concord::Debug)]
pub struct PutObjectWithMetadata {
    request: S3ApiBucketPutObject,
    metadata: Metadata,
}

impl PutObjectWithMetadata {
    /// Builds and signs the request with `client`'s context (the one the request was created
    /// from); nothing is sent.
    ///
    /// # Errors
    ///
    /// [`S3Error::Build`], [`S3Error::Input`] (a metadata name or value that is not a header
    /// name or value) or [`S3Error::Signing`].
    pub fn prepare(self, client: &S3Client) -> Result<SignedRequest<PutObjectOutput>, S3Error> {
        let metadata = self.metadata;
        client.signer.sign(self.request.prepare(), move |request| {
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

    /// Sends the request once, signed with `client`'s context (the one the request was created
    /// from).
    ///
    /// # Errors
    ///
    /// [`S3Error`]; a failed `If-Match` / `If-None-Match: *` is a `412` [`S3Error::Api`].
    pub async fn send(self, client: &S3Client) -> Result<PutObjectOutput, S3Error> {
        self.prepare(client)?.send().await
    }
}
