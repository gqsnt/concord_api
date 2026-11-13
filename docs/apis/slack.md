# the Slack Web API in Concord

Written by Claude Opus 5.5 (`claude-opus-5-5`, as the session names it), from the guide only (the tour, chapters 3 to 6, the grammar, the rules, the tables of chapter 8) and from memory of Slack's public documentation, on 2026-11-12. Nothing was compiled, and Slack was not called.

## The description

```rust
use concord::{by_cursor, Json, Secret};
use serde_json::json;

concord::api! {
    /// The Slack Web API: one method is one URL, `https://slack.com/api/<family>.<method>`.
    /// Every answer is a `200`; `{ "ok": false, "error": "…" }` is a refusal.
    pub client Slack(
        /// A bot (`xoxb-…`) or a user (`xoxp-…`) token.
        token: Secret,
    ) {
        base "https://slack.com/api";                 // `/api` here: a `path` would follow the upload host
        headers { "authorization" = "Bearer {token}"; }
        read Json;
        problem SlackProblem;
        status { 2xx { body } => Reply::into_result(body)? }     // `ok: false` is a Refused error

        /// Slack's cursor: the next one is `response_metadata.next_cursor`, empty at the end.
        part page(cursor?: String, limit?: u32) {
            query { cursor; limit; }
            paginate by_cursor(cursor, next_cursor);
        }

        scope auth {
            /// Who this token is: the team, the user, the bot.
            GET test() -> Identity { path "auth.test"; }
        }

        scope conversations {
            /// The conversations the token can see. `types` goes as one comma list.
            GET list(
                exclude_archived?: bool,
                types?: Vec<ConversationType>,
                team_id?: TeamId,
                ..page
            ) -> ChannelList {
                path "conversations.list";
                query { exclude_archived; "types" = types.as_ref().map(|t| comma(t)); team_id; }
            }

            /// The messages of a conversation, newest first.
            GET history(
                channel: ChannelId,
                oldest?: Ts,
                latest?: Ts,
                inclusive?: bool,
                include_all_metadata?: bool,
                ..page
            ) -> MessageList {
                path "conversations.history";
                query { channel; oldest; latest; inclusive; include_all_metadata; }
            }

            GET info(channel: ChannelId, include_locale?: bool, include_num_members?: bool) -> ChannelInfo {
                path "conversations.info";
                query { channel; include_locale; include_num_members; }
            }

            /// Joins a public channel. Joining again changes nothing (Slack adds a warning).
            POST join(channel: ChannelId) -> ChannelInfo {
                path "conversations.join";
                body slack_json(json!({ "channel": channel }));
                idempotent;
            }
        }

        scope chat {
            /// Slack wants `text` or `blocks` (else `no_text`).
            POST post_message(
                channel: ChannelId,
                text?: String,
                blocks?: Vec<Block>,
                thread_ts?: Ts,
                reply_broadcast?: bool,
                unfurl_links?: bool,
                unfurl_media?: bool,
                metadata?: Metadata,
            ) -> Posted {
                path "chat.postMessage";
                body slack_json(json!({
                    "channel": channel, "text": text, "blocks": blocks, "thread_ts": thread_ts,
                    "reply_broadcast": reply_broadcast, "unfurl_links": unfurl_links,
                    "unfurl_media": unfurl_media, "metadata": metadata,
                }));
            }

            /// The same update twice gives the same message.
            POST update(channel: ChannelId, ts: Ts, text?: String, blocks?: Vec<Block>) -> Updated {
                path "chat.update";
                body slack_json(json!({ "channel": channel, "ts": ts, "text": text, "blocks": blocks }));
                idempotent;
            }

            POST delete(channel: ChannelId, ts: Ts) -> Deleted {
                path "chat.delete";
                body slack_json(json!({ "channel": channel, "ts": ts }));
            }
        }

        scope reactions {
            /// `name` is the emoji without colons; `timestamp` is the `ts` of the message.
            POST add(channel: ChannelId, timestamp: Ts, name: String) -> Done {
                path "reactions.add";
                body slack_json(json!({ "channel": channel, "timestamp": timestamp, "name": name }));
            }
        }

        scope users {
            GET list(include_locale?: bool, team_id?: TeamId, ..page) -> UserList {
                path "users.list";
                query { include_locale; team_id; }
            }

            GET info(user: UserId, include_locale?: bool) -> UserInfo {
                path "users.info";
                query { user; include_locale; }
            }
        }

        scope files {
            /// Upload, step 1: where to send `length` bytes. A form, not JSON (unsure).
            POST get_upload_url_external(
                filename: String,
                length: u64,
                alt_txt?: String,
                snippet_type?: String,
            ) -> UploadTicket {
                path "files.getUploadURLExternal";
                body slack_form(json!({
                    "filename": filename, "length": length, "alt_txt": alt_txt, "snippet_type": snippet_type,
                }));
            }

            /// Upload, step 2: the bytes, to the `upload_url` of step 1. Another host, no token,
            /// and the answer is text (`OK - 1234`), not Slack's JSON (unsure).
            POST upload(upload_url: concord::Base, file: concord::Body) {
                base upload_url;
                headers { remove "authorization"; }
                status { remove 2xx }
                body file;
            }

            /// Upload, step 3: the files are shared, in a channel when `channel_id` is given.
            POST complete_upload_external(
                uploaded: Vec<FileRef>,                   // Slack's `files`: the name of this scope
                channel_id?: ChannelId,
                initial_comment?: String,
                thread_ts?: Ts,
            ) -> Completed {
                path "files.completeUploadExternal";
                body slack_json(json!({
                    "files": uploaded, "channel_id": channel_id,
                    "initial_comment": initial_comment, "thread_ts": thread_ts,
                }));
            }
        }

        scope views {
            /// A modal. `trigger_id` comes from an interaction, lives 3 seconds, and is used once.
            POST open(trigger_id: String, view: View) -> Opened {
                path "views.open";
                body slack_json(json!({ "trigger_id": trigger_id, "view": view }));
            }
        }
    }
}
```

## Beside the macro

**The answer of every method.** `Json` reads the body before the arm runs, and `{ "ok": false, … }` is not a `T`: so `ok` is looked at first, by hand.

```rust
use serde::{de::{DeserializeOwned, Error as _}, Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// `{ "ok": true, …the fields of T }` or `{ "ok": false, "error": … }`.
pub enum Reply<T> { Ok(T), No(SlackProblem) }

impl<'de, T: DeserializeOwned> Deserialize<'de> for Reply<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let body = Value::deserialize(d)?;
        let ok = body.get("ok").and_then(Value::as_bool) == Some(true);
        let reply = if ok { T::deserialize(body).map(Reply::Ok) } else { SlackProblem::deserialize(body).map(Reply::No) };
        reply.map_err(D::Error::custom)          // the place in the body (`channels[3].name`) is lost here
    }
}

