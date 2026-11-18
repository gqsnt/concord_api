use concord::bytes::Bytes;
use concord::{Fail, Json};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Map, Value};

/// Slack's error response: `{"ok": false, "error": "channel_not_found"}`, with the status `200`.
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

/// A response with nothing in it but `"ok": true`.
#[derive(Debug, Deserialize)]
pub struct Acknowledged {}

/// The arm of every `200`: `ok` says whether it is the success or the client's problem.
/// The bytes are decoded twice, once for `ok` and once for the value or the problem, so a
/// success that does not fit names its field, as under `read Json;`.
pub fn slack_response<T: DeserializeOwned>(body: Bytes) -> Result<T, Fail<SlackProblem>> {
    #[derive(Deserialize)]
    struct Head {
        ok: bool,
    }
    let head: Head = Json::decode(&body).map_err(Fail::from_read)?;
    if head.ok {
        Json::decode(&body).map_err(Fail::from_read)
    } else {
        let problem: SlackProblem = Json::decode(&body).map_err(Fail::from_read)?;
        Err(problem)?
    }
}

/// Slack's policy: a `429` was not applied, so even a `POST` may be retried after `Retry-After`.
pub fn retry() -> slack::Retry {
    slack::Retry::standard().not_applied_on(429)
}
