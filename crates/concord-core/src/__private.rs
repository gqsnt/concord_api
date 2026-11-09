//! Support code for `api!` expansions. Not a public API: names and signatures may change in any
//! release.

use std::fmt::{self, Display, Write as _};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, Url};

use crate::codec::{CodecError, Decode, Encode, EncodedBody, Incremental};
use crate::error::Error;
use crate::header::ToHeaderValue;
use crate::prepared::{Decoder, HeadStep, OpenHeadStep, Prepared, head_step, open_step};
use crate::response::Incoming;

pub use crate::next::{DefaultSend, followed, next_url};
pub use crate::value::IntoValue;

/// Static facts about one endpoint, produced by the macro.
#[derive(Debug)]
pub struct EndpointMeta {
    /// `Client.scope.endpoint`.
    pub name: &'static str,
    /// The HTTP method, as written.
    pub method: &'static str,
    /// Whether the endpoint only reads: its method is GET, HEAD or OPTIONS, or it (or a scope
    /// above it) is declared `#[read]`.
    pub read: bool,
    /// The base is a sensitive client value; withhold the complete URL in rendered output.
    pub sensitive_base: bool,
    /// Number of generated host labels (they are the leftmost labels).
    pub labels: usize,
    /// Indices (from the left) of generated labels that contain a sensitive value.
    pub sensitive_labels: &'static [usize],
    /// Number of generated path segments (they are the last segments; a trailing slash counts).
    pub segments: usize,
    /// Indices of generated segments that contain a sensitive value.
    pub sensitive_segments: &'static [usize],
    /// Query keys whose values are withheld from Concord output.
    pub sensitive_query: &'static [&'static str],
    /// Whether the `content-type` key is sensitive on this endpoint (marker or sticky
    /// composition): the body's media type inserted by Concord is then flagged like a declared
    /// `content-type` value would be.
    pub sensitive_content_type: bool,
    /// Whether the `accept` key is sensitive on this endpoint: a format's `Accept` suggestion
    /// inserted by Concord is then flagged like a declared value.
    pub sensitive_accept: bool,
    /// Whether the chain's last `accept` contribution is a `remove` (literal chains; a native
    /// chain decides it in [`HeaderPlan`]): the format's suggestion is then not sent.
    pub accept_removed: bool,
}

/// A value written as text: path segments, subdomain labels, query values and template holes
/// (`Display`). The trait only carries the diagnostic; every `Display` type implements it.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is written as text here and has no `Display`",
    label = "used in a path, subdomain, query value or template",
    note = "path segments, subdomain labels, query values and template holes are written with `Display`; a value that may be absent is declared `x?: T`, and a list is placed with `each`",
    note = "a native `HeaderValue` is not text: it can only be placed as a whole header value; to reuse its text elsewhere, convert it explicitly (for example `to_str()`) into a `String` value"
)]
pub trait Text: Display {}

impl<T: Display + ?Sized> Text for T {}

/// A template hole: checks [`Text`] (for its diagnostic) and returns the value unchanged.
#[inline]
pub fn text<T: Text + ?Sized>(value: &T) -> &T {
    value
}

/// The checked base URL; public as [`crate::BaseUrl`].
pub use crate::base::BaseUrl as Base;

/// Everything except RFC 3986 unreserved characters (`A-Z a-z 0-9 - . _ ~`).
const PATH_VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

struct EncodeInto<'a>(&'a mut String);

impl fmt::Write for EncodeInto<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0
            .extend(percent_encoding::utf8_percent_encode(s, PATH_VALUE));
        Ok(())
    }
}

/// The build error for a value whose `Display` implementation returned an error.
fn unformattable(meta: &'static EndpointMeta, what: &'static str) -> Error {
    Error::build(
        Some(meta),
        what,
        "could not be formatted: a `Display` implementation returned an error",
    )
}

/// A value's `Display` text, or the build error if its implementation fails.
fn display<T: Display + ?Sized>(
    value: &T,
    meta: &'static EndpointMeta,
    what: &'static str,
) -> Result<String, Error> {
    let mut out = String::new();
    write!(out, "{value}").map_err(|_| unformattable(meta, what))?;
    Ok(out)
}

/// Appends a value to a path segment, percent-encoding everything but unreserved characters.
pub fn encode_segment<T: Text + ?Sized>(
    out: &mut String,
    value: &T,
    meta: &'static EndpointMeta,
    what: &'static str,
) -> Result<(), Error> {
    write!(EncodeInto(out), "{value}").map_err(|_| unformattable(meta, what))
}

/// Appends a value's `Display` text.
pub fn display_into<T: Text + ?Sized>(
    out: &mut String,
    value: &T,
    meta: &'static EndpointMeta,
    what: &'static str,
) -> Result<(), Error> {
    write!(out, "{value}").map_err(|_| unformattable(meta, what))
}

