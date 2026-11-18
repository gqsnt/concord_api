use concord::bytes::Bytes;
use concord::{multipart, Body, Fail, Json, MultipartError};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::time::Duration;

/// Telegram's error response: `{"ok": false, "error_code": 429, "description": "…", "parameters": {…}}`.
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

/// The arm of every 2xx: `{"ok": true, "result": …}` returns the result, and `{"ok": false, …}`
/// returns the client's problem, even though the status is `200`. The bytes are decoded once
/// for `ok`, then as the result or the problem: a result that does not fit names its field
/// (`result.chat.id`), as under `read Json;`.
pub fn telegram_result<T: DeserializeOwned>(body: Bytes) -> Result<T, Fail<TgProblem>> {
    #[derive(Deserialize)]
    struct Head {
        ok: bool,
    }
    #[derive(Deserialize)]
    struct Reply<T> {
        result: T,
    }
    let head: Head = Json::decode(&body).map_err(Fail::from_read)?;
    if head.ok {
        let reply: Reply<T> = Json::decode(&body).map_err(Fail::from_read)?;
        Ok(reply.result)
    } else {
        let problem: TgProblem = Json::decode(&body).map_err(Fail::from_read)?;
        Err(problem)?
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
