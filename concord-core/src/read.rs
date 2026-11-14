//! How an answer becomes a value.

use crate::answer::{BodyClock, BodyTimeout, Headers, KnownKind, KnownOutcome};
use crate::error::{Class, Clock, Error, Kept, Kind, MismatchFailure};
use crate::transport::{TransportError, TryInfo};
use bytes::Bytes;
use futures_core::Stream;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

/// Turns the body of an answer into the value. One statement of the description names one: `read x;`.
///
/// The core has no format of its own. `Json`, `Text`, `Binary` and `Stream` of `concord` are
/// four readers written with this trait, and one of yours is used in the same way.
///
/// ```
/// # use concord_core as concord;
/// use concord::read::{Incoming, ReadError, Reader};
///
/// #[derive(Clone, Copy)]
/// struct Upper;
///
/// impl Reader<String> for Upper {
///     async fn read(&self, body: Incoming) -> Result<String, ReadError> {
///         let bytes = body.bytes(concord::MIB).await?;
///         match std::str::from_utf8(&bytes) {
///             Ok(text) => Ok(text.to_uppercase()),
///             Err(e) => Err(ReadError::not_the_type(bytes).with_details(e.to_string())),
///         }
///     }
/// }
/// # use futures_util::FutureExt;
/// # async {
/// let transport = concord::fake(|_| Ok(concord::reply(200, "hello")));
/// let request = concord::reqwest::Request::new(concord::http::Method::GET,
///     "https://example.test/".parse().unwrap());
/// let response = concord::Transport::send(&transport, request,
///     concord::transport::TryInfo::new("hello.get", 1)).await.unwrap();
/// assert_eq!(Upper.read(Incoming::new(response)).await.unwrap(), "HELLO");
/// # }.now_or_never().unwrap();
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot read a `{T}`",
    note = "write read Json; with use concord::Json;, or give {Self} a concord::Reader<{T}>"
)]
pub trait Reader<T>: Send + Sync {
    /// Consume the Incoming body once and return the declared value or a ReadError.
    /// Use [`Incoming::bytes`] with an explicit byte limit for buffered formats,
    /// or [`Incoming::stream`] to hand chunks on. Preserve stream failures with `?`;
    /// report decoding failures with [`ReadError::not_the_type`] and safe schema places.
    /// Body waits carry the call's clocks; only the whole-call clock bounds your
    /// reader's own work between body reads. Do not retain or print response values
    /// in an error source; optional decoder text belongs in [`ReadError::with_details`].
    fn read(&self, body: Incoming) -> impl Future<Output = Result<T, ReadError>> + Send;
}

impl<T, R: Reader<T>> Reader<T> for &R {
    fn read(&self, body: Incoming) -> impl Future<Output = Result<T, ReadError>> + Send {
        (**self).read(body)
    }
}

/// An answer whose body is not read yet.
pub struct Incoming {
    stream: ByteStream,
}

impl std::fmt::Debug for Incoming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Incoming").field(&self.stream).finish()
    }
}

impl From<reqwest::Response> for Incoming {
    fn from(response: reqwest::Response) -> Self {
        Self::new(response)
    }
}

impl Incoming {
    /// An answer to read, from a response, with no clocks and zero tries.
    ///
    /// ```
    /// # use concord_core as concord;
    /// # use concord::read::{Incoming, Reader};
    /// # use futures_util::FutureExt;
    /// # struct Text;
    /// # impl Reader<String> for Text {
    /// #     async fn read(&self, body: Incoming) -> Result<String, concord::read::ReadError> {
    /// #         Ok(String::from_utf8_lossy(&body.bytes(100).await?).into_owned())
    /// #     }
    /// # }
    /// # async {
    /// # let reader = Text;
    /// let text = reader.read(Incoming::new(concord::reply(200, "<a/>"))).await?;
    /// assert_eq!(text, "<a/>");
    /// # Ok::<(), concord::read::ReadError>(())
    /// # }.now_or_never().unwrap().unwrap();
    /// ```
    pub fn new(response: reqwest::Response) -> Incoming {
        let status = response.status();
        let headers = response.headers().clone().into();
        let announced = response
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|text| text.parse().ok())
            .or_else(|| response.content_length());
        Incoming {
            stream: ByteStream {
                body: Some(Box::pin(response.bytes_stream())),
                status,
                headers,
                announced,
                received: 0,
                info: TryInfo::new("", 0),
                clock: None,
                waiting: None,
                evidence: None,
            },
        }
    }

    /// Attach the call engine's clocks. Pass this Incoming to an answer helper.
    /// A directly constructed Incoming deliberately has no clocks.
    #[doc(hidden)]
    pub fn with_body_clock(mut self, clock: Arc<dyn BodyClock>) -> Self {
        self.stream.clock = Some(clock);
        self
    }

    pub(crate) fn with_info(mut self, info: &TryInfo) -> Self {
        self.stream.info = info.clone();
        if self.stream.clock.is_none() {
            self.stream.clock = info
                .run()
                .map(|run| Arc::new(run.clone()) as Arc<dyn BodyClock>);
        }
        self
    }

    pub(crate) fn clock(&self) -> Option<Arc<dyn BodyClock>> {
        self.stream.clock.clone()
    }

    pub(crate) fn keep_refusal(&mut self) -> Arc<Mutex<ReadEvidence>> {
        let evidence = Arc::new(Mutex::new(ReadEvidence::default()));
        self.stream.evidence = Some(evidence.clone());
        self.stream.record_refusal(&ReadEvidence::default());
        evidence
    }

    /// The status of the answer.
    pub fn status(&self) -> http::StatusCode {
        self.stream.status()
    }

    /// The headers of the answer.
    pub fn headers(&self) -> &Headers {
        self.stream.headers()
    }

    /// The `Content-Type` of the answer.
    pub fn content_type(&self) -> Option<&str> {
        self.headers().get("content-type")
    }

    /// The whole body, up to a limit. An announced excess fails before any read.
    /// A piece that would exceed the limit is dropped in its entirety.
    /// A single nonempty frame is returned without copying. Multiple frames are
    /// joined with fallible reservations. An announced size can guide capacity
    /// only up to twice the received bytes, with a 64 KiB floor, and within the
    /// limit. Growth is capped at the announcement while it covers the bytes received,
    /// so an honest body ends in a buffer of its own size. The stream must still
    /// confirm the announced size at its end.
    ///
    /// # Errors
    ///
    /// A body that is too large, or that breaks. Pass the [`ReadError`] on with `?`.
    pub async fn bytes(self, limit: usize) -> Result<Bytes, ReadError> {
        self.bytes_with_reservation(limit, reserve_join).await
    }

    // The same collector is exercised with an observing/failing reservation in
    // unit tests, without a global allocator or unsafe test infrastructure.
    async fn bytes_with_reservation(
        self,
        limit: usize,
        mut reserve: impl FnMut(&mut Vec<u8>, usize) -> Result<(), std::collections::TryReserveError>,
    ) -> Result<Bytes, ReadError> {
        let mut stream = self.stream;
        // PANIC-SAFETY: usize is at most 64 bits on reqwest-supported targets; the cast cannot truncate.
        if stream.announced.is_some_and(|length| length > limit as u64) {
            return Err(ReadError::limited(Bytes::new()));
        }
        let announced = stream
            .announced
            .and_then(|length| usize::try_from(length).ok())
            // Vec cannot represent a capacity above isize::MAX. An impossible
            // hint must not panic before the stream can report a short body.
            .filter(|length| *length <= isize::MAX.unsigned_abs());
        // Retain the first nonempty frame. Most responses already arrive in one
        // Bytes allocation; joining it eagerly would copy the whole body.
        let mut first = Bytes::new();
        let mut joined: Option<Vec<u8>> = None;
        loop {
            match std::future::poll_fn(|cx| stream.poll_piece(cx)).await {
                Some(Ok(piece)) => {
                    let length = joined.as_ref().map_or(first.len(), Vec::len);
                    if piece.len() > limit.saturating_sub(length) {
                        return Err(ReadError::limited(joined.map_or(first, Bytes::from)));
                    }
                    if piece.is_empty() {
                        continue;
                    }
                    if let Some(bytes) = &mut joined {
                        // PANIC-SAFETY: The preceding limit check bounds this sum by limit.
                        let needed = bytes.len() + piece.len();
                        if needed > bytes.capacity() {
                            let ceiling = announced
                                .filter(|size| *size >= needed)
                                .unwrap_or(limit)
                                .min(limit);
                            let capacity =
                                needed.max(bytes.capacity().saturating_mul(2).min(ceiling));
                            // PANIC-SAFETY: capacity >= needed > bytes.len() in this branch.
                            reserve(bytes, capacity)
                                .map_err(|error| allocation_error(&stream, error))?;
                        }
                        bytes.extend_from_slice(&piece);
                    } else if first.is_empty() {
                        first = piece;
                    } else {
                        // PANIC-SAFETY: The preceding limit check bounds this sum by limit.
                        let needed = first.len() + piece.len();
                        // use a bounded part of the hint only once a
                        // second frame actually needs a join buffer.
                        let mut bytes = Vec::new();
                        reserve(&mut bytes, join_capacity(needed, announced).min(limit))
                            .map_err(|error| allocation_error(&stream, error))?;
                        bytes.extend_from_slice(&first);
                        bytes.extend_from_slice(&piece);
                        first = Bytes::new();
                        joined = Some(bytes);
                    }
                }
                Some(Err(failure)) => return Err(ReadError::transport(stream, failure)),
                None => return Ok(joined.map_or(first, Bytes::from)),
            }
        }
    }

    /// The body as it comes, without reading a byte.
    pub fn stream(self) -> ByteStream {
        self.stream
    }
}

