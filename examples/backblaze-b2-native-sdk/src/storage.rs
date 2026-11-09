//! Account-authorized file-name traversal and single-attempt version deletion.

use std::{fmt, num::NonZeroUsize};

use concord::{Delivery, ResponseMeta, reqwest::StatusCode};

use crate::wire::DeleteFileVersionRequest;
use crate::{BucketId, Error, FileId, FileName, InputError, Session, Violation};

/// The parameters of `b2_list_file_names`, including an optional opaque resume name.
/// A traversal owns a copy: its filters stay fixed across pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileNameQuery {
    /// Bucket to list (the key needs `listFiles` for it).
    pub bucket_id: BucketId,
    /// Inclusive starting name; use the returned continuation verbatim, even if no file has it.
    pub start_file_name: Option<String>,
    /// Literal prefix; must respect any prefix restriction on the application key.
    pub prefix: Option<String>,
    /// Folder delimiter, usually `/`; folder entries have no file ID or SHA-1.
    pub delimiter: Option<String>,
    /// Per-page count: omitted or zero uses B2's default (100); maximum 10,000.
    pub max_file_count: Option<u16>,
}

impl FileNameQuery {
    /// Starts at the beginning, with B2's default filters and count.
    pub fn new(bucket_id: BucketId) -> Self {
        Self {
            bucket_id,
            start_file_name: None,
            prefix: None,
            delimiter: None,
            max_file_count: None,
        }
    }
}

/// A file or virtual folder from a name listing. This lists visible names, not all versions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedFile {
    /// `upload`, `folder`, or a future provider action (kept verbatim).
    pub action: String,
    /// Unencoded `fileName`.
    pub file_name: String,
    /// Version ID; `None` for a virtual folder.
    pub file_id: Option<FileId>,
    /// Stored byte count (zero for a folder).
    pub content_length: u64,
    /// Provider checksum text: null for folders, possibly `none`; no bytes were verified here.
    pub content_sha1: Option<String>,
}

/// One bounded listing response, with its head for diagnostics.
#[derive(Debug)]
pub struct FileNamePage {
    /// Files and virtual folders returned by B2.
    pub files: Vec<ListedFile>,
    /// Opaque next starting name; only `None` proves the traversal complete.
    pub next_file_name: Option<String>,
    /// The listing's response head.
    pub meta: ResponseMeta,
}

/// The storage scope, bound to the authorization's API URL and account token.
#[derive(Clone, Copy)]
pub struct Storage<'a> {
    session: &'a Session,
}

impl<'a> Storage<'a> {
    pub(crate) fn new(session: &'a Session) -> Self {
        Self { session }
    }

    /// Creates a traversal with at most `page_cap` requests. No request is sent yet.
    /// Each `next_page` sends once; errors are never retried automatically.
    /// Each GET sends the account token as `authorization` and the declared `Json` output's
    /// `accept: application/json`; no body means no format-added `content-type`.
    pub fn list_file_names(
        &self,
        query: FileNameQuery,
        page_cap: NonZeroUsize,
    ) -> FileNameTraversal<'a> {
        FileNameTraversal {
            session: self.session,
            query,
            remaining: page_cap.get(),
            complete: false,
        }
    }

    /// Deletes exactly the specified name/ID pair with one POST, using the account token
    /// (`deleteFiles` capability). Never retries or reauthorizes automatically.
    /// Sends the account token as `authorization`; the declared `Json` request/output formats
    /// add `content-type: application/json` and `accept: application/json`.
    ///
    /// # Errors
    ///
    /// A mismatched pair is B2's typed `ErrorCode::FileNotPresent` provider error. A timeout,
    /// lost answer, server failure or invalid success document may have deleted the version:
    /// inspect `DeleteError::may_have_deleted` and list again before deciding what to do.
    /// A name listing shows only current visible versions; it cannot prove an older version's
    /// absence or provide exactly-once deletion. Cancellation also leaves the outcome unknown.
    pub async fn delete_file_version(
        &self,
        file_name: &FileName,
        file_id: &FileId,
    ) -> Result<DeletedFileVersion, DeleteError> {
        let request = DeleteFileVersionRequest {
            file_name: file_name.as_str().to_owned(),
            file_id: file_id.as_str().to_owned(),
        };
        let (wire, meta) = self
            .session
            .storage
            .delete_file_version(request)
            .send()
            .await
            .map_err(DeleteError::new)?;
        if wire.file_name != file_name.as_str() || wire.file_id != file_id.as_str() {
            return Err(DeleteError::new(Error::contract(
                Violation::DeletedFileVersion,
                Some(meta),
            )));
        }
        Ok(DeletedFileVersion {
            file_name: file_name.clone(),
            file_id: file_id.clone(),
            meta,
        })
    }
}

