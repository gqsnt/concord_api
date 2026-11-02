//! A built native request plus the declared response handling.

use std::fmt;

use bytes::Bytes;

use crate::__private::{EndpointMeta, redacted_url};
use crate::error::Error;
use crate::repr::BodyError;

/// A request built by an endpoint, not yet sent. `T` is the endpoint's output type (`()` when it
/// declares none).
///
/// Building performs no I/O. `Debug` renders the method, URL and headers with every sensitive
/// placement replaced by `<redacted>`.
///
/// To change the request natively (extra headers, a per-request timeout, a streaming body), take
/// it apart with [`into_parts`](Self::into_parts). That is a one-way door: Concord's redaction
/// metadata does not follow the native request, whose own `Debug` prints everything.
#[must_use = "a prepared request does nothing until it is sent"]
pub struct Prepared<T> {
    client: reqwest::Client,
    request: reqwest::Request,
    decoder: Decoder<T>,
}

impl<T> Prepared<T> {
    pub(crate) fn new(
        client: reqwest::Client,
        request: reqwest::Request,
        decoder: Decoder<T>,
    ) -> Self {
        Self {
            client,
            request,
            decoder,
        }
    }

    /// The native request, for inspection. Its `Debug` output is **not** redacted.
    pub fn request(&self) -> &reqwest::Request {
        &self.request
    }

    /// Executes the request once with the application's `reqwest::Client`, requires a 2xx status
    /// and decodes the body with the declared representation.
    pub async fn send(self) -> Result<T, Error> {
        let (client, request, decoder) = self.into_parts();
        decoder.finish(client.execute(request).await).await
    }

    /// Executes the request once and returns the native response without checking its status or
    /// reading its body.
    pub async fn send_raw(self) -> Result<reqwest::Response, Error> {
        let meta = self.decoder.meta;
        self.client
            .execute(self.request)
            .await
            .map_err(|e| Error::from_transport(meta, e))
    }

    /// The application's client, the native request and the declared response handling.
    ///
    /// `reqwest::RequestBuilder::from_parts(client, request)` turns the pair back into a builder.
    /// After executing the request yourself, [`Decoder::finish`] applies the endpoint's response
    /// handling and Concord's error conversion.
    pub fn into_parts(self) -> (reqwest::Client, reqwest::Request, Decoder<T>) {
        (self.client, self.request, self.decoder)
    }
}

impl<T> fmt::Debug for Prepared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = self.decoder.meta;
        let headers = DebugHeaders(self.request.headers());
        f.debug_struct("Prepared")
            .field("endpoint", &meta.name)
            .field("method", &self.request.method().as_str())
            .field("url", &redacted_url(self.request.url(), meta))
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

/// An endpoint's declared response handling, applicable to any native response: requires a 2xx
/// status, then reads the body and decodes it with the declared representation (or, for an
/// endpoint without `->`, drains it without buffering).
pub struct Decoder<T> {
    pub(crate) meta: &'static EndpointMeta,
    decode: fn(Bytes) -> Result<T, BodyError>,
    drain: bool,
}

impl<T> Decoder<T> {
    pub(crate) fn new(
        meta: &'static EndpointMeta,
        decode: fn(Bytes) -> Result<T, BodyError>,
        drain: bool,
    ) -> Self {
        Self {
            meta,
            decode,
            drain,
        }
    }

    /// Decodes `response` as the endpoint would have. A non-2xx status is
    /// [`ErrorKind::Status`](crate::ErrorKind::Status) and keeps the response unread.
    pub async fn decode(self, response: reqwest::Response) -> Result<T, Error> {
        if !response.status().is_success() {
            return Err(Error::from_status(self.meta, response));
        }
        let body = if self.drain {
            let mut response = response;
            while response
                .chunk()
                .await
                .map_err(|e| Error::from_transport(self.meta, e))?
                .is_some()
            {}
            Bytes::new()
        } else {
            response
                .bytes()
                .await
                .map_err(|e| Error::from_transport(self.meta, e))?
        };
        (self.decode)(body).map_err(|e| Error::from_decode(self.meta, e))
    }

    /// Completes a request you executed yourself: a transport failure becomes a Concord
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) error (with the same redaction as
    /// [`Prepared::send`]), and a response is handled as by [`decode`](Self::decode).
    ///
    /// ```no_run
    /// # async fn run(prepared: concord::Prepared<String>) -> Result<(), concord::Error> {
    /// let (client, mut request, decoder) = prepared.into_parts();
    /// *request.timeout_mut() = Some(std::time::Duration::from_secs(2));
    /// let text = decoder.finish(client.execute(request).await).await?;
    /// # let _ = text; Ok(()) }
    /// ```
    pub async fn finish(
        self,
        result: Result<reqwest::Response, reqwest::Error>,
    ) -> Result<T, Error> {
        match result {
            Ok(response) => self.decode(response).await,
            Err(e) => Err(Error::from_transport(self.meta, e)),
        }
    }
}

impl<T> Clone for Decoder<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Decoder<T> {}

impl<T> fmt::Debug for Decoder<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoder")
            .field("endpoint", &self.meta.name)
            .finish()
    }
}