// Keep the reservation policy separate from allocation so a hostile hint can
// be checked without attempting its requested allocation in a test.
fn join_capacity(needed: usize, announced: Option<usize>) -> usize {
    // PANIC-SAFETY: The fixed 64 KiB floor fits usize on supported targets; growth saturates.
    let ceiling = needed.saturating_mul(2).max(64 * crate::KIB);
    announced.unwrap_or(needed).min(ceiling).max(needed)
}
fn reserve_join(
    bytes: &mut Vec<u8>,
    capacity: usize,
) -> Result<(), std::collections::TryReserveError> {
    bytes.try_reserve_exact(capacity.saturating_sub(bytes.len()))
}
fn allocation_error(stream: &ByteStream, error: std::collections::TryReserveError) -> ReadError {
    let mut failure = ReadError::not_the_type(Bytes::new());
    failure.kind = ReadKind::Transport(Box::new(ReadTransport {
        status: stream.status,
        headers: stream.headers.clone(),
        info: stream.info.clone(),
        failure: BodyFailure::Transport(TransportError::new(Class::Body).with_source(error)),
    }));
    failure.kept = None;
    failure
}

/// Bound decoder details to 1 KiB, including a UTF-8 safe ` … [cut]` marker.
/// This is the same policy used by [`ReadError::with_details`]. Applying it again
/// preserves the text and marker, and the returned allocation does not retain an
/// arbitrarily large source buffer. Details may contain response values; they
/// belong in optional diagnostics, never in an error's ordinary message or source.
///
/// ```
/// # use concord_core as concord;
/// let message = concord::read::details("é".repeat(1000));
/// assert!(message.len() <= 1024);
/// assert!(message.ends_with(" … [cut]"));
/// assert_eq!(concord::read::details(message.clone()), message);
/// ```
pub fn details(message: String) -> String {
    const MARK: &str = " … [cut]";
    let text = message.strip_suffix(MARK).unwrap_or(&message);
    let already_cut = text.len() != message.len();
    // PANIC-SAFETY: The fixed UTF-8 cut marker is shorter than 1024 bytes.
    let limit = 1024 - MARK.len();
    if message.len() <= 1024 {
        return if already_cut {
            format!("{text}{MARK}")
        } else {
            text.to_owned()
        };
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        // PANIC-SAFETY: The loop stops at byte zero, which is always a UTF-8 boundary.
        end -= 1;
    }
    // PANIC-SAFETY: end is at most 1024 minus the fixed marker length.
    let mut kept = String::with_capacity(end + MARK.len());
    kept.push_str(text.get(..end).unwrap_or_default());
    kept.push_str(MARK);
    kept
}

/// A schema location, without keys or variant names taken from the answer.
///
/// Field names must come from the program. At most 32 pieces are retained;
/// appending further pieces leaves the place unchanged. The root prints as empty
/// text. `Debug` prints exactly the same text as `Display`.
#[derive(Clone, Default)]
pub struct Place {
    pieces: Vec<PlacePiece>,
}

#[derive(Clone)]
enum PlacePiece {
    Field(&'static str),
    Index(usize),
    Key,
    Variant,
}

impl Place {
    /// The root of the value.
    pub fn root() -> Self {
        Self::default()
    }

    fn push(mut self, piece: PlacePiece) -> Self {
        if self.pieces.len() < 32 {
            self.pieces.push(piece);
        }
        self
    }

    /// A field whose name comes from the program's schema.
    pub fn field(self, name: &'static str) -> Self {
        self.push(PlacePiece::Field(name))
    }

    /// An element of a sequence.
    pub fn index(self, index: usize) -> Self {
        self.push(PlacePiece::Index(index))
    }

    /// A map key; its value is deliberately not accepted or retained.
    pub fn key(self) -> Self {
        self.push(PlacePiece::Key)
    }

    /// An enum variant; its value is deliberately not accepted or retained.
    pub fn variant(self) -> Self {
        self.push(PlacePiece::Variant)
    }
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (position, piece) in self.pieces.iter().enumerate() {
            match piece {
                PlacePiece::Field(name) => {
                    if position != 0 {
                        f.write_str(".")?;
                    }
                    f.write_str(name)?;
                }
                PlacePiece::Index(index) => write!(f, "[{index}]")?,
                PlacePiece::Key => f.write_str("[key]")?,
                PlacePiece::Variant => f.write_str("[variant]")?,
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

/// Why a reader could not give the value.
pub struct ReadError {
    body: bytes::Bytes,
    details: Option<String>,
    source: ReadSource,
    kind: ReadKind,
    kept: Option<Kept>,
}

struct ReadTransport {
    status: http::StatusCode,
    headers: Headers,
    info: TryInfo,
    failure: BodyFailure,
}

enum ReadKind {
    Decode,
    ContentType,
    Limit,
    Transport(Box<ReadTransport>),
    Forward(Error),
}

#[derive(Debug)]
struct ReadSource {
    place: Place,
    unexpected_label: bool,
}
impl std::fmt::Display for ReadSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the body is not the declared type")?;
        if !self.place.pieces.is_empty() {
            write!(f, " at {}", self.place)?;
        }
        if self.unexpected_label {
            f.write_str("; unexpected content type")?;
        }
        Ok(())
    }
}
impl std::error::Error for ReadSource {}
impl std::fmt::Debug for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadError")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the body could not be read as the declared type")
    }
}
impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ReadKind::Transport(read) => match &read.failure {
                BodyFailure::Transport(failure) => Some(failure),
                BodyFailure::Timeout(_) => None,
            },
            ReadKind::Forward(error) => Some(error),
            _ => Some(&self.source),
        }
    }
}