/// A bounded traversal. Successful pages advance the cursor; failures leave it unchanged.
/// The request budget also counts failed or cancelled page attempts.
pub struct FileNameTraversal<'a> {
    session: &'a Session,
    query: FileNameQuery,
    remaining: usize,
    complete: bool,
}

impl FileNameTraversal<'_> {
    /// Whether B2 returned `nextFileName: null`; an empty page or the cap does not complete it.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Requests still permitted by this traversal's cap.
    pub fn pages_remaining(&self) -> usize {
        self.remaining
    }

    /// A complete query for resuming with the same bucket, prefix, delimiter and count.
    /// Available before the first page, at the cap and after an error; `None` only on completion.
    pub fn resume_point(&self) -> Option<FileNameQuery> {
        (!self.complete).then(|| self.query.clone())
    }

    /// Sends one GET and advances only after a decoded, validated page. Returns `None` without
    /// sending when complete or capped; inspect `is_complete` and `resume_point` to distinguish.
    /// An empty page with a continuation must still be followed.
    ///
    /// # Errors
    ///
    /// `Error::Input` for a count above 10,000 (nothing sent), `Error::Provider` for a non-2xx,
    /// `Error::Concord` for transport/decoding/size failures, `Error::Contract` for invalid
    /// entry IDs, lengths or bucket. The cursor remains resumable; retry is the caller's choice.
    pub async fn next_page(&mut self) -> Result<Option<FileNamePage>, Error> {
        if self.complete || self.remaining == 0 {
            return Ok(None);
        }
        if self
            .query
            .max_file_count
            .is_some_and(|count| count > 10_000)
        {
            return Err(InputError::MaxFileCount.into());
        }
        self.remaining -= 1;
        let query = &self.query;
        let (wire, meta) = self
            .session
            .storage
            .list_file_names(query.bucket_id.as_str())
            .maybe_start_file_name(query.start_file_name.clone())
            .maybe_prefix(query.prefix.clone())
            .maybe_delimiter(query.delimiter.clone())
            .maybe_max_file_count(query.max_file_count)
            .send()
            .await?;
        let mut files = Vec::with_capacity(wire.files.len());
        for file in wire.files {
            let invalid = || Error::contract(Violation::ListedFile, Some(meta.clone()));
            if file.bucket_id != query.bucket_id.as_str() {
                return Err(invalid());
            }
            let file_id = file
                .file_id
                .map(FileId::new)
                .transpose()
                .map_err(|_| invalid())?;
            files.push(ListedFile {
                action: file.action,
                file_name: file.file_name,
                file_id,
                content_length: file.content_length.value().ok_or_else(invalid)?,
                content_sha1: file.content_sha1,
            });
        }
        self.complete = wire.next_file_name.is_none();
        self.query.start_file_name = wire.next_file_name.clone();
        Ok(Some(FileNamePage {
            files,
            next_file_name: wire.next_file_name,
            meta,
        }))
    }
}

/// A confirmed deletion whose response identifies the requested version.
#[derive(Debug)]
pub struct DeletedFileVersion {
    /// Deleted name.
    pub file_name: FileName,
    /// Deleted version ID.
    pub file_id: FileId,
    /// The delete's response head.
    pub meta: ResponseMeta,
}

/// A single failed delete, preserving uncertainty and the underlying typed failure.
#[derive(Debug)]
pub struct DeleteError {
    error: Error,
    may_have_deleted: bool,
}

impl DeleteError {
    fn new(error: Error) -> Self {
        let may_have_deleted = match &error {
            Error::Input(_) => false,
            Error::Concord(error) => error.delivery() != Delivery::NotSent,
            Error::Provider(error) => {
                !error.status().is_client_error() || error.status() == StatusCode::REQUEST_TIMEOUT
            }
            _ => true,
        };
        Self {
            error,
            may_have_deleted,
        }
    }

    /// Transport/contract failure or the typed provider error (including `file_not_present`).
    pub fn error(&self) -> &Error {
        &self.error
    }

    /// Whether this attempt may have deleted the version. If true, list again; do not blindly
    /// resend. False means unsent or a 4xx refusal other than 408, not that the version exists.
    pub fn may_have_deleted(&self) -> bool {
        self.may_have_deleted
    }
}

impl fmt::Display for DeleteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "B2 version delete failed (may have deleted: {}): {}",
            self.may_have_deleted, self.error
        )
    }
}

impl std::error::Error for DeleteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
