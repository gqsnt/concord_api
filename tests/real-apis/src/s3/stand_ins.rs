// The data types that the page names and does not print, cut to the fields that its code uses.

/// A key, a prefix or a key marker of a listing asked for with `encoding-type=url`: the service
/// writes it percent-encoded, with `+` for a space (the AWS SDK for Python decodes these fields
/// with `unquote_plus`); it is decoded once, here. A continuation token is opaque: never decoded.
fn url_decoded<'de, D: serde::Deserializer<'de>>(text: D) -> Result<String, D::Error> {
    use serde::de::Error;
    let text = <String as serde::Deserialize>::deserialize(text)?;
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.bytes();
    while let Some(byte) = rest.next() {
        bytes.push(match byte {
            b'+' => b' ',
            b'%' => {
                let digits = [rest.next(), rest.next()];
                let hex = |digit: Option<u8>| char::from(digit.unwrap_or(b'x')).to_digit(16);
                match (hex(digits[0]), hex(digits[1])) {
                    (Some(high), Some(low)) => u8::try_from(high * 16 + low).map_err(D::Error::custom)?,
                    _ => return Err(D::Error::custom("a percent escape is not two hex digits")),
                }
            }
            byte => byte,
        });
    }
    String::from_utf8(bytes).map_err(D::Error::custom)
}

fn url_decoded_option<'de, D: serde::Deserializer<'de>>(text: D) -> Result<Option<String>, D::Error> {
    url_decoded(text).map(Some)
}

#[derive(Debug, serde::Deserialize)]
pub struct S3Problem {
    #[serde(rename = "Code")]
    pub code: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct BucketsPage {
    #[serde(rename = "ContinuationToken")]
    pub continuation_token: Option<String>,
}
/// `ListBucketResult`, with the service's element names.
#[derive(Debug, serde::Deserialize)]
pub struct ObjectsPage {
    #[serde(rename = "Contents", default)]
    pub objects: Vec<ObjectInfo>,
    #[serde(rename = "CommonPrefixes", default)]
    pub common_prefixes: Vec<CommonPrefix>,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "NextContinuationToken")]
    pub next_continuation_token: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct ObjectInfo {
    #[serde(rename = "Key", deserialize_with = "url_decoded")]
    pub key: String,
    #[serde(rename = "Size")]
    pub size: u64,
}
#[derive(Debug, serde::Deserialize)]
pub struct CommonPrefix {
    #[serde(rename = "Prefix", deserialize_with = "url_decoded")]
    pub prefix: String,
}
/// `ListMultipartUploadsResult`, with the service's element names.
#[derive(Debug, serde::Deserialize)]
pub struct UploadsPage {
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "NextKeyMarker", default, deserialize_with = "url_decoded_option")]
    pub next_key_marker: Option<String>,
    #[serde(rename = "NextUploadIdMarker")]
    pub next_upload_id_marker: Option<String>,
}
/// `ListPartsResult`, with the service's element names and its parts.
#[derive(Debug, serde::Deserialize)]
pub struct PartsPage {
    #[serde(rename = "Part", default)]
    pub parts: Vec<PartInfo>,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "NextPartNumberMarker")]
    pub next_part_number_marker: Option<u32>,
}
#[derive(Debug, serde::Deserialize)]
pub struct PartInfo {
    #[serde(rename = "PartNumber")]
    pub part_number: u32,
    #[serde(rename = "ETag")]
    pub etag: Option<String>,
    #[serde(rename = "Size")]
    pub size: u64,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct CreateBucketConfiguration {}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename = "Delete")]
pub struct DeleteBatch {
    #[serde(rename = "@xmlns")]
    namespace: &'static str,
    #[serde(rename = "Object")]
    objects: Vec<DeleteObject>,
    #[serde(rename = "Quiet")]
    quiet: bool,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct DeleteObject {
    #[serde(rename = "Key")]
    key: String,
}
impl DeleteBatch {
    pub fn new(keys: impl IntoIterator<Item = String>) -> Self {
        Self {
            namespace: "http://s3.amazonaws.com/doc/2006-03-01/",
            objects: keys.into_iter().map(|key| DeleteObject { key }).collect(),
            quiet: false,
        }
    }
}
#[derive(Debug, serde::Deserialize)]
pub struct DeleteResult {
    #[serde(rename = "Deleted", default)]
    pub deleted: Vec<DeletedObject>,
}
#[derive(Debug, serde::Deserialize)]
pub struct DeletedObject {
    #[serde(rename = "Key")]
    pub key: String,
}
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
