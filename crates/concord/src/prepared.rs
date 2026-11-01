//! A built native request plus the declared response representation.

use std::fmt;
use std::marker::PhantomData;

use crate::__private::{EndpointMeta, redacted_url};
use crate::error::Error;
use crate::repr::Decode;

/// A request built by an endpoint, not yet sent.
///
/// Building performs no I/O. `Debug` renders the method, URL and headers with every sensitive
/// placement replaced by `<redacted>`.
///
/// To change the request natively (extra headers, a per-request timeout, a streaming body), take
/// it apart with [`into_parts`](Self::into_parts). That is a one-way door: Concord's redaction
/// metadata does not follow the native request, whose own `Debug` prints everything.
#[must_use = "a prepared request does nothing until it is sent"]
pub struct Prepared<R> {
    client: reqwest::Client,
    request: reqwest::Request,
    meta: &'static EndpointMeta,
    _repr: PhantomData<fn() -> R>,
}

impl<R: Decode> Prepared<R> {
    pub(crate) fn new(
        client: reqwest::Client,
        request: reqwest::Request,
        meta: &'static EndpointMeta,
    ) -> Self {
        Self {
            client,
            request,
            meta,
            _repr: PhantomData,
        }
    }

    /// The native request, for inspection. Its `Debug` output is **not** redacted.
    pub fn request(&self) -> &reqwest::Request {
        &self.request
    }

    /// Executes the request once with the application's `reqwest::Client`, requires a 2xx status
    /// and decodes the body with the declared representation.
    pub async fn send(self) -> Result<R::Output, Error> {
        let (client, request, decoder) = self.into_parts();
        let response = client
            .execute(request)
            .await
            .map_err(|e| Error::from_transport(decoder.meta, e))?;
        decoder.decode(response).await
    }

    /// Executes the request once and returns the native response without checking its status or
    /// reading its body.
    pub async fn send_raw(self) -> Result<reqwest::Response, Error> {
        let meta = self.meta;
        self.client
            .execute(self.request)
            .await
            .map_err(|e| Error::from_transport(meta, e))
    }

    /// The application's client, the native request and the declared decoder.
    ///
    /// `reqwest::RequestBuilder::from_parts(client, request)` turns the pair back into a builder.
    pub fn into_parts(self) -> (reqwest::Client, reqwest::Request, Decoder<R>) {
        (
            self.client,
            self.request,
            Decoder {
                meta: self.meta,
                _repr: PhantomData,
            },
        )
    }
}

impl<R> fmt::Debug for Prepared<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers = DebugHeaders(self.request.headers());
        f.debug_struct("Prepared")
            .field("endpoint", &self.meta.name)
            .field("method", &self.request.method().as_str())
            .field("url", &redacted_url(self.request.url(), self.meta))
            .field("headers", &headers)
            .field("body", &self.request.body().map(|_| "..."))
            .finish()
    }
}

struct DebugHeaders<'a>(&'a reqwest::header::HeaderMap);

impl fmt::Debug for DebugHeaders<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for (name, value) in self.0 {
            if value.is_sensitive() {
                list.entry(&format_args!("{name}: <redacted>"));
            } else {
                // Values were validated as visible ASCII when built.
                list.entry(&format_args!(
                    "{name}: {}",
                    value.to_str().unwrap_or("<non-ASCII>")
                ));
            }
        }
        list.finish()
    }
}

/// Applies an endpoint's declared response handling to a native response: requires a 2xx status,
/// reads the body and decodes it.
pub struct Decoder<R> {
    pub(crate) meta: &'static EndpointMeta,
    _repr: PhantomData<fn() -> R>,
}

impl<R: Decode> Decoder<R> {
    /// Decodes `response` as the endpoint would have.
    pub async fn decode(self, response: reqwest::Response) -> Result<R::Output, Error> {
        if !response.status().is_success() {
            return Err(Error::from_status(self.meta, response));
        }
        let body = response
            .bytes()
            .await
            .map_err(|e| Error::from_transport(self.meta, e))?;
        R::decode(body).map_err(|e| Error::from_decode(self.meta, e))
    }
}

impl<R> Clone for Decoder<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R> Copy for Decoder<R> {}

impl<R> fmt::Debug for Decoder<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoder")
            .field("endpoint", &self.meta.name)
            .finish()
    }
}
