// The data types that the page names and does not print, cut to the fields that its code uses.

#[derive(Debug, serde::Deserialize)]
pub struct S3Problem {
    #[serde(rename = "Code")]
    pub code: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct BucketsPage {
    pub continuation_token: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct ObjectsPage {
    pub objects: Vec<ObjectInfo>,
    pub common_prefixes: Vec<CommonPrefix>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: u64,
}
#[derive(Debug, serde::Deserialize)]
pub struct CommonPrefix {
    pub prefix: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct UploadsPage {
    pub is_truncated: bool,
    pub next_key_marker: Option<String>,
    pub next_upload_id_marker: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct PartsPage {
    pub is_truncated: bool,
    pub next_part_number_marker: Option<u32>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct CreateBucketConfiguration {}
#[derive(Clone, Debug, serde::Serialize)]
pub struct DeleteBatch {}
#[derive(Debug, serde::Deserialize)]
pub struct DeleteResult {}
#[derive(Debug, serde::Deserialize)]
pub struct InitiatedUpload {
    pub upload_id: String,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct CompleteUpload {
    pub parts: Vec<(u32, String)>,
}
impl CompleteUpload {
    pub fn from_etags(parts: impl IntoIterator<Item = (u32, String)>) -> Self {
        CompleteUpload { parts: parts.into_iter().collect() }
    }
}
#[derive(Debug, serde::Deserialize)]
pub struct CopyResult {}
#[derive(Debug, serde::Deserialize)]
pub struct CompleteResult {
    pub etag: String,
}