/// An endpoint's body format and media type. The generated code holds one in a `static` per
/// endpoint with a body, so the media type is a compile-time-validated `HeaderValue` (the format's
/// `Encode::CONTENT_TYPE` or a declared media type, through `HeaderValue::from_static`: an invalid
/// one fails the static's evaluation, a compile error) and each request only clones it.
pub struct BodyFormat<T> {
    encode: fn(T) -> Result<EncodedBody, CodecError>,
    content_type: Option<HeaderValue>,
    /// Whether `content_type` is a declared media type (`request F "…";`) rather than the
    /// format's own `CONTENT_TYPE`: a runtime media type must then equal it.
    declared: bool,
}

impl<T> BodyFormat<T> {
    /// The format `E` with its own media type, `E::CONTENT_TYPE`.
    pub const fn new<E: Encode<T>>() -> Self {
        Self {
            encode: E::encode_body,
            content_type: match E::CONTENT_TYPE {
                Some(text) => Some(HeaderValue::from_static(text)),
                None => None,
            },
            declared: false,
        }
    }

    /// The format `E` with a declared media type (`request Json "…";` on the endpoint, a scope
    /// or the client). `E::CONTENT_TYPE` is not used.
    pub const fn with_media_type<E: Encode<T>>(media_type: HeaderValue) -> Self {
        Self {
            encode: E::encode_body,
            content_type: Some(media_type),
            declared: true,
        }
    }
}

/// An encoded body on its way into the request: the bytes and the media type the precedence
/// rules of [`EncodedBody`] pick from.
pub struct BodyParts {
    body: reqwest::Body,
    /// The format's `CONTENT_TYPE` or the declared media type.
    media: Option<HeaderValue>,
    /// Whether `media` was declared.
    declared: bool,
    /// The media type the encoder bound to these bytes.
    runtime: Option<HeaderValue>,
}

impl BodyParts {
    /// A body with a static (format or declared) media type and no runtime one.
    pub fn new(body: reqwest::Body, media: Option<HeaderValue>) -> Self {
        Self {
            body,
            media,
            declared: false,
            runtime: None,
        }
    }
}

/// Encodes a body with its declared format (through `Encode::encode_body`, polling nothing) and
/// returns it with its media types. A static media type was validated at compile time, a runtime
/// one is a `HeaderValue`; a failing encoder is an `Encode` error.
pub fn encode_body<T>(
    format: &'static BodyFormat<T>,
    value: T,
    meta: &'static EndpointMeta,
) -> Result<BodyParts, Error> {
    let (body, runtime) = (format.encode)(value)
        .map_err(|e| Error::from_encode(meta, e))?
        .into_parts();
    Ok(BodyParts {
        body,
        media: format.content_type.clone(),
        declared: format.declared,
        runtime,
    })
}

/// An endpoint's response format. The generated code holds one in a `static` per endpoint with a
/// typed output, so the format's `Accept` suggestion is a compile-time-validated `HeaderValue`
/// (through `HeaderValue::from_static`: an invalid one fails the static's evaluation).
pub struct DecodeFormat<T> {
    head: HeadStep<T>,
    accept: Option<HeaderValue>,
    /// `limit N;` on the endpoint or the nearest owner.
    limit: Option<usize>,
}

impl<T: 'static> DecodeFormat<T> {
    /// The format `F` for the output `T`.
    pub const fn new<F: Decode<T> + 'static>() -> Self {
        Self {
            head: head_step::<F, T>,
            accept: match F::ACCEPT {
                Some(text) => Some(HeaderValue::from_static(text)),
                None => None,
            },
            limit: None,
        }
    }

    /// The format with the declared finite body bound (`limit N;`).
    pub const fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

impl<T> DecodeFormat<T> {
    /// The endpoint's response handling.
    pub fn decoder(&self, meta: &'static EndpointMeta) -> Decoder<T> {
        let decoder = Decoder::typed(meta, self.head);
        match self.limit {
            Some(limit) => decoder.with_limit(limit),
            None => decoder,
        }
    }

    /// The format's `Accept` suggestion.
    pub fn accept(&self) -> Option<&HeaderValue> {
        self.accept.as_ref()
    }
}

/// A build error of an endpoint with a declared error document, as its `send()` returns it.
pub fn widen<D>(error: Error) -> Error<D> {
    error.widen()
}

/// An owner's declared error document (`error D in F;`): a `static` per endpoint, like
/// [`DecodeFormat`]. Its format never contributes an `Accept`.
pub struct ErrorFormat<D> {
    head: HeadStep<D>,
}

impl<D: 'static> ErrorFormat<D> {
    /// The format `F` for the error document `D`.
    pub const fn new<F: Decode<D> + 'static>() -> Self {
        Self {
            head: head_step::<F, D>,
        }
    }
}

impl<D> ErrorFormat<D> {
    /// The endpoint's response handling, with this error document.
    pub fn attach<T>(&self, decoder: Decoder<T>) -> Decoder<T, D> {
        decoder.with_error(self.head)
    }
}

