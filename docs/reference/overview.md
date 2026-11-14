# Concord on one page

*[Concord](../../README.md) · [docs](../README.md) · reference · next: [the grammar](grammar.md)*

Keep this page open when you use Concord and have read the tour. It sums up what a description may contain, what a call returns, what Concord promises, and where each of these stops. Each section ends with links to the pages that cover it in full.

## What Concord is

1. An API is a tree, and you describe it as one, in `concord::api!`: the client at the root, scopes for the levels in between, and endpoints at the leaves.
2. The client is a type. A scope becomes a method that returns a handle (a value that holds the inputs of the scope); an endpoint becomes a method that returns a call, and `.await` sends it.
3. Every request is built from the description, so each promise is enforced in one place, for every endpoint.
4. The macro knows no data format, authentication scheme or pager: `json`, `Json` and `by_offset` are ordinary Rust items, and you can replace them with your own.
5. It is built on `reqwest`, and is async only, on Tokio.
6. You depend on the package `concord-api` (`concord-api = "5.0.0-rc.1"`); in code, the library is named `concord`.

```rust
use concord::Json;

concord::api! {
    pub client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;

        GET get(id: u64) -> Task { path "{id}"; }
    }
}
```

```rust
let tasks = Tasks::new();
let task = tasks.get(7).await?;                 // `Task` is your own struct
```

