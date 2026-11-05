//! Reader configuration: the response media policy and the JSON Lines options.

use concord_core::{CodecError, ResponseHead};

use crate::media;

/// Which response `Content-Type` a reader accepts when it [opens](super::JsonLinesReader::open) a
/// response. Checked before the body is polled; a refusal is an
/// [`OpenError`](crate::stream::OpenError) of kind
/// [`Metadata`](crate::stream::OpenErrorKind::Metadata) that keeps the response unread.
///
/// Except for [`any`](Self::any), the response needs exactly one well-formed `Content-Type` field
/// (not a media range) whose `charset`, if present, is `utf-8` or `us-ascii` and appears once:
/// the readers parse UTF-8 and transcode nothing. Media types compare case-insensitively, on
/// `type/subtype` (with any `+suffix`), ignoring parameters.
///
/// The entries of [`standard_or`](Self::standard_or) and [`only`](Self::only) are static
/// `type/subtype` strings, validated like `HeaderValue::from_static`: in a `const` they are
/// checked at compile time, otherwise construction panics on an entry that is not a media type.
///
/// ```
/// use concord_extras::jsonl::MediaPolicy;
/// // A provider that serves its JSON Lines downloads as `application/octet-stream`.
/// const DOWNLOADS: MediaPolicy = MediaPolicy::standard_or(&["application/octet-stream"]);
/// # let _ = DOWNLOADS;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaPolicy(Accept);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Accept {
    Standard,
    StandardOr(&'static [&'static str]),
    Only(&'static [&'static str]),
    Any,
}

impl MediaPolicy {
    /// The reader's documented media types (the default).
    pub const fn standard() -> Self {
        Self(Accept::Standard)
    }

    /// The documented media types and these ones too (`"application/octet-stream"`, say).
    ///
    /// # Panics
    ///
    /// If an entry is not a `type/subtype` media type made of RFC 9110 tokens (parameters and
    /// `*` are not allowed); at compile time in a `const`.
    pub const fn standard_or(extra: &'static [&'static str]) -> Self {
        assert_media_types(extra);
        Self(Accept::StandardOr(extra))
    }

    /// Exactly these media types, instead of the documented ones.
    ///
    /// # Panics
    ///
    /// If `types` is empty, or an entry is not a `type/subtype` media type (see
    /// [`standard_or`](Self::standard_or)).
    pub const fn only(types: &'static [&'static str]) -> Self {
        assert!(
            !types.is_empty(),
            "MediaPolicy::only needs at least one media type"
        );
        assert_media_types(types);
        Self(Accept::Only(types))
    }

    /// No `Content-Type` check at all: a missing, repeated, malformed or foreign field, or a
    /// foreign charset, is accepted. A deliberate choice for a provider whose `Content-Type`
    /// says nothing about the body; the body is still parsed as UTF-8 JSON.
    pub const fn any() -> Self {
        Self(Accept::Any)
    }
}

impl Default for MediaPolicy {
    fn default() -> Self {
        Self::standard()
    }
}

/// The documented media types of one reader, and the refusal label naming them.
pub(super) struct Family {
    pub(super) standard: fn(&mime::Mime) -> bool,
    pub(super) refused: &'static str,
}

/// Applies `policy` (with `family` as the documented types) to the response head.
pub(super) fn check_media(
    policy: MediaPolicy,
    family: &Family,
    head: &ResponseHead<'_>,
) -> Result<(), CodecError> {
    let (standard, extra): (bool, &[&str]) = match policy.0 {
        Accept::Any => return Ok(()),
        Accept::Standard => (true, &[]),
        Accept::StandardOr(extra) => (true, extra),
        Accept::Only(types) => (false, types),
    };
    let Some(found) = media::single(head)? else {
        return Err(CodecError::new("the response has no Content-Type"));
    };
    if found.essence.type_() == mime::STAR || found.essence.subtype() == mime::STAR {
        return Err(CodecError::new(
            "the response Content-Type is a media range, not a media type",
        ));
    }
    let essence = found.essence.essence_str();
    let accepted = (standard && (family.standard)(&found.essence))
        || extra.iter().any(|e| e.eq_ignore_ascii_case(essence));
    if !accepted {
        return Err(CodecError::new(if standard {
            family.refused
        } else {
            "the response Content-Type is not one the reader's media policy names"
        }));
    }
    media::check_charset(
        &found.charsets,
        "the response declares a charset other than utf-8",
    )
}

