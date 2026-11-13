# the Telegram Bot API in Concord

Written by Claude Opus 5.5 (`claude-opus-5-5`, as the harness names it), from the Concord guide pages and from memory of Telegram's public Bot API documentation (nothing fetched, nothing called), on 2026-11-12.

## The description

Fourteen endpoints: the twelve methods of the brief, with `editMessageText` written as two (it answers a `Message` or `true`), and the download of a file. Telegram's URL is flat, so the tree is flat: no scope.

```rust
use concord::read::Stream;
use concord::{ByteStream, Json, Secret};
use serde_json::json;

use crate::types::{ChatFullInfo, ChatId, File, InlineKeyboardMarkup, LinkPreviewOptions, Message,
                   MessageEntity, ParseMode, ReplyMarkup, ReplyParameters, Update, UpdateKind, User};

concord::api! {
    /// The Telegram Bot API: twelve of its methods, and the download of a file.
    pub client Telegram(
        /// The bot's token from @BotFather (`123456:ABC-…`). Telegram wants it in the path.
        token: Secret,
    ) {
        base "https://api.telegram.org/bot{token}";
        read Json;
        problem TgProblem;
        status { 2xx { body } => Reply::into_result(body)? }        // `{ "ok": true, "result": … }`

        /// How a text is formatted: one of the two.
        part formatted(parse_mode?: ParseMode, entities?: Vec<MessageEntity>) {}

        /// What a new message may say about how it is delivered.
        part delivery(
            message_thread_id?: i64,
            disable_notification?: bool,
            protect_content?: bool,
            reply_parameters?: ReplyParameters,
            reply_markup?: ReplyMarkup,
        ) {}

        /// The bot itself.
        GET get_me() -> User { path "getMe"; }

        /// New updates, oldest first. While there is none, the server holds the request up to
        /// `timeout` seconds: send it with `.long_poll(seconds)`, which sets Concord's clock too.
        GET get_updates(offset?: i64, limit?: u32, timeout?: u32, allowed_updates?: Vec<UpdateKind>) -> Vec<Update> {
            path "getUpdates";
            query { offset; limit; timeout; "allowed_updates" = allowed_updates.as_ref().map(AsJson); }
        }

        /// `sendMessage` (`send_message` is a name that Concord keeps).
        POST post_message(chat_id: ChatId, text: String, link_preview_options?: LinkPreviewOptions,
                          ..formatted, ..delivery) -> Message {
            path "sendMessage";
            body params(json!({
                "chat_id": chat_id, "text": text, "link_preview_options": link_preview_options,
                "parse_mode": parse_mode, "entities": entities,
                "message_thread_id": message_thread_id, "disable_notification": disable_notification,
                "protect_content": protect_content, "reply_parameters": reply_parameters,
                "reply_markup": reply_markup
            }));
        }

        /// `sendPhoto`: a `file_id`, a URL that Telegram fetches, or bytes.
        POST post_photo(chat_id: ChatId, photo: InputFile, caption?: String, ..formatted, ..delivery) -> Message {
            path "sendPhoto";
            body upload("photo", photo, json!({
                "chat_id": chat_id, "caption": caption,
                "parse_mode": parse_mode, "caption_entities": entities,
                "message_thread_id": message_thread_id, "disable_notification": disable_notification,
                "protect_content": protect_content, "reply_parameters": reply_parameters,
                "reply_markup": reply_markup
            }));
        }

        /// `sendDocument`: the same three ways. (`thumbnail` is left out.)
        POST post_document(chat_id: ChatId, document: InputFile, caption?: String,
                           disable_content_type_detection?: bool, ..formatted, ..delivery) -> Message {
            path "sendDocument";
            body upload("document", document, json!({
                "chat_id": chat_id, "caption": caption,
                "disable_content_type_detection": disable_content_type_detection,
                "parse_mode": parse_mode, "caption_entities": entities,
                "message_thread_id": message_thread_id, "disable_notification": disable_notification,
                "protect_content": protect_content, "reply_parameters": reply_parameters,
                "reply_markup": reply_markup
            }));
        }

        /// `editMessageText` for a message in a chat: the message as it is now.
        POST edit_message_text(chat_id: ChatId, message_id: i64, text: String,
                               reply_markup?: InlineKeyboardMarkup, ..formatted) -> Message {
            path "editMessageText";
            body params(json!({
                "chat_id": chat_id, "message_id": message_id, "text": text, "reply_markup": reply_markup,
                "parse_mode": parse_mode, "entities": entities
            }));
        }

        /// `editMessageText` for a message that was sent inline: Telegram answers `true`.
        POST edit_inline_message_text(inline_message_id: String, text: String,
                                      reply_markup?: InlineKeyboardMarkup, ..formatted) -> bool {
            path "editMessageText";
            body params(json!({
                "inline_message_id": inline_message_id, "text": text, "reply_markup": reply_markup,
                "parse_mode": parse_mode, "entities": entities
            }));
        }

        /// `true`. Most messages can be deleted only in their first 48 hours (unsure of every case).
        POST delete_message(chat_id: ChatId, message_id: i64) -> bool {
            path "deleteMessage";
            body params(json!({ "chat_id": chat_id, "message_id": message_id }));
        }

        /// Ends the wait on an inline button; `text` shows as a notice, or as an alert.
        POST answer_callback_query(callback_query_id: String, text?: String, show_alert?: bool,
                                   url?: String, cache_time?: u32) -> bool {
            path "answerCallbackQuery";
            body params(json!({
                "callback_query_id": callback_query_id, "text": text, "show_alert": show_alert,
                "url": url, "cache_time": cache_time
            }));
        }

        /// A chat, with everything that Telegram tells a bot about it.
        GET get_chat(chat_id: ChatId) -> ChatFullInfo {
            path "getChat";
            query { chat_id; }
        }

        /// Where a file is: its `file_path`, good for at least an hour. 20 MB at most.
        GET get_file(file_id: String) -> File {
            path "getFile";
            query { file_id; }
        }

        /// The bytes of a file, from the `file_path` of `get_file`: another path on the same host.
        GET download(file_path: String) -> ByteStream {
            base "https://api.telegram.org/file/bot{client.token}";
            path "{*file_path}";
            read Stream;
            status { remove 2xx }                                    // the bytes are not `{ "ok": … }`
        }

        /// Updates go to `url`, and `get_updates` answers `409`. (`certificate`, `ip_address` left out.)
        POST set_webhook(url: String, secret_token?: Secret, max_connections?: u32,
                         allowed_updates?: Vec<UpdateKind>, drop_pending_updates?: bool) -> bool {
            path "setWebhook";
            body params(json!({
                "url": url, "secret_token": secret_token, "max_connections": max_connections,
                "allowed_updates": allowed_updates, "drop_pending_updates": drop_pending_updates
            }));
            idempotent drop_pending_updates != &Some(true);          // a second drop loses what came between
        }

        /// Back to `get_updates`.
        POST delete_webhook(drop_pending_updates?: bool) -> bool {
            path "deleteWebhook";
            body params(json!({ "drop_pending_updates": drop_pending_updates }));
            idempotent drop_pending_updates != &Some(true);
        }
    }
}
```