→ [The tour](../guide/tour.md); [the crates](crates.md#the-crates); [scope and limits](../design/scope.md).

## The language

### The tree

There are three kinds of node. `client Riot(key: Secret) { … }` generates a type, built with `Riot::new(key)`. `scope matches(route: MatchRoute) { … }` generates a method that returns a handle. `GET get(id: MatchId) -> Match { … }` generates a method that returns a call; an empty block is written `;`. In a block, statements come first, then children (the scopes and endpoints inside it); on the client, parts come between the two. There are seven HTTP method keywords; `METHOD "VERSION-CONTROL"` names any other method.

### The signature and the block

The first line of an endpoint, its signature, is what the caller sees: read only those lines, and you have the whole API. The block under each signature says what goes on the wire, and how the response is read.

- `id: u64`: required. An argument of the method, in the order written.
- `count?: u32`: optional. A setter with the same name; the input is not sent unless it is set.
- `x: Option<T>`: a required argument that may be `None`.
- `milestone?: Option<u64>`: optional, with three states: not set, set to nothing, set to a number.
- `..page`: a part (a named group of inputs and statements, declared on the client), named at the end of the inputs.
- `-> T`: the return type, what a success returns. With no arrow, a success returns `()`.

Every input must be used, at its node or below, or the description does not compile. An argument accepts the declared type, a reference to it, a string for a `String` or a `Secret`, or an array or a slice for a `Vec`.

### Twelve statements and one declaration

| Statement | What it declares | Smallest example |
|---|---|---|
| `base` | Where the URL starts. | `base "https://uploads.example.com";` |
| `path` | Segments added to the URL. | `path "files/{name}";` |
| `query` | Pairs of the query string. | `query { count; }` |
| `headers` | Request headers. | `headers { "x-api-key" = key; }` |
| `body` | The request body. | `body json(task);`, `body json { text; cursor; }` |
| `read` | The reader: how a response body becomes a value. | `read Json;` |
| `status` | Arms, written like `match` arms: what a status means here. | `status { 404 => None }` |
| `problem` | The type of the API's own error body. | `problem RiotProblem;` |
| `middleware` | A value that every request passes through, such as a signer. | `middleware Signed::new(key.clone());` |
| `paginate` | How to fetch the next page of a list. | `paginate by_offset_limit(start, count);` |
| `idempotent` | Whether the request may be sent twice, when retries are on. | `idempotent;`, `idempotent false;` |
| `idle_timeout` | How long the server may stay silent, for the calls below. | `idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);` |
| `part` | A named group of inputs, with the statements that use them. | `part regional(route: Region) { base "https://{route}.api.riotgames.com"; }` |

`problem`, `middleware` and `part` appear on the client only; `body`, `paginate` and `idempotent` on an endpoint or in a part; `path` anywhere except in a part; the rest anywhere. A statement appears at most once in a node, except `middleware`.

An entry, one line of `query`, `headers` or a body, is `name;` (the key and the input share one name) or `"key" = value;`. A value is an input, fixed text, a template (`"v{version}"`) or any Rust expression. A placeholder `{x}` fills one segment of a path, or part of one; in the host, one label, or part of one. `{*x}` is allowed in a path only, as a whole segment, and its value may contain several segments. An arm binds the fields it uses, as in `200 { body } => Some(body)`: the fields are `body`, `headers` and `status`, and naming `body` reads the body.

### Names in the Rust expressions

What follows `body`, `read`, `middleware`, `paginate`, `idempotent` and `idle_timeout` is ordinary Rust, and so are the expression of an arm and the value of an entry that is not a template. No keyword of the language appears there.

- A bare name is an input of the node where the statement is written; an input of a node above is qualified with its owner: `client.key` or `matches.route`. In these expressions, an input is a reference: `&Option<u32>` for `count?: u32`.
- A bare name that is not an input is whatever Rust finds in your module, with no message.
- In its node, an input shadows an item with the same name, so refer to the item by its path: `body concord::form(form);`. The `f` of `body f { … }` is a path and is never shadowed.
- In `paginate`, an input refers to its slot on the call, which the pager reads and sets, not to its value. In `middleware`, only the client's required inputs are available.
- `?` ends the call: with a Build error in an entry or a `body`, and with the API's problem or a Mismatch in an arm. In `paginate`, `idempotent`, `idle_timeout` and `middleware`, `rustc` rejects it. In `read`, its behaviour is not promised either way.

### What a node inherits

What a node sets applies to everything below it.

- `base`, `read`, `idle_timeout`: the nearest one applies. `path` only adds segments, from the root down.
- An entry or an arm set above can be set again only with `override`, and removed only with `remove`. Within one node, each key is written once.
- Arms add to the simple rule, which reads a 2xx into the return type and makes any other status an error. An exact status wins over a class (`4xx`); under a class arm inherited from above, an exact status needs `override`.
- A part's inputs come after the node's own inputs, and its statements before the node's own statements. A key that both the part and the node set does not compile.
- Every request passes through each middleware, and nothing can remove one.
- An entry or a `path` that holds a `Secret` stays with the `base` that applies where it was written: under a different `base` below, the description does not compile until it is written again ([rule 16](rules.md#behaviour)).

→ [The grammar](grammar.md); [the rules](rules.md), rules 1 to 4, 7, 10, 13, 16, 25; [the cases](cases.md), tables 1 to 3.

## The call

```rust
let riot = Riot::new(key);
let ids  = riot.matches(Europe).ids(&puuid).count(5).send().await?;   // one page
```

Each level of the tree is one method call. A required input is an argument. An optional input has a setter with the same name; it accepts a `T`, a `&T` or an `Option<T>` (`None` unsets it).

Generated names follow the tree: the client `Riot` generates the module `riot`, its scope `matches` the handle `riot::Matches`, and the endpoint `ids` inside it the call `riot::matches::Ids`. The error is `riot::Error`, with `riot::Result<T>`; the retry policy is `riot::Retry` (`concord::Retry<RiotProblem>`).

### The methods of a call

| On a call | What it does |
|---|---|
| `.await`, `send()` | Sends the request and returns the value. |
| `send_full()` | Returns the value with the status and the headers: an `Answer<T>`. |
| `send_raw()` | Returns the `reqwest::Response`, with nothing read and no arm applied. |
| `send_if_none_match(etag)` | A conditional request, on a `GET` or a `HEAD`: returns an `Answer<Option<T>>`, which is `None` on a `304` that no arm matches. |
| `pages()`, `pages().items()` | On an endpoint with `paginate`: every page; every item. |
| `request()`, `into_request()` | The built `reqwest::Request`: `request()` builds it to inspect and keeps the call; `into_request()` consumes the call. |
| `edit_request(…)` | Changes the built request before it is sent. |
| `read_response(response)` | Reads a response that you obtained yourself, with the endpoint's arms and reader. |
| `with_retry`, `with_timeout`, `with_idle_timeout` | Concord's settings, for this call. |

A client has `new(required inputs)`, the same settings, and `with_base`, `with_transport` (the transport, which actually sends the request) and `with_middleware`. An API option keeps its own name; a Concord setting starts with `with_`. Concord's own method names form a closed list that no child or optional input may use, and a minor version adds none to it unless the name starts with `with_`.

### What `.await` returns

| The response | What the caller gets |
|---|---|
| A status that an arm matches | The value of the arm. |
| A 2xx that no arm matches, `-> T` | The body, read into `T` by the nearest `read`. |
| The same, `-> Option<T>` or `-> Result<T, E>` | The body read as `T`, wrapped in `Some` or `Ok`. A `null` body does not become `None`. |
| A 2xx, no arrow | `()`. The body is ignored. |
| A 2xx that no arm matches, `HEAD` with an arrow | A Mismatch. |
| Any other status that no arm matches | An error. |

A call to an endpoint with `paginate` **cannot be awaited directly**, so that nobody mistakes one page for the whole list: `.send().await` returns one page, and `.pages()` yields all of them. If you await it by mistake, `rustc` reports that it is not a future.

### The error type

Each client has one error type, with four kinds. **Build**: nothing was sent. **Transport**: the exchange failed; its class is timeout, connect, body, blocked or other. **Refused**: the API rejected the request, with a status that is not a 2xx and that no arm matches, or through an arm that ended with the API's own error. **Mismatch**: the response is not what the description promised, such as a body that cannot be read.

An error keeps the status, the headers and at most 64 KiB of the body; `e.problem()` returns the API's own error in your type. It also has `e.is_status(404)`, `e.class()`, `e.tries()`, `e.retry_after()` and `e.details()`.

→ [The rules](rules.md), rules 2, 11, 12, 15, 17, 20, 21, 24; [what a generated client has](crates.md#what-a-generated-client-has), in the crates; [the contracts](contracts.md), C5, C6, C8.8.

## What Concord promises

The promises cover the requests that the description builds.

| # | Concord promises | It depends on |
|---|---|---|
| G1 | It never prints a value: errors, logs and `Debug` show names. | What Concord itself prints. `e.details()` and `request()` show values on purpose. |
| G2 | A value stays where it is placed: it cannot change the structure of the request. | The request as built. After `.edit_request(…)`, it is not checked again. |
| G3 | Nothing declared goes to another origin; a secret does not follow a change of `base`. | Concord's own client, which follows no redirects. Your own client that follows them is outside the promise. |
| G4 | The body of an error response is never read as the success type. | None of your arms saying otherwise. |
| G5 | A body read whole stops at 10 MiB, and what an error keeps is bounded. | A body read whole. A stream has no limit. |
| G6 | Same inputs, same request: no hidden state, clock or randomness. | Your own code doing the same: a middleware can compute anything. |
| G7 | No call waits forever on a silent server: 30 seconds of silence at most. | Silence of the transport, or of a body being read. A middleware's or a reader's own wait is bounded only by the call timeout, `timeout`. |
| G8 | A request is retried only when you asked for it, and only when repeating it has no further effect. | What Concord itself passes down. What a middleware or `reqwest` sends again happens below it. |
| G9 | With no arrow, the status alone is the result. | No arm that applies reads a body; otherwise it does not compile. |
| G10 | A list never looks complete when it is not: a next page that cannot be fetched is an error. | Five checks, one of them against the request just sent. A longer cycle, or a list that never ends, is not detected. |

**A signed API is signed by construction:** its signer is a `middleware`, and nothing can remove it. This depends on the author declaring the signer last.

Four things are your own code: `.edit_request(…)`; a pager; a middleware or a transport; and a response that you obtained yourself.

→ [What Concord promises](../guide/promises.md): the promises, the safe defaults, and where the promises stop.

## What fails silently

If you guess wrong about one of these, no message tells you, so learn them first. Each number is a rule in [the rules](rules.md).

- **5.** The URL is the nearest `base`, then every `path`. It ends with `/` when the last of these does.
- **8.** A value is sent as `serde` serializes it. A list repeats its key. A `None` in an entry sends nothing.
- **15.** An error keeps 64 KiB of the body and prints no value; call `details()` when you want the values.
- **18.** Retries are off until you turn them on. Then only a request that may be repeated is retried: `GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE` unless `idempotent false;`, an endpoint marked `idempotent;`, a status listed with `not_applied_on`. An unmarked `POST` is not retried after a failed connection. `with_retry` on a call replaces the client's policy.
- **19.** At most 30 seconds of silence, on the transport and on a body while it is read; time spent in a middleware is not silence. A body in memory is sent in one piece. The whole call has no limit until you set a call timeout, `timeout`.
- **22.** `concord::Http`: 30 seconds to connect, no redirects followed, no cookies by default. Your own `reqwest::Client` keeps its own settings.
- **23.** Names in the logs, never values.

And the silent half of seven rules that are otherwise checked:

- **9.** In a body built by an expression, a `None` is whatever the type serializes (`null` in `json!`). A body of entries leaves out every `None`; in JSON, `"k" = Some(x);` writes `null` when `x` is `None`. A form has no `null`.
- **10.** An `override` with `None` sends nothing for that key. For each send, the order is `idle_timeout`, then `idempotent`, then the request by kind, from the client down. An entry keeps the position where its key was first written.
- **11.** There is no default format: the nearest `read` applies. `Json`, `Text` and `Binary` stop at 10 MiB. An alias of `Option` after the arrow is not recognized.
- **12.** With no arrow, success returns `()` and the body is ignored.
- **16.** A secret is read once, when the request is built.
- **20.** `request()` shows the values that the description places, including a `Secret`, but not what a middleware or a transport adds.
- **26.** A middleware is made once, in `new`, and shared by every handle and every call.

→ [The silent rules, in short](rules.md#the-silent-rules-in-short), in the rules.

## What Concord does not do

A fact of the API that every caller shares belongs in the description; state that lives across calls (a cache of responses) belongs in your own Rust value.

| Concord does not | The escape hatch |
|---|---|
| Sign requests, renew or obtain a token (OAuth2), limit the request rate, or share a quota | A middleware that you write. |
| Retry by default | `with_retry(Retry::standard())`. |
| Follow a redirect | An arm for the `3xx` and a second request; or your own `reqwest::Client` that follows them, outside G3. |
| Send a stream body twice | Your own loop. |
| Follow pages to another host, or poll by repeating a request | Your own loop. |
| Send to a full URL that the server returns | Same origin: `call.at_url(url)`. Another origin: a plain `reqwest` client, which holds none of your keys. |
| Send a header whose name the caller chooses (`x-amz-meta-*`) | Your own method on the call, using `.edit_request(…)`. |
| Put a `middleware` on a scope | A middleware on the client that looks at `info.endpoint()`. |
| Make a raw call on a handle | An endpoint, or your own method beside the generated ones. |
| Decompress a response by default | The feature `gzip` or `deflate`, then `builder.gzip(true)` or `builder.deflate(true)` in `concord::Http::with(…)` ([compression, proxies and TLS](../guide/calling.md#compression-proxies-and-tls)); or your own reader. |
| Split a stream into records (JSON Lines, SSE) | `read Stream;` yields the bytes, and another crate splits them into records. |
| Offer a blocking API | A blocking program runs it on its own Tokio runtime. |

→ [Scope and limits](../design/scope.md).

Limits of this release, `5.0.0-rc.1`: the crates are not on crates.io yet; no test calls a real API; editor support inside `api! { }` has not been verified.

See [Alternatives](../design/alternatives.md) for why each choice was made, and for the strongest alternative to each.
