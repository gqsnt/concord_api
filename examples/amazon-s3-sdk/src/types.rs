//! Validated S3 values: bucket names, object keys, entity tags, ranges, dates, metadata.
//!
//! Every constructor checks the value against the rule this SDK relies on and returns an
//! [`InputError`] naming the rule, never the value. A value that passed is written on the wire by
//! its `Display` (which Concord then encodes for its placement).

use std::collections::BTreeMap;
use std::fmt;
use std::time::SystemTime;

use crate::error::InputError;

/// A general-purpose bucket name usable with virtual-hosted-style addressing
/// (`https://{bucket}.s3.{region}.amazonaws.com`).
///
/// Checked: 3 to 63 characters; lowercase ASCII letters, digits and hyphens; begins and ends with
/// a letter or a digit; not prefixed `xn--`; not a directory bucket (`--x-s3` suffix, out of this
/// SDK's scope). Names with dots are valid S3 names but are refused here: AWS documents that the
/// virtual-hosted wildcard TLS certificate does not match them, and this SDK does not implement
/// path-style addressing.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BucketName(String);

impl BucketName {
    /// Validates a bucket name.
    ///
    /// # Errors
    ///
    /// [`InputError`] naming the violated rule.
    pub fn new(name: impl Into<String>) -> Result<Self, InputError> {
        let name = name.into();
        let bytes = name.as_bytes();
        if !(3..=63).contains(&bytes.len()) {
            return Err(InputError::new(
                "bucket name",
                "must be 3 to 63 characters long",
            ));
        }
        if bytes.contains(&b'.') {
            return Err(InputError::new(
                "bucket name",
                "contains a dot: virtual-hosted-style HTTPS addressing (the only addressing this SDK implements) cannot reach it",
            ));
        }
        if !bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        {
            return Err(InputError::new(
                "bucket name",
                "may contain only lowercase ASCII letters, digits and hyphens",
            ));
        }
        let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        if !alnum(bytes[0]) || !alnum(bytes[bytes.len() - 1]) {
            return Err(InputError::new(
                "bucket name",
                "must begin and end with a letter or a digit",
            ));
        }
        if name.starts_with("xn--") {
            return Err(InputError::new("bucket name", "must not start with `xn--`"));
        }
        if name.ends_with("--x-s3") {
            return Err(InputError::new(
                "bucket name",
                "is a directory bucket name (`--x-s3`), which this SDK does not support",
            ));
        }
        Ok(Self(name))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BucketName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for BucketName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// An object key: any UTF-8 text of 1 to 1,024 bytes, without a period-only path segment.
///
/// `/` is part of the key and is sent unencoded, as AWS's SigV4 rules for S3 require
/// (`photos/2006/x.jpg` is the request path `/photos/2006/x.jpg`); every other byte outside
/// `A-Z a-z 0-9 - . _ ~` is percent-encoded once. Keys with a `.` or `..` segment
/// (`a/../b`) are refused: AWS recommends against them, and URL parsers (reqwest's included)
/// remove such segments from a request path, which would address another object.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectKey(String);

impl ObjectKey {
    /// Validates an object key.
    ///
    /// # Errors
    ///
    /// [`InputError`] naming the violated rule.
    pub fn new(key: impl Into<String>) -> Result<Self, InputError> {
        let key = key.into();
        if key.is_empty() || key.len() > 1024 {
            return Err(InputError::new(
                "object key",
                "must be 1 to 1024 bytes of UTF-8",
            ));
        }
        if key
            .split('/')
            .any(|segment| segment == "." || segment == "..")
        {
            return Err(InputError::new(
                "object key",
                "contains a period-only path segment (`.` or `..`), which a URL parser would remove",
            ));
        }
        Ok(Self(key))
    }

    /// The key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// An entity tag exactly as S3 sent it, quotes included (`"fba9dede5f27731c9771645a39863328"`).
///
/// It is an **opaque validator**: compare it for equality, or send it back in `If-Match` /
/// `If-None-Match`. It is not a content checksum — S3 documents that it is the MD5 of the data
/// only for some objects (not for multipart uploads, SSE-KMS or directory buckets) — so this
/// type offers no digest accessor.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ETag(String);

impl ETag {
    /// Parses an entity tag (RFC 9110 `entity-tag`: an optional `W/`, then a double-quoted
    /// opaque value without `"`, control characters or spaces).
    ///
    /// # Errors
    ///
    /// [`InputError`] when the text is not an entity tag.
    pub fn parse(raw: impl Into<String>) -> Result<Self, InputError> {
        let raw = raw.into();
        let opaque = raw.strip_prefix("W/").unwrap_or(&raw);
        let valid = opaque.len() >= 2
            && opaque.starts_with('"')
            && opaque.ends_with('"')
            && opaque.as_bytes()[1..opaque.len() - 1]
                .iter()
                .all(|&b| b == 0x21 || (0x23..=0x7e).contains(&b) || b >= 0x80);
        if valid {
            Ok(Self(raw))
        } else {
            Err(InputError::new(
                "entity tag",
                "is not a double-quoted entity tag (RFC 9110)",
            ))
        }
    }

    /// The tag as sent by S3, quotes included.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is a weak tag (`W/"..."`). S3 sends strong tags.
    pub fn is_weak(&self) -> bool {
        self.0.starts_with("W/")
    }
}

impl fmt::Display for ETag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ETag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ETag({})", self.0)
    }
}

/// The opaque `NextContinuationToken` of a truncated `ListObjectsV2` page, sent back unchanged as
/// `continuation-token`. AWS documents it as obfuscated and "not a real key": the SDK never
/// interprets it. `Debug` shows only its length.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ContinuationToken(String);

impl ContinuationToken {
    /// Wraps a token received from S3 (or stored from an earlier traversal).
    ///
    /// # Errors
    ///
    /// [`InputError`] for an empty token.
    pub fn new(token: impl Into<String>) -> Result<Self, InputError> {
        let token = token.into();
        if token.is_empty() {
            return Err(InputError::new("continuation token", "must not be empty"));
        }
        Ok(Self(token))
    }

