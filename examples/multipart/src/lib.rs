//! U29 (R09): a `multipart/form-data` upload, two ways.
//!
//! # Route 1: reqwest's own `multipart` feature (native recipe, [`upload`])
//!
//! The endpoint is declared without a body: Concord owns the path, the authorization header and
//! the decoder. The application builds a [`reqwest::multipart::Form`] and hands the prepared
//! request back to reqwest:
//!
//! ```text
//! api.upload(folder).prepare()?.into_parts()          // (client, request, decoder)
//!   → RequestBuilder::from_parts(client, request)
//!       .multipart(form).send().await               // native body, boundary, Content-Type
//!   → decoder.finish(result).await                  // declared 2xx policy + `json` decoding
//! ```
//!
//! Why `from_parts(..).multipart(..)` rather than setting the body on the `Request`: reqwest's
//! builder writes the `Content-Type` with the per-body boundary *and* the `Content-Length` (when
//! every part has a known length) from the same `Form`, through public API only. Setting the body
//! by hand would need reqwest's `stream` feature (`Form::into_stream`) and would send a chunked
//! body, because the form's computed length is not public.
//!
//! # Route 2: a declared body with a runtime media type ([`Multipart`], `upload_form`)
//!
//! The boundary is chosen per body, so the media type (`multipart/form-data; boundary=...`) is
//! known only once the body is encoded. [`Multipart`] is a small local format whose
//! [`Encode::encode_body`] writes the whole form into memory, picks a boundary that occurs
//! nowhere in it, and returns the bytes and their media type together
//! ([`EncodedBody::with_content_type`]). The declaration then states the body it sends:
//!
//! ```text
//! POST upload_form(folder: String, body form: UploadForm) -> Stored {
//!     path "folders/{folder}/files";
//!     request Multipart;
//!     response Json;
//! }
//! api.upload_form(folder, form).send().await          // generated send(), nothing native
//! ```
//!
//! The body is bytes of a known length, so it is sent with `Content-Length` (not chunked), and the
//! `Content-Type` sent is exactly the boundary written into the body. A declared media type or a
//! `content-type` header that contradicts it is a `Build` error before any I/O (Concord's
//! [`EncodedBody`] precedence), so the boundary can never be separated from its bytes.
//!
//! # Which route fits
//!
//! | | native recipe ([`upload`]) | declared body ([`Multipart`]) |
//! |---|---|---|
//! | declaration | no body; the call site attaches one | `body form: UploadForm` with `request Multipart;` |
//! | call | `prepare()?.into_parts()`, reqwest's builder, `decoder.finish` | the generated `send()` / `prepare()` |
//! | part content | anything reqwest's `Part` takes, including streamed parts (`Part::stream_with_length`) | bytes held in memory (the whole form is buffered once) |
//! | encoding rules | reqwest's (names, file names, part headers) | this crate's ([`UploadForm`]), refusing what it cannot write unambiguously |
//! | dependencies | reqwest's `multipart` feature | none beyond Concord |
//!
//! Use the native recipe for large or streamed files, or when reqwest's `Form` is already the
//! application's currency. Use the declared body for forms of in-memory parts, when the request
//! should stay an ordinary generated operation (one `send()`, the body visible in the
//! declaration, the media type checked against the declaration before anything is sent).
//!
//! This crate is the only one in the workspace that enables reqwest's `multipart` feature; the
//! `concord` crate itself has no multipart policy and does not depend on it.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use concord::reqwest::header::HeaderValue;
use concord::{CodecError, Encode, EncodedBody, Json};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;

/// What the server answers after an upload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Stored {
    /// Identifier of the stored upload.
    pub id: u64,
    /// Number of files stored.
    pub files: u32,
}

concord::api! {
    /// A small file store. `key` is the full `Authorization` value (e.g. `Bearer ...`).
    pub client Files(#[sensitive] header("authorization") key: String) {
        base "http://files.invalid/v1";

        /// Upload into a folder; the multipart body is attached natively (see [`upload`]).
        POST upload(folder: String) -> Stored { response Json; path "folders/{folder}/files"; }

        /// Upload into a folder with a declared multipart body: the request format below
        /// produces the boundary and the `Content-Type` together.
        POST upload_form(folder: String, body form: UploadForm) -> Stored {
            path "folders/{folder}/files";
            request Multipart;
            response Json;
        }
    }
}

/// One file part of an upload.
#[derive(Debug, Clone)]
pub struct FileUpload {
    /// The form field name.
    pub field: String,
    /// The file name sent in `Content-Disposition`.
    pub file_name: String,
    /// The part's media type, e.g. `text/csv`.
    pub content_type: String,
    /// The file content (buffered; see [`Part::stream_with_length`] for streamed content).
    pub content: Vec<u8>,
}

