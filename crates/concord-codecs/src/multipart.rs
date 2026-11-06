//! `multipart/form-data` (RFC 7578) and `multipart/related` (RFC 2387) request bodies (feature
//! `multipart`): [`MultipartBody`] holds the parts, [`Multipart`] is the format that frames them
//! and chooses the boundary.
//!
//! A declared body states what it sends; the boundary and the `Content-Type` that names it are
//! produced together when the request is prepared:
//!
//! ```ignore
//! use concord::multipart::{Multipart, MultipartBody};
//!
//! concord::api! {
//!     pub client Files {
//!         base "https://files.example.com/v1";
//!         POST upload(body form: MultipartBody) -> Stored {
//!             path "files";
//!             request Multipart;
//!             response Json;
//!         }
//!     }
//! }
//!
//! let form = MultipartBody::form_data()
//!     .text("purpose", "batch")
//!     .file("file", "input.jsonl", "application/jsonl", bytes);
//! let stored = api.upload(form).send().await?;
//! ```
//!
//! The parts are held in memory and the body is sent with `Content-Length`. For large or streamed
//! files, the native route (reqwest's own `multipart` feature through `Prepared::into_parts`,
//! shown in the repository's `examples/multipart`) remains available.

use std::fmt;

use concord_core::bytes::Bytes;
use concord_core::reqwest::header::HeaderValue;
use concord_core::{CodecError, Encode, EncodedBody, reqwest};

use crate::media;

/// The format of a [`MultipartBody`]: `multipart/form-data` or `multipart/related` with a
/// boundary chosen per body, sent as the body's runtime media type.
///
/// # Wire format
///
/// For every part, in the order it was added:
///
/// ```text
/// --<boundary> CRLF
/// <each header line: Name ": " value CRLF>
/// CRLF
/// <content, verbatim>
/// CRLF
/// ```
///
/// then `--<boundary>--` CRLF. There is no preamble and no epilogue. Header names and values are
/// written as given (no folding, no encoding) and content is written verbatim: no
/// `Content-Transfer-Encoding` is added (HTTP carries binary, RFC 7578 §4.7). A form-data body
/// without parts is the close delimiter alone (what browsers send for an empty form).
///
/// The form-data helpers write `Content-Disposition: form-data; name="<name>"` (and
/// `; filename="<filename>"` plus a `Content-Type` line for [`MultipartBody::file`]). Names and
/// file names are written between the quotes as they are, UTF-8 included (RFC 7578 §5.1). They are
/// never escaped: a name or file name containing `"` or `\` is refused, so a recipient that reads
/// an RFC 9110 quoted string and one that reads the WHATWG form encoding see the same value. The
/// RFC 8187 `filename*` form is not written (RFC 7578 §4.2 forbids it).
///
/// # The boundary
///
/// The boundary is `concord-boundary-<n>`: the smallest `n` (0, 1, 2, ... in decimal) for which it
/// occurs in no part, neither in the header lines (names and file names included) nor in the
/// content. It is chosen once every part is known, by one scan of each part.
///
/// **Collisions are checked, not avoided by randomness.** The candidates are deterministic: the
/// same parts always get the same boundary (`concord-boundary-0` unless a part contains it), so
/// tests can predict it. A part may contain any candidate, on purpose or not; the chosen boundary
/// still occurs in none of them, so no delimiter can appear inside a part (RFC 2046 §5.1.1). The
/// boundary consists of letters, digits and `-` (RFC 2046 `bchars`, and an RFC 9110 token, so it
/// is not quoted in the media type) and has at most 37 characters (RFC 2046 allows 70).
///
/// # Media type and length
///
/// [`encode_body`](Encode::encode_body) returns the bytes bound to
/// `multipart/form-data; boundary=<boundary>`, `multipart/related; boundary=<boundary>`, or, for
/// [`MultipartBody::related_with_type`], `multipart/related; boundary=<boundary>; type="<type>"`.
/// `CONTENT_TYPE` is `None`: the media type exists only per body. How the runtime media type meets
/// a declared media type (`request Multipart "m";`) or a `content-type` header is the table on
/// [`EncodedBody`]: anything but the same value byte for byte is a `Build` error before any I/O,
/// so declare neither. The body is assembled once in memory (the parts' bytes are copied into it)
/// and has a known length: it is sent with `Content-Length`, never chunked.
///
/// [`encode`](Encode::encode) alone has nowhere to put the media type and refuses, like
/// [`Encoded`](crate::Encoded): Concord itself calls `encode_body`.
///
/// # Refusals
///
/// Each is a [`CodecError`] from `encode_body`, so a `Build` error when the request is prepared,
/// before anything is sent. The label is static text; a refusal about one part carries its
/// 1-based position (`part`). Neither `Display` nor `Debug` of the error contains a name, a file
/// name, a header value or content.
///
/// - a `multipart/related` body without parts (it needs a root part);
/// - a `multipart/related` `type` that is not `type/subtype` (two RFC 9110 tokens, no
///   parameters);
/// - a form-data field ([`text`](MultipartBody::text), [`file`](MultipartBody::file)) in a
///   `multipart/related` body;
/// - a field name or a file name containing a control character (U+0000 to U+001F except tab,
///   and U+007F: CR, LF and NUL among them), `"` or `\`;
/// - a part header name that is not an RFC 9110 token (empty, with a space or a colon);
/// - a part header value (a file's content type included) containing a control character other
///   than tab.
///
/// Not checked: that a part added with [`MultipartBody::part`] to a form-data body has a
/// `Content-Disposition`, that field names are distinct, and that a content type is a media type
/// (it is checked as a header value).
///
/// # Example
///
/// ```
/// use concord_core::Encode;
/// use concord_codecs::multipart::{Multipart, MultipartBody, Part};
///
/// let form = MultipartBody::form_data()
///     .text("purpose", "batch")
///     .file("file", "input.jsonl", "application/jsonl", &b"{\"id\":1}\n"[..]);
/// let (body, media) = Multipart::encode_body(form)?.into_parts();
/// assert_eq!(media.unwrap(), "multipart/form-data; boundary=concord-boundary-0");
/// assert_eq!(
///     body.as_bytes().unwrap(),
///     b"--concord-boundary-0\r\n\
///       Content-Disposition: form-data; name=\"purpose\"\r\n\
///       \r\n\
///       batch\r\n\
///       --concord-boundary-0\r\n\
///       Content-Disposition: form-data; name=\"file\"; filename=\"input.jsonl\"\r\n\
///       Content-Type: application/jsonl\r\n\
///       \r\n\
///       {\"id\":1}\n\r\n\
///       --concord-boundary-0--\r\n"
/// );
///
/// // multipart/related (RFC 2387): JSON metadata as the root part, then the media. The media
/// // contains the first candidate, so the second one is chosen.
/// let related = MultipartBody::related()
///     .part(Part::new(r#"{"name":"a.bin"}"#).header("Content-Type", "application/json; charset=UTF-8"))
///     .part(Part::new(&b"--concord-boundary-0"[..]).header("Content-Type", "application/octet-stream"));
/// let (_, media) = Multipart::encode_body(related)?.into_parts();
/// assert_eq!(media.unwrap(), "multipart/related; boundary=concord-boundary-1");
/// # Ok::<(), concord_core::CodecError>(())
/// ```
pub struct Multipart(());

