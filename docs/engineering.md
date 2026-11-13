# Engineering: the level Concord is held to, and where it stands

*Concord V5 design · reference · [index](../README.md) · the other sheets: [grammar](grammar.md), [rules](rules.md), [messages](messages.md), [the crate](crate.md), [contracts](contracts.md)*

The level is the one of the libraries of the `tokio` team and of `reqwest`. That is a list of properties, not polish. Each one below has a state and the evidence for it.

| State | Meaning |
|---|---|
| **holds** | There is evidence that a compiler or a reader produced. |
| **partly** | Decided and written; the evidence covers a part. |
| **not yet** | Decided on paper; nothing has checked it. |

The evidence is in [`api/`](../api), four crates as the owner asked (decision D196): `concord-core` (the traits and the engine), `concord-extras` (what is shipped: `json`, `Json`, `by_offset`, …, written with the public traits and helpers of the core; only the `tower` adapter also uses a hidden helper of the engine, for the URL of an answer), `concord-macros` (the macro) and `concord`, the facade that people depend on; beside them, a kit for tests, `concord-testkit`. `api/tour` holds the clients of [the tour](tour.md) written by hand, as the macro writes them, and `api/tour-gen` the same clients written by the macro; both pass the same tests. Of the 99 contracts, 93 hold with a test that carries their number, and 6 hold with no test yet ([contracts](contracts.md)). The tests are judged by the faults that they catch: of 143 mutations written into the code one at a time (`just mutants`), they catch 143. How the engine and the macro work: [the engine](engine.md), [the macro](macro.md).

## The core

| # | Property | State | Evidence, and what is missing |
|---|---|---|---|
| K1 | **One concept, one job.** The whole surface fits on one page. | holds | Twelve public modules, one sentence each ([the crates](crate.md)). The core has no format, no scheme and no pager, and no `auth` at all: a credential is a value, and a signature is a middleware. Two things with one job each: a middleware is what a request goes through, a transport is what really sends it. After three rounds of review: one way to give an HTTP client (`with_transport`), no `Deref` on an answer, two helpers on the error (`is_timeout`, `is_status`), pages typed by what they give and not by the call. |
| K2 | **Layers with clean seams.** | holds | Description → generated code → core → middlewares → transport → `reqwest`, and the shipped pieces beside the core, in a crate of their own. The seams are the public traits: `Reader`, `Middleware`, `Transport`, a body as a function, a pager as a function with `Slot` and `Call`. The clients of `api/tour` are written by hand against them, and those of `api/tour-gen` by the macro. What generated code calls is a hidden module with no promise of stability, as in `serde` and `tokio`. |
| K3 | **The domain's own words and types.** | holds | Table "What is reused" below. |
| K4 | **Wrong use does not compile.** | holds | Table "Wrong use" below: what the types of the core refuse is checked by the compiler. The checks of the macro are code: its 46 messages ([messages](messages.md)), each compared with the page by a test. Places where wrong use compiled were found and closed: a signed API whose client is made without its signer (`middleware`, D236), and eleven more in the challenge run (D239 to D265), each a row of the table. |
| K5 | **Async done right.** | holds | Table "Async" below: bounds and cancel safety are in the signatures and in the docs of each method. How the clocks, a drop and pages work is [the engine](engine.md) sections 4, 5 and 7, with tests on paused time for C7.6, C7.7 and C8.5. |
| K6 | **Errors are a designed API.** | holds | `concord::Error` compiles as designed: one pointer, kinds that can grow, `source()`. The catalogue of lines is in [messages](messages.md), "When a request is built or answered"; a test produces each line that a client of `api/tour` can produce through the public surface, and compares it whole. Every `Debug` of Concord's own types prints names and no value. |
| K7 | **Explicit, with safe defaults.** | holds | Table "Defaults" below: one row for each, with its knob. |
| K8 | **A stated cost.** | holds | Table "Cost" below. The size and the compile time of generated code are measured: [the macro](macro.md) section 8. The budget of a call and how it is measured: [the engine](engine.md) section 9. The measure of record goes through a client that `concord::api!` writes, on the `GET` that the budget names: Concord adds 1.07 µs and 23 allocations, inside the budget of 2 µs and 25. |
| K9 | **It can grow for years.** | partly | "Growing" below. Checked against the surface. No release has tested it. |
| K10 | **It composes with the ecosystem.** | holds | `concord::tower::Tower<S>` compiles (feature `tower`): a `tower` service is a transport. `Pages`, `Items`, `ByteStream` are `futures` streams. `reqwest::Client` is a transport. |

