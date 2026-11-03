//! A built native request plus the declared response handling.

use std::fmt;

use bytes::Bytes;

use crate::__private::{EndpointMeta, redacted_url};
use crate::codec::{CodecError, Decode, ResponseHead};
use crate::error::Error;

/// A request built by an endpoint, not yet sent. `T` is the endpoint's output type (`()` when it
/// declares none).
///
/// Building performs no I/O. `Debug` renders the method, URL and headers with every sensitive
/// placement replaced by `<redacted>`.
///
/// To change the request natively (extra headers, a per-request timeout, a streaming body), take
/// it apart with [`into_parts`](Self::into_parts). That is a one-way door: Concord's redaction
/// metadata does not follow the native request, whose own `Debug` prints the full URL (sensitive
/// query values and path segments included) and masks only header values flagged sensitive.
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

    /// The native request, for inspection. Its `Debug` output is **not** Concord's: it prints the full
    /// URL, sensitive query values and path segments included; only header values flagged sensitive
    /// (by Concord or already on a native value) are masked (reqwest prints them as `Sensitive`).
    pub fn request(&self) -> &reqwest::Request {
        &self.request
    }

    /// Executes the request once with the application's `reqwest::Client`, requires a 2xx status,
    /// lets the declared format check the response head, then reads and decodes the body (an
    /// endpoint without `->` drains it and returns `()`). Never retries.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) if reqwest fails to send the request
    /// or read the response, [`Status`](crate::ErrorKind::Status) for a non-2xx response (kept
    /// unread), [`Metadata`](crate::ErrorKind::Metadata) if the format refuses the 2xx response's
    /// head (kept unread), [`Decode`](crate::ErrorKind::Decode) if the body does not decode.
    pub async fn send(self) -> Result<T, Error> {
        let (client, request, decoder) = self.into_parts();
        decoder.finish(client.execute(request).await).await
    }

    /// Executes the request once and returns the native response without checking its status or
    /// reading its body.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) only: any status is returned as a
    /// response.
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
    ///
    /// The decoder is fixed: changing the request (its `Accept`, say) does not change which
    /// format checks and decodes the response.
    pub fn into_parts(self) -> (reqwest::Client, reqwest::Request, Decoder<T>) {
        (self.client, self.request, self.decoder)
    }
}

impl<T> fmt::Debug for Prepared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = self.decoder.meta;
        let headers = DebugHeaders(self.request.headers());
        f.debug_struct("Prepared")
            .field("endpoint", &meta.map(|m| m.name))
            .field("method", &self.request.method().as_str())
            .field("url", &DebugUrl(self.request.url(), meta))
            .field("headers", &headers)
            .field("body", &self.request.body().map(|_| "..."))
            .finish()
    }
}

/// The URL with the endpoint's sensitive placements redacted. A [`Prepared`] is only built by
/// generated code, which always has the endpoint's metadata; without it the URL is withheld.
struct DebugUrl<'a>(&'a reqwest::Url, Option<&'static EndpointMeta>);

impl fmt::Debug for DebugUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            Some(meta) => fmt::Debug::fmt(&redacted_url(self.0, meta), f),
            None => f.write_str("<redacted>"),
        }
    }
}

struct DebugHeaders<'a>(&'a reqwest::header::HeaderMap);

impl fmt::Debug for DebugHeaders<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for (name, value) in self.0 {
            if value.is_sensitive() {
                list.entry(&format_args!("{name}: <redacted>"));
            } else if let Ok(text) = value.to_str() {
                list.entry(&format_args!("{name}: {text}"));
            } else {
                // A native value that is not text: its escaped bytes (`"caf\xe9"`).
                list.entry(&format_args!("{name}: {value:?}"));
            }
        }
        list.finish()
    }
}

/// The format's continuation once the head was accepted: decodes the body with the context taken
/// from the head. Boxing a zero-sized context allocates nothing.
type BodyStep<T> = Box<dyn FnOnce(Bytes) -> Result<T, CodecError> + Send>;

