//! The open format contracts: [`Encode`], [`Decode`], the response head they read, and
//! [`CodecError`].
//!
//! Every body (`body item: NewItem`) and every typed output (`-> Item`) has a format, given by the
//! nearest `request`/`response`/`format` statement of its endpoint, an enclosing scope or the
//! client. A format is any type implementing `Encode<T>` and/or `Decode<T>`, named by its Rust
//! path (`format Json;`, `response my_sdk::Csv;`). This crate implements none: the standard
//! formats are ordinary implementations in another crate, with no privilege over yours. Concord
//! never chooses a format by looking at a Rust type name. An endpoint without `->` has no typed
//! output: any 2xx succeeds and the body is drained without being buffered, whatever the
//! defaults.

use std::error::Error as StdError;
use std::fmt;

use bytes::Bytes;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use reqwest::{StatusCode, Version};

use crate::response::Incoming;

/// Encodes a request body of type `T` with the format `Self`.
///
/// Implement it on your own marker type for a custom format, then name the type in a statement
/// (`request Csv;`, `format Csv;`). The local marker type satisfies the orphan rules whatever `T`
/// is. The encoder runs when the request is prepared; it receives the payload **by value**, so a
/// one-shot, non-`Clone` payload can be moved into a native streaming body
/// (`reqwest::Body::wrap_stream`, `Body::wrap`).
///
/// `CONTENT_TYPE` is the media type sent as `Content-Type` with the body, unless the body's
/// `request` statement declares its own (`request Csv "text/csv; header=present";`) or the
/// endpoint sends a `content-type` header. It is validated when the declaration is compiled
/// (visible ASCII, like `HeaderValue::from_static`): an invalid value is a compile error wherever
/// it may be sent, and is not used at all where a declared media type replaces it.
///
/// A media type known only once the body is encoded (a generated multipart boundary, a CSV
/// dialect chosen per job) is a **runtime** media type: override
/// [`encode_body`](Self::encode_body) to return it with the body in an [`EncodedBody`]. Concord
/// calls `encode_body` (whose default wraps [`encode`](Self::encode) with no runtime media type),
/// so existing formats need no change. The precedence of a runtime media type over declarations
/// is documented on [`EncodedBody`].
#[diagnostic::on_unimplemented(
    message = "the format `{Self}` cannot encode a `{T}` request body",
    label = "this body's format is `{Self}` (the nearest `request` statement: on the endpoint, a scope or the client); it needs `{Self}: Encode<{T}>`",
    note = "a format is a type implementing `Encode<{T}>`, named by its path in a statement (`request Ndjson;`)"
)]
pub trait Encode<T> {
    /// The media type sent as `Content-Type` with the body (see the trait documentation).
    const CONTENT_TYPE: Option<&'static str>;

    /// Encodes the payload into a request body.
    ///
    /// # Errors
    ///
    /// A [`CodecError`] when the payload cannot be encoded: the request is not built
    /// ([`ErrorKind::Build`](crate::ErrorKind::Build)).
    fn encode(value: T) -> Result<reqwest::Body, CodecError>;

    /// Encodes the payload into a body and, for a format whose media type depends on the
    /// payload, the runtime media type that describes it. This is what Concord calls when it
    /// prepares a request.
    ///
    /// The default is [`encode`](Self::encode) without a runtime media type (then
    /// [`CONTENT_TYPE`](Self::CONTENT_TYPE) or a declared media type applies, as before). A format
    /// that overrides it should make `encode` agree, or refuse there, for code that calls `encode`
    /// directly.
    ///
    /// # Errors
    ///
    /// As for [`encode`](Self::encode).
    fn encode_body(value: T) -> Result<EncodedBody, CodecError> {
        Self::encode(value).map(EncodedBody::new)
    }
}