impl Encode<MultipartBody> for Multipart {
    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(_: MultipartBody) -> Result<reqwest::Body, CodecError> {
        Err(CodecError::new(
            "a multipart body has a runtime media type; it is encoded with `encode_body`",
        ))
    }

    fn encode_body(value: MultipartBody) -> Result<EncodedBody, CodecError> {
        value.encode()
    }
}

/// The parts of a multipart body, in the order they are added, and its subtype. Encoded by the
/// [`Multipart`] format, which documents the wire format and the refusals.
///
/// Building never fails: every rule is checked when the body is encoded, before anything is sent.
///
/// `Debug` shows the subtype and, per part, the number of header lines and of content bytes; never
/// a name, a file name, a header value or content (a field may hold a token or a signed URL).
#[derive(Clone)]
pub struct MultipartBody {
    subtype: Subtype,
    parts: Vec<Part>,
}

#[derive(Clone)]
enum Subtype {
    FormData,
    Related { root_type: Option<String> },
}

/// One part of a [`MultipartBody`]: content bytes and the header lines written before them.
///
/// `Debug` shows the number of header lines and of content bytes only.
#[derive(Clone)]
pub struct Part {
    field: Option<Field>,
    headers: Vec<(String, String)>,
    content: Bytes,
}

/// The `Content-Disposition: form-data` of a part made by a form-data helper.
#[derive(Clone)]
struct Field {
    name: String,
    file_name: Option<String>,
}

impl MultipartBody {
    /// An empty `multipart/form-data` body (RFC 7578).
    pub fn form_data() -> Self {
        Self {
            subtype: Subtype::FormData,
            parts: Vec::new(),
        }
    }

