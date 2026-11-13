# Real APIs in Concord's language

*Concord V5 design · evidence · [index](../../README.md) · the language: [grammar](../grammar.md), [rules](../rules.md) · why each choice is what it is: [alternatives](../alternatives.md)*

Real APIs, written in the language by models that had the guide only: not the decisions, not the code. The question each time: **does a real API fit, and how does it feel to write it?** Nothing here was compiled (the macro is not written), and no API was called. A detail of a provider that the writer was not sure of is marked `(unsure)` in the file.

## The descriptions of the challenge run (2026-11-12)

| API | File | Endpoints | Writer | Writing it | Reading it back | Calling it | Lines of Rust that exist only because the language could not say something |
|---|---|---:|---|---:|---:|---:|---:|
| Riot Games, in full | [riot.md](riot.md) | 75 | Opus, `xhigh` | 7 | 7 | 8 | 0 |
| Stripe, its core | [stripe.md](stripe.md) | 40 | Opus, `xhigh` | 7 | 7 | 8 | 16 |
| GitHub REST, its core | [github.md](github.md) | 42 | Sol, `xhigh` | 7 | 8 | 8 | 63 |
| An object store like S3 | [s3.md](s3.md) | 18 | Sol, `xhigh` | 7 | 7 | 8 | 22 |
| Slack Web API | [slack.md](slack.md) | 15 | Opus, `xhigh` | 6 | 7 | 8 | 35 |
| Telegram Bot API | [telegram.md](telegram.md) | 14 | Opus, `xhigh` | 6 | 7 | 7 | 16 |
| Kubernetes | [kubernetes.md](kubernetes.md) | 15 | Sol, `xhigh` | 7 | 7 | 8 | 43 |
| Elasticsearch | [elasticsearch.md](elasticsearch.md) | 15 | Sol, `xhigh` | 6 | 7 | 7 | 55 |

Scores are the writer's own, from 1 to 10. Opus is Claude, like the owner of this design: its files are not an outside view. The first four are the same APIs as in the final run, each written again by the writer who had not written it before; the first versions are kept in [`first/`](first/). The last four were chosen because they are **not** like a plain REST API: every answer a `200`, a token in the path, a long poll, a stream that never ends, the same resource under two parents, a `GET` with a body.

**These eight files were written before the last decisions of the run** (sections 3e and 3f of the sheet), and they are what brought those decisions. They are kept as written. What each would say differently today is the table below.

## What the eight descriptions changed

