use std::fmt;

use concord::multipart::{MultipartBody, Part};
use concord::reqwest::{
    StatusCode, Url,
    header::{HeaderMap, HeaderValue, LOCATION, RANGE},
};
use concord::{EncodedBody, Received};
use serde::{Deserialize, Serialize};

use crate::{DriveClient, Error, File, FileFields, NewFile, Response, ResponseMeta};

/// Google requires non-final chunks to be multiples of 256 KiB.
pub const CHUNK_ALIGNMENT: usize = 256 * 1024;
const MULTIPART_LIMIT: usize = 5 * 1024 * 1024;

/// Persistable upload state. Contains the session URL (a sensitive capability), no bearer token.
/// Use protected storage. Restoring always requires a status probe before sending bytes.
#[derive(Serialize, Deserialize)]
pub struct UploadCheckpoint {
    url: String,
    total: u64,
    media_type: String,
    acknowledged: u64,
}

impl fmt::Debug for UploadCheckpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadCheckpoint")
            .field("total", &self.total)
            .field("acknowledged", &self.acknowledged)
            .finish_non_exhaustive()
    }
}

/// A resumable session bound to one client origin and one source length/media type.
/// Not Clone: share through application synchronization if necessary.
pub struct UploadSession {
    client: DriveClient,
    url: Url,
    total: u64,
    media_type: HeaderValue,
    acknowledged: u64,
    needs_probe: bool,
    complete: bool,
}

impl fmt::Debug for UploadSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadSession")
            .field("total", &self.total)
            .field("acknowledged", &self.acknowledged)
            .field("needs_probe", &self.needs_probe)
            .field("complete", &self.complete)
            .finish_non_exhaustive()
    }
}

/// An acknowledged byte prefix, or completed metadata. HTTP 308 is successful progress here.
#[derive(Debug)]
pub enum UploadStatus {
    /// Resume at this zero-based offset; do not infer progress from the chunk length sent.
    Incomplete {
        /// Number of confirmed bytes.
        acknowledged: u64,
        /// The 308 head, including its Range header when present.
        meta: ResponseMeta,
    },
    /// Server returned HTTP 200/201 and valid selected metadata.
    Complete(Response<File>),
}

impl DriveClient {
    /// Upload at most 5 MiB of media with metadata as multipart/related.
    /// The boundary is chosen by `concord::multipart` after scanning both parts, so
    /// boundary-like bytes survive exactly.
    /// A lost response is an uncertain create; nothing is automatically replayed.
    pub async fn upload_multipart(
        &self,
        metadata: NewFile,
        media_type: &str,
        bytes: &[u8],
        fields: &FileFields,
    ) -> Result<Response<File>, Error> {
        if bytes.len() > MULTIPART_LIMIT {
            return Err(Error::invalid("multipart media exceeds 5 MiB"));
        }
        let media = media_header(media_type)?;
        let json = serde_json::to_vec(&metadata)
            .map_err(|_| Error::invalid("metadata could not be serialized"))?;
        let payload = MultipartBody::related()
            .part(Part::new(json).header("Content-Type", "application/json; charset=UTF-8"))
            .part(Part::new(bytes.to_vec()).header(
                "Content-Type",
                media.to_str().expect("validated MIME is ASCII"),
            ));
        let prepared = self
            .api
            .uploads()
            .multipart(payload, fields.wire())
            .prepare()?;
        self.finish(self.receive(prepared).await?).await
    }

    /// Start a resumable create. The nonzero total is fixed for the life of the session.
    /// The returned Location must be an absolute URL at the same origin and upload path.
    pub async fn initiate_upload(
        &self,
        metadata: NewFile,
        media_type: &str,
        total: u64,
        fields: &FileFields,
    ) -> Result<Response<UploadSession>, Error> {
        if total == 0 {
            return Err(Error::invalid("use multipart for an empty file"));
        }
        let media = media_header(media_type)?;
        let prepared = self
            .api
            .uploads()
            .initiate(metadata, fields.wire(), media_type.to_owned(), total)
            .prepare()?;
        let received = self.receive(prepared).await?;
        if !received.status().is_success() {
            return Err(
                Error::provider_response(received, self.config.max_error_bytes, false).await,
            );
        }
        let meta = received.meta();
        if meta.status() != StatusCode::OK {
            return Err(Error::protocol(
                "resumable initiation must return HTTP 200",
                Some(meta),
            ));
        }
        let location = single_header(meta.headers(), LOCATION.as_str())
            .map_err(|message| Error::protocol(message, Some(meta.clone())))?
            .ok_or_else(|| Error::protocol("session Location is missing", Some(meta.clone())))?;
        let url = self
            .session_url(location)
            .map_err(|error| error.with_meta(meta.clone()))?;
        // Initiation is documented as empty: bound and check it instead of draining arbitrarily.
        received
            .bytes_limited(0)
            .await
            .map_err(|error| Error::from(error).with_meta(meta.clone()))?;
        Ok(Response {
            value: UploadSession {
                client: self.clone(),
                url,
                total,
                media_type: media,
                acknowledged: 0,
                needs_probe: false,
                complete: false,
            },
            meta,
        })
    }