    /// The token, verbatim.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContinuationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ContinuationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContinuationToken(<{} bytes>)", self.0.len())
    }
}

/// An object version id (`x-amz-version-id`), opaque.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct VersionId(String);

impl VersionId {
    /// Wraps a version id.
    ///
    /// # Errors
    ///
    /// [`InputError`] for an empty id.
    pub fn new(id: impl Into<String>) -> Result<Self, InputError> {
        let id = id.into();
        if id.is_empty() {
            return Err(InputError::new("version id", "must not be empty"));
        }
        Ok(Self(id))
    }

    /// The id, verbatim.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VersionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An AWS Region code used both in the default endpoint and in the SigV4 credential scope
/// (`us-east-1`, `eu-west-3`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Region(String);

impl Region {
    /// Validates a Region code: 1 to 63 lowercase ASCII letters, digits and hyphens.
    ///
    /// # Errors
    ///
    /// [`InputError`] when the code has another shape.
    pub fn new(code: impl Into<String>) -> Result<Self, InputError> {
        let code = code.into();
        let ok = (1..=63).contains(&code.len())
            && code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !code.starts_with('-')
            && !code.ends_with('-');
        if ok {
            Ok(Self(code))
        } else {
            Err(InputError::new(
                "region",
                "must be lowercase ASCII letters, digits and inner hyphens",
            ))
        }
    }

    /// The code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A 12-digit AWS account id (`x-amz-expected-bucket-owner`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AccountId(String);

impl AccountId {
    /// Validates a 12-digit account id.
    ///
    /// # Errors
    ///
    /// [`InputError`] for anything but 12 ASCII digits.
    pub fn new(id: impl Into<String>) -> Result<Self, InputError> {
        let id = id.into();
        if id.len() == 12 && id.bytes().all(|b| b.is_ascii_digit()) {
            Ok(Self(id))
        } else {
            Err(InputError::new("account id", "must be 12 ASCII digits"))
        }
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One byte range of a single `GET` or `HEAD` (`Range: bytes=...`).
///
/// S3 does not support several ranges in one request, so this type holds exactly one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ByteRange {
    /// `bytes=first-last`, both inclusive.
    Inclusive {
        /// First byte offset.
        first: u64,
        /// Last byte offset (inclusive).
        last: u64,
    },
    /// `bytes=first-`: from `first` to the end.
    From {
        /// First byte offset.
        first: u64,
    },
    /// `bytes=-len`: the last `len` bytes.
    Suffix {
        /// Number of trailing bytes.
        len: u64,
    },
}

impl ByteRange {
    /// `bytes=first-last` (inclusive).
    ///
    /// # Errors
    ///
    /// [`InputError`] when `last < first`.
    pub fn inclusive(first: u64, last: u64) -> Result<Self, InputError> {
        if last < first {
            return Err(InputError::new(
                "byte range",
                "last offset is before the first",
            ));
        }
        Ok(Self::Inclusive { first, last })
    }