| What got in the way | Found in | What was decided |
|---|---|---|
| A body made of inputs: each one named a third time as a JSON key; an input that was not set went out as `null`; a part could not add to a body | Slack (28 of 48 inputs), Telegram (55 repetitions), Stripe (`expand` in every `POST`), GitHub (`PATCH`) | **A body made of entries:** `body json { chat_id; text; }`. An optional input that was not set is left out; a part may add entries. |
| An endpoint that is silent by nature: a long poll, a watch, a followed log. Every caller had to set a longer clock | Telegram, Kubernetes (twice) | **The statement `idle_timeout`**, with the inputs as variables. |
| A `429` on a `POST` that is not idempotent was never sent again; `idempotent;` would have posted twice after a lost connection | Slack, Telegram, Riot | **`Retry::not_applied_on(429)` is back**, in the policy that the author ships. |
| No writer for a body of several parts; each author wrote the boundary and the escaping by hand | Telegram, Stripe | **`multipart` is shipped.** |
| The prefix `send_` was reserved: `sendMessage` could not keep its name | Telegram | **Not reserved.** Exact names only. |
| `base "https://{route}.api.riotgames.com";` written eighteen times, once in each scope that takes a route | Riot | **A part may hold `base`.** |
| A reader of the author's passed the decoder's error on as the cause, with a value of the answer in it | (found by an attack on the guide's own example) | **A reader gives no cause:** `ReadError::not_the_type(body).with_details(…)`. |
| The guide named `IntoItems` and did not show it | GitHub | **Shown in full** in chapter 6. |

## Three, written again after those decisions

Slack, Telegram and Kubernetes were the three with the most pain. Each was written a second time, in the language after sections 3e to 3h of the sheet, **by the writer of the other family, who had not seen the first file**. The files are in [`again/`](again/).

| API | File | Writer | Writing it | Reading it back | Calling it | The first time |
|---|---|---|---:|---:|---:|---|
| Slack Web API | [again/slack.md](again/slack.md) | Sol, `xhigh` | 7 | 8 | 8 | 6, 7, 8 (Opus) |
| Telegram Bot API | [again/telegram.md](again/telegram.md) | Sol, `xhigh` | 6 | 7 | 8 | 6, 7, 7 (Opus) |
| Kubernetes | [again/kubernetes.md](again/kubernetes.md) | Opus, `xhigh` | 7 | 7 | 7 | 7, 7, 8 (Sol) |

**What it shows.** The scores moved by one point at most: a body made of entries, `idle_timeout` and a part that holds `base` took away the repetition that the first writers named (Slack: no input is named a third time now), and each new writer then met the next thing. The lines of Rust beside the macro are not counted the same way by the two families (Sol counts every function that an author writes, a middleware for a quota too; Opus counts only what the language could not say), so they are not compared here: Slack 65, Telegram 63, Kubernetes 25.

**What these three changed** (section 3i of the sheet):

| What got in the way | Found in | What was decided |
|---|---|---|
| An optional secret in a body: a `Secret` could not be an entry, and an entry with a value wrote `null` for a `None`. The author wrote the whole body by hand | Telegram (`setWebhook`) | **A `Secret` may be an entry of a body. A `None` writes nothing in every block of entries**; `"k" = Some(x);` writes `null`. |
| The guide named `multipart` and did not show what it takes; `.file(…)` ended the builder | Telegram | `multipart().text(…).file(…).finish()`, shown in full. The boundary is not random when every part is in memory. |
| A long poll that asks the same request again after an empty answer: pages stop there | Telegram (`getUpdates`) | A loop of the caller. Later: a pager that may say "again". |
| A whole URL that the server gives, with its query | Slack (the upload) | The caller sends it with `reqwest`. Later: a `base` that holds a query. |
| A family prefix with no `/` after it (`conversations.` before `list`) | Slack | The whole name in each endpoint. |
| Keys in camelCase: 17 of 23 entries are the long form, `"labelSelector" = label_selector;` | Kubernetes | The long form stays: the block is the whole wire. Later: `keys camel;`, a function of the author's. **This is the one repetition left that three of the eight APIs meet.** |
| An error record inside a stream; a certificate as an input of the client; a scope with a type parameter | Kubernetes | Unchanged (the table below). |

One fault of a description, not of the language: [telegram.md](telegram.md), the first one, gives a `Secret` to `json!`. That does not compile (a `Secret` is not written by `serde`). The file is kept as written.

## What stays, with its door

Each is a row of [chapter 8](../8-scope.md), and the case for and against it is in [alternatives](../alternatives.md).

| The API wants | Concord's answer | Found in |
|---|---|---|
| "Ask again at the new place" after a `301` | Concord's own client follows no redirect. A `reqwest::Client` of yours that follows them, given to `with_transport`. | GitHub |
| Records inside a stream, and an error record among them | A stream is bytes; a crate cuts the records. | Kubernetes (watch), Elasticsearch (bulk) |
| The same resource under two parents, written once | Two scopes. (A part that holds scopes and endpoints can come later.) | Kubernetes |
| A header whose name the caller chooses (`x-amz-meta-*`) | A method of the author's with `.edit_request(…)`. (A map of pairs as one input can come later.) | The object store |
| One endpoint under two methods (`_search` as `GET` and `POST`) | Two endpoints, or `.edit_request(…)`. | Elasticsearch |
| An answer whose shape depends on an option (`expand`, `filter_path`) | The type after the arrow is fixed. The author's type holds both shapes; `send_raw()` for the rest. | Stripe, Elasticsearch |
| A lifetime over several calls (open a point in time, walk, close) | A loop of the caller. | Elasticsearch |
| A certificate that the client cannot work without | The caller gives `concord::Http::with(…)` to `with_transport`; the author ships a function that does it. | Kubernetes |
| Keys with a piece that is `.` or `..` | They cannot be sent: the URL type would remove the piece. | The object store |
| A shared quota, the headers of every answer, a signature | A `middleware` in the description. What it keeps for the caller to read comes in as a required input of the client. | Riot, GitHub, the object store |

## How to read a file

Each has the same six sections: the description; the Rust beside the macro; pieces of calling code; the findings; the measures (how often a name is written again, how much Rust exists only because of the language); the scores. The briefs are in `experience/briefs/` (`challenge-api2-*.prompt.md`), and each writer's last message in `experience/challenge/`.
