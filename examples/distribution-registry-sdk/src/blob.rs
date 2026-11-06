//! Blobs: `HEAD` (existence, length) and `GET` read incrementally and verified.
//!
//! A blob is read as it arrives and hashed on the way. Its bytes are **unverified until the
//! reader ends**: [`BlobReader::next_chunk`] returns `Ok(None)` only after the whole body was read,
//! its length matched the expected size (a descriptor's `size`, else `Content-Length`) and its
//! digest matched the digest it was fetched by. A body longer than expected fails at the first
//! byte too many, without reading further. Consumers that cannot use unverified bytes read the
//! whole blob with [`BlobReader::read_to_end`], which returns nothing unless it verified.
//!
//! A `Docker-Content-Digest` header with the fetch digest's algorithm must name the same digest
//! (checked before the body is read); one with another algorithm is ignored, as the spec allows.
//! The header is never trusted instead of the local computation.

use std::fmt;

use concord::Incoming;
use concord::bytes::{Bytes, BytesMut};
use concord::reqwest::StatusCode;

use crate::client::{Repository, drain, verbatim_last_segment};
use crate::digest::{Digest, Hasher};
use crate::error::{InputError, IntegrityProblem, RegistryError, expect_status};
use crate::manifest::Descriptor;
use crate::meta::ResponseMeta;

/// What `HEAD /v2/<name>/blobs/<digest>` reported.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct BlobInfo {
    /// `Content-Length`: the blob's length.
    pub size: Option<u64>,
    /// `Docker-Content-Digest`, when sent.
    pub digest: Option<Digest>,
    /// The response metadata.
    pub meta: ResponseMeta,
}

impl Repository {
    /// `HEAD /v2/<name>/blobs/<digest>`: `Ok(None)` for `404` (the blob is not in this
    /// repository; a `HEAD` answer has no body, so an unknown repository looks the same).
    ///
    /// # Errors
    ///
    /// A [`RegistryError`]; `Integrity` when `Docker-Content-Digest` (same algorithm) names
    /// another blob.
    pub async fn blob_info(&self, digest: &Digest) -> Result<Option<BlobInfo>, RegistryError> {
        let api = self.api.clone();
        let received = self
            .registry
            .execute(&self.pull(), |auth| {
                let prepared = api
                    .clone()
                    .maybe_authorization(auth)
                    .blob_head(digest.clone())
                    .prepare()?;
                verbatim_last_segment(prepared, digest.as_str())
            })
            .await?;
        if received.status() == StatusCode::NOT_FOUND {
            let meta = ResponseMeta::capture(&received);
            drain(received, &meta).await?;
            return Ok(None);
        }
        let (received, meta) = expect_status(received, StatusCode::OK).await?;
        let protocol = |problem| RegistryError::protocol(problem, Some(meta.clone()));
        let size = meta.content_length().map_err(protocol)?;
        let header = meta.digest_header().map_err(protocol)?;
        drain(received, &meta).await?;
        if let Some(header) = &header
            && header.algorithm() == digest.algorithm()
            && header != digest
        {
            return Err(RegistryError::integrity(
                IntegrityProblem::HeaderDigestMismatch {
                    expected: digest.clone(),
                    header: header.clone(),
                },
                Some(meta),
            ));
        }
        Ok(Some(BlobInfo {
            size,
            digest: header,
            meta,
        }))
    }

    /// `GET /v2/<name>/blobs/<digest>` for a descriptor: verified against its digest **and**
    /// size.
    ///
    /// # Errors
    ///
    /// As [`blob`](Self::blob).
    pub async fn blob_for(&self, descriptor: &Descriptor) -> Result<BlobReader, RegistryError> {
        self.blob(&descriptor.digest, Some(descriptor.size)).await
    }

