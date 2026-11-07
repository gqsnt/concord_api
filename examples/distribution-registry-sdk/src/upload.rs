//! Blob uploads: start, ordered chunks, status after an interruption, completion by digest.
//!
//! **The server's URL, always.** Every request of a session goes to the **latest** `Location`
//! the registry returned — after the start, after each chunk, after a status query — exactly as
//! sent, opaque query state included. The SDK never builds an upload URL from the
//! `Docker-Upload-UUID` (kept for correlation only), and a `Location` on another origin is
//! refused ([`ProtocolProblem::ForeignLocation`]) before anything is sent to it. Completion adds
//! `digest=<digest>` to that URL's query, after the registry's own parameters.
//!
//! **Inclusive ranges.** A chunk is sent with `Content-Range: <start>-<end>`, both inclusive
//! (the spec's non-standard use); the registry acknowledges with `Range: [bytes=]0-<end>`.
//! [`BlobUpload::offset`] is the number of bytes the registry acknowledged: the next chunk starts
//! there. Distribution writes `0-0` both for a fresh session and for one holding a single byte;
//! the SDK reads `0-0` as "nothing received" and therefore refuses a first chunk of exactly one
//! byte ([`InputError::OneByteFirstChunk`]) — a one-byte blob goes with the completing `PUT`.
//!
//! **Uncertainty and resume.** Once a chunk or completion request has been started, the session
//! needs a status query unless a documented answer was read: after a transport failure, a
//! response that breaks the protocol, a non-2xx answer other than `416`, or a dropped future,
//! [`BlobUpload::needs_status`] is `true` and [`BlobUpload::patch`] / [`BlobUpload::complete`]
//! refuse ([`InputError::StatusRequired`]) until [`BlobUpload::status`] has read the registry's
//! authoritative range (and its `Location`). The caller then continues from
//! [`BlobUpload::offset`], re-slicing its source there. A `416` carries the last valid range and
//! `Location`; the SDK adopts them and returns the `416` as an error, so the caller resumes from
//! the reported offset without a status query.
//!
//! The SDK retries nothing on its own. A Bearer challenge on any request is answered once with a
//! push-scoped token (`repository:<name>:pull,push`).

use std::fmt;
use std::num::NonZeroUsize;

use concord::bytes::Bytes;
use concord::reqwest::header::{HeaderMap, RANGE};
use concord::reqwest::{StatusCode, Url};

use crate::client::{Repository, drain, missing_location};
use crate::digest::Digest;
use crate::error::{
    InputError, IntegrityProblem, ProtocolProblem, RegistryError, api_error, expect_status,
};
use crate::meta::ResponseMeta;

/// An upload session (`POST /v2/<name>/blobs/uploads/`). Not `Clone`: one owner drives it.
/// `Debug` withholds the `Location`.
pub struct BlobUpload {
    repository: Repository,
    location: Url,
    offset: u64,
    /// One past the furthest byte ever sent; a status may not acknowledge more.
    sent: u64,
    uuid: Option<String>,
    needs_status: bool,
    finished: bool,
}

/// The registry's acknowledgement.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct UploadProgress {
    /// Bytes the registry holds: the next chunk starts here.
    pub offset: u64,
    /// The response metadata.
    pub meta: ResponseMeta,
}

/// A completed upload.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct BlobPushed {
    /// The blob's digest (the registry's `Docker-Content-Digest` agreed, if sent).
    pub digest: Digest,
    /// The blob's length, as uploaded through this session.
    pub size: u64,
    /// The response metadata (`Location: /v2/<name>/blobs/<digest>` among the headers).
    pub meta: ResponseMeta,
}

