use concord::bytes::Bytes;
use concord::{multipart, Body, Fail, MultipartError};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// How Telegram says no: `{"ok": false, "error_code": 429, "description": "…", "parameters": {…}}`.
#[derive(Debug, Deserialize)]
pub struct TgProblem {
    pub error_code: i64,
    pub description: String,
    pub parameters: Option<ResponseParameters>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseParameters {
    pub retry_after: Option<u64>,
    pub migrate_to_chat_id: Option<i64>,
}

/// A file to send from its bytes.
#[derive(Clone, Debug)]
pub struct InputFile {
    pub name: String,
    pub content_type: String,
    pub bytes: Bytes,
}

/// The arm of every 2xx: `{"ok": true, "result": …}` gives the result, and `{"ok": false, …}`
/// gives the client's problem, though the status is `200`.
pub fn telegram_result<T: DeserializeOwned>(mut body: Value) -> Result<T, Fail<TgProblem>> {
    match body.get("ok").and_then(Value::as_bool) {
        Some(true) => {
            let result = body.get_mut("result").map(Value::take)
                .ok_or_else(|| concord::mismatch("no result"))?;
            serde_json::from_value(result).map_err(|_| concord::mismatch("not the promised result"))
        }
        Some(false) => {
            let problem: TgProblem = serde_json::from_value(body)
                .map_err(|_| concord::mismatch("not a Telegram refusal"))?;
            Err(problem)?
        }
        None => Err(concord::mismatch("no `ok` in the answer")),
    }
}

/// The body of an upload: the chat, the caption when there is one, and the file.
pub fn upload(
    chat_id: &ChatId, caption: &Option<String>, field: &str, file: &InputFile,
) -> Result<Body, MultipartError> {
    let mut form = multipart().text("chat_id", chat_id);
    if let Some(caption) = caption {
        form = form.text("caption", caption);
    }
    form.file(field, &file.name, &file.content_type, file.bytes.clone()).finish()
}

/// Telegram's policy: a `429` was not applied, and its body says how long to wait.
pub fn retry() -> telegram::Retry {
    telegram::Retry::standard()
        .not_applied_on(429)
        .when(|tried| {
            let seconds = tried.problem()?.parameters.as_ref()?.retry_after?;
            Some(Duration::from_secs(seconds))
        })
}
