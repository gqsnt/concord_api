//! Document reads and optimistic concurrency control. See the dated references in PROVENANCE.md.

use std::{fmt, io, num::NonZeroU64};

use concord::{Json, QueryPairs, QueryPairsWriter, Received, reqwest::StatusCode};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::value::RawValue;

use crate::{
    ERROR_DOCUMENT_LIMIT, Error, ErrorDocument, ErrorField, Invalid, Protocol, ProviderError,
    ResponsePolicy, pit::Shards, provider_error, standard_response,
};

/// Maximum serialized source size for one conditional write: 8 MiB.
pub const DOCUMENT_LIMIT: usize = 8 * 1024 * 1024;

/// The sequence number and primary term identifying the last change to a document.
/// Both values are required, and neither is an Elasticsearch `_version` counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct DocumentVersion {
    #[serde(rename = "_seq_no")]
    seq_no: u64,
    #[serde(rename = "_primary_term")]
    primary_term: NonZeroU64,
}

impl DocumentVersion {
    /// Builds a concurrency token. Prefer the pair returned by [`Document::version`].
    /// Zero is a valid sequence number; the primary term must be positive.
    pub fn new(seq_no: u64, primary_term: u64) -> Result<Self, Error> {
        let primary_term =
            NonZeroU64::new(primary_term).ok_or(Error::Invalid(Invalid::DocumentVersion))?;
        Ok(Self {
            seq_no,
            primary_term,
        })
    }

    /// The document's `_seq_no`.
    pub fn seq_no(self) -> u64 {
        self.seq_no
    }

    /// The document's `_primary_term`.
    pub fn primary_term(self) -> u64 {
        self.primary_term.get()
    }
}

impl QueryPairs for DocumentVersion {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        out.pair("if_seq_no", self.seq_no);
        out.pair("if_primary_term", self.primary_term);
    }
}

/// A bounded JSON object for the generated conditional-index endpoint.
/// `Debug` withholds the source. Single-document JSON may contain raw line breaks.
#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct DocumentSource(Box<RawValue>);

impl DocumentSource {
    /// Serializes at most [`DOCUMENT_LIMIT`] bytes and refuses non-object documents.
    pub fn new<T: Serialize + ?Sized>(source: &T) -> Result<Self, Error> {
        let mut writer = SourceWriter {
            bytes: Vec::new(),
            exceeded: false,
        };
        if serde_json::to_writer(&mut writer, source).is_err() {
            return Err(Error::Invalid(if writer.exceeded {
                Invalid::DocumentTooLarge
            } else {
                Invalid::DocumentEncoding
            }));
        }
        let text = String::from_utf8(writer.bytes)
            .map_err(|_| Error::Invalid(Invalid::DocumentEncoding))?;
        if !text.trim_start().starts_with('{') {
            return Err(Error::Invalid(Invalid::DocumentNotObject));
        }
        RawValue::from_string(text)
            .map(Self)
            .map_err(|_| Error::Invalid(Invalid::DocumentEncoding))
    }
}

impl fmt::Debug for DocumentSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DocumentSource(<{} bytes>)", self.0.get().len())
    }
}