## The surface

| # | Property | State | Evidence, and what is missing |
|---|---|---|---|
| S1 | **It reads without a manual.** | holds | In the final run, on the final language: two readers on two models, who had only the tour, each answered twelve questions with one right answer about a description never seen. Twelve right, twice. (In the two-hour run, on the language of that day: twelve of twelve, one reader.) All three are models. |
| S2 | **One way to say one thing.** | holds | [The grammar](grammar.md) is on one page, with a table of what was found not regular and what was done with each. Four real APIs, 178 endpoints, were then checked against it by a reader: nothing outside it. One irregularity is kept and named by three readers: in `paginate`, a name is a place. |
| S3 | **The generated client is ordinary good Rust.** | partly | `cargo doc` on `api/tour` builds with no warning. A generated type has one name, its place in the tree. Read five times by a caller: 6, then 6.5, then 7, and 7 and 7 in the final run. Ten rules that the macro must follow came out of these readings ([the macro](macro.md) section 9). The macro writes the clients of `api/tour-gen`, which pass the tests of the clients written by hand. Its size and compile time are measured up to 2,000 endpoints ([the macro](macro.md) section 8): the check grows faster than the size, an open question. |
| S4 | **Messages teach.** | partly | The macro produces the 46 refusals of [the catalogue](messages.md), each with what it points at and what to write, and a test compares each with the page, word for word. How it says them on stable Rust: [the macro](macro.md) sections 6 and 7. No reader has yet judged the messages as the macro prints them. |
| S5 | **Extending uses the same pieces as the library.** | holds | It is now a fact of the build: `json`, `form`, `multipart`, `Json`, `Text`, `Binary`, `Stream`, the pagers and `basic` are in `concord-extras`, another crate, which can use only what your crate can use. The macro knows none of them. A pager "of your own" is one of its tests, and it runs. |
| S6 | **No cliffs.** | holds | Table "The ladder" below. |
| S7 | **Tools keep working.** | not yet | **On paper.** Table "Tools" below: stated; only `rustfmt` was tried. What the design does for an editor is [the macro](macro.md) section 10; nothing of it was tried. |

## What is reused, and what is new (K3)

| Reused as it is | From |
|---|---|
| `Request`, `Response`, `Client`, `ClientBuilder`, `Body` (in a fake) | `reqwest` |
| `StatusCode`, `HeaderMap`, `Method` | `http` |
| `Bytes` | `bytes` |
| `Serialize`, `Deserialize`; `json!`, `Value` | `serde`, `serde_json` |
| `Stream` | `futures-core` |
| `Service` (behind a feature) | `tower-service` |

The three crates that are in public signatures are re-exported as they are: `concord::reqwest`, `concord::http`, `concord::bytes`. Someone who writes a transport names the same `reqwest` as Concord, and a generated client needs no dependency of its own on them.

| New | Why it is not something that exists |
|---|---|
| `Secret` | `reqwest` has no type for "may go on the wire, may not be printed", and it must be replaceable in place. |
| `Body` | `reqwest::Body` does not carry its content type, and cannot be made again. |
| `Headers` | A view of `HeaderMap` that reads text, and `require`. `as_map()` gives the `HeaderMap`. |
| `Answer<T>` | A value with its status and headers. `reqwest::Response` is the unread one. |
| `Error<E>` | `reqwest::Error` prints URLs, drops the body of a refusal, and knows nothing of the API's own problem. |
| `Reader`, `Middleware`, `Transport` | One method each. `tower::Service` was looked at for the transport and left as an adapter: every fake would carry `poll_ready`. |
| `Http` | A `reqwest::Client` that follows no redirect, whatever settings of yours it takes: a `3xx` is an answer ([the engine](engine.md) section 6). |
| `Retry`, `Pages`, `Items`, `IntoItems` | `reqwest` 0.13 has a retry of its own, with a budget; it has no wait between tries, does not read `Retry-After`, and cannot know which endpoint is idempotent. It stays on, below Concord's, in `Http` and in a `reqwest::Client` of yours. No shared type exists for pages, nor a trait for "a list of items" that `Option` and `Result` do not have. |