/// A request body with the media type it was encoded with at run time, returned by
/// [`Encode::encode_body`].
///
/// The body itself carries its length: a body built from bytes is sent with `Content-Length`,
/// a lazily produced one whose size hint is exact likewise, any other stream chunked. Concord
/// neither buffers a body to learn its length nor claims a length a stream does not report.
///
/// # How a runtime media type meets the declaration
///
/// A runtime media type is bound to its bytes, so Concord never sends the body under another
/// one. When the request is prepared (before any I/O):
///
/// | Declaration for the body | Static media (`EncodedBody::new`) | Runtime media `R` |
/// |---|---|---|
/// | nothing | `Encode::CONTENT_TYPE`, if any | `R` |
/// | `request F "m";` (endpoint, scope or client) | `m` | `R` if `m == R` byte for byte, else a `Build` error |
/// | a sent `content-type` header (literal, native name, inherited, `override`, `append`) | the header wins | accepted if it is exactly one value equal to `R`, else a `Build` error |
/// | the header chain ends with `remove content-type` | the format's media type is sent | `R` is sent |
/// | an optional body that is absent | no media type | no media type |
///
/// Equality is byte equality of the header values: Concord parses no media type, so
/// `text/csv;header=present` and `text/csv; header=present` differ. The sensitivity of the
/// `content-type` key applies to an inserted runtime value as to a declared one.
pub struct EncodedBody {
    body: reqwest::Body,
    content_type: Option<HeaderValue>,
}

impl EncodedBody {
    /// A body without a runtime media type: the format's `CONTENT_TYPE` or the declared media
    /// type applies.
    pub fn new(body: reqwest::Body) -> Self {
        Self {
            body,
            content_type: None,
        }
    }

    /// A body bound to the media type `content_type` (a validated header value).
    pub fn with_content_type(body: reqwest::Body, content_type: HeaderValue) -> Self {
        Self {
            body,
            content_type: Some(content_type),
        }
    }

    /// The runtime media type, if any.
    pub fn content_type(&self) -> Option<&HeaderValue> {
        self.content_type.as_ref()
    }

    /// The body and its runtime media type.
    pub fn into_parts(self) -> (reqwest::Body, Option<HeaderValue>) {
        (self.body, self.content_type)
    }
}

impl From<reqwest::Body> for EncodedBody {
    fn from(body: reqwest::Body) -> Self {
        Self::new(body)
    }
}

impl fmt::Debug for EncodedBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedBody")
            .field("runtime_content_type", &self.content_type.is_some())
            .field("body", &format_args!("..."))
            .finish()
    }
}

