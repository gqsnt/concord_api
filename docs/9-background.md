# 9. Background

*Concord V5 design · chapter 9 of 9 · [index](../README.md) · previous: [8. Scope and limits](8-scope.md) · reference: [the rules](rules.md), [the contracts](contracts.md)*

What known libraries do, what changes from V4, what changed from the first version of these docs, and how the design was finished.

## 9.1 What known libraries do

"Checked" means read in the source of the library, by the author of these docs, on the date given. "Rethink" means checked by a model of the open rethink in its own run, and not again here. "Lessons" means: from one of the five field studies of 2026-11-11 (`experience/lessons/`), where each fact has its URL and was opened by the model that wrote the study; not checked again here. The rest is from memory.

| Choice | Same choice in, or the fact behind it | Source |
|---|---|---|
| A plain signature; the wire in a block | gRPC HTTP annotations | memory |
| No prefix on methods; a call that you `.await` | `reqwest`, AWS SDK for Rust; `axum::serve` is a value that you `.await` | memory; rethink (axum 0.8.9) |
| An optional value accepts a value or an `Option` | `reqwest` (`pool_idle_timeout` and seven others) | checked, 0.13.5, 2026-11-10 |
| A type found by its spelling (`Option` and `Result` after the arrow) | `clap`, for `Option` | checked, `clap_derive` 4.6.7, 2026-11-10 |
| `{x}` and `{*x}` in a route | `axum` routes | rethink (axum 0.8.9) |
| A Refused error that keeps the body (at most 64 KiB) | Neither `reqwest` (`error_for_status`) nor `ureq` keeps it; a known pain | checked, 2026-11-10 |
| An error whose printed line holds no URL and no chain of sources (`source()` is there to ask) | V4 (`error.rs`). `reqwest`'s error prints `for url (…)`, and `anyhow` prints every source. | checked, 2026-11-10 |
| Changing something inherited takes a word | Kotlin, C#, Swift (`override`) | memory |
| A value from above names its owner | Rust itself (`self.x`) | memory |
| A default size limit | axum 2 MB; `ureq` 10 MB | checked, 2026-11-10 |
| A secret as a type that hides itself | The `secrecy` crate; `HeaderValue::set_sensitive` | memory; checked (`http` 1.5.0) |
| One trait where a request goes in and an answer comes out | Go's `http.RoundTripper`; `tower::Service` | memory |
| A response made from memory, for a test | `reqwest::Response: From<http::Response<T>>` | rethink (0.13.5) |
| The status rule apart from decoding | `reqwest` (`error_for_status` then `json`), `ureq` | checked, 2026-11-10 |
| Retry as a value on the client; safe to repeat said by the author | `tower::retry::Policy` separates "may this be tried again" from "can it be cloned"; Stripe's libraries retry a `POST` that carries an `Idempotency-Key` | rethink (tower 0.5.3); memory |
| One struct for each operation, setters, then send | AWS SDK for Rust, `octocrab`, `progenitor` | memory |
| The method at the start of the line | Every API reference (`GET /users`); Retrofit's `@GET`; `axum`'s `get(handler)` | memory |
| A value written by one serializer, wherever it goes | Smithy fixes the wire form of a value for each place. Retrofit added string converters in 2016, because `toString()` was not enough. "Calling to-string everywhere" is the first trap of the study. | lessons (`encoding`) |
| A list repeats its key | The default of OpenAPI 3 (`style: form`, `explode: true`); `requests` | lessons (`encoding`) |
| A space is `%20` | Smithy's HTTP bindings; AWS SigV4 | lessons (`encoding`) |
| A body as a value that carries its label | Retrofit converters, Feign's `Encoder`. Retrofit 1.6 took `Content-Type` back out of the header list and derived it from the body again. | lessons (`encoding`) |
| Retry by what happened, not only by the method | Go, issue 19943: a missing answer does not prove that the server did nothing. HTTP/2 `REFUSED_STREAM` is the case where it does. RFC 9110, section 9.2.2. | lessons (`retry`) |
| A random wait | Amazon, "Exponential Backoff and Jitter" (2015); now in the AWS SDKs | lessons (`retry`) |
| A try ends where data reaches the caller | gRPC commits at the response headers; `curl` warns about `--retry-all-errors` | lessons (`retry`) |
| Signing, and a token that is renewed, as a layer around what sends | `reqwest-middleware`; `tower`'s layers; the interceptors of the AWS SDKs and of OkHttp. (OkHttp's `Authenticator` and Azure's bearer policy send once more after a `401`: in Concord that is the layer's own code.) | lessons (`auth`) |
| A credential that can be replaced in place | Azure's `AzureKeyCredential.Update`; the credential providers of the AWS SDKs | lessons (`auth`) |
| The end of a list is stated for each operation | Smithy's `@paginated`. In 2024 the AWS SDK for Rust looped forever on S3 `ListParts`, because a general rule ("an empty token is the end") was wrong for it. | lessons (`pages`) |
| A short page is not the end | Slack, DynamoDB, Google's AIP-158 | lessons (`pages`) |
| Pages, and items | AWS SDK for Rust, Azure, Stripe | lessons (`pages`) |
| The same request twice in a row ends the pages | The default of the AWS SDK for Rust. botocore makes it an error; Go makes it opt-in. | lessons (`pages`) |
| A next URL is a decision about where credentials go | Wazuh's advisory of November 2026 | lessons (`pages`) |
| An error that prints no value, and a full text on demand | .NET 9 redacts URLs and header values in its logs by default. AWS ships `DisplayErrorContext`, after "dispatch failure" hid the cause. | lessons (`errors`) |
| Keep the body of a refusal | `curl --fail-with-body` (7.76.0); .NET stopped disposing the content in 3.0 | lessons (`errors`) |
| Clocks named apart | AWS SDK for Rust (attempt, operation), OkHttp (read, call), Envoy | lessons (`retry`) |

