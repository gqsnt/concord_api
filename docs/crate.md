# The crates: the public surface of the runtime

*Concord V5 design · reference · [index](../README.md) · the other sheets: [engineering](engineering.md), [grammar](grammar.md), [rules](rules.md), [messages](messages.md), [contracts](contracts.md) · how: [the engine](engine.md), [the macro](macro.md)*

The surface is real Rust: [`api/`](../api). Every public item is there with its signature and its documentation. It compiles, and its documentation builds:

```bash
cd api && cargo doc --offline --no-deps --features concord/tower,concord/multipart
```

That `rustdoc` is the reference. This page is the map of it. Where an older page and the code disagree, the code is right.

## The crates

People depend on one crate, `concord`. It holds no code: it re-exports three. A fifth, `concord-testkit`, is for tests only. It depends on `concord-core` and on `concord-macros` at exact versions, so that the macro and the engine that it writes for are always one release ([contracts](contracts.md) C10.9).

| Crate | What it is | What it knows |
|---|---|---|
| [`concord-core`](../api/concord-core/src) | The traits and the engine: the request, the answer, the error, retry, pages, the middleware, the transport, and Concord's own HTTP client, `Http`. How the engine runs a call is [the engine](engine.md). | No format, no scheme, no pager. It depends on `reqwest`, `http`, `bytes`, `futures-core` and `futures-util` (a body that is a stream), `tokio` (its timer only), `erased-serde`, and on `serde` for its trait only. |
| [`concord-macros`](../api/concord-macros/src) | `api!`. How it works is [the macro](macro.md). | The tree, the statements, the parts, the inputs, templates. Nothing of what stands on the right side of `body`, `read`, `paginate`, `idempotent`, `idle_timeout` and `middleware`: it copies that as written. A body made of entries is a value of the core (`Entries`) handed to a function that it does not know. It has no word for who you are. What it generates names the core only. |
| [`concord-extras`](../api/concord-extras/src) | What is shipped: three ways to write a body (`json`, `form`, and `multipart` behind the feature of that name), four readers, six pagers (`by_…`), Basic, the `tower` adapter. These are real code, and the pagers have tests. | The public traits and helpers of the core, the doors that your own crate has: `Bounded` and `WriteError` for the writers of bodies, `read::details` for the readers, `page::fragmentless` for the pagers. One exception: the `tower` adapter also uses a hidden helper of the engine for the URL of an answer, pinned to the version of the core; that helper is not a door. Each other file is also the example of how to write your own. |
| [`concord`](../api/concord/src) | The facade. | Nothing. |
| [`concord-testkit`](../api/concord-testkit/src) | For the tests of a client: `Server`, a local HTTP/1.1 server that a test scripts and that records each request; `Script`, a transport with virtual time and no socket. A dev-dependency, never a dependency of a library. | The core. |

This is the owner's rule (decision D196): the macro does not know what JSON is, or a bearer token, or an offset. So a reader, a writer or a pager of yours is not second class: it takes the same place, with the same syntax, as the ones that are shipped.

## One page