impl ReadError {
    /// The whole body arrived and is not the type. Concord constructs the cause
    /// from the place and failure kind; a reader cannot supply a decoder cause.
    ///
    /// ```compile_fail
    /// # use concord_core as concord;
    /// use concord::{bytes::Bytes, read::ReadError};
    /// let decoder_error = std::io::Error::other("private answer value");
    /// let _ = ReadError::not_the_type(Bytes::new(), decoder_error);
    /// ```
    pub fn not_the_type(body: bytes::Bytes) -> ReadError {
        ReadError {
            body: Bytes::copy_from_slice(
                // PANIC-SAFETY: 64 KiB fits usize on supported 32/64-bit targets; the subtraction is saturating.
                body.get(..body.len().min(64 * crate::KIB))
                    .unwrap_or_default(),
            ),
            // PANIC-SAFETY: 64 KiB fits usize on supported 32/64-bit targets; the subtraction is saturating.
            kept: Some(if body.len() > 64 * crate::KIB {
                Kept::Cut
            } else {
                Kept::Complete
            }),
            kind: ReadKind::Decode,
            details: None,
            source: ReadSource {
                place: Place::root(),
                unexpected_label: false,
            },
        }
    }

    /// A decoder message for [`Self::details`], kept up to 1 KiB with a cut marker.
    /// It may contain response values; it never becomes the error's source.
    pub fn with_details(mut self, details: impl Into<String>) -> ReadError {
        self.details = Some(self::details(details.into()));
        self
    }

    /// The structured location at which decoding failed, where the decoder gives it.
    /// Free text belongs in [`Self::with_details`].
    ///
    /// ```compile_fail
    /// # use concord_core as concord;
    /// use concord::{bytes::Bytes, read::ReadError};
    /// let text = String::from("items[PRIVATE_SENTINEL].kind");
    /// let _ = ReadError::not_the_type(Bytes::new()).at(text);
    /// ```
    pub fn at(mut self, place: Place) -> ReadError {
        self.source.place = place;
        self
    }

    /// The decoder's free text, available only by asking for it explicitly.
    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }

    /// Decoding failed, and the response's content type was not this reader's either.
    pub fn label_was_not_expected(mut self) -> ReadError {
        self.source.unexpected_label = true;
        self
    }

    /// The `Content-Type` of the answer is not the one this reader holds the server to.
    pub fn wrong_content_type() -> ReadError {
        let mut error = Self::not_the_type(Bytes::new());
        error.kind = ReadKind::ContentType;
        error.kept = None;
        error
    }

    fn limited(body: Bytes) -> Self {
        let kept = (!body.is_empty()).then_some(Kept::Cut);
        let mut error = Self::not_the_type(body);
        error.kind = ReadKind::Limit;
        error.kept = kept;
        error
    }

    fn transport(stream: ByteStream, failure: BodyFailure) -> Self {
        let mut error = Self::not_the_type(Bytes::new());
        error.kind = ReadKind::Transport(Box::new(ReadTransport {
            status: stream.status,
            headers: stream.headers.clone(),
            info: stream.info.clone(),
            failure,
        }));
        error.kept = None;
        error
    }

    pub(crate) fn into_error<P>(
        self,
        status: http::StatusCode,
        headers: Headers,
        info: &TryInfo,
    ) -> Error<P> {
        let failure = match self.kind {
            ReadKind::Transport(read) => {
                return read
                    .failure
                    .into_error(read.status, read.headers, &read.info)
            }
            ReadKind::Forward(error) => {
                let status = error.status().unwrap_or(status);
                let headers = error.headers().cloned().unwrap_or(headers);
                return error.without_problem().with_response(status, headers);
            }
            ReadKind::ContentType => MismatchFailure::ContentType,
            ReadKind::Limit => MismatchFailure::BodyLimit,
            ReadKind::Decode => {
                let unexpected_label = self.source.unexpected_label
                    && headers.as_map().contains_key(http::header::CONTENT_TYPE);
                if self.body.is_empty() && self.kept == Some(Kept::Complete) {
                    MismatchFailure::Empty { unexpected_label }
                } else {
                    MismatchFailure::Decode {
                        place: self.source.place,
                        unexpected_label,
                    }
                }
            }
        };
        let mut error = Error::mismatch(info.endpoint(), info.number(), status, headers, failure);
        if let Some(kept) = self.kept {
            error = error.with_body(&self.body, kept);
        }
        if let Some(details) = self.details {
            error = error.with_reader_details(details);
        }
        error
    }

    pub(crate) fn into_refused<P>(
        self,
        status: http::StatusCode,
        headers: Headers,
        info: &TryInfo,
        evidence: &ReadEvidence,
    ) -> Error<P> {
        let mut error = Error::refused(info.endpoint(), info.number(), status, headers, None);
        if let Some(kept) = evidence.kept {
            error = error.with_body(&evidence.body, kept);
        } else if let Some(kept) = self.kept {
            error = error.with_body(&self.body, kept);
        }
        if let Some(details) = self.details {
            error = error.with_reader_details(details);
        }
        error
    }
}

impl From<Error> for ReadError {
    /// From the error of a [`ByteStream`] that broke: it stays a failure of the transport.
    /// Any other forwarded error is a Mismatch without the reader's source.
    fn from(error: Error) -> ReadError {
        let mut failure = Self::not_the_type(Bytes::new());
        if error.is_from_byte_stream() {
            failure.kind = ReadKind::Forward(error);
        }
        failure.kept = None;
        failure
    }
}

/// A failure while reading a handed-over body, with the answer's known head.
///
/// Its kind is always [`Kind::Transport`]. `Display`, `Debug` and `source()`
/// follow the same value-free rules as [`Error`]. Use `?` to convert it into
/// any client's `Error<P>`, or into [`ReadError`] inside a reader.
pub struct StreamError(Error);

impl StreamError {
    /// The failure kind: always Transport.
    pub fn kind(&self) -> Kind {
        self.0.kind()
    }

    /// The endpoint whose body was handed over.
    pub fn endpoint(&self) -> &'static str {
        self.0.endpoint()
    }

    /// The known status of the answer.
    pub fn status(&self) -> Option<http::StatusCode> {
        self.0.status()
    }

    /// Whether the answer had this status.
    pub fn is_status(&self, code: u16) -> bool {
        self.0.is_status(code)
    }

    /// The known headers of the answer.
    pub fn headers(&self) -> Option<&Headers> {
        self.0.headers()
    }

    /// The transport failure class.
    pub fn class(&self) -> Option<Class> {
        self.0.class()
    }

    /// Whether a clock expired.
    pub fn is_timeout(&self) -> bool {
        self.0.is_timeout()
    }

    /// How many attempts entered the middleware chain.
    pub fn tries(&self) -> u32 {
        self.0.tries()
    }
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::fmt::Debug for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl std::error::Error for StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.0)
    }
}

impl<P> From<StreamError> for Error<P> {
    fn from(error: StreamError) -> Self {
        error.0.without_problem()
    }
}

impl From<StreamError> for ReadError {
    fn from(error: StreamError) -> Self {
        error.0.into()
    }
}

impl From<StreamError> for std::io::Error {
    fn from(error: StreamError) -> Self {
        std::io::Error::other(error)
    }
}

#[derive(Default)]
pub(crate) struct ReadEvidence {
    body: Vec<u8>,
    kept: Option<Kept>,
    remaining: Option<ResponseBody>,
    announced: Option<u64>,
    received: u64,
}

/// Finish collecting a failed binding as a refusal, including when a reader
/// declined the label or announced size before touching the stream.
pub(crate) async fn finish_refusal(
    evidence: &Arc<Mutex<ReadEvidence>>,
    status: http::StatusCode,
    headers: &Headers,
    info: &TryInfo,
    clock: Option<Arc<dyn BodyClock>>,
) {
    let mut stream = {
        let mut saved = evidence
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if saved.kept == Some(Kept::Cut) {
            return;
        }
        ByteStream {
            body: saved.remaining.take(),
            status,
            headers: headers.clone(),
            announced: saved.announced,
            received: saved.received,
            info: info.clone(),
            clock,
            waiting: None,
            evidence: Some(evidence.clone()),
        }
    };
    while stream.body.is_some() {
        match std::future::poll_fn(|cx| stream.poll_piece(cx)).await {
            Some(Ok(_)) => {
                if evidence
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .kept
                    == Some(Kept::Cut)
                {
                    break;
                }
            }
            _ => break,
        }
    }
}

