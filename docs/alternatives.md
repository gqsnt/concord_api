# For each choice, the strongest other choice

*Concord V5 design · reference · [index](../README.md) · the other sheets: [grammar](grammar.md), [rules](rules.md) · the tradeoffs: [chapter 8](8-scope.md) · the decisions: D239 to D265*

This page holds the case for and against each choice of Concord, from the challenge run of 2026-11-12. For each choice, a reader argued for the strongest other design first, with a real fragment written both ways, and judged only after that. Opus tried the twelve choices of the language, and Opus again the twelve of the calling side. Astra and Sol each named, freely, the five things they would change. Opus and Astra each tried to make the behaviour smaller. Where it was close, four blind judges (Astra, Sol, Fable, Opus) read both forms without being told which one was Concord's. Then eight real APIs were written in the language (four again; Slack, Telegram, Kubernetes and Elasticsearch for the first time), and moved some verdicts again. Last, a third attack on the rules (Astra), the two engineers who must build the engine (Astra) and the macro (Sol), and the tables of every case (Fable) read the pages, and three of the APIs were written again, each by the other provider (Slack and Telegram by Sol, Kubernetes by Opus). Every reader was a model. Opus and Fable are Claude models, like the owner of this design; Astra and Sol (Codex) are the outside view.

Each entry gives Concord's choice now, the other design, what that design wins and loses, the tradeoff, who was asked, and the verdict. **Changed in this run**: the trial changed Concord; "now" is the new form, and the other design is often the old one. **Kept**: the other design lost. **Kept, a close call**: it nearly won. **Can come later without breaking**: the other design is an addition, and a description that compiles today keeps its meaning. There are 31 entries: 12 changed in this run, 11 kept, 5 kept as close calls, and 3 that can come later.

## The describing side

### 1. An input is written twice

- **Now.** An input is declared in the signature and placed in the block. An input that nothing places does not compile.
  ```rust
  GET ids(puuid: Puuid, start?: u32, count?: u32) -> Vec<MatchId> {
      path "by-puuid/{puuid}/ids";
      query { start; count; }
  }
  ```