## Wrong use (K4)

| The mistake | What happens |
|---|---|
| A required input is forgotten | It is an argument: the call does not compile. |
| A call is built and never sent | A warning: calls and what they return are `#[must_use]`. |
| A call is sent twice | It does not compile: sending takes the call. `clone()` first, on purpose. A body that is a stream is shared by the clones: the second send is a Build error, and nothing is sent ([contracts](contracts.md) C2.15). |
| A secret is printed with `{:?}` | It prints `Secret(…)`. It has no `Display`. |
| An argument of a type that only looks close | The set of accepted arguments is sealed (`concord::arg::Arg`): a type from outside cannot add one. |
| An arm gives another type than the arrow | It does not compile: an arm is a closure with the arrow's type. |
| A `HEAD` with an arrow and no arm for a 2xx | It does not compile: there is nothing to read. |
| An endpoint with no arrow under an arm that reads the body | It does not compile: it gets an arrow, or removes the arm. |
| An exact status under an inherited class arm, without `override` | It does not compile: arms are compared by what they take. |
| `.items()` on a page that is an `Option` or a `Result` | It does not compile: they have no `IntoItems`, and no error of the API is lost in an iterator. |
| A paged call awaited directly | It does not compile (`is not a future`): `.send()` for one page, `.pages()` for all. |
| A key of the API on an entry inherited under another `base` | It does not compile until the entry is written again, or removed: a secret is bound to the `base` in reach where it is written ([contracts](contracts.md) C8.9). |
| A struct that holds a `Secret`, given to `json(…)` | It does not compile: a `Secret` has no `Serialize`. As an entry of a body, a `Secret` is written as its text. |
| A reader that gives its decoder's error, which can quote the body, as the cause | It cannot: a reader hands Concord no cause, and Concord makes `source()` from the place and the kind ([contracts](contracts.md) C5.11). |
| `headers { user_agent; }`, or a `host` header | It does not compile: write `"user-agent" = user_agent;`; the host is the `base`. |
| An optional input of the client in a `middleware` expression | It does not compile: a middleware is made in `new`, before any option is set. |
| An endpoint with no `base` in reach | It does not compile. |
| `Self` in a type of a description | It does not compile: the macro says to write the path of the type. |
| An envelope arm from above (`2xx { body } => unpack(body)`) over an endpoint whose arrow is an `Option` or a `Result` | It does not compile: a `null` would be a `None` with no word. The endpoint writes the arm itself, or removes it (message 46). |
| A type that is not `Clone` as an input | A compile error on the type, in the signature. |
| A `match` on the kind of an error with no `_` arm | It does not compile: the enums are `#[non_exhaustive]`. |
| A client of a signed API made without its signer | It cannot be made: the description says `middleware Signed::new(key.clone());`, and `new` makes the signer. Nothing removes it, and `with_transport` goes under it. |
| Library code that panics | There is none to check yet. The rule: the runtime never panics on any input or any answer. |

## Async (K5)

| What | `Send` | `Sync` | `'static` | If it is dropped |
|---|---|---|---|---|
| A client, a handle | yes | yes | yes | — |
| A call | yes | yes | yes | Nothing was built, nothing was sent. |
| `Sending` (a call on its way) | yes | — | yes | **Not cancel safe**, like any request. Dropped in the transport: the request may have reached the server. Dropped between two tries, or while a body is read: nothing more goes out. After a drop Concord starts no further try ([the engine](engine.md) section 5). |
| `Pages::try_next`, `Items::try_next` | yes | — | — | **Cancel safe.** The exchange in flight lives in the value and goes on with the next `try_next`. Nothing is lost, nothing is sent twice. After an error they are over, as a stream too. |
| `ByteStream::try_next` | yes | — | — | **Cancel safe.** No piece is lost. |
| `Reader::read`, `Transport::send` (yours) | must be | — | — | Your future is dropped with the call. |

