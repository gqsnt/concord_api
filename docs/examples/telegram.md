# Telegram

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [Slack](slack.md) · next: [Elasticsearch](elasticsearch.md)*

This page is for a Rust developer who writes a bot with the Telegram Bot API. It gives a client with 13 methods that you can copy, and shows a token in the path, an envelope around every response, a long poll, and a `multipart` upload.

Here is Telegram as Concord sees it. There is one host, and the bot's token is part of the path: `base "https://api.telegram.org/bot{token}";`. The token is a `Secret` inside a template, so it is never printed. The bot's files are on the same host under another prefix, `/file/bot{token}/`, so one endpoint has its own `base`. Every response is an envelope, `{"ok": true, "result": …}` or `{"ok": false, "error_code": …, "description": …}`, often with a `200` status, and one arm on the client unwraps it. Updates arrive by a long poll (`getUpdates` waits up to `timeout` seconds when there is nothing new) or by a webhook. Flood control responds with a `429` and `retry_after` in its body. Telegram's own reference is [core.telegram.org/bots/api](https://core.telegram.org/bots/api). `(unsure)` marks a detail about Telegram that has not been checked.

## The description

```rust
use concord::read::Stream;
use concord::{json, ByteStream, Json, Secret};
use std::time::Duration;

concord::api! {
    /// The Telegram Bot API: messages, files, and updates by long poll or by webhook.
    pub client Telegram(token: Secret) {
        base "https://api.telegram.org/bot{token}";
        read Json;
        problem TgProblem;
        status { 2xx { body } => telegram_result(body)? }

        part formatted(parse_mode?: ParseMode, entities?: Vec<MessageEntity>) {
            body json { parse_mode; entities; }
        }

        GET get_me() -> User { path "getMe"; }

        /// One batch of updates. With nothing new, Telegram holds the request for `timeout`
        /// seconds: a long poll. Requesting again with the same offset is the next poll.
        GET get_updates(
            offset?: i64, limit?: u8, timeout?: u32, allowed_updates?: Vec<String>,
        ) -> Vec<Update> {
            path "getUpdates";
            query {
                offset; limit; timeout;
                "allowed_updates" = allowed_updates.as_ref().map(serde_json::to_string).transpose()?;
            }
            idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
            idempotent false;
        }

        POST send_message(
            chat_id: ChatId, text: String, disable_notification?: bool,
            reply_markup?: ReplyMarkup, ..formatted,
        ) -> Message {
            path "sendMessage";
            body json { chat_id; text; disable_notification; reply_markup; }
        }

        /// Give `chat_id` and `message_id`, or `inline_message_id`: Telegram checks it.
        POST edit_message_text(
            text: String, chat_id?: ChatId, message_id?: i64,
            inline_message_id?: String, reply_markup?: InlineKeyboardMarkup, ..formatted,
        ) -> EditedMessage {
            path "editMessageText";
            body json { text; chat_id; message_id; inline_message_id; reply_markup; }
        }

        POST delete_message(chat_id: ChatId, message_id: i64) -> bool {
            path "deleteMessage";
            body json { chat_id; message_id; }
        }

        /// A photo that Telegram has already, by its `file_id`, or one on the web, by its URL.
        POST send_photo(chat_id: ChatId, photo: String, caption?: String, ..formatted) -> Message {
            path "sendPhoto";
            body json { chat_id; photo; caption; }
        }

        /// A new file, from its bytes.
        POST send_document(chat_id: ChatId, document: InputFile, caption?: String) -> Message {
            path "sendDocument";
            body upload(chat_id, caption, "document", document);
        }

        GET get_file(file_id: String) -> File {
            path "getFile";
            query { file_id; }
        }

        /// The bytes of a file: on the same host, under another prefix, and not in an envelope.
        GET download_file(file_path: String) -> ByteStream {
            base "https://api.telegram.org/file/bot{client.token}";
            path "{*file_path}";
            read Stream;
            status { remove 2xx }
        }

        POST set_webhook(
            url: String, secret_token?: Secret, allowed_updates?: Vec<String>,
            drop_pending_updates?: bool,
        ) -> bool {
            path "setWebhook";
            body json { url; secret_token; allowed_updates; drop_pending_updates; }
        }

        POST delete_webhook(drop_pending_updates?: bool) -> bool {
            path "deleteWebhook";
            body json { drop_pending_updates; }
        }

        POST answer_callback_query(
            callback_query_id: String, text?: String, show_alert?: bool, cache_time?: u32,
        ) -> bool {
            path "answerCallbackQuery";
            body json { callback_query_id; text; show_alert; cache_time; }
        }

        GET get_chat(chat_id: ChatId) -> ChatFullInfo {
            path "getChat";
            query { chat_id; }
        }
    }
}
```

