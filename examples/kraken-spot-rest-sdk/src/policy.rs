//! One policy shared by the generated public and private endpoint send paths.

use crate::{ApiFailure, Credentials, Envelope, Error, HttpFailure, LocalError, Nonces};
use concord::{Decoder, Endpoint, Json, ResponseMeta, SendWith, reqwest};
use std::fmt;

/// Maximum successful response body, in bytes. Oversized reads fail explicitly.
pub const RESPONSE_LIMIT: usize = 1024 * 1024;
/// Maximum non-2xx error document, in bytes.
pub const ERROR_LIMIT: usize = 16 * 1024;

/// A decoded provider result together with its HTTP metadata.
pub struct Response<T> {
    /// Provider data, with envelope errors already checked.
    pub value: T,
    /// Head and endpoint identity.
    pub meta: ResponseMeta,
}
impl<T> fmt::Debug for Response<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("meta", &self.meta)
            .field("value", &"<withheld>")
            .finish()
    }
}

/// Cheaply cloned signing state. Public requests send no credentials and use no nonce.
#[derive(Clone)]
pub struct Policy {
    credentials: Option<Credentials>,
    nonces: Nonces,
}
impl Policy {
    /// Public-only policy; private requests fail locally.
    pub fn public() -> Self {
        Self {
            credentials: None,
            nonces: Nonces::new(0),
        }
    }
    /// Shares this nonce source with every clone and all private endpoint sends.
    pub fn private(credentials: Credentials, nonces: Nonces) -> Self {
        Self {
            credentials: Some(credentials),
            nonces,
        }
    }

    /// Prepares and signs without I/O, consuming one private nonce allocation.
    /// Prefer generated `send()` normally. This explicit escape supports inspection
    /// or controlled scheduling; sending these out of order can cause InvalidNonce.
    /// Generated `prepare()` alone is unsigned and does not run this policy.
    pub fn prepare_signed<T, E: Endpoint<Envelope<T>>>(
        &self,
        endpoint: E,
    ) -> Result<Signed<T>, Error> {
        let (client, mut request, decoder) = endpoint.prepare()?.into_parts();
        let path = request.url().path();
        match (request.method().as_str(), path) {
            ("GET", "/0/public/AssetPairs" | "/0/public/Ticker") => {}
            ("POST", "/0/private/Balance" | "/0/private/OpenOrders") => {
                let credentials = self
                    .credentials
                    .as_ref()
                    .ok_or(LocalError::MissingCredentials)?;
                let encoded = request
                    .body()
                    .and_then(reqwest::Body::as_bytes)
                    .ok_or(LocalError::SigningInput)?;
                let nonce = self.nonces.next()?;
                // Form has encoded provider options exactly once. Add only the
                // policy-owned nonce; never decode/re-encode the signed bytes.
                let mut payload = format!("nonce={nonce}").into_bytes();
                if !encoded.is_empty() {
                    payload.push(b'&');
                    payload.extend_from_slice(encoded);
                }
                let signature = credentials.signature(path, nonce, &payload);
                request.headers_mut().insert("api-key", credentials.key());
                request.headers_mut().insert("api-sign", signature);
                *request.body_mut() = Some(reqwest::Body::from(payload));
            }
            _ => return Err(LocalError::SigningInput.into()),
        }
        Ok(Signed {
            client,
            request,
            decoder,
        })
    }
}
impl fmt::Debug for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Policy(<withheld>)")
    }
}

/// A decorated request with a redacted Debug implementation. No I/O at construction.
/// A signed request cannot be cloned; repeating a logical read needs a new allocation.
pub struct Signed<T> {
    client: reqwest::Client,
    request: reqwest::Request,
    decoder: Decoder<Envelope<T>>,
}
impl<T> fmt::Debug for Signed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signed")
            .field("operation", &self.decoder.operation())
            .finish()
    }
}
impl<T> Signed<T> {
    /// Explicit native request inspection. Body, URL and headers can contain secrets.
    pub fn request_unredacted(&self) -> &reqwest::Request {
        &self.request
    }

    /// Dispatches exactly once and applies the same bounded envelope/error policy
    /// as generated `send()`. HTTP status is authoritative on a non-2xx answer.
    pub async fn send(self) -> Result<Response<T>, Error> {
        let received = self.decoder.execute(&self.client, self.request).await?;
        if !received.status().is_success() {
            let (meta, document) = received
                .document::<Json, crate::ErrorDocument>(ERROR_LIMIT)
                .await;
            return Err(Error::Http(Box::new(HttpFailure { meta, document })));
        }
        let (envelope, meta) = received.decode_with_meta().await?;
        if !envelope.error.is_empty() {
            return Err(Error::Api(Box::new(ApiFailure {
                meta,
                errors: envelope.error,
            })));
        }
        let Some(value) = envelope.result else {
            return Err(Error::MissingResult(Box::new(meta)));
        };
        Ok(Response { value, meta })
    }
}

impl<T: Send> SendWith<Envelope<T>> for Policy {
    type Output = Result<Response<T>, Error>;
    async fn send<E: Endpoint<Envelope<T>> + Send>(&self, endpoint: E) -> Self::Output {
        self.prepare_signed(endpoint)?.send().await
    }
}