struct SourceWriter {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl io::Write for SourceWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > DOCUMENT_LIMIT - self.bytes.len() {
            self.exceeded = true;
            return Err(io::Error::other("document source limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The typed outcome of reading a document. Absence is a normal result, not an error document.
#[derive(Debug)]
pub enum GetDocument {
    /// A document with its concurrency token.
    Found(Document),
    /// A `404` get-result with `found: false`. A missing index is still [`Error::Provider`].
    NotFound {
        /// The response's `_index` (an alias may resolve to a concrete index).
        index: String,
        /// The response's `_id`.
        id: String,
    },
}

/// A retrieved document. `Debug` never prints its source.
#[derive(concord::Debug)]
pub struct Document {
    index: String,
    id: String,
    version: DocumentVersion,
    #[sensitive]
    source: Option<Box<RawValue>>,
}

impl Document {
    /// The concrete `_index` returned by Elasticsearch.
    pub fn index(&self) -> &str {
        &self.index
    }

    /// The `_id`.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The complete concurrency token to pass to conditional indexing.
    pub fn version(&self) -> DocumentVersion {
        self.version
    }

    /// The source as exact JSON text, absent if source storage is disabled.
    pub fn source_json(&self) -> Option<&str> {
        self.source.as_deref().map(RawValue::get)
    }

    /// Decodes the source as `T`. A serde error may quote source content.
    pub fn source<T: de::DeserializeOwned>(&self) -> Option<Result<T, serde_json::Error>> {
        self.source_json().map(serde_json::from_str)
    }
}

impl<'de> Deserialize<'de> for GetDocument {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        GetWire::deserialize(deserializer)?.into_document()
    }
}

// Decode directly from JSON: serde's untagged-enum buffering cannot preserve RawValue sources.
#[derive(Deserialize)]
struct GetWire {
    error: Option<ErrorField>,
    status: Option<u16>,
    #[serde(rename = "_index")]
    index: Option<String>,
    #[serde(rename = "_id")]
    id: Option<String>,
    found: Option<bool>,
    #[serde(rename = "_seq_no")]
    seq_no: Option<u64>,
    #[serde(rename = "_primary_term")]
    primary_term: Option<NonZeroU64>,
    #[serde(rename = "_source")]
    source: Option<Box<RawValue>>,
}

impl GetWire {
    fn into_document<E: de::Error>(self) -> Result<GetDocument, E> {
        if self.error.is_some() {
            return Err(E::custom("a get-result cannot contain an error"));
        }
        let index = self.index.ok_or_else(|| E::missing_field("_index"))?;
        let id = self.id.ok_or_else(|| E::missing_field("_id"))?;
        let found = self.found.ok_or_else(|| E::missing_field("found"))?;
        if !found {
            return Ok(GetDocument::NotFound { index, id });
        }
        let version = DocumentVersion {
            seq_no: self.seq_no.ok_or_else(|| E::missing_field("_seq_no"))?,
            primary_term: self
                .primary_term
                .ok_or_else(|| E::missing_field("_primary_term"))?,
        };
        Ok(GetDocument::Found(Document {
            index,
            id,
            version,
            source: self.source,
        }))
    }
}

// Require an actual `error` member before treating a 404 body as a provider error. The ordinary
// ErrorDocument has optional fields and would otherwise greedily match every get-result.
enum Get404 {
    Error {
        error: ErrorField,
        status: Option<u16>,
    },
    Document(GetDocument),
}

impl<'de> Deserialize<'de> for Get404 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = GetWire::deserialize(deserializer)?;
        if let Some(error) = wire.error {
            return Ok(Self::Error {
                error,
                status: wire.status,
            });
        }
        wire.into_document().map(Self::Document)
    }
}

impl ResponsePolicy<GetErrorDocument> for GetDocument {
    async fn read(received: Received<Self, GetErrorDocument>) -> Result<Self, Error> {
        if received.status() == StatusCode::NOT_FOUND {
            let (meta, body) = received.failure(ERROR_DOCUMENT_LIMIT).await.into_document();
            let document = match body {
                Ok(GetErrorDocument::Lookup(Ok(missing @ Self::NotFound { .. }))) => {
                    return Ok(missing);
                }
                Ok(GetErrorDocument::Lookup(Ok(Self::Found(_)))) => {
                    return Err(Error::Protocol(Protocol::DocumentStatus));
                }
                Ok(GetErrorDocument::Lookup(Err(document))) => Ok(document),
                Ok(GetErrorDocument::Provider(_)) => unreachable!("404 codec selected"),
                Err(error) => Err(error),
            };
            return Err(Error::Provider(Box::new(ProviderError::new(
                meta, document,
            ))));
        }
        if !received.status().is_success() {
            let (meta, document) = received.failure(ERROR_DOCUMENT_LIMIT).await.into_document();
            let document = document.map(|document| match document {
                GetErrorDocument::Provider(document) => document,
                GetErrorDocument::Lookup(_) => unreachable!("non-404 codec selected"),
            });
            return Err(Error::Provider(Box::new(ProviderError::new(
                meta, document,
            ))));
        }
        let result = received.into_success()?.decode().await?;
        match result {
            Self::Found(_) => Ok(result),
            Self::NotFound { .. } => Err(Error::Protocol(Protocol::DocumentStatus)),
        }
    }
}

/// The result of a successful index operation (complete document replacement).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexResult {
    /// A new document was created.
    Created,
    /// An existing document was replaced.
    Updated,
}

/// Successful conditional indexing, including the token for the new revision.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct IndexResponse {
    /// The concrete `_index`.
    #[serde(rename = "_index")]
    pub index: String,
    /// The `_id`.
    #[serde(rename = "_id")]
    pub id: String,
    /// Elasticsearch's `_version` counter; distinct from the concurrency token.
    #[serde(rename = "_version")]
    pub document_version: u64,
    /// The new revision's sequence number and primary term.
    #[serde(flatten)]
    pub version: DocumentVersion,
    /// Whether this was created or updated.
    pub result: IndexResult,
    /// Replication counts. A primary write may succeed while a replica fails.
    #[serde(rename = "_shards")]
    pub shards: Shards,
}

impl ResponsePolicy for IndexResponse {
    async fn read(received: Received<Self, crate::ErrorDocument>) -> Result<Self, Error> {
        if received.status() == StatusCode::CONFLICT {
            return Err(Error::VersionConflict(provider_error(received).await));
        }
        standard_response(received).await
    }
}

/// A document lookup has a distinct HTTP 404 representation; other statuses use the API error.
pub enum GetErrorDocument {
    /// HTTP 404 is either an explicit missing document or a provider error.
    Lookup(Result<GetDocument, ErrorDocument>),
    /// Other non-2xx statuses use the ordinary API error.
    Provider(ErrorDocument),
}
/// JSON error handling for a document lookup, selected by the response status.
pub struct GetErrorJson;
impl concord::Decode<GetErrorDocument> for GetErrorJson {
    type Context = u16;
    fn check_content_type(head: &concord::ResponseHead<'_>) -> Result<(), concord::CodecError> {
        <Json as concord::Decode<ErrorDocument>>::check_content_type(head)
    }
    fn context(head: &concord::ResponseHead<'_>) -> Result<u16, concord::CodecError> {
        Ok(head.status().as_u16())
    }
    fn decode_body(
        status: u16,
        body: concord::bytes::Bytes,
    ) -> Result<GetErrorDocument, concord::CodecError> {
        if status == 404 {
            <Json as concord::Decode<Get404>>::decode_body((), body).map(|document| {
                GetErrorDocument::Lookup(match document {
                    Get404::Document(document) => Ok(document),
                    Get404::Error { error, status } => Err(ErrorDocument {
                        error: Some(error),
                        status,
                    }),
                })
            })
        } else {
            <Json as concord::Decode<ErrorDocument>>::decode_body((), body)
                .map(GetErrorDocument::Provider)
        }
    }
}
