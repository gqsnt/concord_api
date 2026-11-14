# For each choice, the strongest other choice

*[Concord](../../README.md) · [docs](../README.md) · design · previous: [why Concord](why.md) · next: [scope and limits](scope.md)*

For someone who asks "why not the other way?" before adopting Concord: for each choice, the strongest other design, written both ways where it helps, what each side wins and loses, and why Concord went its way.

Each entry gives Concord's choice, the other design, what that design wins and loses, the tradeoff, and the reason for Concord's choice. Two marks: **a close call** means that the other design nearly won; **can come later without breaking** means that the other design is an addition, and a description that compiles today keeps its meaning. The short form of every tradeoff, with the door for who needs the other side, is in [scope and limits](scope.md#the-tradeoffs). The language itself: [the grammar](../reference/grammar.md) and [the rules](../reference/rules.md).

## The describing side

### An input is written twice

- **Concord.** An input is declared in the signature and placed in the block. An input that nothing places does not compile.
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
- **Why Concord's way.** The second mention is a boundary: the signature is what the caller sees, and the block is everything that goes out. In real APIs almost every input is named a second time where it is placed, and that mention is one word (`query { count; }`). A rest entry can come later without breaking.

### How an input says "may be left out"

- **Concord.** A mark on the name: `count?: u32`. The type is the value when it is given; in the block the name is a `&Option<u32>`. `milestone?: Option<u64>` has three states: not set, set to nothing, a number. A JSON body sends them as no key, `null` and the number; a query, a header or a form sends nothing for the first two. An alias changes nothing.
- **The other design.** Optional by the spelling of the type, as in `clap`'s derive:
  ```rust
  GET ids(puuid: Puuid, start: Option<u32>, count: Option<u32>) -> Vec<MatchId>   // the other design
  ```
- **It wins.** The signature is plain Rust. The written type is the variable's type.
- **It loses.** Behind an alias, an optional input becomes required, with no message. "Not sent" and "`null`" need `Option<Option<u64>>`, and `.milestone(None)` is then ambiguous (GitHub).
- **Tradeoff.** Rust's own spelling, or a mark that says "may be left out" whatever the type.
- **Why Concord's way.** Optionality is a fact about the call, not about the value's type. The mark holds behind an alias, and keeps "not set" apart from "set to nothing" with no nested `Option`. The cost accepted: `?` is not Rust.

### The form of an arm

- **Concord.** `201 { body, headers } => expr`. Naming `body` reads it. Fields are left out without `..`. Rust finds the type of `body` from the arm; where it cannot, the arm says it: `4xx { body } => { let odd: UploadError = body; Err(odd.into())? }`.
- **The other design.** A closure with typed parameters: `200 => |body: Repository, headers: &Headers| Some(Tagged::new(headers.get("etag"), body))`.
- **It wins.** The type of `body` is written, not inferred. The shape is Rust's.
- **It loses.** Longer, with the type written twice. And a closure that is not one: no capture, no `async`.
- **Tradeoff.** Written types, or a short form that is what it looks like.
- **Why Concord's way.** It reads as a `match`, and in real APIs the compiler finds the type of `body` in the common case. Several keys on one arm (`2xx | 301 { headers } => …`) can come later without breaking.

### What a node inherits

- **Concord.** `base`, `read` and `idle_timeout`: the nearest one. `path` adds. A key (an entry, an arm) set above is set again only with `override`, and taken away with `remove`. What an arm takes decides when `override` is needed: under a client's `4xx { body } => …`, an endpoint writes `override 404 => None`. An arm is still written, overridden and removed by its key: `remove 4xx` takes away the arm written `4xx`, and nothing else.
- **The other design.** The nearest one wins for everything, as in a config overlay. No `override`.
- **It wins.** One rule. No word at each replacement that is meant (`override "accept" = …`).
- **It loses.** An endpoint that writes its own success arm drops the client's envelope check (a refusal inside a `200`) with no message.
- **Tradeoff.** A word for each replacement, or an inherited arm lost with no message.
- **Why Concord's way.** An exception to an inherited rule is always written, never silent. If arms were compared by how they are written, a `200` under an inherited `2xx` would be another key, and `override 2xx` would leave an inherited `200` in place; comparing them by what they take closes both.

### How an API says no

- **Concord.** `problem E;`, once, on the client: one error type for a client. `status { }`, at any node, says what a status means there. A refusal that only one endpoint has is a value: `-> Result<Merged, MergeRefused>`.
- **The other design.** `problem` on an endpoint too, the client's as the default: `PUT merge(sha?: String) -> Merged { problem MergeRefused; }`.
- **It wins.** One channel for failures. Each endpoint's failures are exact in its own type.
- **It loses.** The error type differs between calls of one client: `?` needs conversions, and retry rules and wrappers meet many problem types.
- **Tradeoff.** Exact failures per endpoint, or `?` across every call of a client.
- **Why Concord's way.** `status` and `problem` do two jobs: what a status means for this endpoint, and how this API says no. A refusal that is exact to one endpoint already has a place, after its arrow.

### The form of a part

- **Concord.** A named group of inputs with the lines that place them, given to a node by `..name`. A part may hold `base`.
  ```rust
  part page(limit?: u32, starting_after?: String) {
      query { limit; starting_after; }
      paginate by_cursor(starting_after, List::next_cursor);
  }
  part regional(route: Region) { base "https://{route}.api.riotgames.com"; }
  scope matches(..regional) { path "lol/match/v5/matches"; … }
  ```
- **The other designs.** A type parameter: `part page<Id>(…, starting_after?: Id)`, named `..page<CustomerId>`. Or a part that holds scopes and endpoints (Kubernetes: the same pods under two parents).
- **They win.** Stripe's lists keep their typed cursors (`CustomerId`, `RefundId`, …) in one part. Kubernetes writes its pods once.
- **They lose.** The first type parameter of the language. A part with children is a second tree.
- **Tradeoff.** More sharing, or a part that stays a small group of inputs.
- **Why Concord's way.** A part stays small. It holds `base` because an input that goes in the host must come with the line that places it: without that, Riot's regional `base` would be written again in every scope that takes a route.
- **Can come later without breaking.**

### Where `middleware` stands

- **Concord.** On the client only. Made once, in `new`, from the client's required inputs (an optional one is refused). Nothing takes it away. State that a caller reads (quotas) comes in as a required input.
- **The other design.** On any node, still made once from the client's inputs, for the requests below: `scope private { middleware Sign::new(client.key.clone()); … }`.
- **It wins.** The tree says what is signed. No list of endpoint names (`info.endpoint()`) beside the description, which a rename breaks with no message. Slack, for example, leaves its upload out of a cooldown by a branch on the endpoint's name in its middleware.
- **It loses.** An order between a client's and a scope's middlewares. "`new` signs" becomes "signs what is under `private`".
- **Tradeoff.** The tree shows what is signed, or one rule for every request.
- **Why Concord's way.** One rule for every request: `new` makes the signer, and nothing below can switch it off. On a scope it can come later without breaking.
- **A close call.**

### The arrow

- **Concord.** `-> T` is what the caller gets. No arrow gives `()`, and the body is ignored. An endpoint with no arrow does not compile when an arm in reach would read a body. An `Option<T>` or `Result<T, E>` arrow reads a 2xx as `T` and wraps it; arms give the rest, `404 => None`.
  ```rust
  // the client: status { 2xx { body } => Reply::into_result(body)? }
  DELETE cancel(id: OrderId);          // does not compile: give it an arrow (`-> Ignored`)
  ```
- **The other design.** The arrow is always written, `-> ()` too.
- **It wins.** The rule for an endpoint with no arrow ([rule 12](../reference/rules.md)) is not silent. The user's view says what each endpoint gives.
- **It loses.** Noise on every `DELETE`. And it misses the real hole: under an envelope arm (see [read answers and errors, send bodies](../guide/responses-and-errors.md)), an endpoint with no arrow could never succeed.
- **Tradeoff.** A visible `()`, or Rust's habit.
- **Why Concord's way.** The compile error closes the hole where it is, and "no arrow" keeps Rust's habit. An `Option` arrow reads a 2xx as `T`, so a `200` with a `null` body is not a silent `None`.

### Templates and holes

- **Concord.** `{x}` is one encoded piece of a path; `{*x}` a whole segment that keeps its `/`; a host hole stays inside one label. A value stays in its place, or the request is refused.
- **The other design.** RFC 6570 URI Templates: `path "{+key}";` for `path "{*key}";`, and `{/x*}`, `{?a,b}`.
- **It wins.** A standard, the one GitHub sends at run time (`upload_url: ".../assets{?name,label}"`).
- **It loses.** `{+x}` lets `?`, `#` and `..` through: a value leaves its place. `{?a,b}` repeats `query { }`. And `{*key}` is axum 0.8's catch-all.
- **Tradeoff.** A standard that trusts the value, or a small form that refuses it.
- **Why Concord's way.** A value that leaves its place is a request that nobody described. The small form refuses it.

### The short entry `name;`

- **Concord.** In `query`, `name;` sends the input under its own name. In `headers`, a short entry whose name has `_` is refused ([message 41](../reference/messages.md)): write `"user-agent" = user_agent;`.
- **The other design.** The key takes the form of its place: in `headers`, `_` becomes `-` (`if_match;` sends `if-match`), as FastAPI's `Header()` does.
- **It wins.** An object store like S3 writes eight entries short (`content_type;`, `if_match;`, …).
- **It loses.** "The key is the name" gets an exception by place.
- **Tradeoff.** Shorter entries, or one rule with no exception.
- **Why Concord's way.** One rule with no exception: the key is the name. A header named `user_agent` would be dropped by nginx by default, so the short form is refused rather than sent; it is not converted, because a conversion is the exception.

### Keys in another case than the inputs

- **Concord.** A key in another case than its input is written in the long form. In a part, it is written once for every endpoint that names the part. The block stays the whole wire, as written.
  ```rust
  part selectors(label_selector?: String, field_selector?: String) {
      query { "labelSelector" = label_selector; "fieldSelector" = field_selector; }
  }
  ```
- **The other design.** A word that changes the case of every key, as `#[serde(rename_all = "camelCase")]` does for the fields of a struct, once on the client:
  ```rust
  keys camelCase;                                               // not Concord
  query { label_selector; field_selector; resource_version; }
  ```
- **It wins.** The case is said once for the API, and the short entry fits most keys again.
- **It loses.** The macro would learn a vocabulary: the names of the cases, and how each one cuts a name. The block is no longer the whole wire: the key that goes out is written nowhere. A key that is a Rust keyword (`continue`) is still written long.
- **Tradeoff.** The case said once, or every key written where it goes.
- **Why Concord's way.** The macro knows no vocabulary, and the block names every key. This is the one repetition that some real APIs still meet: in Kubernetes most entries that place an input are long, and the object store and Riot have some (`"versionId"`, `"uploadId"`; `"startTime"`, `"endTime"`). A statement that names a function of the author's, from a name to its key (`keys camel;`), can come later without breaking: the function is the author's, and the macro copies it like every right side.

### A body made of entries

- **Concord.** `body json { channel; text; }`. Entries are as in `query`, and `serde` writes them as one map, in order. `body json { … }` is `body json(entries)`, where `json` is a path to a function seen from where `api!` stands (an input of that name does not hide it): the macro still knows no format. A part may hold one too. A `None` writes nothing, in every block of entries: one level of `Option` is presence, and what is inside is written as the format writes it. So `milestone?: Option<u64>` sends no key, `null` or a number in JSON, and `"k" = Some(x);` with `x: Option<T>` is the way to write a `None` as `null`. A form has no `null`. A `Secret` may be an entry, written as its text: `body form { grant_type; client_id; client_secret; }`.
- **The other designs.** A body of several inputs names each input as a key in `json!`:
  ```rust
  body slack_json(json!({ "channel": channel, "text": text, "thread_ts": thread_ts }));   // the other design
  ```
  Or another rule for a body of entries: every value as `serde` writes it, a `None` as `null`, and an entry left out only when it names an optional input alone.
- **They win.** No new form: any JSON shape is written as it is. With the other rule, a value goes out exactly as `serde` writes it, and a `None` is `null` with no word of Concord's.
- **They lose.** With `json!`, each input is written a third time, an input that was not set goes out as `null`, and a part cannot place its inputs in a body. With the other rule, a `None` sends nothing in a query and `null` in a body, an `Option` that the author made from an input goes out as `null` when it is `None`, and a form cannot keep the promised `null`.
- **Tradeoff.** One form for every body, or the common body without a third mention; `None` as `serde` writes it, or one meaning of `None` in every block.
- **Why Concord's way.** In APIs where every method is a `POST` whose body is its inputs (Slack, Telegram), most of the repeated names are in bodies. Entries name each input once more, as `query` does, and a `None` means the same thing in every block. A `Secret` as an entry lets a webhook or a token request be written in one line.

### An upload in parts

- **Concord.** `multipart` is shipped, in `concord-extras`, behind the feature `multipart`: `body multipart().text("chat_id", id).file("photo", "a.jpg", "image/jpeg", photo).finish();`. `text` and `file` give the builder back; `finish()` gives a `Result` of a `Body` (a content type that is not a legal header value is its error: nothing panics). The boundary is not random when every part is in memory: the same inputs give the same bytes. With a part that is a stream, it is random, and the body goes out once. The language does not change. The core has `Body::concat(parts)`, a body made of bodies, whose one-shot streams are taken with the whole: a stream that was already sent is a Build error before anything goes out. A writer of the author's has the same pieces (`Body::as_bytes()`, `Body::into_stream()`).
- **The other designs.** Not shipped: it is one function that gives a `Body`, and the author writes it. Or shipped, with a random boundary for every body.
- **They win.** One piece less in `concord-extras`. With a random boundary, one rule for every body.
- **They lose.** Each author writes the boundary and the escaping by hand. With a random boundary, the same inputs give other bytes: a pager that gives back the same call is not seen to repeat.
- **Tradeoff.** Less that Concord ships, or one writer that every upload shares.
- **Why Concord's way.** Uploads are common (Telegram, Stripe), and the boundary and the escaping are easy to get wrong. Shipped, the writer also keeps "the same inputs, the same request", and takes a stream used twice as a Build error before a byte goes out.

### What an endpoint knows about time

- **Concord.** The statement `idle_timeout <expression>;`, on an endpoint, a scope, the client or a part (the nearest one), gives the clock for silence of the calls below, from the inputs:
  ```rust
  GET get_updates(offset?: i64, timeout?: u32) -> Vec<Update> {
      path "getUpdates";
      query { offset; timeout; }
      idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
  }
  ```
  The call's `with_idle_timeout` wins over it; it wins over the client's. Retry stays the caller's.
- **The other design.** Only the caller says it, on each call or on the client.
- **It wins.** One statement less. And no value that is a guess: for an upload, the silence depends on the line, not on the API.
- **It loses.** A long poll (Telegram) or a quiet stream (a Kubernetes watch) is a fact of the endpoint, yet every caller has to know it, or fail every poll at 30 seconds.
- **Tradeoff.** The fact written once, or one statement less.
- **Why Concord's way.** A fact of the API goes in the description. Slow and silent endpoints are common in real APIs, and the caller can still give another length.

### Everything generated is as public as the client

- **Concord.** Every generated item has the client's visibility. `#[doc(hidden)]` is accepted on a scope or an endpoint.
- **The other design.** A node takes Rust's visibility: `pub(crate) scope multipart(…) { … }`, and the author's `upload_large` beside it is the only way in.
- **It wins.** A safe flow can be the only way in (an upload left open is billed). The tree stops being the whole semver surface.
- **It loses.** "Hide the blocks, and what is left is what a user needs" stops being true. In real APIs, a wrapper of the author's sits beside a raw call that is still useful.
- **Tradeoff.** The author picks the way in, or the description is the whole public API.
- **Why Concord's way.** The description stays the whole public API, and `#[doc(hidden)]` is the cheap door for a call that should not be shown.
- **A close call.**

### One base, moved as one

- **Concord.** `with_base(…)` replaces every base. A second host is a `base` on the node that lives there.
- **The other design.** A base with a name, moved by its name: `base uploads "https://uploads.github.com";` and `gh.with_base_of(Uploads, upload_base)`.
- **It wins.** One description serves github.com and GitHub Enterprise (`host/api/v3`, `host/api/uploads`).
- **It loses.** Names for bases, more generated names, and inheritance that must carry a base's name.
- **Tradeoff.** Each host moved alone, or one setting for all.
- **Why Concord's way.** One setting covers the common case, and a named base is an addition.
- **Can come later without breaking.**

## The calling side

### One level of the tree is one call

- **Concord.** A scope is a method that gives a handle; an endpoint a method that gives a call: `gh.repo(&owner, &name).pull(4521).merge()`.
- **The other design.** Flat methods named by their path, as in the AWS SDKs: `gh.repo_pull_merge(&owner, &name, 4521)`.
- **It wins.** The author can reshape the tree without breaking callers.
- **It loses.** The handle a caller keeps. Inputs of several levels meet in one list.
- **Tradeoff.** Freedom to reshape, or calls that read like the URL.
- **Why Concord's way.** A call reads like the URL, and a caller can keep a handle (`let repo = gh.repo(&owner, &name);`) for many calls.

### Arguments by position; what an argument takes

- **Concord.** Required inputs are arguments, in order; an optional input is a method. An argument is its type, a reference, text for a `String` or a `Secret`, a slice for a `Vec`. Nothing else.
- **The other design.** Every input named, the required ones checked by type (as `bon` does): `by_riot_id().game_name("Faker").tag_line("KR1")`. Or a type that opts in to text.
- **It wins.** No swap compiles: with positions, `by_riot_id("KR1", "Faker")` compiles and answers `None`. Text reaches a newtype with no ceremony.
- **It loses.** Every one-id call grows. Text for a newtype removes what makes position safe.
- **Tradeoff.** Calls that cannot swap, or short calls whose types protect them.
- **Why Concord's way.** The two hold each other up: a newtype must be spelled at the call, so its type protects the position.

### A call is a value, and Concord's own names

- **Concord.** `.await` sends a call (except a paged one: see [what `.await` does on a paged call](#what-await-does-on-a-paged-call)); `send()` does the same. An option of the API keeps its name. Concord's settings start with `with_`, the one reserved prefix; its other names are exact (`send`, `request`, `pages`, `at_url`, …). The promise: no new name on a call, a handle or a client in a minor version, unless it starts with `with_`.
- **The other design.** The verb is required (`.send().await`, as reqwest and octocrab). One reserved method, `.with(settings)`, in place of a prefix.
- **It wins.** One spelling (`tokio::spawn` needs `send()` anyway). Discord's `with_counts` keeps its name.
- **It loses.** Seven characters on the most common line. `with` is itself a likely parameter.
- **Tradeoff.** One spelling everywhere, or the short one where most calls are.
- **Why Concord's way.** The short spelling goes where most calls are. Exact names reserve as little as possible: a wider prefix such as `send_` would take names from APIs (Telegram's `sendMessage` and many more could not keep their names).

### One error type for a client, four kinds

- **Concord.** `riot::Error` is opaque: `kind()` (Build, Transport, Refused, Mismatch), `is_status()`, `problem()`, accessors that give an `Option`. `e.retry_after()` gives the longer of two waits: the `Retry-After` of the answer, and the wait that a rule of `when` asked for the try that did not happen. Its `source()` never holds a value of the answer: Concord makes it from the place and the kind. A reader hands it no cause and no text of its own. The place is made of pieces whose names come from the program, never from the answer: `ReadError::not_the_type(body).at(Place::root().field("items").index(3))`. Free text goes in `.with_details(…)`, which only `e.details()` gives.
- **The other design.** A `#[non_exhaustive]` enum whose variants hold what each kind always has: `Err(riot::Error::Refused(r)) => r.headers().retry_after()`.
- **It wins.** What is always there is not an `Option`.
- **It loses.** Public fields: one that moves breaks callers.
- **Tradeoff.** Types that say what each kind holds, or a surface that can grow.
- **Why Concord's way.** Kinds can grow without breaking. `e.retry_after()` covers the common case of the other design, reading the wait of a refusal.

### What `.await` does on a paged call

- **Concord.** A call to an endpoint with `paginate` cannot be awaited directly:
  ```rust
  let first = repo.issues().list().send().await?;      // one page, on purpose
  let mut all = repo.issues().list().pages().items();  // every issue
  ```
- **The other design.** `.await` gives the first page, as on any call.
- **It wins.** "`.await` sends" has no exception. Adding `paginate` later breaks no build.
- **It loses.** `repo.issues().list().await?` gives 30 issues of 3,000, and nothing says so. A test with five records passes.
- **Tradeoff.** One rule for `.await`, or no partial answer that looks whole.
- **Why Concord's way.** A partial answer that looks whole is the worse failure. The accepted cost: adding `paginate` to an endpoint breaks its callers' builds.

### One endpoint, one answer type

- **Concord.** The type after the arrow is fixed. On a `GET` or `HEAD` call, `send_if_none_match(etag)` gives an `Answer<Option<T>>`, `None` on a `304`, with its headers: `repo.get().send_if_none_match(&etag).await?`.
- **The other design.** Typed adapters on a call: `repo.get().with_condition(if_none_match(tag))`, plain Rust that may change the answer of any endpoint.
- **It wins.** Any mode of the protocol, on any endpoint.
- **It loses.** An adapter that changes the answer type needs rules with arms and pages: a large contract.
- **Tradeoff.** Any mode anywhere, or one mode with a small surface.
- **Why Concord's way.** A conditional read is the mode that APIs ask for, and one method gives it with a small surface. General adapters are not part of Concord.

### Generated names follow the tree

- **Concord.** `riot::Account` is the handle of the scope `account`. Your type `Account` stays yours.
- **The other design.** A handle says what it is: `github::IssueHandle`.
- **It wins.** In rustdoc, `pub fn issue(&self, …) -> Issue` reads like a fetch. GitHub alone has five such pairs.
- **It loses.** A suffix on every handle a user names (`bucket: &s3::Bucket`). Rust's habit is the module path (`io::Error`).
- **Tradeoff.** Names that say what they are, or short names under a module.
- **Why Concord's way.** Rust's habit: a short name under its module. The generated text links the author's type by its path, so the two are not confused on the page.
- **A close call.**

### A test is a fake transport

- **Concord.** `with_transport(concord::fake(|request| …))` replaces only the network. The reader, the arms and the middlewares run for real.
- **The other design.** Typed answers per endpoint, with no wire (as `aws-smithy-mocks`).
- **It wins.** An application tests its own logic without JSON written by hand.
- **It loses.** The description goes untested: a wrong `path` passes. Retry and the clocks have nothing to act on.
- **Tradeoff.** Tests of the application, or tests of the client.
- **Why Concord's way.** A test of a client should test the description too. A `fake` that gets the `TryInfo` can come later without breaking.

## The behaviour

### A transport and middlewares

- **Concord.** `with_transport` replaces what really sends; middlewares are steps above it. A caller's middlewares come first, then the description's in the order written, then the transport. The author writes the signer last: nothing changes a request after it is signed.
- **The other design.** One `tower` stack: `with_layer(…)` for everything.
- **It wins.** One idea, `tower`'s layers with no adapter, and the caller chooses where a layer sits.
- **It loses.** `poll_ready`, boxed errors, services that must be cloned. "Replace what sends" and "add a step" become one operation.
- **Tradeoff.** One powerful idea, or two simple ones.
- **Why Concord's way.** Two simple ideas. The order is fixed so that a caller's middleware, placed after the description's, could not change a signed request.

### Retry is off until set

- **Concord.** Nothing is sent again until the caller writes `with_retry(…)`.
- **The other design.** Retry on by default, with the author's own policy as the default.
- **It wins.** What the author knows (GitHub's secondary limit) reaches every caller.
- **It loses.** More traffic when an API is down.
- **Tradeoff.** Good behaviour by default, or no traffic nobody asked for.
- **Why Concord's way.** No traffic that nobody asked for. Turning it on is one line, and the author can ship a policy for the caller to name.
- **A close call.**

### Which requests are sent again

- **Concord.** With retry on: a `GET`, `HEAD`, `OPTIONS`, `PUT` or `DELETE` (unless `idempotent false;`); a request whose endpoint says `idempotent;`; and a request whose status the policy lists with `not_applied_on` (see [how much retry policy](#how-much-retry-policy-and-what-is-logged)).
- **The other design.** Only reads by default; a `PUT` or a `DELETE` only with `idempotent;` (as Go's `net/http`).
- **It wins.** A `DELETE` that worked but whose answer was lost is not sent again, so it never answers `404` for something that worked.
- **It loses.** Every ordinary `PUT` and `DELETE` needs a mark. It leaves HTTP's rule, the one a reader knows.
- **Tradeoff.** Safer failures, or more useful ones.
- **Why Concord's way.** HTTP's own rule, the one a reader knows. An API that breaks it says so with `idempotent false;`.
- **A close call.**

### How much retry policy, and what is logged

- **Concord.** `Retry::standard()`, `.tries(n)`, `.when(f)`, `.unless(f)`, `.not_applied_on(status)`. Fixed waits: random up to 1 second, then 2, then 4, at most 60, never less than a `Retry-After`. A `Retry-After` is a number of seconds or a date, which Concord reads itself. A wait over 60 seconds, from the header or from a rule, means no other try; `e.retry_after()` gives the longer of the two. Rules add up: `github::retry().when(mine)` keeps GitHub's rule. A request may be repeated by its method, its mark, or a status that the policy lists with `not_applied_on`. After `blocked`, the blocked error. One log event, `retry`.
- **The other design.** Also `first_wait` and `max_wait`; a second `when` replaces the first; after `blocked`, the error of the try before; a log event for each try and each answer.
- **It wins.** Tuning for tests and long waits; a free log of the traffic.
- **It loses.** Knobs with no first-day user. A rule that erases the author's with no message. A result after `blocked` that nobody can guess. A middleware already sees every try.
- **Tradeoff.** More knobs, or fewer rules to learn.
- **Why Concord's way.** Fewer rules to learn, and rules that add up. `not_applied_on` stays because real APIs need it: a `429` on a `POST` that is not idempotent (Slack, Telegram, Riot's tournament `POST`s) was turned away before anything was done, and is safe to send again. `idempotent;` would also send it again after a lost connection, and post a message twice.

### Two clocks, and what is silence

- **Concord.** Two clocks: `timeout` bounds the whole call, and `idle_timeout` one silence of the server, which it watches in the transport until the head of the answer and in a body while it is read; a middleware's time and a reader's own waits are never silence. The whole rule: [rule 19](../reference/rules.md#behaviour).
- **The other designs.** A clock per phase, as OkHttp: connect, read, write, the call. Or a clock for silence that starts at the first middleware, where a middleware that waits on purpose (a quota) calls `info.sign_of_life()`.
- **They win.** With a write clock, an upload from memory is not silence (without it, a caller adds `with_idle_timeout` for that alone). With `sign_of_life()`, a middleware that hangs before it sends is still cut at 30 seconds.
- **They lose.** Four knobs; and a write clock must see the body leave, which `reqwest` does not show. `sign_of_life()` is a trap: forget it, and a quota's wait ends at 30 seconds and is tried again through the quota.
- **Tradeoff.** Exact clocks, or two that fit in the head.
- **Why Concord's way.** Two clocks fit in the head, and the clock for silence watches only what can be silent: the server. It does not run while a middleware holds an answer.

### Pages: when built, and where they go

- **Concord.** A page's request is built when that page is asked for. Every page goes to the origin of the first; another origin is a Build error. `at_url` gives a path and a query, never an origin: a URL at another origin than the one that the call builds is a Build error, in pages and outside them. The same request twice is an error: the same method, URL (without its fragment), body bytes, and each value of each header, in order. What a secret wrote in the URL or in a header is left out (a piece of a path, a pair of the query, a value of a header), and so is a header value that `edit_request` marks `sensitive`; a body is compared as its bytes, and a secret in a host that was replaced is another origin. Both checks look at the request as built, before any middleware; a middleware that sends it elsewhere is its author's word. After an error the pages are over.
- **The other designs.** The next request built early, as soon as an answer comes. No check for a repeated request. Or a check that compares no headers.
- **They win.** One comparison less, or a smaller one. Without the check, a poll that asks the same request again (Telegram's `getUpdates` after an empty batch) could be pages.
- **They lose.** Built early, a page misses a `Secret` that was replaced. Without the check, a repeating cursor loops for ever. Without the headers, an API that pages by a header (Cosmos DB) is stopped as if it repeated.
- **Tradeoff.** Less engine, or pages that cannot loop or leave the origin.
- **Why Concord's way.** Pages that cannot loop and cannot carry a key to another host. What a secret wrote is left out of the comparison, so a token in the path (as Telegram has it), replaced between two pages, does not look like a repeat. A pager that may say "again" can come later without breaking.

### Redirects

- **Concord.** Concord's own client follows no redirect. A `3xx` is an answer, and an arm can take it.
- **The other design.** A controlled mode: `gh.with_redirects(same_origin().max_hops(3))`, each hop through the middlewares again, inside the deadline.
- **It wins.** A renamed GitHub repository works on every endpoint. GitHub asks clients to follow redirects.
- **It loses.** Concord owns redirect state and cycles.
- **Tradeoff.** Moved resources that just work, or one call that is one exchange.
- **Why Concord's way.** One call is one exchange, and a credential is never carried to another place by a redirect that nobody described.
- **Can come later without breaking.**