impl<T> Reply<T> {
    pub fn into_result(self) -> Result<T, SlackProblem> {
        match self { Reply::Ok(value) => Ok(value), Reply::No(problem) => Err(problem) }
    }
}
```

**The data types.** Two in full:

```rust
/// How Slack says no: inside a `200`, or in the body of a `429` (unsure).
#[derive(Debug, Deserialize)]
pub struct SlackProblem {
    pub error: String,                           // `channel_not_found`, `not_in_channel`, `missing_scope`, …
    pub needed: Option<String>,                  // with `missing_scope`: the scope that was needed
    pub provided: Option<String>,
    pub response_metadata: Option<Meta>,         // its `messages` explain an `invalid_arguments`
}

#[derive(Deserialize)]
pub struct ChannelList {
    pub channels: Vec<Channel>,
    pub response_metadata: Option<Meta>,         // `Meta { next_cursor, warnings, messages }`
}

/// `{ "ok": true }` and nothing more. The client's arm reads every body, so an arrow is needed.
#[derive(Deserialize)] pub struct Done {}
```

Named only: `Identity`, `Channel`, `ChannelInfo { channel }`, `MessageList { messages, has_more, response_metadata }`, `Message`, `Posted { channel, ts, message }`, `Updated`, `Deleted`, `UserList { members, response_metadata }`, `User`, `UserInfo { user }`, `UploadTicket { upload_url, file_id }`, `FileRef { id, title }`, `Completed { files }`, `View`, `Opened { view }`, `Block`, `Metadata`, `Meta`; the newtypes over `String` `ChannelId`, `UserId`, `TeamId`, `FileId` and `Ts` (`"1503435956.000247"`); and `ConversationType` (`public_channel`, `private_channel`, `mpim`, `im`).

**Pages.** The part is shared by three endpoints with three page types, so its function cannot name one.

```rust
/// Each list names its items with its own key; the cursor is reached through this.
pub trait Listed { fn meta(&self) -> Option<&Meta>; }
impl Listed for ChannelList { fn meta(&self) -> Option<&Meta> { self.response_metadata.as_ref() } }
// … the same line for MessageList and UserList