/// An endpoint's incremental response format (`-> stream I`): a `static` per endpoint, like
/// [`DecodeFormat`]. `R` is the format's reader for `I`.
pub struct OpenFormat<R> {
    head: OpenHeadStep<R>,
    accept: Option<HeaderValue>,
}

impl<R: 'static> OpenFormat<R> {
    /// The incremental format `F` for items `I`.
    pub const fn new<I: 'static, F: Incremental<I, Records = R> + 'static>() -> Self {
        Self {
            head: open_step::<F, I>,
            accept: match F::ACCEPT {
                Some(text) => Some(HeaderValue::from_static(text)),
                None => None,
            },
        }
    }
}

impl<R> OpenFormat<R> {
    /// The endpoint's response handling.
    pub fn decoder(&self, meta: &'static EndpointMeta) -> Decoder<R> {
        Decoder::opened(meta, self.head)
    }

    /// The format's `Accept` suggestion.
    pub fn accept(&self) -> Option<&HeaderValue> {
        self.accept.as_ref()
    }
}

/// The identity of a declared endpoint, for its generated `Endpoint::operation()`.
pub fn declared(name: &'static str, method: &'static str, read: bool) -> crate::Operation {
    crate::Operation::declared(name, method, read)
}

/// The response handling of an endpoint declared `-> stream` without an item: any 2xx, the body
/// returned unread (a `response` default does not apply).
pub fn incoming(meta: &'static EndpointMeta) -> Decoder<Incoming> {
    Decoder::incoming_for(meta, |body| body)
}

/// The request being composed by one endpoint's generated code.
pub struct Parts<'a> {
    meta: &'static EndpointMeta,
    base: &'a Base,
    url: Url,
    labels: Vec<String>,
    segments: Vec<String>,
    trailing_slash: bool,
    query: Vec<(std::borrow::Cow<'static, str>, String)>,
    headers: HeaderMap,
    /// Set by [`HeaderPlan`] when a native key classified `content-type` as sensitive.
    content_type_sensitive: bool,
    /// Set by [`HeaderPlan`] when a native key classified `accept` as sensitive.
    accept_sensitive: bool,
    /// Set by [`HeaderPlan`] when a native chain's last `accept` contribution is a `remove`.
    accept_removed: bool,
}

impl<'a> Parts<'a> {
    /// Starts composing a request for `meta` against `base`.
    pub fn new(meta: &'static EndpointMeta, base: &'a Base) -> Self {
        Self {
            meta,
            base,
            url: base.url().clone(),
            labels: Vec::new(),
            segments: Vec::new(),
            trailing_slash: false,
            query: Vec::new(),
            headers: HeaderMap::new(),
            content_type_sensitive: false,
            accept_sensitive: false,
            accept_removed: false,
        }
    }

    /// Applies what a [`HeaderPlan`] found about `accept`: whether the format's suggestion would
    /// be sensitive, and whether the chain removed `accept` last (the suggestion is then not
    /// sent).
    pub fn accept(&mut self, sensitive: bool, removed: bool) {
        self.accept_sensitive |= sensitive;
        self.accept_removed |= removed;
    }

    /// Applies what a [`HeaderPlan`] found about `content-type`: the media type Concord inserts
    /// for the body is then flagged like a declared `content-type` value would be.
    pub fn content_type_sensitive(&mut self, sensitive: bool) {
        self.content_type_sensitive |= sensitive;
    }

    /// Adds the next host label (leftmost first). `what` names the template for errors.
    pub fn label(&mut self, mut label: String, what: &'static str) -> Result<(), Error> {
        label.make_ascii_lowercase();
        let bytes = label.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.len() <= 63
            && bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            && bytes[0] != b'-'
            && bytes[bytes.len() - 1] != b'-';
        if !valid {
            return Err(Error::build(
                Some(self.meta),
                what,
                "is not a DNS label (1-63 ASCII letters, digits or interior hyphens)",
            ));
        }
        self.labels.push(label);
        Ok(())
    }

