//! `b2_download_file_by_id`: a bounded download, verified against `X-Bz-Content-Sha1`.

use std::collections::BTreeMap;
use std::fmt;

use concord::bytes::Bytes;
use concord::reqwest::StatusCode;
use concord::reqwest::header::HeaderMap;
use concord::{ResponseMeta, field};
use percent_encoding::percent_decode_str;

use crate::Session;
use crate::error::{ChecksumError, ChecksumFailure, Error, Violation};
use crate::model::{FileId, Sha1Hex};
use crate::wire::provider_error;

/// What the download's headers say about the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadedFile {
    /// `X-Bz-File-Id` (equal to the requested ID).
    pub file_id: FileId,
    /// `X-Bz-Content-Sha1`, which the body matched.
    pub content_sha1: Sha1Hex,
    /// The number of bytes received.
    pub content_length: u64,
    /// `X-Bz-File-Name`, percent-decoded; `None` when absent, repeated or not UTF-8.
    pub file_name: Option<String>,
    /// `Content-Type`, when a single valid one was sent.
    pub content_type: Option<String>,
    /// `X-Bz-Upload-Timestamp` (milliseconds since the epoch), when a single valid one was sent.
    pub upload_timestamp: Option<u64>,
    /// `X-Bz-Info-*` headers: lowercase name after the prefix, percent-decoded value (pairs that
    /// do not decode are left out).
    pub info: BTreeMap<String, String>,
}

/// A verified download.
pub struct Download {
    /// The file's bytes; their SHA-1 equals `X-Bz-Content-Sha1`.
    pub bytes: Bytes,
    /// What the headers said.
    pub file: DownloadedFile,
    /// The response head.
    pub meta: ResponseMeta,
}

impl fmt::Debug for Download {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Download")
            .field("bytes", &self.bytes.len())
            .field("file", &self.file)
            .field("meta", &self.meta)
            .finish()
    }
}

fn single_text<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    field::single(headers, name)
        .ok()
        .flatten()
        .and_then(|value| value.to_str().ok())
}

fn decoded(value: &str) -> Option<String> {
    percent_decode_str(value)
        .decode_utf8()
        .ok()
        .map(|text| text.into_owned())
}

impl Session {
    /// Downloads one file version by ID, reading at most `limit` body bytes, and verifies it:
    /// status `200`, `X-Bz-File-Id` equal to `file_id`, and the SHA-1 of the bytes equal to
    /// `X-Bz-Content-Sha1` (B2: "you should check the SHA1 of the data you receive"). No
    /// `Range` is sent, so the whole file is checked. The bytes are returned only once verified.
    ///
    /// # Errors
    ///
    /// [`Error::Provider`] for a non-2xx (with B2's error document), [`Error::Concord`] for no
    /// answer, a body that broke off or one longer than `limit` (`ErrorKind::Limit`),
    /// [`Error::Contract`] for another status or file ID, [`Error::Checksum`] when the bytes
    /// cannot be verified or do not match.
    pub async fn download_file_by_id(
        &self,
        file_id: &FileId,
        limit: usize,
    ) -> Result<Download, Error> {
        let received = self
            .download
            .download_file_by_id(file_id.as_str())
            .prepare()?
            .receive()
            .await?;
        if !received.status().is_success() {
            return Err(provider_error(received).await);
        }
        let (bytes, meta) = received.decode_limited_with_meta(limit).await?;
        if meta.status() != StatusCode::OK {
            return Err(Error::contract(Violation::DownloadStatus, Some(meta)));
        }
        let headers = meta.headers();
        if single_text(headers, "x-bz-file-id") != Some(file_id.as_str()) {
            return Err(Error::contract(Violation::DownloadFileId, Some(meta)));
        }
        let expected = match field::single(headers, "x-bz-content-sha1") {
            Ok(Some(value)) if value.as_bytes() == b"none" => Err(ChecksumFailure::Unavailable),
            Ok(Some(value)) => value
                .to_str()
                .ok()
                .and_then(|text| Sha1Hex::new(text).ok())
                .ok_or(ChecksumFailure::Malformed),
            Ok(None) | Err(_) => Err(ChecksumFailure::Missing),
        };
        let expected = match expected {
            Ok(expected) => expected,
            Err(failure) => {
                return Err(Error::Checksum(Box::new(ChecksumError::new(failure, meta))));
            }
        };
        let actual = Sha1Hex::of(&bytes);
        if actual != expected {
            let failure = ChecksumFailure::Mismatch {
                expected: expected.as_str().to_owned(),
                actual: actual.as_str().to_owned(),
            };
            return Err(Error::Checksum(Box::new(ChecksumError::new(failure, meta))));
        }
        let info = headers
            .iter()
            .filter_map(|(name, value)| {
                let key = name.as_str().strip_prefix("x-bz-info-")?;
                Some((key.to_owned(), decoded(value.to_str().ok()?)?))
            })
            .collect();
        let file = DownloadedFile {
            file_id: file_id.clone(),
            content_sha1: actual,
            content_length: bytes.len() as u64,
            file_name: single_text(headers, "x-bz-file-name").and_then(decoded),
            content_type: single_text(headers, "content-type").map(str::to_owned),
            upload_timestamp: field::unsigned(headers, "x-bz-upload-timestamp")
                .ok()
                .flatten(),
            info,
        };
        Ok(Download { bytes, file, meta })
    }
}