/// `paginate by_cursor(cursor, next_cursor);` in the part: an empty cursor, or none, is the end.
pub fn next_cursor<T: Listed>(page: &T) -> Option<String> {
    page.meta()?.next_cursor.clone().filter(|c| !c.is_empty())
}

/// For `.pages().items()`. (The text of this trait is not in the guide: its shape is a guess.)
impl concord::page::IntoItems for ChannelList {
    type Item = Channel;
    fn into_items(self) -> Vec<Channel> { self.channels }
}
// … the same for MessageList (`messages`) and UserList (`members`)
```

**Bodies and one entry.**

```rust
/// `body slack_json(json!({ … }));`: JSON with the charset Slack asks for (else a `missing_charset`
/// warning, unsure), and no key whose value is `null`: an option that was not set is not sent.
pub fn slack_json(fields: Value) -> Result<concord::Body, serde_json::Error> {
    Ok(concord::Body::new(serde_json::to_vec(&no_nulls(fields))?, "application/json; charset=utf-8"))
}

/// `body slack_form(json!({ … }));`: the same pairs, as a form.
pub fn slack_form(fields: Value) -> Result<concord::Body, serde_urlencoded::ser::Error> {
    Ok(concord::Body::new(serde_urlencoded::to_string(no_nulls(fields))?, "application/x-www-form-urlencoded"))
}

fn no_nulls(mut fields: Value) -> Value {
    if let Value::Object(map) = &mut fields { map.retain(|_, v| !v.is_null()); }
    fields
}

/// `"types" = types.as_ref().map(|t| comma(t));`: `public_channel,im`, each item as serde writes it.
pub fn comma<T: Serialize>(items: &[T]) -> String {
    let texts: Vec<String> = items.iter().filter_map(|i| serde_json::to_value(i).ok()?.as_str().map(String::from)).collect();
    texts.join(",")
}
```

**Trying again.** A `429` with `Retry-After` is in the standard table already. These come inside a `200`:

```rust
/// `Slack::new(token).with_retry(slack_web::retry())`. Only a `GET` or an `idempotent;` `POST` goes again.
pub fn retry() -> concord::Retry<SlackProblem> {
    concord::Retry::standard().when(|tried| {
        let error = tried.problem()?.error.as_str();
        matches!(error, "internal_error" | "service_unavailable" | "request_timeout")   // the list: unsure
            .then_some(Duration::from_secs(1))
    })
}
```

**A token that rotates** (opt-in at Slack). No `middleware` stands in the description: the token is a value, and rotation needs inputs that most users do not have, while a middleware of the description sees only the required inputs of the client. So the crate ships one for `with_middleware`:

```rust
/// Renewed *before* it ends (12 hours, unsure): Slack says `token_expired` inside a `200`, unseen here.
pub struct Rotating { token: Secret, refresh: Secret, app: App, ends: tokio::sync::Mutex<Instant> }

impl concord::Middleware for Rotating {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let mut ends = self.ends.lock().await;
        if *ends < Instant::now() + Duration::from_secs(300) {
            let fresh = oauth_refresh(&self.app, &self.refresh).await      // `oauth.v2.access`, plain reqwest (not written)
                .map_err(|e| TransportError::blocked().with_source(e))?;
            self.token.replace(fresh.access_token);                         // requests built from now on
            self.refresh.replace(fresh.refresh_token);                      // used once (unsure)
            *ends = Instant::now() + Duration::from_secs(fresh.expires_in);
        }
        drop(ends);
        if request.headers().contains_key("authorization") {               // not on the upload host
            request.headers_mut().insert("authorization", bearer(&self.token));   // built before: again (finding 15)
        }
        next.send(request, info).await
    }
}
```

## Calling it

```rust
let slack = Slack::new(std::env::var("SLACK_BOT_TOKEN")?).with_retry(slack_web::retry());
let general = ChannelId::from("C0123456789");

