# Google Drive

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [Anthropic](anthropic.md) · next: [Real APIs](README.md)*

This page is for a Rust developer who calls the Google Drive API. It gives a client with 6 endpoints that you can copy, and shows an upload of metadata and content in one `multipart/related` request, Google's own convention, and a list paginated by a token.

Here is Google Drive as Concord sees it. There is one host, `www.googleapis.com`: the metadata of files is under `/drive/v3`, and uploads go to `/upload/drive/v3`. Authentication is an OAuth 2.0 access token, sent as `authorization: Bearer …`; obtaining it is the application's work (see [A token that is renewed](../guide/credentials.md#a-token-that-is-renewed)). An upload of type `multipart` sends the metadata of the file (JSON) and its content in one request, for a file of 5 MB or less; a larger file needs a resumable upload, a protocol of several requests that this page does not show. A list is paginated by `pageToken` and `nextPageToken`. Google rejects a request with an error status and `{"error": {"code", "message", "errors"}}`, read into `DriveProblem`. Google's own reference is [developers.google.com/drive/api](https://developers.google.com/drive/api). `(unsure)` marks a detail about Drive that has not been checked.

## The description

```rust
use concord::read::Stream;
use concord::{by_cursor, json, Body, ByteStream, Json, Secret};

concord::api! {
    /// Google Drive API v3: files listed, read, uploaded with their metadata, changed, deleted.
    pub client Drive(token: Secret) {
        base "https://www.googleapis.com";
        headers { "authorization" = "Bearer {token}"; }
        read Json;
        problem DriveProblem;

        part listing(q?: String, page_size?: u32, page_token?: String, fields?: String) {
            query { q; "pageSize" = page_size; "pageToken" = page_token; fields; }
            paginate by_cursor(page_token, FileList::next_page_token);
        }

        GET list(..listing) -> FileList { path "drive/v3/files"; }

        GET get(file_id: String, fields?: String) -> File {
            path "drive/v3/files/{file_id}";
            query { fields; }
        }

        /// The bytes of a file that is not a Google document (those are exported instead).
        GET download(file_id: String) -> ByteStream {
            path "drive/v3/files/{file_id}";
            query { "alt" = "media"; }
            read Stream;
        }

        /// The metadata and the content in one request, `multipart/related`: metadata first.
        POST upload(metadata: NewFile, content: Body) -> File {
            path "upload/drive/v3/files";
            query { "uploadType" = "multipart"; }
            body related(metadata, content);
        }

        PATCH update(file_id: String, changes: NewFile) -> File {
            path "drive/v3/files/{file_id}";
            body json(changes);
        }

        DELETE delete(file_id: String) {
            path "drive/v3/files/{file_id}";
            idempotent false;
        }
    }
}
```

`fields` chooses the fields of the response, which Drive keeps short by default; every field that a type requires must be asked for. `delete` declares `idempotent false;`, because a second attempt after a lost response gets a `404` for a file that the first attempt deleted.

## Beside the description

The data type `File` (`id`, `name`, `mimeType`, …) is a plain `serde` type, named here but not shown.

The body of `upload` is built by `concord::multipart_related`: the first part is the root, the metadata as JSON, and the second is the content, with its own content type. The writer chooses a boundary that occurs in no part, and adds the `type` parameter that `multipart/related` defines (RFC 2387); Drive's own examples leave it out (unsure whether Drive reads it).

```rust
use concord::http::{HeaderMap, HeaderValue};
use concord::{json, multipart_related, Body};
use serde::{Deserialize, Serialize};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Google's error response: `{"error": {"code": 404, "message": …, "errors": […]}}`.
#[derive(Debug, Deserialize)]
pub struct DriveProblem {
    pub error: DriveError,
}

#[derive(Debug, Deserialize)]
pub struct DriveError {
    pub code: u16,
    pub message: String,
}

/// The metadata of a file that is created or changed. Fields left out are not sent.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
}

/// The body of `upload`: the metadata as JSON, then the content with its own content type.
pub fn related(metadata: &NewFile, content: &Body) -> Result<Body, BoxError> {
    let mut first = HeaderMap::new();
    first.insert("content-type", HeaderValue::from_static("application/json; charset=UTF-8"));
    let mut second = HeaderMap::new();
    let media = content.get_content_type().unwrap_or("application/octet-stream");
    second.insert("content-type", HeaderValue::from_str(media)?);
    Ok(multipart_related([(first, json(metadata)?), (second, content.clone())])?)
}

/// A page of files. For `by_cursor(page_token, …)`: the token of the next page, if any.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileList {
    pub files: Vec<File>,
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub incomplete_search: bool,
}

impl FileList {
    pub fn next_page_token(&self) -> Option<String> {
        self.next_page_token.clone()
    }
}

impl concord::page::IntoItems for FileList {
    type Item = File;
    fn into_items(self) -> Vec<File> {
        self.files
    }
    fn count(&self) -> usize {
        self.files.len()
    }
}
```

## Calling it

```rust
use concord::Body;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A report uploaded into a folder with its name, in one request; then read back.
pub async fn upload(drive: &Drive, folder: &str, csv: Vec<u8>) -> Result<File, BoxError> {
    let metadata = NewFile {
        name: Some("report.csv".into()),
        parents: vec![folder.into()],
        ..Default::default()
    };
    let file = drive.upload(metadata, Body::new(csv, "text/csv")).await?;
    let mut bytes = drive.download(&file.id).await?;
    while let Some(chunk) = bytes.try_next().await? {
        println!("{} bytes", chunk.len());
    }
    Ok(file)
}

/// Every PDF of the user's Drive, page after page, with only the fields that are needed.
pub async fn pdfs() -> Result<(), BoxError> {
    let drive = Drive::new(std::env::var("GOOGLE_ACCESS_TOKEN")?);
    let mut files = drive.list()
        .q("mimeType = 'application/pdf' and trashed = false")
        .fields("nextPageToken, files(id, name)")
        .page_size(100)
        .pages().items();
    while let Some(file) = files.try_next().await? {
        println!("{} {}", file.id, file.name);
    }
    Ok(())
}

/// A file renamed, then deleted.
pub async fn rename_and_delete(drive: &Drive, file_id: &str) -> drive::Result<()> {
    let name = NewFile { name: Some("old report.csv".into()), ..Default::default() };
    let renamed = drive.update(file_id, name).await?;
    println!("{}", renamed.name);
    drive.delete(file_id).await?;
    Ok(())
}
```

## What does not fit well, and the workaround

| Drive wants | What to write |
|---|---|
| Metadata and content in one request, as `multipart/related`. | `concord::multipart_related`, called by a function of yours (`related`) that names the parts and their headers. |
| A file larger than 5 MB: a resumable upload, a session over several requests. | Your own function around the calls: one that starts the session and returns its URL, then the chunks sent to that URL (see [When Concord does not cover what you need](../guide/extending.md)). |
| A Google document (Docs, Sheets), which has no bytes to download. | `files/{id}/export` with a `mimeType`: another endpoint, on the same pattern as `download`. |
| `fields`, which removes fields from the response. The return type is fixed. | Ask for every field that the type requires, as `pdfs` does; for any other shape, `send_raw()`. |
| The access token, which expires after about an hour (unsure). | A renewed credential, `concord::auth::Renewed`, as middleware on the client (see [A token that is renewed](../guide/credentials.md#a-token-that-is-renewed)). |
