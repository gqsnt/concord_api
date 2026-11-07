use crate::{Error, InputError};
use concord::{
    CodecError, Encode, EncodedBody,
    bytes::Bytes,
    multipart::{Multipart, MultipartBody},
    reqwest::{self, header::HeaderValue},
};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fmt,
    io::{self, Read},
    path::Path,
};

/// Local attachment bound, for both buffered and lazy sources (16 MiB).
pub const MAX_ATTACHMENT_BYTES: usize = 16 * 1024 * 1024;
/// Local text bound (1 MiB). Mailgun's final MIME size limit is still authoritative.
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

/// A retained Mailgun identifier. Debug is redacted; access the exact wire value explicitly.
#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(try_from = "String")]
pub struct MessageId(String);

impl MessageId {
    /// Validate a nonempty identifier, without changing the provider's angle brackets.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 998
            || value.chars().any(char::is_whitespace)
            || value.chars().any(char::is_control)
            || value == "<>"
            || value.starts_with('<') != value.ends_with('>')
        {
            return Err(Error::Input(InputError::MessageId));
        }
        Ok(Self(value))
    }

    /// Exact returned identifier, suitable for persistence or an event search.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn matches(&self, candidate: &str) -> bool {
        fn bare(s: &str) -> &str {
            s.strip_prefix('<')
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(s)
        }
        bare(&self.0) == bare(candidate)
    }
}

impl TryFrom<String> for MessageId {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::new(value)
    }
}

// Display is the query wire spelling. Concord marks its query placement sensitive.
impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl fmt::Debug for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MessageId(<redacted>)")
    }
}

/// Successful submission, not a delivery receipt.
#[derive(Deserialize)]
pub struct Accepted {
    /// Persist this identifier to correlate later events.
    pub id: MessageId,
    /// Provider acknowledgement text; not interpreted as delivery evidence.
    pub message: String,
}
impl fmt::Debug for Accepted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Accepted(<redacted>)")
    }
}

enum Source {
    Bytes(Bytes),
    Stream(reqwest::Body),
}

/// One file attachment. Deliberately not Clone, including the buffered case.
pub struct Attachment {
    filename: String,
    media: String,
    source: Source,
}

impl Attachment {
    fn metadata(filename: &str, media: &str) -> Result<(), Error> {
        if filename.is_empty()
            || filename.len() > 255
            || filename
                .chars()
                .any(|c| c.is_control() || matches!(c, '"' | '\\' | '/'))
            || media.chars().any(char::is_control)
            || media.parse::<mime::Mime>().is_err()
            || media.contains('*')
        {
            return Err(Error::Input(InputError::Attachment));
        }
        Ok(())
    }

    /// Buffer an attachment with an explicit filename and media type.
    pub fn bytes(filename: &str, media: &str, bytes: impl Into<Bytes>) -> Result<Self, Error> {
        Self::metadata(filename, media)?;
        let bytes = bytes.into();
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err(Error::Input(InputError::Size));
        }
        Ok(Self {
            filename: filename.into(),
            media: media.into(),
            source: Source::Bytes(bytes),
        })
    }

    /// Read a file with a bounded buffer. This performs blocking local file I/O.
    /// The filename sent to Mailgun is supplied separately from the local path.
    pub fn from_file(path: impl AsRef<Path>, filename: &str, media: &str) -> Result<Self, Error> {
        Self::metadata(filename, media)?;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| Error::Input(InputError::File))?
            .take(MAX_ATTACHMENT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Input(InputError::File))?;
        Self::bytes(filename, media, bytes)
    }

    /// Take a non-rewindable source without polling or buffering it.
    /// The byte bound is enforced during transmission; a late source/limit failure is an
    /// ambiguous send. Constructing or preparing the message polls the source zero times.
    pub fn stream<S>(filename: &str, media: &str, source: S) -> Result<Self, Error>
    where
        S: Stream<Item = Result<Bytes, io::Error>> + Send + 'static,
    {
        Self::metadata(filename, media)?;
        let stream = futures_util::stream::try_unfold(
            (Box::pin(source), 0usize),
            |(mut source, count)| async move {
                let Some(chunk) = source.next().await else {
                    return Ok(None);
                };
                let chunk = chunk?;
                if chunk.len() > MAX_ATTACHMENT_BYTES - count {
                    return Err(io::Error::other("attachment exceeds local byte bound"));
                }
                let count = count + chunk.len();
                Ok(Some((chunk, (source, count))))
            },
        );
        Ok(Self {
            filename: filename.into(),
            media: media.into(),
            source: Source::Stream(reqwest::Body::wrap_stream(stream)),
        })
    }
}

impl fmt::Debug for Attachment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Attachment(<redacted>)")
    }
}

/// Text email with exactly one attachment and 1–100 recipients.
/// Header line breaks are refused; full mailbox syntax validation belongs to Mailgun.
pub struct TextEmail {
    from: String,
    to: Vec<String>,
    subject: String,
    text: String,
    attachment: Attachment,
    tags: Vec<String>,
    variables: BTreeMap<String, serde_json::Value>,
}