/// Checks a response head with a format and returns its body step.
pub(crate) type HeadStep<T> = fn(&ResponseHead<'_>) -> Result<BodyStep<T>, CodecError>;

/// A format's head checks and body decoding, monomorphized for `F` and erased behind a plain
/// function pointer, so a [`Decoder`] stays `Copy` and names only its output type.
pub(crate) fn head_step<F: Decode<T> + 'static, T: 'static>(
    head: &ResponseHead<'_>,
) -> Result<BodyStep<T>, CodecError> {
    F::check_content_type(head)?;
    let context = F::context(head)?;
    Ok(Box::new(move |body| F::decode_body(context, body)))
}

enum Kind<T> {
    /// No typed output: any 2xx, the body drained unbuffered.
    Drain(fn() -> T),
    Typed(HeadStep<T>),
}

impl<T> Clone for Kind<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Kind<T> {}

/// A response handling, applicable to any native response: requires a 2xx status, lets the
/// format check the head (its `Content-Type`, then anything else it checks) before the body is
/// read, then reads and decodes the body. For an endpoint without `->` it drains the body
/// without buffering it instead.
///
/// Every endpoint has one ([`Prepared::into_parts`]); [`Decoder::new`] builds one for a format
/// and output type of your choosing, without a declaration.
pub struct Decoder<T> {
    pub(crate) meta: Option<&'static EndpointMeta>,
    kind: Kind<T>,
}

impl<T: 'static> Decoder<T> {
    /// The response handling of the format `F` for the output `T`, outside any declaration
    /// (errors then name no endpoint): for a response you obtained yourself, with the same
    /// status, head and body rules as a generated endpoint.
    ///
    /// ```no_run
    /// # use concord_core::{bytes::Bytes, CodecError, Decode, Decoder, ResponseHead};
    /// # struct Lines;
    /// # impl Decode<usize> for Lines {
    /// #     type Context = ();
    /// #     fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> { Ok(()) }
    /// #     fn decode_body((): (), b: Bytes) -> Result<usize, CodecError> { Ok(b.len()) }
    /// # }
    /// # async fn run(client: concord_core::reqwest::Client) -> Result<(), concord_core::Error> {
    /// let response = client.get("https://example.test/report").send().await;
    /// let lines: usize = Decoder::<usize>::new::<Lines>().finish(response).await?;
    /// # let _ = lines; Ok(()) }
    /// ```
    pub fn new<F: Decode<T> + 'static>() -> Self {
        Self {
            meta: None,
            kind: Kind::Typed(head_step::<F, T>),
        }
    }
}

impl Decoder<()> {
    /// The response handling of an endpoint without typed output: any 2xx, the body drained
    /// without being buffered (no time or size bound beyond the client's own timeouts).
    pub fn drain() -> Self {
        Self {
            meta: None,
            kind: Kind::Drain(|| ()),
        }
    }
}

impl<T> Decoder<T> {
    pub(crate) fn typed(meta: &'static EndpointMeta, head: HeadStep<T>) -> Self {
        Self {
            meta: Some(meta),
            kind: Kind::Typed(head),
        }
    }

    pub(crate) fn drained(meta: &'static EndpointMeta, unit: fn() -> T) -> Self {
        Self {
            meta: Some(meta),
            kind: Kind::Drain(unit),
        }
    }

    /// Handles `response` as the endpoint would have: a non-2xx status is
    /// [`ErrorKind::Status`](crate::ErrorKind::Status) and keeps the response unread; the format
    /// then checks the head and, if it accepts it, the body is read and decoded.
    ///
    /// # Errors
    ///
    /// [`Status`](crate::ErrorKind::Status) for a non-2xx response,
    /// [`Metadata`](crate::ErrorKind::Metadata) if the format refuses the head (response kept
    /// unread), [`Transport`](crate::ErrorKind::Transport) if reading the body fails,
    /// [`Decode`](crate::ErrorKind::Decode) if it does not decode.
    pub async fn decode(self, response: reqwest::Response) -> Result<T, Error> {
        if !response.status().is_success() {
            return Err(Error::from_status(self.meta, response));
        }
        match self.kind {
            Kind::Drain(unit) => {
                let mut response = response;
                while response
                    .chunk()
                    .await
                    .map_err(|e| Error::from_transport(self.meta, e))?
                    .is_some()
                {}
                Ok(unit())
            }
            Kind::Typed(head) => {
                // The head is borrowed only for the format's checks; the response is then moved,
                // either into the error (unread) or into `bytes()`.
                let step = match head(&ResponseHead::of(&response)) {
                    Ok(step) => step,
                    Err(e) => return Err(Error::from_metadata(self.meta, e, response)),
                };
                let body = response
                    .bytes()
                    .await
                    .map_err(|e| Error::from_transport(self.meta, e))?;
                step(body).map_err(|e| Error::from_decode(self.meta, e))
            }
        }
    }

    /// Completes a request you executed yourself: a transport failure becomes a Concord
    /// [`ErrorKind::Transport`](crate::ErrorKind::Transport) error (with the same redaction as
    /// [`Prepared::send`]), and a response is handled as by [`decode`](Self::decode).
    ///
    /// # Errors
    ///
    /// [`Transport`](crate::ErrorKind::Transport) for `Err` or a failed body read,
    /// [`Status`](crate::ErrorKind::Status) for a non-2xx response,
    /// [`Metadata`](crate::ErrorKind::Metadata) if the format refuses the head,
    /// [`Decode`](crate::ErrorKind::Decode) if the body does not decode.
    ///
    /// ```no_run
    /// # async fn run(prepared: concord_core::Prepared<String>) -> Result<(), concord_core::Error> {
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
            .field("endpoint", &self.meta.map(|m| m.name))
            .finish()
    }
}
