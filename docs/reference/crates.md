# The crates: the public surface

*[Concord](../../README.md) · [docs](../README.md) · reference · previous: [the messages](messages.md) · next: [the contracts](contracts.md)*

This page is a map of the public items: which crate holds each one, under which path, and what a generated client has. The documentation of each item, built by `rustdoc`, is the reference:

```bash
cargo doc --no-deps --features concord-api/tower,concord-api/multipart --open
```

## The crates

You depend on one crate: the package `concord-api`, whose library is named `concord` (`concord-api = "5.0.0-rc.1"` in `Cargo.toml`, `concord::api!` in code; the name `concord` is taken on crates.io by another project). It holds no code of its own: it re-exports three crates. A fifth, `concord-testkit`, is for tests only. `concord-api` depends on `concord-core` and `concord-macros` at exact versions, so that the macro and the engine it generates code for always come from the same release ([contracts](contracts.md) C10.9). The crates are not on crates.io yet.

| Crate | What it is | What it knows |
|---|---|---|
| [`concord-core`](../../concord-core/src) | The traits and the engine: the request, the response, the error, retry, pages, the middleware, the transport, and Concord's own HTTP client, `Http`. How the engine runs a call is described in [the engine](../internals/engine.md). | No format, no authentication scheme, no pager. It depends on `reqwest`, `http`, `bytes`, `hyper` (to tell the failures of the HTTP library apart), `futures-core` and `futures-util` (a body that is a stream), `tokio` (its timer only), `tracing` (the one log event), `erased-serde`, and on `serde` for its trait only. |
| [`concord-macros`](../../concord-macros/src) | `api!`. How it works is described in [the macro](../internals/macro.md). | The tree, the statements, the parts, the inputs, templates. Nothing of what follows `body`, `read`, `paginate`, `idempotent`, `idle_timeout` and `middleware`: it copies that code as written. A body made of entries is a value of the core (`Entries`), passed to a function that the macro does not know. It has no keyword for authentication. The code it generates names the core only. |
| [`concord-extras`](../../concord-extras/src) | What Concord ships: three ways to write a body (`json`, `form`, and `multipart` behind the feature of that name), four readers, six pagers (`by_…`), Basic authentication, the `tower` adapter. | Only the public traits and helpers of the core, the same extension points that your own crate has: `Bounded` and `WriteError` for body writers, `read::details` for readers, `page::fragmentless` for pagers. One exception: the `tower` adapter also uses a hidden helper of the engine for the URL of a response, pinned to the version of the core; that helper is not a public extension point. Every other file also shows how to write your own. |
| [`concord-api`](../../concord-api/src), library `concord` | The facade. | Nothing. |
| [`concord-testkit`](../../concord-testkit/src) | For testing a client: `Server`, a local HTTP/1.1 server that a test scripts and that records each request; `Script`, a transport with virtual time and no socket. A dev-dependency, never a dependency of a library. | The core. |

The rule is that the macro does not know what JSON is, or a bearer token, or an offset. So your own reader, writer or pager is not second class: it fits in the same place, with the same syntax, as the shipped ones.

## Every public item, by module

