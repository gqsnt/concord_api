# Slack

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [Kubernetes](kubernetes.md) · next: [Telegram](telegram.md)*

This page is for a Rust developer who calls the Slack Web API. It gives a client with 14 methods that you can copy, and shows an API that responds `200` to everything, method names with a dot, one page type for lists with different names, and an upload in three steps.

Here is Slack as Concord sees it. There is one host and prefix, `https://slack.com/api`, and a method name for each path: `chat.postMessage`, `conversations.list`. The scopes keep Slack's method families but add no path, because a scope's `path` would put a `/` after `conversations`. Authentication is a token, `authorization: Bearer xoxb-…`. Slack responds `200` to everything, and `"ok": false` with an `"error"` code is an error: one arm on the client reads `ok` before the endpoint's own type is read. Lists are paginated by `response_metadata.next_cursor`, and name their items `channels`, `messages` or `members`. A `429` comes with `Retry-After`, and means that the request was not applied. Slack's own reference is [docs.slack.dev/apis/web-api](https://docs.slack.dev/apis/web-api/). `(unsure)` marks a detail about Slack that has not been checked.

## The description

```rust
use concord::{by_cursor, form, json, Json, Secret};
use std::time::Duration;

concord::api! {
    /// The Slack Web API: conversations, messages, reactions, users, files and views.
    pub client Slack(token: Secret) {
        base "https://slack.com/api";
        headers { "authorization" = "Bearer {token}"; }
        read Json;
        problem SlackProblem;
        status { 2xx { body } => slack_response(body)? }

        part page(limit?: u16, cursor?: String) {
            query { limit; cursor; }
            paginate by_cursor(cursor, SlackPage::next_cursor);
        }
        part workspace(team_id?: String) {
            query { team_id; }
        }
        part content(text?: String, blocks?: Vec<Block>, attachments?: Vec<Attachment>) {
            body json { text; blocks; attachments; }
        }

        POST auth_test() -> AuthIdentity {
            path "auth.test";
            body form { }
        }
        scope conversations {
            GET list(
                types?: Vec<String>, exclude_archived?: bool, ..page, ..workspace,
            ) -> SlackPage<Conversation> {
                path "conversations.list";
                query { "types" = types.as_ref().map(|v| v.join(",")); exclude_archived; }
            }
            GET history(
                channel: String, oldest?: String, latest?: String, inclusive?: bool, ..page,
            ) -> SlackPage<Message> {
                path "conversations.history";
                query { channel; oldest; latest; inclusive; }
            }
            GET info(channel: String, include_num_members?: bool) -> ChannelInfo {
                path "conversations.info";
                query { channel; include_num_members; }
            }
            POST join(channel: String) -> ChannelInfo {
                path "conversations.join";
                body form { channel; }
            }
        }
        scope chat {
            POST post_message(
                channel: String, thread_ts?: String, reply_broadcast?: bool,
                unfurl_links?: bool, ..content,
            ) -> MessageReceipt {
                path "chat.postMessage";
                body json { channel; thread_ts; reply_broadcast; unfurl_links; }
            }
            POST update(channel: String, ts: String, ..content) -> MessageReceipt {
                path "chat.update";
                body json { channel; ts; }
            }
            POST delete(channel: String, ts: String) -> MessageReceipt {
                path "chat.delete";
                body form { channel; ts; }
            }
        }
        POST reactions_add(channel: String, timestamp: String, name: String) -> Acknowledged {
            path "reactions.add";
            body form { channel; timestamp; name; }
        }
        scope users {
            GET list(..page, ..workspace) -> SlackPage<User> { path "users.list"; }
            GET info(user: String) -> UserInfo {
                path "users.info";
                query { user; }
            }
        }
        scope files {
            /// The first step of an upload: Slack returns the URL where the bytes go.
            POST get_upload_url_external(filename: String, length: u64, alt_txt?: String) -> UploadTicket {
                path "files.getUploadURLExternal";
                body form { filename; length; alt_txt; }
            }
            /// The last step. Slack responds when the file is processed, which can take minutes.
            POST complete_upload_external(
                uploads: Vec<UploadedFile>, channel_id?: String, initial_comment?: String,
            ) -> CompletedFiles {
                path "files.completeUploadExternal";
                body json { "files" = uploads; channel_id; initial_comment; }
                idle_timeout Duration::from_secs(300);
            }
        }
        POST views_open(view: Modal, trigger_id: String) -> OpenedView {
            path "views.open";
            body json { view; trigger_id; }
        }
    }
}
```

A read is a `GET`, as Slack documents it; a write sends JSON or a form, whichever the method accepts. No write is marked `idempotent`, so `retry()` retries a write only after a `429`.

## Beside the description

The data types are plain `serde` types, named here but not shown; ids and timestamps stay `String`: `AuthIdentity` (`team_id`, `user_id`), `Conversation` (`id`), `ChannelInfo` (`channel`), `Message` (`ts`), `MessageReceipt` (`channel`, `ts`), `User` (`id`), `UserInfo` (`user`), `UploadTicket` (`upload_url`, `file_id`), `UploadedFile` (`id`, `title`), `CompletedFiles`, and the Block Kit types `Block`, `Attachment`, `Modal`, `OpenedView`.

Slack's errors, its pages, its retry policy, and the bytes of an upload:

```rust
use concord::bytes::Bytes;
use concord::reqwest;
use concord::Fail;
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

/// Slack's policy: a `429` was not applied, so even a `POST` may be retried after `Retry-After`.
pub fn retry() -> slack::Retry {
    slack::Retry::standard().not_applied_on(429)
}

/// The middle step of an upload: the bytes go to the URL that Slack returned, on another host.
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
```

## Calling it

```rust
use concord::bytes::Bytes;
use concord::Secret;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client, and who it is. An error response has the status `200`; `e.problem()` has its code.
pub async fn connect(token: Secret) -> slack::Result<Slack> {
    let slack = Slack::new(token).with_retry(retry()).with_timeout(Duration::from_secs(90));
    match slack.auth_test().await {
        Ok(me) => println!("team {}, user {}", me.team_id, me.user_id),
        Err(e) => {
            if let Some(problem) = e.problem() {
                eprintln!("Slack said no: {}", problem.error);
            }
            return Err(e);
        }
    }
    Ok(slack)
}

/// Every public channel, all pages; then one page of a channel's history, on purpose.
pub async fn channels(slack: &Slack) -> slack::Result<()> {
    let mut channels = slack.conversations().list()
        .types(["public_channel"]).exclude_archived(true).limit(200)
        .pages().items();
    while let Some(channel) = channels.try_next().await? {
        println!("{}", channel.id);
    }
    let recent = slack.conversations().history("C123").limit(15).send().await?;
    println!("{} messages", recent.items.len());
    Ok(())
}

/// Post, edit, react, delete.
pub async fn post(slack: &Slack) -> slack::Result<()> {
    let sent = slack.chat().post_message("C123").text("Deploying").unfurl_links(false).await?;
    slack.chat().update(&sent.channel, &sent.ts).text("Deployed").await?;
    slack.reactions_add(&sent.channel, &sent.ts, "white_check_mark").await?;
    slack.chat().delete(&sent.channel, &sent.ts).await?;
    Ok(())
}

/// An upload in three steps: request a URL, send the bytes there, then share the file.
pub async fn upload(slack: &Slack, name: &str, bytes: Bytes) -> Result<(), BoxError> {
    let ticket = slack.files().get_upload_url_external(name, bytes.len() as u64).await?;
    upload_bytes(&ticket.upload_url, bytes).await?;
    let file = UploadedFile { id: ticket.file_id, title: Some(name.to_owned()) };
    slack.files().complete_upload_external([file]).channel_id("C123").await?;
    Ok(())
}
```

## What does not fit well, and the workaround

| Slack wants | What to write |
|---|---|
| An error inside a `200`. `problem` alone reads only the bodies of statuses that are not 2xx. | One arm on the client, `2xx { body } => slack_response(body)?`: it reads `ok`, then returns the success or the client's problem. |
| A family prefix with no `/` after it: `conversations.` before `list`. `path` adds whole segments. | The whole method name in each endpoint; the scope only groups them. |
| A complete URL that Slack returns for the bytes of an upload, on another host, with its query. | A plain `reqwest` client sends the bytes there, with none of your keys (`upload_bytes`). Its error is a `reqwest::Error`, not a `slack::Error`. |
| A list in one value, joined by commas: `types=public_channel,private_channel`. | `"types" = types.as_ref().map(\|v\| v.join(","));`. |
| An input with the same name as the scope that contains it: `files` under `scope files`. | Another name in the signature, and the key in the entry: `"files" = uploads;`. |
| An acknowledgement that contains nothing but `"ok": true`. An endpoint with no return type reads no body, so it does not compile under the arm that reads `ok`. | A return type with no fields, `-> Acknowledged`: the arm checks `ok`. |
| A quota per method and per workspace, shared by every call. | A middleware layer that holds back each method after its `429`, as the quota on the [Riot Games](riot.md) page does. |