    /// `bytes=first-`.
    pub fn from(first: u64) -> Self {
        Self::From { first }
    }

    /// `bytes=-len`.
    ///
    /// # Errors
    ///
    /// [`InputError`] for `len == 0`.
    pub fn suffix(len: u64) -> Result<Self, InputError> {
        if len == 0 {
            return Err(InputError::new(
                "byte range",
                "a suffix length must be positive",
            ));
        }
        Ok(Self::Suffix { len })
    }

    /// Whether a `Content-Range: bytes first-last/complete` answers this range.
    pub(crate) fn is_answered_by(&self, range: &ContentRange) -> bool {
        match *self {
            Self::Inclusive { first, last } => {
                range.first == first
                    && (range.last == last
                        || (range.last < last
                            && range
                                .complete_length
                                .is_some_and(|len| range.last + 1 == len)))
            }
            Self::From { first } => {
                range.first == first
                    && range
                        .complete_length
                        .is_none_or(|len| range.last + 1 == len)
            }
            Self::Suffix { len } => match range.complete_length {
                Some(complete) => range.last + 1 == complete && range.len() == len.min(complete),
                None => range.len() <= len,
            },
        }
    }
}

impl fmt::Display for ByteRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inclusive { first, last } => write!(f, "bytes={first}-{last}"),
            Self::From { first } => write!(f, "bytes={first}-"),
            Self::Suffix { len } => write!(f, "bytes=-{len}"),
        }
    }
}

/// A `206 Partial Content` response's `Content-Range: bytes first-last/complete`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ContentRange {
    /// First byte offset of the part.
    pub first: u64,
    /// Last byte offset of the part (inclusive).
    pub last: u64,
    /// The complete representation's length; `None` for `*`.
    pub complete_length: Option<u64>,
}

impl ContentRange {
    /// Parses `bytes first-last/complete` or `bytes first-last/*` (RFC 9110 §14.4).
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let rest = value.strip_prefix("bytes ")?;
        let (span, complete) = rest.split_once('/')?;
        let (first, last) = span.split_once('-')?;
        let first: u64 = digits(first)?;
        let last: u64 = digits(last)?;
        let complete_length = match complete {
            "*" => None,
            n => Some(digits(n)?),
        };
        if last < first || complete_length.is_some_and(|len| last >= len) {
            return None;
        }
        Some(Self {
            first,
            last,
            complete_length,
        })
    }

    /// The number of bytes in the part.
    pub fn len(&self) -> u64 {
        self.last - self.first + 1
    }

    /// Always `false`: a satisfied range holds at least one byte.
    pub fn is_empty(&self) -> bool {
        false
    }
}

