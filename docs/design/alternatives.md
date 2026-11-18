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
- **It loses.** Noise on every `DELETE`. And it misses the real gap: under an envelope arm (see [Read responses and errors](../guide/responses-and-errors.md)), an endpoint with no arrow could never succeed.
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

- **Concord.** `multipart` ships in `concord-extras`, behind the feature `multipart`: `body multipart().text("chat_id", id).file("photo", "a.jpg", "image/jpeg", photo).finish();`. `text` and `file` return the builder; `finish()` returns a `Result` of a `Body` (its error is a content type that is not a legal header value: nothing panics). When every part is in memory, the boundary is not random: the same inputs produce the same bytes. When a part is a stream, the boundary is random, chosen once by `finish()`: a part made by `Body::stream` makes the body go out once, and a part made by `Body::repeatable` keeps it repeatable. The language does not change. The core has `Body::concat(parts)`, a body made of bodies, whose one-shot streams are taken together with the whole: a stream that was already sent is a Build error before anything goes out. A custom writer has the same building blocks (`Body::as_bytes()`, `Body::concat`, `Body::map_stream`, `Body::is_repeatable`).
- **The other designs.** Not shipped: it is one function that returns a `Body`, and the author writes it. Or shipped, with a random boundary for every body.
- **They win.** One piece less in `concord-extras`. With a random boundary, one rule for every body.
- **They lose.** Each author writes the boundary and the escaping by hand. With a random boundary for every body, the same buffered inputs produce different bytes, so a pager that returns the same call is not detected as a repeat. Concord's boundary is random only when a part is a stream, and `finish()` chooses it once, for every attempt (G6); a body that the description builds again for each page is never the same request anyway (C8.4).
- **Tradeoff.** Less that Concord ships, or one writer that every upload shares.
- **Why Concord's way.** Uploads are common (Telegram, Stripe), and the boundary and the escaping are easy to get wrong. The writer produces deterministic bytes for buffered parts and keeps one boundary for all openings of a repeatable body. A one-shot stream used twice is a Build error before a byte goes out.

### Writers that take what `json` takes, with one convention each

- **Concord.** Four more writers ship in `concord-extras`, each a function like `json`: `bracket_form(value)` and `bracket_pairs(value)` (feature `form`), `gzip(body)` (feature `gzip-body`), `multipart_mixed(parts)` and `multipart_related(parts)` (feature `multipart`), and `json_lines(records, max_bytes)` (feature `json`). `bracket_form` and `bracket_pairs` take what `form` takes: `bracket_form` has no size parameter, so `body concord::bracket_form { metadata; expand; }` works as `body concord::form { … }` does, under the same guard against deep nesting. Each writer has one convention: `a[b]` for a map and `a[0]` for every list; one gzip member with no time in its header; parts with their own headers, the first related part as the root; one JSON value per line, with a line feed after each. Only `json_lines` takes a bound, because its iterator can be lazy, so the size of its input is not already in the author's hands.
  ```rust
  POST write(events: Vec<String>) {
      headers { "content-encoding" = "gzip"; }
      body concord::gzip(concord::json(events)?);
  }
  ```
- **The other designs.** Writers with options: a choice of conventions for nested keys (`a[0]` or `a[]`, as the `qs` package of Node.js offers), a size parameter on every writer, a `gzip` that also sets `Content-Encoding`, or that compresses a stream as it is sent. Or one feature for each writer.
- **They win.** One writer covers Rack and Rails (`a[]`) as well as Stripe (`a[0]`). A size parameter bounds what every writer builds. A `gzip` that sets its own header cannot be used without it.
- **They lose.** A writer with options is a small language of its own, and the entry form `body w { a; b; }` has no place for an argument. A bound on a value that the author already holds in memory protects nothing. A writer returns a `Body`, which carries no header but its content type, so a `gzip` that set a header would need a second way from a writer to the request; and streaming compression requires a deterministic map over a repeatable source to preserve retries (`Body::map_stream`), and a body mapped again for each page is never compared as the same request. One feature per writer adds features and removes no dependency.
- **Tradeoff.** One writer for every convention, or one convention per writer and your own writer for the rest.
- **Why Concord's way.** A writer is a plain function, and your own is a few lines ([a writer of bodies](../guide/extending.md#a-writer-of-bodies)); a shipped one is worth it where the convention is common and easy to get wrong. One convention each keeps the entry form, and gives the same bytes for the same input, so that a retry and a page comparison see one body. Features follow the formats: `json` also gives `json_lines`, `form` gives `bracket_form` and `bracket_pairs`, `multipart` gives the three multipart writers, and `gzip-body`, the one that adds a dependency (`flate2`), gives `gzip`.

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

### A body opened again for each attempt: what an edit sees, and whose failure counts