The two clocks are `tokio` timers, so a test that pauses time controls them ([the engine](engine.md) section 4). Concord needs a `tokio` runtime, as `reqwest` does, and starts no task of its own.

## The error (K6)

The printed lines, with their one pattern, are in [messages](messages.md), "When a request is built or answered".

`Debug` prints the same names as a struct, so `fn main() -> Result<…>` is as safe as a log line. `source()` is the cause, when there is one to give ([contracts](contracts.md) C6.1): the HTTP library's error with its URL taken out (none for an error about a status or about decoding: its class is enough), the failure of the author's own code in a Build error, or a cause that Concord makes from the place and the kind of a body that did not read; never a value of the answer, since a reader hands no cause of its own. It is never written into the line itself. Everything that an error keeps from an answer is bounded: 64 KiB of the body, 1 KiB of a reader's own message. A caller matches with `e.kind()`, `e.class()`, `e.is_status(404)`, `e.is_timeout()`, `e.problem()`, `e.retry_after()`.

## Defaults (K7)

| | Default | The knob |
|---|---|---|
| Trying again | Off. | `with_retry(Retry::standard())` |
| Silence | 30 seconds, watched in the transport until the head, and while a body is read ([the engine](engine.md) section 4). A middleware's time is not silence. | `idle_timeout …;` in the description (a long poll); `with_idle_timeout(…)` |
| The whole call | No limit. | `with_timeout(…)` |
| Connecting | 30 seconds. | `concord::Http::with(…)`, or your own HTTP client. |
| A body read whole | At most 10 MiB. | `read Json.limit(…);` |
| What an error keeps | At most 64 KiB of the body, and 1 KiB of a reader's own message. | — |
| A redirect | Not followed: a `3xx` is an answer. | An arm; or your own HTTP client. |
| Cookies, `Referer` | None. | `Http::with(…)`, or your own HTTP client. |
| Decompression | None: a body is given as it came. | `concord::Http::with(…)`, or your own HTTP client. |
| `Accept` | Not written. | `headers { "accept" = "…"; }` |
| The label of an answer | Not checked. | `read Json.strict();` |
| Who you are | Nothing, until the description places a credential. | An entry of `headers`, of `query` or of a body. A signature is a middleware of the description. |
| How an answer is read | Nothing: there is no format by default, and an endpoint that reads a body with no reader in reach does not compile. | `read Json;` on the client |
| The proxy settings of the environment | Read, as `reqwest` does. | `Http::with(…)`, or your own HTTP client. |

Nothing else is ambient: no file is read, no global state is kept.

## Cost (K8)

| | |
|---|---|
| One call, built | The inputs are moved in. A reference given as an argument is cloned once. |
| One request, built | One `reqwest::Request`. Nothing of the tree is looked up: what an endpoint inherits was worked out when the macro ran. |
| One try | One copy of the request (`try_clone`), one boxed future for the transport. |
| One page | One clone of the call. |
| An error | One allocation; a `Result` that holds it is one pointer wider than its value. |
| Generated code for one endpoint | Its size and its compile time: [the macro](macro.md) section 8. The methods that are the same for every call come from one macro of the core (`call_methods!`). |
| What is shared | The transport, retry, the clocks, reading and the error are functions of the runtime, written once and called by every endpoint. |

The time and the allocations of a call, against `reqwest` alone: [the engine](engine.md) section 9.

## Growing (K9)