- **The other design.** The placement goes on the parameter (Retrofit's `@Query`). Or a rest entry, `query { .. }`, places every input that nothing else places, under its own name.
- **It wins.** An input is written once. A new optional filter is one edit, not two.
- **It loses.** On the parameter, the signature carries the wire. With `..`, the block no longer names every key, and a renamed input renames its wire key with no message.
- **Tradeoff.** Fewer words, or a block that is the whole wire.
- **Asked.** Opus (language): keep, 70%. Astra and Sol each first wanted the placement on the parameter, and changed their mind on the GitHub example (Astra: on its `labels`). Sol: "The duplication buys a useful boundary." In the real APIs, almost every input is named a second time to place it (GitHub 64 of 64, Slack 48 of 48, Kubernetes 36 of 36; written again, Slack 46 of 46 and Kubernetes 45 of 45).
- **Verdict: kept.** A rest entry can come later without breaking.

### 2. How an input says "may be left out"

- **Now.** A mark on the name: `count?: u32`. The type is the value when it is given; in the block the name is a `&Option<u32>`. `milestone?: Option<u64>` has three states: not set, set to nothing, a number. A JSON body sends them as no key, `null` and the number; a query, a header or a form sends nothing for the first two. An alias changes nothing.
- **The other design** (the form before this run). Optional by the spelling of the type, as in `clap`'s derive:
  ```rust
  GET ids(puuid: Puuid, start: Option<u32>, count: Option<u32>) -> Vec<MatchId>   // before
  ```
- **It wins.** The signature is plain Rust. The written type is the variable's type.
- **It loses.** Behind an alias, an optional input becomes required, with no message. "Not sent" and "`null`" need `Option<Option<u64>>`, and `.milestone(None)` is then ambiguous (GitHub).
- **Tradeoff.** Rust's own spelling, or a mark that says "may be left out" whatever the type.
- **Asked.** Opus (language), knowing the current form: keep, 80%. Astra and Sol each put the mark among their five changes. The four blind judges all picked the mark (95, 95, 70, 60%). Fable: "Optionality is a fact about the call, not about the value's type." The accepted cost: `?` is not Rust.
- **Verdict: changed in this run** (L1). Open to the owner's veto (V1).

### 3. The form of an arm

- **Now.** `201 { body, headers } => expr`. Naming `body` reads it. Fields are left out without `..`. Rust finds the type of `body` from the arm; where it cannot, the arm says it: `4xx { body } => { let odd: UploadError = body; Err(odd.into())? }`.
- **The other design.** A closure with typed parameters: `200 => |body: Repository, headers: &Headers| Some(Tagged::new(headers.get("etag"), body))`.
- **It wins.** The type of `body` is written, not inferred. The shape is Rust's.
- **It loses.** Longer, with the type written twice. And a closure that is not one: no capture, no `async`.
- **Tradeoff.** Written types, or a short form that is what it looks like.
- **Asked.** Opus (language): keep, 80%. In the first four real APIs, the compiler always found the type of `body`.
- **Verdict: kept.** It reads as a `match`. Several keys on one arm (`2xx | 301 { headers } => …`) can come later without breaking.

### 4. What a node inherits

- **Now.** `base`, `read` and `idle_timeout`: the nearest one. `path` adds. A key (an entry, an arm) set above is set again only with `override`, and taken away with `remove`. New: what an arm takes decides when `override` is needed. Under a client's `4xx { body } => …`, an endpoint writes `override 404 => None`. An arm is still written, overridden and removed by its key: `remove 4xx` takes away the arm written `4xx`, and nothing else.
- **The other design.** The nearest one wins for everything, as in a config overlay. No `override`.
- **It wins.** One rule. No word at each replacement that is meant (`override "accept" = …`).
- **It loses.** An endpoint that writes its own success arm drops the client's envelope check (a refusal inside a `200`) with no message. Concord had that hole before this run: a `200` under an inherited `2xx` was another key.
- **Tradeoff.** A word for each replacement, or an inherited arm lost with no message.
- **Asked.** Opus (language): keep the model, 75%, and close the hole. Astra's attack on the rules found it from the other side (`override 2xx` left an inherited `200`).
- **Verdict: changed in this run** (L3), narrowly. The model stays.

### 5. How an API says no

- **Now.** `problem E;`, once, on the client: one error type for a client. `status { }`, at any node, says what a status means there. A refusal that only one endpoint has is a value: `-> Result<Merged, MergeRefused>`.
- **The other design.** `problem` on an endpoint too, the client's as the default (Astra): `PUT merge(sha?: String) -> Merged { problem MergeRefused; }`.
- **It wins.** One channel for failures. Each endpoint's failures are exact in its own type.
- **It loses.** The error type differs between calls of one client: `?` needs conversions, and retry rules and wrappers meet many problem types.
- **Tradeoff.** Exact failures per endpoint, or `?` across every call of a client.
- **Asked.** Astra: change, among its five. Opus (language): keep, 85%. `status` and `problem` do two jobs: what a status means for this endpoint, and how this API says no.
- **Verdict: kept.**

### 6. The form of a part

- **Now.** A named group of inputs with the lines that place them, given to a node by `..name`. New: a part may hold `base` (E9).
  ```rust
  part page(limit?: u32, starting_after?: String) {
      query { limit; starting_after; }
      paginate by_cursor(starting_after, List::next_cursor);
  }
  part regional(route: Region) { base "https://{route}.api.riotgames.com"; }
  scope matches(..regional) { path "lol/match/v5/matches"; … }
  ```
- **The other designs.** A type parameter: `part page<Id>(…, starting_after?: Id)`, named `..page<CustomerId>`. Or a part that holds scopes and endpoints (Kubernetes: the same pods under two parents).
- **They win.** Stripe's seven lists keep their typed cursors (`CustomerId`, `RefundId`, …) in one part. Kubernetes writes its pods once.
- **They lose.** The first type parameter of the language. A part with children is a second tree.
- **Tradeoff.** More sharing, or a part that stays a small group of inputs.
- **Asked.** Opus (language): keep, 65%. Then the writer of Kubernetes (Sol) wrote the same pods under two parents, and the second writer of Riot (Opus) wrote its regional `base` eighteen times, once in each scope that takes a route.
- **Verdict: can come later without breaking.** A part that holds `base` came in this run (E9).

### 7. Where `middleware` stands

- **Now.** On the client only. Made once, in `new`, from the client's required inputs (an optional one is refused). Nothing takes it away. State that a caller reads (quotas) comes in as a required input.
- **The other design.** On any node, still made once from the client's inputs, for the requests below: `scope private { middleware Sign::new(client.key.clone()); … }`.
- **It wins.** The tree says what is signed. No list of endpoint names (`info.endpoint()`) beside the description, which a rename breaks with no message.
- **It loses.** An order between a client's and a scope's middlewares. "`new` signs" becomes "signs what is under `private`".
- **Tradeoff.** The tree shows what is signed, or one rule for every request.
- **Asked.** The coordinator expected a close call. Opus (language): lean keep, 55%. Slack, written again (Sol), branched on the endpoint's name in its middleware to leave its upload out of a cooldown, and wished to say that in the tree.
- **Verdict: kept, a close call.** On a scope it can come later without breaking.

### 8. The arrow

- **Now.** `-> T` is what the caller gets. No arrow gives `()`, and the body is ignored. New: an endpoint with no arrow does not compile when an arm in reach would read a body (L4). An `Option<T>` or `Result<T, E>` arrow reads a 2xx as `T` and wraps it; arms give the rest, `404 => None` (L2).
  ```rust
  // the client: status { 2xx { body } => Reply::into_result(body)? }
  DELETE cancel(id: OrderId);          // does not compile: give it an arrow (`-> Ignored`)
  ```
- **The other design.** The arrow is always written, `-> ()` too.
- **It wins.** Rule 12 is not silent. The user's view says what each endpoint gives.
- **It loses.** Noise on every `DELETE`. And it misses the real hole: under the envelope arm of chapter 5, an endpoint with no arrow could never succeed.
- **Tradeoff.** A visible `()`, or Rust's habit.
- **Asked.** Opus (language) found that hole and asked for the compile error, 75%. Astra's attack on the rules found that a `200` with a `null` body was a silent `None` (L2).
- **Verdict: changed in this run** (L2, L4). "No arrow" stays.

### 9. Templates and holes

- **Now.** `{x}` is one encoded piece of a path; `{*x}` a whole segment that keeps its `/`; a host hole stays inside one label. A value stays in its place, or the request is refused.
- **The other design.** RFC 6570 URI Templates: `path "{+key}";` for `path "{*key}";`, and `{/x*}`, `{?a,b}`.
- **It wins.** A standard, the one GitHub sends at run time (`upload_url: ".../assets{?name,label}"`).
- **It loses.** `{+x}` lets `?`, `#` and `..` through: a value leaves its place. `{?a,b}` repeats `query { }`. And `{*key}` is axum 0.8's catch-all.
- **Tradeoff.** A standard that trusts the value, or a small form that refuses it.
- **Asked.** Opus (language): keep, 80%.
- **Verdict: kept.**

### 10. The short entry `name;`

- **Now.** In `query`, `name;` sends the input under its own name. In `headers`, a short entry whose name has `_` is refused (message 41): write `"user-agent" = user_agent;`.
- **The other design.** The key takes the form of its place: in `headers`, `_` becomes `-` (`if_match;` sends `if-match`), as FastAPI's `Header()` does.
- **It wins.** S3 writes eight entries short (`content_type;`, `if_match;`, …).
- **It loses.** "The key is the name" gets an exception by place. (Before this run, `headers { user_agent; }` compiled and sent `user_agent`, which nginx drops by default.)
- **Tradeoff.** Shorter entries, or one rule with no exception.
- **Asked.** Opus (language): change, 75%, by the conversion or the refusal. The coordinator's own list and Astra found the same trap.
- **Verdict: changed in this run** (L6), in the smaller form: refused, not converted.

### 11. Keys in another case than the inputs

- **Now.** A key in another case than its input is written in the long form. In a part, it is written once for every endpoint that names the part. The block stays the whole wire, as written.
  ```rust
  part selectors(label_selector?: String, field_selector?: String) {
      query { "labelSelector" = label_selector; "fieldSelector" = field_selector; }
  }
  ```
- **The other design.** A word that changes the case of every key, as `#[serde(rename_all = "camelCase")]` does for the fields of a struct. The second writer of Kubernetes wished for it, once on the client:
  ```rust
  keys camelCase;                                               // wished; not Concord
  query { label_selector; field_selector; resource_version; }
  ```
- **It wins.** The case is said once for the API, and the short entry fits most keys again.
- **It loses.** The macro would learn a vocabulary: the names of the cases, and how each one cuts a name. The block is no longer the whole wire: the key that goes out is written nowhere. A key that is a Rust keyword (`continue`) is still written long.
- **Tradeoff.** The case said once, or every key written where it goes.
- **Asked.** The second writer of Kubernetes (Opus): 17 of its 23 entries that place an input are the long form, for a camelCase key or the keyword `continue`. The object store and Riot have some (`"versionId"`, `"uploadId"`; `"startTime"`, `"endTime"`). The coordinator weighed the word and did not take it. No outside model (Astra, Sol) argued either side.
- **Verdict: kept**, and marked for the owner's eyes: it is the one repetition that three of the eight APIs still meet (Kubernetes, in both of its descriptions; the object store; Riot). A statement that names a function of the author's, from a name to its key (`keys camel;`), can come later without breaking: the function is the author's, and the macro copies it like every right side.

### 12. A body made of entries

- **Now.** `body json { channel; text; }` (E1). Entries are as in `query`, and `serde` writes them as one map, in order. `body json { … }` is `body json(entries)`, where `json` is a path to a function seen from where `api!` stands (an input of that name does not hide it): the macro still knows no format. A part may hold one too. A `None` writes nothing, in every block of entries: one level of `Option` is presence, and what is inside is written as the format writes it. So `milestone?: Option<u64>` sends no key, `null` or a number in JSON, and `"k" = Some(x);` with `x: Option<T>` is the way to write a `None` as `null`. A form has no `null`. A `Secret` may be an entry, written as its text: `body form { grant_type; client_id; client_secret; }`.
- **The other designs.** Before this run, a body of several inputs names each input as a key in `json!`:
  ```rust
  body slack_json(json!({ "channel": channel, "text": text, "thread_ts": thread_ts }));   // before
  ```
  Or the first rule for a body of entries: every value as `serde` writes it, a `None` as `null`, and an entry left out only when it names an optional input alone.
- **They win.** No new form: any JSON shape is written as it is. With the first rule, a value goes out exactly as `serde` writes it, and a `None` is `null` with no word of Concord's.
- **They lose.** With `json!`, each input is written a third time, an input that was not set goes out as `null`, and a part cannot place its inputs in a body. With the first rule, a `None` sent nothing in a query and `null` in a body, and an `Option` that the author made from an input went out as `null` when it was `None`.
- **Tradeoff.** One form for every body, or the common body without a third mention; `None` as `serde` writes it, or one meaning of `None` in every block.
- **Asked.** The writers of Slack and Telegram (Opus), where every method is a `POST` whose body is its inputs: 28 of Slack's 48 repetitions were in bodies, and 55 of Telegram's 64. Stripe (`expand`) and GitHub (`PATCH`, written by Sol) met the same. Written again from the guide (Sol), Slack named each of its 46 inputs a second time, and none a third. Then the tables of cases (Fable) found six cells under the first rule that no page said: an optional `client.x` that was not set, and five values through `form`. The third attack (Astra) found one of them on its own: a form loses the promised `null`. Telegram, written again (Sol), wrote 13 lines for its webhook: a `Secret` could not be an entry, and an optional one mapped to its text went out as `null` when it was not set.
- **Verdict: changed in this run** (E1; 3i.1, 3i.2). Open to the owner's veto (V7).

### 13. An upload in parts

- **Now.** `multipart` is shipped, in `concord-extras`, behind the feature `multipart` (E3): `body multipart().text("chat_id", id).file("photo", "a.jpg", "image/jpeg", photo).finish();`. `text` and `file` give the builder back; `finish()` gives a `Result` of a `Body` (a content type that is not a legal header value is its error: nothing panics). The boundary is not random when every part is in memory: the same inputs give the same bytes. With a part that is a stream, it is random, and the body goes out once. The language does not change. The core gains `Body::concat(parts)`, a body made of bodies, whose one-shot streams are taken with the whole: a stream that was already sent is a Build error before anything goes out. A writer of the author's has the same pieces (`Body::as_bytes()`, `Body::into_stream()`).
- **The other designs.** Before this run, not shipped: it is one function that gives a `Body`, and the author writes it. Or shipped, with a random boundary for every body, as the first code had it.
- **They win.** One piece less in `concord-extras`: before this run, nothing was waiting for it. With a random boundary, one rule for every body.
- **They lose.** Each author writes the boundary and the escaping by hand: 25 lines for Telegram, 17 for Stripe. With a random boundary, the same inputs give other bytes: a pager that gives back the same call is not seen to repeat.
- **Tradeoff.** Less that Concord ships, or one writer that every upload shares.
- **Asked.** No reader argued for "not shipped". The writers of Telegram and Stripe (Opus) each wrote it by hand. Then the engineer of the engine (Astra) found two faults in the shipped writer: a stream used again inside it gave 152 bytes before its error, and the random boundary broke "the same inputs, the same request". Telegram, written again (Sol), could not tell from the guide where the builder becomes a `Body`, or whether it can fail.
- **Verdict: changed in this run** (E3; 3i.10, 3i.11).

### 14. What an endpoint knows about time

- **Now.** The statement `idle_timeout <expression>;` (E2), on an endpoint, a scope, the client or a part (the nearest one), gives the clock for silence of the calls below, from the inputs:
  ```rust
  GET get_updates(offset?: i64, timeout?: u32) -> Vec<Update> {
      path "getUpdates";
      query { offset; timeout; }
      idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
  }
  ```
  The call's `with_idle_timeout` wins over it; it wins over the client's. Retry stays the caller's.
- **The other design** (before this run). Only the caller says it, on each call or on the client.
- **It wins.** One statement less. And no value that is a guess: for an upload, the silence depends on the line, not on the API.
- **It loses.** A long poll (Telegram) or a quiet stream (a Kubernetes watch) is a fact of the endpoint, yet every caller had to know it, or fail every poll at 30 seconds.
- **Tradeoff.** The fact written once, or one statement less.
- **Asked.** The coordinator's own list. Opus (language): yes, 60%. Then the writers of Telegram (Opus) and Kubernetes (Sol) each needed it. Written again, Slack and Telegram (Sol) and Kubernetes (Opus) each used it where the API is slow or silent.
- **Verdict: changed in this run** (E2). Open to the owner's veto (V7).

### 15. Everything generated is as public as the client

- **Now.** Every generated item has the client's visibility. New: `#[doc(hidden)]` is accepted on a scope or an endpoint.
- **The other design.** A node takes Rust's visibility: `pub(crate) scope multipart(…) { … }`, and the author's `upload_large` beside it is the only way in.
- **It wins.** A safe flow can be the only way in (an upload left open is billed). The tree stops being the whole semver surface.
- **It loses.** "Hide the blocks, and what is left is what a user needs" stops being true. Every wrapper in the first four real APIs sits beside a raw call that is still useful.
- **Tradeoff.** The author picks the way in, or the description is the whole public API.
- **Asked.** The coordinator's own list. Opus (language): lean keep, 55%, with `#[doc(hidden)]` as the cheap door.
- **Verdict: kept, a close call.**

### 16. One base, moved as one

- **Now.** `with_base(…)` replaces every base. A second host is a `base` on the node that lives there.
- **The other design.** A base with a name, moved by its name: `base uploads "https://uploads.github.com";` and `gh.with_base_of(Uploads, upload_base)`.
- **It wins.** One description serves github.com and GitHub Enterprise (`host/api/v3`, `host/api/uploads`).
- **It loses.** Names for bases, more generated names, and inheritance that must carry a base's name.
- **Tradeoff.** Each host moved alone, or one setting for all.
- **Asked.** Astra and Sol, each on their own, among their five. The first GitHub description had met it (its finding 14: GitHub Enterprise has two hosts).
- **Verdict: can come later without breaking.**

## The calling side

### 17. One level of the tree is one call

- **Now.** A scope is a method that gives a handle; an endpoint a method that gives a call: `gh.repo(&owner, &name).pull(4521).merge()`.
- **The other design.** Flat methods named by their path, as in the AWS SDKs: `gh.repo_pull_merge(&owner, &name, 4521)`.
- **It wins.** The author can reshape the tree without breaking callers.
- **It loses.** The handle a caller keeps. Inputs of several levels meet in one list.
- **Tradeoff.** Freedom to reshape, or calls that read like the URL.
- **Asked.** Opus (calling side): keep, 80%.
- **Verdict: kept.**

### 18. Arguments by position; what an argument takes

- **Now.** Required inputs are arguments, in order; an optional input is a method. An argument is its type, a reference, text for a `String` or a `Secret`, a slice for a `Vec`. Nothing else.
- **The other design.** Every input named, the required ones checked by type (as `bon` does): `by_riot_id().game_name("Faker").tag_line("KR1")`. Or a type that opts in to text.
- **It wins.** No swap compiles: today `by_riot_id("KR1", "Faker")` compiles and answers `None`. Text reaches a newtype with no ceremony.
- **It loses.** Every one-id call grows. Text for a newtype removes what makes position safe.
- **Tradeoff.** Calls that cannot swap, or short calls whose types protect them.
- **Asked.** Opus (calling side): keep, 70% and 75%. The two hold each other up: a newtype must be spelled at the call.
- **Verdict: kept.**

### 19. A call is a value, and Concord's own names

- **Now.** `.await` sends a call (except a paged one: entry 21); `send()` does the same. An option of the API keeps its name. Concord's settings start with `with_`, the one reserved prefix; its other names are exact (`send`, `request`, `pages`, `at_url`, …). The promise: no new name on a call, a handle or a client in a minor version, unless it starts with `with_`.
- **The other design.** The verb is required (`.send().await`, as reqwest and octocrab). One reserved method, `.with(settings)`, in place of a prefix.
- **It wins.** One spelling (`tokio::spawn` needs `send()` anyway). Discord's `with_counts` keeps its name.
- **It loses.** Seven characters on the most common line. `with` is itself a likely parameter.
- **Tradeoff.** One spelling everywhere, or the short one where most calls are.
- **Asked.** Opus (calling side): keep, 70% and 75%. Later in the run, Telegram's writer (Opus) met a prefix `send_` that this run had reserved for an hour: `sendMessage` and twenty more could not keep their names. The prefix was taken back (E5).
- **Verdict: kept.**

### 20. One error type for a client, four kinds

- **Now.** `riot::Error` is opaque: `kind()` (Build, Transport, Refused, Mismatch), `is_status()`, `problem()`, accessors that give an `Option`. New: `e.retry_after()`, the longer of two waits: the `Retry-After` of the answer, and the wait that a rule of `when` asked for the try that did not happen. Its `source()` never holds a value of the answer: Concord makes it from the place and the kind. A reader hands it no cause and no text of its own. The place is made of pieces whose names come from the program, never from the answer: `ReadError::not_the_type(body).at(Place::root().field("items").index(3))`. Free text goes in `.with_details(…)`, which only `e.details()` gives.
- **The other design.** A `#[non_exhaustive]` enum whose variants hold what each kind always has: `Err(riot::Error::Refused(r)) => r.headers().retry_after()`.
- **It wins.** What is always there is not an `Option`. Riot's writer wrote `e.headers().retry_after()`, which does not compile.
- **It loses.** Public fields: one that moves breaks callers.
- **Tradeoff.** Types that say what each kind holds, or a surface that can grow.
- **Asked.** Opus (calling side): keep, 65%.
- **Verdict: kept.** Kinds can grow without breaking. `e.retry_after()` covers the writer's case.

### 21. What `.await` does on a paged call

- **Now.** A call to an endpoint with `paginate` cannot be awaited directly:
  ```rust
  let first = repo.issues().list().send().await?;      // one page, on purpose
  let mut all = repo.issues().list().pages().items();  // every issue
  ```
- **The other design** (before this run). `.await` gives the first page, as on any call.
- **It wins.** "`.await` sends" has no exception. Adding `paginate` later breaks no build.
- **It loses.** `repo.issues().list().await?` gives 30 issues of 3,000, and nothing says so. A test with five records passes.
- **Tradeoff.** One rule for `.await`, or no partial answer that looks whole.
- **Asked.** Opus (calling side): change, 60%. The four blind judges all picked the change (80, 85, 78, 75%), accepting that adding pages breaks callers' builds.
- **Verdict: changed in this run** (B1; `.items()` on `Pages`, B2). Open to the owner's veto (V2, V5).

### 22. One endpoint, one answer type

- **Now.** The type after the arrow is fixed. New: on a `GET` or `HEAD` call, `send_if_none_match(etag)` gives an `Answer<Option<T>>`, `None` on a `304`, with its headers: `repo.get().send_if_none_match(&etag).await?`.
- **The other design.** Typed adapters on a call (Astra, Sol): `repo.get().with_condition(if_none_match(tag))`, plain Rust that may change the answer of any endpoint.
- **It wins.** Any mode of the protocol, on any endpoint.
- **It loses.** An adapter that changes the answer type needs rules with arms and pages: a large contract.
- **Tradeoff.** Any mode anywhere, or one mode with a small surface.
- **Asked.** Astra and Sol, each among their five. Four readers of two providers had asked for conditional reads.
- **Verdict: changed in this run** (B10), in the small form. General adapters were not taken.

### 23. Generated names follow the tree

- **Now.** `riot::Account` is the handle of the scope `account`. Your type `Account` stays yours.
- **The other design.** A handle says what it is: `github::IssueHandle`.
- **It wins.** In rustdoc, `pub fn issue(&self, …) -> Issue` reads like a fetch. GitHub alone has five such pairs.
- **It loses.** A suffix on every handle a user names (`bucket: &s3::Bucket`). Rust's habit is the module path (`io::Error`).
- **Tradeoff.** Names that say what they are, or short names under a module.
- **Asked.** Opus (calling side): 55% for a suffix.
- **Verdict: kept, a close call.** The generated text links the author's type by its path.

### 24. A test is a fake transport

- **Now.** `with_transport(concord::fake(|request| …))` replaces only the network. The reader, the arms and the middlewares run for real.
- **The other design.** Typed answers per endpoint, with no wire (as `aws-smithy-mocks`).
- **It wins.** An application tests its own logic without JSON written by hand.
- **It loses.** The description goes untested: a wrong `path` passes. Retry and the clocks have nothing to act on.
- **Tradeoff.** Tests of the application, or tests of the client.
- **Asked.** Opus (calling side): keep, 80%; give `fake` the `TryInfo`, 70%.
- **Verdict: kept.** A `fake` that gets the `TryInfo` can come later without breaking.

## The behaviour

### 25. A transport and middlewares

- **Now.** `with_transport` replaces what really sends; middlewares are steps above it. New: a caller's middlewares come first, then the description's in the order written, then the transport. The author writes the signer last: nothing changes a request after it is signed.
- **The other design.** One `tower` stack: `with_layer(…)` for everything.
- **It wins.** One idea, `tower`'s layers with no adapter, and the caller chooses where a layer sits.
- **It loses.** `poll_ready`, boxed errors, services that must be cloned. "Replace what sends" and "add a step" become one operation.
- **Tradeoff.** One powerful idea, or two simple ones.
- **Asked.** Opus (calling side): keep, 80%. Astra's attack showed that a caller's middleware, then placed after the description's, could change a signed request.
- **Verdict: kept.** The order changed in this run (L8).

### 26. Retry is off until set

- **Now.** Nothing is sent again until the caller writes `with_retry(…)`.
- **The other design.** Retry on by default, with the author's own policy as the default.
- **It wins.** What the author knows (GitHub's secondary limit) reaches every caller.
- **It loses.** More traffic when an API is down.
- **Tradeoff.** Good behaviour by default, or no traffic nobody asked for.
- **Asked.** Opus (calling side): keep, 55%.
- **Verdict: kept, a close call.**

### 27. Which requests are sent again

- **Now.** With retry on: a `GET`, `HEAD`, `OPTIONS`, `PUT` or `DELETE` (unless `idempotent false;`); a request whose endpoint says `idempotent;`; and a request whose status the policy lists with `not_applied_on` (entry 28).
- **The other design.** Only reads by default; a `PUT` or a `DELETE` only with `idempotent;` (as Go's `net/http`).
- **It wins.** A `DELETE` that worked but whose answer was lost is not sent again, so it never answers `404` for something that worked.
- **It loses.** Every ordinary `PUT` and `DELETE` needs a mark. It leaves HTTP's rule, the one a reader knows.
- **Tradeoff.** Safer failures, or more useful ones.
- **Asked.** Astra and Sol, asked freely, each wanted the change. Blind, three judges of four kept HTTP's rule (90, 88, 62%); Opus picked reads only (65%). Fable: "Y fails safer, X fails more usefully."
- **Verdict: kept, a close call.**

### 28. How much retry policy, and what is logged

- **Now.** `Retry::standard()`, `.tries(n)`, `.when(f)`, `.unless(f)`, `.not_applied_on(status)`. Fixed waits: random up to 1 second, then 2, then 4, at most 60, never less than a `Retry-After`. A `Retry-After` is a number of seconds or a date, which Concord reads itself. A wait over 60 seconds, from the header or from a rule, means no other try; `e.retry_after()` gives the longer of the two. Rules add up: `github::retry().when(mine)` keeps GitHub's rule. A request may be repeated by its method, its mark, or a status that the policy lists with `not_applied_on`. After `blocked`, the blocked error. One log event, `retry`.
- **The other design** (before this run). Also `first_wait` and `max_wait`; a second `when` replaced the first; after `blocked`, the error of the try before; four log events.
- **It wins.** Tuning for tests and long waits; a free log of the traffic.
- **It loses.** Knobs with no first-day user. A rule that erased the author's with no message. A result after `blocked` that nobody could guess. A middleware already sees every try.
- **Tradeoff.** More knobs, or fewer rules to learn.
- **Asked.** Opus and Astra, asked to make the behaviour smaller, agreed on these cuts. Where they differed, the run kept `when`, `unless` and `tries` (Astra would cut them) and cut three of the four log events (Astra would keep them). Opus (calling side): rules must add up, 85%. **`not_applied_on` was cut too, for two hours, on those two readers' word.** Then Slack and Telegram showed its use: a `429` on a `POST` that is not idempotent. `idempotent;` would also send it again after a lost connection, and post a message twice. Riot's tournament `POST`s meet the same `429`. So it is back (E4), in the API's own policy (`slack::retry()`).
- **Verdict: changed in this run** (B3, B4, B9; E4 took one cut back). Open to the owner's veto (V3, V4).

### 29. Two clocks, and what is silence

- **Now.** `timeout`: the whole call, no limit by default. `idle_timeout`: silence, 30 seconds, unless the call, the description (entry 14) or the client gives another length. The clock for silence watches two things only: the transport, from the moment it gets the request until the head of the answer, and a body while it is read. Each piece of a stream body going out, and each piece of the answer, starts it again. A middleware's time and a reader's own waits are never silence: only `timeout` bounds them.
- **The other designs.** A clock per phase, as OkHttp: connect, read, write, the call. Or the clock of before this run: it started at the first middleware, and a middleware that waited on purpose (a quota) called `info.sign_of_life()`.
- **They win.** An upload from memory is not silence: three of the first four real calling codes add `with_idle_timeout` for that alone. With `sign_of_life()`, a middleware that hangs before it sends is still cut at 30 seconds.
- **They lose.** Four knobs; and a write clock must see the body leave, which `reqwest` does not show. `sign_of_life()` was a trap: forget it, and a quota's wait ends at 30 seconds and is tried again through the quota.
- **Tradeoff.** Exact clocks, or two that fit in the head.
- **Asked.** Opus (calling side): keep two clocks, 75%. Opus (smaller): start the clock at the transport. Astra (smaller): keep every sign of life, `sign_of_life()` too. Then the tables of cases asked whether the clock runs while a middleware holds an answer: it does not (E6).
- **Verdict: changed in this run** (B5, E6). The two clocks are kept; what the clock for silence watches changed, and `sign_of_life()` went with it. Open to the owner's veto (V4).

### 30. Pages: when built, and where they go

- **Now.** A page's request is built when that page is asked for. Every page goes to the origin of the first; another origin is a Build error. `at_url` gives a path and a query, never an origin: a URL at another origin than the one that the call builds is a Build error, in pages and outside them. The same request twice is an error: the same method, URL (without its fragment), body bytes, and each value of each header, in order. What a secret wrote in the URL or in a header is left out (a piece of a path, a pair of the query, a value of a header), and so is a header value that `edit_request` marks `sensitive`; a body is compared as its bytes, and a secret in a host that was replaced is another origin. Both checks look at the request as built, before any middleware; a middleware that sends it elsewhere is its author's word. After an error the pages are over.
- **The other designs.** Before this run, the next request was built early. A smaller design drops the check for a repeated request. And the first answer of this run compared no headers.
- **They win.** One comparison less, or a smaller one. Without the check, a poll that asks the same request again (Telegram's `getUpdates` after an empty batch) could be pages.
- **They lose.** Built early, a page missed a replaced `Secret`. Without the check, a repeating cursor loops for ever. Without the headers, an API that pages by a header (Cosmos DB) is stopped as if it repeated.
- **Tradeoff.** Less engine, or pages that cannot loop or leave the origin.
- **Asked.** Opus and Astra (smaller): keep the checks; Opus cut the early build. Astra's attack on secrets: one origin. The second attack on the rules (Sol) added the headers. The third (Astra) found that a token in the path, as Telegram has it, replaced between two pages, looked like a move. Telegram, written again (Sol), could not page its poll, and wrote a loop of the caller.
- **Verdict: changed in this run** (B6). The checks are kept. A pager that may say "again" can come later without breaking.

### 31. Redirects

- **Now.** Concord's own client follows no redirect. A `3xx` is an answer, and an arm can take it.
- **The other design.** A controlled mode (Sol): `gh.with_redirects(same_origin().max_hops(3))`, each hop through the middlewares again, inside the deadline.
- **It wins.** A renamed GitHub repository works on every endpoint. GitHub asks clients to follow redirects.
- **It loses.** Concord owns redirect state and cycles.
- **Tradeoff.** Moved resources that just work, or one call that is one exchange.
- **Asked.** Sol, among its five.
- **Verdict: can come later without breaking.**

## What changed in this run because of the trial

| Change | Entry | Sheet |
|---|---|---|
| An optional input is marked: `count?: u32` | 2 | L1 |
| What an arm takes decides when `override` is needed | 4 | L3, 3f.4 |
| A part may hold `base` | 6 | E9 |
| What an arrow reads; no arrow under an arm that reads | 8 | L2, L4 |
| A short header entry with `_` is refused | 10 | L6 |
| A body made of entries: `body json { a; b; }`; a `None` writes nothing; a `Secret` may be an entry | 12 | E1, 3i.1, 3i.2 |
| `multipart` is shipped, with `finish()` and the same bytes for the same inputs; `Body::concat` | 13 | E3, 3i.10, 3i.11 |
| The statement `idle_timeout` (twelve statements now) | 14 | E2 |
| `#[doc(hidden)]` on a scope or an endpoint | 15 | L9 |
| `e.retry_after()`, the longer of two waits | 20 | B7, 3i.9 |
| A paged call is not awaited directly; `.pages().items()` | 21 | B1, B2 |
| `send_if_none_match(etag)` | 22 | B10 |
| A caller's middlewares first, the description's last | 25 | L8 |
| A smaller retry whose rules add up | 28 | B3, B4, B9; E4 |
| What the clock for silence watches | 29 | B5, E6 |
| Pages: built when asked for; one origin, also for `at_url`; "the same request" leaves out what a secret wrote | 30 | B6, 3f.2, 3f.7, 3h.2, 3i.6, 3i.7 |

The run's other changes, and the small answers behind each row, are in `experience/challenge/sheet.md` and D239 to D265.