Reads use `GET` with a query, writes use `POST` with a JSON body: Telegram takes either for any method, and the description picks one for each endpoint. Everything that is not `{ "ok": true, … }` becomes a `TgProblem`: a non-2xx through `problem`, an `ok: false` inside a `200` (unsure that Telegram ever sends one) through the client's arm.

## Beside the macro

The data types, by name: `User`, `Update` (`update_id`, `message?`, `callback_query?`, …), `UpdateKind` (`message`, `callback_query`, …), `Message`, `Chat` (its `id` is a `ChatId`), `ChatId` (untagged: an `i64`, or `"@channel"`), `ChatFullInfo`, `MessageEntity`, `ParseMode` (`MarkdownV2`, `HTML`, `Markdown`), `LinkPreviewOptions`, `ReplyParameters`, `ReplyMarkup` (four kinds of keyboard), `InlineKeyboardMarkup`, `File`, `PhotoSize`, `ResponseParameters` (`migrate_to_chat_id?`, `retry_after?`). Two in full:

```rust
/// How Telegram says no: `{ "ok": false, "error_code": 429,
/// "description": "Too Many Requests: retry after 5", "parameters": { "retry_after": 5 } }`.
#[derive(Debug, Clone, Deserialize)]
pub struct TgProblem {
    pub error_code: i32,
    pub description: String,
    pub parameters: Option<ResponseParameters>,
}

/// A file to send: one that Telegram has, one on the web, or bytes.
#[derive(Clone)]
pub enum InputFile {
    Id(String),                                      // a `file_id` from an earlier message
    Url(String),                                     // Telegram fetches it (5 MB for a photo, 20 MB else: unsure)
    Upload { name: String, bytes: bytes::Bytes },    // multipart (10 MB for a photo, 50 MB else)
}
```

The arm of the client, as in the guide's `Vault` (chapter 5, section 5.6):

```rust
#[derive(Deserialize)]
pub struct Reply<T> {
    ok: bool,
    result: Option<T>,
    error_code: Option<i32>,
    description: Option<String>,
    parameters: Option<ResponseParameters>,
}

impl<T> Reply<T> {
    pub fn into_result(self) -> Result<T, TgProblem> {
        match (self.ok, self.result) {
            (true, Some(result)) => Ok(result),
            _ => Err(TgProblem {
                error_code: self.error_code.unwrap_or(0),
                description: self.description.unwrap_or_default(),
                parameters: self.parameters,
            }),
        }
    }
}
```

The two body functions. `params` exists because a body made of inputs writes `null` for an input that was not set (finding 4). `upload` exists because no writer for a body of several parts ships (finding 5).

```rust
type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// `body params(json!({ … }));`: JSON, without the keys of the inputs that were not set.
pub fn params(mut fields: Value) -> Result<concord::Body, BoxError> {
    if let Some(map) = fields.as_object_mut() { map.retain(|_, value| !value.is_null()); }
    Ok(concord::Body::new(serde_json::to_vec(&fields)?, "application/json"))
}

/// `body upload("photo", photo, json!({ … }));`: JSON for an id or a URL, multipart for bytes.
pub fn upload(field: &str, file: &InputFile, mut fields: Value) -> Result<concord::Body, BoxError> {
    let (name, bytes) = match file {
        InputFile::Id(text) | InputFile::Url(text) => {
            fields[field] = Value::String(text.clone());
            return params(fields);
        }
        InputFile::Upload { name, bytes } => (name, bytes),
    };
    if name.contains(['"', '\r', '\n']) { return Err("a file name with a quote or a line break".into()); }
    let boundary = format!("tg-{:032x}", rand::random::<u128>());
    let mut out = Vec::with_capacity(bytes.len() + 2048);
    for (key, value) in fields.as_object().into_iter().flatten() {
        let text = match value {
            Value::Null => continue,                         // not set
            Value::String(text) => text.clone(),
            other => other.to_string(),                      // a number, a bool; an object as JSON text
        };
        write!(out, "--{boundary}\r\ncontent-disposition: form-data; name=\"{key}\"\r\n\r\n{text}\r\n")?;
    }
    write!(out, "--{boundary}\r\ncontent-disposition: form-data; name=\"{field}\"; filename=\"{name}\"\r\n")?;
    write!(out, "content-type: application/octet-stream\r\n\r\n")?;
    out.extend_from_slice(bytes);
    write!(out, "\r\n--{boundary}--\r\n")?;
    Ok(concord::Body::new(out, format!("multipart/form-data; boundary={boundary}")))
}
```

The entry of `allowed_updates` in a query (finding 9), the long poll (finding 6), and the retry that the guide already gives for Telegram (chapter 6, section 6.3):

```rust
/// `"allowed_updates" = allowed_updates.as_ref().map(AsJson);`: a value as JSON text.
pub struct AsJson<'a, T>(pub &'a T);

