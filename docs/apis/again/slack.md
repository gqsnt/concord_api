# the Slack Web API in Concord
Written by GPT-6 (Codex), from the eight permitted Concord guide pages and Slack's public documentation, on 2026-11-12.

## The description

This is a selected input surface for all fourteen requested methods, plus the byte upload. Bearer authentication, dotted RPC paths, JSON/form bodies and `ok` are from Slack's [Web API guide](https://docs.slack.dev/apis/web-api/). Each method links to its reference below. Application refusals arrive inside HTTP 200; HTTP 429 carries [Retry-After](https://docs.slack.dev/apis/web-api/rate-limits/).

```rust
use concord::{by_cursor, form, json, Base, Body, Json, Secret};
use std::time::Duration;

concord::api! {
    pub client Slack(token: Secret) {
        base "https://slack.com/api";
        headers { "authorization" = "Bearer {token}"; }
        read Json;
        problem SlackProblem;
        status { 200 { body } => slack_response(body)? }
        /// Shares a method's observed 429 cooldown within this client/workspace.
        middleware Cooldown::default();

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
            GET list(types?: Vec<String>, exclude_archived?: bool, ..page, ..workspace) -> SlackPage<Conversation> {
                path "conversations.list";
                query { "types" = types.as_ref().map(|v| v.join(",")); exclude_archived; }
            }
            GET history(channel: String, oldest?: String, latest?: String, inclusive?: bool, include_all_metadata?: bool, ..page) -> SlackPage<Message> {
                path "conversations.history";
                query { channel; oldest; latest; inclusive; include_all_metadata; }
            }
            GET info(channel: String, include_locale?: bool, include_num_members?: bool) -> ChannelInfo {
                path "conversations.info";
                query { channel; include_locale; include_num_members; }
            }
            POST join(channel: String) -> ChannelInfo {
                path "conversations.join";
                body form { channel; }
            }
        }
        scope chat {
            POST post_message(channel: String, thread_ts?: String, reply_broadcast?: bool, unfurl_links?: bool, unfurl_media?: bool, ..content) -> MessageReceipt {
                path "chat.postMessage";
                body json { channel; thread_ts; reply_broadcast; unfurl_links; unfurl_media; }
            }
            POST update(channel: String, ts: String, ..content) -> MessageReceipt {
                path "chat.update";
                body json { channel; ts; }
            }
            POST delete(channel: String, ts: String) -> DeletedMessage {
                path "chat.delete";
                body form { channel; ts; }
            }
        }
        POST reactions_add(channel: String, timestamp: String, name: String) -> Acknowledged {
            path "reactions.add";
            body form { channel; timestamp; name; }
        }
        scope users {
            GET list(include_locale?: bool, ..page, ..workspace) -> SlackPage<User> {
                path "users.list";
                query { include_locale; }
            }
            GET info(user?: String, include_locale?: bool) -> UserInfo {
                path "users.info";
                query { user; include_locale; }
            }
        }
        scope files {
            POST get_upload_url_external(filename: String, length: u64, alt_txt?: String, snippet_type?: String) -> UploadTicket {
                path "files.getUploadURLExternal";
                body form { filename; length; alt_txt; snippet_type; }
            }
            /// Completion waits for asynchronous processing; 300s is this crate's allowance, not Slack's SLA.
            POST complete_upload_external(uploads: Vec<UploadedFile>, channel_id?: String, initial_comment?: String, thread_ts?: String) -> CompletedFiles {
                path "files.completeUploadExternal";
                body json { "files" = uploads; channel_id; initial_comment; thread_ts; }
                idle_timeout Duration::from_secs(300);
            }
            /// Narrow form: a returned address representable as Base, including its path, without a query.
            POST upload(upload_base: Base, file: Body) {
                base upload_base;
                headers { remove "authorization"; }
                body file;
                status { override 200 => (), 2xx => Err(concord::mismatch("byte upload did not return HTTP 200"))? }
            }
        }
        POST views_open(view: Modal, trigger_id?: String, interactivity_pointer?: String) -> OpenedView {
            path "views.open";
            body json { view; trigger_id; interactivity_pointer; }
        }
    }
}
```

The scopes retain Slack's families; none adds a path segment. The singleton methods stay on the client. Read methods follow the documented GET forms; form bodies are deliberate on `join`, `delete`, reactions and the upload ticket. No mutating POST is marked idempotent.

## Beside the macro

These are ordinary public serde models; only the two nontrivial data shapes are expanded. IDs and timestamps stay strings. Unknown object fields are accepted; enum-like provider text stays extensible. `Block`, `Attachment` and `Modal` are named, serializable Block Kit models, not Concord syntax.

