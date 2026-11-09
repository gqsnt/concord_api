//! Object metadata, bodies and the response formats of `HeadObject`, `GetObject` and
//! `PutObject`.
//!
//! These formats read the response **head** (in Concord's `context` step, before any body byte):
//! a malformed header is an `ErrorKind::Metadata` error that keeps the response unread.
//! `GetObject` is declared `-> stream`: its format returns the parsed head with the **unread**
//! body ([`ObjectBody`]), which the caller reads chunk by chunk or collects under a limit.

use concord::bytes::{Bytes, BytesMut};
use concord::reqwest::{self, StatusCode, header::HeaderMap};
use concord::{CodecError, Decode, Incoming, Incremental, Operation, ResponseHead};

use crate::error::{BodyError, BodyErrorKind, ResponseMeta, S3Error};
use crate::types::{ContentRange, ETag, HttpDate, Metadata, StorageClass, VersionId};

/// What `HeadObject` and `GetObject` say about an object (system and user-defined metadata).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ObjectHead {
    /// `200 OK` or `206 Partial Content`.
    pub status: StatusCode,
    /// `Content-Length`: bytes in this response's representation (the part, for a range).
    pub content_length: Option<u64>,
    /// `Content-Type` stored with the object.
    pub content_type: Option<String>,
    /// `ETag`: an opaque validator for conditional requests (not necessarily an MD5).
    pub etag: Option<ETag>,
    /// `Last-Modified`.
    pub last_modified: Option<HttpDate>,
    /// `Content-Range` of a `206`.
    pub content_range: Option<ContentRange>,
    /// `Accept-Ranges`.
    pub accept_ranges: Option<String>,
    /// `Cache-Control`.
    pub cache_control: Option<String>,
    /// `Content-Disposition`.
    pub content_disposition: Option<String>,
    /// `Content-Encoding` (reqwest removes it if *it* decompressed the body: build the client
    /// without automatic decompression to keep object bytes exact).
    pub content_encoding: Option<String>,
    /// `Content-Language`.
    pub content_language: Option<String>,
    /// `Expires`, as sent (S3 stores any text).
    pub expires: Option<String>,
    /// `x-amz-version-id`.
    pub version_id: Option<VersionId>,
    /// `x-amz-storage-class` (absent for `STANDARD`).
    pub storage_class: Option<StorageClass>,
    /// `x-amz-delete-marker: true`.
    pub delete_marker: bool,
    /// `x-amz-server-side-encryption`.
    pub server_side_encryption: Option<String>,
    /// `x-amz-missing-meta`: user metadata entries S3 could not return as headers.
    pub missing_meta: Option<u32>,
    /// `x-amz-meta-*`, by name without the prefix.
    pub metadata: Metadata,
    /// Status and request ids.
    pub meta: ResponseMeta,
}

impl ObjectHead {
    pub(crate) fn parse(status: StatusCode, headers: &HeaderMap) -> Result<Self, CodecError> {
        let text = |name: &'static str| -> Result<Option<String>, CodecError> {
            let Some(first) = concord::field::single(headers, name)
                .map_err(|_| CodecError::new("an S3 object header occurs more than once"))?
            else {
                return Ok(None);
            };
            std::str::from_utf8(first.as_bytes())
                .map(|s| Some(s.to_owned()))
                .map_err(|_| CodecError::new("an S3 object header is not UTF-8"))
        };
        let content_length = text("content-length")?
            .map(|v| {
                v.trim()
                    .parse::<u64>()
                    .map_err(|_| CodecError::new("Content-Length is not a number"))
            })
            .transpose()?;
        let etag = text("etag")?
            .map(|v| ETag::parse(v).map_err(|_| CodecError::new("ETag is not an entity tag")))
            .transpose()?;
        let last_modified = text("last-modified")?
            .map(|v| {
                HttpDate::parse(&v)
                    .map_err(|_| CodecError::new("Last-Modified is not an HTTP date"))
            })
            .transpose()?;
        let content_range = text("content-range")?
            .map(|v| {
                ContentRange::parse(&v).ok_or_else(|| {
                    CodecError::new("Content-Range is not a single satisfied byte range")
                })
            })
            .transpose()?;
        let version_id = text("x-amz-version-id")?.and_then(|v| VersionId::new(v).ok());
        let missing_meta = text("x-amz-missing-meta")?
            .map(|v| {
                v.trim()
                    .parse::<u32>()
                    .map_err(|_| CodecError::new("x-amz-missing-meta is not a number"))
            })
            .transpose()?;
        Ok(Self {
            status,
            content_length,
            content_type: text("content-type")?,
            etag,
            last_modified,
            content_range,
            accept_ranges: text("accept-ranges")?,
            cache_control: text("cache-control")?,
            content_disposition: text("content-disposition")?,
            content_encoding: text("content-encoding")?,
            content_language: text("content-language")?,
            expires: text("expires")?,
            version_id,
            storage_class: text("x-amz-storage-class")?.map(|s| StorageClass::from_wire(&s)),
            delete_marker: text("x-amz-delete-marker")?.as_deref() == Some("true"),
            server_side_encryption: text("x-amz-server-side-encryption")?,
            missing_meta,
            metadata: Metadata::received(crate::xml::metadata_headers(headers)?),
            meta: ResponseMeta::from_headers(status, headers),
        })
    }

    /// Whether this is a part of the object (`206 Partial Content`).
    pub fn is_partial(&self) -> bool {
        self.status == StatusCode::PARTIAL_CONTENT
    }
}