| Module of `concord` | What it holds | From the core | From the extras |
|---|---|---|---|
| `concord::secret` | Text that goes on the wire and nowhere else. | `Secret` (`new`, `expose`, `replace`) | |
| `concord::base` | Where a URL starts, when the user gives it. | `Base` (parsed from text) | |
| `concord::body` | The body of a request, as a value. | `Body` (`new`, `from`, `stream`, `content_type`, `concat`, `as_bytes`, `into_stream`); `Entries`, the value of `body json { a; b; }` (a `None` writes nothing); `Bounded` (`new`, `error`) and `WriteError`, what a writer of bodies uses to report a value it cannot write; `error` takes the `Bounded` by value, so the name of the input goes into one error only (both: "The doors", below) | `json`, `form`, `multipart` (feature `multipart`: `text`, `file`, then `finish()`, which gives a `Result` of a `Body`; its error, `MultipartError`, is `#[non_exhaustive]` and shows no type of another crate) |
| `concord::read` | How an answer becomes a value. | `Reader`, `Incoming` (`new`, for the test of a reader), `ReadError` (`not_the_type(body)`, then `at`, `with_details`, `label_was_not_expected`: the place, the details and the label are the reader's to give; a reader hands no cause of its own), `Place` (the place of a failure, made of pieces: `root`, `field`, `index`, `key`, `variant`), `ByteStream` (`try_next`), `StreamError` (what `try_next()` gives when the body fails later: its kind is Transport; `?` turns it into any `Error<E>` or a `ReadError`), `details` (a reader's own message cut to 1 KiB, with the mark ` … [cut]`, as `with_details` cuts it) | `Json`, `Text`, `Binary`, `Stream` |
| `concord::answer` | What came back. | `Answer<T>` (`status`, `headers`, `value`, `into_value`, `into_parts`; it keeps the URL it came from, hidden, for a relative next page), `Headers` (`get`, `require`, `retry_after`, `as_map`) | |
| `concord::error` | The one error of a client. | `Error<E>` (with `retry_after`), `Kind`, `Class`, `Kept`, `Fail`, `mismatch` | |
| `concord::retry` | Whether and how to try again. | `Retry` (`none`, `standard`, `tries`, `not_applied_on`, `when`, `unless`; the waits are fixed), `Tried` | |
| `concord::page` | A list that comes in several answers. | `Pages` (`try_next`, `items`), `Items` (`try_next`); both are streams, and both are over after an error. `IntoItems` (`Item`, `into_items`, `count`): what a page must be for `.items()` and the counting pagers, written for `Vec<T>`, with Concord's words when it is not met. `Call`, `Slot`, `IntoNext`: what a pager is made with; `fragmentless` (a URL without its fragment, before it is resolved or compared) | `by_offset`, `by_offset_limit`, `by_cursor`, `by_page`, `by_link`, `by_next_url` |
| `concord::transport` | What really sends a request (a transport). | `Transport`, `Http` (`new`, `with`), `TryInfo` (`endpoint`, `number`), `TransportError` (`connect`, `timeout`, `blocked`, `with_source`), `fake`, `reply`, `reply_with` | |
| `concord::middleware` | What every request goes through on its way out: a signature, a token that is renewed, a quota. | `Middleware` (`handle`), `Next` (`send`; `Next::to` for a test of one middleware) | — |
| `concord::tower` (feature `tower`) | A `tower` service as a transport. | | `Tower` |
| `concord::arg` | What an argument of a generated method may be. | `Arg` (sealed: only Concord implements it). It has a page, which says what an argument may be; its implementations stay hidden. | |
| `concord::reqwest`, `concord::http`, `concord::bytes` | The three crates that are in Concord's signatures, as Concord is built with them. | re-exported | |
| the root | | `Secret`, `Base`, `Body`, `Reader`, `ByteStream`, `Answer`, `Headers`, `Error`, `Fail`, `mismatch`, `Retry`, `Pages`, `Items`, `Transport`, `Middleware`, `Next`, `Http`, `Sending`, `fake`, `reply`, `reply_with`, `KIB`, `MIB` | `json`, `form`, `multipart` (feature `multipart`), `Json`, `Text`, `Binary`, `by_offset`, `by_offset_limit`, `by_cursor`, `by_page`, `by_link`, `by_next_url`, `basic`. (`Stream` is named by its module, `concord::read::Stream`: at the root it would meet the trait of that name.) |

The traits that a value of the author must have (`Reader`, `IntoBody`, `Slot`, `IntoNext`, `IntoItems`, `Transport`, `Middleware`, and the hidden `Value`, `InheritAcrossBase` and `InheritPathAcrossBase`) carry `#[diagnostic::on_unimplemented]`: when one is not met, `rustc` says it in Concord's words ([the macro](macro.md) section 7).

Beside those three, the core depends on `serde` (the trait only: no format), `erased-serde` (a body of entries holds values of several types), `futures-core` and `futures-util` (a body that is a stream) and `tokio` (its timer only). None of them is in a signature, so code may change them.