fn digits(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// An HTTP date (`If-Modified-Since`, `If-Unmodified-Since`, `Last-Modified`, `Expires`),
/// written in the IMF-fixdate form `Wed, 12 Oct 2009 17:50:00 GMT`. Whole seconds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HttpDate(SystemTime);

impl HttpDate {
    /// The date of an instant, truncated to whole seconds.
    pub fn from_system_time(time: SystemTime) -> Self {
        // A round trip through the text form truncates the sub-second part.
        let text = httpdate::fmt_http_date(time);
        Self(httpdate::parse_http_date(&text).unwrap_or(time))
    }

    /// Parses an HTTP date (IMF-fixdate, and the obsolete RFC 850 and asctime forms that RFC
    /// 9110 requires recipients to accept).
    ///
    /// Two deviations from IMF-fixdate found in AWS's own examples are tolerated, because the
    /// date is still unambiguous: a wrong day name (GetObject's `Wed, 12 Oct 2009 17:50:00 GMT`
    /// is a Monday) and a one-digit day (HeadObject's `Sun, 1 Jan 2006 12:00:00 GMT`). The date
    /// is read from its day, month, year and time and written back in IMF-fixdate.
    ///
    /// # Errors
    ///
    /// [`InputError`] when the text is not an HTTP date.
    pub fn parse(text: &str) -> Result<Self, InputError> {
        const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        if let Ok(time) = httpdate::parse_http_date(text) {
            return Ok(Self(time));
        }
        // `Ddd, D Mon YYYY HH:MM:SS GMT`: pad the day.
        let padded;
        let text = match text.as_bytes() {
            [_, _, _, b',', b' ', d, b' ', ..] if d.is_ascii_digit() && text.len() == 28 => {
                padded = format!("{}0{}", &text[..5], &text[5..]);
                padded.as_str()
            }
            _ => text,
        };
        if let Ok(time) = httpdate::parse_http_date(text) {
            return Ok(Self(time));
        }
        // IMF-fixdate with another day name: `Ddd, DD Mon YYYY HH:MM:SS GMT`.
        if let Some(rest) = text
            .get(3..)
            .filter(|r| r.starts_with(", ") && text.len() == 29)
        {
            for day in DAYS {
                if let Ok(time) = httpdate::parse_http_date(&format!("{day}{rest}")) {
                    return Ok(Self(time));
                }
            }
        }
        Err(InputError::new(
            "HTTP date",
            "is not an HTTP date (RFC 9110 §5.6.7)",
        ))
    }

    /// The instant.
    pub fn system_time(&self) -> SystemTime {
        self.0
    }
}

impl fmt::Display for HttpDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&httpdate::fmt_http_date(self.0))
    }
}

impl fmt::Debug for HttpDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HttpDate({self})")
    }
}

/// A `Content-Type` sent with `PutObject`: `type/subtype` with optional parameters, visible
/// ASCII. S3 stores it with the object and returns it on `GET`/`HEAD`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ContentType(String);

impl ContentType {
    /// Validates a media type (`text/plain; charset=utf-8`).
    ///
    /// # Errors
    ///
    /// [`InputError`] for anything that is not `type/subtype[; parameters]` in visible ASCII.
    pub fn new(media_type: impl Into<String>) -> Result<Self, InputError> {
        let media_type = media_type.into();
        let essence = media_type.split(';').next().unwrap_or("").trim();
        let token = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$&-^_.+".contains(&b))
        };
        let shape = essence
            .split_once('/')
            .is_some_and(|(t, s)| token(t) && token(s));
        let printable = media_type.bytes().all(|b| (0x20..0x7f).contains(&b));
        if shape && printable && media_type.trim() == media_type {
            Ok(Self(media_type))
        } else {
            Err(InputError::new(
                "content type",
                "is not `type/subtype[; parameters]` in visible ASCII",
            ))
        }
    }

    /// The media type.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An S3 storage class (`x-amz-storage-class`, `<StorageClass>`). Unknown values S3 may add later
/// are kept as [`StorageClass::Other`].
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum StorageClass {
    /// `STANDARD`
    Standard,
    /// `REDUCED_REDUNDANCY`
    ReducedRedundancy,
    /// `STANDARD_IA`
    StandardIa,
    /// `ONEZONE_IA`
    OnezoneIa,
    /// `INTELLIGENT_TIERING`
    IntelligentTiering,
    /// `GLACIER`
    Glacier,
    /// `DEEP_ARCHIVE`
    DeepArchive,
    /// `GLACIER_IR`
    GlacierIr,
    /// A value this SDK does not name (received from S3).
    Other(String),
}