| Module of `concord` | What it holds | From the core | From the extras |
|---|---|---|---|
| `concord::secret` | Text that goes on the wire and nowhere else. | `Secret` (`new`, `expose`, `replace`) | |
| `concord::base` | Where a URL starts, when the caller provides it. | `Base` (parsed from text) | |
| `concord::body` | The body of a request, as a value. | `Body` (`new`, `from`, `stream`, `content_type`, `concat`, `as_bytes`, `into_stream`); `Entries`, the value of `body json { a; b; }` (a `None` writes nothing); `Bounded` (`new`, `error`) and `WriteError`, which a body writer uses to report a value it cannot write; `error` takes the `Bounded` by value, so the name of the input goes into one error only ([a writer of bodies](../guide/extending.md#a-writer-of-bodies)) | `json`, `form`, `multipart` (feature `multipart`: `text`, `file`, then `finish()`, which returns a `Result` of a `Body`; its error, `MultipartError`, is `#[non_exhaustive]` and exposes no type of another crate) |
| `concord::read` | How a response becomes a value. | `Reader`, `Incoming` (`new`, to test a reader), `ReadError` (`not_the_type(body)`, then `at`, `with_details`, `label_was_not_expected`: the reader provides the location, the details and whether the content type was unexpected; a reader passes on no cause of its own), `Place` (the location of a failure, made of components: `root`, `field`, `index`, `key`, `variant`), `ByteStream` (`try_next`), `StreamError` (what `try_next()` returns when the body fails later: its kind is Transport, and `?` converts it into any `Error<E>` or a `ReadError`), `details` (a reader's own message cut to 1 KiB, with the mark ` … [cut]`, as `with_details` cuts it) | `Json`, `Text`, `Binary`, `Stream` |
| `concord::answer` | A value with the status and headers of its response. | `Answer<T>` (`status`, `headers`, `value`, `into_value`, `into_parts`; it keeps the URL it came from, hidden, for a relative next page), `Headers` (`get`, `require`, `retry_after`, `as_map`) | |
| `concord::error` | The one error type of a client. | `Error<E>` (with `retry_after`), `Kind`, `Class`, `Kept`, `Fail`, `mismatch` | |
| `concord::retry` | Whether and how to retry. | `Retry` (`none`, `standard`, `tries`, `not_applied_on`, `when`, `unless`; the waits are fixed), `Tried` | |
| `concord::page` | A list that comes in several responses. | `Pages` (`try_next`, `items`), `Items` (`try_next`); both are streams, and both end after an error. `IntoItems` (`Item`, `into_items`, `count`): what a page type must implement for `.items()` and the counting pagers, implemented for `Vec<T>`, with Concord's own message when it is missing. `Call`, `Slot`, `IntoNext`: what a pager is built with; `fragmentless` (a URL without its fragment, before it is resolved or compared) | `by_offset`, `by_offset_limit`, `by_cursor`, `by_page`, `by_link`, `by_next_url` |
| `concord::transport` | The transport: what actually sends a request. | `Transport`, `Http` (`new`, `with`), `TryInfo` (`endpoint`, `number`), `TransportError` (`connect`, `timeout`, `blocked`, `with_source`), `fake`, `reply`, `reply_with` | |
| `concord::middleware` | What every request passes through on its way out: a signature, a token that is renewed, a quota. | `Middleware` (`handle`), `Next` (`send`; `Next::to` to test one middleware) | — |
| `concord::tower` (feature `tower`) | A `tower` service as a transport. | | `Tower` |
| `concord::arg` | What an argument of a generated method may be. | `Arg` (sealed: only Concord implements it). It has a documentation page, which says what an argument may be; its implementations stay hidden. | |
| `concord::reqwest`, `concord::http`, `concord::bytes` | The three crates that appear in Concord's signatures, in the versions Concord is built with. | re-exported | |
| the root | `api!`, from `concord-macros`, and the following. | `Secret`, `Base`, `Body`, `Reader`, `ByteStream`, `Answer`, `Headers`, `Error`, `Fail`, `mismatch`, `Retry`, `Pages`, `Items`, `Transport`, `Middleware`, `Next`, `Http`, `Sending`, `fake`, `reply`, `reply_with`, `KIB`, `MIB` | `json`, `form`, `multipart`, `Multipart` and `MultipartError` (feature `multipart`), `Json`, `Text`, `Binary`, `by_offset`, `by_offset_limit`, `by_cursor`, `by_page`, `by_link`, `by_next_url`, `basic`. (`Stream` is named through its module, `concord::read::Stream`: at the root it would clash with the trait of that name.) |

The traits that an author's value must implement (`Reader`, `IntoBody`, `Slot`, `IntoNext`, `IntoItems`, `Transport`, `Middleware`, and the hidden `Value`, `InheritAcrossBase` and `InheritPathAcrossBase`) carry `#[diagnostic::on_unimplemented]`: when one is not implemented, `rustc` reports it in Concord's words (the macro, [The messages that are `rustc`'s](../internals/macro.md#the-messages-that-are-rustcs)).

Besides those three, the core depends on `serde` (the trait only: no format), `erased-serde` (a body of entries holds values of several types), `futures-core` and `futures-util` (a body that is a stream), `tokio` (its timer only), `hyper` (to tell the failures of the HTTP library apart) and `tracing` (the one log event). None of them appears in a public signature, so Concord can replace them without breaking your code.

Generated code calls `concord::__private`, which is hidden and has no promise of stability ([the hidden interface](../internals/engine.md#the-hidden-interface-method-by-method), in the engine). Generated code names an outside crate only through `concord`, so a crate that holds a description needs `concord` and `serde`, and nothing else.

A description needs an ordinary `use` line for what its right-hand sides name: `use concord::{json, Json, by_offset_limit};`. Your own function with the same name as a shipped item wins, by Rust's own rules. An input shadows an item with the same name in the statements of its node, which must then refer to the item by its path (`concord::form(form)`). The `f` of `body f { … }` is the one exception: it is a path, resolved from where `api!` is invoked, and no input shadows it.

## What a generated client has

Five test clients show it: `Tasks` and `Riot` from [the tour](../guide/tour.md), and `Files`, `Feed` and `Shapes`, which have the shapes that the tests of a call need. The macro generates them in [`tests/tour-gen/src/`](../../tests/tour-gen/src) from their descriptions; [`tests/tour/src/`](../../tests/tour/src) holds the same clients written by hand, exactly as the macro generates them.

| On | Methods |
|---|---|
| A client | `new(required inputs)`; one method for each optional input, scope and endpoint; `with_retry`, `with_timeout`, `with_idle_timeout`, `with_base`, `with_transport`, `with_middleware` |
| The module of a client (`riot`) | `riot::Error` (`concord::Error<RiotProblem>`), `riot::Result<T>`, `riot::Retry` (`concord::Retry<RiotProblem>`) |
| A handle (a scope) | One method for each optional input, scope and endpoint |
| A call | One method for each optional input. `.await` and `send()` (with `paginate`: `send()` only, for one page); `send_full()`; `send_raw()`; `send_if_none_match(etag)` on a `GET` or a `HEAD`; `request()`, `into_request()`; `edit_request(…)`; `at_url(url)` (through `concord::page::Call`); `read_response(response)`; `with_retry`, `with_timeout`, `with_idle_timeout`; `pages()` when the endpoint has `paginate`, and then `.pages().items()` |

A minor version adds no name to a call, a handle or a client unless the name starts with `with_` ([contracts](contracts.md) C1.6). A generated type is defined at its position in the tree and has no other name: `riot::Matches`, `riot::matches::Ids`. An author's type reaches it through one hidden trait for each client (the macro, [Where the generated items live, and how they reach the author's types](../internals/macro.md#where-the-generated-items-live-and-how-they-reach-the-authors-types)). `send()`, `send_full()` and `send_raw()` return a `concord::Sending<T, E>`: a named future, so that the documentation of a client reads plainly.

## Concord's own HTTP client

`concord::Http` is the transport of a client that was given none: a `reqwest::Client` that Concord builds, wrapped in a type of its own. `Http::with(f)` takes your settings; `Http::new()` is `Http::with(|b| b)`.

```rust
let http = concord::Http::with(|builder| builder.add_root_certificate(cert))?;
let riot = Riot::new(key).with_transport(http);
```

| Step of `Http::with(f)` | Setting |
|---|---|
| Before `f`: defaults, which `f` may change | 30 seconds to connect; no `Referer`; no cookies and no decompression (every decoder is turned off, whatever features the build of `reqwest` has: a body is passed on as it arrived). The proxy settings of the environment are read, as `reqwest` does by default. An `f` that turns decompression on changes what a reader receives and what its limit counts: the decoded bytes that the transport yields. |
| After `f`: the one setting that a promise needs, which `f` cannot change | No redirect is followed: a `3xx` is a response, and the HTTP library never carries a credential elsewhere. |

Both functions return a `Result`, whose error is `reqwest`'s. A `reqwest::Client` that you pass to `with_transport` keeps its own settings ([rule 22](rules.md#the-call-and-the-client)). See [C7.1](contracts.md#c7-concords-own-client-and-the-timeouts) for the exact contract.

### The features of `concord-api`

| Feature | What it turns on | Default |
|---|---|---|
| `json` | `json(x)` to write a body, `Json` to read one | on |
| `form` | `form(x)` to write a form body | on |
| `rustls` | TLS for `concord::Http`, through `reqwest/rustls` | on |
| `http2` | HTTP/2 for `concord::Http`, through `reqwest/http2` | on |
| `system-proxy` | The proxy settings of the system (macOS, Windows), as `reqwest` reads them, through `reqwest/system-proxy` | on |
| `gzip` | The gzip decoder of `reqwest` (`reqwest/gzip`), for `builder.gzip(true)` in `Http::with` | off |
| `deflate` | The deflate decoder of `reqwest` (`reqwest/deflate`), for `builder.deflate(true)` in `Http::with` | off |
| `socks` | SOCKS proxies, configured with `reqwest::Proxy` (also reachable as `concord::reqwest::Proxy`) in `Http::with` (`reqwest/socks`) | off |
| `tower` | A `tower` service as a transport: `concord::tower::Tower` | off |
| `multipart` | `multipart()` to write a `multipart/form-data` body | off |

`gzip`, `deflate` and `socks` only make a capability of `reqwest` available. `concord::Http` still decompresses nothing until `Http::with` turns decoding on, and with decompression on, a reader's limit counts the decoded bytes. `socks` sets no proxy by itself: a SOCKS proxy comes from `Http::with`, or from the proxy settings of the environment, which `concord::Http` reads as `reqwest` does; Concord's tests cover only the first. Turning the default features off (`default-features = false`) also turns off `json`, `form`, `rustls`, `http2` and `system-proxy`: list again the ones that you need. `brotli`, `zstd`, cookies and `native-tls` are not forwarded yet: until they are, add `reqwest` to your own `Cargo.toml` with that feature, or bring your own `reqwest::Client` with `with_transport` ([compression, proxies and TLS](../guide/calling.md#compression-proxies-and-tls)).

## Replacing a shipped item with your own

A body format, a reader, a pager, a middleware, a list type as a page, a transport, a setting of the HTTP client, a stack of `tower` layers, a method on a generated type: each one that you write fits in the same place as the shipped items. See [When Concord does not say what you need](../guide/extending.md) for how to write each, with real code to copy.
