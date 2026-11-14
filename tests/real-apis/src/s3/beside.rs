use concord::http::{HeaderName, HeaderValue};
use concord::page::{Call, Slot};
use concord::{reqwest, Answer, Fail, Secret};
use serde::Deserialize;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The keys that sign: given by the caller, never fetched or renewed here.
#[derive(Clone, Debug)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: Secret,
    pub session_token: Option<Secret>,
}

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

/// The next marker of a page: none at the end, or a Mismatch when a truncated page has none.
pub type NextMarker<T> = Result<Option<T>, Fail<S3Problem>>;

fn truncated_next<T: Clone>(more: bool, next: &Option<T>) -> NextMarker<T> {
    if !more {
        return Ok(None);
    }
    next.clone().map(Some).ok_or_else(|| concord::mismatch("a truncated page with no next marker"))
}

impl ObjectsPage {
    pub fn next(&self) -> NextMarker<String> {
        truncated_next(self.is_truncated, &self.next_continuation_token)
    }
}

impl PartsPage {
    pub fn next(&self) -> NextMarker<u32> {
        truncated_next(self.is_truncated, &self.next_part_number_marker)
    }
}

/// `by_cursor`, with a next value that may fail.
pub fn checked_cursor<C, S, V>(slot: S, next: fn(&C::Value) -> NextMarker<V>)
    -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<S3Problem>> + Send + 'static
where
    C: Call<Problem = S3Problem>,
    S: Slot<C, Value = V>,
    V: Send + 'static,
{
    move |call, answer| Ok(next(answer.value())?.map(|value| slot.set(call, value)))
}

/// The uploads in progress move by two markers together.
pub fn uploads_after<C, K, U>(key: K, upload: U)
    -> impl Fn(C, &Answer<UploadsPage>) -> Result<Option<C>, Fail<S3Problem>> + Send + 'static
where
    C: Call<Value = UploadsPage, Problem = S3Problem>,
    K: Slot<C, Value = String>,
    U: Slot<C, Value = String>,
{
    move |call, answer| {
        let page = answer.value();
        if !page.is_truncated {
            return Ok(None);
        }
        let missing = || concord::mismatch("a truncated page with no next marker");
        let next_key = page.next_key_marker.clone().ok_or_else(missing)?;
        let next_upload = page.next_upload_id_marker.clone().ok_or_else(missing)?;
        Ok(Some(upload.set(key.set(call, next_key), next_upload)))
    }
}

/// Metadata of the caller's (`x-amz-meta-*`), for `.edit_request(…)`: the description names
/// every key, and these keys are the caller's. Checked before the call is built.
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
