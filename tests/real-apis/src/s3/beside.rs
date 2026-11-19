use concord::http::{HeaderName, HeaderValue};
use concord::page::{Paging, Slot};
use concord::reqwest;
use serde::Deserialize;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub use concord::sign::Credentials;

/// A copy, or the completion of an upload, can fail inside a `200`: the root of the XML says which.
#[derive(Debug, Deserialize)]
pub enum CopyReply {
    #[serde(rename = "CopyObjectResult")]
    Success(CopyResult),
    #[serde(rename = "Error")]
    Error(S3Problem),
}

#[derive(Debug, Deserialize)]
pub enum CompleteReply {
    #[serde(rename = "CompleteMultipartUploadResult")]
    Success(CompleteResult),
    #[serde(rename = "Error")]
    Error(S3Problem),
}

pub fn copy_ok(reply: CopyReply) -> Result<CopyResult, S3Problem> {
    match reply {
        CopyReply::Success(value) => Ok(value),
        CopyReply::Error(problem) => Err(problem),
    }
}

pub fn complete_ok(reply: CompleteReply) -> Result<CompleteResult, S3Problem> {
    match reply {
        CompleteReply::Success(value) => Ok(value),
        CompleteReply::Error(problem) => Err(problem),
    }
}

/// `x-amz-copy-source`: a header is not percent-encoded by Concord, so this does it.
pub fn copy_source(bucket: &str, key: &str, version: &Option<String>) -> String {
    let mut source = format!("/{}/{}", encoded(bucket, false), encoded(key, true));
    if let Some(id) = version {
        source.push_str("?versionId=");
        source.push_str(&encoded(id, false));
    }
    source
}

fn encoded(text: &str, keep_slashes: bool) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) || (keep_slashes && byte == b'/') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

impl BucketsPage {
    pub fn next(&self) -> Option<String> {
        self.continuation_token.clone()
    }
}

/// A truncated upload page needs a key marker. An absent upload ID is sent as an empty marker.
pub fn uploads_after<C, K, U>(key: K, upload: U) -> Paging<C>
where
    C: concord::page::Call<Value = UploadsPage, Problem = S3Problem>,
    K: Slot<C, Value = String>,
    U: Slot<C, Value = String>,
{
    Paging::<C>::from_fn(|call, answer| answer.value().is_truncated.then_some(call))
        .try_map_next(move |call, page| {
            let next_key = page.next_key_marker.clone()
                .ok_or_else(|| concord::mismatch("a truncated page with no next marker"))?;
            let next_upload = page.next_upload_id_marker.clone().unwrap_or_default();
            Ok(upload.set(key.set(call, next_key), next_upload))
        })
}

/// The caller's own metadata (`x-amz-meta-*`), for `.edit_request(…)`: the description names
/// every key, and these keys are chosen by the caller. Checked before the call is built.
pub fn metadata<K: AsRef<str>, V: AsRef<str>>(
    entries: impl IntoIterator<Item = (K, V)>,
) -> Result<impl Fn(&mut reqwest::Request) + Send + Sync + 'static, BoxError> {
    let mut headers = Vec::new();
    for (suffix, value) in entries {
        let name = format!("x-amz-meta-{}", suffix.as_ref());
        headers.push((HeaderName::try_from(name)?, HeaderValue::try_from(value.as_ref())?));
    }
    Ok(move |request: &mut reqwest::Request| {
        for (name, value) in &headers {
            request.headers_mut().insert(name.clone(), value.clone());
        }
    })
}