impl Repository {
    /// Starts an upload: `POST /v2/<name>/blobs/uploads/` with an empty body; `202 Accepted` and
    /// a `Location` are required.
    ///
    /// # Errors
    ///
    /// A [`RegistryError`].
    pub async fn start_upload(&self) -> Result<BlobUpload, RegistryError> {
        let api = self.api.clone();
        let received = self
            .registry
            .execute(&self.push(), |auth| {
                api.clone()
                    .maybe_authorization(auth)
                    .start_upload(Bytes::new())
                    .prepare()
            })
            .await?;
        let url = received.url().clone();
        let (received, meta) = expect_status(received, StatusCode::ACCEPTED).await?;
        let location = self
            .registry
            .location(&url, &meta)?
            .ok_or_else(|| missing_location(&meta))?;
        let offset = acknowledged(meta.headers())
            .map_err(|problem| RegistryError::protocol(problem, Some(meta.clone())))?
            .unwrap_or(0);
        if offset != 0 {
            return Err(RegistryError::protocol(
                ProtocolProblem::UnexpectedRange {
                    expected: None,
                    reported: offset,
                },
                Some(meta),
            ));
        }
        let uuid = meta.upload_uuid().map(str::to_owned);
        drain(received).await?;
        Ok(BlobUpload {
            repository: self.clone(),
            location,
            offset: 0,
            sent: 0,
            uuid,
            needs_status: false,
            finished: false,
        })
    }

    /// Uploads `data` as one blob: a start, `PATCH` chunks of `chunk_size` bytes, then a
    /// completing `PUT` with its sha256 digest (a blob no larger than one chunk is sent with the
    /// `PUT`). It stops at the first failure and drops the session; use
    /// [`start_upload`](Self::start_upload) to resume after an interruption.
    ///
    /// # Errors
    ///
    /// [`InputError::ChunkSize`] for a chunk size of 1 with more than one chunk; then a
    /// [`RegistryError`].
    pub async fn push_blob(
        &self,
        data: Bytes,
        chunk_size: NonZeroUsize,
    ) -> Result<BlobPushed, RegistryError> {
        let digest = Digest::sha256_of(&data);
        if data.len() > chunk_size.get() && chunk_size.get() < 2 {
            return Err(InputError::ChunkSize.into());
        }
        let mut upload = self.start_upload().await?;
        if data.len() <= chunk_size.get() {
            return upload.complete(&digest, Some(data)).await;
        }
        while let Some(start) = usize::try_from(upload.offset())
            .ok()
            .filter(|start| *start < data.len())
        {
            let end = data.len().min(start + chunk_size.get());
            upload.patch(data.slice(start..end)).await?;
        }
        upload.complete(&digest, None).await
    }
}

impl BlobUpload {
    /// Bytes the registry acknowledged: where the next chunk starts.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Whether the session's state is uncertain and [`status`](Self::status) must run before
    /// another chunk or the completion.
    pub fn needs_status(&self) -> bool {
        self.needs_status
    }

    /// Whether the upload completed.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// The latest `Location`, **unredacted** (it can be an upload capability: do not log it).
    pub fn location(&self) -> &Url {
        &self.location
    }

    /// The first `Docker-Upload-UUID` the registry sent (correlation only).
    pub fn uuid(&self) -> Option<&str> {
        self.uuid.as_deref()
    }

