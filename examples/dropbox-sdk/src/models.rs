//! Folder request and response types.

use crate::{InputError, RequestContext};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Approximate provider page size, in the documented range 1–2000.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PageSize(u16);

impl PageSize {
    /// Validate a page size. Dropbox may return slightly more entries than this limit.
    pub fn new(value: u16) -> Result<Self, InputError> {
        if (1..=2000).contains(&value) {
            Ok(Self(value))
        } else {
            Err(InputError::PageSize)
        }
    }

    /// Numeric value sent to Dropbox.
    pub fn get(self) -> u16 {
        self.0
    }
}

/// Options for starting a folder listing; continuation bodies contain only the cursor.
#[derive(Clone, Debug, Serialize)]
pub struct ListFolderOptions {
    /// Folder path relative to the selected root. Use `""` for that root, not `"/"`.
    /// Dropbox also accepts documented `id:` / `ns:` references; it validates path syntax.
    pub path: String,
    /// Include descendants. Default false.
    pub recursive: bool,
    /// Include deletion records. Default false.
    pub include_deleted: bool,
    /// Traverse mounted folders. Default true.
    pub include_mounted_folders: bool,
    /// Include non-downloadable files. Default true.
    pub include_non_downloadable_files: bool,
    /// Approximate page size; absent uses the provider default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<PageSize>,
}

impl ListFolderOptions {
    /// Use Dropbox's defaults for all options and the supplied raw path.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            recursive: false,
            include_deleted: false,
            include_mounted_folders: true,
            include_non_downloadable_files: true,
            limit: None,
        }
    }
}

impl Default for ListFolderOptions {
    fn default() -> Self {
        Self::new("")
    }
}

/// Metadata shared by files, folders and deletion records.
#[derive(Clone, Debug, Deserialize)]
pub struct EntryPath {
    /// Last path component, including extension.
    pub name: String,
    /// Display path relative to the selected root; may be absent for unmounted entries.
    pub path_display: Option<String>,
    /// Lowercase path relative to the selected root; may be absent for unmounted entries.
    pub path_lower: Option<String>,
}

/// File metadata needed for folder inventory. Additional provider fields are ignored.
#[derive(Clone, Debug, Deserialize)]
pub struct FileMetadata {
    /// Name and optional paths.
    #[serde(flatten)]
    pub path: EntryPath,
    /// Stable file identifier.
    pub id: String,
    /// Revision identifier.
    pub rev: String,
    /// File size in bytes.
    pub size: u64,
    /// Client-supplied modification timestamp, for display only.
    pub client_modified: String,
    /// Dropbox server modification timestamp.
    pub server_modified: String,
    /// Whether direct download is supported (no download operation is exposed here).
    #[serde(default = "default_true")]
    pub is_downloadable: bool,
    /// Dropbox content hash, when supplied.
    pub content_hash: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Folder metadata needed for inventory.
#[derive(Clone, Debug, Deserialize)]
pub struct FolderMetadata {
    /// Name and optional paths.
    #[serde(flatten)]
    pub path: EntryPath,
    /// Stable folder identifier.
    pub id: String,
}

/// A typed folder entry. Unknown future tags fail decoding rather than silently losing entries.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = ".tag", rename_all = "snake_case")]
pub enum Entry {
    /// A file.
    File(FileMetadata),
    /// A folder.
    Folder(FolderMetadata),
    /// A deletion record, which has no file/folder ID.
    Deleted(EntryPath),
}

impl Entry {
    /// Name and root-relative paths, regardless of entry kind.
    pub fn path(&self) -> &EntryPath {
        match self {
            Self::File(v) => &v.path,
            Self::Folder(v) => &v.path,
            Self::Deleted(v) => v,
        }
    }
}

/// An opaque continuation bound to the member/root that produced it.
/// Only successful responses create cursors. Do not persist or log the raw value.
#[derive(Clone, PartialEq, Eq)]
pub struct Cursor {
    pub(crate) value: String,
    pub(crate) context: RequestContext,
}

impl Cursor {
    /// Exact provider value, without escaping or normalization.
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Context that every continuation request will reuse.
    pub fn context(&self) -> &RequestContext {
        &self.context
    }
}

impl fmt::Debug for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Cursor(<redacted>)")
    }
}

/// One complete JSON page; an empty entries array alone does not establish completion.
#[derive(Debug)]
pub struct FolderPage {
    /// Entries in provider order.
    pub entries: Vec<Entry>,
    /// Continuation bound to this page's member/root context, retained even on the last page.
    pub cursor: Cursor,
    /// Only false indicates provider exhaustion at the time of this response.
    pub has_more: bool,
}