const fn assert_media_types(list: &[&str]) {
    let mut i = 0;
    while i < list.len() {
        assert!(
            is_media_type(list[i].as_bytes()),
            "a MediaPolicy entry must be a `type/subtype` media type, without parameters or `*`"
        );
        i += 1;
    }
}

/// `token "/" token`, without `*`.
const fn is_media_type(bytes: &[u8]) -> bool {
    let mut slash = 0;
    let mut slashes = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'/' {
            slash = i;
            slashes += 1;
        } else if b == b'*' || !is_tchar(b) {
            return false;
        }
        i += 1;
    }
    slashes == 1 && slash > 0 && slash + 1 < bytes.len()
}

const fn is_tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// What a JSON Lines reader does with a blank line (empty, or only spaces, tabs and CR).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BlankLines {
    /// A blank line is a [`Format`](crate::stream::ReadErrorKind::Format) error (JSON Lines: every
    /// line is a JSON value).
    Refuse,
    /// Blank lines are skipped (NDJSON leniency).
    Ignore,
}

/// What a JSON Lines reader does with a UTF-8 byte order mark (`EF BB BF`) at the very start of
/// the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BomPolicy {
    /// A terminal [`Format`](crate::stream::ReadErrorKind::Format) error (JSON Lines forbids it).
    Refuse,
    /// The mark is skipped; a mark anywhere else is not.
    Skip,
}

/// What a JSON Lines reader does with a complete line that is not a record: blank (when
/// refused), not UTF-8, not one JSON value, or not of the requested type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RecordErrors {
    /// The first bad line is a terminal error; the reader is then fused (the default).
    Stop,
    /// Each bad line ended by a line feed (LF or CRLF) is reported as a nonterminal `Err` item
    /// ([`ReadError::is_terminal`](crate::stream::ReadError::is_terminal) is `false`) and reading
    /// continues with the next line.
    ///
    /// A bad final line without a line feed, delimited only by the true end of input, is
    /// terminal: the input may have been cut off inside it, so it is not counted as a skipped
    /// record. (A valid final line without a line feed is still delivered.) Limit, byte order
    /// mark and transport failures stay terminal too.
    Yield,
}

/// The JSON Lines family a configuration starts from: it names the documented media types and
/// the writer's `Content-Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Flavor {
    JsonLines,
    Ndjson,
}

/// The default [`JsonLinesOptions::max_record_bytes`] and
/// [`JsonValuesReader::max_value_bytes`](super::JsonValuesReader::max_value_bytes): 16 MiB.
pub const DEFAULT_MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

/// The configuration of a [`JsonLinesReader`](super::JsonLinesReader) (and the media family of a
/// [`JsonLinesWriter`](super::JsonLinesWriter)). Start from a preset, then adjust:
///
/// | | [`json_lines()`](Self::json_lines) | [`ndjson()`](Self::ndjson) |
/// |---|---|---|
/// | documented media types | `application/jsonl`, `application/jsonlines`, `application/x-jsonlines` | `application/x-ndjson`, `application/ndjson` |
/// | writer `Content-Type` | `application/jsonl` | `application/x-ndjson` |
/// | blank lines | [`Refuse`](BlankLines::Refuse) | [`Ignore`](BlankLines::Ignore) |
/// | byte order mark | [`Refuse`](BomPolicy::Refuse) | [`Refuse`](BomPolicy::Refuse) |
/// | record errors | [`Stop`](RecordErrors::Stop) | [`Stop`](RecordErrors::Stop) |
/// | `max_record_bytes` | 16 MiB | 16 MiB |
/// | totals | none | none |
/// | media policy | [`standard`](MediaPolicy::standard) | [`standard`](MediaPolicy::standard) |
#[derive(Clone, Debug)]
pub struct JsonLinesOptions {
    pub(super) flavor: Flavor,
    pub(super) blank_lines: BlankLines,
    pub(super) bom: BomPolicy,
    pub(super) max_record_bytes: usize,
    pub(super) max_total_bytes: Option<u64>,
    pub(super) max_records: Option<u64>,
    pub(super) record_errors: RecordErrors,
    pub(super) media: MediaPolicy,
}