// 1. Post; join first if the bot is not in the channel; answer in the thread; react.
let posted = match slack.chat().post_message(&general).text("Deploy started").await {
    Err(e) if e.problem().is_some_and(|p| p.error == "not_in_channel") => {
        slack.conversations().join(&general).await?;
        slack.chat().post_message(&general).text("Deploy started").await?
    }
    first => first?,
};
slack.chat().post_message(&general).text("Step 1 of 3 done").thread_ts(&posted.ts).await?;
slack.reactions().add(&general, &posted.ts, "rocket").await?;
```

```rust
// 2. Every channel, page by page; then one day of one channel.
let mut channels = slack.conversations().list()
    .types([ConversationType::PublicChannel, ConversationType::PrivateChannel])
    .exclude_archived(true)
    .limit(200)
    .pages()
    .items();
while let Some(channel) = channels.try_next().await? {
    println!("#{} {}", channel.name, channel.id);
}
let mut pages = slack.conversations().history(&general).oldest(&yesterday).pages();
while let Some(page) = pages.try_next().await? {
    for message in page.messages { archive(&message)?; }
}
```

```rust
// 3. A file: where to send it, the bytes, then share it.
let bytes = tokio::fs::read("weekly.pdf").await?;
let ticket = slack.files().get_upload_url_external("weekly.pdf", bytes.len() as u64).await?;
slack.files()
    .upload(ticket.upload_url.parse::<concord::Base>()?, concord::Body::new(bytes, "application/pdf"))
    .await?;
slack.files()
    .complete_upload_external([FileRef { id: ticket.file_id, title: Some("Weekly report".into()) }])
    .channel_id(&general)
    .await?;