/// Uses the default `Decode::ACCEPT = None`: no format-added `accept`.
///
/// The response format of `HeadObject`: parses the head; the (empty) body is ignored.
pub struct HeadFormat;

impl Decode<ObjectHead> for HeadFormat {
    type Context = ObjectHead;

    fn context(head: &ResponseHead<'_>) -> Result<ObjectHead, CodecError> {
        ObjectHead::parse(head.status(), head.headers())
    }

    fn decode_body(head: ObjectHead, _body: Bytes) -> Result<ObjectHead, CodecError> {
        Ok(head)
    }
}

/// Uses the default `Incremental::ACCEPT = None`: no format-added `accept`.
///
/// The response format of `GetObject`: checks and parses the head, then hands the **unread**
/// body to the caller in a [`GetObjectOutput`].
///
/// Refused before the body is read (`ErrorKind::Metadata`): a `multipart/byteranges` response,
/// a `206` without one valid `Content-Range`, a `206` whose `Content-Length` differs from its
/// range's length, and any malformed object header.
pub struct Download;

impl Incremental<Bytes> for Download {
    type Records = GetObjectOutput;
    type Context = ObjectHead;

    fn context(head: &ResponseHead<'_>) -> Result<ObjectHead, CodecError> {
        let parsed = ObjectHead::parse(head.status(), head.headers())?;
        if parsed.content_type.as_deref().is_some_and(|t| {
            t.trim_start()
                .to_ascii_lowercase()
                .starts_with("multipart/byteranges")
        }) {
            return Err(CodecError::new(
                "a multipart/byteranges response (this SDK requests one range)",
            ));
        }
        if parsed.status == StatusCode::PARTIAL_CONTENT {
            let Some(range) = parsed.content_range else {
                return Err(CodecError::new("a 206 response has no Content-Range"));
            };
            if parsed.content_length.is_some_and(|len| len != range.len()) {
                return Err(CodecError::new(
                    "Content-Length disagrees with Content-Range",
                ));
            }
        }
        Ok(parsed)
    }

    fn open(head: ObjectHead, body: Incoming) -> GetObjectOutput {
        let (response, operation) = body.into_parts();
        GetObjectOutput {
            body: ObjectBody {
                expected: head.content_length,
                meta: head.meta.clone(),
                response: Some(response),
                operation,
                received: 0,
                failure: None,
            },
            head,
        }
    }
}

/// A `GetObject` result: the parsed head and the unread body.
#[derive(Debug)]
#[non_exhaustive]
pub struct GetObjectOutput {
    /// Status, validators, system and user metadata.
    pub head: ObjectHead,
    /// The object bytes (or the requested part), not yet read.
    pub body: ObjectBody,
}

impl GetObjectOutput {
    /// Reads the whole body, refusing more than `limit` bytes (before reading when
    /// `Content-Length` already says so). See [`ObjectBody::collect`].
    ///
    /// # Errors
    ///
    /// [`S3Error::Body`].
    pub async fn bytes(self, limit: u64) -> Result<(ObjectHead, Bytes), S3Error> {
        let bytes = self.body.collect(limit).await?;
        Ok((self.head, bytes))
    }
}

/// How far an [`ObjectBody`] got, like the `completion()` of Concord's incremental readers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyCompletion {
    /// The end has not been read yet: more bytes may follow.
    Reading,
    /// The body ended cleanly after exactly `Content-Length` bytes (or, without a
    /// `Content-Length`, at the clean end of a chunked body): the only state that proves every
    /// byte was received.
    EndOfInput,
    /// A [`BodyError`] of this kind ended the transfer: not the end of the object. Later
    /// [`ObjectBody::chunk`] calls read nothing and return `Ok(None)`.
    Failed(BodyErrorKind),
}

/// An object body being received.
///
/// [`chunk`](Self::chunk) returns the bytes as they arrive and `Ok(None)` once nothing more will
/// be read: at the verified end (exactly `Content-Length` bytes; a shorter body is
/// [`BodyErrorKind::Truncated`]), **and also on every call after an error**. The first error is
/// the one to keep; [`completion`](Self::completion) is
/// [`EndOfInput`](BodyCompletion::EndOfInput) only at the verified end, so a loop that went on
/// after an error can still tell the two apart. Dropping the value (or the future reading it)
/// stops the transfer and closes the connection; nothing keeps reading in the background.
pub struct ObjectBody {
    response: Option<reqwest::Response>,
    operation: Option<Operation>,
    expected: Option<u64>,
    received: u64,
    meta: ResponseMeta,
    failure: Option<BodyErrorKind>,
}