    /// Adds a path segment containing values (already encoded).
    pub fn segment(&mut self, segment: String, what: &'static str) -> Result<(), Error> {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(Error::build(Some(self.meta), what, "is empty, `.` or `..`"));
        }
        self.segments.push(segment);
        Ok(())
    }

    /// Adds a literal path segment (validated by the macro).
    pub fn segment_static(&mut self, segment: &'static str) {
        self.segments.push(segment.to_owned());
    }

    /// Ends the path with `/`.
    pub fn trailing_slash(&mut self) {
        self.trailing_slash = true;
    }

    /// Adds one query occurrence. `what` names the parameter for errors.
    pub fn query<T: Text + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
        what: &'static str,
    ) -> Result<(), Error> {
        let text = display(value, self.meta, what)?;
        self.query.push((std::borrow::Cow::Borrowed(key), text));
        Ok(())
    }

    /// Adds the pairs a `query(..)` value writes, in its order. `what` names the parameter.
    pub fn query_pairs<P: crate::QueryPairs + ?Sized>(
        &mut self,
        value: &P,
        what: &'static str,
    ) -> Result<(), Error> {
        let mut writer = crate::value::QueryPairsWriter::new(&mut self.query);
        value.query_pairs(&mut writer);
        match writer.finish() {
            Ok(()) => Ok(()),
            Err(reason) => Err(Error::build(Some(self.meta), what, reason)),
        }
    }

    /// Adds one query occurrence per item, in iteration order.
    pub fn query_each<I>(
        &mut self,
        key: &'static str,
        values: I,
        what: &'static str,
    ) -> Result<(), Error>
    where
        I: IntoIterator,
        I::Item: Text,
    {
        for value in values {
            self.query(key, &value, what)?;
        }
        Ok(())
    }

    /// Adds one header line. The name is native: a `HeaderName` constant written in the
    /// declaration, or a literal name the macro checked with `http` and lowered to a validated
    /// static; `what` names the placement for errors.
    ///
    /// The outgoing sensitivity flag is the value's own flag OR the declaration's classification
    /// (`sensitive`): a flag already set on a native value is never cleared.
    pub fn header<T: ToHeaderValue + ?Sized>(
        &mut self,
        name: HeaderName,
        what: &'static str,
        value: &T,
        sensitive: bool,
    ) -> Result<(), Error> {
        let mut value = value.to_header_value().ok_or_else(|| {
            Error::build(
                Some(self.meta),
                what,
                "value is not a valid header value (rejected by `http::HeaderValue`)",
            )
        })?;
        if sensitive {
            value.set_sensitive(true);
        }
        self.headers
            .try_append(name, value)
            .map_err(|_| Error::build(Some(self.meta), what, "does not fit in the header map"))?;
        Ok(())
    }

    /// Adds one header line per item, in iteration order.
    pub fn header_each<I>(
        &mut self,
        name: HeaderName,
        what: &'static str,
        values: I,
        sensitive: bool,
    ) -> Result<(), Error>
    where
        I: IntoIterator,
        I::Item: ToHeaderValue,
    {
        for value in values {
            self.header(name.clone(), what, &value, sensitive)?;
        }
        Ok(())
    }

    /// Prepends the labels to the base host and validates the result. Called once, right after
    /// the last label, so host errors precede path errors.
    pub fn host(&mut self) -> Result<(), Error> {
        if !self.labels.is_empty() {
            let domain = match self.url.host() {
                Some(url::Host::Domain(domain)) => domain,
                _ => {
                    return Err(Error::build(
                        Some(self.meta),
                        "subdomain",
                        "requires a base URL with a domain name, not an IP address",
                    ));
                }
            };
            let host = format!("{}.{}", self.labels.join("."), domain);
            if host.len() > 253 {
                return Err(Error::build(
                    Some(self.meta),
                    "host",
                    "is longer than 253 bytes",
                ));
            }
            self.url
                .set_host(Some(&host))
                .map_err(|_| Error::build(Some(self.meta), "host", "is not a valid host name"))?;
        }
        Ok(())
    }

    /// Assembles the native request. `accept`: the format's `Accept` suggestion (`None` for an
    /// endpoint without typed output), sent only when no `accept` header was composed and the
    /// chain did not end by removing one. It is applied after the authored headers, so it never
    /// takes part in their composition (no conflict, no `remove` target).
    pub fn finish<T, D>(
        self,
        method: Method,
        client: &reqwest::Client,
        body: Option<BodyParts>,
        decoder: Decoder<T, D>,
        accept: Option<&HeaderValue>,
    ) -> Result<Prepared<T, D>, Error> {
        let mut url = self.url;
        if !self.segments.is_empty() || self.trailing_slash {
            let mut path = self.base.prefix.clone();
            for segment in &self.segments {
                path.push('/');
                path.push_str(segment);
            }
            if self.trailing_slash {
                path.push('/');
            }
            url.set_path(&path);
        }
        if !self.query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in &self.query {
                pairs.append_pair(key, value);
            }
        }
        let mut request = reqwest::Request::new(method, url);
        let mut headers = self.headers;
        // The body's media type is sent exactly when the body is, unless a `content-type` header
        // contribution was sent (it wins, body or not; an empty value counts as sent). The
        // inserted value carries the key's static classification, like a declared value would.
        // A media type the encoder bound to the bytes at run time is never separated from them:
        // a contradicting declared media type or sent header is a build error (`EncodedBody`).
        if let Some(BodyParts {
            body,
            media,
            declared,
            runtime,
        }) = body
        {
            let content_type = match runtime {
                None => media,
                Some(runtime) => {
                    if declared && media.as_ref() != Some(&runtime) {
                        return Err(Error::build(
                            Some(self.meta),
                            "content-type",
                            "of the body's encoder differs from the declared media type",
                        ));
                    }
                    let mut sent = headers.get_all(CONTENT_TYPE).iter();
                    match (sent.next(), sent.next()) {
                        (None, _) => {}
                        (Some(one), None) if *one == runtime => {}
                        _ => {
                            return Err(Error::build(
                                Some(self.meta),
                                "content-type",
                                "header contradicts the media type the body was encoded with",
                            ));
                        }
                    }
                    Some(runtime)
                }
            };
            if let Some(mut content_type) = content_type
                && !headers.contains_key(CONTENT_TYPE)
            {
                if self.meta.sensitive_content_type || self.content_type_sensitive {
                    content_type.set_sensitive(true);
                }
                headers
                    .try_insert(CONTENT_TYPE, content_type)
                    .map_err(|_| {
                        Error::build(
                            Some(self.meta),
                            "content-type",
                            "does not fit in the header map",
                        )
                    })?;
            }
            *request.body_mut() = Some(body);
        }
        if let Some(accept) = accept
            && !(self.meta.accept_removed || self.accept_removed)
            && !headers.contains_key(ACCEPT)
        {
            let mut accept = accept.clone();
            if self.meta.sensitive_accept || self.accept_sensitive {
                accept.set_sensitive(true);
            }
            headers.try_insert(ACCEPT, accept).map_err(|_| {
                Error::build(Some(self.meta), "accept", "does not fit in the header map")
            })?;
        }
        *request.headers_mut() = headers;
        Ok(Prepared::new(client.clone(), request, decoder))
    }
}