type ResponseBody = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

/// A body handed over, to read as it comes.
///
/// It is what an endpoint gives with `read concord::read::Stream;` and `-> ByteStream`:
/// the call ends at the head, and the body is yours. The call's clocks go on while you read
/// it: the `timeout` of the call (`with_timeout`) runs until the end of the stream, and the
/// clock for silence (30 seconds by default) runs while a `try_next()` waits.
///
/// ```
/// # use concord_core as concord;
/// mod files {
/// #   use concord_core as concord;
///     pub struct Problem;
///     pub type Result<T> = std::result::Result<T, concord::Error<Problem>>;
/// }
/// async fn consume(mut body: concord::ByteStream) -> files::Result<usize> {
///     let mut length = 0;
///     while let Some(chunk) = body.try_next().await? {
///         length += chunk.len();
///     }
///     Ok(length)
/// }
/// # use futures_util::FutureExt;
/// let body = concord::read::Incoming::new(concord::reply(200, "hello")).stream();
/// assert_eq!(consume(body).now_or_never().unwrap().unwrap(), 5);
/// ```
pub struct ByteStream {
    body: Option<ResponseBody>,
    status: http::StatusCode,
    headers: Headers,
    announced: Option<u64>,
    received: u64,
    info: TryInfo,
    clock: Option<Arc<dyn BodyClock>>,
    waiting: Option<Pin<Box<dyn Future<Output = BodyTimeout> + Send>>>,
    evidence: Option<Arc<Mutex<ReadEvidence>>>,
}

impl Drop for ByteStream {
    fn drop(&mut self) {
        if let Some(evidence) = &self.evidence {
            let mut evidence = evidence
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            evidence.remaining = self.body.take();
            evidence.announced = self.announced;
            evidence.received = self.received;
        }
    }
}

impl std::fmt::Debug for ByteStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteStream")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

enum BodyFailure {
    Transport(TransportError),
    Timeout(BodyTimeout),
}

impl BodyFailure {
    fn into_error<P>(self, status: http::StatusCode, headers: Headers, info: &TryInfo) -> Error<P> {
        match self {
            Self::Transport(failure) => Error::transport_after_head(
                info.endpoint(),
                info.number(),
                status,
                headers,
                failure,
            ),
            Self::Timeout(timeout) => {
                let (clock, duration) = match timeout {
                    BodyTimeout::Idle(duration) => (Clock::Idle, duration),
                    BodyTimeout::IdleFromDescription(duration) => {
                        (Clock::DescriptionIdle, duration)
                    }
                    BodyTimeout::Total(duration) => (Clock::Total, duration),
                };
                Error::clock(info.endpoint(), info.number(), clock, duration)
                    .with_response(status, headers)
            }
        }
    }
}