/// Builds a form from file parts and plain text fields, in order (text fields first).
///
/// Field names go through [`Form::percent_encode_attr_chars`]: a name made only of RFC 8187
/// `attr-char`s (letters, digits, `` !#$&+-.^_`|~ ``) is sent as `name="..."`; any other name is
/// sent as `name*=utf-8''<percent-encoded>`, which a recipient decodes unambiguously. reqwest's
/// default (`percent_encode_path_segment`) leaves `\` unescaped inside `name="..."`, where a
/// recipient reads it as a quoted-pair escape (`file\name` arrives as `filename`).
///
/// File names are written by reqwest as a quoted string: `\` and `"` are backslash-escaped,
/// non-ASCII is sent as raw UTF-8, and CR/LF are sent as a backslash followed by the raw
/// character. That last form is never a line break on the wire, but RFC 9110 does not allow CR/LF
/// in a quoted string, so a strict recipient may reject it: sanitize such names first if the
/// server is not known to accept them.
///
/// Fails only for a content type that is not a valid media type.
pub fn file_form(text: &[(&str, &str)], files: Vec<FileUpload>) -> Result<Form, reqwest::Error> {
    let mut form = Form::new().percent_encode_attr_chars();
    for (name, value) in text {
        form = form.text(name.to_string(), value.to_string());
    }
    for file in files {
        let part = Part::bytes(file.content)
            .file_name(file.file_name)
            .mime_str(&file.content_type)?;
        form = form.part(file.field, part);
    }
    Ok(form)
}

/// Uploads `form` into `folder`: declared path, authorization and decoder; native multipart body.
///
/// A non-2xx answer is a [`concord::ErrorKind::Status`] error whose response is still unread
/// ([`concord::Error::into_response`]); a failure to send or to read the answer is a
/// [`concord::ErrorKind::Transport`] error. Both come from [`concord::Decoder::finish`], exactly
/// as for a declared body.
pub async fn upload(api: &Files, folder: &str, form: Form) -> Result<Stored, concord::Error> {
    let (client, request, decoder) = api.upload(folder).prepare()?.into_parts();
    let result = reqwest::RequestBuilder::from_parts(client, request)
        .multipart(form)
        .send()
        .await;
    decoder.finish(result).await
}

/// The parts of a declared multipart upload ([`Multipart`]), in the order they were added.
///
/// Encoding rules (RFC 7578), applied when the request is prepared:
///
/// - Field names: a name made only of RFC 8187 `attr-char`s is written `name="..."`; any other
///   name is written `name*=utf-8''<percent-encoded>` (the same choice as the native recipe).
/// - File names: written as a quoted string, `\` and `"` backslash-escaped, non-ASCII as raw
///   UTF-8. A file name containing a control character (CR, LF, ...) is **refused** (a `Build`
///   error): it cannot be written as an RFC 9110 quoted string.
/// - A file part's media type is written as its `Content-Type`; it must be a valid header value.
/// - Text values and file content are written verbatim.
#[derive(Debug, Clone, Default)]
pub struct UploadForm {
    parts: Vec<FormPart>,
}

#[derive(Debug, Clone)]
enum FormPart {
    Text { name: String, value: String },
    File(FileUpload),
}