- **Concord.** `Body::repeatable(make)` is opened once for each attempt. `edit_request` runs once, before the first attempt, on a request whose body is a placeholder that cannot be read: an edit changes the headers and the URL, and reading or removing the placeholder, or a detected replacement by a different kind of body, is a Build error. Wrappers and look-alikes cannot always be detected, so edits must leave the body alone; if building succeeds, the declared repeatable body is sent. When the body fails, while it is opened or read, the attempt ends with a Transport error of class Other, with the body's error in the chain of `source()`, whatever the transport reports.
  ```rust
  let body = Body::repeatable(move || {
      let snapshot = snapshot.clone();
      async move {
          let file = tokio::fs::File::open(snapshot).await?;
          Ok::<_, io::Error>(ReaderStream::new(file))
      }
  }).content_type("application/octet-stream");
  ```
- **The other designs.** An edit that sees a request with no body, or an edit that runs again on each attempt, with that attempt's stream. And, for a body that fails, the outcome that the transport reports, as for any other attempt.
- **They win.** With no body, an edit sees an ordinary request. With an edit on each attempt, an edit could read or sign the body. The transport's outcome needs no path from the body to the engine.
- **They lose.** An edit that sees no body cannot tell a request without a body from one whose body is not open yet: a signer inside an edit would sign an empty payload, and an edit that sets `content-length: 0` on a request with no body would send an empty upload, and report it as a success. An edit on each attempt breaks the rule that a request is built once ([C3.1](../reference/contracts.md)): what an edit computes, such as a key made for one write, would change from one attempt to the next. And a transport may report a body that failed as something else: a failed connection, which the standard table retries, or even a response; the cause is then lost.
- **Tradeoff.** An edit that sees an ordinary request, or one that a missing body cannot mislead; a failure as the transport saw it, or as the body caused it.
- **Why Concord's way.** What an edit is for, the headers and the URL (the headers that come with an upload URL, the metadata of an object), works on the placeholder; what needs the bytes of each attempt is a middleware, which sees each attempt's own stream. The body's own error is the cause, and class Other is not in the standard table, so the standard policy does not open a failing source again and again; a `when` rule can still ask for another attempt.

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

### A limit, described bucket by bucket

- **Concord.** `concord::limit::Limit` is a middleware. Each bucket is described in one place: the key that a request is charged to, its windows (known in advance, or learned from the headers of a response), its cost, and which `429` holds it.
  ```rust
  Limit::new()
      .bucket(Bucket::new(|call| Some(format!("{} application", call.request().url().host_str()?)))
          .learn(counts("x-app-rate-limit", "x-app-rate-limit-count"))
          .hit_when(|seen| seen.header("x-rate-limit-type") == Some("application")))
  ```
  A call is charged to all its buckets or to none, before it is sent. The limit never sends a request again, and never fails a call after its response: a header that cannot be read teaches nothing, and the response is returned as it is.
- **The other designs.** A limit that sends the request again after a `429`, once its `Retry-After` has passed. A limit that fails a call whose response headers it cannot read. Or the keys, the windows, the costs and the rules for a `429` in separate tables, joined by the name of the bucket.
- **They win.** A `429` is recovered with no retry policy. A server that changes its headers is noticed at the first response. Separate tables let one rule serve many buckets.
- **They lose.** A limit that sends again repeats an unmarked `POST`, past the engine's checks, in an attempt that `tries()` does not count. A limit that fails after a response hides a write that succeeded: the server did the work, and the caller sees an error. Separate tables spread one bucket over several places, and a name misspelled in one of them silently makes another bucket.
- **Tradeoff.** A limit that recovers on its own, or one that only plans, and leaves sending again to retry.
- **Why Concord's way.** Sending again is the job of retry, which already knows which request may be repeated ([C4.1](../reference/contracts.md)); an attempt that the engine makes again goes through the limit like any other, and waits behind the same gates. A response that was received is the truth about what the server did, so it is returned unchanged: the worst that an unreadable header can do is let one more request through, and the server answers it with a `429` that the limit reads. A bucket in one place is read in one place.

### A renewal that is a shared future, and a `401` that the engine retries

- **Concord.** `concord::auth::Renewed` keeps one token. When it is missing or about to expire, the first call starts a renewal, and the calls that come meanwhile wait for the same one. The renewal is a future that those calls share, not a task: a call that is dropped leaves it to the others, and when the last one is dropped, the renewal is dropped with it. A `401` marks the token as rejected and is returned as it is; `after_401`, added to the retry policy, asks the engine for one more attempt.
  ```rust
  let documents = Documents::new().with_middleware(renewed)
      .with_retry(Retry::standard().when(after_401));
  ```