**The facts of `reqwest` 0.13.5 that this design leans on.** All checked in its source on 2026-11-11.

| Fact | Where | What this design does with it |
|---|---|---|
| On a redirect to another host it removes `Authorization`, `Cookie`, `cookie2`, `Proxy-Authorization` and `WWW-Authenticate`, and nothing else. | `redirect.rs`, `remove_sensitive_headers` | A key in `x-riot-token` would follow. So Concord's own client follows no redirect (G7). |
| It follows ten redirects by default. A policy can be a function that sees the next URL and the URLs before it, and says follow or stop. | `redirect.rs`, `Policy::custom`, `Attempt` | Concord's own client sets the policy that follows nothing, after any setting of yours. |
| It has no timeout by default: `timeout`, `read_timeout` and `connect_timeout` all start empty. | `async_impl/client.rs` | Concord's own client sets `connect_timeout` to 30 seconds. |
| Its read timeout, before the head, is one timer that starts with the request. It is not reset while a body is sent. After the head it is reset at each piece of the body. A request can carry its own total `timeout`, not its own read timeout. (Checked 2026-11-12.) | `async_impl/client.rs` (`read_timeout_fut`), `async_impl/body.rs` (`ReadTimeoutBody`), `async_impl/request.rs` | A long upload, a long poll or a slow answer could not be allowed on one call. So Concord holds the wait for an answer itself (decision D150, G8). |
| It adds `Referer` by default, and keeps no cookies by default. | `async_impl/client.rs` | Concord's own client switches `Referer` off. |
| It sends `Accept: */*` when the request has no `Accept`. | `async_impl/client.rs` | Stated in chapter 3: Concord adds no `Accept` of its own. |
| Its own retry, by default, sends again only what the protocol says was not processed, at most twice. | `retry.rs`, the default builder | Left as it is. It is below what Concord counts. |
| `Request::try_clone` gives nothing when the body is a stream. | `async_impl/request.rs` | That is the test for "a stream body is never sent twice". |
| It marks an error as a failed connection by looking for a connector's mark in the chain of causes. (Read by Astra, 2026-11-12.) | `error.rs`, `is_connect` | For a time "never sent" stood on this mark. It was not proved for every platform and TLS backend, and the promise was cut (section 9.6). |
| `reqwest::Url` is the `url` crate's type. | `lib.rs` | — |

Two more facts come from the rethink and were not checked again here: the `url` crate drops a path segment equal to `.` or `..` without a word (two models ran it); `serde_urlencoded` writes nothing for a `None`.

**Where V5 goes against the majority**