/// Renders a URL built by `meta`'s endpoint with sensitive placements replaced by `<redacted>`.
pub(crate) fn redacted_url(url: &Url, meta: &EndpointMeta) -> String {
    const REDACTED: &str = "<redacted>";
    if meta.sensitive_base {
        return REDACTED.to_owned();
    }
    let mut out = String::new();
    out.push_str(url.scheme());
    out.push_str("://");
    if let Some(host) = url.host_str() {
        if meta.sensitive_labels.is_empty() {
            out.push_str(host);
        } else {
            for (i, label) in host.split('.').enumerate() {
                if i > 0 {
                    out.push('.');
                }
                out.push_str(if i < meta.labels && meta.sensitive_labels.contains(&i) {
                    REDACTED
                } else {
                    label
                });
            }
        }
    }
    if let Some(port) = url.port() {
        let _ = write!(out, ":{port}");
    }
    let path = url.path();
    if meta.sensitive_segments.is_empty() {
        out.push_str(path);
    } else {
        let pieces: Vec<&str> = path.split('/').collect();
        // `pieces[0]` is the empty string before the leading slash.
        let first_generated = pieces.len().saturating_sub(meta.segments);
        for (i, piece) in pieces.iter().enumerate() {
            if i > 0 {
                out.push('/');
            }
            let generated = i.checked_sub(first_generated);
            let hide = generated.is_some_and(|g| meta.sensitive_segments.contains(&g));
            out.push_str(if hide { REDACTED } else { piece });
        }
    }
    if let Some(query) = url.query() {
        out.push('?');
        for (i, pair) in query.split('&').enumerate() {
            if i > 0 {
                out.push('&');
            }
            let raw_key = pair.split('=').next().unwrap_or("");
            let key = url::form_urlencoded::parse(raw_key.as_bytes())
                .next()
                .map(|(k, _)| k);
            // Declared sensitive keys, and any key on the well-known list: a `query(..)` value's
            // keys are only known now (literal keys on the list are already in
            // `sensitive_query`, so this adds no redaction to them).
            let sensitive = key.is_some_and(|k| {
                meta.sensitive_query.contains(&k.as_ref()) || sensitive_text_name(&k)
            });
            if sensitive {
                out.push_str(raw_key);
                out.push('=');
                out.push_str(REDACTED);
            } else {
                out.push_str(pair);
            }
        }
    }
    out
}

/// `Debug` through `Display`, for generated `Debug` impls.
pub struct DebugDisplay<'a, T: ?Sized>(pub &'a T);

impl<T: Text + ?Sized> fmt::Debug for DebugDisplay<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&Shown::of(self.0), f)
    }
}

/// A value's `Display` text for `Debug`; a failing `Display` is shown as `<Display failed>`
/// rather than propagated (propagating would make `format!` panic).
struct Shown(Option<String>);

impl Shown {
    fn of<T: Display + ?Sized>(value: &T) -> Self {
        let mut text = String::new();
        Self(write!(text, "{value}").ok().map(|()| text))
    }
}

impl fmt::Debug for Shown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(text) => fmt::Debug::fmt(text, f),
            None => f.write_str("<Display failed>"),
        }
    }
}

/// `Debug` of an `each` value: the `Display` text of every item.
pub struct DebugEach<'a, T: ?Sized>(pub &'a T);

impl<'a, T: ?Sized> fmt::Debug for DebugEach<'a, T>
where
    &'a T: IntoIterator,
    <&'a T as IntoIterator>::Item: Text,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.into_iter().map(|item| Shown::of(&item)))
            .finish()
    }
}