| | The policy |
|---|---|
| Versions | Semantic versions. A change that breaks a description, a call or a trait of an extension is a major version. A new major version of `reqwest` is a new major version of Concord: its types are in almost every signature. |
| `reqwest`, `http`, `bytes`, `serde` | They are in the public surface, so their major versions are Concord's. `tower-service` and `futures-core` are behind a feature or used for one trait: the same holds for them. `tokio` is used for its timer only and is in no public signature: its new major version is not one of Concord's. |
| Enums | `Kind`, `Class`, `Kept` are `#[non_exhaustive]`. Every other type has private fields. |
| Traits to implement | `Reader`, `Middleware`, `Transport`, `IntoItems`, and `Slot` for the places of a call: a new method always comes with a default. |
| Traits not to implement | `Arg`, `IntoBody` and `IntoNext` are sealed, so a case can be added. `page::Call` is written by the macro for every call, and by nobody else. |
| A call | An option of the API keeps its own name (Telegram's `send_message` too). Concord's own names are a closed list, and the prefix `with_`. Concord adds no name to a call, a handle or a client in a minor version, unless it starts with `with_`: it cannot collide. |
| The language | A new statement or a new method word is an addition. What compiles keeps its meaning. A new reader, writer or pager is not a change of the language at all: the macro knows none, so it is an item added to `concord-extras`, or to your crate. |
| The crates | `concord-core`, `concord-macros`, `concord-extras` and `concord` have one version and are released together. People depend on `concord`, which depends on the core and the macro at exact versions: the macro and the engine that it writes for are always a pair. |
| `Retry::standard()` | Its numbers are part of the contract. |
| Features | Additive. `json` and `form` (on by default) bring `serde_json` and `serde_urlencoded`; `tower` adds an adapter, and `multipart` a writer of bodies; neither changes anything else. The TLS stack is a feature too: the application chooses it, not the library. |
| Threads | `Send`, `Sync` and `Unpin` of the public types are asserted in the crate, so a change that loses one does not compile. |

## The ladder (S6)

Each step is one more thing to learn, never a rewrite.

| The need | What is written | Lines |
|---|---|---|
| A plain endpoint | `GET get(id: u64) -> Task { path "{id}"; }` | 1 |
| An optional filter | `done?: bool` and `query { done; }` | 2 |
| A status that means something | `status { 404 => None }` | 1 |
| How answers are read | `read Json;`, once on the client | 1 |
| Another format, a large body | `read Text;`, `read Stream;` | 1 |
| Who you are | `headers { "authorization" = "Bearer {token}"; }` | 1 |
| Pages | `paginate by_cursor(after, Page::next);`, then `.pages()` | 1 |
| A body made of inputs | `body json { chat_id; text; }` | 1 |
| A long poll | `idle_timeout Duration::from_secs(60);` on the endpoint | 1 |
| An upload of files | `body multipart().file(…).finish();` (feature `multipart`) | 1 |
| A body in a format of your own | `body xml(task);` and a function | about 5 |
| A pager of your own | `paginate keyset(after);` and a function | about 10 |
| A signature, a token that is renewed | `middleware Signed::new(key.clone());` and a type with `Middleware` | about 15 |
| A format to read of your own | `read Xml;` and a type with `Reader` | about 10 |
| One request changed by hand | `.edit_request(\|request\| …)` | 1 |
| The raw answer | `.send_raw()` | 1 |
| A conditional `GET` | `.send_if_none_match(etag)` | 1 |
| Sending it yourself, reading it with the description | `.request()?`, your client, then `.read_response(response)` | 2 |
| Everything by hand | `.into_request()`, then `reqwest` | — |

## Tools (S7)

| Tool | What is expected | Tried? |
|---|---|---|
| `rustfmt` | It leaves the inside of `api! { }` alone: the language is not Rust syntax. A description is formatted by hand, in the style of the tour. | Tried (`rustfmt` 1.10.0, on a file with a description and a badly formatted function): the function was formatted, the description came back byte for byte. A formatter for descriptions is not planned yet. |
| `rust-analyzer` | The types and the Rust expressions in a description keep their own tokens and spans, so hover and "go to definition" work on them. Completion of Concord's own words does not. | Not tried. What the design does for it: [the macro](macro.md) section 10. |
| `cargo expand` | The generated code reads like `api/tour`: a type at its place in the tree, one function for one request, the author's expressions as written. | The hand-written form is the evidence. |
| A generator from OpenAPI | The grammar is regular and small: a description is a tree of nodes with statements. Behaviours (a reader, a pager) stay written by a person. | Not tried. |

## What readers said about the real surface

Every line here is a reading by a model, named with the model that its log shows (`experience/README.md` has the table). None is a person, and none is a test of behaviour. Opus and Fable are Claude, like the author; Sol and Astra are the outside view. The reports are in `experience/run/`.

| Round | What was read | Who | Result | What was done |
|---|---|---|---|---|
| 1 | The tour only, then twelve questions with one right answer about a description never seen | Astra | 12 of 12: nine "known", three "guessed" | Nothing to change. |
| 1 | The runtime surface: "you maintain `tower` and `hyper`, would you merge it?" | Opus | Merge after changes. Ten findings. | All ten applied (D183, D184). |
| 1 | The generated documentation, as someone who only calls the client | Fable | 6 of 10: "no endpoint has real documentation" | D186: the documentation that the macro generates. |
| 2 | The runtime surface, a second look | Opus | Merge after changes. Of its ten: six applied well, four with a flaw (three doc lines, one constructor to hide). Two new findings. | All taken (D195). |
| 2 | The runtime surface, fresh | Astra | Merge after changes. Five findings; two were also Opus's. The worst: a connection that fails after a followed redirect looked like "nothing was sent". | All five taken, one in another form than proposed (D192 to D195). |
| 2 | The generated documentation, as a caller | Fable | 6.5 of 10: the endpoint docs "already at reqwest level", the pages around them not | D188 to D190. |
| 2 | How the language feels, as a caller, an author, and the author of a hard API | Sol | 9, 9 and 8 of 10. Three points. | One taken (D187), one sentence corrected, one left with its reason (D191). |
| 3 | The runtime surface: its own five findings, checked | Astra | All five "applied well". Two new, both doc lines of a contract. Merge after changes. | Both taken (D203). |
| 3 | The runtime surface, fresh | Sol | Merge after changes. Four findings, none about structure. | All four taken (D203). |
| 3 | How the language feels, as the same three people | Opus | 8, 8 and 7 of 10. Seven points. | Five taken (D199, D202, D203); `Refused` keeps its name; a worked S3 description is not written yet. |
| 4 | The four crates and the extension traits, after the owner's rule: "does the cut hold?" | Astra | Merge after changes. "The cut holds": the core knows no format, scheme or pager, and the extras use nothing that a user's crate could not use. An XML reader, a multipart writer, a pager by page number, a keyset pager on two inputs, an HMAC signer, a shared OAuth token and a recorder all fit the traits. Three findings. "Keep value credentials as `Secret`, with `Auth` reserved for processes." | All three taken (D205). |
| 4 | How the language feels after the owner's rule, as the same three people, on four pages | Opus | 8, 7 and 7 of 10. Seven points; five were pages that disagreed with each other. The rule "helped": a reader is a value, `read Xml;` weighs the same as `read Json;`, the list of schemes is gone. It "hurt": `use concord::{json, Json};` reads like a typo, and in `paginate` a name is not a variable. | The five disagreements fixed; one became a decision (D204). Two points of taste and one open point not taken (D205). |
| 5 | Seven pages against nine rules and against the descriptions in `api/tour`: "what still disagrees?" | Fable | Twelve places. None in the tour, the grammar or the crate map. | All twelve fixed. |
| 5 | The generated documentation after its last two passes, as a caller | Fable | 7 of 10 (it was 6, then 6.5). "The endpoint method doc is the best part … better than most hand-written clients." Seven points. | Three taken at once: the pager's trait is off the page of a call, the request methods say what they give in plain words, two lines of author's talk are gone. Four are rules for the macro and are not written yet: a type in generated text is a link; a client with no `problem` line does not show `()`; a setter says what it accepts; an example on every client. |
| 6 | The removal of `auth`, on the owner's word: a signature and a renewed token as a transport around another | Astra | "Sound after changes. Removing `auth` is viable, but the current contract promises more than the engine can enforce through an opaque `send`." A signer over the body, a shared token that is renewed, and both stacked over a fake can all be written. Six findings, all about what the contracts say. | All six taken (D208). |
| F | **The final run** (2026-11-12, D209 to D234). Thirteen open points of taste, as an opinion | Astra, Opus | They agreed on eleven of thirteen. | D210 to D222. |
| F | The engine page, first version, as the engineer who must build it | Astra (`max`) | Nine findings. "Could I start coding from this page? No." The worst: the proof of "never sent" did not hold, and nothing said who owns the clocks. | All nine taken (D228); the page was rewritten. |
| F | The macro page, first version, with 22 probes | Sol (`max`) | Eight findings. "No. The single blocker is the missing lowering contract for the author's bindings and types." The main claims held under probe: hygiene, contextual words, several errors in one build. | All eight taken (D232); the page was rewritten. |
| F | Four real APIs written in the language, from the guide only: Riot in full, Stripe, GitHub, an object store like S3 | Sol (two), Opus (two) | 178 endpoints, all written with what the grammar has. Writing it: 6, 6, 7 and 6 of 10. Calling it: 8, 8, 7 and 7. Seven things could not be said; none is an endpoint. | Parts came back (D229); six more decisions (D224 to D231). The files are kept: [`docs/apis/`](apis/README.md). |
| F | The same four descriptions against the final grammar | Fable (`high`) | No construct that the grammar does not have, and no rule broken, in any of the four. | — |
| F | The reading test, from the tour alone, on a description never seen | Astra, Opus | 12 of 12, and 12 of 12. Each named the two questions where the tour helped least. | Three sentences added to the tour. |
| F | How it feels, as three people (first reader, author of 300 endpoints, caller) | Astra, Sol, Opus | 8, 8, 8; 8, 7, 8; 8, 7, 8. What felt wrong, by how many named it: in `paginate` a name is a place (three); an `Option` is found by its spelling (two); an endpoint could not have its own `base` (two); a `None` in a body is not left out (one); `with_transport` replaces the author's transport (one). | `base` on an endpoint: taken (D233). The rule for a `None` in a body is now said (rule 9). The three others are kept, each a row of [chapter 8](8-scope.md), and are among the questions for the owner. |
| F | The whole paper, as the implementer of the engine, twice | Astra (`max`) | Round 1: fifteen questions, seven blocking. Round 2: "all fifteen earlier answers are now stated in the pages"; seven more, one blocking. Four of the seven were faults in code that is real (`Debug` that printed values; `link()` and a quoted `;`). | Every question has its answer in the paper (D234). The last seven were not read again: two rounds was the rule. |
| F | The whole paper, as the implementer of the macro, twice, with 18 and then more probes | Sol (`max`) | Round 1: fifteen questions, six blocking. Round 2: "all fifteen round-one answers appear in the revised pages"; four more, three blocking, each a follow-up of an answer (the signature of `.items()`; whose `Option` it is; a kept file without `move`). | The same. The last four were not read again. |
| F | Every page against every other: "what still disagrees?", twice | Fable (`high`) | Round 1: 37 places (25 facts said two ways, 8 counts). Round 2: 27 of them corrected; 24 places in all, 20 outside the README. No dead link and no example that the grammar refuses, in either round. | All corrected by the writer of each file, or by the coordinator. The pages were not read a third time. |
| F | The generated documentation of `api/tour`, as a caller, twice (two fresh readers) | Fable (`high`) | 7 of 10, then 7 of 10. The second reader's worst: `riot::Error` shows the hidden trait; the page of `Secret` showed the hidden module (fixed after); a line of statuses with no meaning beside it. | Ten rules for the macro ([the macro](macro.md) section 9). What `rustdoc` cannot hide is said there. |
| after | **The owner's four answers** (D235 to D237): the statement `middleware`; no "never sent" | — | Applied to the pages by their four writers and to `api/` by a worker (68 tests pass). | Read in the challenge run, below. |
| C | **The challenge run** (2026-11-12, D238 to D265). An attack on the rules by a reader told to break them, five times | Astra (`max`) three times, Sol (`max`) twice, in turn | 20 findings, then 15, 6, 7 and 2. The fourth and the fifth found no hole; the fifth no trap either. The worst of the first: a credential that followed a `base` to another host (the coordinator's own D233). | Every finding has its end in the sheet of the run: a rule, or a written reason. |
| C | A parser written from the grammar alone, fed with every description of the paper and with 180 attacks | Sol (`max`) | Every whole description parsed (41 roots, 289 nodes). No surprise; seven corrections of the grammar's own text. | All seven taken (D246). |
| C | For each choice, the strongest other design; four blind judges for three close pairs | Opus (twice), Astra, Sol; judges: Astra, Sol, Fable, Opus | Asked blind, four of four picked the mark for an optional input (`count?: u32`) and a paged call that is not awaited. Both outside models came to think, on their own, that writing an input twice is right. | [alternatives](alternatives.md): 31 entries. D239, D243. |
| C | An attack on secrets and origins; three pre-mortems ("a year from now this design had to change: why?") | Astra (`max`) for secrets and the engine, Sol (`max`) for the macro, Opus for the language | Seven ways in for a secret, each with a probe; five reasons for the engine. `Debug` and the JSON error paths resisted. | D241, D242, D245, D246. |
| C | Every case that the rules must decide, as tables, built five times | Fable (`high`) | 10 cells of 557 with no answer; then 9 of 737, 3 of 758, 2 of 774, none of 774. | [cases](cases.md). |
| C | Eight real APIs from the guide only; three of them written a second time by the other writer | Opus, Sol | Writing them: 7, 7, 7, 7, 7, 6, 6, 6; the second time 7, 6, 7. What they changed: a body made of entries, `idle_timeout`, a part that holds `base`, `multipart`, a secret as an entry of a body. | D247 to D250, D256, D259, D261. |
| C | The paper as the engineer who must build the engine, three times | Astra (`max`) | One blocking question of five (a stream inside `multipart` could send bytes before its error); then "yes, I could start", twice, with six and five smaller questions. | D259, D262, D264. |
| C | The paper as the engineer who must build the macro | Sol (`max`) | Four readings: two blocking questions of two; then one of four, one of four, and one of two in a reading beyond the plan. Each a new one, each answered. After the first round each was in the hidden interface between generated code and the core. **This side did not converge.** | D260, D262, D264, D265. The interface is one table of [the engine](engine.md), section 2.2. |
| C | What is real code in `api/`, read after each pass of the worker | Astra (`max`) on Sol's code | Seven readings: 9 findings, then 8, 6, 3, 4, 3, and at the last "no fault found in real code" (a round trip of `Base` held over 1.4 million generated texts). Each with a probe. The worst: a reader could put a value of the answer where Concord prints (twice); a `Base` that the URL library changed with no word. | D253, D255, D265, and the passes `api-b` to `api-j`. |
| C | Every page against every other | Fable (`high`) | 22 places; eight of them in the two pages that the coordinator writes. No dead link in 390. | All corrected. |

What the rounds show, and what they do not:

- **In the two-hour run** the findings got smaller (ten, then five and two, then two and four), and no reader asked for another structure. The owner's rule (D196) and the removal of `auth` (D207) then changed the language more than any reader did.
- **In the final run** the questions changed kind. Until then, readers were asked "is this right?". Now two were asked "could you build it?", and both said no, twice: thirteen blocking questions, then four. Each has an answer in the paper. The four of the last round, and the seven smaller ones beside them, were not read again: the plan allowed two rounds. A third round would find more; how many is not known.
- **The real APIs changed the language** more than the reviews of the surface had: parts, a pager for numbered pages, an arrow that is a `Result`, `base` on an endpoint, keys with an empty piece.
- The scores are not comparable from round to round: the readers, the questions and the text all changed. A reader of the feel in the final run gave 7 or 8 where an earlier one gave 9; the language they read had more in it, and they were asked for what felt wrong.
- **In the challenge run** the readers were asked to break it, not to judge it, and the same reading was made again after each correction. The findings fell at each turn (the table above), and they changed kind: holes of the language first, then holes of the rules that had closed them, then wordings that disagreed. What did not converge is the hidden interface between generated code and the core: in four readings the macro's implementer found a new blocking question each time (two the first time). It is the layer that no page had described before this run, and the one that 5.0 does not freeze.
- Opus and Fable are Claude, like the author of this design. Their readings are not an outside view. Sol and Astra are.
- **Every reading is a model's.** The owner has not read the result.