impl ByteStream {
    /// The next piece, or `None` at the end. After an error the stream is over.
    ///
    /// Cancel safe: dropping this future loses no byte and stops its idle timer.
    pub fn try_next(
        &mut self,
    ) -> impl Future<Output = Result<Option<Bytes>, StreamError>> + Send + '_ {
        struct Wait<'a>(&'a mut ByteStream);
        impl Future for Wait<'_> {
            type Output = Result<Option<Bytes>, StreamError>;
            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                Pin::new(&mut *self.0)
                    .poll_next(cx)
                    .map(|piece| piece.transpose())
            }
        }
        impl Drop for Wait<'_> {
            fn drop(&mut self) {
                self.0.waiting = None;
            }
        }
        Wait(self)
    }

    /// The status of the answer that this body belongs to.
    pub fn status(&self) -> http::StatusCode {
        self.status
    }

    /// The headers of the answer that this body belongs to.
    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    fn error(&self, failure: BodyFailure) -> StreamError {
        StreamError(
            failure
                .into_error(self.status, self.headers.clone(), &self.info)
                .mark_byte_stream(),
        )
    }

    fn poll_piece(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<Bytes, BodyFailure>>> {
        let was_open = self.body.is_some();
        let piece = self.poll_piece_inner(cx);
        if was_open {
            if let Some(evidence) = &self.evidence {
                let mut evidence = evidence
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match &piece {
                    Poll::Ready(Some(Ok(bytes))) => {
                        // PANIC-SAFETY: 64 KiB fits usize on supported 32/64-bit targets; the subtraction is saturating.
                        let available = (64 * crate::KIB).saturating_sub(evidence.body.len());
                        let prefix = bytes.get(..bytes.len().min(available)).unwrap_or_default();
                        // PANIC-SAFETY: The retained evidence and prefix are bounded to a combined 64 KiB.
                        let needed = evidence.body.len() + prefix.len();
                        if needed > evidence.body.capacity() {
                            // PANIC-SAFETY: The retained evidence and prefix are bounded to a combined 64 KiB.
                            let extra = needed - evidence.body.len();
                            evidence.body.reserve_exact(extra);
                        }
                        evidence.body.extend_from_slice(prefix);
                        evidence.kept = Some(
                            if bytes.len() > available || evidence.kept == Some(Kept::Cut) {
                                Kept::Cut
                            } else {
                                Kept::Interrupted
                            },
                        );
                    }
                    Poll::Ready(Some(Err(_))) => {
                        if evidence.kept != Some(Kept::Cut) {
                            evidence.kept = Some(Kept::Interrupted);
                        }
                    }
                    Poll::Ready(None) => {
                        if evidence.kept != Some(Kept::Cut) {
                            evidence.kept = Some(Kept::Complete);
                        }
                    }
                    Poll::Pending => {}
                }
                self.record_refusal(&evidence);
            }
        }
        piece
    }

    fn record_refusal(&self, evidence: &ReadEvidence) {
        if let Some(clock) = &self.clock {
            clock.record(KnownOutcome {
                kind: KnownKind::Refused,
                status: self.status,
                headers: &self.headers,
                body: &evidence.body,
                kept: evidence.kept.unwrap_or(Kept::Interrupted),
            });
        }
    }

    fn poll_piece_inner(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, BodyFailure>>> {
        let Some(body) = self.body.as_mut() else {
            return Poll::Ready(None);
        };
        // Start the deadline before author/transport code can consume time in
        // its poll. Still poll the body first: ready work wins over the timer.
        if self.waiting.is_none() {
            self.waiting = self.clock.as_ref().map(|clock| clock.wait());
        }
        // Work first: a clock only ends a wait, never work that is already ready.
        let piece = match body.as_mut().poll_next(cx) {
            Poll::Ready(piece) => piece,
            Poll::Pending => {
                let Some(timer) = self.waiting.as_mut() else {
                    return Poll::Pending;
                };
                let Poll::Ready(timeout) = timer.as_mut().poll(cx) else {
                    return Poll::Pending;
                };
                self.body = None;
                self.waiting = None;
                return Poll::Ready(Some(Err(BodyFailure::Timeout(timeout))));
            }
        };
        self.waiting = None;
        match piece {
            Some(Ok(bytes)) => {
                // PANIC-SAFETY: usize is at most 64 bits on reqwest-supported targets; the cast cannot truncate.
                self.received = self.received.saturating_add(bytes.len() as u64);
                Poll::Ready(Some(Ok(bytes)))
            }
            Some(Err(error)) => {
                self.body = None;
                Poll::Ready(Some(Err(BodyFailure::Transport(TransportError::from(
                    error,
                )))))
            }
            None => {
                self.body = None;
                if self.announced.is_some_and(|length| self.received < length) {
                    Poll::Ready(Some(Err(BodyFailure::Transport(TransportError::new(
                        Class::Body,
                    )))))
                } else {
                    Poll::Ready(None)
                }
            }
        }
    }
}

/// The silence clock runs from a poll that finds no piece until the next piece,
/// including time between polls. Dropping a `next()` future cannot be observed by
/// this implementation. Use [`ByteStream::try_next`] to count only time spent
/// waiting inside each call; dropping that future resets its silence clock.
impl Stream for ByteStream {
    type Item = Result<Bytes, StreamError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.poll_piece(cx)
            .map(|piece| piece.map(|result| result.map_err(|failure| this.error(failure))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;
    #[test]
    fn places_print_only_their_first_32_pieces() {
        for (place, expected) in [
            (Place::root(), ""),
            (
                Place::root().field("items").index(3).field("kind"),
                "items[3].kind",
            ),
            (
                Place::root().field("data").key().index(0).field("kind"),
                "data[key][0].kind",
            ),
            (
                Place::root().variant().field("rows").index(0).field("kind"),
                "[variant].rows[0].kind",
            ),
        ] {
            assert_eq!(place.to_string(), expected);
            assert_eq!(format!("{place:?}"), expected);
            assert_eq!(format!("{place:#?}"), expected);
        }
        let mut place = Place::root();
        for n in 0..100 {
            place = place.index(n);
        }
        assert_eq!(place.pieces.len(), 32);
        assert_eq!(
            place.to_string(),
            (0..32).map(|n| format!("[{n}]")).collect::<String>()
        );
    }

    // Response values stay out of error sources through every reader error conversion.
    // Free text through `at` is rejected by its compile-fail example.
    #[test]
    fn reader_at_cannot_echo_answer_in_source_or_debug() {
        for error in [
            ReadError::not_the_type(bytes::Bytes::from_static(b"PRIVATE_SENTINEL")),
            ReadError::wrong_content_type(),
        ] {
            let error = error
                .with_details("PRIVATE_SENTINEL")
                .at(Place::root()
                    .field("items")
                    .key()
                    .variant()
                    .index(0)
                    .field("value"))
                .label_was_not_expected();
            assert_eq!(error.details(), Some("PRIVATE_SENTINEL"));
            let mut source: Option<&dyn std::error::Error> = Some(&error);
            while let Some(cause) = source {
                assert!(!format!("{cause} {cause:?} {cause:#?}").contains("PRIVATE_SENTINEL"));
                source = cause.source();
            }
        }
    }

    #[test]
    fn reader_causes_hold_only_place_and_kind() {
        let error = ReadError::not_the_type(bytes::Bytes::from_static(b"PRIVATE"))
            .with_details("PRIVATE".repeat(1000))
            .at(Place::root().field("items").index(3).field("kind"))
            .label_was_not_expected();
        assert!(error
            .source()
            .unwrap()
            .to_string()
            .contains("items[3].kind"));
        assert!(error
            .source()
            .unwrap()
            .to_string()
            .contains("unexpected content type"));
        assert!(!format!("{error:?} {error}").contains("PRIVATE"));
        assert!(error.source().unwrap().source().is_none());
        assert!(error.details.as_ref().unwrap().len() <= 1024 + "… [cut]".len());
        assert!(error.details.as_ref().unwrap().capacity() < 2048);
    }
    #[test]
    fn details_cut_at_a_utf8_boundary() {
        let error = ReadError::not_the_type(bytes::Bytes::new()).with_details("🦀".repeat(1000));
        let text = error.details.unwrap();
        assert!(text.len() <= 1024);
    }

    #[test]
    fn c5_6_reader_evidence_does_not_retain_the_original_allocation() {
        let mut storage = Vec::with_capacity(7_000_000);
        storage.extend_from_slice(b"private body");
        let bytes = Bytes::from(storage);
        let original = bytes.as_ptr();
        let error = ReadError::not_the_type(bytes);
        assert_eq!(error.body, "private body");
        assert_ne!(error.body.as_ptr(), original);
        assert_eq!(error.kept, Some(Kept::Complete));
        fn send_sync<T: Send + Sync>() {}
        send_sync::<ReadError>();
    }
}

#[cfg(test)]
mod c5_script_tests {
    use super::*;
    use crate::__private::{arm, no_arm, read_body, refused, refused_plain};
    use crate::answer::{KnownKind, KnownOutcome};
    use crate::error::Kind;
    // Testkit uses the non-test core crate. Only the shared reqwest response crosses
    // back into this unit-test crate; every helper under test is the local one.
    use concord_core::transport::{Transport as _, TryInfo as ScriptInfo};
    use concord_testkit::Script;
    use futures_util::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::time::Instant;

    struct Binary(usize);
    impl Reader<Bytes> for Binary {
        async fn read(&self, incoming: Incoming) -> Result<Bytes, ReadError> {
            incoming.bytes(self.0).await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c5_4_content_length_overrides_the_transport_hint_in_both_directions() {
        for (declared, body, limit, expected) in [
            (100, "abc", 200, Kind::Transport),
            (100, "abc", 10, Kind::Mismatch),
        ] {
            let script = Script::new();
            script.answer(200).declare_length(declared).body(body);
            let error: Error = read_body(&Binary(limit), response(&script).await, &info())
                .await
                .unwrap_err();
            assert_eq!(error.kind(), expected);
            let mut raw = http::Response::new(body);
            raw.headers_mut().insert("content-length", declared.into());
            let error: Error = read_body(&Binary(limit), reqwest::Response::from(raw), &info())
                .await
                .unwrap_err();
            assert_eq!(error.kind(), expected);
        }
        // A smaller declaration does not hide bytes or the reader's limit.
        for limit in [2, 3] {
            let script = Script::new();
            script.answer(200).body("abc");
            let mut scripted = response(&script).await;
            scripted
                .headers_mut()
                .insert("content-length", "1".parse().unwrap());
            let scripted: Result<_, Error> = read_body(&Binary(limit), scripted, &info()).await;
            let mut raw = http::Response::new("abc");
            raw.headers_mut()
                .insert("content-length", "1".parse().unwrap());
            let handmade: Result<_, Error> =
                read_body(&Binary(limit), reqwest::Response::from(raw), &info()).await;
            if limit == 2 {
                assert_eq!(scripted.unwrap_err().kind(), Kind::Mismatch);
                assert_eq!(handmade.unwrap_err().kind(), Kind::Mismatch);
            } else {
                assert_eq!(scripted.unwrap(), "abc");
                assert_eq!(handmade.unwrap(), "abc");
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c5_4_whole_body_allocation_stays_within_the_limit() {
        let limit = 10 * 1024 * 1024;
        let chunks = futures_util::stream::iter(
            (0..10240).map(|_| Ok::<_, std::io::Error>(Bytes::from(vec![0; 1024]))),
        );
        let bytes = Incoming::new(crate::reply(200, reqwest::Body::wrap_stream(chunks)))
            .bytes(limit)
            .await
            .unwrap();
        assert_eq!(bytes.len(), limit);
        assert!(bytes.try_into_mut().unwrap().capacity() <= limit);
    }

    #[tokio::test(start_paused = true)]
    async fn c5_11_a_readers_forwarded_author_error_loses_its_source_and_context() {
        struct Forward;
        impl Reader<()> for Forward {
            async fn read(&self, incoming: Incoming) -> Result<(), ReadError> {
                let body = incoming.bytes(100).await?;
                Err(Error::build_source(
                    "wrong.endpoint",
                    Box::new(std::io::Error::other(
                        String::from_utf8_lossy(&body).into_owned(),
                    )),
                )
                .into())
            }
        }
        let error: Error = read_body(&Forward, crate::reply(200, "PRIVATE_SENTINEL"), &info())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), Kind::Mismatch);
        assert_eq!(error.endpoint(), info().endpoint());
        assert_eq!(error.tries(), info().number());
        assert!(!error.details().to_string().contains("PRIVATE_SENTINEL"));
        use std::error::Error as _;
        assert!(error.source().unwrap().source().is_none());
    }
    struct StreamReader;
    impl Reader<ByteStream> for StreamReader {
        async fn read(&self, incoming: Incoming) -> Result<ByteStream, ReadError> {
            Ok(incoming.stream())
        }
    }
    struct Problem(AtomicUsize);
    impl Reader<Bytes> for Problem {
        async fn read(&self, incoming: Incoming) -> Result<Bytes, ReadError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            assert_eq!(incoming.status(), http::StatusCode::BAD_REQUEST);
            assert_eq!(incoming.headers().get("x-id"), Some("one"));
            incoming.bytes(64 * crate::KIB).await
        }
    }
    #[derive(Clone)]
    struct Snapshot {
        kind: KnownKind,
        status: http::StatusCode,
        body: Vec<u8>,
        kept: Kept,
    }
    struct TestClock {
        idle: Duration,
        total: Option<(Instant, Duration)>,
        waits: Arc<AtomicUsize>,
        evidence: Mutex<Vec<Snapshot>>,
    }
    impl TestClock {
        fn new(idle: u64, total: Option<u64>) -> Arc<Self> {
            Arc::new(Self {
                idle: Duration::from_secs(idle),
                total: total.map(|secs| {
                    (
                        Instant::now() + Duration::from_secs(secs),
                        Duration::from_secs(secs),
                    )
                }),
                waits: Arc::new(AtomicUsize::new(0)),
                evidence: Mutex::new(Vec::new()),
            })
        }
    }
    impl BodyClock for TestClock {
        fn wait(&self) -> Pin<Box<dyn Future<Output = BodyTimeout> + Send>> {
            let waits = self.waits.clone();
            let deadline = Instant::now() + self.idle;
            let (deadline, expired) = match self.total {
                Some((total, duration)) if total <= deadline => {
                    (total, BodyTimeout::Total(duration))
                }
                _ => (deadline, BodyTimeout::Idle(self.idle)),
            };
            Box::pin(async move {
                // Preparing a deadline is not a wait: ready body work never
                // polls this future. Count only timers actually polled.
                waits.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep_until(deadline).await;
                expired
            })
        }
        fn record(&self, evidence: KnownOutcome<'_>) {
            assert!(evidence.body.len() <= 64 * crate::KIB);
            self.evidence.lock().unwrap().push(Snapshot {
                kind: evidence.kind,
                status: evidence.status,
                body: evidence.body.to_vec(),
                kept: evidence.kept,
            });
        }
    }
    fn pending<F: Future>(future: Pin<&mut F>) -> bool {
        let waker = futures_util::task::noop_waker();
        future.poll(&mut Context::from_waker(&waker)).is_pending()
    }
    fn info() -> TryInfo {
        TryInfo::new("files.get", 3)
    }
    async fn response(script: &Script) -> reqwest::Response {
        script
            .send(
                reqwest::Request::new(
                    http::Method::GET,
                    "https://example.com/file".parse().unwrap(),
                ),
                ScriptInfo::new("files.get", 3),
            )
            .await
            .unwrap()
    }
    fn incoming(response: reqwest::Response, clock: &Arc<TestClock>) -> Incoming {
        Incoming::new(response).with_body_clock(clock.clone())
    }

    #[tokio::test(start_paused = true)]
    async fn c5_4_an_announced_excess_does_not_wait_for_a_byte() {
        let script = Script::new();
        script
            .answer(200)
            .declare_length(100)
            .pause(Duration::from_secs(60))
            .body("a")
            .stall();
        let clock = TestClock::new(1, None);
        let start = Instant::now();
        let error: Error = read_body(
            &Binary(10),
            incoming(response(&script).await, &clock),
            &info(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), Kind::Mismatch);
        assert_eq!(error.body(), None);
        assert_eq!(error.kept(), None);
        assert_eq!(clock.waits.load(Ordering::SeqCst), 0);
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn c5_4_pieces_are_counted_and_the_piece_past_the_limit_is_not_kept() {
        let script = Script::new();
        script.answer(200).body("ab").body("cde").stall();
        let error: Error = read_body(&Binary(3), response(&script).await, &info())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), Kind::Mismatch);
        assert_eq!(error.body(), Some(b"ab".as_slice()));
        assert_eq!(error.kept(), Some(Kept::Cut));
    }

    #[tokio::test(start_paused = true)]
    async fn c5_4_a_cut_or_unmet_declared_length_is_transport_with_the_head() {
        for declared in [false, true] {
            let script = Script::new();
            let answer = script.answer(201).header("x-id", "one").body("ab");
            if declared {
                answer.declare_length(10);
            } else {
                answer.cut();
            }
            let error: Error<String> = read_body(&Binary(100), response(&script).await, &info())
                .await
                .unwrap_err();
            assert_eq!(error.kind(), Kind::Transport);
            assert_eq!(error.class(), Some(Class::Body));
            assert_eq!(error.status(), Some(http::StatusCode::CREATED));
            assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
            assert_eq!(error.endpoint(), "files.get");
            assert_eq!(error.tries(), 3);
            assert_eq!(error.body(), None);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c5_4_unexpected_eof_after_a_head_is_body_for_whole_and_stream_reads() {
        fn broken_response() -> reqwest::Response {
            crate::reply_with(
                201,
                &[("x-id", "one")],
                reqwest::Body::wrap_stream(futures_util::stream::iter([
                    Ok(Bytes::from_static(b"prefix")),
                    Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
                ])),
            )
        }

        // Prove this fixture carries a real reqwest body error with the I/O cause
        // that would classify as Connect without the response-head context.
        let failure = broken_response().bytes().await.unwrap_err();
        assert!(failure.is_body() || failure.is_decode());
        let mut cause = std::error::Error::source(&failure);
        let mut has_eof = false;
        while let Some(source) = cause {
            has_eof |= source
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::UnexpectedEof);
            cause = source.source();
        }
        assert!(has_eof);

        for streaming in [false, true] {
            let error: Error = if streaming {
                let mut stream = Incoming::new(broken_response()).with_info(&info()).stream();
                assert_eq!(stream.try_next().await.unwrap().unwrap(), "prefix");
                let error = stream.try_next().await.unwrap_err();
                assert!(stream.try_next().await.unwrap().is_none());
                error.into()
            } else {
                read_body(&Binary(100), broken_response(), &info())
                    .await
                    .unwrap_err()
            };
            assert_eq!(error.kind(), Kind::Transport);
            assert_eq!(error.class(), Some(Class::Body));
            assert_eq!(error.status(), Some(http::StatusCode::CREATED));
            assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
            assert_eq!(error.endpoint(), "files.get");
            assert_eq!(error.tries(), 3);
            assert_eq!(
                error.to_string(),
                "transport error: the body broke after status 201, in files.get, after 3 tries"
            );
            assert!(std::error::Error::source(&error).is_none());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c5_4_even_a_clean_stream_end_before_its_announced_size_is_transport() {
        let script = Script::new();
        script.answer(200).body("ab");
        let mut incoming = Incoming::new(response(&script).await);
        incoming.stream.announced = Some(10);
        let error: Error = read_body(&Binary(100), incoming, &info())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), Kind::Transport);
        assert_eq!(error.class(), Some(Class::Body));
        assert!(error
            .to_string()
            .contains("the body broke after status 200"));
    }

    #[tokio::test(start_paused = true)]
    async fn c5_6_refusal_decodes_only_a_complete_body_within_64_kib() {
        for length in [0, 64 * crate::KIB, 64 * crate::KIB + 1] {
            let script = Script::new();
            script
                .answer(400)
                .header("x-id", "one")
                .body(vec![b'x'; length]);
            let problem = Problem(AtomicUsize::new(0));
            let error = refused(&problem, response(&script).await, &info()).await;
            let complete = length <= 64 * crate::KIB;
            assert_eq!(error.kind(), Kind::Refused);
            assert_eq!(
                error.kept(),
                Some(if complete { Kept::Complete } else { Kept::Cut })
            );
            assert_eq!(error.body().unwrap().len(), length.min(64 * crate::KIB));
            assert_eq!(error.problem().is_some(), complete);
            assert_eq!(problem.0.load(Ordering::SeqCst), usize::from(complete));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c5_7_refused_survives_a_cut_a_short_body_and_silence() {
        for end in 0..3 {
            let script = Script::new();
            let answer = script.answer(400).header("x-id", "one").body("prefix");
            match end {
                0 => {
                    answer.cut();
                }
                1 => {
                    answer.declare_length(100);
                }
                _ => {
                    answer.stall();
                }
            }
            let problem = Problem(AtomicUsize::new(0));
            let clock = TestClock::new(2, None);
            let error = refused(&problem, incoming(response(&script).await, &clock), &info()).await;
            assert_eq!(error.kind(), Kind::Refused);
            assert_eq!(error.body(), Some(b"prefix".as_slice()));
            assert_eq!(error.kept(), Some(Kept::Interrupted));
            assert!(error.problem().is_none());
            assert_eq!(problem.0.load(Ordering::SeqCst), 0);
            let evidence = clock.evidence.lock().unwrap();
            let last = evidence.last().unwrap();
            assert_eq!(last.kind, KnownKind::Refused);
            assert_eq!(last.status, http::StatusCode::BAD_REQUEST);
            assert_eq!(last.body, b"prefix");
            assert_eq!(last.kept, Kept::Interrupted);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c5_2_no_arm_keeps_its_mismatch_when_the_body_breaks_or_stalls() {
        for cut in [true, false] {
            let script = Script::new();
            let answer = script.answer(200).header("x-id", "one").body("prefix");
            if cut {
                answer.cut();
            } else {
                answer.stall();
            }
            let clock = TestClock::new(1, None);
            let error: Error = no_arm(incoming(response(&script).await, &clock), &info()).await;
            assert_eq!(error.kind(), Kind::Mismatch);
            assert_eq!(error.body(), Some(b"prefix".as_slice()));
            assert_eq!(error.kept(), Some(Kept::Interrupted));
            assert!(error.to_string().contains("no arm takes this status"));
            assert_eq!(
                clock.evidence.lock().unwrap().last().unwrap().kind,
                KnownKind::NoArm
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c5_7_a_total_clock_cancellation_can_recover_progressive_evidence() {
        let script = Script::new();
        script.answer(503).body("prefix").stall();
        let clock = TestClock::new(100, Some(5));
        let info = info();
        let mut future = Box::pin(refused_plain::<()>(
            incoming(response(&script).await, &clock),
            &info,
        ));
        assert!(pending(future.as_mut()));
        tokio::time::advance(Duration::from_secs(6)).await;
        drop(future); // The engine cancels the whole try without polling it again.
        let evidence = clock.evidence.lock().unwrap();
        let last = evidence.last().unwrap();
        assert_eq!(last.kind, KnownKind::Refused);
        assert_eq!(last.status, http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(last.body, b"prefix");
        assert_eq!(last.kept, Kept::Interrupted);
    }

    #[tokio::test(start_paused = true)]
    async fn c5_10_stream_ends_the_try_at_the_head_and_later_errors_keep_context() {
        let script = Script::new();
        script.answer(201).header("x-id", "one").body("a").cut();
        let mut body = read_body::<_, (), _>(&StreamReader, response(&script).await, &info())
            .await
            .unwrap();
        assert_eq!(body.status(), http::StatusCode::CREATED);
        assert_eq!(body.headers().get("x-id"), Some("one"));
        assert_eq!(body.try_next().await.unwrap().unwrap(), "a");
        let error = body.next().await.unwrap().unwrap_err();
        assert_eq!(error.kind(), Kind::Transport);
        assert_eq!(error.class(), Some(Class::Body));
        assert_eq!(error.status(), Some(http::StatusCode::CREATED));
        assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
        assert_eq!(error.endpoint(), "files.get");
        assert_eq!(error.tries(), 3);
        assert!(body.try_next().await.unwrap().is_none());
        assert!(body.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn c5_10_stream_silence_is_transport_timeout_and_success_did_not_read() {
        let script = Script::new();
        script
            .answer(200)
            .pause(Duration::from_secs(10))
            .body("later");
        let clock = TestClock::new(2, None);
        let start = Instant::now();
        let mut body = read_body::<_, (), _>(
            &StreamReader,
            incoming(response(&script).await, &clock),
            &info(),
        )
        .await
        .unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(clock.waits.load(Ordering::SeqCst), 0);
        let error = body.try_next().await.unwrap_err();
        assert!(error.is_timeout());
        assert_eq!(error.status(), Some(http::StatusCode::OK));
        assert_eq!(error.to_string(), "transport error: the body was silent for 2 s after status 200 (with_idle_timeout changes it), in files.get, after 3 tries");
        assert_eq!(start.elapsed(), Duration::from_secs(2));
        assert!(body.try_next().await.unwrap().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn c5_10_cancelled_try_next_loses_no_byte_and_idle_does_not_run_between_calls() {
        let script = Script::new();
        script
            .answer(200)
            .pause(Duration::from_secs(4))
            .body("a")
            .pause(Duration::from_secs(1))
            .body("b");
        let clock = TestClock::new(2, None);
        let mut body = read_body::<_, (), _>(
            &StreamReader,
            incoming(response(&script).await, &clock),
            &info(),
        )
        .await
        .unwrap();
        let mut next = Box::pin(body.try_next());
        assert!(pending(next.as_mut()));
        tokio::time::advance(Duration::from_secs(1)).await;
        drop(next);
        tokio::time::advance(Duration::from_millis(1500)).await;
        assert_eq!(body.try_next().await.unwrap().unwrap(), "a");
        tokio::time::advance(Duration::from_secs(100)).await;
        assert_eq!(body.try_next().await.unwrap().unwrap(), "b");
        assert!(body.try_next().await.unwrap().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn c5_10_total_deadline_follows_the_stream_but_ready_work_wins() {
        for waits in [true, false] {
            let script = Script::new();
            let answer = script.answer(200).body("a");
            let answer = if waits {
                answer.pause(Duration::from_secs(1))
            } else {
                answer
            };
            answer.body("b");
            let clock = TestClock::new(30, Some(5));
            let mut body = read_body::<_, (), _>(
                &StreamReader,
                incoming(response(&script).await, &clock),
                &info(),
            )
            .await
            .unwrap();
            assert_eq!(body.try_next().await.unwrap().unwrap(), "a");
            tokio::time::advance(Duration::from_secs(100)).await;
            if waits {
                let error = body.try_next().await.unwrap_err();
                assert!(error.to_string().contains("the call took longer than 5 s"));
            } else {
                assert_eq!(body.try_next().await.unwrap().unwrap(), "b");
                assert!(body.try_next().await.unwrap().is_none());
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c7_6_a_readers_own_wait_is_not_silence_of_the_body() {
        struct Waiting;
        impl Reader<Bytes> for Waiting {
            async fn read(&self, incoming: Incoming) -> Result<Bytes, ReadError> {
                tokio::time::sleep(Duration::from_secs(100)).await;
                incoming.bytes(10).await
            }
        }
        let script = Script::new();
        script.answer(200).body("ok");
        let clock = TestClock::new(1, None);
        let value =
            read_body::<_, (), _>(&Waiting, incoming(response(&script).await, &clock), &info())
                .await
                .unwrap();
        assert_eq!(value, "ok");
        assert_eq!(clock.waits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn c5_11_from_stream_error_stays_transport_in_a_reader() {
        struct ThroughStream;
        impl Reader<()> for ThroughStream {
            async fn read(&self, incoming: Incoming) -> Result<(), ReadError> {
                let mut stream = incoming.stream();
                while stream.try_next().await?.is_some() {}
                Ok(())
            }
        }
        let script = Script::new();
        script.answer(200).header("x-id", "one").body("a").cut();
        let error: Error<String> = read_body(&ThroughStream, response(&script).await, &info())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), Kind::Transport);
        assert_eq!(error.class(), Some(Class::Body));
        assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
        assert_eq!(error.tries(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn c5_5_an_arm_that_does_not_bind_body_drops_it_without_a_read() {
        let script = Script::new();
        script.answer(200).body("unwanted").cut();
        let response = response(&script).await;
        let value = arm::<_, ()>(
            Ok(7),
            response.status(),
            &response.headers().clone().into(),
            &info(),
        )
        .unwrap();
        drop(response);
        assert_eq!(value, 7);
    }

    #[tokio::test(start_paused = true)]
    async fn c5_11_from_stream_error_preserves_the_body_clock_note_and_source() {
        struct ThroughStream;
        impl Reader<()> for ThroughStream {
            async fn read(&self, incoming: Incoming) -> Result<(), ReadError> {
                let mut stream = incoming.stream();
                while stream.try_next().await?.is_some() {}
                Ok(())
            }
        }
        for (idle, total, text) in [
            (
                1,
                None,
                "the body was silent for 1 s after status 200 (with_idle_timeout changes it)",
            ),
            (30, Some(2), "the call took longer than 2 s (with_timeout)"),
        ] {
            let script = Script::new();
            script.answer(200).header("x-id", "one").body("a").stall();
            let clock = TestClock::new(idle, total);
            let error: Error<String> = read_body(
                &ThroughStream,
                incoming(response(&script).await, &clock),
                &info(),
            )
            .await
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("transport error: {text}, in files.get, after 3 tries")
            );
            assert!(std::error::Error::source(&error).is_none());
            assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
        }
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    #[tokio::test]
    async fn c5_4_actual_two_frame_reader_caps_the_reservation() {
        let largest = std::cell::Cell::new(0);
        let frames = futures_util::stream::iter([
            Ok::<_, std::io::Error>(Bytes::from_static(b"a")),
            Ok(Bytes::from_static(b"b")),
        ]);
        let response = crate::reply_with(
            200,
            &[("content-length", "1073741824")],
            reqwest::Body::wrap_stream(frames),
        );
        let result = Incoming::new(response)
            .bytes_with_reservation(1024 * crate::MIB, |buffer, capacity| {
                largest.set(largest.get().max(capacity));
                reserve_join(buffer, capacity)
            })
            .await;
        assert!(result.is_err());
        assert_eq!(largest.get(), 64 * crate::KIB);
    }

    #[tokio::test]
    async fn c5_4_both_initial_and_growing_reservations_return_body_errors() {
        for fail_at in [1, 2] {
            let frames = futures_util::stream::iter([
                Ok::<_, std::io::Error>(Bytes::from_static(b"a")),
                Ok(Bytes::from_static(b"b")),
                Ok(Bytes::from(vec![b'c'; 100])),
            ]);
            let response = crate::reply(201, reqwest::Body::wrap_stream(frames));
            let mut calls = 0;
            let error = Incoming::new(response)
                .bytes_with_reservation(200, |buffer, capacity| {
                    calls += 1;
                    if calls == fail_at {
                        Err(Vec::<u8>::new().try_reserve_exact(usize::MAX).unwrap_err())
                    } else {
                        reserve_join(buffer, capacity)
                    }
                })
                .await
                .unwrap_err();
            let error: Error = error.into_error(
                http::StatusCode::CREATED,
                Headers::default(),
                &TryInfo::new("reserve.get", 1),
            );
            assert_eq!(calls, fail_at);
            assert_eq!(error.class(), Some(Class::Body));
            assert_eq!(error.status(), Some(http::StatusCode::CREATED));
        }
    }

    #[tokio::test]
    async fn c5_4_an_honest_large_body_has_exact_capacity_and_linear_growth() {
        const TOTAL: usize = 64 * crate::MIB + 1;
        for frame_size in [16 * crate::KIB, crate::MIB] {
            let frame = Bytes::from(vec![b'x'; frame_size]);
            let mut remaining = TOTAL;
            let source = futures_util::stream::iter(std::iter::from_fn(move || {
                if remaining == 0 {
                    return None;
                }
                let length = remaining.min(frame.len());
                remaining -= length;
                Some(Ok::<_, std::io::Error>(frame.slice(..length)))
            }));
            let response = crate::reply_with(
                200,
                &[("content-length", "67108865")],
                reqwest::Body::wrap_stream(source),
            );
            let mut allocations = 0;
            let mut reserved_sum = 0usize;
            let mut largest = 0;
            let body = Incoming::new(response)
                .bytes_with_reservation(TOTAL.saturating_mul(2), |bytes, capacity| {
                    allocations += 1;
                    reserved_sum += capacity;
                    largest = largest.max(capacity);
                    // Fail a per-frame growth mutant early, before it can do quadratic work.
                    assert!(allocations <= 32, "growth must be geometric: {allocations}");
                    assert!(
                        reserved_sum <= 4 * TOTAL,
                        "amortized reservation bound: {reserved_sum}"
                    );
                    reserve_join(bytes, capacity)
                })
                .await
                .unwrap();
            assert_eq!(body.len(), TOTAL);
            assert_eq!(largest, TOTAL);
            assert_eq!(body.try_into_mut().unwrap().capacity(), TOTAL);
        }
    }

    #[tokio::test]
    async fn c5_4_growth_stays_linear_after_an_understated_hint() {
        const TOTAL: usize = 2 * crate::MIB;
        let frame = Bytes::from(vec![b'x'; 16 * crate::KIB]);
        let source = futures_util::stream::iter(
            (0..128).map(move |_| Ok::<_, std::io::Error>(frame.clone())),
        );
        let response = crate::reply_with(
            200,
            &[("content-length", "0")],
            reqwest::Body::wrap_stream(source),
        );
        let mut allocations = 0;
        let mut reserved_sum = 0;
        let bytes = Incoming::new(response)
            .bytes_with_reservation(TOTAL, |buffer, capacity| {
                allocations += 1;
                reserved_sum += capacity;
                assert!(
                    allocations <= 32,
                    "understated hints must not disable geometric growth"
                );
                assert!(reserved_sum <= 4 * TOTAL);
                reserve_join(buffer, capacity)
            })
            .await
            .unwrap();
        assert_eq!(bytes.len(), TOTAL);
    }

    #[test]
    fn c5_4_two_bytes_cannot_reserve_a_gibibyte() {
        assert_eq!(join_capacity(2, Some(1024 * crate::MIB)), 64 * crate::KIB);
        assert_eq!(join_capacity(2000, Some(3000)), 3000);
        assert_eq!(join_capacity(70_000, Some(1 << 30)), 140_000);
    }

    #[test]
    fn c5_4_a_failed_reservation_is_a_body_error() {
        let stream = Incoming::new(crate::reply(200, "")).stream;
        let error = reserve_join(&mut Vec::new(), usize::MAX).unwrap_err();
        let error: Error = allocation_error(&stream, error).into_error(
            http::StatusCode::OK,
            Headers::default(),
            &TryInfo::new("reserve.get", 1),
        );
        assert_eq!(error.class(), Some(Class::Body));
        assert_eq!(error.status(), Some(http::StatusCode::OK));
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn c5_4_a_huge_content_length_returns_instead_of_aborting() {
        const NAME: &str =
            "read::allocation_tests::c5_4_a_huge_content_length_returns_instead_of_aborting";
        if std::env::var_os("CONCORD_ALLOCATION_CHILD").is_some() {
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let frames = futures_util::stream::iter([
                Ok::<_, std::io::Error>(Bytes::from_static(b"a")),
                Ok(Bytes::from_static(b"b")),
            ]);
            let response = crate::reply_with(
                200,
                &[("content-length", "1152921504606846976")],
                reqwest::Body::wrap_stream(frames),
            );
            let error = rt
                .block_on(Incoming::new(response).bytes(usize::MAX))
                .unwrap_err();
            let error: Error = error.into_error(
                http::StatusCode::OK,
                Headers::default(),
                &TryInfo::new("huge.get", 1),
            );
            assert_eq!(error.class(), Some(Class::Body));
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("CONCORD_ALLOCATION_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