/// `Debug` of a value placed only as a header (no `Display` needed): its header value, or
/// `<redacted>` when that value carries the native sensitivity flag.
pub struct DebugHeader<'a, T: ?Sized>(pub &'a T);

impl<T: ToHeaderValue + ?Sized> fmt::Debug for DebugHeader<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&ShownHeader::of(self.0), f)
    }
}

/// `Debug` of an `each` value placed only as a header: every item's header value.
pub struct DebugHeaderEach<'a, T: ?Sized>(pub &'a T);

impl<'a, T: ?Sized> fmt::Debug for DebugHeaderEach<'a, T>
where
    &'a T: IntoIterator,
    <&'a T as IntoIterator>::Item: ToHeaderValue,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.into_iter().map(|item| ShownHeader::of(&item)))
            .finish()
    }
}

/// A value's header value for `Debug`: `<redacted>` when flagged sensitive, the text when it is
/// visible ASCII, the escaped bytes otherwise (`"caf\xe9"`), `<invalid header value>` when the
/// value has none.
struct ShownHeader(Option<HeaderValue>);

impl ShownHeader {
    fn of<T: ToHeaderValue + ?Sized>(value: &T) -> Self {
        Self(value.to_header_value())
    }
}

impl fmt::Debug for ShownHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            None => f.write_str("<invalid header value>"),
            Some(value) if value.is_sensitive() => f.write_str("<redacted>"),
            Some(value) => match value.to_str() {
                Ok(text) => fmt::Debug::fmt(text, f),
                Err(_) => fmt::Debug::fmt(value, f),
            },
        }
    }
}

/// `Debug` placeholder for sensitive values.
pub struct Redacted;

/// `Debug` of a value whose classification depends on native header names (known only at run
/// time): `<redacted>` when `self.0` holds, the value otherwise.
pub struct Withheld<T>(pub bool, pub T);

impl<T: fmt::Debug> fmt::Debug for Withheld<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 {
            f.write_str("<redacted>")
        } else {
            fmt::Debug::fmt(&self.1, f)
        }
    }
}

/// Header and query names whose values Concord withholds from its output even when they are not
/// marked `#[sensitive]`, compared after mapping `_` to `-` (the macro classifies literal names
/// with the same list; a test keeps the two equal).
pub const SENSITIVE_NAMES: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "api-key",
    "apikey",
    "x-auth-token",
    "auth-token",
    "access-token",
    "refresh-token",
    "id-token",
    "token",
    "client-secret",
    "secret",
    "password",
    "signature",
    "session",
    "api-token",
    "x-api-token",
    "x-access-token",
    "private-token",
    "x-goog-api-key",
    "ocp-apim-subscription-key",
    "x-amz-security-token",
];

/// The type a header key path must have: a native `HeaderName` (constant or static). Only the
/// diagnostic is Concord's; the name itself is used as it is.
///
/// Sealed: implemented for `HeaderName` only. The generated code resolves each key path once
/// and caches what it decides from the name (the per-value sensitivity conditions), which is
/// sound because a `HeaderName` constant or static cannot change. A downstream implementation
/// (a type whose `native_key` returns different names over time) would break that assumption, so
/// none is accepted.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a native header name",
    label = "a header key path names a `HeaderName` constant or static",
    note = "write a literal name, `header(\"x-name\")`, or define `const X_NAME: HeaderName = HeaderName::from_static(\"x-name\");`"
)]
pub trait NativeKey: sealed::Sealed {
    fn native_key(&self) -> &HeaderName;
}

mod sealed {
    /// Not nameable outside this crate, so [`NativeKey`](super::NativeKey) cannot be implemented
    /// there.
    pub trait Sealed {}

    impl Sealed for reqwest::header::HeaderName {}
}

impl NativeKey for HeaderName {
    #[inline]
    fn native_key(&self) -> &HeaderName {
        self
    }
}

/// Whether a query key is on [`SENSITIVE_NAMES`], with the macro's normalization (ASCII
/// lowercase, `_` as `-`).
fn sensitive_text_name(key: &str) -> bool {
    SENSITIVE_NAMES.iter().any(|listed| {
        listed.len() == key.len()
            && listed.bytes().zip(key.bytes()).all(|(l, k)| {
                l == match k.to_ascii_lowercase() {
                    b'_' => b'-',
                    other => other,
                }
            })
    })
}

/// Whether a native header name is on [`SENSITIVE_NAMES`]. Native names are lowercase.
pub fn sensitive_name(name: &HeaderName) -> bool {
    let name = name.as_str();
    SENSITIVE_NAMES.iter().any(|listed| {
        listed.len() == name.len()
            && listed
                .bytes()
                .zip(name.bytes())
                .all(|(l, n)| l == if n == b'_' { b'-' } else { n })
    })
}

/// Header fields owned by HTTP framing (reqwest/hyper set them); declaring one is an error (a
/// compile error for a literal name, a build error for a native constant).
pub const RESERVED_NAMES: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
];