impl UploadForm {
    /// An empty form.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a text field.
    #[must_use]
    pub fn text(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.parts.push(FormPart::Text {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// Adds a file part.
    #[must_use]
    pub fn file(mut self, file: FileUpload) -> Self {
        self.parts.push(FormPart::File(file));
        self
    }
}

/// `multipart/form-data` for a declared [`UploadForm`] body, with a boundary generated per body
/// (see the [crate documentation](crate) for when to prefer the native recipe).
///
/// [`encode_body`](Encode::encode_body) writes every part into one buffer, chooses a boundary
/// that occurs nowhere in that buffer, and returns the bytes (known length: sent with
/// `Content-Length`) bound to `multipart/form-data; boundary=<boundary>`. Its `CONTENT_TYPE` is
/// `None`: the media type exists only per body. [`encode`](Encode::encode) alone has nowhere to
/// put it and refuses.
pub struct Multipart(());

impl Encode<UploadForm> for Multipart {
    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(_: UploadForm) -> Result<concord::reqwest::Body, CodecError> {
        Err(CodecError::new(
            "a multipart body has a runtime media type; it is encoded with `encode_body`",
        ))
    }

    fn encode_body(form: UploadForm) -> Result<EncodedBody, CodecError> {
        let parts = form
            .parts
            .iter()
            .map(encode_part)
            .collect::<Result<Vec<_>, _>>()?;
        let boundary = choose_boundary(&parts, boundary_candidates());
        let media = HeaderValue::try_from(format!("multipart/form-data; boundary={boundary}"))
            .map_err(|_| CodecError::new("the multipart boundary is not a header value"))?;
        Ok(EncodedBody::with_content_type(
            assemble(&parts, &boundary).into(),
            media,
        ))
    }
}

/// One part: its header block (without the trailing empty line) and its content.
struct EncodedPart<'a> {
    head: String,
    content: &'a [u8],
}

fn encode_part(part: &FormPart) -> Result<EncodedPart<'_>, CodecError> {
    match part {
        FormPart::Text { name, value } => Ok(EncodedPart {
            head: format!("Content-Disposition: form-data; {}", name_param(name)),
            content: value.as_bytes(),
        }),
        FormPart::File(file) => {
            if file.file_name.chars().any(char::is_control) {
                return Err(CodecError::new(
                    "a multipart file name contains a control character",
                ));
            }
            if HeaderValue::from_str(&file.content_type).is_err() {
                return Err(CodecError::new(
                    "a multipart part media type is not a header value",
                ));
            }
            Ok(EncodedPart {
                head: format!(
                    "Content-Disposition: form-data; {}; filename=\"{}\"\r\nContent-Type: {}",
                    name_param(&file.field),
                    quote(&file.file_name),
                    file.content_type
                ),
                content: &file.content,
            })
        }
    }
}

/// `name="..."` for an `attr-char` name, else the RFC 8187 extended value.
fn name_param(name: &str) -> String {
    if !name.is_empty() && name.bytes().all(is_attr_char) {
        return format!("name=\"{name}\"");
    }
    let mut out = String::from("name*=utf-8''");
    for b in name.bytes() {
        if is_attr_char(b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// RFC 8187 `attr-char`.
fn is_attr_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&b)
}

/// The inside of an RFC 9110 quoted string (control characters were refused before).
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The first candidate that occurs in no part header or content. The generated candidates are
/// distinct and of one length, and a body of `n` bytes contains fewer than `n` strings of that
/// length, so at most that many are skipped.
fn choose_boundary(parts: &[EncodedPart<'_>], candidates: impl Iterator<Item = String>) -> String {
    let mut candidates = candidates;
    loop {
        let candidate = candidates.next().expect("the candidates are unbounded");
        let needle = candidate.as_bytes();
        let occurs = parts
            .iter()
            .any(|p| contains(p.head.as_bytes(), needle) || contains(p.content, needle));
        if !occurs {
            return candidate;
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Boundaries of 40 characters (RFC 2046 allows 70): `concord-`, a process-wide counter (so the
/// candidates are distinct) and 64 bits from the standard library's randomly keyed hasher over
/// that counter (so they are not predictable from outside the process).
fn boundary_candidates() -> impl Iterator<Item = String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let keys = RandomState::new();
    std::iter::repeat_with(move || {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut hasher = keys.build_hasher();
        hasher.write_u64(n);
        format!("concord-{n:016x}{:016x}", hasher.finish())
    })
}

/// `--boundary CRLF head CRLF CRLF content CRLF` per part, then `--boundary-- CRLF`.
fn assemble(parts: &[EncodedPart<'_>], boundary: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(format!("--{boundary}\r\n{}\r\n\r\n", part.head).as_bytes());
        out.extend_from_slice(part.content);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_candidate_found_in_the_body_is_skipped() {
        let content = b"before --concord-a after".to_vec();
        let form = UploadForm::new().file(FileUpload {
            field: "f".into(),
            file_name: "x.bin".into(),
            content_type: "application/octet-stream".into(),
            content,
        });
        let parts: Vec<_> = form.parts.iter().map(|p| encode_part(p).unwrap()).collect();
        let candidates = ["concord-a", "x.bin", "concord-b"]
            .map(String::from)
            .into_iter();
        // `concord-a` is in the content and `x.bin` in a part header: both are skipped.
        assert_eq!(choose_boundary(&parts, candidates), "concord-b");
    }

    #[test]
    fn generated_candidates_are_distinct_valid_boundaries() {
        let some: Vec<_> = boundary_candidates().take(64).collect();
        let distinct: std::collections::BTreeSet<_> = some.iter().collect();
        assert_eq!(distinct.len(), some.len());
        for b in &some {
            assert_eq!(b.len(), 40);
            assert!(b.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'));
        }
    }
}