    /// `PATCH <Location>` with `chunk` at [`offset`](Self::offset): `Content-Range:
    /// <offset>-<offset + len - 1>`, `Content-Type: application/octet-stream`. `202 Accepted`
    /// with a `Location` and a `Range` acknowledging exactly this chunk is required; the session
    /// then moves to the new `Location`.
    ///
    /// # Errors
    ///
    /// [`InputError::EmptyChunk`], [`InputError::OneByteFirstChunk`],
    /// [`InputError::StatusRequired`], [`InputError::UploadFinished`] (nothing sent); then a
    /// [`RegistryError`] (a `416` is `Api`, with the session moved to the reported range).
    pub async fn patch(&mut self, chunk: Bytes) -> Result<UploadProgress, RegistryError> {
        self.ready()?;
        if chunk.is_empty() {
            return Err(InputError::EmptyChunk.into());
        }
        if self.offset == 0 && chunk.len() == 1 {
            return Err(InputError::OneByteFirstChunk.into());
        }
        let start = self.offset;
        let last = start + chunk.len() as u64 - 1;
        let range = format!("{start}-{last}");
        self.needs_status = true;
        self.sent = self.sent.max(last + 1);
        let api = self.repository.api.clone();
        let location = self.location.clone();
        let received = self
            .repository
            .registry
            .execute(&self.repository.push(), |auth| {
                api.clone()
                    .maybe_authorization(auth)
                    .upload_chunk(range.clone(), chunk.clone())
                    .prepare()?
                    .follow(location.as_str())
            })
            .await?;
        let url = received.url().clone();
        if received.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            let meta = ResponseMeta::capture(&received);
            let reported = self.adopt(&url, &meta).ok().flatten();
            if let Some((location, offset)) = reported
                && offset <= self.sent
            {
                self.location = location;
                self.offset = offset;
                self.needs_status = false;
            }
            return Err(api_error(received).await);
        }
        let (received, meta) = expect_status(received, StatusCode::ACCEPTED).await?;
        let (location, offset) = self
            .adopt(&url, &meta)?
            .ok_or_else(|| missing_location(&meta))?;
        if offset != last + 1 {
            return Err(RegistryError::protocol(
                ProtocolProblem::UnexpectedRange {
                    expected: Some(last),
                    reported: offset,
                },
                Some(meta),
            ));
        }
        drain(received).await?;
        self.location = location;
        self.offset = offset;
        self.needs_status = false;
        Ok(UploadProgress { offset, meta })
    }

    /// `GET <Location>`: the registry's authoritative progress (`204 No Content` with `Range`; a
    /// `Location` there, if sent, becomes the session's). Clears
    /// [`needs_status`](Self::needs_status).
    ///
    /// # Errors
    ///
    /// [`InputError::UploadFinished`]; then a [`RegistryError`] (`404 BLOB_UPLOAD_UNKNOWN`: the
    /// upload is gone and must be restarted; `Protocol` for a range beyond the bytes sent).
    pub async fn status(&mut self) -> Result<UploadProgress, RegistryError> {
        if self.finished {
            return Err(InputError::UploadFinished.into());
        }
        let api = self.repository.api.clone();
        let location = self.location.clone();
        let received = self
            .repository
            .registry
            .execute(&self.repository.push(), |auth| {
                api.clone()
                    .maybe_authorization(auth)
                    .upload_status()
                    .prepare()?
                    .follow(location.as_str())
            })
            .await?;
        let url = received.url().clone();
        let (received, meta) = expect_status(received, StatusCode::NO_CONTENT).await?;
        let new_location = self.repository.registry.location(&url, &meta)?;
        let offset = acknowledged(meta.headers())
            .map_err(|problem| RegistryError::protocol(problem, Some(meta.clone())))?
            .ok_or_else(|| {
                RegistryError::protocol(ProtocolProblem::MissingRange, Some(meta.clone()))
            })?;
        if offset > self.sent {
            return Err(RegistryError::protocol(
                ProtocolProblem::UnexpectedRange {
                    expected: self.sent.checked_sub(1),
                    reported: offset,
                },
                Some(meta),
            ));
        }
        drain(received).await?;
        if let Some(location) = new_location {
            self.location = location;
        }
        self.offset = offset;
        self.needs_status = false;
        Ok(UploadProgress { offset, meta })
    }

    /// `PUT <Location>&digest=<digest>`, with `last` as the final chunk (or an empty body when
    /// every byte was sent by `PATCH`). `201 Created` is required; a `Docker-Content-Digest` with
    /// the same algorithm must equal `digest`.
    ///
    /// # Errors
    ///
    /// [`InputError::StatusRequired`], [`InputError::UploadFinished`] (nothing sent); then a
    /// [`RegistryError`] (`400 DIGEST_INVALID` when the uploaded bytes have another digest; the
    /// session then needs a status query, and the spec says such an upload must be restarted).
    pub async fn complete(
        &mut self,
        digest: &Digest,
        last: Option<Bytes>,
    ) -> Result<BlobPushed, RegistryError> {
        self.ready()?;
        let body = last.unwrap_or_default();
        let mut url = self.location.clone();
        url.query_pairs_mut().append_pair("digest", digest.as_str());
        self.needs_status = true;
        self.sent = self.sent.max(self.offset + body.len() as u64);
        let api = self.repository.api.clone();
        let received = self
            .repository
            .registry
            .execute(&self.repository.push(), |auth| {
                api.clone()
                    .maybe_authorization(auth)
                    .complete_upload(body.clone())
                    .prepare()?
                    .follow(url.as_str())
            })
            .await?;
        let (received, meta) = expect_status(received, StatusCode::CREATED).await?;
        let header = meta
            .digest_header()
            .map_err(|problem| RegistryError::protocol(problem, Some(meta.clone())))?;
        drain(received).await?;
        self.finished = true;
        self.needs_status = false;
        self.offset += body.len() as u64;
        if let Some(header) = header
            && header.algorithm() == digest.algorithm()
            && header != *digest
        {
            return Err(RegistryError::integrity(
                IntegrityProblem::HeaderDigestMismatch {
                    expected: digest.clone(),
                    header,
                },
                Some(meta),
            ));
        }
        Ok(BlobPushed {
            digest: digest.clone(),
            size: self.offset,
            meta,
        })
    }

    fn ready(&self) -> Result<(), InputError> {
        if self.finished {
            return Err(InputError::UploadFinished);
        }
        if self.needs_status {
            return Err(InputError::StatusRequired);
        }
        Ok(())
    }

    /// The `Location` and acknowledged offset of an answer (`None` without a `Location`).
    fn adopt(&self, url: &Url, meta: &ResponseMeta) -> Result<Option<(Url, u64)>, RegistryError> {
        let Some(location) = self.repository.registry.location(url, meta)? else {
            return Ok(None);
        };
        let offset = acknowledged(meta.headers())
            .map_err(|problem| RegistryError::protocol(problem, Some(meta.clone())))?
            .ok_or_else(|| {
                RegistryError::protocol(ProtocolProblem::MissingRange, Some(meta.clone()))
            })?;
        Ok(Some((location, offset)))
    }
}