- **The other design.** A background task renews the token, and the middleware sends the request again with a new token after a `401`, as OkHttp's `Authenticator` and Azure's bearer policy do.
- **It wins.** A renewal finishes even when every call that waited for it was dropped, so the next call finds a token. Recovery from a `401` needs no line in the retry policy.
- **It loses.** A task is something that Concord would start on your runtime, and it can outlive the calls and the client that needed it. A middleware that sends again goes around the engine: an unmarked `POST`, or a body that is a stream, is sent twice, and `tries()` counts one attempt.
- **Tradeoff.** A renewal that survives its callers, or no work that nobody waits for.
- **Why Concord's way.** Concord starts no task of its own, so a dropped call leaves nothing running, and the next call starts the renewal again. Whether a request may be sent twice is decided in one place, the engine, for every middleware; `after_401` is one rule on a policy that the client already has, and an API that promises that a `401` applied nothing says so with `not_applied_on(401)`.

### Readers of records, with a bound and no default

- **Concord.** `read JsonLines::new(64 * KIB);` and `read Sse::new(16 * KIB);`. The bound of one record is an argument, with no default. A record over the bound is an error that ends the stream, and the reader holds at most its bound, plus one chunk of the transport.
- **The other designs.** A default bound, as `Json` has 10 MiB for a whole body. Or no bound: a reader of lines that grows its buffer until the line ends.
- **They win.** `read JsonLines;` is shorter, and most records are small.
- **They lose.** With no bound, a server that never ends its line fills the memory of the client. A default bound is a number that fits no stream: a delta of a streamed reply and a whole object of a Kubernetes watch differ by orders of magnitude, so a default that is too small fails on the first large record, and one that is too large bounds nothing.
- **Tradeoff.** One argument less, or a bound that the author chose.
- **Why Concord's way.** A whole body ends, so one limit for the whole body is a safe default. A stream may never end, so the bound that matters is the bound of one record, and the size of a record is a fact of the API, which the author of the description knows.

### A URL that the API returned: a second description, and `at_url`

- **Concord.** There is no call type for a URL that the API returned. The other host is described on its own, with its own `base` and none of the API's credentials, and `concord::transfer::at(call, url)` checks the URL and returns `call.at_url(url)`. `at_url` never changes the origin: a URL at another origin than the base of that call is a Build error ([C8.6](../reference/contracts.md)).
  ```rust
  client Uploads(origin: Base) {
      base origin;
      POST send(bytes: Body) { body bytes; }
  }
  ```
  ```rust
  concord::transfer::at(uploads.send(bytes), &ticket.url)?.await?;
  ```
- **The other design.** A call type of its own for a URL that is known only at run time: a method, the URL and a body, with its own rules for headers, retry and timeouts.
- **It wins.** No second description: one line at the call.
- **It loses.** It is a second way to make a call, beside the description, with rules of its own to learn and to keep in step with the engine's. It could only be built on hidden parts of the engine, which nothing in `concord-extras` but the `tower` adapter uses. And the rule that keeps a credential at its origin would be written a second time.
- **Tradeoff.** One line at the call, or one way to make a call.
- **Why Concord's way.** The description and the engine already have what is needed: a description of the other host carries none of the API's credentials, and `at_url` is the engine's, so the origin rule is the engine's too, with the retry, timeout, reader and body rules of every call. What is left is small: refusing a URL that would not be sent as it is written, because a server checks a signed URL against the bytes that it signed.

### A refresh token that rotates: a driver that the application polls, and a pair handed back

- **Concord.** `concord::auth::Rotating::new(&base, store, exchange)` returns the middleware and a `RotationDriver`, a future that runs the exchanges and the saves. The application polls it or spawns it; Concord starts no task. While the driver is polled, a renewal that began is finished even when every call that waited for it was dropped. `renewal_timeout` bounds how long a call waits, and never cancels the exchange. A pair that the server issued and the store could not keep is not dropped: the driver ends with `RotationError::Unsaved`, which holds the pair, and the application saves it by its own means.
  ```rust
  let (auth, driver) = Rotating::new(&base, store, exchange);
  let driver_task = tokio::spawn(driver);       // the application's task, not Concord's
  ```