/// Whether a native header name is on [`RESERVED_NAMES`].
pub fn reserved_name(name: &HeaderName) -> bool {
    RESERVED_NAMES.contains(&name.as_str())
}

/// A literal header value that is not visible ASCII (so not lowered to a `from_static` static):
/// built through the checked native constructor. The macro already checked it with `http`; the
/// downstream `http` stays authoritative.
pub fn header_literal(
    bytes: &'static [u8],
    meta: &'static EndpointMeta,
    what: &'static str,
) -> Result<HeaderValue, Error> {
    HeaderValue::from_bytes(bytes).map_err(|_| {
        Error::build(
            Some(meta),
            what,
            "literal is not a valid header value (rejected by `http::HeaderValue::from_bytes`)",
        )
    })
}

/// The written shape of one header contribution of an endpoint's chain: what it does and its
/// label. The generated code holds one `static` array of them per endpoint whose chain writes a
/// native `HeaderName` constant, shared by the endpoint's `prepare()` ([`HeaderPlan::compose`]) and
/// the client's `validate_header_names()` ([`NameCheck`]), so both apply the same rules to the same
/// declarations.
#[derive(Clone, Copy)]
pub struct HeaderShape {
    pub op: HeaderOp,
    /// The declaration, for errors (`` `override AUTHORIZATION` on `Api.admin` ``).
    pub label: &'static str,
}

/// What a header contribution does to the header it names.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HeaderOp {
    /// Introduces the header; an error if an earlier contribution (on this owner or an enclosing
    /// one) already sets it.
    Set,
    /// Adds a value after the inherited ones.
    Append,
    /// Replaces the inherited values; an error if nothing sets the header.
    Override,
    /// `override ... else inherit`: replaces them only when its source is present.
    OverrideElseInherit,
    /// Drops the inherited values; an error if nothing sets the header.
    Remove,
}

/// A structural rule a chain breaks: decided on resolved names and written operations alone.
#[derive(Clone, Copy)]
enum Violation {
    /// A header field that reqwest/hyper manage is declared (other than by `remove`).
    Reserved,
    /// The header is introduced while an earlier contribution (its label) already sets it.
    AlreadySet(&'static str),
    /// `override`, `else inherit` or `remove` with nothing to act on.
    NoTarget,
    /// The body declares a media type while a `content-type` contribution survives.
    MediaConflict,
}

impl Violation {
    fn reason(self) -> &'static str {
        match self {
            Violation::Reserved => {
                "names a header field that reqwest/hyper manage; it cannot be declared"
            }
            Violation::AlreadySet(_) => {
                "sets a header that an earlier declaration already sets (both name the same header); use `override` or `append`, or `remove` it first"
            }
            Violation::NoTarget => "changes a header that no enclosing owner sets",
            Violation::MediaConflict => {
                "sets `content-type` while the body declares a media type: both would set `Content-Type`; keep one"
            }
        }
    }
}

/// The structural rules of a chain (in composition order: root first, each owner's `remove`s
/// before its other contributions), on resolved names and written operations only. No value is
/// converted, formatted or read, and no presence is assumed: every rule holds or fails whatever
/// optional values turn out to be present, so the verdict is the same for every request of the
/// endpoint. Returns the first violation in chain order (the media-type rule, which
/// concerns the chain's last `content-type` contribution, last).
fn structure<const N: usize>(
    names: &[&HeaderName; N],
    shapes: &[HeaderShape; N],
    explicit_media: bool,
) -> Result<(), (usize, Violation)> {
    let mut last_content_type = None;
    for i in 0..N {
        let op = shapes[i].op;
        if op != HeaderOp::Remove && reserved_name(names[i]) {
            return Err((i, Violation::Reserved));
        }
        // Whether the header is set when this contribution applies: the latest earlier
        // contribution to the same header was not a `remove`. (An `else inherit` or an optional
        // value sets it as a declaration, present or not.)
        let earlier = (0..i).rev().find(|&j| names[j] == names[i]);
        let set_before = earlier.is_some_and(|j| shapes[j].op != HeaderOp::Remove);
        match (op, earlier) {
            (HeaderOp::Set, Some(j)) if set_before => {
                return Err((i, Violation::AlreadySet(shapes[j].label)));
            }
            (HeaderOp::Override | HeaderOp::OverrideElseInherit | HeaderOp::Remove, _)
                if !set_before =>
            {
                return Err((i, Violation::NoTarget));
            }
            _ => {}
        }
        if names[i] == CONTENT_TYPE {
            last_content_type = Some(i);
        }
    }
    // A `content-type` contribution survives exactly when the chain's last one is not a `remove`
    // (a later `else inherit`, present or absent, leaves a `content-type` declaration in force).
    match last_content_type {
        Some(i) if explicit_media && shapes[i].op != HeaderOp::Remove => {
            Err((i, Violation::MediaConflict))
        }
        _ => Ok(()),
    }
}