/// Decodes a successful response into `T` with the format `Self`.
///
/// Implement it on your own marker type for a custom format, then name the type in a statement
/// (`response Csv;`). On a **2xx** response, Concord calls, in this order:
///
/// 1. [`check_content_type`](Self::check_content_type) on the borrowed [`ResponseHead`]: the
///    format's checks of the `Content-Type` field — and nothing else, because the
///    `IgnoreContentType` adapter of the standard formats skips exactly this step;
/// 2. [`context`](Self::context) on the borrowed head: every other check of the head (a version
///    header, a status the format treats specially), producing the owned
///    [`Context`](Self::Context) the body decoder needs;
/// 3. reads the body (only now);
/// 4. [`decode_body`](Self::decode_body) with that context and the bytes.
///
/// A failure in step 1 or 2 is an [`ErrorKind::Metadata`](crate::ErrorKind::Metadata) error that
/// keeps the native response **unread** ([`Error::into_response`](crate::Error::into_response));
/// a failure in step 4 is [`ErrorKind::Decode`](crate::ErrorKind::Decode). A non-2xx response is
/// a [`Status`](crate::ErrorKind::Status) error before any of this: the format is not consulted.
/// `decode_body` never receives the head, only the `Context` that `context` built from it. A
/// refusal placed in step 1 or 2 therefore keeps the response unread; a format that carries head
/// data in its `Context` and refuses on it in `decode_body` makes that refusal a `Decode` error
/// after the body was consumed, so put every refusal that depends only on the head in the first
/// two steps.
///
/// The response `Content-Type` is the format's to check; Concord never compares it with the
/// request's `Accept`. A format may suggest an `Accept` ([`ACCEPT`](Self::ACCEPT)); the suggestion
/// never selects another format.
///
/// A stateless format names its context `()`:
///
/// ```
/// use concord_core::bytes::Bytes;
/// use concord_core::{CodecError, Decode, ResponseHead};
///
/// /// Line count of a text body.
/// pub struct Lines;
///
/// impl Decode<usize> for Lines {
///     type Context = ();
///     fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
///         Ok(())
///     }
///     fn decode_body((): (), body: Bytes) -> Result<usize, CodecError> {
///         Ok(body.split(|b| *b == b'\n').count())
///     }
/// }
/// ```
///
/// `decode_body` is also the body-only route for a document you obtained yourself (the body of
/// a [`Status`](crate::ErrorKind::Status) error, say): calling it performs **no** head check, by
/// name. It cannot look at the head:
///
/// ```compile_fail,E0050
/// use concord_core::{CodecError, Decode, ResponseHead};
/// struct Late;
/// impl Decode<u32> for Late {
///     type Context = ();
///     fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> { Ok(()) }
///     fn decode_body(
///         (): (),
///         body: concord_core::bytes::Bytes,
///         head: &ResponseHead<'_>, // no: the head is gone once the body is read
///     ) -> Result<u32, CodecError> {
///         let _ = (body, head);
///         Ok(0)
///     }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "the format `{Self}` cannot decode a `{T}` response",
    label = "this output's format is `{Self}` (the nearest `response` statement: on the endpoint, a scope or the client); it needs `{Self}: Decode<{T}>`",
    note = "a format is a type implementing `Decode<{T}>`, named by its path in a statement (`response Csv;`); an endpoint without typed output omits `->`"
)]
pub trait Decode<T> {
    /// Owned per-response state taken from the head and consumed by
    /// [`decode_body`](Self::decode_body): `()` for a stateless format, a parsed version, a
    /// charset. It lives across the body read, so it is `Send` (the request future stays `Send`)
    /// and `'static` (it cannot borrow the response).
    type Context: Send + 'static;

    /// Erased response handling used by generated error-document declarations.
    #[doc(hidden)]
    fn __concord_error_head(
        head: &ResponseHead<'_>,
    ) -> Result<crate::prepared::BodyStep<T>, CodecError> {
        Self::check_content_type(head)?;
        let context = Self::context(head)?;
        Ok(Box::new(move |body| Self::decode_body(context, body)))
    }

    /// A request `Accept` value this format suggests, sent only as a fallback: for an endpoint
    /// with a typed output whose header chain leaves no `accept` value and does not end by
    /// removing one. An authored `Accept` (literal or native, present or empty) always wins.
    /// Validated when the declaration is compiled, like [`Encode::CONTENT_TYPE`]. The suggestion
    /// is a request preference: the response is still checked by
    /// [`check_content_type`](Self::check_content_type), not compared with it.
    const ACCEPT: Option<&'static str> = None;

    /// The format's checks of the response `Content-Type` field. Accepts everything unless
    /// overridden. Put every `Content-Type`-derived check here (media type, charset), and only
    /// those: the standard `IgnoreContentType` adapter skips exactly this step.
    ///
    /// # Errors
    ///
    /// A [`CodecError`] when the format refuses the field: the call becomes an
    /// [`ErrorKind::Metadata`](crate::ErrorKind::Metadata) error and the body stays unread.
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        let _ = head;
        Ok(())
    }

    /// Every other check of the response head, and the state the body decoder needs.
    ///
    /// It may read `Content-Type` to build its state (a charset to decode with), but should not
    /// refuse on it: the `IgnoreContentType` adapter does not skip this step, so a refusal here
    /// would survive it. When the field is missing or unreadable, fall back to a default.
    ///
    /// # Errors
    ///
    /// A [`CodecError`] when the head is not acceptable: an
    /// [`ErrorKind::Metadata`](crate::ErrorKind::Metadata) error, body unread.
    fn context(head: &ResponseHead<'_>) -> Result<Self::Context, CodecError>;

    /// Decodes a complete body with the context produced from its head. Called directly, it
    /// decodes bytes **without** any head check.
    ///
    /// # Errors
    ///
    /// A [`CodecError`] when the body does not decode: an
    /// [`ErrorKind::Decode`](crate::ErrorKind::Decode) error.
    fn decode_body(context: Self::Context, body: Bytes) -> Result<T, CodecError>;
}