- **The other designs.** A task that Concord starts on the runtime. Or a renewal that is a future shared by the calls that wait, as for `Renewed` ([above](#a-renewal-that-is-a-shared-future-and-a-401-that-the-engine-retries)). Or a driver that drops the pair, with an error, when its last save fails.
- **They win.** A task needs no line in the application. A shared future needs neither a task nor a driver. A dropped pair keeps the result of the driver small.
- **They lose.** A task outlives the calls and the client, on a runtime that Concord does not own. A shared future is dropped with its last call, and an exchange that was sent and then dropped can only lose the replacement token: the old one is spent, and the account then needs a person. A timeout that cancels the exchange is the same loss, and so is a pair dropped after its save failed, only later.
- **Tradeoff.** One line less in the application, or a renewal whose owner is named.
- **Why Concord's way.** Concord starts no task, and a single-use token cannot be renewed by a future that any call may drop. The driver is the owner that the application names, as the connection future of `hyper` and the `Connection` of `tokio-postgres` are: the library gives the future, and the application decides where it runs. The author bounds the exchange itself, with the timeout of the client that calls the token endpoint. When the store cannot keep a pair, the application is the last place that can, so the pair goes back to it.

### Signing: a middleware for one origin, and `sign` for one request

- **Concord.** `concord::sign::SigV4` (feature `sign`) is a middleware for a service with one origin and one region: it checks the origin against its base before a key is used, and signs each attempt anew. `SigV4::sign(&mut request, region)` signs one request, for a region chosen per call, after the same check of the origin as the middleware. `SigV4::sign_any_origin(&mut request, region)` signs one request without that check, for an adapter whose hosts are known only per call; that adapter checks the origin itself, against the origins that it was configured with. Two functions are public for the two checks that a provider's own proof most often gets wrong: `body_bytes`, which refuses a stream without polling it, and `check_origin`. There are no digest helpers.
- **The other designs.** The middleware only, for one origin and one region. Or public helpers for any signature: SHA-256 and HMAC, and a view of the request to sign.
- **They win.** With the middleware only, there is one way to sign, and Concord always checks the origin. With helpers, an author writes a provider's proof without another crate.
- **They lose.** With the middleware only, an object store whose host changes with the bucket, and a call to another region, cannot be signed. Helpers duplicate `sha2` and `hmac`, which an author of a proof imports anyway; they would cover one hash, and they would be a public surface to keep for as long as Concord exists.
- **Tradeoff.** One way to sign, or the way that each adapter needs; a kit for every proof, or only the two checks that such a kit would hide.
- **Why Concord's way.** The [S3 example](../examples/s3.md) has a host for each bucket and calls to other regions, and it must be writable: `sign_any_origin` exists for it, and the check of the origin is then the adapter's, as the first paragraph of the rustdoc of `sign_any_origin` says. The method without the check, `sign_any_origin`, says so in its name; `sign`, the name an author reaches for first, checks the origin as the middleware does. A provider's own HMAC proof is a recipe with `hmac` and `sha2` ([a signature](../guide/credentials.md#a-signature)); what Concord adds is what such a recipe gets wrong: hashing a stream, and signing for another host.

### Needs that have a recipe, not a value

- **Concord.** Six needs have a recipe in the guide, compiled and run by the repository's tests, and no value in the library: [a key made for one write](../guide/retries.md#a-key-made-for-one-write), [a documented download redirect](../guide/extending.md#a-documented-download-redirect), a provider's own HMAC proof ([a signature](../guide/credentials.md#a-signature)), and the three weighed here against a value: [binary frames](../guide/recipes.md#binary-frames), [a multipart response and its batch](../guide/recipes.md#a-multipart-response-and-its-batch), and [several hosts](../guide/recipes.md#several-hosts). (A large upload sent again has a value, `Body::repeatable` ([above](#a-body-opened-again-for-each-attempt-what-an-edit-sees-and-whose-failure-counts)); the caller's loop stays the recipe for a `Body::stream`.)
- **The other designs.** A value for each: a reader of length-prefixed frames; a reader of multipart responses, with a parser of the HTTP responses that a batch holds; a pool of hosts, with failover in the engine.
- **They win.** Each need is one line of a description.
- **They lose.** Each value is a surface that Concord keeps for as long as it exists, for few APIs, and each repeats or goes around something that already holds.
  - `LengthDelimitedCodec` over `StreamReader`, from `tokio-util`, which `reqwest` already depends on, reads frames, and it reads the same `ByteStream`, so the timeouts and the head of the response are kept.
  - A parser of MIME of Concord's own would serve a handful of products. `multer` reads a finite reply under a limit on the whole stream, which is the limit that protects memory.
  - A pool cannot be the engine's: a second client starts counting attempts again, and each call has its own timeout. One client for each trusted origin, each with its own credentials, keeps a credential with its base.
- **Tradeoff.** A line in the description, or a few dozen lines that the author can read and change.
- **Why Concord's way.** The rule that sorts a need into a value, a recipe or nothing is in [scope and limits](scope.md#a-value-a-recipe-or-nothing). The recipes need nothing new from the engine: they rely on contracts that already hold (a `ByteStream` keeps both timeouts, a credential stays with its base), and the tests of the recipes pin those contracts.
- **Can come later without breaking.**