A body of entries leaves out every input that was not set, including the `Secret` of `set_webhook`, which is sent as a plain string when it is set. The part `formatted` adds its entries to the body of every node that includes it.

## Beside the description

The data types are plain `serde` types, named here but not shown: `User`, `Message` (`message_id`, `document`), `Document` (`file_id`), `Update` (`update_id`), `File` (`file_path`), `ChatFullInfo`, `MessageEntity`, `ReplyMarkup`, `InlineKeyboardMarkup`. `ChatId` is an id or a `@username`, serialized untagged; `ParseMode` is serialized as Telegram spells it (`HTML`, `MarkdownV2`). `EditedMessage` is a `Message`, or `true` for a message sent inline.

Telegram's errors, the envelope, the body of an upload, and Telegram's retry policy:

```rust
use concord::bytes::Bytes;
use concord::{multipart, Body, Fail, MultipartError};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
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
/// returns the client's problem, even though the status is `200`.
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
```

## Calling it

```rust
use concord::Secret;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Send a message, change it, delete it.
pub async fn hello(bot: &Telegram, chat: ChatId) -> telegram::Result<()> {
    let me = bot.get_me().await?;
    let sent = bot.send_message(&chat, format!("{} is working…", me.first_name)).await?;
    bot.edit_message_text("Done").chat_id(&chat).message_id(sent.message_id).await?;
    bot.delete_message(&chat, sent.message_id).await?;
    Ok(())
}

/// Updates by long poll, for ever. An empty batch is not the end: poll again.
pub async fn poll(bot: &Telegram, mut handle: impl FnMut(Update)) -> telegram::Result<()> {
    bot.delete_webhook().drop_pending_updates(false).await?;
    let mut offset = None::<i64>;
    loop {
        let batch = bot.get_updates().offset(offset).timeout(50).with_retry(retry()).await?;
        for update in batch {
            offset = Some(update.update_id + 1);
            handle(update);
        }
    }
}

/// Send a file from its bytes, then read it back from Telegram.
pub async fn round_trip(bot: &Telegram, chat: &ChatId, file: InputFile) -> Result<Vec<u8>, BoxError> {
    let sent = bot.send_document(chat, file).caption("The report").await?;
    let document = sent.document.ok_or("no document in the message")?;
    let file = bot.get_file(document.file_id).await?;
    let path = file.file_path.ok_or("no file_path")?;
    let mut stream = bot.download_file(path).with_timeout(Duration::from_secs(600)).await?;
    let mut bytes = Vec::new();
    while let Some(piece) = stream.try_next().await? {
        bytes.extend_from_slice(&piece);
    }
    Ok(bytes)
}

/// Updates by webhook: Telegram sends them to `url`, with the secret in a header.
pub async fn use_webhook(bot: &Telegram, url: &str, secret: Secret) -> telegram::Result<bool> {
    bot.set_webhook(url)
        .secret_token(secret)
        .allowed_updates(["message", "callback_query"])
        .await
}
```

## What does not fit well, and the workaround

| Telegram wants | What to write |
|---|---|
| A long poll that sends the same request again after an empty response, but pagination stops when the same request would be sent twice in a row. | Your own loop (`poll` above). `get_updates` declares `idempotent false;`, because a negative offset makes Telegram forget updates. |
| A file by its id or URL in JSON, or by its bytes in `multipart/form-data`: one method, two bodies. | Two endpoints on one path: `send_photo` with a body of entries, `send_document` with `multipart()`. |
| Exactly one of two groups of inputs (`chat_id` and `message_id`, or `inline_message_id`). | Each input optional, with a doc comment: Telegram rejects a call that has neither. |
| `getMe` as both a `GET` and a `POST`, but an endpoint has exactly one method. | The method that you use, or two endpoints with two names. |
| A list in one query value, as JSON: `allowed_updates=["message"]`. A `Vec` repeats its key for each item. | `"allowed_updates" = allowed_updates.as_ref().map(serde_json::to_string).transpose()?;`. |
| A flood-control error (`429`) whose wait time is in the body: `parameters.retry_after`. | A rule of the retry policy reads it (`retry()` above). Whether a `429` always means "not applied" is (unsure). |
