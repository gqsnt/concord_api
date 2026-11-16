//! Buffered multipart/mixed and multipart/related request bodies.

use concord_core::{http::HeaderMap, Body};
use std::{error::Error, fmt};

/// Writes ordered, buffered parts as a `multipart/mixed` body.
///
/// Each part is `(HeaderMap, Body)`. The map supplies the complete MIME headers,
/// including exactly one valid `Content-Type`; body content-type metadata is
/// ignored. Repeated header values are preserved. No `Content-Disposition`,
/// name, content ID or embedded HTTP request is inferred. Parts retain their
/// input order; header names are emitted in lowercase, sorted by name, followed
/// by their values in insertion order.
/// There is no preamble or epilogue: each part consists of `--boundary\r\n`,
/// headers written as `name: value\r\n`, a blank CRLF line, the unchanged part
/// bytes and CRLF, followed after the last part by `--boundary--\r\n`;
/// the content type's `boundary` parameter is unquoted.
///
/// The boundary is the first candidate in the deterministic sequence
/// `concord-00000000000000000000000000000000`,
/// `concord-00000000000000000000000000000001`, and so on, absent from every
/// part and header. No randomness or clock is used. Header values are written
/// as their bytes. All parts must be buffered; a stream is refused without
/// claiming or polling any source. Memory holds the parts, emitted headers
/// and output, proportional to the author's input. No task or request is started.
///
/// The result is repeatable and carries the multipart content type and boundary.
/// The function does not implement a batch protocol, inspect subresponses, choose
/// retry safety or send anything.
///
/// ```
/// use concord::{multipart_mixed, Body};
/// use concord::http::{HeaderMap, HeaderValue};
/// use concord_testkit::{script::Request, Script};
///
/// fn batch(document: &Body) -> Result<Body, concord::MimeWriteError> {
///     let mut headers = HeaderMap::new();
///     headers.insert("content-type", HeaderValue::from_static("application/json"));
///     multipart_mixed([(headers, document.clone())])
/// }
/// concord::api! {
///     client Batch {
///         base "https://batch.example.test";
///         POST submit(document: Body) { body batch(document); }
///     }
/// }
/// #[tokio::main(flavor = "current_thread", start_paused = true)]
/// async fn main() -> Result<(), concord::Error> {
///     let script = Script::new();
///     script.answer(204);
///     Batch::new().with_transport(script.clone())
///         .submit(Body::from(r#"{"operation":"check"}"#)).await?;
///     let requests: Vec<Request> = script.requests();
///     assert_eq!(requests.len(), 1);
///     assert_eq!(requests[0].info().number(), 1);
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns [`MimeWriteError`] for no parts, a stream part or an invalid/missing
/// part content type. Part errors carry their one-based position.
/// Parts are validated in input order, so the first invalid part determines
/// the error.
pub fn multipart_mixed<I>(parts: I) -> Result<Body, MimeWriteError>
where
    I: IntoIterator<Item = (HeaderMap, Body)>,
{
    write_parts(parts, false)
}

/// Writes ordered, buffered parts as a `multipart/related` body, with the first as root.
///
/// Part and header ordering, boundary selection and stream rejection follow
/// [`multipart_mixed`].
/// As with [`multipart_mixed`], there is no preamble or epilogue, headers use
/// `name: value\r\n`, the closing delimiter is `--boundary--\r\n`, and the
/// `boundary` parameter is unquoted.
/// The enclosing content type also has a quoted `type` parameter equal to the
/// first part's media type, without its parameters.
/// A valid part media type has token type and subtype separated by `/`, followed
/// only by syntactically valid parameters. Part header values are sent as given.
/// There is no `start` parameter: the first part is always the root. An author
/// may supply `Content-ID` headers and references in the root bytes; the writer
/// never makes IDs, rewrites references or validates their application meaning.
/// To choose another root or use a provider's multipart convention, supply a
/// custom writer.
///
/// ```no_run
/// use concord::{multipart_related, Body};
/// use concord::http::{HeaderMap, HeaderValue};
///
/// fn message(root: &Body, attachment: &Body) -> Result<Body, concord::MimeWriteError> {
///     let mut json = HeaderMap::new();
///     json.insert("content-type", HeaderValue::from_static("application/json"));
///     let mut image = HeaderMap::new();
///     image.insert("content-type", HeaderValue::from_static("image/png"));
///     image.insert("content-id", HeaderValue::from_static("<image-1>"));
///     multipart_related([(json, root.clone()), (image, attachment.clone())])
/// }
/// concord::api! {
///     client Messages {
///         base "https://messages.example.test";
///         POST send(root: Body, attachment: Body) { body message(root, attachment); }
///     }
/// }
/// async fn send() -> Result<(), concord::Error> {
///     Messages::new().send(
///         Body::from(r#"{"image":"cid:image-1"}"#), Body::from(vec![137, 80, 78, 71]),
///     ).await?;
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns the same errors as [`multipart_mixed`]. No part source is claimed,
/// including when a later part fails validation.
/// Parts are validated in input order, so the first invalid part determines
/// the error.
pub fn multipart_related<I>(parts: I) -> Result<Body, MimeWriteError>
where
    I: IntoIterator<Item = (HeaderMap, Body)>,
{
    write_parts(parts, true)
}