| Types | Fields needed here; provider reference |
|---|---|
| `AuthIdentity` | `team_id`, `user_id`: String; optional identity details ([auth.test](https://docs.slack.dev/reference/methods/auth.test/)) |
| `Conversation`, `ChannelInfo` | Conversation: `id`: String, optional name/flags; ChannelInfo: `channel`: Conversation ([info](https://docs.slack.dev/reference/methods/conversations.info/), [join](https://docs.slack.dev/reference/methods/conversations.join/)) |
| `Message`, `User`, `ResponseMetadata` | Message: `ts`: String, optional text/user/subtype; User: `id`: String, optional profile/flags; metadata: `next_cursor`: Option<String>, optional messages/warnings ([history](https://docs.slack.dev/reference/methods/conversations.history/), [users.list](https://docs.slack.dev/reference/methods/users.list/)) |
| `UserInfo` | `user`: User ([users.info](https://docs.slack.dev/reference/methods/users.info/)); the reference marks the input user optional; omitted-user semantics are (unsure) |
| `MessageReceipt`, `DeletedMessage` | Both: `channel`, `ts`: String; receipt also has `message`: Option<Value>, `text`: Option<String> ([postMessage](https://docs.slack.dev/reference/methods/chat.postMessage/), [update](https://docs.slack.dev/reference/methods/chat.update/), [delete](https://docs.slack.dev/reference/methods/chat.delete/)) |
| `Acknowledged` | Struct with zero named fields, deriving Deserialize; unknown fields accepted, so `{ "ok": true }` decodes ([reactions.add](https://docs.slack.dev/reference/methods/reactions.add/)) |
| `UploadTicket`, `UploadedFile`, `CompletedFiles`, `SlackFile` | Ticket: `upload_url`, `file_id`: String; UploadedFile: `id`: String, optional title omitted when absent; CompletedFiles: `files`: Vec<SlackFile>; SlackFile: id and optional title ([ticket](https://docs.slack.dev/reference/methods/files.getUploadURLExternal/), [completion](https://docs.slack.dev/reference/methods/files.completeUploadExternal/)) |
| `Block`, `Attachment`, `Modal`, `OpenedView`, `OpenedModal` | Block Kit payloads; Modal serializes its kind as `type`; OpenedView: `view`: OpenedModal; OpenedModal: id/hash strings ([views.open](https://docs.slack.dev/reference/methods/views.open/), [JSON modal example](https://docs.slack.dev/surfaces/modals/)) |

```rust
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use concord::{Fail, Next, TransportError, TryInfo};
use tokio::{sync::Mutex, time::{sleep_until, Instant}};

#[derive(Debug, Deserialize)]
pub struct SlackProblem {
    pub error: String,
    #[serde(flatten)]
    pub details: Map<String, Value>,
}

#[derive(Deserialize)]
pub struct SlackPage<T> {
    #[serde(alias = "channels", alias = "messages", alias = "members")]
    pub items: Vec<T>,
    pub response_metadata: Option<ResponseMetadata>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
```

Slack has no uniform `result` field: success fields and `ok` are siblings. The arm first decodes a JSON value, checks `ok`, then decodes the endpoint's success type. Even a refusal carrying malformed success fields remains a refusal. Missing/nonboolean `ok`, missing error codes and malformed successes are Mismatches. The arm's error retains HTTP 200 and the typed problem; Concord keeps no original body bytes after this arm consumes them.

The three list keys are normalized in `SlackPage`; metadata and other top-level fields remain available. Its cursor method follows [Slack's pagination contract](https://docs.slack.dev/apis/web-api/pagination/): empty, null or absent ends the list, never a short or empty items array. Aliases accept whichever one of those keys is present; this trades per-endpoint envelope strictness for one shared part.

The following support block is written in full. `Cooldown` is a behavioral state holder, not a provider data model. Use one Slack client per credential/workspace; this middleware shares observed method cooldowns, including across pages and retries. It cannot recall requests already in flight or coordinate separately constructed clients. It skips byte uploads by endpoint name because middleware cannot be removed below the client. It does not resend anything itself.

```rust
pub fn slack_response<T: DeserializeOwned>(body: Value) -> Result<T, Fail<SlackProblem>> {
    match body.get("ok").and_then(Value::as_bool) {
        Some(false) => {
            let problem: SlackProblem = serde_json::from_value(body)
                .map_err(|_| concord::mismatch("Slack refusal has no string error code"))?;
            Err(problem)?
        }
        Some(true) => serde_json::from_value(body)
            .map_err(|_| concord::mismatch("Slack success does not match its declared type")),
        None => Err(concord::mismatch("Slack response has no boolean ok")),
    }
}

impl<T> SlackPage<T> {
    pub fn next_cursor(&self) -> Option<String> {
        self.response_metadata.as_ref()?.next_cursor.as_deref()
            .filter(|s| !s.is_empty()).map(str::to_owned)
    }
}
impl<T: Send> concord::page::IntoItems for SlackPage<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> { self.items }
    fn count(&self) -> usize { self.items.len() }
}

#[derive(Default)]
pub struct Cooldown(Mutex<HashMap<String, Instant>>);
impl concord::Middleware for Cooldown {
    async fn handle(&self, request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        if info.endpoint() == "slack.files.upload" { return next.send(request, info).await; }
        let method = info.endpoint().to_owned();
        loop {
            let until = self.0.lock().await.get(&method).copied();
            match until {
                Some(t) if t > Instant::now() => sleep_until(t).await,
                _ => break,
            }
        }
        let response = next.send(request, info).await?;
        if response.status() == 429 {
            let seconds = response.headers().get("retry-after").and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            if let Some(until) = seconds.and_then(|s| Instant::now().checked_add(Duration::from_secs(s))) {
                self.0.lock().await.entry(method)
                    .and_modify(|t| *t = (*t).max(until)).or_insert(until);
            }
        }
        Ok(response)
    }
}

pub fn retry() -> concord::Retry<SlackProblem> {
    concord::Retry::standard().not_applied_on(429)
}

pub async fn upload_external(url: url::Url, bytes: bytes::Bytes)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(30)).timeout(Duration::from_secs(300)).build()?;
    let response = http.post(url).header("content-type", "application/octet-stream")
        .body(bytes).send().await?;
    if response.status() != 200 {
        return Err(std::io::Error::other("byte upload did not return HTTP 200").into());
    }
    Ok(())
}
```

`retry()` opts into retries on Slack's 429 without claiming that posting twice is safe after a lost connection. Concord honors Retry-After but stops its automatic retries when the requested wait exceeds 60 seconds; the caller then gets `retry_after()`. The cooldown still delays later calls to that method. Waiting in middleware is bounded by a caller's whole-call timeout, not idle_timeout.

The [upload workflow](https://docs.slack.dev/reference/methods/files.getUploadURLExternal/) requires ticket, raw-byte POST, then completion, which may wait for processing. `files.upload` is the nearest legal description: an endpoint base with no inherited `api` path, no bearer token and no JSON success arm. It works for an address representable by Base. Whether Slack ever returns a query-bearing upload URL is (unsure); its documented contract gives a whole URL, so the general calling example uses `upload_external`, the other-origin door in the scope table. That helper uses a fresh HTTP client with no Slack credential and requires HTTP 200. Its errors are ordinary Rust errors, outside `slack::Error`. All code here is illustrative and was not executed.

## Calling it

1. Authenticate and handle a refusal whose HTTP status can still be 200.

```rust
let slack = Slack::new(token).with_retry(retry()).with_timeout(Duration::from_secs(90));
match slack.auth_test().await {
    Ok(me) => println!("{} {}", me.team_id, me.user_id),
    Err(e) => {
        if let Some(p) = e.problem() { eprintln!("{}", p.error); }
        if let Some(wait) = e.retry_after() { eprintln!("retry after {wait:?}"); }
        return Err(e.into());
    }
}
```

2. Discover conversations across every cursor, then deliberately read one history page.

```rust
let mut channels = slack.conversations().list().types(["public_channel", "private_channel"])
    .exclude_archived(true).limit(100).pages().items();
while let Some(channel) = channels.try_next().await? { println!("{}", channel.id); }
let recent = slack.conversations().history("C123").limit(15).send().await?;
let mut users = slack.users().list().limit(100).pages().items();
while let Some(user) = users.try_next().await? { println!("{}", user.id); }
```

3. Post, edit, react, delete; open a modal from an interaction's supplied modal and trigger.

```rust
let sent = slack.chat().post_message("C123").text("Deploying").unfurl_links(false).await?;
slack.chat().update(&sent.channel, &sent.ts).text("Deployed").await?;
slack.reactions_add(&sent.channel, &sent.ts, "white_check_mark").await?;
slack.chat().delete(&sent.channel, &sent.ts).await?;
slack.views_open(modal).trigger_id(trigger_id).await?;
```

4. Upload bytes to the returned address, then share the completed file.

```rust
let bytes = bytes::Bytes::from_static(b"status\nok\n");
let ticket = slack.files().get_upload_url_external("report.csv", bytes.len() as u64).await?;
upload_external(ticket.upload_url.parse()?, bytes).await?;
let files = [UploadedFile { id: ticket.file_id, title: Some("Report".into()) }];
slack.files().complete_upload_external(files).channel_id("C123").await?;
```

## Findings

| # | where | what got in the way | what you wrote instead | kind |
|---|---|---|---|---|
| 1 | External byte upload | I cannot place the server's opaque whole URL in an endpoint; Base describes an origin/path prefix, not an arbitrary address with query. | A constrained `base upload_base` endpoint and the scope table's credential-free reqwest door for the general workflow. | cannot be said |
| 2 | Family scopes | I wanted `conversations.` once. Reading the URL glue rule again confirmed that a scope path would introduce a slash. | Scopes without paths; repeat each full dotted method in the leaf. | said with pain |
| 3 | Every HTTP 200 | `problem` alone does not inspect `ok`; the guide's example has a result field that Slack lacks. I stopped to work out body type inference. | One inherited arm reading Value; generic Rust checks failure before deserializing success. | said with pain |
| 4 | Shared paging | I reread the slot rule: cursor means a place inside paginate but a borrowed optional value inside query. Different list keys also resist one page type. | One part, three serde aliases, a cursor extractor and IntoItems; accept weaker envelope strictness. | said with pain |
| 5 | reactions.add | My first instinct was no arrow for a mere acknowledgement; that would conflict with the inherited body-reading arm. | A named empty Acknowledged struct and an arrow, preserving ok:false checks. | said with pain |
| 6 | Shared cooldown | Middleware is client-wide and cannot be removed from the external upload; method/workspace coordination needs state. | A Rust map and an endpoint-name branch; documented one-workspace-per-client boundary. | said with pain |
| 7 | Input placement | Almost every input name occurs again just to say where it goes, even when the wire key is identical. | Short query/form/JSON entries, with parts for paging, workspace and message content. | small |
| 8 | conversations.list types | A Vec would repeat types keys; Slack wants one comma-separated value ([reference](https://docs.slack.dev/reference/methods/conversations.list/)). | An Option map and join expression, leaving an unset filter absent. | small |
| 9 | Completion and retries | I had to distinguish slow completion, safe 429 retry, and ambiguous failed writes. | Endpoint idle_timeout, a separate opt-in retry value and no idempotent mark on writes. | small |
| 10 | files.completeUploadExternal | The provider's input `files` is also the owner scope's name, which an input may not reuse. | Rename it to uploads and explicitly write the wire key `"files"`. | small |

Wishes, in prose only; none is proposed syntax:

1. Place a complete server-returned URL on one endpoint while explicitly excluding inherited credentials and paths.
2. Share a dotted method prefix at a scope without introducing a slash.
3. Check an envelope's success discriminator before reading a typed success, with a typed failure branch.
4. Name a cursor's request slot and response field once, independent of each list's item key.
5. Check the common response envelope and discard only the successful payload without inventing an acknowledgement model.
6. Apply shared method cooldowns to the Web API branch while excluding the upload branch declaratively.

## Measures

- Endpoints: **15**: the fourteen named Web API methods plus byte upload.
- Lines of `api!`: **96** physical lines, from `concord::api! {` through its matching `}`, including blank lines and comments; imports excluded.
- Second mentions solely for placement: **46**; inputs in all: **46** written declarations, counting each part once and the client token once. For comparison, **54** input positions after expanding parts at their uses (client counted once). A placement is one input reference in base/path/query/headers/body, including the types-to-comma-list conversion; excludes the extra pager-slot mention and inherited reuse. The body/status binding named body is not an input.
- Rust beside the macro because the grammar cannot express the behavior directly: **65** nonblank, noncomment lines in the support block, including its behavioral state holder; excludes provider data models, imports and calling code. Breakdown: envelope conversion **12**, page adapters **11**, cooldown **27**, retry policy **3**, whole-URL upload **12**. This includes the guide's intentional extension doors, not just missing placement syntax.
- No build or API request was run; no prior provider description was read.

## Scores

- Writing this description: **7/10** — parts and entry bodies remove much repetition, but whole URLs and Slack's flat success envelope made me leave the description to finish the design.
- Reading it back a month later: **8/10** — signatures and full dotted paths are recognizable, while the inherited arm and the separate upload route need their adjacent explanation.
- The calling code: **8/10** — options and page traversal read cleanly, but general uploading leaves the generated client and its error type.