    /// `GET /v2/<name>/blobs/<digest>`: a reader over the unread body, verified against `digest`
    /// and, when given, `size` (else `Content-Length`, when sent).
    ///
    /// # Errors
    ///
    /// [`IntegrityProblem::UnsupportedAlgorithm`] before sending, for a digest this SDK cannot
    /// compute; then a [`RegistryError`]: `Api` (`BLOB_UNKNOWN`, ...), `Integrity` (a
    /// `Content-Length` or `Docker-Content-Digest` that contradicts the expectation).
    pub async fn blob(
        &self,
        digest: &Digest,
        size: Option<u64>,
    ) -> Result<BlobReader, RegistryError> {
        let algorithm = digest.supported_algorithm().ok_or_else(|| {
            RegistryError::integrity(IntegrityProblem::UnsupportedAlgorithm, None)
        })?;
        let api = self.api.clone();
        let received = self
            .registry
            .execute(&self.pull(), |auth| {
                let prepared = api
                    .clone()
                    .maybe_authorization(auth)
                    .blob(digest.clone())
                    .prepare()?;
                verbatim_last_segment(prepared, digest.as_str())
            })
            .await?;
        let (received, meta) = expect_status(received, StatusCode::OK).await?;
        let protocol = |problem| RegistryError::protocol(problem, Some(meta.clone()));
        let length = meta.content_length().map_err(protocol)?;
        let header = meta.digest_header().map_err(protocol)?;
        let integrity = |problem| RegistryError::integrity(problem, Some(meta.clone()));
        if let (Some(expected), Some(length)) = (size, length)
            && expected != length
        {
            return Err(integrity(IntegrityProblem::SizeMismatch {
                expected,
                actual: length,
            }));
        }
        if let Some(header) = header
            && header.algorithm() == digest.algorithm()
            && header != *digest
        {
            return Err(integrity(IntegrityProblem::HeaderDigestMismatch {
                expected: digest.clone(),
                header,
            }));
        }
        let incoming = received
            .decode()
            .await
            .map_err(|error| RegistryError::request(error, meta.clone()))?;
        Ok(BlobReader {
            incoming: Some(incoming),
            hasher: Some(Hasher::new(algorithm)),
            expected: digest.clone(),
            size: size.or(length),
            read: 0,
            meta,
        })
    }
}

/// A blob body being read and verified (see the [module documentation](self)). `Debug` shows
/// the digest, the counters and the metadata.
pub struct BlobReader {
    incoming: Option<Incoming>,
    hasher: Option<Hasher>,
    expected: Digest,
    size: Option<u64>,
    read: u64,
    meta: ResponseMeta,
}

impl BlobReader {
    /// The next chunk of **unverified** bytes, or `Ok(None)` once the whole blob was read and
    /// verified. After an error, or after `Ok(None)`, every call is
    /// [`InputError::ReaderEnded`].
    ///
    /// # Errors
    ///
    /// `Integrity` (size or digest), `Request` (the body broke off: Concord's `Transport`).
    pub async fn next_chunk(&mut self) -> Result<Option<Bytes>, RegistryError> {
        let Some(incoming) = self.incoming.as_mut() else {
            return Err(InputError::ReaderEnded.into());
        };
        let chunk = match incoming.chunk().await {
            Ok(chunk) => chunk,
            Err(error) => {
                self.end();
                return Err(RegistryError::request(error, self.meta.clone()));
            }
        };
        match chunk {
            Some(chunk) => {
                self.read += chunk.len() as u64;
                if let Some(size) = self.size
                    && self.read > size
                {
                    self.end();
                    return Err(self.integrity(IntegrityProblem::SizeMismatch {
                        expected: size,
                        actual: self.read,
                    }));
                }
                if let Some(hasher) = self.hasher.as_mut() {
                    hasher.update(&chunk);
                }
                Ok(Some(chunk))
            }
            None => {
                self.incoming = None;
                let actual = self.hasher.take().map(Hasher::finish);
                if let Some(size) = self.size
                    && self.read != size
                {
                    return Err(self.integrity(IntegrityProblem::SizeMismatch {
                        expected: size,
                        actual: self.read,
                    }));
                }
                match actual {
                    Some(actual) if actual == self.expected => Ok(None),
                    Some(actual) => Err(self.integrity(IntegrityProblem::DigestMismatch {
                        expected: self.expected.clone(),
                        actual,
                    })),
                    None => Err(InputError::ReaderEnded.into()),
                }
            }
        }
    }

    /// The whole blob, verified; at most `limit` bytes are read (more is
    /// [`IntegrityProblem::SizeMismatch`] with `expected: limit`).
    ///
    /// # Errors
    ///
    /// As [`next_chunk`](Self::next_chunk).
    pub async fn read_to_end(mut self, limit: u64) -> Result<Bytes, RegistryError> {
        let mut out = BytesMut::new();
        while let Some(chunk) = self.next_chunk().await? {
            if out.len() as u64 + chunk.len() as u64 > limit {
                self.end();
                return Err(self.integrity(IntegrityProblem::SizeMismatch {
                    expected: limit,
                    actual: out.len() as u64 + chunk.len() as u64,
                }));
            }
            out.extend_from_slice(&chunk);
        }
        Ok(out.freeze())
    }

    /// The digest the blob is verified against.
    pub fn digest(&self) -> &Digest {
        &self.expected
    }

    /// Bytes read so far.
    pub fn bytes_read(&self) -> u64 {
        self.read
    }

    /// The response metadata.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    fn end(&mut self) {
        self.incoming = None;
        self.hasher = None;
    }

    fn integrity(&self, problem: IntegrityProblem) -> RegistryError {
        RegistryError::integrity(problem, Some(self.meta.clone()))
    }
}

impl fmt::Debug for BlobReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlobReader")
            .field("digest", &self.expected)
            .field("size", &self.size)
            .field("read", &self.read)
            .field("ended", &self.incoming.is_none())
            .field("meta", &self.meta)
            .finish()
    }
}
