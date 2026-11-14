use concord::bytes::Bytes;
use concord::reqwest;
use concord::Fail;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Map, Value};

/// How Slack says no: `{"ok": false, "error": "channel_not_found"}`, with the status `200`.
#[derive(Debug, Deserialize)]
pub struct SlackProblem {
    pub error: String,
    #[serde(flatten)]
    pub details: Map<String, Value>,
}

/// A page of any list: Slack names the list `channels`, `messages` or `members`.
#[derive(Debug, Deserialize)]
pub struct SlackPage<T> {
    #[serde(alias = "channels", alias = "messages", alias = "members")]
    pub items: Vec<T>,
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMetadata {
    pub next_cursor: Option<String>,
}

impl<T> SlackPage<T> {
    /// The cursor of the next page. Empty, `null` or missing: this was the last page.
    pub fn next_cursor(&self) -> Option<String> {
        let cursor = self.response_metadata.as_ref()?.next_cursor.as_deref()?;
        (!cursor.is_empty()).then(|| cursor.to_owned())
    }
}

impl<T: Send> concord::page::IntoItems for SlackPage<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// An answer with nothing in it but `"ok": true`.
#[derive(Debug, Deserialize)]
pub struct Acknowledged {}

/// The arm of every `200`: `ok` says whether it is the success or the client's problem.
pub fn slack_response<T: DeserializeOwned>(body: Value) -> Result<T, Fail<SlackProblem>> {
    match body.get("ok").and_then(Value::as_bool) {
        Some(true) => serde_json::from_value(body)
            .map_err(|_| concord::mismatch("not the promised answer")),
        Some(false) => {
            let problem: SlackProblem = serde_json::from_value(body)
                .map_err(|_| concord::mismatch("a refusal with no error code"))?;
            Err(problem)?
        }
        None => Err(concord::mismatch("no `ok` in the answer")),
    }
}

/// Slack's policy: a `429` was not applied, so even a `POST` may go again after its `Retry-After`.
pub fn retry() -> slack::Retry {
    slack::Retry::standard().not_applied_on(429)
}

/// The middle step of an upload: the bytes go to the URL that Slack gave, on another host.
/// A plain `reqwest` client sends them, with none of the client's credentials.
pub async fn upload_bytes(url: &str, bytes: Bytes) -> Result<(), reqwest::Error> {
    let answer = reqwest::Client::new()
        .post(url)
        .header("content-type", "application/octet-stream")
        .body(bytes)
        .send()
        .await?;
    answer.error_for_status()?;
    Ok(())
}