- **Placement in the block.** Retrofit, axum, FastAPI, OpenAPI and Smithy put it on the parameter. The cost is that an input is written twice.
- **No redirect is followed.** `reqwest` and `ureq` follow them, and on another host they drop the standard credential headers only. Concord cannot know that a provider's own header is safe to send there, and a followed redirect is a second request that nobody described. So its client follows none.
- **No `Accept` header from a reader.** `curl --json` and most SDKs send one. Concord labels what it sends and does not negotiate.
- **Retry is off until set.** The API SDKs of the large providers retry by default; HTTP libraries mostly do not.

**Where no precedent is known on the client side.** A client written as a tree of scopes and composed at compile time. A compile error when an inherited key is written again without a word.

## 9.2 From V4

V4 means `main` of `gqsnt/concord_api` at `4222656b`. The item-by-item list is in `notes/16_CUT_LIST.md`.

| V4 | V5 |
|---|---|
| Place words in the signature | A plain signature; placement in the block |
| Four kinds of values, with a `?` mark | Two: required, or optional by a mark on the name (`count?: u32`) |
| `set_x`, `maybe_x` | `.x(v)` |
| Four words for exceptions | `override`, `remove` |
| `base` is fixed text and may contain a path | A scheme, a host, maybe a port, maybe a path prefix; the host may have a hole, and the whole base may come from an input |
| No size limit unless written | 10 MiB by default |
| The caller brings the `reqwest` client | Concord builds a safe one by default; the caller may bring one |
| An error hands out the unread `reqwest` response | A Refused error or a Mismatch keeps the status, the headers and at most 64 KiB of the body |
| A status that means something needs a hand-written format | `status { }` arms in the description |
| No way to test without a socket | A function in place of the network |
| No logging | Concord logs, names only |
| Generated docs of about 800 words per endpoint | The signature, the author's doc, one request line |
| Separate crates for auth, retry and pagination, with their own words | Retry and pages: one line each in the description (`idempotent`, `paginate`), and a Rust value. Who you are: a `Secret` placed in `headers` or `query`; a signature is a middleware of the author's, named in the description |

## 9.3 From the first version of these docs

The first version (2026-11-10 and 11, kept in `archive/docs-v1/`) had the same tree, signature and block, and 166 rules. Eighteen independent reviews had each asked for more. On 2026-11-11 three models that had not read it started again from chapter 1, then read each other, then read it (`rethink/`). This version is the result, with the picks of decisions D113 to D124.

| First version | This version | Why |
|---|---|---|
| Concord was its own HTTP client: it followed redirects, ran a deadline, read bodies chunk by chunk. | `reqwest` does that, under the settings of `concord::Http` ([engine](engine.md), section 6). Concord keeps two timers of its own (D150). | It held most of the rules, and it closed the layer below. |
| The transport was sealed; `request()` gave a view of Concord's own. | The transport is a public trait; `request()` gives the `reqwest::Request`. | "We should allow people full control." |
| `GET ids(…) -> T { path "…"; }` | The same. (For one day it was `fn ids(…) -> T { GET "…"; }`: decision D115, taken back by D139.) | Your choice: the method at the start of the line. |
| `.send().await` | `.await` (`send()` stays) | Shorter, and nothing is lost. |
| `#[sensitive] key: String`, and a mark that followed the value | `key: Secret`, and Concord prints no value at all | A type needs no rule. Seven rules leave. |
| `format Json;`, `body task;` | `body json(task);` at the endpoint; `read X;` for the answer | The encoding is a value, said where the body is. No format to inherit and to override. |
| `subdomain "{route}";` | `base "https://{route}.api…";` | One word less. |
| `sign f(key, secret);`: headers only, once | `middleware Signed::new(key.clone());`: every try, the whole request | A nonce needs a new value at each try. |
| Retry, pages, auth: outside | Retry and pages are in: chapter 6. A credential that is a value is placed like any value; a signature and a renewed token are a middleware | Decisions D109, D207 and D236. |
| `-> stream T` with three record formats | `read Stream;` gives bytes | The formats are the job of a format crate. Parked, with their text. |
| Parts, `keep`, written defaults | `keep` and written defaults are still out (section 8.3). Parts were parked too, and came back in their smallest form (section 9.6). | Each exists because of another choice. |
| One sheet of 166 rules | One short sheet of rules for a user; contracts for the code; a sheet of the crate | A rule to learn and a test to write are not the same thing. |

## 9.4 From paper to prototype

Written the same day (decision D125), in `proto/`: a runtime crate, the macro, and the same four clients written by hand. 84 tests pass. What the first compile changed in this design is decisions D129 to D138.