    /// An empty `multipart/related` body **without** the `type` parameter. The first part added
    /// is the root.
    ///
    /// RFC 2387 §3.1 requires `type` (the root part's media type); this form deviates on purpose,
    /// for providers whose documented request omits it (Google Drive's `uploadType=multipart`
    /// upload sends `multipart/related; boundary=...`). Use
    /// [`related_with_type`](Self::related_with_type) unless the provider documents the bare form.
    pub fn related() -> Self {
        Self {
            subtype: Subtype::Related { root_type: None },
            parts: Vec::new(),
        }
    }

    /// An empty `multipart/related` body whose media type carries `type="<root_type>"`, the media
    /// type of the root (first) part without parameters (`application/json`).
    pub fn related_with_type(root_type: impl Into<String>) -> Self {
        Self {
            subtype: Subtype::Related {
                root_type: Some(root_type.into()),
            },
            parts: Vec::new(),
        }
    }

    /// Adds a part with its own header lines.
    #[must_use]
    pub fn part(mut self, part: Part) -> Self {
        self.parts.push(part);
        self
    }

    /// Adds a form-data text field: `Content-Disposition: form-data; name="<name>"` and the value's
    /// UTF-8 bytes, without `Content-Type` (RFC 7578 §4.4: `text/plain`).
    #[must_use]
    pub fn text(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.part(Part {
            field: Some(Field {
                name: name.into(),
                file_name: None,
            }),
            headers: Vec::new(),
            content: Bytes::from(value.into()),
        })
    }

    /// Adds a form-data file:
    /// `Content-Disposition: form-data; name="<name>"; filename="<file_name>"`, then
    /// `Content-Type: <content_type>`, then the content.
    #[must_use]
    pub fn file(
        self,
        name: impl Into<String>,
        file_name: impl Into<String>,
        content_type: impl Into<String>,
        content: impl Into<Bytes>,
    ) -> Self {
        self.part(Part {
            field: Some(Field {
                name: name.into(),
                file_name: Some(file_name.into()),
            }),
            headers: vec![("Content-Type".to_owned(), content_type.into())],
            content: content.into(),
        })
    }

    fn encode(self) -> Result<EncodedBody, CodecError> {
        let (mut media, related) = match &self.subtype {
            Subtype::FormData => (String::from("multipart/form-data"), false),
            Subtype::Related { root_type } => {
                if self.parts.is_empty() {
                    return Err(CodecError::new("a multipart/related body has no part"));
                }
                if let Some(root) = root_type
                    && !root
                        .split_once('/')
                        .is_some_and(|(kind, sub)| media::is_token(kind) && media::is_token(sub))
                {
                    return Err(CodecError::new(
                        "the multipart/related type is not a media type without parameters",
                    ));
                }
                (String::from("multipart/related"), true)
            }
        };
        let mut heads = Vec::with_capacity(self.parts.len());
        for (index, part) in self.parts.iter().enumerate() {
            let head = part
                .head(related)
                .map_err(|e| e.at("part", index as u64 + 1))?;
            heads.push(head);
        }
        let segments: Vec<&[u8]> = heads
            .iter()
            .map(String::as_bytes)
            .chain(self.parts.iter().map(|p| &p.content[..]))
            .collect();
        let boundary = choose_boundary(&segments);
        media.push_str("; boundary=");
        media.push_str(&boundary);
        if let Subtype::Related {
            root_type: Some(root),
        } = &self.subtype
        {
            media.push_str("; type=\"");
            media.push_str(root);
            media.push('"');
        }
        let media = HeaderValue::try_from(media)
            .map_err(|_| CodecError::new("the multipart media type is not a header value"))?;

        let delimiter = boundary.len() + 4; // "--" boundary CRLF
        let size = heads
            .iter()
            .zip(&self.parts)
            .map(|(head, part)| delimiter + head.len() + 2 + part.content.len() + 2)
            .sum::<usize>()
            + delimiter
            + 2;
        let mut out = Vec::with_capacity(size);
        for (head, part) in heads.iter().zip(&self.parts) {
            out.extend_from_slice(b"--");
            out.extend_from_slice(boundary.as_bytes());
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(head.as_bytes());
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(&part.content);
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(b"--");
        out.extend_from_slice(boundary.as_bytes());
        out.extend_from_slice(b"--\r\n");
        debug_assert_eq!(out.len(), size);
        Ok(EncodedBody::with_content_type(out.into(), media))
    }
}

impl Part {
    /// A part with this content and no header lines yet (RFC 2046 then reads it as
    /// `text/plain; charset=us-ascii`: add a `Content-Type`).
    pub fn new(content: impl Into<Bytes>) -> Self {
        Self {
            field: None,
            headers: Vec::new(),
            content: content.into(),
        }
    }