impl ObjectBody {
    fn operation_name(&self) -> &'static str {
        self.operation.map_or("S3 object body", |op| op.name())
    }

    fn fail(&mut self, kind: BodyErrorKind, transport: Option<concord::Error>) -> S3Error {
        self.response = None;
        self.failure = Some(kind);
        S3Error::Body(Box::new(BodyError::new(
            self.operation_name(),
            kind,
            self.received,
            self.expected,
            self.meta.clone(),
            transport,
        )))
    }

    /// The next chunk; `Ok(None)` at the verified end, or after an earlier error (see
    /// [`completion`](Self::completion)).
    ///
    /// # Errors
    ///
    /// [`S3Error::Body`] ([`BodyErrorKind::Transport`] or [`BodyErrorKind::Truncated`]); every
    /// later call returns `Ok(None)` and [`completion`](Self::completion) stays
    /// [`Failed`](BodyCompletion::Failed).
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, S3Error> {
        let Some(response) = self.response.as_mut() else {
            return Ok(None);
        };
        match response.chunk().await {
            Ok(Some(chunk)) => {
                // hyper enforces `Content-Length` framing, so no chunk crosses it.
                self.received += chunk.len() as u64;
                Ok(Some(chunk))
            }
            Ok(None) => {
                self.response = None;
                if self.expected.is_some_and(|len| self.received != len) {
                    return Err(self.fail(BodyErrorKind::Truncated, None));
                }
                Ok(None)
            }
            Err(error) => {
                let converted = match self.operation {
                    Some(op) => op.transport_error(error),
                    None => Operation::label("S3 object body").transport_error(error),
                };
                Err(self.fail(BodyErrorKind::Transport, Some(converted)))
            }
        }
    }

    /// Reads the whole body into memory, refusing more than `limit` bytes. When `Content-Length`
    /// exceeds `limit` nothing is read.
    ///
    /// # Errors
    ///
    /// [`S3Error::Body`].
    pub async fn collect(mut self, limit: u64) -> Result<Bytes, S3Error> {
        if self.expected.is_some_and(|len| len > limit) {
            return Err(self.fail(BodyErrorKind::LimitExceeded, None));
        }
        let mut buffer = BytesMut::new();
        while let Some(chunk) = self.chunk().await? {
            if buffer.len() as u64 + chunk.len() as u64 > limit {
                return Err(self.fail(BodyErrorKind::LimitExceeded, None));
            }
            buffer.extend_from_slice(&chunk);
        }
        Ok(buffer.freeze())
    }

    /// Bytes received so far.
    pub fn received(&self) -> u64 {
        self.received
    }

    /// The `Content-Length` S3 declared.
    pub fn expected(&self) -> Option<u64> {
        self.expected
    }

    /// Whether nothing more will be read: the verified end was reached **or** an error stopped
    /// the transfer. [`completion`](Self::completion) says which.
    pub fn is_finished(&self) -> bool {
        self.response.is_none()
    }

    /// Whether the body ended cleanly, failed, or may still yield bytes.
    pub fn completion(&self) -> BodyCompletion {
        match (&self.response, self.failure) {
            (Some(_), _) => BodyCompletion::Reading,
            (None, Some(kind)) => BodyCompletion::Failed(kind),
            (None, None) => BodyCompletion::EndOfInput,
        }
    }
}

impl std::fmt::Debug for ObjectBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectBody")
            .field("operation", &self.operation)
            .field("received", &self.received)
            .field("expected", &self.expected)
            .field("finished", &self.is_finished())
            .field("completion", &self.completion())
            .finish()
    }
}

/// What `PutObject` returns.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct PutObjectOutput {
    /// `ETag` of the stored object (opaque).
    pub etag: Option<ETag>,
    /// `x-amz-version-id` (versioned buckets).
    pub version_id: Option<VersionId>,
    /// `x-amz-expiration`, as sent.
    pub expiration: Option<String>,
    /// `x-amz-server-side-encryption`.
    pub server_side_encryption: Option<String>,
    /// `x-amz-object-size`.
    pub object_size: Option<u64>,
    /// Status and request ids.
    pub meta: ResponseMeta,
}

/// Uses the default `Decode::ACCEPT = None`: no format-added `accept`.
///
/// The response format of `PutObject`: parses the head (S3 sends an empty body).
pub struct PutFormat;

impl Decode<PutObjectOutput> for PutFormat {
    type Context = PutObjectOutput;

    fn context(head: &ResponseHead<'_>) -> Result<PutObjectOutput, CodecError> {
        let headers = head.headers();
        let text = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        Ok(PutObjectOutput {
            etag: text("etag")
                .map(|v| ETag::parse(v).map_err(|_| CodecError::new("ETag is not an entity tag")))
                .transpose()?,
            version_id: text("x-amz-version-id").and_then(|v| VersionId::new(v).ok()),
            expiration: text("x-amz-expiration"),
            server_side_encryption: text("x-amz-server-side-encryption"),
            object_size: text("x-amz-object-size").and_then(|v| v.parse().ok()),
            meta: ResponseMeta::from_headers(head.status(), headers),
        })
    }

    fn decode_body(output: PutObjectOutput, _body: Bytes) -> Result<PutObjectOutput, CodecError> {
        Ok(output)
    }
}