/// Opens a successful response for incremental reading into `Records` with the format `Self`:
/// the contract of an endpoint declared `-> stream T` (`response JsonValues;`, say).
///
/// It is the streaming counterpart of [`Decode`], not a variant of it: [`Decode`] receives a
/// collected body, `Incremental` receives the **unread** body and returns an owned reader. On a
/// **2xx** response Concord calls [`check_content_type`](Self::check_content_type), then
/// [`context`](Self::context), both on the borrowed head (a refusal is an
/// [`ErrorKind::Metadata`](crate::ErrorKind::Metadata) error that keeps the response unread), then
/// [`open`](Self::open) with the context and the [`Incoming`] body. Opening polls nothing; the
/// reader reads as its caller asks and names the operation in its own errors. A non-2xx response
/// is a [`Status`](crate::ErrorKind::Status) error before any of this.
///
/// ```
/// use concord_core::{CodecError, Incoming, Incremental, ResponseHead};
///
/// /// Hands the unread body over as is (a reader of your own then reads it).
/// pub struct Opaque;
///
/// impl Incremental<()> for Opaque {
///     type Records = Incoming;
///     type Context = ();
///     fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
///         Ok(())
///     }
///     fn open((): (), body: Incoming) -> Incoming {
///         body
///     }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "the format `{Self}` cannot open a stream of `{T}`",
    label = "this streaming output's format is `{Self}` (the nearest `response` statement: on the endpoint, a scope or the client); it needs `{Self}: Incremental<{T}>`",
    note = "`-> stream T` needs a format implementing `Incremental<{T}>` (an incremental reader); a finite `-> T` needs `Decode<{T}>`; `-> stream` without an item returns the unread `Incoming` body"
)]
pub trait Incremental<T> {
    /// The owned reader returned by the endpoint's `send()`.
    type Records;

    /// Owned per-response state taken from the head for [`open`](Self::open).
    type Context: Send + 'static;

    /// A request `Accept` value this format suggests, as [`Decode::ACCEPT`].
    const ACCEPT: Option<&'static str> = None;

    /// The format's checks of the response `Content-Type` field, as
    /// [`Decode::check_content_type`] (the `IgnoreContentType` adapter skips exactly this step).
    ///
    /// # Errors
    ///
    /// A [`CodecError`]: an [`ErrorKind::Metadata`](crate::ErrorKind::Metadata) error, body unread.
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        let _ = head;
        Ok(())
    }

    /// Every other check of the head, and the state [`open`](Self::open) needs.
    ///
    /// # Errors
    ///
    /// A [`CodecError`]: an [`ErrorKind::Metadata`](crate::ErrorKind::Metadata) error, body unread.
    fn context(head: &ResponseHead<'_>) -> Result<Self::Context, CodecError>;

    /// Builds the reader over the unread body. Must not poll it.
    fn open(context: Self::Context, body: Incoming) -> Self::Records;
}

/// The reader an incremental format `F` opens for items `I`: the output type of an endpoint
/// declared `-> stream I` with `response F;` (its `send()` returns one).
pub type Records<F, I> = <F as Incremental<I>>::Records;