What generated code calls is `concord::__private`: hidden, and with no promise of stability. How a call is sent through it (`prepare`, `Prepared::send`, `Prepared::send_raw`, `__prepare_if_none_match`, `answered_from`, `call_methods!`, `pages`) is one table of [the engine](engine.md), section 2.2. Its other main pieces: `Core` and `Options` (what a client and a call hold); `Draft` (a request being built), which takes each value with its name, `Piece::Val("region", &value)`; one adapter that every placed value goes through, which knows `Secret` and `Option<Secret>` before it asks for `Serialize`; `InheritAcrossBase` and `InheritPathAcrossBase`, two traits that the secret side of the adapter does not have, so that a secret, in an entry or in a piece of a `path`, stays with the `base` in reach where it was written ([contracts](contracts.md) C8.9); `Run`, `exchange`, `exchange_raw` and `again` (the clocks and the tries of one send, in one loop, and the decision to try again); `no_arm` (the Mismatch for a 2xx that a `HEAD` takes in no arm, with its evidence); and functions such as `reader`, whose bound checks an expression of the author, and which carries the type that is read (the names of the others are the implementer's). How they run: [the engine](engine.md); how the macro uses them: [the macro](macro.md). Generated code names an outside crate only through `concord`, so a crate that holds a description needs `concord` and `serde`, and nothing else.

A description needs an ordinary `use` line for what its right sides name: `use concord::{json, Json, by_offset_limit};`. A function of yours with the same name as a shipped one wins, by Rust's own rules; and an input hides an item of its name in the lines of its node, which then writes the item with its path (`concord::form(form)`). The `f` of `body f { … }` is the one exception: it is a path, seen from where `api!` stands, and no input hides it.

## What a generated client has

Written by the macro in [`api/tour-gen/src/`](../api/tour-gen/src) from five descriptions, and by hand, as the macro writes them, in [`api/tour/src/`](../api/tour/src): the clients of [the tour](tour.md) (`Tasks`, `Riot`, `Files`, `Feed`), and `Shapes`, which has the shapes that the tests of a call need.

| On | Methods |
|---|---|
| A client | `new(required inputs)`; one method for each optional input, scope and endpoint; `with_retry`, `with_timeout`, `with_idle_timeout`, `with_base`, `with_transport`, `with_middleware` |
| The module of a client (`riot`) | `riot::Error` (`concord::Error<RiotProblem>`), `riot::Result<T>`, `riot::Retry` (`concord::Retry<RiotProblem>`) |
| A handle (a scope) | One method for each optional input, scope and endpoint |
| A call | One method for each optional input. `.await` and `send()` (with `paginate`: `send()` only, for one page); `send_full()`; `send_raw()`; `send_if_none_match(etag)` on a `GET` or a `HEAD`; `request()`, `into_request()`; `edit_request(…)`; `at_url(url)` (through `concord::page::Call`); `read_response(response)`; `with_retry`, `with_timeout`, `with_idle_timeout`; `pages()` when the endpoint has `paginate`, and then `.pages().items()` |

Concord adds no name to a call, a handle or a client in a minor version, unless it starts with `with_` ([contracts](contracts.md) C1.6). A generated type is defined at its place in the tree and has no other name: `riot::Matches`, `riot::matches::Ids`. A type of the author reaches it through one hidden trait for each client ([the macro](macro.md) section 3). `send()`, `send_full()` and `send_raw()` give a `concord::Sending<T, E>`: a future with a name, so the documentation of a client reads plainly.

## The doors, each with real code to copy

| To write your own | The shape | Where the code is |
|---|---|---|
| Body format | A function that gives a `Body` (or a `Result` of one). If it takes something that `serde` can write, it also takes a body made of entries: `body xml { a; b; }`. To report a value that cannot be written as `input <name> cannot be written in a body`, it uses what `json` and `form` use: `concord::body::Bounded`. It wraps the value before the format's serializer sees it (`Bounded::new(&value)`, at most 128 nested levels for each entry) and turns the format's error into a `concord::body::WriteError` (`value.error(e)`, which takes the `Bounded`: once). `json()` and `form()` give that one error type: its text is fixed (`the body could not be written`), and its source is the format's error. A body made of other bodies is `Body::concat(parts)`; `as_bytes()` and `into_stream()` look inside one | [`concord-extras/src/body.rs`](../api/concord-extras/src/body.rs): `json` is three lines; [`multipart.rs`](../api/concord-extras/src/multipart.rs): a body made of parts |
| Reader | A type with `Reader<T>`: `async fn read(&self, body: Incoming)`. A message of its own (what the decoder said) can be cut to 1 KiB with `concord::read::details`, as `Json` cuts it (`ReadError::with_details` cuts it the same way) | [`concord-extras/src/read.rs`](../api/concord-extras/src/read.rs): `Json`, `Text` |
| Pager | A function from the call and its answer to the next call; `Slot` for an input, `Call::at_url` for an address, `concord::page::fragmentless` to take the fragment off a next URL; `concord::Fail` for its error | [`concord-extras/src/page.rs`](../api/concord-extras/src/page.rs), with a pager "of your own" in its tests |
| A signature, a token that is renewed, a quota | A middleware: a type with `Middleware`, `async fn handle(&self, request, info, next)`. The author writes `middleware Signed::new(key.clone());` in the description, the one that signs last; a caller adds one with `with_middleware(…)`, and it runs before the author's. | [Contracts](contracts.md) C1.19 and C9.6; chapter 6 |
| A list type of yours as a page | `impl IntoItems for List<T>`: `into_items` and `count`, a few lines. Then `.pages().items()` and the counting pagers work on it | [Contracts](contracts.md) C8.7 |
| Transport, what really sends | A type with `Transport`: `async fn send(&self, request, info)`, set with `with_transport(…)`, under the middlewares. It must not change what a middleware signed. | The doc of `concord::transport::Transport` |
| A setting of the HTTP client (a certificate, a proxy) | `concord::Http::with(\|builder\| …)`, given to `with_transport` | [The engine](engine.md), section 6 |
| Stack of `tower` layers under a client | `concord::tower::Tower::new(stack)` | [`concord-extras/src/tower.rs`](../api/concord-extras/src/tower.rs) |
| Method on a generated type | An `impl` beside the macro; a trait of yours from another crate | Chapter 4, section 4.7 |