impl<T: Serialize> Serialize for AsJson<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = serde_json::to_string(self.0).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&text)
    }
}

impl telegram::GetUpdates {
    /// Telegram's `timeout`, and Concord's clock for silence ten seconds longer.
    pub fn long_poll(self, seconds: u32) -> Self {
        self.timeout(seconds).with_idle_timeout(Duration::from_secs(u64::from(seconds) + 10))
    }
}

/// Flood control: wait as long as Telegram says. It helps the `GET`s only (finding 7).
pub fn retry() -> concord::Retry<TgProblem> {
    concord::Retry::standard().when(|tried| {
        let seconds = tried.problem()?.parameters.as_ref()?.retry_after?;
        Some(Duration::from_secs(seconds))
    })
}
```

## Calling it

```rust
// 1. A bot that answers, by long polling. Following updates is a loop of the caller.
let tg = Telegram::new(std::env::var("BOT_TOKEN")?).with_retry(telegram::retry());
println!("running as @{}", tg.get_me().await?.username.unwrap_or_default());
let mut offset = None;
loop {
    let updates = tg.get_updates()
        .offset(offset)
        .allowed_updates([UpdateKind::Message, UpdateKind::CallbackQuery])
        .long_poll(50)                                   // Telegram holds 50 s; Concord waits 60 s
        .await?;
    for update in updates {
        offset = Some(update.update_id + 1);             // the next poll confirms this one
        handle(&tg, update).await?;
    }
}
```

```rust
// 2. A reply, and a button. A `POST` is never sent again by Concord: flood control is this loop.
async fn handle(tg: &Telegram, update: Update) -> telegram::Result<()> {
    if let Some(msg) = update.message {
        let text = format!("You said: {}", msg.text.as_deref().unwrap_or("…"));
        loop {
            match tg.post_message(&msg.chat.id, &text).reply_parameters(ReplyParameters::to(msg.message_id)).await {
                Err(e) if e.is_status(429) => {
                    let wait = e.problem().and_then(|p| p.parameters.as_ref()?.retry_after).unwrap_or(1);
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
                sent => { sent?; break; }
            }
        }
    }
    if let Some(query) = update.callback_query {
        tg.answer_callback_query(&query.id).text("Saved").await?;
    }
    Ok(())
}
```

```rust
// 3. A photo: uploaded once, then sent again by its `file_id`.
let png = tokio::fs::read("chart.png").await?;
let sent = tg.post_photo(&chat, InputFile::Upload { name: "chart.png".into(), bytes: png.into() })
    .caption("<b>Today</b>")
    .parse_mode(ParseMode::Html)
    .with_idle_timeout(Duration::from_secs(120))         // 10 MB in memory goes out in one piece
    .await?;
if let Some(largest) = sent.photo.unwrap_or_default().pop() {
    tg.post_photo(&other_chat, InputFile::Id(largest.file_id)).await?;     // nothing uploaded
}
```

```rust
// 4. A document that a user sent to the bot, saved to disk: two calls, two paths.
let file = tg.get_file(&document.file_id).await?;
let path = file.file_path.ok_or("over 20 MB: the Bot API does not give it")?;
let mut bytes = tg.download(path).with_timeout(Duration::from_secs(300)).await?;
let mut out = tokio::fs::File::create("incoming.bin").await?;
while let Some(chunk) = bytes.try_next().await? {
    out.write_all(&chunk).await?;
}
```

## Findings

| # | Where | What got in the way | What I wrote instead | Kind |
|---|---|---|---|---|
| 1 | `post_message`, `post_photo`, `post_document` | Rule 24 refuses a child whose name starts with `send_`, anywhere. Telegram's most used method is `sendMessage`, and about twenty of its methods start with `send` (unsure of the exact count). I read rule 24 and section 4.3 twice to be sure it holds on a client, which has no `send`. | `post_…`, with the provider's name in the doc comment and in `path`. A Telegram user will type `send_message` first. | **cannot be said** |
| 2 | client `base`, `download` | The token is in the path, and a file is at `file/bot<token>/…` on the same host. With `path "bot{token}"` on the client, `download` cannot leave it: a `path` only adds, and rule 16 says to write a secret piece again or remove it, but a `path` has no form for either. | The token in the client's `base`, and a second `base` on `download`. The cost: `with_base` replaces the base, so a local Bot API server (`http://localhost:8081/bot<token>/…`) loses the token on every call, and `download` loses `file/`. Such a server cannot be reached through `with_base`. | **said with pain** |
| 3 | every `POST` | A body made of inputs has no short form like `query { a; b; }`: each input is written again in `json!`, with its key. A part gives an endpoint its inputs but cannot place them in a body, since `body` holds one value, so `formatted` and `delivery` save the signatures and every body still names all seven. | `params(json!({ … }))` that names every input. The parts have an empty block: unsure that this is legal, since every part in the guide places what it declares. | **said with pain** |
| 4 | every `POST` | In a body, an input that was not set is `null` (rule 9). Telegram's documentation does not say that `null` is the same as leaving it out (unsure). | `params`, 4 lines, which drops the `null` keys. | small |
| 5 | `post_photo`, `post_document` | No writer for a body of several parts. The body is JSON or multipart depending on the input, which works well as a function. But escaping, the boundary and JSON text for nested fields are mine to get right. The bytes are in memory: the guide shows no way to build a stream `Body` with a head and a tail around a file. | `upload`, 25 lines. The caller gives a long `with_idle_timeout` for 50 MB. | **said with pain** |
| 6 | `get_updates` | A long poll: the server is silent for up to `timeout` seconds, and Concord's clock for silence is 30. The description cannot tie the one to the other, so `.timeout(50)` on its own fails at 30 s with a Transport error. | `long_poll(seconds)`, a method of mine that sets both. The caller must know to use it (a doc comment says so). Unsure that a call type of an endpoint under the client is named `telegram::GetUpdates`: rule 24 shows only one under a scope. | **said with pain** |
| 7 | the `post_…` methods, `retry()` | Flood control is a `429` with `retry_after` in the body, on `sendMessage` above all, which is a `POST`. Concord never sends a `POST` again after a `429`, so the author's `retry()` helps only the `GET`s. `e.retry_after()` reads a header that Telegram may not send (unsure). `idempotent;` would be false after a connection that broke. | The caller's loop, which reads `e.problem()` (piece 2). Every bot writes it. | **said with pain** |
| 8 | `get_updates` | Following updates is not pages: an empty answer would end `.pages()`, and the same offset twice is a Mismatch (chapter 8, "a loop of your own"). | No `paginate`. The caller's loop (piece 1). | small |
| 9 | `get_updates`, `set_webhook` | `allowed_updates` is a JSON array in a body and JSON text in a query, while rule 8 gives a type one wire form. | `AsJson`, 7 lines, in the query entry. | small |
| 10 | `delete_message`, `set_webhook`, … | "Returns True on success": the caller gets a `bool` that is always `true`. With no arrow, the client's 2xx arm does not compile (rule 12). | `-> bool`. | small |
| 11 | every endpoint | On an API of remote calls, the leading method word (P2) says nothing (`POST delete_message`, `GET get_chat`), and each `path` is the endpoint's own name in camel case, written by hand 14 times. | As written. | small |
| 12 | `set_webhook` | Can a `Secret` be written into a JSON body? The guide covers a header, a query and a path only. | `secret_token?: Secret` inside `json!` (unsure that it serializes). | small |
| 13 | client `base` | The token holds a `:`. Rule 8 says a value is encoded for its place, but not which characters stay as they are (unsure what Telegram does with `%3A`). | Nothing. | small |
| 14 | every `chat_id` | `ChatId` is a number or `@name`, and the closed list of arguments refuses an `i64`. | In my types, `Chat.id` is a `ChatId`, so `&msg.chat.id` passes. An id from a config file is `ChatId::from(id)`. | small |
| 15 | (no middleware) | Telegram limits sends per chat (about 1 a second) and in all (about 30 a second). A middleware sees a `reqwest::Request`, so it would have to parse the body again (JSON or multipart) to find the chat. | Not written. | small |
| 16 | `download` | Without `status { remove 2xx }`, the client's arm would read the stream as a `Reply<ByteStream>`. The message would come from `rustc` (no `Reader` for that type), not from Concord. | `status { remove 2xx }`. | small |

What I wished I could write:

1. Keep `send_` on a call only, where `send` lives, so that `tg.send_message(chat, "hi")` compiles on a client.
2. A node that writes its whole path from the base, replacing the paths above it, so the token can sit in the client's `path`, where `with_base` keeps it.
3. A body made of entries that a part can add to, the way `query { chat_id; text; }` is, with an unset input left out.
5. A shipped `multipart(fields, file_field, file)` that streams the file between its head and its tail.
6. A fact on the endpoint: "silent for up to `timeout` seconds", from which Concord takes its clock for silence.
7. A fact on the client: "a `429` from this API means nothing was done", so that `retry()` may send a `POST` again after one.

## Measures

| Measure | Count |
|---|---|
| Endpoints | 14 (12 methods, `editMessageText` as two, and `download`) |
| Lines of the `api!` block | 147 |
| A name of an input written a second time only to place it | 64, for 45 inputs declared (62 on the endpoints once the parts are spread, and the token, placed twice). 55 of them are in a body (53 inside `json!`), where the wire key is written as well. |
| Rust beside the macro that exists only because the language could not say something | 16 lines, without doc comments: `params` 4, `AsJson` 7, `long_poll` 5. Also `upload`, 25 lines, which exists because no piece ships, not because of the language. The caller's flood loop (8 lines, piece 2) comes on top. |

## Scores

- **Writing this description: 6/10.** The tree, the envelope arm and `problem` took minutes. The bodies were copying, and three things sent me back to the rules: the `send_` names, where the token goes, and the long poll.
- **Reading it back a month later: 7/10.** The signatures read like Telegram's own method list. The `json!` bodies are long and add nothing, and the second `base` on `download` needs its comment.
- **The calling code: 7/10.** `tg.post_message(&chat, "hi").parse_mode(…).await?` is short and typed. But the caller must know `long_poll`, write the flood loop, and type `post_` where Telegram says `send`.