impl JsonLinesOptions {
    /// Strict JSON Lines (see the type documentation).
    pub fn json_lines() -> Self {
        Self {
            flavor: Flavor::JsonLines,
            blank_lines: BlankLines::Refuse,
            bom: BomPolicy::Refuse,
            max_record_bytes: DEFAULT_MAX_RECORD_BYTES,
            max_total_bytes: None,
            max_records: None,
            record_errors: RecordErrors::Stop,
            media: MediaPolicy::standard(),
        }
    }

    /// NDJSON: JSON Lines whose blank lines are ignored (see the type documentation).
    pub fn ndjson() -> Self {
        Self {
            flavor: Flavor::Ndjson,
            blank_lines: BlankLines::Ignore,
            ..Self::json_lines()
        }
    }

    /// Blank-line handling.
    #[must_use]
    pub fn blank_lines(mut self, blank_lines: BlankLines) -> Self {
        self.blank_lines = blank_lines;
        self
    }

    /// Byte order mark handling.
    #[must_use]
    pub fn bom(mut self, bom: BomPolicy) -> Self {
        self.bom = bom;
        self
    }

    /// The largest line, in bytes, excluding its LF or CRLF terminator. A longer line is a
    /// terminal [`Limit`](crate::stream::ReadErrorKind::Limit) error, detected before the reader's
    /// line buffer grows past this size (it holds at most this many bytes plus one pending CR).
    #[must_use]
    pub fn max_record_bytes(mut self, max: usize) -> Self {
        self.max_record_bytes = max;
        self
    }

    /// The most input bytes to read. A chunk that would cross it is a terminal
    /// [`Limit`](crate::stream::ReadErrorKind::Limit) error, never a clean end.
    #[must_use]
    pub fn max_total_bytes(mut self, max: u64) -> Self {
        self.max_total_bytes = Some(max);
        self
    }

    /// The most records to deliver. A further record is a terminal
    /// [`Limit`](crate::stream::ReadErrorKind::Limit) error, never a clean end; an input with
    /// exactly this many records ends cleanly.
    #[must_use]
    pub fn max_records(mut self, max: u64) -> Self {
        self.max_records = Some(max);
        self
    }

    /// Bad-line handling. With [`RecordErrors::Yield`] only bad lines ended by a line feed are
    /// skipped; a bad final line without one is terminal (see [`RecordErrors::Yield`]).
    #[must_use]
    pub fn record_errors(mut self, record_errors: RecordErrors) -> Self {
        self.record_errors = record_errors;
        self
    }

    /// The response media policy applied by [`open`](super::JsonLinesReader::open).
    #[must_use]
    pub fn media(mut self, media: MediaPolicy) -> Self {
        self.media = media;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_type_entries() {
        for ok in [
            "application/jsonl",
            "application/octet-stream",
            "application/vnd.x+json",
        ] {
            assert!(is_media_type(ok.as_bytes()), "{ok}");
        }
        for bad in [
            "",
            "application",
            "application/",
            "/jsonl",
            "application/jsonl; charset=utf-8",
            "application/*",
            "*/*",
            "a/b/c",
            "application/json l",
        ] {
            assert!(!is_media_type(bad.as_bytes()), "{bad}");
        }
    }

    #[test]
    #[should_panic(expected = "MediaPolicy entry")]
    fn invalid_entry_panics() {
        let _ = MediaPolicy::standard_or(&["application/jsonl; charset=utf-8"]);
    }

    #[test]
    #[should_panic(expected = "at least one")]
    fn empty_only_panics() {
        let _ = MediaPolicy::only(&[]);
    }
}
