# Alternatives: for each choice, the strongest other choice

*[Concord](../../README.md) · [docs](../README.md) · design · previous: [why Concord](why.md) · next: [scope and limits](scope.md)*

This page is for a developer who asks "why not the other way?" before adopting Concord. For each choice, it gives the strongest other design, written both ways where that helps, what each side gains and loses, and why Concord chose as it did.

Every entry has the same form: Concord's choice, the other design, what that design wins and what it loses, the tradeoff, and the reason for Concord's choice. Two marks appear: **a close call** means that the other design nearly won; **can come later without breaking** means that the other design is an addition, so a description that compiles today keeps its meaning. The short form of every tradeoff, with the escape hatch for when you need the other side, is in [scope and limits](scope.md#the-tradeoffs). The language itself is described in [the grammar](../reference/grammar.md) and [the rules](../reference/rules.md).

## The describing side

### An input is written twice

- **Concord.** An input is declared in the signature and placed in the block. An input that nothing places does not compile.
  ```rust
  GET ids(puuid: Puuid, start?: u32, count?: u32) -> Vec<MatchId> {
      path "by-puuid/{puuid}/ids";
      query { start; count; }
  }
  ```
- **The other design.** The placement goes on the parameter, as with Retrofit's `@Query`. Or a rest entry, `query { .. }`, places under its own name every input that nothing else places.
- **It wins.** An input is written once, so a new optional filter is one edit, not two.
- **It loses.** On the parameter, the signature carries the wire. With `..`, the block no longer names every key, and renaming an input silently renames its wire key.
- **Tradeoff.** Fewer words, or a block that shows the whole wire.
- **Why Concord's way.** The second mention marks a boundary: the signature is what the caller sees, and the block is everything that goes out. In real APIs almost every input is named a second time where it is placed, and that mention is one word (`query { count; }`). A rest entry can come later without breaking.

### How an input says "may be left out"

- **Concord.** A mark on the name: `count?: u32`. The written type is the type of the value when one is given; in the block, the name is a `&Option<u32>`. `milestone?: Option<u64>` has three states: not set, set to nothing, and set to a number. A JSON body sends them as no key, `null` and the number; a query, a header or a form sends nothing for the first two. An alias changes nothing.
- **The other design.** An input is optional because of how its type is spelled, as in `clap`'s derive:
  ```rust
  GET ids(puuid: Puuid, start: Option<u32>, count: Option<u32>) -> Vec<MatchId>   // the other design
  ```
- **It wins.** The signature is plain Rust, and the written type is the variable's type.
- **It loses.** Behind an alias, an optional input silently becomes required. Telling "not sent" from "`null`" needs `Option<Option<u64>>`, and `.milestone(None)` is then ambiguous. GitHub has such a field: the `milestone` of an issue, which `null` clears ([GitHub](../examples/github.md)).
- **Tradeoff.** Rust's own spelling, or a mark that means "may be left out" whatever the type.
- **Why Concord's way.** Optionality is a fact about the call, not about the type of the value. The mark still holds behind an alias, and it keeps "not set" apart from "set to nothing" without a nested `Option`. The accepted cost: `?` is not Rust.

### The form of an arm

- **Concord.** `201 { body, headers } => expr`. Naming `body` reads the body. Fields can be left out without `..`. Rust infers the type of `body` from the arm; where it cannot, the arm states it: `4xx { body } => { let odd: UploadError = body; Err(odd.into())? }`.
- **The other design.** A closure with typed parameters: `200 => |body: Repository, headers: &Headers| Some(Tagged::new(headers.get("etag"), body))`.
- **It wins.** The type of `body` is written, not inferred, and the shape is Rust's own.
- **It loses.** It is longer, and the type is written twice. And it looks like a closure without being one: it cannot capture, and cannot be `async`.
- **Tradeoff.** Written types, or a short form that is what it looks like.
- **Why Concord's way.** It reads like a `match`, and in real APIs the compiler infers the type of `body` in the common case. Several keys on one arm (`2xx | 301 { headers } => …`) can come later without breaking.

### What a node inherits

- **Concord.** For `base`, `read` and `idle_timeout`, the nearest one applies. `path` adds to the paths above. A key (an entry, an arm) set above can be set again only with `override`, and taken away with `remove`. The statuses that an arm matches decide when `override` is needed: under a client's `4xx { body } => …`, an endpoint writes `override 404 => None`. An arm is still written, overridden and removed by its key: `remove 4xx` takes away the arm written `4xx`, and nothing else.
- **The other design.** The nearest one wins for everything, as in a config overlay, and there is no `override`.
- **It wins.** One rule, and no extra word at each intended replacement (`override "accept" = …`).
- **It loses.** An endpoint that writes its own success arm silently drops the client's envelope check (an error response inside a `200`).
- **Tradeoff.** A word for each replacement, or an inherited arm that is silently lost.
- **Why Concord's way.** An exception to an inherited rule is always written explicitly, never implied. If arms were compared by how they are written, a `200` under an inherited `2xx` would be a different key, and `override 2xx` would leave an inherited `200` in place; comparing arms by the statuses they match closes both gaps.

### How an API rejects a request

- **Concord.** `problem E;`, written once on the client, gives the client one error type. `status { }`, at any node, says what a status means there. An error response that only one endpoint has is a value: `-> Result<Merged, MergeRefused>`.
- **The other design.** `problem` on an endpoint too, with the client's as the default: `PUT merge(sha?: String) -> Merged { problem MergeRefused; }`.
- **It wins.** All failures go through one channel, and each endpoint's failures are exact in its own type.
- **It loses.** The error type differs between calls of one client, so `?` needs conversions, and retry rules and wrappers must deal with many problem types.
- **Tradeoff.** Exact failures for each endpoint, or `?` across every call of a client.
- **Why Concord's way.** `status` and `problem` already do two different jobs: what a status means for this endpoint, and how this API rejects a request. An error that is specific to one endpoint already has a place: after its arrow.

### The form of a part

- **Concord.** A named group of inputs with the lines that place them, given to a node with `..name`. A part may hold `base`.
  ```rust
  part page(limit?: u32, starting_after?: String) {
      query { limit; starting_after; }
      paginate by_cursor(starting_after, List::next_cursor);
  }
  part regional(route: Region) { base "https://{route}.api.riotgames.com"; }
  scope matches(..regional) { path "lol/match/v5/matches"; … }
  ```
- **The other designs.** A type parameter: `part page<Id>(…, starting_after?: Id)`, used as `..page<CustomerId>`. Or a part that holds scopes and endpoints (for Kubernetes, which has the same pods under two parents).
- **They win.** Stripe's lists keep their typed cursors (`CustomerId`, `RefundId`, …) in one part, and Kubernetes writes its pods once.
- **They lose.** The first adds the language's first type parameter. The second, a part with children, is a second tree.
- **Tradeoff.** More sharing, or a part that stays a small group of inputs.
- **Why Concord's way.** A part stays small. It holds `base` because an input that goes in the host must come with the line that places it: without that, Riot's regional `base` would be repeated in every scope that takes a route.
- **Can come later without breaking.**

### Where `middleware` can be written

- **Concord.** On the client only. It is made once, in `new`, from the client's required inputs (an optional one is rejected), and nothing takes it away. State that a caller reads (such as quotas) comes in as a required input.
- **The other design.** On any node, still made once from the client's inputs, for the requests below it: `scope private { middleware Sign::new(client.key.clone()); … }`.
- **It wins.** The tree shows what is signed. There is no list of endpoint names (`info.endpoint()`) kept beside the description, which a rename would silently break. Slack, for example, exempts its upload from a cooldown by branching on the endpoint's name in its middleware.
- **It loses.** It needs an order between a client's middleware and a scope's. And "`new` signs" becomes "it signs what is under `private`".
- **Tradeoff.** A tree that shows what is signed, or one rule for every request.
- **Why Concord's way.** One rule for every request: `new` makes the signer, and nothing below can switch it off. Middleware on a scope can come later without breaking.
- **A close call.**

### The arrow

- **Concord.** `-> T` is what the caller gets. With no arrow, the call returns `()`, and the body is ignored. An endpoint with no arrow does not compile when an arm that applies to it would read a body. An `Option<T>` or `Result<T, E>` arrow reads a 2xx as `T` and wraps it; arms produce the rest, as in `404 => None`.
  ```rust
  // the client: status { 2xx { body } => Reply::into_result(body)? }
  DELETE cancel(id: OrderId);          // does not compile: give it an arrow (`-> Ignored`)
  ```
- **The other design.** The arrow is always written, `-> ()` included.
- **It wins.** The case that [rule 12](../reference/rules.md) covers, an endpoint with no arrow, becomes explicit, and a caller sees what each endpoint returns.
- **It loses.** Noise on every `DELETE`. And it misses the real gap: under an envelope arm (see [Read responses and errors, send bodies](../guide/responses-and-errors.md)), an endpoint with no arrow could never succeed.
- **Tradeoff.** A visible `()`, or Rust's habit of leaving it out.
- **Why Concord's way.** The compile error closes the gap where it is, and leaving the arrow out keeps Rust's habit. An `Option` arrow reads a 2xx as `T`, so a `200` with a `null` body does not silently become `None`.

### Templates and placeholders

- **Concord.** `{x}` is a value encoded to stay within one segment of a path; `{*x}` is a whole segment that keeps its `/`; a placeholder in the host stays inside one label. A value stays where it is placed, or the request is rejected.
- **The other design.** RFC 6570 URI Templates: `path "{+key}";` in place of `path "{*key}";`, as well as `{/x*}` and `{?a,b}`.
- **It wins.** It is a standard, and the one that GitHub sends at run time (`upload_url: ".../assets{?name,label}"`).
- **It loses.** `{+x}` lets `?`, `#` and `..` through, so a value can escape its placeholder. `{?a,b}` duplicates `query { }`. And Concord's `{*key}` is the same as axum 0.8's catch-all.
- **Tradeoff.** A standard that trusts the value, or a small form that rejects it.
- **Why Concord's way.** A value that escapes its placeholder makes a request that nobody described. The small form rejects it.

### The short entry `name;`

- **Concord.** In `query`, `name;` sends the input under its own name. In `headers`, a short entry whose name contains `_` is rejected ([message 41](../reference/messages.md)): write `"user-agent" = user_agent;`.
- **The other design.** The key takes the form that its place expects: in `headers`, `_` becomes `-` (`if_match;` sends `if-match`), as FastAPI's `Header()` does.
- **It wins.** An object store like S3 can write eight entries in the short form (`content_type;`, `if_match;`, …).
- **It loses.** "The key is the name" gets an exception that depends on the place.
- **Tradeoff.** Shorter entries, or one rule with no exception.
- **Why Concord's way.** One rule with no exception: the key is the name. By default nginx drops a header named `user_agent`, so the short form is rejected rather than sent; it is not converted, because a conversion would be the exception.

### Keys in a different case from their inputs

- **Concord.** A key in a different case from its input is written in the long form. In a part, it is written once for every endpoint that names the part. The block stays the whole wire, as written.
  ```rust
  part selectors(label_selector?: String, field_selector?: String) {
      query { "labelSelector" = label_selector; "fieldSelector" = field_selector; }
  }
  ```
- **The other design.** A word that changes the case of every key, written once on the client, as `#[serde(rename_all = "camelCase")]` does for the fields of a struct:
  ```rust
  keys camelCase;                                               // not Concord
  query { label_selector; field_selector; resource_version; }
  ```
- **It wins.** The case is stated once for the whole API, and the short entry fits most keys again.
- **It loses.** The macro would have to learn a vocabulary: the names of the cases, and how each one splits a name into words. The block is no longer the whole wire, because the key that goes out is written nowhere. And a key that is a Rust keyword (`continue`) still needs the long form.
- **Tradeoff.** The case stated once, or every key written where it goes.
- **Why Concord's way.** The macro knows no vocabulary, and the block names every key. This is the one repetition that some real APIs still have: in Kubernetes most entries that place an input are long, and the object store and Riot have some (`"versionId"`, `"uploadId"`; `"startTime"`, `"endTime"`). A statement that names one of the author's functions, mapping a name to its key (`keys camel;`), can come later without breaking: the function is the author's, and the macro copies it like every right-hand side.

### A body made of entries

- **Concord.** `body json { channel; text; }`. Entries work as in `query`, and `serde` serializes them as one map, in order. `body json { … }` means `body json(entries)`, where `json` is a path to a function, resolved from where `api!` is invoked (an input of that name does not hide it), so the macro still knows no format. A part may hold such a body too. In every block of entries, a `None` writes nothing: one level of `Option` means presence, and what is inside it is written as the format writes it. So `milestone?: Option<u64>` sends no key, `null` or a number in JSON, and `"k" = Some(x);` with `x: Option<T>` is how you send a `None` as `null`. A form has no `null`. A `Secret` may be an entry, and is written as its text: `body form { grant_type; client_id; client_secret; }`.
- **The other designs.** A body made of several inputs names each input as a key in `json!`:
  ```rust
  body slack_json(json!({ "channel": channel, "text": text, "thread_ts": thread_ts }));   // the other design
  ```
  Or a different rule for a body of entries: every value as `serde` writes it, a `None` as `null`, and an entry left out only when it names an optional input on its own.
- **They win.** No new form: any JSON shape is written as it is. With the other rule, a value goes out exactly as `serde` writes it, and a `None` is `null` without any rule of Concord's.
- **They lose.** With `json!`, each input is written a third time, an input that was not set goes out as `null`, and a part cannot place its inputs in a body. With the other rule, a `None` sends nothing in a query but `null` in a body; an `Option` that the author built from an input goes out as `null` when it is `None`; and a form cannot keep the promised `null`.
- **Tradeoff.** One form for every body, or the common body without a third mention; `None` as `serde` writes it, or one meaning of `None` in every block.
- **Why Concord's way.** In APIs where every method is a `POST` whose body is its inputs (Slack, Telegram), most of the repeated names are in bodies. Entries name each input only once more, as `query` does, and a `None` means the same thing in every block. Because a `Secret` can be an entry, a webhook or a token request fits on one line.

### A multipart upload

- **Concord.** `multipart` ships in `concord-extras`, behind the feature `multipart`: `body multipart().text("chat_id", id).file("photo", "a.jpg", "image/jpeg", photo).finish();`. `text` and `file` return the builder; `finish()` returns a `Result` of a `Body` (its error is a content type that is not a legal header value: nothing panics). When every part is in memory, the boundary is not random: the same inputs produce the same bytes. When a part is a stream, the boundary is random, and the body is sent once. The language does not change. The core has `Body::concat(parts)`, a body made of bodies, whose one-shot streams are taken together with the whole: a stream that was already sent is a Build error before anything goes out. A custom writer has the same building blocks (`Body::as_bytes()`, `Body::into_stream()`).
- **The other designs.** Not shipped: it is one function that returns a `Body`, and the author writes it. Or shipped, with a random boundary for every body.
- **They win.** One piece less in `concord-extras`. With a random boundary, one rule for every body.
- **They lose.** Each author writes the boundary and the escaping by hand. With a random boundary, the same inputs produce different bytes, so a pager that returns the same call is not detected as a repeat.
- **Tradeoff.** Less that Concord ships, or one writer that every upload shares.
- **Why Concord's way.** Uploads are common (Telegram, Stripe), and the boundary and the escaping are easy to get wrong. A shipped writer also keeps the rule "the same inputs, the same request", and turns a stream used twice into a Build error before a byte goes out.

### What an endpoint knows about time

- **Concord.** The statement `idle_timeout <expression>;`, on an endpoint, a scope, the client or a part (the nearest one applies), sets the idle timeout for the calls below it, computed from the inputs:
  ```rust
  GET get_updates(offset?: i64, timeout?: u32) -> Vec<Update> {
      path "getUpdates";
      query { offset; timeout; }
      idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
  }
  ```
  The call's `with_idle_timeout` takes precedence over it, and it takes precedence over the client's. Retry stays the caller's choice.
- **The other design.** Only the caller sets it, on each call or on the client.
- **It wins.** One statement less. And no value that is a guess: for an upload, how long the connection stays silent depends on the network, not on the API.
- **It loses.** A long poll (Telegram) or a quiet stream (a Kubernetes watch) is a fact about the endpoint, yet every caller has to know it, or see every poll fail at 30 seconds.
- **Tradeoff.** The fact written once, or one statement less.
- **Why Concord's way.** A fact about the API belongs in the description. Slow and silent endpoints are common in real APIs, and the caller can still set a different length.

### Everything generated is as public as the client

- **Concord.** Every generated item has the client's visibility. `#[doc(hidden)]` is accepted on a scope or an endpoint.
- **The other design.** A node takes a Rust visibility, as in `pub(crate) scope multipart(…) { … }`, with the author's own `upload_large` beside it as the only way in.
- **It wins.** A safe flow can be the only way in (an upload left open is billed). And the tree is no longer the whole semver surface.
- **It loses.** "Hide the blocks, and what is left is what a user needs" stops being true. In real APIs, the author's wrapper sits beside a raw call that is still useful.
- **Tradeoff.** The author chooses the way in, or the description is the whole public API.
- **Why Concord's way.** The description stays the whole public API, and `#[doc(hidden)]` is the cheap escape hatch for a call that should not be shown.
- **A close call.**

### One base, moved as one

- **Concord.** `with_base(…)` replaces every base. A second host is a `base` on the node that lives there.
- **The other design.** A named base, moved by its name: `base uploads "https://uploads.github.com";` and `gh.with_base_of(Uploads, upload_base)`.
- **It wins.** One description serves both github.com and GitHub Enterprise (`host/api/v3`, `host/api/uploads`).
- **It loses.** It needs names for bases, more generated names, and inheritance that carries a base's name.
- **Tradeoff.** Each host moved on its own, or one setting for all.
- **Why Concord's way.** One setting covers the common case, and a named base can be added later.
- **Can come later without breaking.**

## The calling side

### One level of the tree is one call

- **Concord.** A scope is a method that returns a handle, and an endpoint is a method that returns a call: `gh.repo(&owner, &name).pull(4521).merge()`.
- **The other design.** Flat methods named after their path, as in the AWS SDKs: `gh.repo_pull_merge(&owner, &name, 4521)`.
- **It wins.** The author can reshape the tree without breaking callers.
- **It loses.** There is no handle for a caller to keep, and the inputs of several levels end up in one list.
- **Tradeoff.** Freedom to reshape, or calls that read like the URL.
- **Why Concord's way.** A call reads like the URL, and a caller can keep a handle (`let repo = gh.repo(&owner, &name);`) for many calls.

### Arguments by position, and what an argument accepts

- **Concord.** Required inputs are positional arguments, in order; an optional input is a method. An argument may be the input's type, a reference to it, text for a `String` or a `Secret`, or a slice for a `Vec`. Nothing else.
- **The other design.** Every input is named, and the required ones are checked by type (as `bon` does): `by_riot_id().game_name("Faker").tag_line("KR1")`. Or a type that opts in to accepting text.
- **It wins.** No swapped call compiles: with positions, `by_riot_id("KR1", "Faker")` compiles and returns `None`. And text reaches a newtype with no ceremony.
- **It loses.** Every call that takes one id grows longer. And accepting text for a newtype removes what makes positions safe.
- **Tradeoff.** Calls whose arguments cannot be swapped, or short calls protected by their types.
- **Why Concord's way.** The two choices support each other: a newtype must be spelled out at the call, so its type protects the position.

### A call is a value, and Concord's own names

- **Concord.** `.await` sends a call (except a paged one: see [what `.await` does on a paged call](#what-await-does-on-a-paged-call)); `send()` does the same. An option of the API keeps its name. Concord's settings start with `with_`, the one reserved prefix; its other names are exact (`send`, `request`, `pages`, `at_url`, …). The promise: no new name on a call, a handle or a client in a minor version, unless it starts with `with_`.
- **The other design.** The verb is required (`.send().await`, as in reqwest and octocrab). And one reserved method, `.with(settings)`, in place of a prefix.
- **It wins.** One spelling (`tokio::spawn` needs `send()` anyway). And Discord's `with_counts` keeps its name.
- **It loses.** Seven more characters on the most common line. And `with` is itself a likely parameter name.
- **Tradeoff.** One spelling everywhere, or the short one where most calls are.
- **Why Concord's way.** The short spelling goes where most calls are. Exact names reserve as little as possible: a wider prefix such as `send_` would take names away from APIs (Telegram's `sendMessage`, and many more, could not keep their names).

### One error type for a client, four kinds

- **Concord.** `riot::Error` is opaque: `kind()` (Build, Transport, Refused, Mismatch), `is_status()`, `problem()`, and accessors that return an `Option`. `e.retry_after()` returns the longer of two waits: the response's `Retry-After`, and the wait that a `when` rule asked for before the retry that did not happen. Its `source()` never holds a value from the response: Concord builds it from the location and the kind. A reader passes it no cause and no text of its own. The place is built from components whose names come from the program, never from the response: `ReadError::not_the_type(body).at(Place::root().field("items").index(3))`. Free text goes in `.with_details(…)`, and only `e.details()` returns it.
- **The other design.** A `#[non_exhaustive]` enum whose variants hold what each kind always has: `Err(riot::Error::Refused(r)) => r.headers().retry_after()`.
- **It wins.** What is always there is not an `Option`.
- **It loses.** The fields are public, so moving one breaks callers.
- **Tradeoff.** Types that say what each kind holds, or a surface that can grow.
- **Why Concord's way.** Kinds can grow without breaking. `e.retry_after()` covers the common case of the other design: reading the wait from an error response.

### What `.await` does on a paged call

- **Concord.** A call to an endpoint with `paginate` cannot be awaited directly:
  ```rust
  let first = repo.issues().list().send().await?;      // one page, on purpose
  let mut all = repo.issues().list().pages().items();  // every issue
  ```
- **The other design.** `.await` returns the first page, as on any call.
- **It wins.** "`.await` sends" has no exception, and adding `paginate` later breaks no build.
- **It loses.** `repo.issues().list().await?` returns 30 issues out of 3,000, and nothing says so. A test with five records passes.
- **Tradeoff.** One rule for `.await`, or no partial result that looks complete.
- **Why Concord's way.** A partial result that looks complete is the worse failure. The accepted cost: adding `paginate` to an endpoint breaks its callers' builds.

### One endpoint, one response type

- **Concord.** The type after the arrow is fixed. On a `GET` or `HEAD` call, `send_if_none_match(etag)` returns an `Answer<Option<T>>`, `None` on a `304`, with its headers: `repo.get().send_if_none_match(&etag).await?`.
- **The other design.** Typed adapters on a call, such as `repo.get().with_condition(if_none_match(tag))`: plain Rust that may change the response type of any endpoint.
- **It wins.** Any mode of the protocol, on any endpoint.
- **It loses.** An adapter that changes the response type needs rules for how it combines with arms and pages: a large contract.
- **Tradeoff.** Any mode anywhere, or one mode with a small surface.
- **Why Concord's way.** A conditional read is the mode that APIs ask for, and one method provides it with a small surface. General adapters are not part of Concord.

### Generated names follow the tree

- **Concord.** `riot::Account` is the handle of the scope `account`. Your own type `Account` stays yours.
- **The other design.** A handle's name says what it is: `github::IssueHandle`.
- **It wins.** In rustdoc, `pub fn issue(&self, …) -> Issue` reads like a fetch. GitHub alone has five such pairs.
- **It loses.** A suffix on every handle that a caller names (`bucket: &s3::Bucket`). And Rust's habit is the module path (`io::Error`).
- **Tradeoff.** Names that say what they are, or short names under a module.
- **Why Concord's way.** It follows Rust's habit: a short name under its module. The generated documentation links the author's type by its path, so the two are not confused on the page.
- **A close call.**

### A test is a fake transport

- **Concord.** `with_transport(concord::fake(|request| …))` replaces only the network. The reader, the arms and the middleware run for real.
- **The other design.** Typed responses for each endpoint, with no wire (as in `aws-smithy-mocks`).
- **It wins.** An application tests its own logic without writing JSON by hand.
- **It loses.** The description goes untested: a wrong `path` passes. And retry and the timeouts have nothing to act on.
- **Tradeoff.** Tests of the application, or tests of the client.
- **Why Concord's way.** A test of a client should test the description too. A `fake` that receives the `TryInfo` can come later without breaking.

## The behaviour

### A transport and middleware

- **Concord.** `with_transport` replaces the transport, which actually sends the request; middleware layers are steps above it. A caller's middleware comes first, then the description's in the order written, then the transport. The author writes the signer last, so nothing changes a request after it is signed.
- **The other design.** One `tower` stack: `with_layer(…)` for everything.
- **It wins.** One idea, `tower`'s layers with no adapter, and the caller chooses where each layer goes.
- **It loses.** `poll_ready`, boxed errors, and services that must be cloned. And "replace what sends" and "add a step" become one operation.
- **Tradeoff.** One powerful idea, or two simple ones.
- **Why Concord's way.** Two simple ideas. The order is fixed so that a caller's middleware cannot be placed after the description's and change a signed request.

### Retries are off until you turn them on

- **Concord.** Nothing is retried until the caller writes `with_retry(…)`.
- **The other design.** Retry is on by default, with the author's own policy as the default.
- **It wins.** What the author knows (GitHub's secondary rate limit) reaches every caller.
- **It loses.** More traffic when an API is down.
- **Tradeoff.** Good behaviour by default, or no traffic that nobody asked for.
- **Why Concord's way.** No traffic that nobody asked for. Turning retries on takes one line, and the author can ship a policy for the caller to name.
- **A close call.**

### Which requests are retried

- **Concord.** With retry on, Concord retries a `GET`, `HEAD`, `OPTIONS`, `PUT` or `DELETE` (unless `idempotent false;`); a request whose endpoint says `idempotent;`; and a request whose status the policy lists with `not_applied_on` (see [how much retry policy](#how-much-retry-policy-and-what-is-logged)).
- **The other design.** Only reads by default; a `PUT` or a `DELETE` only with `idempotent;` (as in Go's `net/http`).
- **It wins.** A `DELETE` that succeeded but whose response was lost is not retried, so it never returns `404` for something that worked.
- **It loses.** Every ordinary `PUT` and `DELETE` needs a mark. And it departs from HTTP's rule, the one readers know.
- **Tradeoff.** Safer failures, or more useful ones.
- **Why Concord's way.** It follows HTTP's own rule, the one readers know. An API that breaks it says so with `idempotent false;`.
- **A close call.**

### How much retry policy, and what is logged

- **Concord.** `Retry::standard()`, `.tries(n)`, `.when(f)`, `.unless(f)`, `.not_applied_on(status)`. The waits are fixed: random up to 1 second, then 2, then 4, at most 60, and never less than a `Retry-After`. A `Retry-After` is a number of seconds or a date, which Concord parses itself. A wait over 60 seconds, from the header or from a rule, means no further attempt; `e.retry_after()` returns the longer of the two. Rules add up: `github::retry().when(mine)` keeps GitHub's rule. A request may be repeated because of its method, its mark, or a status that the policy lists with `not_applied_on`. After `blocked`, the result is the blocked error. There is one log event, `retry`.
- **The other design.** Also `first_wait` and `max_wait`; a second `when` replaces the first; after `blocked`, the error of the previous attempt; and a log event for each attempt and each response.
- **It wins.** Tuning for tests and for long waits, and a free log of the traffic.
- **It loses.** Knobs that nobody needs on the first day. A rule that silently erases the author's. A result after `blocked` that nobody can guess. And a middleware already sees every attempt.
- **Tradeoff.** More knobs, or fewer rules to learn.
- **Why Concord's way.** Fewer rules to learn, and rules that add up. `not_applied_on` stays because real APIs need it: a `429` on a `POST` that is not idempotent (Slack, Telegram, Riot's tournament `POST`s) was turned away before anything was done, so it is safe to retry. `idempotent;` would also retry it after a lost connection, and post a message twice.

### Two timeouts, and what counts as silence

- **Concord.** Two timeouts: `timeout` bounds the whole call, and `idle_timeout` bounds one silence of the server. The idle timeout watches the transport until the head of the response arrives, and a body while it is read; a middleware's time and a reader's own waits never count as silence. The whole rule is [rule 19](../reference/rules.md#behaviour).
- **The other designs.** One timeout per phase, as in OkHttp: connect, read, write, and the whole call. Or an idle timeout that starts at the first middleware, where a middleware that waits on purpose (for a quota) calls `info.sign_of_life()`.
- **They win.** With a write timeout, an upload from memory does not count as silence (without it, a caller adds `with_idle_timeout` for that alone). With `sign_of_life()`, a middleware that hangs before it sends is still cut off at 30 seconds.
- **They lose.** Four knobs, and a write timeout must see the body leave, which `reqwest` does not expose. `sign_of_life()` is a trap: forget it, and a quota's wait expires at 30 seconds and is retried through the quota.
- **Tradeoff.** Exact timeouts, or two that are easy to keep in mind.
- **Why Concord's way.** Two timeouts are easy to keep in mind, and the idle timeout watches only what can be silent: the server. It does not run while a middleware holds a response.

### Pages: when each request is built, and where it goes

- **Concord.** A page's request is built when that page is requested. Every page goes to the origin of the first; another origin is a Build error. `at_url` sets a path and a query, never an origin: a URL at a different origin from the one that the call builds is a Build error, in pages and outside them. The same request twice is an error: the same method, URL (without its fragment), body bytes, and each value of each header, in order. What a secret wrote in the URL or in a header is left out of that comparison (a path segment, a query pair, a header value), and so is a header value that `edit_request` marks `sensitive`; a body is compared by its bytes, and a secret in a host that was replaced makes another origin. Both checks look at the request as built, before any middleware; a middleware that sends it elsewhere does so on its author's responsibility. After an error, the pages end.
- **The other designs.** The next request built early, as soon as a response arrives. No check for a repeated request. Or a check that compares no headers.
- **They win.** One comparison less, or a smaller one. Without the check, a poll that sends the same request again (Telegram's `getUpdates` after an empty batch) could be written as pages.
- **They lose.** Built early, a page misses a `Secret` that was replaced. Without the check, a repeating cursor loops forever. Without the headers, an API that paginates by a header (Cosmos DB) is stopped as if it repeated.
- **Tradeoff.** Less engine, or pages that cannot loop or leave the origin.
- **Why Concord's way.** Pages that cannot loop and cannot carry a key to another host. What a secret wrote is left out of the comparison, so a token in the path (as Telegram has it), replaced between two pages, does not look like a repeat. A pager that may say "again" can come later without breaking.

### Redirects

- **Concord.** Concord's own client follows no redirect. A `3xx` is a response like any other, and an arm can handle it.
- **The other design.** A controlled mode, `gh.with_redirects(same_origin().max_hops(3))`, where each hop goes through the middleware again, inside the deadline.
- **It wins.** A renamed GitHub repository works on every endpoint, and GitHub asks clients to follow redirects.
- **It loses.** Concord would own redirect state and cycles.
- **Tradeoff.** Moved resources that work with no extra code, or one call that is one exchange.
- **Why Concord's way.** One call is one exchange, and a credential is never carried elsewhere by a redirect that nobody described.
- **Can come later without breaking.**
