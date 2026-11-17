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
