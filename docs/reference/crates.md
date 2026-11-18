# The crates: the public surface

*[Concord](../../README.md) · [docs](../README.md) · reference · previous: [the messages](messages.md) · next: [the contracts](contracts.md)*

This page is a map of the public items: which crate holds each one, under which path, and what a generated client has. The documentation of each item, built by `rustdoc`, is the reference:

```bash
cargo doc --no-deps --features concord-api/tower,concord-api/multipart,concord-api/gzip-body,concord-api/auth,concord-api/rotating,concord-api/sign,concord-api/limit --open
```

## The crates

You depend on one crate: the package `concord-api`, whose library is named `concord` (`concord-api = "5.0.0-rc.1"` in `Cargo.toml`, `concord::api!` in code; the name `concord` is taken on crates.io by another project). It holds no code of its own: it re-exports three crates. A fifth, `concord-testkit`, is for tests only. `concord-api` depends on `concord-core` and `concord-macros` at exact versions, so that the macro and the engine it generates code for always come from the same release ([contracts](contracts.md) C10.9). The crates are not on crates.io yet.

| Crate | What it is | What it knows |
|---|---|---|
| [`concord-core`](../../concord-core/src) | The traits and the engine: the request, the response, the error, retry, pages, the middleware, the transport, and Concord's own HTTP client, `Http`. How the engine runs a call is described in [the engine](../internals/engine.md). | No format, no authentication scheme, no pager. It depends on `reqwest`, `http`, `bytes`, `hyper` (to tell the failures of the HTTP library apart), `futures-core` and `futures-util` (a body that is a stream), `tokio` (its timer only), `tracing` (the one log event), `erased-serde`, and on `serde` for its trait only. |
| [`concord-macros`](../../concord-macros/src) | `api!`. How it works is described in [the macro](../internals/macro.md). | The tree, the statements, the parts, the inputs, templates. Nothing of what follows `body`, `read`, `paginate`, `idempotent`, `idle_timeout` and `middleware`: it copies that code as written. A body made of entries is a value of the core (`Entries`), passed to a function that the macro does not know. It has no keyword for authentication. The code it generates names the core only. |
| [`concord-extras`](../../concord-extras/src) | What Concord ships: body writers (`json` and `json_lines` under `json`; `form`, `bracket_form` and `bracket_pairs` under `form`; the three multipart writers under `multipart`; `gzip` under `gzip-body`), six readers (four that read a body, and `JsonLines` and `Sse` for records), six pagers (`by_…`), Basic authentication, four middleware behind their features (a token that is renewed, `auth`; a refresh token that rotates, `rotating`; AWS Signature Version 4, `sign`; rate limits, `limit`), the check of a URL that the API returned (`transfer`), and the `tower` adapter. | Only the public traits and helpers of the core, the same extension points that your own crate has: `Bounded` and `WriteError` for body writers, `read::details` for readers, `page::fragmentless` for pagers, `Middleware` for `auth`, `sign` and `limit`, `page::Call` for `transfer`. Only the `tower` adapter uses a hidden helper of the core, for the response URL. That helper is pinned to the core version and is not a public extension point. Wrapping writers use the public `Body::get_content_type`, `Body::map_stream` and `Body::is_repeatable` methods. Every other file also shows how to write your own. |
| [`concord-api`](../../concord-api/src), library `concord` | The facade. | Nothing. |
| [`concord-testkit`](../../concord-testkit/src) | For testing a client: `Server`, a local HTTP/1.1 server that a test scripts and that records each request; `Script`, a transport with virtual time and no socket. A dev-dependency, never a dependency of a library. | The core. |

The rule is that the macro does not know what JSON is, or a bearer token, or an offset. So your own reader, writer or pager is not second class: it fits in the same place, with the same syntax, as the shipped ones.

## Every public item, by module