/// Header composition of one endpoint whose chain writes a native `HeaderName` constant, decided
/// on names alone: no value is converted, formatted or evaluated to find which header a
/// contribution names. Chains with literal names only are composed by the macro instead.
pub struct HeaderPlan<const N: usize> {
    /// Whether each contribution's value is sent (when present).
    pub live: [bool; N],
    /// Each contribution's outgoing classification: its header's name is on
    /// [`SENSITIVE_NAMES`], or some contribution to the same header is marked.
    pub sensitive: [bool; N],
    /// Whether some contribution to `content-type` is marked.
    pub content_type_sensitive: bool,
    /// Whether some contribution to `accept` is marked (the format's suggestion, if Concord
    /// inserts one, is then flagged sensitive).
    pub accept_sensitive: bool,
    /// Whether the last contribution to `accept` is a `remove` (the format's suggestion is then
    /// not sent).
    pub accept_removed: bool,
}

impl<const N: usize> HeaderPlan<N> {
    /// Composes the chain by native name equality: first the structural rules (shared with
    /// [`NameCheck`]), then which values are sent. `present`: for an `override ... else inherit`,
    /// whether its source is present (ignored otherwise); `marked`: each contribution's own
    /// classification (`#[sensitive]`, or a withheld value it reads). `explicit_media`: the body
    /// declares a media type, which conflicts with a surviving `content-type` header.
    pub fn compose(
        names: [&HeaderName; N],
        shapes: &[HeaderShape; N],
        present: [bool; N],
        marked: [bool; N],
        explicit_media: bool,
        meta: &'static EndpointMeta,
    ) -> Result<Self, Error> {
        if let Err((i, violation)) = structure(&names, shapes, explicit_media) {
            return Err(match violation {
                Violation::AlreadySet(earlier) => {
                    Error::header_conflict(meta, shapes[i].label, earlier)
                }
                other => Error::build(Some(meta), shapes[i].label, other.reason()),
            });
        }
        let mut live = [false; N];
        let mut sensitive = [false; N];
        let mut content_type_sensitive = false;
        let mut accept_sensitive = false;
        let mut accept_removed = false;
        for i in 0..N {
            let op = shapes[i].op;
            // A value survives unless a later contribution to the same header replaces or
            // removes it.
            let closes = |j: usize| match shapes[j].op {
                HeaderOp::Set | HeaderOp::Override | HeaderOp::Remove => true,
                HeaderOp::OverrideElseInherit => present[j],
                HeaderOp::Append => false,
            };
            live[i] =
                op != HeaderOp::Remove && !(i + 1..N).any(|j| names[j] == names[i] && closes(j));
            sensitive[i] =
                sensitive_name(names[i]) || (0..N).any(|j| names[j] == names[i] && marked[j]);
            if names[i] == ACCEPT {
                accept_sensitive |= marked[i];
                accept_removed = op == HeaderOp::Remove;
            }
            if names[i] == CONTENT_TYPE {
                content_type_sensitive |= marked[i];
            }
        }
        Ok(Self {
            live,
            sensitive,
            content_type_sensitive,
            accept_sensitive,
            accept_removed,
        })
    }
}

/// The checks of a client's `validate_header_names()`: the structural rules of
/// [`HeaderPlan::compose`] on each endpoint whose chain writes a native constant, collected
/// across endpoints. Nothing but the names is evaluated.
#[derive(Default)]
pub struct NameCheck {
    problems: Vec<crate::HeaderNameError>,
}

impl NameCheck {
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks one endpoint's chain; records its first violation (the one its `prepare()` reports
    /// when it composes the headers), if any.
    pub fn endpoint<const N: usize>(
        &mut self,
        endpoint: &'static str,
        names: [&HeaderName; N],
        shapes: &[HeaderShape; N],
        explicit_media: bool,
    ) {
        if let Err((i, violation)) = structure(&names, shapes, explicit_media) {
            self.problems.push(crate::HeaderNameError {
                endpoint,
                declaration: shapes[i].label,
                earlier: match violation {
                    Violation::AlreadySet(earlier) => Some(earlier),
                    _ => None,
                },
                reason: violation.reason(),
            });
        }
    }

    pub fn finish(self) -> Result<(), crate::HeaderNameErrors> {
        if self.problems.is_empty() {
            Ok(())
        } else {
            Err(crate::HeaderNameErrors {
                problems: self.problems,
            })
        }
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// The response handling of an endpoint without `->`: any 2xx, body drained unbuffered (a
/// `response` default does not apply).
pub fn drain(meta: &'static EndpointMeta) -> Decoder<()> {
    Decoder::drained(meta, || ())
}

/// Formats a query or header template; a failing `Display` is a build error naming the placement.
pub fn format_template(
    args: fmt::Arguments<'_>,
    meta: &'static EndpointMeta,
    what: &'static str,
) -> Result<String, Error> {
    let mut out = String::new();
    out.write_fmt(args).map_err(|_| unformattable(meta, what))?;
    Ok(out)
}