/// The head of a response — status, version and header fields — borrowed from the native
/// response before its body is read. Nothing is cloned.
///
/// The fields are those reqwest delivers: a body reqwest decompressed itself (its `gzip`/`brotli`/
/// `deflate`/`zstd` features) has already lost its `Content-Encoding` and `Content-Length`
/// fields, and its bytes arrive decompressed. `Content-Encoding` is not a media type.
///
/// `Debug` prints the status, the version and the header **names**: header values may be
/// credentials (`Set-Cookie`) and are reachable only through [`headers`](Self::headers).
#[derive(Clone, Copy)]
pub struct ResponseHead<'a> {
    status: StatusCode,
    version: Version,
    headers: &'a HeaderMap,
}

impl<'a> ResponseHead<'a> {
    /// A head made of its parts: for a response obtained another way, or a format's unit test.
    pub fn new(status: StatusCode, version: Version, headers: &'a HeaderMap) -> Self {
        Self {
            status,
            version,
            headers,
        }
    }

    /// The head of a native response, borrowed.
    pub fn of(response: &'a reqwest::Response) -> Self {
        Self::new(response.status(), response.version(), response.headers())
    }

    /// The status code (a 2xx when Concord calls a format).
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The HTTP version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The header fields, as received. Unredacted.
    pub fn headers(&self) -> &'a HeaderMap {
        self.headers
    }

    /// The one field line of `name`, if present ([`field::single`](crate::field::single) on this
    /// head's headers).
    ///
    /// # Errors
    ///
    /// [`FieldError::Repeated`].
    pub fn single(
        &self,
        name: impl reqwest::header::AsHeaderName,
    ) -> Result<Option<&'a HeaderValue>, FieldError> {
        crate::field::single(self.headers, name)
    }

    /// The field `name` as a non-negative decimal integer
    /// ([`field::unsigned`](crate::field::unsigned) on this head's headers).
    ///
    /// # Errors
    ///
    /// [`FieldError::Repeated`], [`FieldError::Invalid`] or [`FieldError::Overflow`].
    pub fn unsigned(
        &self,
        name: impl reqwest::header::AsHeaderName,
    ) -> Result<Option<u64>, FieldError> {
        crate::field::unsigned(self.headers, name)
    }

    /// The `Content-Type` field lines, without interpretation: which forms are acceptable is the
    /// format's policy, and media types are parsed by the format (this crate has no media-type
    /// parser).
    pub fn content_type(&self) -> ContentTypeField<'a> {
        let mut values = self.headers.get_all(CONTENT_TYPE).iter();
        match (values.next(), values.next()) {
            (None, _) => ContentTypeField::Missing,
            (Some(one), None) => ContentTypeField::One(one),
            (Some(_), Some(_)) => ContentTypeField::Repeated,
        }
    }
}

impl fmt::Debug for ResponseHead<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseHead")
            .field("status", &self.status.as_u16())
            .field("version", &self.version)
            .field(
                "header_names",
                &self.headers.keys().map(|k| k.as_str()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Why [`field::single`](crate::field::single) or [`field::unsigned`](crate::field::unsigned) (and
/// the same methods of [`ResponseHead`]) could not read a field. The field's value is never part
/// of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldError {
    /// The field has more than one line.
    Repeated,
    /// The value is empty, not text, or not of the expected form.
    Invalid,
    /// The number is larger than the type holds.
    Overflow,
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Repeated => "the header field is repeated",
            Self::Invalid => "the header field value is not of the expected form",
            Self::Overflow => "the header field's number is too large",
        })
    }
}

impl StdError for FieldError {}

/// The `Content-Type` field lines of a [`ResponseHead`].
#[derive(Clone, Copy, Debug)]
pub enum ContentTypeField<'a> {
    /// No `Content-Type` field.
    Missing,
    /// Exactly one field line (whose value may still be malformed or comma-joined).
    One(&'a HeaderValue),
    /// More than one field line, equal or not. A format that accepts some repetitions reads them
    /// through [`ResponseHead::headers`].
    Repeated,
}