impl TextEmail {
    /// Construct a message with required text and attachment. No I/O or source polling.
    pub fn new(
        from: impl Into<String>,
        to: Vec<String>,
        subject: impl Into<String>,
        text: impl Into<String>,
        attachment: Attachment,
    ) -> Result<Self, Error> {
        let (from, subject, text) = (from.into(), subject.into(), text.into());
        let header =
            |s: &str| !s.trim().is_empty() && s.len() <= 998 && !s.chars().any(char::is_control);
        if !header(&from)
            || !header(&subject)
            || to.is_empty()
            || to.len() > 100
            || !to.iter().all(|s| header(s))
            || text.is_empty()
        {
            return Err(Error::Input(InputError::Message));
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(Error::Input(InputError::Size));
        }
        Ok(Self {
            from,
            to,
            subject,
            text,
            attachment,
            tags: Vec::new(),
            variables: BTreeMap::new(),
        })
    }

    /// Add one repeated `o:tag` field, preserving its spelling. At most three are allowed.
    /// Tags must be nonempty ASCII, at most 128 bytes, without control characters.
    /// Validation performs no I/O and never polls an attachment source.
    pub fn tag(mut self, tag: impl Into<String>) -> Result<Self, Error> {
        if self.tags.len() == 3 {
            return Err(Error::Input(InputError::TagLimit));
        }
        let tag = tag.into();
        if tag.is_empty() || tag.len() > 128 || !tag.is_ascii() || tag.chars().any(char::is_control)
        {
            return Err(Error::Input(InputError::Tag));
        }
        self.tags.push(tag);
        Ok(self)
    }

    /// Attach a JSON value as `v:<name>` for later event `user-variables`.
    /// Supply the bare name; the SDK accepts nonempty `[A-Za-z0-9_-]+` names.
    /// This is a local naming rule; the provider docs do not specify a name grammar.
    /// Reusing a name replaces its value. Values are JSON encoded, including string quotes.
    /// Recipients can see this data in the delivered email's X-Mailgun-Variables header.
    /// Validation performs no I/O and never polls an attachment source.
    pub fn variable(
        mut self,
        name: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Result<Self, Error> {
        let name = name.into();
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err(Error::Input(InputError::VariableName));
        }
        self.variables.insert(name, value.into());
        Ok(self)
    }
}
impl fmt::Debug for TextEmail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TextEmail(<redacted>)")
    }
}

/// Provider field mapping over Concord multipart; reqwest frames lazy attachments.
/// Both branches bind runtime Content-Type to the encoded body and use generated `send()`.
pub struct MailgunMultipart;

impl Encode<TextEmail> for MailgunMultipart {
    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(_: TextEmail) -> Result<reqwest::Body, CodecError> {
        Err(CodecError::new(
            "Mailgun multipart requires its runtime media type",
        ))
    }

    fn encode_body(value: TextEmail) -> Result<EncodedBody, CodecError> {
        let TextEmail {
            from,
            to,
            subject,
            text,
            attachment,
            tags,
            variables,
        } = value;
        // Map all text fields once so buffered and one-shot attachments cannot diverge.
        let mut fields = vec![("from".to_owned(), from)];
        fields.extend(to.into_iter().map(|to| ("to".to_owned(), to)));
        fields.extend([("subject".to_owned(), subject), ("text".to_owned(), text)]);
        fields.extend(tags.into_iter().map(|tag| ("o:tag".to_owned(), tag)));
        fields.extend(
            variables
                .into_iter()
                .map(|(name, value)| (format!("v:{name}"), value.to_string())),
        );
        let Attachment {
            filename,
            media,
            source,
        } = attachment;
        match source {
            Source::Bytes(bytes) => {
                let mut form = MultipartBody::form_data();
                for (name, value) in fields {
                    form = form.text(name, value);
                }
                Multipart::encode_body(form.file("attachment", filename, media, bytes))
            }
            Source::Stream(body) => {
                // Native codec escape only: Concord still owns the declaration and execution.
                // Names are fixed ASCII or validated variable names. Preserve the literal
                // colon: attr-char encoding would emit name*=utf-8''o%3Atag instead.
                let mut form = reqwest::multipart::Form::new().percent_encode_noop();
                for (name, value) in fields {
                    form = form.text(name, value);
                }
                let part = reqwest::multipart::Part::stream(body)
                    .file_name(filename)
                    .mime_str(&media)
                    .map_err(|_| CodecError::new("invalid attachment media type"))?;
                let form = form.part("attachment", part);
                let media = HeaderValue::from_str(&format!(
                    "multipart/form-data; boundary={}",
                    form.boundary()
                ))
                .map_err(|_| CodecError::new("invalid multipart boundary"))?;
                Ok(EncodedBody::with_content_type(
                    reqwest::Body::wrap_stream(form.into_stream()),
                    media,
                ))
            }
        }
    }
}