What did **not** change from the first version, and was confirmed by readers who did not know it: the tree, the plain signature, the block, `Option` for "maybe absent" (until the mark `?` of section 9.7), setters, `override` and `remove`, the arms of `status { }`, one error type for a client, the three promises.

## 9.5 The forty-year pass

On 2026-11-11 and 12, after your message about the bar ([chapter 1](1-why.md), section 1.8), the design went through a loop with a stop rule: `experience/README.md`. It is on paper: the prototype was not changed, and it still has the spelling and the rules of the evening of 2026-11-11.

| Where the input came from | What |
|---|---|
| Five field studies | What older projects learned about retry, errors, pages, auth and encoding, with sources. Written without seeing this design. |
| Three case reports | Sixteen real APIs, their hardest endpoints written in the language by readers who had only these docs. |
| The author | A challenge of each part, committed before any report came back. |
| The source of `reqwest` | One fact that no reader could see on paper: what its read timeout covers. |
| Three rounds of attacks | Fresh readers with only the docs: two fast ones, and in the first two rounds two long ones (129 drills from the field studies; "the keeper for ten years"). 27 distinct findings, then 16, then 4. |

What changed in that pass, from the smaller shape of 2026-11-11. Some of these forms changed again in the final run (section 9.6):

| Before | After | Decision |
|---|---|---|
| `fn ids(…) -> T { GET "…"; }` | `GET ids(…) -> T { path "…"; }` | D139, D142 |
| `json(x);`, `form(x);`, `body(x);` | `body <expression>;` | D143 |
| A value prints with `Display`; a list in a query is parked; a space is `+` | A value is written by `serde`; a list repeats its key, or `join`; a space is `%20` | D144 |
| `base` is fixed text | `base` is text, or an input of the type `concord::Base` | D145 |
| One arm written means every answer is written | Arms add to the simple rule | D146 |
| An arrow on a `HEAD` is refused | `HEAD exists(…) -> bool` | D146 |
| The API's error is JSON | It is read by the client's reader | D147 |
| `Auth` has one method; a key is fixed | `renew` after a `401`; `Secret::replace` | D148 |
| Retry decides on the head, by the method | Retry looks at the whole try, and decides by what happened; `when` | D149 |
| 30 seconds through a setting of `reqwest`, which no call could change | Concord's own clock: 30 seconds of silence, or your `timeout` | D150, D169 |
| A pager edits a `reqwest::Request`, with keys as text | A pager is a function on the call; ready forms name inputs; `.items()`; it can fail | D151 |
| `auth_reason()`; names only | `reason()`; `details()` | D152 |
| — | After the attacks: the same request twice in a row is an error; a page that arrived is always given; `timeout` starts before `auth`; a transport sees the endpoint and the try | D159 to D166 |
| — | After your word about how it must feel, and a mixed panel of six readers: who you are is always `auth` (`header`, `query`); two clocks with two names (`timeout`, `idle_timeout`); a bare word is the API's and `with_…` is Concord's; `read`, `problem`, `idempotent`; `request()`, `send_full()`, `edit_request(…)`, `details()`; `PUT` and `DELETE` are idempotent as HTTP says; `while let Some(x) = pages.try_next().await?`; a tour to read first | D168 to D175 |
| A flat item, re-exported; `Json` when nothing is written; `auth bearer(token)`; `offset(start, count)` as a word of the language; redirects followed to the same origin | In the two-hour run, with the public surface written as real Rust and read three times: a generated type has one name; an arm binds the body that it reads; no redirect is followed; "never sent" is said by the transport. Then your rule: the macro knows no format, no scheme and no pager. So `read Json;` is written, a credential that is a value is an input of type `Secret` in `headers` or `query`, pagers are ordinary functions, and there are four crates behind `concord`. Then `auth` left altogether, at your word: a signature is a transport of the author's | D176 to D207 |

How far it got against the bar is the last section of `experience/README.md`.

## 9.6 The final run

On 2026-11-12 the owner set the goal: 5.0 is the design on paper, finished (D209).

**What was decided, and why** (D210 to D232).

