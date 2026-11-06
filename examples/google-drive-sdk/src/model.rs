use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Error, ResponseMeta};

/// File metadata. Fields outside `id` may be absent after a partial-field request.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    /// Opaque file identifier (always requested by this SDK).
    pub id: String,
    /// Display name, including Unicode.
    pub name: Option<String>,
    /// Media type.
    pub mime_type: Option<String>,
    /// Byte length, encoded by Drive as a JSON string.
    pub size: Option<String>,
    /// User description.
    pub description: Option<String>,
    /// Parent folder identifiers.
    pub parents: Option<Vec<String>>,
    /// MD5 checksum where Drive supplies it.
    pub md5_checksum: Option<String>,
    /// RFC 3339 modification time, retained as sent.
    pub modified_time: Option<String>,
    /// Whether the file is in the trash.
    pub trashed: Option<bool>,
}

/// Writable metadata supported when creating a file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewFile {
    /// File name, preferably including its extension.
    /// Serialized as supplied, without Unicode normalization or MIME filename encoding.
    pub name: String,
    /// Optional explicit stored MIME type; use the media type supplied to the upload normally.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional user description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional parent folder identifiers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parents: Option<Vec<String>>,
}

/// A supported optional metadata projection; `id` is always selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileField {
    /// Display name.
    Name,
    /// MIME type.
    MimeType,
    /// Byte length.
    Size,
    /// Description.
    Description,
    /// Parent identifiers.
    Parents,
    /// MD5 checksum.
    Md5Checksum,
    /// Modification timestamp.
    ModifiedTime,
    /// Trash flag.
    Trashed,
}

impl FileField {
    fn wire(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::MimeType => "mimeType",
            Self::Size => "size",
            Self::Description => "description",
            Self::Parents => "parents",
            Self::Md5Checksum => "md5Checksum",
            Self::ModifiedTime => "modifiedTime",
            Self::Trashed => "trashed",
        }
    }
}

/// Validated partial-field selection. Paging control fields cannot be removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFields(String);

impl FileFields {
    /// Select `id` plus these optional fields, with duplicates removed.
    pub fn only(fields: &[FileField]) -> Self {
        let mut names = vec!["id"];
        for field in fields {
            if !names.contains(&field.wire()) {
                names.push(field.wire());
            }
        }
        Self(names.join(","))
    }

    pub(crate) fn wire(&self) -> String {
        self.0.clone()
    }
}

impl Default for FileFields {
    fn default() -> Self {
        Self::only(&[
            FileField::Name,
            FileField::MimeType,
            FileField::Size,
            FileField::Description,
            FileField::Parents,
            FileField::Md5Checksum,
            FileField::ModifiedTime,
            FileField::Trashed,
        ])
    }
}

/// Opaque files/changes cursor. Debug output withholds the value.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PageToken(String);

impl PageToken {
    /// Wrap a nonempty token without interpreting or normalizing its bytes.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::invalid("page token is empty"));
        }
        Ok(Self(value))
    }

    /// The opaque value, unredacted, for persistence.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PageToken {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::new(value)
    }
}

impl From<PageToken> for String {
    fn from(value: PageToken) -> Self {
        value.0
    }
}

impl fmt::Debug for PageToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PageToken(<redacted>)")
    }
}

/// Options for the supported user-scoped file listing (`spaces=drive`).
#[derive(Clone, Debug)]
pub struct ListOptions {
    /// Page size, 1–1000.
    pub page_size: u16,
    /// Drive's `q` expression, as supplied by the caller.
    pub query: Option<String>,
    /// Drive's `orderBy` expression.
    pub order_by: Option<String>,
    /// File metadata selection.
    pub fields: FileFields,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            page_size: 100,
            query: None,
            order_by: None,
            fields: FileFields::default(),
        }
    }
}

/// Files response with its unmodified continuation and search-completeness flag.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesPage {
    /// Files on this page (an empty nonterminal page is valid).
    #[serde(default)]
    pub files: Vec<File>,
    /// Continuation, absent at the last page.
    pub next_page_token: Option<PageToken>,
    /// True when Drive did not search all requested corpora.
    #[serde(default)]
    pub incomplete_search: bool,
}

/// A change-feed record; removed files need not carry metadata.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// Affected file identifier.
    pub file_id: String,
    /// Whether this file was removed from the change list.
    #[serde(default)]
    pub removed: bool,
    /// Change time, retained as sent.
    pub time: Option<String>,
    /// Current file metadata, if available.
    pub file: Option<File>,
}

/// Current changes boundary for initial polling.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPageToken {
    /// Persist this token for the initial changes request.
    pub start_page_token: PageToken,
}

/// Options for the supported user-scoped changes feed.
#[derive(Clone, Debug)]
pub struct ChangesOptions {
    /// Page size, 1–1000.
    pub page_size: u16,
    /// Nested file metadata selection.
    pub fields: FileFields,
}

impl Default for ChangesOptions {
    fn default() -> Self {
        Self {
            page_size: 100,
            fields: FileFields::default(),
        }
    }
}

/// One changes page. Exactly one boundary token must be present.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesPage {
    /// Changes to process before persisting this page's boundary.
    #[serde(default)]
    pub changes: Vec<Change>,
    /// Next page in the current polling batch.
    pub next_page_token: Option<PageToken>,
    /// New boundary for the next polling interval, only on the final page.
    pub new_start_page_token: Option<PageToken>,
}

/// Metadata retained by the page traversal, including the terminal changes boundary.
#[derive(Clone, Debug)]
pub struct ChangesPageMeta {
    /// HTTP response information.
    pub response: ResponseMeta,
    /// Save only after processing the final page successfully.
    pub new_start_page_token: Option<PageToken>,
}