/// A buffered MIME multipart request body's validation failure.
///
/// Formatting prints only names and one-based part positions, never headers or
/// part bytes. These semantic errors have no lower cause.
///
/// ```no_run
/// fn inspect(error: &concord::Error) {
///     if let Some(concord::MimeWriteError::Stream { part, .. }) =
///         concord::MimeWriteError::find(error)
///     {
///         let failing_part = *part;
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub enum MimeWriteError {
    /// The iterator contained no parts.
    Empty,
    /// A part is a stream. No source was claimed or polled.
    #[non_exhaustive]
    Stream {
        /// The part's one-based position.
        part: usize,
    },
    /// A part lacks exactly one syntactically valid Content-Type header.
    #[non_exhaustive]
    ContentType {
        /// The part's one-based position.
        part: usize,
    },
}
impl MimeWriteError {
    /// Finds this error by following the given error and its source chain.
    ///
    /// ```no_run
    /// fn inspect(error: &concord::Error) {
    ///     let _cause = concord::MimeWriteError::find(error);
    /// }
    /// ```
    pub fn find<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a Self> {
        super::find_error(error)
    }
}
impl fmt::Display for MimeWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the MIME body has no parts"),
            Self::Stream { part } => write!(f, "MIME part {part} must be buffered"),
            Self::ContentType { part } => {
                write!(f, "MIME part {part} needs one valid content type")
            }
        }
    }
}
impl Error for MimeWriteError {}

struct Part {
    headers: Vec<u8>,
    body: Body,
}
fn write_parts<I>(parts: I, related: bool) -> Result<Body, MimeWriteError>
where
    I: IntoIterator<Item = (HeaderMap, Body)>,
{
    let mut prepared = Vec::new();
    let mut root_type = None;
    for (index, (headers, body)) in parts.into_iter().enumerate() {
        let part = index.saturating_add(1);
        if body.as_bytes().is_none() {
            return Err(MimeWriteError::Stream { part });
        }
        let mut types = headers.get_all("content-type").iter();
        let media_type = types
            .next()
            .and_then(|value| media_type(value.as_bytes()))
            .filter(|_| types.next().is_none())
            .ok_or(MimeWriteError::ContentType { part })?;
        if root_type.is_none() {
            root_type = Some(media_type);
        }
        let mut keys: Vec<_> = headers.keys().collect();
        keys.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
        let mut encoded = Vec::new();
        for key in keys {
            for value in headers.get_all(key) {
                encoded.extend_from_slice(key.as_str().as_bytes());
                encoded.extend_from_slice(b": ");
                encoded.extend_from_slice(value.as_bytes());
                encoded.extend_from_slice(b"\r\n");
            }
        }
        prepared.push(Part {
            headers: encoded,
            body,
        });
    }
    let root_type = root_type.ok_or(MimeWriteError::Empty)?;
    let mut index = 0u128;
    let boundary = loop {
        let candidate = format!("concord-{index:032x}");
        if prepared.iter().all(|part| {
            !contains(&part.headers, candidate.as_bytes())
                && part
                    .body
                    .as_bytes()
                    .is_some_and(|bytes| !contains(bytes, candidate.as_bytes()))
        }) {
            break candidate;
        }
        // PANIC-SAFETY: Each rejected candidate needs a distinct stored substring; addressable input has fewer than u128::MAX such substrings.
        index += 1;
    };
    let mut bytes = Vec::new();
    for part in prepared {
        bytes.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        bytes.extend_from_slice(&part.headers);
        bytes.extend_from_slice(b"\r\n");
        // No body has been changed since buffered validation above.
        if let Some(payload) = part.body.as_bytes() {
            bytes.extend_from_slice(payload);
        }
        bytes.extend_from_slice(b"\r\n");
    }
    bytes.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let content_type = if related {
        format!("multipart/related; type=\"{root_type}\"; boundary={boundary}")
    } else {
        format!("multipart/mixed; boundary={boundary}")
    };
    Ok(Body::new(bytes, content_type))
}
fn contains(bytes: &[u8], boundary: &[u8]) -> bool {
    bytes
        // PANIC-SAFETY: The generated boundary is always 40 bytes, never empty.
        .windows(boundary.len())
        .any(|window| window == boundary)
}
fn token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}
fn take_token(input: &mut &[u8]) -> bool {
    let length = input.iter().take_while(|byte| token(**byte)).count();
    *input = input.get(length..).unwrap_or_default();
    length != 0
}
fn space(input: &mut &[u8]) {
    let length = input
        .iter()
        .take_while(|&&byte| byte == b' ' || byte == b'\t')
        .count();
    *input = input.get(length..).unwrap_or_default();
}
fn eat(input: &mut &[u8], expected: u8) -> bool {
    match input.split_first() {
        Some((&byte, rest)) if byte == expected => {
            *input = rest;
            true
        }
        _ => false,
    }
}
fn media_type(bytes: &[u8]) -> Option<String> {
    let mut input = bytes;
    space(&mut input);
    let start = input;
    if !take_token(&mut input) || !eat(&mut input, b'/') || !take_token(&mut input) {
        return None;
    }
    let length = start.len().saturating_sub(input.len());
    let media = std::str::from_utf8(start.get(..length)?).ok()?.to_owned();
    loop {
        space(&mut input);
        if input.is_empty() {
            return Some(media);
        }
        if !eat(&mut input, b';') {
            return None;
        }
        space(&mut input);
        if !take_token(&mut input) {
            return None;
        }
        space(&mut input);
        if !eat(&mut input, b'=') {
            return None;
        }
        space(&mut input);
        if eat(&mut input, b'"') {
            loop {
                let (&byte, rest) = input.split_first()?;
                input = rest;
                match byte {
                    b'"' => break,
                    b'\\' => {
                        let (&escaped, rest) = input.split_first()?;
                        if escaped < b' ' && escaped != b'\t' {
                            return None;
                        }
                        if escaped == 127 {
                            return None;
                        }
                        input = rest;
                    }
                    byte if byte == b'\t' || (byte >= b' ' && byte != 127) => (),
                    _ => return None,
                }
            }
        } else if !take_token(&mut input) {
            return None;
        }
    }
}