| | |
|---|---|
| An arm | `200(body) => …` became `200 { body } => …`. Two readers had said that `headers` and `status` "come from nowhere" (D210). |
| `idempotent` | `remove idempotent;` became `idempotent false;`, or any `bool` expression. `remove` keeps one meaning (D212). |
| The problem's reader | `problem E as Xml;` became `problem E read Xml;`: `as` read like a cast (D213). |
| The HTTP client | What `concord::client_builder()` gave could not be trusted, and looked as if it could. `concord::Http` replaced it (D217). |
| Added | `call.read_response(response)`, the way back up from `request()` (D218). |
| Not in 5.0 | `Body::replayable`, `multipart`, a raw call on a handle: "planned" is an open point, and 5.0 has none (D227). |

**What the four real APIs showed** ([`docs/apis/`](apis/README.md)). Riot, Stripe, GitHub and an object store: 178 endpoints, all written with what the grammar has. Three of the four repeated the same inputs on every list, so parts came back (D229). One place was found where wrong use compiled: an arrow that is a `Result`, with no arm for its success (D230).

**What the two implementers' readings changed.** The engine and the macro were each written as a page, then read by a model as the engineer who must build it. Both said that they could not start coding from it, each for one blocker. For the engine (Astra, nine findings, D228): one value, `Run`, owns the clocks of a call; a clock ends a wait and nothing else; "never sent" stands on what `reqwest` reports, and is not proved. For the macro (Sol, eight findings, D232): a type of the author's is reached through one hidden trait for each client; names are bound, and the expression is copied.

The design is frozen there: ten statements and one declaration, 39 messages at compile time. Then phase E asked thirty questions, as the two implementers, and each answer is now in the paper (D234). One decision came after the freeze: `base` may stand on an endpoint (D233).

**The owner's four answers, after the run** (D235). Parts stay, and in `paginate` a name stays a place. The description got an eleventh statement, `middleware`, for what every request goes through: a signed API is now signed by `new` (D236). "Never sent" was cut: a transport says what failed, and no more (D237).

## 9.7 The challenge run

On 2026-11-12 the owner read the tour, found hardly anything, and asked for one more run: an open attack on the design, with other designs put beside it (D238).

**What was attacked.** Thirteen lanes: the rules, a real parser written from the grammar alone, other designs for the language and for the call, tables of every case, three pre-mortems, secrets, a smaller behaviour, and four blind judges for three close pairs. The sheet of the run was attacked too, and its claims about Rust were compiled: 39 cases of 39.

**What held.** The parser accepted every whole description of the paper and all four real APIs, and 180 attacks on it gave no surprise. Secrets and `Debug` resisted. The tree, the signature beside the block, the form of an arm, parts, one error type and the two clocks were each put on trial and kept. The case for and against each is in [alternatives](alternatives.md).

**What changed** (D239 to D246). An optional input is marked, `count?: u32` (four blind judges of four). An `Option` or a `Result` after the arrow is read as its inner type. A secret does not follow a change of `base`. A caller's middlewares run before the description's, so nothing changes a request after it was signed. A call with pages is not awaited directly, and `.items()` is on `Pages`. Retry is smaller, the clock for silence starts when the transport gets the request, and one log event is left of four.

**Then eight real APIs were written**, the four of the final run by other writers, with Slack, Telegram, Kubernetes and Elasticsearch: they gave a body made of entries (`body json { a; b; }`), a twelfth statement (`idle_timeout`), `multipart`, a part that may hold a `base`, and `not_applied_on` back. A second attack on the rules and a reading of the new code then gave the whole rule of a secret and a `base`, the headers in "the same request" of pages, and a reader that hands Concord no cause.

**Then a third attack on the rules** found six things, after twenty and fifteen, and the two implementers read the paper again: seven questions, three of them blocking. Slack, Telegram and Kubernetes were written again, each by the other writer, from the guide only. What came of it: a `None` writes nothing in every block of entries, a `Secret` may be an entry of a body, a secret is bound to the `base` in reach where it is written, and `multipart` ends with `finish()`, which gives a `Result`.

**Then two rounds more, and two readings beyond the plan.** A fourth attack found seven things and a fifth two, with no hole in either. The engineer of the engine said "yes, I could start", twice. The engineer of the macro had one new blocking question in each of four readings, each in the hidden interface between generated code and the core, which is now one table of [the engine](engine.md), section 2.2; that layer did not converge on paper. What came of it: a send is two hidden halves; an envelope arm from above does not reach an endpoint whose arrow is an `Option` (message 46); pages pin the origin of their first request, and `at_url` is checked on its own; a `Base` is taken only when the URL library gives back what was written.