impl StorageClass {
    /// The wire value.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Standard => "STANDARD",
            Self::ReducedRedundancy => "REDUCED_REDUNDANCY",
            Self::StandardIa => "STANDARD_IA",
            Self::OnezoneIa => "ONEZONE_IA",
            Self::IntelligentTiering => "INTELLIGENT_TIERING",
            Self::Glacier => "GLACIER",
            Self::DeepArchive => "DEEP_ARCHIVE",
            Self::GlacierIr => "GLACIER_IR",
            Self::Other(other) => other,
        }
    }

    pub(crate) fn from_wire(value: &str) -> Self {
        match value {
            "STANDARD" => Self::Standard,
            "REDUCED_REDUNDANCY" => Self::ReducedRedundancy,
            "STANDARD_IA" => Self::StandardIa,
            "ONEZONE_IA" => Self::OnezoneIa,
            "INTELLIGENT_TIERING" => Self::IntelligentTiering,
            "GLACIER" => Self::Glacier,
            "DEEP_ARCHIVE" => Self::DeepArchive,
            "GLACIER_IR" => Self::GlacierIr,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl fmt::Display for StorageClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// User-defined object metadata: the `x-amz-meta-*` headers, by name without the prefix.
///
/// S3 stores names in lowercase. Values are restricted to visible US-ASCII and inner spaces —
/// AWS recommends US-ASCII over REST and rewrites other values with RFC 2047, which would not
/// round-trip — and the sum of name and value bytes is at most 2 KB (AWS's limit for
/// user-defined metadata in a `PUT` request header).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Metadata(BTreeMap<String, String>);

/// The documented ceiling for user-defined metadata in one `PUT`.
pub const METADATA_LIMIT: usize = 2048;

impl Metadata {
    /// No metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `name: value` (the name is lowercased; an existing entry is replaced).
    ///
    /// # Errors
    ///
    /// [`InputError`] for a name that is not a header token, a value outside visible ASCII and
    /// inner spaces, or when the total exceeds [`METADATA_LIMIT`].
    pub fn insert(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, InputError> {
        let name = name.into().to_ascii_lowercase();
        let value = value.into();
        let token = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
        if !token {
            return Err(InputError::new(
                "metadata name",
                "must be a non-empty HTTP header token",
            ));
        }
        let printable = value.bytes().all(|b| (0x20..0x7f).contains(&b));
        if !printable || value.trim() != value {
            return Err(InputError::new(
                "metadata value",
                "must be visible US-ASCII with inner spaces only",
            ));
        }
        self.0.insert(name, value);
        if self.size() > METADATA_LIMIT {
            return Err(InputError::new(
                "metadata",
                "exceeds 2 KB of user-defined metadata",
            ));
        }
        Ok(self)
    }

    /// One value by name (lowercase, without `x-amz-meta-`).
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// The entries in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is no entry.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The sum of name and value bytes.
    pub fn size(&self) -> usize {
        self.0.iter().map(|(k, v)| k.len() + v.len()).sum()
    }

    /// Received metadata: kept as S3 returned it (no validation of the server's values).
    pub(crate) fn received(entries: BTreeMap<String, String>) -> Self {
        Self(entries)
    }
}

impl fmt::Debug for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.0.iter()).finish()
    }
}

/// A part number of an object uploaded in parts (`partNumber`, 1 to 10,000), for a `GET` or
/// `HEAD` of one part. (This SDK does not upload in parts.)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PartNumber(u16);

impl PartNumber {
    /// Validates a part number.
    ///
    /// # Errors
    ///
    /// [`InputError`] outside 1 to 10,000.
    pub fn new(n: u16) -> Result<Self, InputError> {
        if (1..=10_000).contains(&n) {
            Ok(Self(n))
        } else {
            Err(InputError::new(
                "part number",
                "must be between 1 and 10000",
            ))
        }
    }
}

impl fmt::Display for PartNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// `max-keys`: 1 to 1,000 keys per `ListObjectsV2` page.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MaxKeys(u16);

impl MaxKeys {
    /// Validates a page size.
    ///
    /// # Errors
    ///
    /// [`InputError`] outside 1 to 1,000 (S3 never returns more than 1,000 keys).
    pub fn new(n: u16) -> Result<Self, InputError> {
        if (1..=1000).contains(&n) {
            Ok(Self(n))
        } else {
            Err(InputError::new("max-keys", "must be between 1 and 1000"))
        }
    }
}

impl fmt::Display for MaxKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// `encoding-type=url`: S3 percent-encodes keys, prefixes, the delimiter and `StartAfter` in the
/// listing (for keys holding characters XML 1.0 cannot carry); the SDK decodes them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct UrlEncoding;

impl fmt::Display for UrlEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("url")
    }
}

/// `x-amz-request-payer: requester`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Requester;

impl fmt::Display for Requester {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("requester")
    }
}

/// `If-None-Match: *` on `PutObject`: write only if no object has the key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct AnyObject;

