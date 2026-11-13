# the Telegram Bot API in Concord
Written by GPT-6 (Codex), from the permitted Concord guide and Telegram's public Bot API documentation, on 2026-11-12.

## The description

This is a focused option subset: ordinary chat messages and text edits, without rich/ephemeral messages, business options, thumbnails, or webhook certificate uploads. Provider facts follow the [public reference](https://core.telegram.org/bots/api#making-requests); the encoding and helper choices are this author's.

```rust
use concord::{json, Body, Json, Secret};
use concord::read::Stream;
use std::time::Duration;

concord::api! {
    pub client Telegram(token: Secret) {
        base "https://api.telegram.org/bot{token}";
        read Json;
        problem TgProblem read Json;
        status { 2xx { body } => telegram_result(body)? }
        middleware AttemptTrace;

        part formatted(parse_mode?: ParseMode, entities?: Vec<MessageEntity>) {
            body json { parse_mode; entities; }
        }

        GET get_me() -> User { path "getMe"; }
        POST get_me_post() -> User { path "getMe"; idempotent; }

        /// One batch; repeated polling and acknowledgement belong to the caller.
        GET get_updates(
            offset?: i64, limit?: u8, timeout?: u32,
            allowed_updates?: Vec<String>,
        ) -> Vec<Update> {
            path "getUpdates";
            query {
                offset; limit; timeout;
                "allowed_updates" = allowed_updates.as_ref()
                    .map(serde_json::to_string).transpose()?;
            }
            idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
            idempotent false;
        }

        POST send_message(
            chat_id: ChatId, text: String, disable_notification?: bool,
            protect_content?: bool, reply_markup?: ReplyMarkup, ..formatted,
        ) -> Message {
            path "sendMessage";
            body json { chat_id; text; disable_notification; protect_content; reply_markup; }
        }

        /// Supply chat_id + message_id, or inline_message_id, exclusively.
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

        POST send_photo(
            chat_id: ChatId, photo: MediaInput, caption?: String,
            parse_mode?: ParseMode, disable_notification?: bool,
        ) -> Message {
            path "sendPhoto";
            body media_body("photo", chat_id, photo, caption, parse_mode, disable_notification);
        }

        POST send_document(
            chat_id: ChatId, document: MediaInput, caption?: String,
            parse_mode?: ParseMode, disable_notification?: bool,
        ) -> Message {
            path "sendDocument";
            body media_body("document", chat_id, document, caption, parse_mode, disable_notification);
        }

        GET get_file(file_id: String) -> File {
            path "getFile";
            query { file_id; }
        }

        /// Same host, different URL prefix; this success is bytes, not an envelope.
        GET download_file(file_path: String) -> concord::ByteStream {
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
            body webhook_body(url, secret_token, allowed_updates, drop_pending_updates);
        }

        POST delete_webhook(drop_pending_updates?: bool) -> bool {
            path "deleteWebhook";
            body json { drop_pending_updates; }
        }

        POST answer_callback_query(
            callback_query_id: String, text?: String, show_alert?: bool,
            url?: String, cache_time?: u32,
        ) -> bool {
            path "answerCallbackQuery";
            body json { callback_query_id; text; show_alert; url; cache_time; }
        }

        GET get_chat(chat_id: ChatId) -> ChatFullInfo {
            path "getChat";
            query { chat_id; }
        }
    }
}
```

There are twelve provider operations, one raw download, and one alternate HTTP spelling. The token is part of each base's path, never a credential header. No inherited `path` needs removing. The download host really is the same host; inventing another host to demonstrate `base` would misdescribe Telegram. Its non-2xx errors still use the client's JSON problem reader.

## Beside the macro

The ordinary serde models are named here rather than reproduced: `User`, `Message`, `Update`, `File`, `ChatFullInfo`, `MessageEntity`, `ReplyMarkup`, and `InlineKeyboardMarkup`. `Update` includes an `i64` update identifier; `File` includes optional `file_path`. Unknown response fields are ignored. `ChatId` is an untagged integer/username enum with constructors `Id(i64)` and `Username(String)`; `ParseMode` serializes to Telegram's exact strings, including `HTML` and `MarkdownV2`.

`EditedMessage` is an untagged `Message`/`bool` enum: Telegram changes the success shape for inline edits. `TgProblem` deserializes the integer `error_code`, string `description`, and optional `parameters`. Its `ResponseParameters` has optional `retry_after: u64` and `migrate_to_chat_id: i64`. These models derive the relevant `Clone`, `Serialize`, `Deserialize`, and `Debug` traits. The [edit contract](https://core.telegram.org/bots/api#editmessagetext) motivates the union; it is not a Concord `Result` arrow.

All non-obvious helpers follow. Only `MediaInput` and the middleware marker are defined in full. Enable Concord's `multipart` feature. Uploads here hold bytes in memory; streamed uploads would need a caller-owned replay loop.

```rust
use bytes::Bytes;
use concord::{multipart, Fail, Middleware, Next, Retry, TransportError, TryInfo};
use serde::de::DeserializeOwned;
use serde_json::{json as value_json, Value};
type WriteError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug)]
pub enum MediaInput {
    Id(String),
    Url(String),
    Upload { file_name: String, mime: String, bytes: Bytes },
}

// A reader cannot return the client's typed refusal: an arm performs that step.
pub fn telegram_result<T: DeserializeOwned>(mut value: Value) -> Result<T, Fail<TgProblem>> {
    match value.get("ok").and_then(Value::as_bool) {
        Some(false) => {
            let problem = serde_json::from_value::<TgProblem>(value)
                .map_err(|_| concord::mismatch("invalid Telegram refusal"))?;
            Err(problem)?
        }
        Some(true) => {
            let result = value.as_object_mut().and_then(|o| o.remove("result"))
                .ok_or_else(|| concord::mismatch("missing Telegram result"))?;
            serde_json::from_value(result)
                .map_err(|_| concord::mismatch("invalid Telegram result"))
        }
        None => Err(concord::mismatch("missing Telegram ok")),
    }
}

pub fn media_body(
    field: &str, chat_id: &ChatId, media: &MediaInput, caption: &Option<String>,
    parse_mode: &Option<ParseMode>, disable_notification: &Option<bool>,
) -> Result<Body, WriteError> {
    let mut fields = value_json!({ "chat_id": chat_id, "caption": caption,
        "parse_mode": parse_mode, "disable_notification": disable_notification });
    let fields = fields.as_object_mut().expect("literal JSON object");
    fields.retain(|_, v| !v.is_null());
    match media {
        MediaInput::Id(text) | MediaInput::Url(text) => {
            fields.insert(field.to_owned(), Value::String(text.clone()));
            Ok(Body::new(serde_json::to_string(fields)?, "application/json"))
        }
        MediaInput::Upload { file_name, mime, bytes } => {
            let mut form = multipart();
            for (name, value) in fields.iter() {
                let text = match value {
                    Value::String(text) => text.clone(),
                    other => serde_json::to_string(other)?,
                };
                form = form.text(name.clone(), text);
            }
            Ok(form.file(field.to_owned(), file_name.clone(), mime.clone(),
                Body::new(bytes.clone(), mime.as_str())))
        }
    }
}

pub fn webhook_body(
    url: &String, secret_token: &Option<Secret>, allowed_updates: &Option<Vec<String>>,
    drop_pending_updates: &Option<bool>,
) -> Result<Body, WriteError> {
    let mut fields = value_json!({ "url": url, "allowed_updates": allowed_updates,
        "drop_pending_updates": drop_pending_updates });
    let fields = fields.as_object_mut().expect("literal JSON object");
    fields.retain(|_, v| !v.is_null());
    if let Some(secret) = secret_token {
        fields.insert("secret_token".into(), Value::String(secret.expose().to_string()));
    }
    Ok(Body::new(serde_json::to_string(fields)?, "application/json"))
}

pub fn retry() -> Retry<TgProblem> {
    Retry::standard().when(|tried| {
        let seconds = tried.problem()?.parameters.as_ref()?.retry_after?;
        Some(Duration::from_secs(seconds))
    })
}

pub struct AttemptTrace;
impl Middleware for AttemptTrace {
    async fn handle(&self, request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        tracing::debug!(endpoint = info.endpoint(), attempt = info.number(), "Telegram request");
        next.send(request, info).await
    }
}
```

`AttemptTrace` is this crate's choice to observe every attempt, including downloads; it logs names, not URLs. It performs no authentication and does not inspect or consume response bodies. The middleware remains in place with a fake transport. The multipart builder's exact accepted argument types are not specified by the permitted guide; its documented `.text`/`.file` shape is used above, with an in-memory `Body` as the file argument. That Rust detail needs confirmation when implemented (finding 11).

The arm first checks `ok`, so a false reply with a wrongly shaped `result` still becomes a typed refusal. A true reply missing a result, or containing the wrong result type, is a Mismatch. Non-2xx replies bypass this arm and decode `TgProblem` normally. An arm-generated refusal preserves HTTP status, so a logical error inside HTTP 200 still has status 200, and its already-decoded body bytes are not retained.

`retry()` reads the [body's waiting hint](https://core.telegram.org/bots/api#responseparameters), including for an eligible refusal inside HTTP 200. It does not grant permission to repeat a write. Whether every flood rejection guarantees that a write had no effect is (unsure); no such promise is encoded. `get_updates` explicitly disables retries, conservatively covering negative offsets that discard queued updates. This does not change the caller's acknowledgement protocol. No pager is supplied: an empty poll is not the end, and polling the same offset again is legitimate.

## Calling it

These are four independent async excerpts; `bot` and application values are shared context. Application functions such as `persist_and_handle` and the output file belong to the program. None of these excerpts was executed.

Identity, a chat message, its edit, and a callback acknowledgement:

```rust
let bot = Telegram::new(std::env::var("TELEGRAM_TOKEN")?);
let me = bot.get_me().with_retry(retry()).await?;
let chat = ChatId::Id(123456789);
let info = bot.get_chat(&chat).await?;
let sent = bot.send_message(&chat, "Working…").await?;
let edited = bot.edit_message_text("Done").chat_id(&chat).message_id(sent.message_id).await?;
bot.answer_callback_query(callback_id).text("Done").await?;
bot.delete_message(&chat, sent.message_id).await?;
```

Switch from a webhook to polling; acknowledge only after durable processing:

```rust
bot.delete_webhook().drop_pending_updates(false).await?;
let mut offset = None::<i64>;
loop {
    let batch = bot.get_updates().offset(offset).timeout(50).limit(100).await?;
    let next = batch.iter().map(|u| u.update_id).max().map(|id| id + 1);
    for update in batch { persist_and_handle(update).await?; }
    if next.is_some() { offset = next; } // Empty batch: repeat the same poll.
}
```

Send by URL, upload bytes, or reuse an existing file identifier:

```rust
bot.send_photo(&chat, MediaInput::Url(photo_url)).caption("The view").await?;
let upload = MediaInput::Upload {
    file_name: "report.pdf".into(), mime: "application/pdf".into(),
    bytes: Bytes::from(tokio::fs::read("report.pdf").await?),
};
bot.send_document(&chat, upload).await?;
bot.send_document(&chat, MediaInput::Id(saved_file_id)).await?;
```

Download an attachment, then configure delivery of future updates:

```rust
use futures_util::TryStreamExt;
use tokio::io::AsyncWriteExt;
let file = bot.get_file(file_id).await?;
let path = file.file_path.ok_or("Telegram omitted file_path")?;
let mut download = bot.download_file(path).with_timeout(Duration::from_secs(600)).await?;
while let Some(bytes) = download.try_next().await? { output.write_all(&bytes).await?; }
bot.set_webhook(webhook_url).secret_token(webhook_secret)
    .allowed_updates(["message", "callback_query"]).drop_pending_updates(false).await?;
```

## Findings

| # | where | what got in the way | what you wrote instead | kind |
|---|---|---|---|---|
| 1 | HTTP verb | A single operation cannot advertise GET and POST or let the caller select between them. | `get_me` and `get_me_post`; other operations expose one supported verb. | cannot be said |
| 2 | `getUpdates` continuation | A quiet poll must repeat an identical request; `.pages()` rejects that request as a cycle. Stopping on an empty batch would silently stop the bot. | No `paginate`; a caller loop retaining offset after an empty batch. | cannot be said |
| 3 | `editMessageText` inputs | The signature cannot require exactly chat + message identifiers or an inline identifier; all-optional setters admit invalid calls. | Documented precondition and an `EditedMessage` union; Telegram validates the flat inputs. | cannot be said |
| 4 | Media bodies | The choice between JSON text and multipart bytes is outside placement. A `body json` part cannot merge into the upload expression. | One Rust writer; repeat the applicable formatting input and its placement on media endpoints. | said with pain |
| 5 | Optional webhook secret | `Secret` cannot serialize in a body entry. Mapping an optional secret to exposed text writes `null` when absent. | A writer that exposes only a present secret and omits absent fields. | said with pain |
| 6 | Reply envelope | A reader can fail with Mismatch, but cannot supply `TgProblem` as a typed refusal. The useful output hides another decoding step. | Root 2xx arm plus generic `telegram_result`; explicit JSON reader for non-2xx problems. | said with pain |
| 7 | Download routing | I reread the additive-path and secret-base rules before choosing where to put `bot<token>`. A shared `path` cannot be replaced. | Put the RPC prefix in `base`; repeat token placement in the download's own `base`, then remove the envelope arm. | said with pain |
| 8 | `allowed_updates` in a query | An ordinary `Vec` repeats the key, rather than sending a JSON array as one query value. | A fallible JSON-string expression; absent still means absent, and an empty list sends `[]`. | small |
| 9 | Long-poll clocks | I checked twice that provider `.timeout(50)` and Concord `.with_timeout(…)` control different things. | Input-dependent `idle_timeout`, with ten seconds of client margin. | small |
| 10 | Body-coded retry permission | Even given a known-not-applied problem, `when` cannot grant write replay permission only for that typed problem inside HTTP 200; `not_applied_on(200)` is too broad. | Export a waiting rule only; leave write retries to a caller that can establish permission. | cannot be said |
| 11 | Multipart helper contract | The guide shows chained builder calls but does not give their argument types, fallibility, or precisely where a builder becomes a `Body`. | Use the shown shape; explicitly leave the file-argument Rust contract unverified. | said with pain |

Wishes for the first two kinds, expressed as requirements rather than invented macro syntax:

- 1: One operation, two supported wire methods, with a caller choice.
- 2: A poll continuation may intentionally resend the same request after an empty answer.
- 3: Exactly one of two identifier groups, checked before sending.
- 4: Typed file parts and ordinary body entries compose with the same shared parts.
- 5: A secret body entry preserves optional omission without constructing a whole payload.
- 6: Unwrap an envelope and report its typed problem once, with result decoding visible.
- 7: Name an alternative path prefix without rebuilding the credential-bearing base.
- 10: Grant replay permission by a predicate on the typed problem, independently of HTTP status.
- 11: Show a complete, typed, fallible multipart writer in the guide.

## Measures

- Endpoints: **14** (12 provider operations + download + POST spelling of `getMe`).
- Lines of `api!`: **108**, including its opening/closing lines, comments and blanks; excludes imports and fences.
- Repeated input names solely for placement: **42** second mentions; declared inputs in all: **42** (44 after expanding parts). Count declarations as written, including client/part inputs once; each is named again for wire placement. The download adds one further reuse of `client.token` (43 wire mentions altogether); the second use of `timeout` computes behavior and is not placement.
- Rust beside the macro solely for missing declarative behavior: **63** nonblank, noncomment lines: envelope conversion (16), media encoding (27), optional secret encoding (13), body-coded retry (6), and the `WriteError` alias (1). Excludes imports, model definitions, the optional tracing middleware, and calling code. Trace middleware is ordinary instrumentation through an existing language feature.
- File length: **326** lines. No build, compilation, or request to Telegram's API was run.

## Scores

- Writing this description: **6/10** — Most methods are short, but uploads and secret omission pull ordinary input placement into custom writers.
- Reading it back a month later: **7/10** — The operation list is clear, while envelope decoding and body construction require reading beside it.
- The calling code: **8/10** — Required arguments and optional setters read naturally; the polling acknowledgement loop remains application work.
