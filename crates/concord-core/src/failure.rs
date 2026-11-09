//! A non-2xx answer read as the provider's declared error document (`error D in F;`).

use std::fmt;

use bytes::Bytes;

use crate::codec::{CodecError, ResponseHead};
use crate::error::Error;
use crate::prepared::{BodyStep, HeadStep};
use crate::response::{Operation, ResponseMeta};

/// The bound of an error body read when the SDK sets none
/// ([`Prepared::error_body_limit`](crate::Prepared::error_body_limit)): 64 KiB.
pub(crate) const DEFAULT_ERROR_LIMIT: usize = 64 * 1024;

/// A failed HTTP status with the provider's error document, read once and bounded: the head
/// (its status stays authoritative), what became of the document, and the bytes read.
///
/// `Debug` prints the status, the operation, the document state and the byte count; never the
/// document or the bytes. [`document`](Self::document) and [`body`](Self::body) are explicit,
/// unredacted access.
pub struct Failure<D> {
    meta: ResponseMeta,
    state: Document<D>,
    body: Bytes,
    head_refused: bool,
    // Preserve the format's result for adapters of an empty document, while the public state
    // still describes the wire fact (`Empty`). This also lets byte formats return empty bytes.
    empty_document: Option<EmptyDecoder<D>>,
}

// Only empty bodies retain a pending decoder. Boxing keeps ordinary failures small; the mutex
// preserves Sync even when a custom format's Context is Send-only. It is consumed, never locked.
type EmptyDecoder<D> = Box<std::sync::Mutex<Result<BodyStep<D>, CodecError>>>;

/// What became of an error document.
#[non_exhaustive]
pub enum Document<D> {
    /// The declared document.
    Read(D),
    /// No body (a `HEAD` answer, or an empty one).
    Empty,
    /// A body the declared format refused (its head check, or decoding).
    Unrecognized(CodecError),
    /// Longer than the bound; the rest was not read and nothing was decoded.
    TooLong {
        /// The bound, in bytes.
        limit: usize,
    },
    /// Reading the body failed (Concord's `Transport` error).
    Broken(Error),
}

impl<D> Failure<D> {
    /// The response head and operation.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The HTTP status: authoritative, whatever the document says.
    pub fn status(&self) -> reqwest::StatusCode {
        self.meta.status()
    }

    /// The call's identity.
    pub fn operation(&self) -> Option<Operation> {
        self.meta.operation()
    }

    /// What became of the document.
    pub fn state(&self) -> &Document<D> {
        &self.state
    }

    /// The declared document, when it was read. **Unredacted.**
    pub fn document(&self) -> Option<&D> {
        match &self.state {
            Document::Read(document) => Some(document),
            _ => None,
        }
    }

    /// The bytes read, at most the bound (a prefix for a broken or too long body). **Unredacted.**
    pub fn body(&self) -> &Bytes {
        &self.body
    }

    /// The parts: head, document state, bytes.
    pub fn into_parts(self) -> (ResponseMeta, Document<D>, Bytes) {
        (self.meta, self.state, self.body)
    }

    /// The head and typed document outcome, for SDKs that expose `Result<D, Error>`.
    /// The error reflects the metadata/decode/read/limit state, head and bounded bytes.
    /// A read or size failure takes precedence over a format refusal.
    /// A metadata refusal has already consumed the body; it has no unread response.
    pub fn into_document(self) -> (ResponseMeta, Result<D, Error>) {
        let Self {
            meta,
            state,
            body,
            head_refused,
            empty_document,
        } = self;
        let operation = meta.operation();
        let outcome = match state {
            Document::Read(document) => Ok(document),
            Document::Empty => empty_document
                .map(|pending| {
                    (*pending)
                        .into_inner()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                })
                .unwrap_or_else(|| Err(CodecError::new("empty error document")))
                .and_then(|decode| decode(body.clone()))
                .map_err(|error| {
                    if head_refused {
                        Error::from_document_metadata(operation, error)
                    } else {
                        Error::from_decode(operation, error)
                    }
                }),
            Document::Unrecognized(error) if head_refused => {
                Err(Error::from_document_metadata(operation, error))
            }
            Document::Unrecognized(error) => Err(Error::from_decode(operation, error)),
            Document::TooLong { limit } => Err(Error::limit(operation, limit as u64)),
            Document::Broken(error) => Err(error),
        };
        let outcome = outcome.map_err(|error| error.with_head(meta.clone()).with_body(body));
        (meta, outcome)
    }

    /// The same failure with a document of another type (`Read` becomes `Unrecognized`).
    pub(crate) fn recast<E>(self) -> Failure<E> {
        let state = match self.state {
            Document::Read(_) => {
                Document::Unrecognized(CodecError::new("error document of another type"))
            }
            Document::Empty => Document::Empty,
            Document::Unrecognized(e) => Document::Unrecognized(e),
            Document::TooLong { limit } => Document::TooLong { limit },
            Document::Broken(e) => Document::Broken(e),
        };
        Failure {
            meta: self.meta,
            state,
            body: self.body,
            head_refused: self.head_refused,
            empty_document: None,
        }
    }
}

/// Reads a response of any status as an error document: the format's head step first (its
/// refusal still keeps the bytes), one bounded read, then decoding.
pub(crate) async fn read<D>(
    mut response: reqwest::Response,
    step: Option<HeadStep<D>>,
    limit: usize,
    operation: Option<Operation>,
) -> Failure<D> {
    let checked = step.map(|step| step(&ResponseHead::of(&response)));
    let head_refused = matches!(&checked, Some(Err(_)));
    let meta = ResponseMeta::take(&mut response, operation);
    let mut body = bytes::BytesMut::new();
    let mut state = None;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let room = limit - body.len();
                if chunk.len() > room {
                    body.extend_from_slice(&chunk[..room]);
                    state = Some(Document::TooLong { limit });
                    break;
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => {
                let error = Error::from_transport(operation, e).with_head(meta.clone());
                state = Some(Document::Broken(error));
                break;
            }
        }
    }
    let body = body.freeze();
    let mut empty_document = None;
    let state = match state {
        Some(state) => state,
        None if body.is_empty() => {
            empty_document = checked.map(|checked| Box::new(std::sync::Mutex::new(checked)));
            Document::Empty
        }
        None => match checked {
            None => Document::Unrecognized(CodecError::new("no error document declared")),
            Some(Err(e)) => Document::Unrecognized(e),
            Some(Ok(decode)) => match decode(body.clone()) {
                Ok(document) => Document::Read(document),
                Err(e) => Document::Unrecognized(e),
            },
        },
    };
    Failure {
        meta,
        state,
        body,
        head_refused,
        empty_document,
    }
}

impl<D> fmt::Debug for Failure<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Failure")
            .field("status", &self.status().as_u16())
            .field("operation", &self.operation())
            .field("state", &self.state)
            .field("body", &format_args!("{} bytes", self.body.len()))
            .finish()
    }
}

/// The variant only: never the document, the codec's underlying error or a body.
impl<D> fmt::Debug for Document<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(_) => f.write_str("Read(<withheld>)"),
            Self::Empty => f.write_str("Empty"),
            Self::Unrecognized(e) => f.debug_tuple("Unrecognized").field(e).finish(),
            Self::TooLong { limit } => write!(f, "TooLong {{ limit: {limit} }}"),
            Self::Broken(e) => f.debug_tuple("Broken").field(&e.kind()).finish(),
        }
    }
}