impl fmt::Display for AnyObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("*")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_names() {
        assert!(BucketName::new("examplebucket").is_ok());
        assert!(BucketName::new("amzn-s3-demo-bucket").is_ok());
        for bad in [
            "ab",
            "Upper",
            "a.b.c",
            "-ab",
            "ab-",
            "xn--abc",
            "a_b",
            "my--usw2-az1--x-s3",
        ] {
            assert!(BucketName::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn object_keys() {
        assert!(ObjectKey::new("photos/2006/x.jpg").is_ok());
        assert!(ObjectKey::new("my-object//example//photo.user").is_ok());
        assert!(ObjectKey::new(".hidden/..backup/x").is_ok());
        for bad in ["", "a/../b", "./x", "a/.", ".."] {
            assert!(ObjectKey::new(bad).is_err(), "{bad}");
        }
        assert!(ObjectKey::new("x".repeat(1025)).is_err());
    }

    #[test]
    fn etags() {
        assert!(ETag::parse("\"fba9dede5f27731c9771645a39863328\"").is_ok());
        assert!(ETag::parse("W/\"x\"").unwrap().is_weak());
        assert!(ETag::parse("\"abc-2\"").is_ok());
        for bad in ["abc", "\"a\"b\"", "\"", "\"a b\""] {
            assert!(ETag::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ranges() {
        assert_eq!(ByteRange::inclusive(0, 9).unwrap().to_string(), "bytes=0-9");
        assert_eq!(ByteRange::from(5).to_string(), "bytes=5-");
        assert_eq!(ByteRange::suffix(3).unwrap().to_string(), "bytes=-3");
        assert!(ByteRange::inclusive(5, 4).is_err());
        let cr = ContentRange::parse("bytes 0-9/443").unwrap();
        assert_eq!(cr.len(), 10);
        assert!(ByteRange::inclusive(0, 9).unwrap().is_answered_by(&cr));
        assert!(!ByteRange::inclusive(0, 10).unwrap().is_answered_by(&cr));
        // A range past the end is shortened by the server.
        let tail = ContentRange::parse("bytes 440-442/443").unwrap();
        assert!(
            ByteRange::inclusive(440, 1000)
                .unwrap()
                .is_answered_by(&tail)
        );
        assert!(ByteRange::suffix(3).unwrap().is_answered_by(&tail));
        assert!(ByteRange::from(440).is_answered_by(&tail));
        for bad in [
            "bytes 9-0/10",
            "bytes 0-9/5",
            "bytes 0-9",
            "items 0-9/10",
            "bytes */10",
        ] {
            assert!(ContentRange::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn dates() {
        let d = HttpDate::parse("Fri, 28 Jan 2011 20:10:32 GMT").unwrap();
        assert_eq!(d.to_string(), "Fri, 28 Jan 2011 20:10:32 GMT");
        // AWS's GetObject example: 12 Oct 2009 was a Monday.
        let documented = HttpDate::parse("Wed, 12 Oct 2009 17:50:00 GMT").unwrap();
        assert_eq!(documented.to_string(), "Mon, 12 Oct 2009 17:50:00 GMT");
        // AWS's HeadObject example: a one-digit day.
        let short = HttpDate::parse("Sun, 1 Jan 2006 12:00:00 GMT").unwrap();
        assert_eq!(short.to_string(), "Sun, 01 Jan 2006 12:00:00 GMT");
        assert!(HttpDate::parse("2009-10-12T17:50:00Z").is_err());
        assert!(HttpDate::parse("Xyz, 32 Oct 2009 17:50:00 GMT").is_err());
    }

    #[test]
    fn metadata_rules() {
        let m = Metadata::new().insert("Title", "the title").unwrap();
        assert_eq!(m.get("title"), Some("the title"));
        assert!(Metadata::new().insert("bad name", "x").is_err());
        assert!(Metadata::new().insert("x", "café").is_err());
        assert!(Metadata::new().insert("x", " padded").is_err());
        assert!(Metadata::new().insert("x", "y".repeat(2048)).is_err());
    }

    #[test]
    fn content_types() {
        assert!(ContentType::new("text/plain; charset=utf-8").is_ok());
        assert!(ContentType::new("application/octet-stream").is_ok());
        for bad in ["text", "text/", "/plain", "text/plain\n", " text/plain"] {
            assert!(ContentType::new(bad).is_err(), "{bad:?}");
        }
    }
}