    /// Restore a checkpoint under this client's origin and credentials. A probe is mandatory.
    /// The application must supply the same source bytes; this SDK does not hash the source.
    pub fn restore_upload(&self, checkpoint: UploadCheckpoint) -> Result<UploadSession, Error> {
        let url = self.session_url(&checkpoint.url)?;
        if checkpoint.total == 0 || checkpoint.acknowledged > checkpoint.total {
            return Err(Error::invalid("invalid upload checkpoint byte counts"));
        }
        Ok(UploadSession {
            client: self.clone(),
            url,
            total: checkpoint.total,
            media_type: media_header(&checkpoint.media_type)?,
            acknowledged: checkpoint.acknowledged,
            needs_probe: true,
            complete: false,
        })
    }

    fn session_url(&self, location: &str) -> Result<Url, Error> {
        let url = Url::parse(location)
            .map_err(|_| Error::protocol("session Location must be absolute", None))?;
        if url.origin() != self.origin.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.path() != "/upload/drive/v3/files"
        {
            return Err(Error::protocol(
                "session Location violates origin or upload-path policy",
                None,
            ));
        }
        Ok(url)
    }
}

impl UploadSession {
    /// The exact returned session URL, unredacted; avoid logging it.
    pub fn session_url(&self) -> &Url {
        &self.url
    }
    /// Fixed total file size in bytes.
    pub fn total(&self) -> u64 {
        self.total
    }
    /// Confirmed prefix length; the next chunk starts here.
    pub fn acknowledged(&self) -> u64 {
        self.acknowledged
    }
    /// True after a cancelled/failed/uncertain write or checkpoint restoration.
    pub fn needs_probe(&self) -> bool {
        self.needs_probe
    }
    /// Whether metadata confirmed upload completion.
    pub fn is_complete(&self) -> bool {
        self.complete
    }
    /// Capture protected persistence state, without credentials or source bytes.
    /// Save after an acknowledgement and before the next write. A restored checkpoint
    /// must be probed even after a clean stop: an interrupted write may have advanced
    /// the server beyond this saved prefix. This checkpoint has no changes-feed token;
    /// persist that token too when the upload is part of a changes workflow.
    pub fn checkpoint(&self) -> UploadCheckpoint {
        UploadCheckpoint {
            url: self.url.as_str().to_owned(),
            total: self.total,
            media_type: self.media_type.to_str().expect("validated MIME").to_owned(),
            acknowledged: self.acknowledged,
        }
    }