    /// Adds a header line `name: value`, after the ones already added (`Content-Type`,
    /// `Content-ID`, a `Content-Disposition` of your own).
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// The header lines, each ended by CRLF, after checking every rule that concerns this part.
    fn head(&self, related: bool) -> Result<String, CodecError> {
        let mut head = String::new();
        if let Some(field) = &self.field {
            if related {
                return Err(CodecError::new(
                    "a form-data field is in a multipart/related body",
                ));
            }
            if !quotable(&field.name) {
                return Err(CodecError::new(
                    "a multipart field name contains a control character, a quote or a backslash",
                ));
            }
            head.push_str("Content-Disposition: form-data; name=\"");
            head.push_str(&field.name);
            head.push('"');
            if let Some(file_name) = &field.file_name {
                if !quotable(file_name) {
                    return Err(CodecError::new(
                        "a multipart file name contains a control character, a quote or a backslash",
                    ));
                }
                head.push_str("; filename=\"");
                head.push_str(file_name);
                head.push('"');
            }
            head.push_str("\r\n");
        }
        for (name, value) in &self.headers {
            if !media::is_token(name) {
                return Err(CodecError::new(
                    "a multipart part header name is not a token",
                ));
            }
            if value.bytes().any(is_control) {
                return Err(CodecError::new(
                    "a multipart part header value contains a control character",
                ));
            }
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        Ok(head)
    }
}

/// A control character other than tab: U+0000 to U+001F except U+0009, and U+007F.
fn is_control(b: u8) -> bool {
    (b < 0x20 && b != b'\t') || b == 0x7f
}

/// Whether `text` can be written between quotes as it is (no escaping, read alike everywhere).
fn quotable(text: &str) -> bool {
    !text
        .bytes()
        .any(|b| is_control(b) || b == b'"' || b == b'\\')
}

/// Every boundary this format writes starts with this prefix and continues with a decimal number.
const PREFIX: &[u8] = b"concord-boundary-";

/// The number of decimal digits of `u64::MAX`.
const MAX_DIGITS: usize = 20;

/// The first candidate `concord-boundary-<n>` found in no segment.
///
/// One scan per segment finds every occurrence of the prefix. The candidates such an occurrence
/// contains are the decimal prefixes of the digits that follow it (`concord-boundary-12` contains
/// candidates 1 and 12; `concord-boundary-07` only 0, since no candidate has a leading zero).
/// The first pass counts them; that count bounds the answer (it rules out at most that many
/// candidates), so the second pass records them in a bitmap of that size.
fn choose_boundary(segments: &[&[u8]]) -> String {
    let ruled_out: usize = segments
        .iter()
        .flat_map(|segment| after_prefix(segment))
        .map(|rest| contained(rest).count())
        .sum();
    let mut taken = vec![0_u64; ruled_out / 64 + 1];
    for rest in segments.iter().flat_map(|segment| after_prefix(segment)) {
        for n in contained(rest) {
            if n > ruled_out as u64 {
                break; // the digits only grow; beyond the bound, nothing is needed
            }
            taken[n as usize / 64] |= 1 << (n % 64);
        }
    }
    let n = (0..=ruled_out)
        .find(|&n| taken[n / 64] & (1 << (n % 64)) == 0)
        .unwrap_or(ruled_out);
    format!("concord-boundary-{n}")
}

/// The bytes after each occurrence of [`PREFIX`] in `segment`.
fn after_prefix(segment: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut from = 0;
    std::iter::from_fn(move || {
        while from + PREFIX.len() <= segment.len() {
            let at = from;
            from += 1;
            if segment[at..].starts_with(PREFIX) {
                return Some(&segment[at + PREFIX.len()..]);
            }
        }
        None
    })
}

/// The candidate numbers whose decimal form starts `rest`, in increasing order.
fn contained(rest: &[u8]) -> impl Iterator<Item = u64> + '_ {
    let take = if rest.first() == Some(&b'0') {
        1
    } else {
        MAX_DIGITS
    };
    rest.iter()
        .take_while(|b| b.is_ascii_digit())
        .scan(0_u64, |n, &digit| {
            *n = n.checked_mul(10)?.checked_add(u64::from(digit - b'0'))?;
            Some(*n)
        })
        .take(take)
}

impl fmt::Debug for MultipartBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let subtype = match self.subtype {
            Subtype::FormData => "form-data",
            Subtype::Related { .. } => "related",
        };
        f.debug_struct("MultipartBody")
            .field("subtype", &subtype)
            .field("parts", &self.parts)
            .finish()
    }
}