impl fmt::Debug for BlobUpload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlobUpload")
            .field("repository", self.repository.name())
            .field("offset", &self.offset)
            .field("sent", &self.sent)
            .field("uuid", &self.uuid)
            .field("needs_status", &self.needs_status)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

/// The bytes a `Range: [bytes=]0-<end>` acknowledges (`end + 1`; `0-0` is 0, see the module
/// documentation), `Ok(None)` without the header.
fn acknowledged(headers: &HeaderMap) -> Result<Option<u64>, ProtocolProblem> {
    let Some(value) =
        concord::field::single(headers, RANGE).map_err(|_| ProtocolProblem::MalformedRange)?
    else {
        return Ok(None);
    };
    let text = value
        .to_str()
        .map_err(|_| ProtocolProblem::MalformedRange)?
        .trim();
    let range = text
        .get(..6)
        .filter(|prefix| prefix.eq_ignore_ascii_case("bytes="))
        .map_or(text, |_| &text[6..]);
    let (start, end) = range
        .split_once('-')
        .ok_or(ProtocolProblem::MalformedRange)?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if start != "0" || !digits(end) {
        return Err(ProtocolProblem::MalformedRange);
    }
    let end: u64 = end.parse().map_err(|_| ProtocolProblem::MalformedRange)?;
    if end == 0 {
        return Ok(Some(0));
    }
    end.checked_add(1)
        .map(Some)
        .ok_or(ProtocolProblem::MalformedRange)
}

#[cfg(test)]
mod tests {
    use concord::reqwest::header::HeaderValue;

    use super::*;

    fn range(values: &[&str]) -> Result<Option<u64>, ProtocolProblem> {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(RANGE, HeaderValue::from_str(value).unwrap());
        }
        acknowledged(&headers)
    }

    #[test]
    fn inclusive_ranges_with_or_without_the_unit() {
        assert_eq!(range(&[]), Ok(None));
        assert_eq!(range(&["0-0"]), Ok(Some(0)));
        assert_eq!(range(&["bytes=0-0"]), Ok(Some(0)));
        assert_eq!(range(&["0-4"]), Ok(Some(5)));
        assert_eq!(range(&["bytes=0-1023"]), Ok(Some(1024)));
        assert_eq!(range(&[" BYTES=0-9 "]), Ok(Some(10)));
        for bad in [
            "1-4",
            "0-",
            "-4",
            "0-4/10",
            "bytes 0-4",
            "0-x",
            "0--1",
            "0-18446744073709551615",
        ] {
            assert_eq!(range(&[bad]), Err(ProtocolProblem::MalformedRange), "{bad}");
        }
        assert_eq!(range(&["0-1", "0-2"]), Err(ProtocolProblem::MalformedRange));
    }
}