    /// Send one bounded chunk beginning at the acknowledged position.
    /// Non-final lengths must be multiples of 256 KiB. No retry or automatic status request.
    /// The acknowledged offset itself need not be aligned; slice the original bytes at
    /// that exact offset after every response or probe, including partial acknowledgements.
    /// Cancellation after the first poll leaves `needs_probe=true`.
    pub async fn send_chunk(&mut self, bytes: &[u8]) -> Result<UploadStatus, Error> {
        if self.complete {
            return Err(Error::invalid("upload is already complete"));
        }
        if self.needs_probe {
            return Err(Error::invalid(
                "probe the session before sending more bytes",
            ));
        }
        let end = self
            .acknowledged
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| Error::invalid("chunk length overflow"))?;
        if bytes.is_empty()
            || bytes.len() > self.client.config.max_chunk_bytes
            || end > self.total
            || (end != self.total && !bytes.len().is_multiple_of(CHUNK_ALIGNMENT))
        {
            return Err(Error::invalid(
                "chunk must fit the remaining file and bound; non-final length must be a multiple of 256 KiB",
            ));
        }
        let range = format!("bytes {}-{}/{}", self.acknowledged, end - 1, self.total);
        let payload =
            EncodedBody::with_content_type(bytes.to_vec().into(), self.media_type.clone());
        let prepared = self
            .client
            .api
            .uploads()
            .transfer(payload, range)
            .prepare()?
            .follow(self.url.as_str())?;
        self.needs_probe = true;
        let received = self.client.receive(prepared).await?;
        self.accept(received, end, false).await
    }

    /// Query authoritative server progress with empty PUT and `Content-Range: bytes */total`.
    /// A missing Range on 308 means zero bytes. A 404 is [`crate::ErrorKind::SessionExpired`].
    /// A 200/201 may report that an uncertain final chunk actually completed.
    pub async fn probe(&mut self) -> Result<UploadStatus, Error> {
        if self.complete {
            return Err(Error::invalid("upload is already complete"));
        }
        let payload = EncodedBody::new(Vec::new().into());
        let prepared = self
            .client
            .api
            .uploads()
            .transfer(payload, format!("bytes */{}", self.total))
            .prepare()?
            .follow(self.url.as_str())?;
        self.needs_probe = true;
        let received = self.client.receive(prepared).await?;
        self.accept(received, self.total, true).await
    }

    async fn accept(
        &mut self,
        received: Received<File>,
        max_ack: u64,
        probing: bool,
    ) -> Result<UploadStatus, Error> {
        let meta = received.meta();
        if meta.status().as_u16() == 308 {
            let range = single_header(meta.headers(), RANGE.as_str())
                .map_err(|message| Error::protocol(message, Some(meta.clone())))?;
            let acknowledged = match range {
                None => 0,
                Some(value) => parse_range(value).ok_or_else(|| {
                    Error::protocol(
                        "invalid resumable Range acknowledgement",
                        Some(meta.clone()),
                    )
                })?,
            };
            if acknowledged < self.acknowledged
                || acknowledged > max_ack
                || acknowledged >= self.total
            {
                return Err(Error::protocol(
                    "resumable Range regressed or exceeded the permitted prefix",
                    Some(meta),
                ));
            }
            // Range defines progress; the documented 308 body carries no file metadata.
            received
                .bytes_limited(self.client.config.max_response_bytes)
                .await
                .map_err(|error| Error::from(error).with_meta(meta.clone()))?;
            self.acknowledged = acknowledged;
            self.needs_probe = false;
            return Ok(UploadStatus::Incomplete { acknowledged, meta });
        }
        if meta.status() == StatusCode::OK || meta.status() == StatusCode::CREATED {
            if !probing && max_ack != self.total {
                return Err(Error::protocol(
                    "upload completed before the final chunk",
                    Some(meta),
                ));
            }
            let response = self.client.finish(received).await?;
            self.acknowledged = self.total;
            self.needs_probe = false;
            self.complete = true;
            return Ok(UploadStatus::Complete(response));
        }
        if meta.status().is_success() {
            return Err(Error::protocol(
                "unexpected resumable success status",
                Some(meta),
            ));
        }
        Err(Error::provider_response(received, self.client.config.max_error_bytes, true).await)
    }
}

fn media_header(value: &str) -> Result<HeaderValue, Error> {
    let mime = value
        .parse::<mime::Mime>()
        .map_err(|_| Error::invalid("invalid upload MIME type"))?;
    if mime.type_() == mime::STAR
        || mime.subtype() == mime::STAR
        || value.bytes().any(|b| !b.is_ascii_graphic() && b != b' ')
    {
        return Err(Error::invalid(
            "upload MIME type must be concrete and contain no control characters",
        ));
    }
    HeaderValue::try_from(value).map_err(|_| Error::invalid("invalid upload MIME header"))
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, &'static str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next();
    if values.next().is_some() {
        return Err("ambiguous repeated session header");
    }
    value
        .map(|value| value.to_str().map_err(|_| "non-text session header"))
        .transpose()
}

fn parse_range(value: &str) -> Option<u64> {
    let last = value.strip_prefix("bytes=0-")?;
    if last.is_empty() || !last.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    last.parse::<u64>().ok()?.checked_add(1)
}