```

```rust
// 4. A burst of posts. Concord sends no `POST` again after a `429`: the caller waits by hand.
for line in report.lines() {
    loop {
        match slack.chat().post_message(&general).text(line).await {
            Err(e) if e.is_status(429) => tokio::time::sleep(e.retry_after().unwrap_or(Duration::from_secs(1))).await,
            sent => { sent?; break; }
        }
    }
}
```

## Findings

| # | Where | What got in the way | What I wrote instead | Kind |
|---|---|---|---|---|
| 1 | `chat.post_message`, every write | A `429` is Slack's "not done, wait `Retry-After`", and `chat.postMessage` is the method that meets it most (about one per second per channel, unsure). An unmarked `POST` is never sent again. `idempotent;` would also send it again after a lost connection: a message posted twice. `idempotent` sees the inputs, not the failure, and a `when` rule answers only the third question. | Nothing in the description; a loop of the caller on `e.retry_after()` (calling code 4), in every program that posts. | **cannot be said** |
| 2 | client `status` | Every answer is a `200`, and the refusal is a field of the body. The reader decodes one type before the arm looks, and `{ "ok": false }` is not a `T`: `Reply<T>` cannot be a derive (I read 5.6 twice before I believed that `flatten` would not do it). Through `serde_json::Value`, a `T` that does not decode loses the place in the body that `Json` promises. | `status { 2xx { body } => Reply::into_result(body)? }`, and `Reply` with a hand `Deserialize`. A reader of mine could decode the bytes twice and keep the place, for about 15 lines more. | **said with pain** |
| 3 | all 14 methods | `chat.postMessage` is one segment, and `path` adds whole segments: the family cannot be said once, on its scope. It is written twice in each method (the scope, the `path`), and the method in two spellings (`post_message`, `postMessage`). | Scopes with no `path`; each endpoint writes its whole segment. | **said with pain** |
| 4 | 8 bodies | A body is one expression, not entries. Each input placed in JSON is written a third time, as its key (27 times); an option not set writes `null`, and what Slack does with `"thread_ts": null` is not documented (unsure); a part cannot add `text` and `blocks` to the bodies of `post_message` and `update`, so they are written in both. | `slack_json(json!({ … }))` and `slack_form(…)`, which drop the nulls (and write the charset Slack asks for). | **said with pain** |
| 5 | token rotation | Slack says `token_expired` inside a `200`. A middleware gets a `reqwest::Response`, so the "renew on `401`" of 6.2 cannot see it, short of reading the body and making a new response. An opt-in rotation cannot be a `middleware` line (it would need optional inputs). It must also not add the token where the description removed it. | `Rotating`, renewed before its time ends, shipped for `with_middleware`, and looking for the header before it writes it. | **said with pain** |
| 6 | scopes `auth`, `reactions`, `views` | Advice 2 and 3 (a scope adds something seen; no scope for one endpoint) against advice 4 (the provider's words). Slack's family is a word of the URL, but not a segment. | The family as a scope: a full crate has many methods in each. | small |
| 7 | 7 reads | Slack takes `GET` or `POST` for a read (unsure for each). With `POST` and a form, the part could not place `cursor` (row 4), and no read would be tried again without `idempotent;` on each. | `GET`, with a query. | small |
| 8 | `reactions.add` | No arrow is refused under the client's arm, and `()` cannot be read from `{ "ok": true }`. Whether `-> ()` counts as "no arrow" is not said. | `-> Done`, an empty struct. | small |
| 9 | `files.upload` | The URL comes whole from the server, on another host. Chapter 8 says: a plain `reqwest`. The grammar lets an endpoint's `base` name its own input of type `concord::Base`, which I read three times before I believed it. Whether a `Base` keeps a path that the server made, and a query (unsure whether Slack's has one), is not said. And `/api` must be in the client's `base`: a client `path "api"` would follow the upload host. | `base upload_url;`, `remove "authorization";` (made necessary by the secret rule, rightly), `status { remove 2xx }`. | small |
| 10 | `files.complete_upload_external` | Slack's input `files` has the name of its scope, an owner in reach. | `uploaded`, and `"files": uploaded`. | small |
| 11 | part `page` | The pager's function must take a page type, and each endpoint that names the part has its own. | A trait `Listed` and a generic `next_cursor<T: Listed>`. That inference picks `T` inside the copied expression is unsure. | small |
| 12 | `.items()` | The text of `IntoItems` and the function type that `by_cursor` takes are not on the pages I may read. Each list names its items with its own key (`channels`, `messages`, `members`). | Three impls, of a guessed shape. | small |
| 13 | `conversations.list` | `types` is one comma list; a `Vec` repeats its key, and `join` needs text, while the items are an enum. | `comma(t)`, 4 lines. | small |
| 14 | errors | A refusal is a `200`: `e.is_status(…)` says nothing, and `{e}` cannot show `channel_not_found`, a stable code that is no secret. | `e.problem().is_some_and(\|p\| p.error == "…")` at each call. | small |
| 15 | `Rotating` | The guide does not say how code of mine reads a `Secret` to write it into a header. | `bearer(&self.token)`, not written. | small |
| 16 | `chat.post_message` | "`text` or `blocks`" is a rule of Slack's; two optional inputs cannot say "one of". | Both optional, and a doc line. | small |
| 17 | calls | An id from a config file is `ChannelId::from("C0…")`: text is not taken for a newtype. | As the guide says. | small |

What I wished I could write:

1. `idempotent on 429;`: this `POST` may go out again after a `429`, and only then.
2. `problem SlackProblem in 2xx if "ok" == false;`: the client says once that a 2xx with `ok: false` is a refusal, and `T` is read only when it is not.
3. `scope chat { prefix "chat."; POST post_message(…) { path "postMessage"; } }`.
4. `body json { channel; text; blocks; thread_ts; }`: entries, a `None` not sent, and a part may add some.
5. A middleware that is told the API's problem of the answer, as a retry rule is: `answer.problem()?.error == "token_expired"`.

## Measures

| | |
|---|---|
| Endpoints | 15: the 14 methods, and the upload to the URL that Slack gives. |
| Lines of the `api!` block | 162, from `concord::api! {` to its `}`. |
| Inputs | 48 declared (2 of them in the part, which 3 endpoints take: 52 as the endpoints see them). |
| A name written a second time only to place it | 48 of 48: every input once (17 in a query, 28 in a body made with `json!`, 1 in a template, 1 as `base`, 1 as `body`). Plus `cursor` once more in `paginate`, which names its place. |
| …and a third time | 28: the key in `json!` that repeats the name (27), and `"types" = types`. |
| Rust beside the macro only because the language could not say something | 35: the hand `Deserialize` of `Reply` (8), `no_nulls` and the two body functions (10), `Listed` and its three impls (4), three `IntoItems` (12), `Done` (1). `comma` is not counted: an expression can say it. And 7 lines of the caller's loop on a `429` (calling code 4). |

## Scores

- **Writing this description: 6/10.** The reads went fast and the `part` paid at once, but the `200` envelope, the body that has no entries and the `429` rule each cost a stop and a second reading.
- **Reading it back a month later: 7/10.** The signatures read like Slack's own method pages, but the client's arm and `slack_json(json!(…))` mean nothing without the Rust beside them, and each family is written twice.
- **The calling code: 8/10.** `slack.chat().post_message(&c).text("…").thread_ts(&ts)` reads like Slack's Node SDK; the two blemishes are matching on `e.problem()` for every refusal and the hand loop on a `429`.