| Module of `concord` | What it holds | From the core | From the extras |
|---|---|---|---|
| `concord::secret` | Text that goes on the wire and nowhere else. | `Secret` (`new`, `expose`, `replace`) | |
| `concord::base` | Where a URL starts, when the caller provides it. | `Base` (parsed from text) | |
| `concord::body` | The body of a request, as a value. | `Body` (`new`, `from`, `stream`, `repeatable`, `content_type`, `get_content_type`, `is_repeatable`, `map_stream`, `concat`, `as_bytes`, `into_stream`); `Entries`, the value of `body json { a; b; }` (a `None` writes nothing); `Bounded` (`new`, `error`) and `WriteError`, which a body writer uses to report a value it cannot write; `error` takes the `Bounded` by value, so the name of the input goes into one error only ([a writer of bodies](../guide/extending.md#a-writer-of-bodies)) | `json`, `json_lines`, `form`, `bracket_form`, `bracket_pairs`, `gzip` (feature `gzip-body`), `multipart_mixed`, `multipart_related` and `multipart` (feature `multipart`: `text`, `file`, then `finish()`, which returns a `Result` of a `Body`; its error, `MultipartError`, is `#[non_exhaustive]` and exposes no type of another crate) |
| `concord::read` | How a response becomes a value. | `Reader`, `Incoming` (`new`, to test a reader), `ReadError` (`not_the_type(body)`, then `at`, `with_details`, `label_was_not_expected`: the reader provides the location, the details and whether the content type was unexpected; a reader passes on no cause of its own), `Place` (the location of a failure, made of components: `root`, `field`, `index`, `key`, `variant`), `ByteStream` (`try_next`), `StreamError` (what `try_next()` returns when the body fails later: its kind is Transport, and `?` converts it into any `Error<E>` or a `ReadError`), `details` (a reader's own message cut to 1 KiB, with the mark ` … [cut]`, as `with_details` cuts it) | `Json` (`limit`, `strict`; `decode`, which decodes bytes already in memory with the reader's error, for an arm that decodes a body itself), `Text`, `Binary`, `Stream`; `JsonLines` (feature `json`) and `Sse`, which read records into a `RecordStream` (`try_next`, `status`, `headers`; a stream), with `SseEvent` and `RecordError` |
| `concord::answer` | A value with the status and headers of its response. | `Answer<T>` (`status`, `headers`, `value`, `into_value`, `into_parts`; it keeps the URL it came from, hidden, for a relative next page), `Headers` (`get`, `require`, `retry_after`, `as_map`) | |
| `concord::error` | The one error type of a client. | `Error<E>` (with `retry_after`), `Kind`, `Class`, `Kept`, `Fail` (`from_read`, which makes a decoder's `ReadError` the failure of an arm or a pager), `mismatch` | |
| `concord::retry` | Whether and how to retry. | `Retry` (`none`, `standard`, `tries`, `not_applied_on`, `when`, `unless`; the waits are fixed), `Tried` | |
| `concord::page` | A list that comes in several responses. | `Pages` (`try_next`, `items`), `Items` (`try_next`); both are streams, and both end after an error. `IntoItems` (`Item`, `into_items`, `count`): what a page type must implement for `.items()` and the counting pagers, implemented for `Vec<T>`, with Concord's own message when it is missing. `Call`, `Slot`, `IntoNext`: what a pager is built with; `fragmentless` (a URL without its fragment, before it is resolved or compared) | `by_offset`, `by_offset_limit`, `by_cursor`, `by_page`, `by_link`, `by_next_url` |
| `concord::transport` | The transport: what actually sends a request. | `Transport`, `Http` (`new`, `with`), `TryInfo` (`endpoint`, `number`), `TransportError` (`connect`, `timeout`, `blocked`, `with_source`), `fake`, `reply`, `reply_with` | |
| `concord::middleware` | What every request passes through on its way out: a signature, a token that is renewed, a quota. | `Middleware` (`handle`), `Next` (`send`; `Next::to` to test one middleware) | — |
| `concord::auth` (feature `auth`) | A token that expires, shared by the calls to one origin; with the feature `rotating`, a refresh token that the server replaces at each exchange. | | `Renewed` (`new`, `early_by`, `renewal_timeout`, `with_token`, `on_renewed`, `header`, `rejected_when`), `Seen` (`status`, `headers`, `body`), `AccessToken` (`expires_in`, `value`, `remaining`), `RenewalError` (`failed`, `not_sent`, `from_call`, `find`), `after_401`; with `rotating`: `Rotating` (`new`, `early_by`, `renewal_timeout`, `header`, `rejected_when`), `RotationDriver` (a future that the application polls), `CredentialStore` (`load`, `save`), `Stored` (`Ready`, `Refreshing`), `TokenPair` (`new`, `expires_in`, `access`, `refresh`, `expires_at`), `Exchange` (`refresh`), `RotationError` (`find`) |
| `concord::sign` (feature `sign`) | Signing a request. | | `SigV4` (`new`, `allow_unsigned_payload`, `sign`, `sign_any_origin`; a middleware), `Credentials` (`new`, `session_token`, `replace`), `CanonicalPath` (`Preserve`, `NormalizeAndEncode`), `SignError` (`failed`, `find`), `body_bytes`, `check_origin` |
| `concord::limit` (feature `limit`) | Rate limits. | | `Limit` (`new`, `without_retry_after`, `max_in_flight`, `max_hold`, `max_keys`, `bucket`), `Bucket` (`new`, `window`, `learn`, `cost`, `hit_when`), `Window` (`rolling`, `fixed`, `remaining`), `RequestInfo`, `Seen`, `LimitError` (`find`), and the readers of headers `counts` and `remaining` |
| `concord::transfer` | A call sent to a URL that the API returned. | | `at`, `TransferError` |
| `concord::tower` (feature `tower`) | A `tower` service as a transport. | | `Tower` |
| `concord::arg` | What an argument of a generated method may be. | `Arg` (sealed: only Concord implements it). It has a documentation page, which says what an argument may be; its implementations stay hidden. | |
| `concord::reqwest`, `concord::http`, `concord::bytes` | Three of the crates that appear in Concord's signatures, re-exported in the versions Concord is built with: `reqwest` 0.13, `http` 1, `bytes` 1. `concord::reqwest::Url` is the `Url` of the `url` crate, version 2: the type of `url()` on a request, and on a request that `concord-testkit` records (`script::Request::url`), so the testkit is tied to these versions as well. Every crate of the signatures is in [what the public surface is tied to](#what-the-public-surface-is-tied-to). | re-exported | |
| the root | `api!`, from `concord-macros`, and the following. | `Secret`, `Base`, `Body`, `Reader`, `ByteStream`, `Answer`, `Headers`, `Error`, `Fail`, `mismatch`, `Retry`, `Pages`, `Items`, `Transport`, `Middleware`, `Next`, `Http`, `Sending`, `fake`, `reply`, `reply_with`, `KIB`, `MIB` | `json`, `json_lines`, `JsonLinesError` and `Json` (feature `json`), `form`, `bracket_form`, `bracket_pairs` and `BracketFormError` (feature `form`), `gzip` and `GzipError` (feature `gzip-body`), `multipart_mixed`, `multipart_related`, `MimeWriteError`, `multipart`, `Multipart` and `MultipartError` (feature `multipart`), `Text`, `Binary`, `by_offset`, `by_offset_limit`, `by_cursor`, `by_page`, `by_link`, `by_next_url`, `basic`. (`Stream` is named through its module, `concord::read::Stream`: at the root it would clash with the trait of that name.) |

The traits that an author's value must implement (`Reader`, `IntoBody`, `Slot`, `IntoNext`, `IntoItems`, `Transport`, `Middleware`, and the hidden `Value`, `InheritAcrossBase` and `InheritPathAcrossBase`) carry `#[diagnostic::on_unimplemented]`: when one is not implemented, `rustc` reports it in Concord's words (the macro, [The messages that are `rustc`'s](../internals/macro.md#the-messages-that-are-rustcs)).

Besides those three, the core depends on `serde` (the trait only: no format), `erased-serde` (a body of entries holds values of several types), `futures-core` and `futures-util` (a body that is a stream), `tokio` (its timer only), `hyper` (to tell the failures of the HTTP library apart) and `tracing` (the one log event). The features of `concord-extras` add their own: `auth` uses `tokio` (its timer) and `futures-util` (the renewal is one shared future); `rotating` adds the `sync` feature of `tokio` (the calls that wait for a renewal); `sign` uses `hmac`, `sha2` and `hex` (the signature), `time` (the date that is signed) and `tokio` (its clock, so that a signature follows virtual time in tests); `limit` uses `tokio` (its timer, and its `sync` feature to wake the calls that wait) and `http-body` (the body that holds the permit of `max_in_flight`); `gzip-body` uses `flate2`, with its Rust backend; `form` uses `serde_urlencoded`, `json` uses `serde_json` and `serde_path_to_error`, and `multipart` uses `getrandom` (the boundary of a body with a stream). Four of them are part of the public surface: `serde` and `futures-core` appear in public signatures, and the errors of `serde_json` and `serde_urlencoded` can be reached by a downcast of a writer's error ([what the public surface is tied to](#what-the-public-surface-is-tied-to)). None of the others appears in a public signature of `concord`, so Concord can replace them without breaking your code; `tokio` is the exception, because a call is sent on a Tokio runtime.

Generated code calls `concord::__private`, which is hidden and has no promise of stability ([the hidden interface](../internals/engine.md#the-hidden-interface-method-by-method), in the engine). Generated code names an outside crate only through `concord`, so a crate that holds a description needs `concord` and `serde`, and nothing else.

A description needs an ordinary `use` line for what its right-hand sides name: `use concord::{json, Json, by_offset_limit};`. Your own function with the same name as a shipped item wins, by Rust's own rules. An input shadows an item with the same name in the statements of its node, which must then refer to the item by its path (`concord::form(form)`). The `f` of `body f { … }` is the one exception: it is a path, resolved from where `api!` is invoked, and no input shadows it.

## What a generated client has

Five test clients show it: `Tasks` and `Riot` from [the tour](../guide/tour.md), and `Files`, `Feed` and `Shapes`, which have the shapes that the tests of a call need. The macro generates them in [`tests/tour-gen/src/`](../../tests/tour-gen/src) from their descriptions; [`tests/tour/src/`](../../tests/tour/src) holds the same clients written by hand, with the public types and methods the macro generates.

| On | Methods |
|---|---|
| A client | `new(required inputs)`; one method for each optional input, scope and endpoint; `with_retry`, `with_timeout`, `with_idle_timeout`, `with_base`, `with_transport`, `with_middleware` |
| The module of a client (`riot`) | `riot::Call<E>`, the sealed trait `riot::Endpoint` with `Value` and `NAME`, `riot::Error` (`concord::Error<RiotProblem>`), `riot::Result<T>`, `riot::Retry` (`concord::Retry<RiotProblem>`) |
| A handle (a scope) | One method for each optional input, scope and endpoint |
| An endpoint type (`riot::matches::Ids`) | An associated function for each option. Its page documents the endpoint. |
| A call (`riot::Call<riot::matches::Ids>`) | One method for each optional input. `.await` and `send()` (with `paginate`: `send()` only, for one page); `send_full()`; `send_raw()`; `send_if_none_match(etag)` on a `GET` or a `HEAD`; `request()`, `into_request()`; `edit_request(…)`; `at_url(url)` (through `concord::page::Call`); `read_response(response)`; `with_retry`, `with_timeout`, `with_idle_timeout`; `pages()` when the endpoint has `paginate`, and then `.pages().items()` |

A minor version adds no name to a call, a handle or a client unless the name starts with `with_` ([contracts](contracts.md) C1.6). A handle or an endpoint type is defined at its position in the tree and has no other name: `riot::Matches`, `riot::matches::Ids`. Every endpoint method returns the client's `Call<E>`, with the endpoint's type as `E`. An author's type reaches the generated items through one hidden trait for each client (the macro, [Where the generated items live, and how they reach the author's types](../internals/macro.md#where-the-generated-items-live-and-how-they-reach-the-authors-types)). `send()`, `send_full()` and `send_raw()` return a `concord::Sending<T, E>`: a named future, so that the documentation of a client reads plainly.

## Concord's own HTTP client

`concord::Http` is the transport of a client that was given none: a `reqwest::Client` that Concord builds, wrapped in a type of its own. `Http::with(f)` takes your settings; `Http::new()` is `Http::with(|b| b)`.

```rust
let http = concord::Http::with(|builder| builder.add_root_certificate(cert))?;
let riot = Riot::new(key).with_transport(http);
```

| Step of `Http::with(f)` | Setting |
|---|---|
| Before `f`: defaults, which `f` may change | 30 seconds to connect; no `Referer`; no decompression (every decoder is turned off, whatever features the build of `reqwest` has: a body is passed on as it arrived). An `f` that turns decompression on changes what a reader receives and what its limit counts: the decoded bytes that the transport yields. |
| Not set by Concord: the defaults of `reqwest`, which `f` may change | No cookie store. Proxies from the environment, and from the system's settings on macOS and Windows (with `system-proxy`). Idle connections are pooled with an idle timeout of 90 seconds, and no cap on idle connections for one host. `accept: */*` on a request that has no `accept` header, and no default `User-Agent`. No timeout for a whole request and none for a read: besides the 30 seconds to connect, the timeouts of a call are Concord's own clocks. TCP keepalive after 15 seconds, then every 15 seconds, three probes; on Linux, Android and Fuchsia, a TCP user timeout of 30 seconds. |
| Not set by Concord, and not a setting of `reqwest`'s builder | The connection pool under `reqwest` (`hyper-util`) sends a request again, on another connection, when a reused connection proved unusable and handed the request back before writing any of it. This covers a request with a streamed body too, has no fixed count, and does not follow `builder.retry(…)`. It never puts a request on the wire twice. |
| After `f`: the two settings that the promises need, which `f` cannot change | No redirect is followed: a `3xx` is a response, and the HTTP library never carries a credential elsewhere. No resend of `reqwest`'s own (`retry::never()`): a retry policy that `f` sets with `builder.retry(…)` is replaced, and `reqwest`'s default resend of an HTTP/2 request that the server refused is off. |

What the connection pool sends again happens below what Concord counts as an attempt, and never puts a request on the wire twice ([what can send a request twice](../guide/retries.md#what-can-send-a-request-twice)). When `Http` recognizes a failure as one of its connector's (the connection was not made: refused, a host name not resolved, a failed TLS handshake or proxy, its connect timeout), the failure states that nothing was sent; the call's error reports it with `e.request_not_sent()` under the conditions of [C9.5](contracts.md#c9-the-transport-and-tests). Both functions return a `Result`, whose error is `reqwest`'s. A `reqwest::Client` that you pass to `with_transport` keeps its own settings, its retry policy among them ([rule 22](rules.md#the-call-and-the-client)). See [C7.1](contracts.md#c7-concords-own-client-and-the-timeouts) for the exact contract.

### The features of `concord-api`

| Feature | What it turns on | Default |
|---|---|---|
| `json` | `json(x)` and bounded `json_lines(records, max_bytes)` to write bodies, `Json` and `read::JsonLines` to read them | on |
| `form` | `form(x)`, `bracket_form(value)` and `bracket_pairs(value)` for forms and nested keys | on |
| `rustls` | TLS for `concord::Http`, through `reqwest/rustls` | on |
| `http2` | HTTP/2 for `concord::Http`, through `reqwest/http2` | on |
| `system-proxy` | The proxy settings of the system (macOS, Windows), as `reqwest` reads them, through `reqwest/system-proxy` | on |
| `gzip` | Response decompression: the gzip decoder of `reqwest` (`reqwest/gzip`), for `builder.gzip(true)` in `Http::with`. The `gzip(body)` writer needs `gzip-body` instead | off |
| `deflate` | The deflate decoder of `reqwest` (`reqwest/deflate`), for `builder.deflate(true)` in `Http::with` | off |
| `socks` | SOCKS proxies, configured with `reqwest::Proxy` (also reachable as `concord::reqwest::Proxy`) in `Http::with` (`reqwest/socks`) | off |
| `tower` | A `tower` service as a transport: `concord::tower::Tower` | off |
| `multipart` | `multipart()`, `multipart_mixed(parts)`, `multipart_related(parts)` to write multipart bodies | off |
| `gzip-body` | `gzip(body)` to compress a buffered request body and keep its content type. Response decompression is the feature `gzip` | off |
| `auth` | `concord::auth`: `Renewed`, a token that is renewed, and the retry rule `after_401` | off |
| `rotating` | `concord::auth::Rotating`, a refresh token that the server replaces at each exchange, with its `RotationDriver`; turns on `auth` | off |
| `sign` | `concord::sign`: `SigV4`, AWS Signature Version 4, and `body_bytes` and `check_origin` for a provider's own proof | off |
| `limit` | `concord::limit`: `Limit`, for rate limits | off |

`gzip`, `deflate` and `socks` only make a capability of `reqwest` available. `concord::Http` still decompresses nothing until `Http::with` turns decoding on, and with decompression on, a reader's limit counts the decoded bytes. `socks` sets no proxy by itself: a SOCKS proxy comes from `Http::with`, or from the proxy settings of the environment, which `concord::Http` reads as `reqwest` does; Concord's tests cover only the first. Turning the default features off (`default-features = false`) also turns off `json`, `form`, `rustls`, `http2` and `system-proxy`: list again the ones that you need. `brotli`, `zstd`, cookies and `native-tls` are not forwarded yet: until they are, add `reqwest` to your own `Cargo.toml` with that feature, or bring your own `reqwest::Client` with `with_transport` ([compression, proxies and TLS](../guide/calling.md#compression-proxies-and-tls)).

## What the public surface is tied to

**Other crates.** Concord's public signatures name types and traits of the first eight crates below; the last two rows are the sources of writers' errors, which a caller can reach by a downcast. A new major version of one of them in Concord's public surface is a breaking change of Concord, and comes only in a new major version of Concord.

| Crate | Version | Where it appears |
|---|---|---|
| `reqwest` | 0.13 | Re-exported as `concord::reqwest`. `Request` and `Response` (a `Transport`, a `Middleware`, `request()`, `edit_request`, `send_raw()`, `read_response`), `Client` (a transport of your own), `ClientBuilder` and `Error` (`Http::with`, `Http::new`), `Body` (`reply`, `reply_with`), `Url`. `Method`, `StatusCode` and the types of `header` are those of `http`, which `reqwest` re-exports. |
| `http` | 1 | Re-exported as `concord::http`: `StatusCode` (`status()` on an error and on an answer), `HeaderMap` (`Headers::as_map`, and the headers of a request that `Script` records), `Method`, and the errors of its `header` module in public variants (`RenewalError::InvalidHeader` holds an `InvalidHeaderValue`, `SignError::InvalidHeaderText` a `ToStrError`). |
| `bytes` | 1 | Re-exported as `concord::bytes`: `Bytes`, in a `Body`, a `ByteStream`, what `Binary` reads, and the requests that `concord-testkit` records. |
| `url` | 2 | `concord::reqwest::Url` is its `Url`: `url()` on a request, and `script::Request::url` in `concord-testkit`. |
| `futures-core` | 0.3 | Its `Stream` trait: `Body::stream` takes one, and `ByteStream`, `Pages`, `Items` and `RecordStream` implement it. |
| `serde` | 1 | Its `Serialize` and `Deserialize` traits: what an input, an entry, `json(x)` and `form(x)` take, and what `Json` reads into. |
| `tokio` | 1 | In `concord-testkit`: `script::Request::at` returns a `tokio::time::Instant`. `concord` names no type of Tokio; sending a call needs a Tokio 1.x runtime. |
| `tower-service` | 0.3 | Feature `tower`: `Tower<S>` is a transport when `S: Service<reqwest::Request, Response = reqwest::Response> + Send + 'static`, its future is `Send`, and its error converts into `Box<dyn Error + Send + Sync>`. |
| `serde_json` | 1 | The source of a writer's error, in no signature: the `source()` of the `WriteError` of `json(x)`, and of the `WriteError` that `JsonLinesError::source()` returns, is a `serde_json::Error`. |
| `serde_urlencoded` | 0.7 | The source of a writer's error, in no signature: the `source()` of the `WriteError` of `form(x)`, and of `bracket_form` and `bracket_pairs` when `serde` reports a failure, is a `serde_urlencoded::ser::Error`. |

**A feature only adds.** Turning a feature on adds items, and never removes or changes an item. The features `rustls`, `http2`, `system-proxy`, `socks`, `gzip` and `deflate` add no item of Concord's own: each turns on the feature of `reqwest` with its name, which adds a capability to `reqwest`, and may add items to `concord::reqwest` (`ClientBuilder::gzip` with `gzip`). Four of them change what `concord::Http` does on the wire, as their rows above say: `rustls` gives it TLS, `http2` lets it negotiate HTTP/2, `system-proxy` makes it read the proxy settings of the system, and `socks` lets the proxy settings of the environment name a SOCKS proxy. `gzip` and `deflate` change nothing on the wire until `Http::with` turns decoding on.

**The Rust version.** Concord needs Rust 1.96 or later (`rust-version` in each manifest). The minimum version is raised only in a minor release, never in a patch release.

**What another crate can turn on.** Cargo unifies the features of one resolved dependency for one target, so another crate of the program that depends on the same `reqwest` 0.13 can give `concord::Http` HTTP/2 negotiation, the proxy settings of the system, or another TLS back end, even when those features of `concord-api` are off: with `native-tls` on, `reqwest` uses native TLS by default, unless its `http3` is on as well. Decompression is the exception: `Http::with` turns every decoder off before `f` runs, whatever the build of `reqwest` has.

## Replacing a shipped item with your own

A body format, a reader, a pager, a middleware, a list type as a page, a transport, a setting of the HTTP client, a stack of `tower` layers, a method on a generated type: each one that you write fits in the same place as the shipped items. See [When Concord does not cover what you need](../guide/extending.md) for how to write each, with real code to copy.