impl fmt::Debug for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Part")
            .field(
                "headers",
                &(self.headers.len() + usize::from(self.field.is_some())),
            )
            .field("bytes", &self.content.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundary(segments: &[&[u8]]) -> String {
        choose_boundary(segments)
    }

    #[test]
    fn the_first_free_candidate_is_chosen() {
        assert_eq!(boundary(&[]), "concord-boundary-0");
        assert_eq!(boundary(&[b"no prefix here"]), "concord-boundary-0");
        assert_eq!(boundary(&[b"concord-boundary-0"]), "concord-boundary-1");
        assert_eq!(
            boundary(&[b"concord-boundary-0", b"xconcord-boundary-1x"]),
            "concord-boundary-2"
        );
        // 12 contains 1 and 12; 0 is free.
        assert_eq!(boundary(&[b"concord-boundary-12"]), "concord-boundary-0");
        assert_eq!(
            boundary(&[b"concord-boundary-0 concord-boundary-12"]),
            "concord-boundary-2"
        );
        // A leading zero contains only candidate 0.
        assert_eq!(boundary(&[b"concord-boundary-01"]), "concord-boundary-1");
        // A prefix cut at the end of a segment contains nothing.
        assert_eq!(boundary(&[b"concord-boundary-"]), "concord-boundary-0");
        assert_eq!(boundary(&[b"concord-boundary"]), "concord-boundary-0");
        // Digits beyond u64 stop the scan without overflow.
        assert_eq!(
            boundary(&[
                b"concord-boundary-0",
                b"concord-boundary-99999999999999999999999"
            ]),
            "concord-boundary-1"
        );
    }

    #[test]
    fn overlapping_occurrences_are_all_found() {
        let segment = b"concord-boundary-concord-boundary-0";
        assert_eq!(after_prefix(segment).count(), 2);
        assert_eq!(boundary(&[segment]), "concord-boundary-1");
    }

    #[test]
    fn many_consecutive_candidates_are_skipped() {
        let mut segment = Vec::new();
        for n in 0..1000 {
            segment.extend_from_slice(format!("concord-boundary-{n}\r\n").as_bytes());
        }
        assert_eq!(boundary(&[&segment]), "concord-boundary-1000");
    }

    /// The chosen boundary against a brute-force oracle: it occurs in no segment, and every
    /// smaller candidate occurs in one.
    #[test]
    fn the_choice_matches_a_brute_force_oracle() {
        let pieces: [&[u8]; 8] = [
            b"concord-boundary-",
            b"0",
            b"1",
            b"2",
            b"10",
            b"x",
            b"\r\n--",
            b"concord-",
        ];
        let occurs = |segments: &[&[u8]], needle: &str| {
            segments
                .iter()
                .any(|s| s.windows(needle.len()).any(|w| w == needle.as_bytes()))
        };
        // A small deterministic generator (no dependency): every sequence of up to 4 pieces,
        // split into two segments at each position.
        let mut checked = 0;
        let mut stack: Vec<Vec<usize>> = vec![Vec::new()];
        while let Some(seq) = stack.pop() {
            let bytes: Vec<u8> = seq.iter().flat_map(|&i| pieces[i].to_vec()).collect();
            for split in 0..=bytes.len() {
                let segments: [&[u8]; 2] = [&bytes[..split], &bytes[split..]];
                let chosen = boundary(&segments);
                assert!(!occurs(&segments, &chosen), "{chosen} in {bytes:?}");
                let n: u64 = chosen["concord-boundary-".len()..].parse().unwrap();
                for smaller in 0..n {
                    assert!(
                        occurs(&segments, &format!("concord-boundary-{smaller}")),
                        "{chosen} skipped a free candidate in {bytes:?}"
                    );
                }
                checked += 1;
            }
            if seq.len() < 4 {
                for i in 0..pieces.len() {
                    let mut next = seq.clone();
                    next.push(i);
                    stack.push(next);
                }
            }
        }
        assert!(checked > 10_000, "{checked}");
    }
}