/// How many numeric positions a [`CodecError`] keeps; later ones are ignored.
pub const CODEC_ERROR_CONTEXT: usize = 3;

/// A format could not encode a body, accept a response head, or decode a body.
///
/// It is built from **static** text: [`new`](Self::new) takes a `&'static str` label, so the
/// common accident of formatting the underlying error into the message
/// (`CodecError::new(e.to_string())`, `CodecError::new(format!(..))`) does not compile:
///
/// ```compile_fail,E0308
/// let e = std::io::Error::other("token=s3cr3t");
/// let _ = concord_core::CodecError::new(e.to_string()); // `String`, not `&'static str`
/// ```
///
/// Positions are numbers under static names ([`at`](Self::at)): `record`, `line`, `column`,
/// `byte`. The underlying error, which may quote the input, is attached with
/// [`with_source`](Self::with_source) and reachable only through
/// [`source_unredacted`](Self::source_unredacted): `Display` and `Debug` never format it and it is
/// not this type's [`std::error::Error::source`].
///
/// ```
/// use concord_core::CodecError;
/// let e = CodecError::new("an NDJSON record does not decode")
///     .at("record", 2)
///     .at("column", 7)
///     .with_source(std::io::Error::other("near \"token=s3cr3t\""));
/// assert_eq!(e.to_string(), "an NDJSON record does not decode (record 2, column 7)");
/// assert!(!format!("{e:?}").contains("s3cr3t"));
/// assert_eq!(e.get("record"), Some(2));
/// assert!(e.source_unredacted().unwrap().to_string().contains("s3cr3t"));
/// ```
///
/// Static text and numbers make the ordinary route safe; they are not a proof. An implementation
/// can still leak a string on purpose (`String::leak`), put a secret in a literal, or encode data
/// in numbers: format code is trusted code.
pub struct CodecError {
    label: &'static str,
    context: [(&'static str, u64); CODEC_ERROR_CONTEXT],
    len: u8,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl CodecError {
    /// An error described by a static, content-free label.
    pub const fn new(label: &'static str) -> Self {
        Self {
            label,
            context: [("", 0); CODEC_ERROR_CONTEXT],
            len: 0,
            source: None,
        }
    }

    /// Adds a numeric position under a static name (`.at("record", 3).at("column", 7)`). Up to
    /// [`CODEC_ERROR_CONTEXT`] entries are kept, in order.
    #[must_use]
    pub fn at(mut self, name: &'static str, value: u64) -> Self {
        if usize::from(self.len) < CODEC_ERROR_CONTEXT {
            self.context[usize::from(self.len)] = (name, value);
            self.len += 1;
        }
        self
    }

    /// Attaches the underlying error (which may quote the input).
    #[must_use]
    pub fn with_source(mut self, source: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// The static label.
    pub fn label(&self) -> &'static str {
        self.label
    }

    /// The numeric positions, in the order they were added.
    pub fn context(&self) -> &[(&'static str, u64)] {
        &self.context[..usize::from(self.len)]
    }

    /// One numeric position by name.
    pub fn get(&self, name: &str) -> Option<u64> {
        self.context()
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, v)| v)
    }

    /// The attached underlying error, **unredacted**: it may quote body content or header
    /// values (serde_json's messages quote the input). Read it deliberately; Concord never
    /// formats or chains it.
    pub fn source_unredacted(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label)?;
        for (i, (name, value)) in self.context().iter().enumerate() {
            f.write_str(if i == 0 { " (" } else { ", " })?;
            write!(f, "{name} {value}")?;
        }
        if self.len > 0 {
            f.write_str(")")?;
        }
        Ok(())
    }
}

impl fmt::Debug for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodecError")
            .field("label", &self.label)
            .field("context", &self.context())
            .field("source", &self.source.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

/// No `source()`: the underlying error may quote the input (see the type documentation).
impl StdError for CodecError {}
