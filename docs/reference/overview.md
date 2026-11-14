# Concord on one page

*[Concord](../../README.md) · [docs](../README.md) · reference · next: [the grammar](grammar.md)*

For you when you use Concord and want one page to keep open: what a description may say, what a call gives, what Concord promises, and where each stops. Each part ends with links to the pages that say it in full.

## What it is

1. An API is a tree, and you write it as one, in `concord::api!`.
2. The client is a type; a scope is a method that gives a handle; an endpoint is a method that gives a call, and `.await` sends it.
3. Every request is built from the description, so the promises are made once, for every endpoint.
4. The macro knows no format, no scheme and no pager: `json`, `Json` and `by_offset` are ordinary Rust items, and one of yours takes the same place.
5. It is `reqwest` underneath, async only, on Tokio.
6. You depend on the package `concord-api` (`concord-api = "5.0.0-rc.1"`); in code, the library is `concord`.

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

→ [The tour](../guide/tour.md); the crates, [The crates](crates.md#the-crates); [scope and limits](../design/scope.md).

## The language

### The tree

Three kinds of node. `client Riot(key: Secret) { … }` gives a type, made with `Riot::new(key)`. `scope matches(route: MatchRoute) { … }` gives a method that returns a handle. `GET get(id: MatchId) -> Match { … }` gives a method that returns a call; an empty block is written `;`. In a block, statements come first, then children; on the client, parts stand between. Seven method words, and `METHOD "VERSION-CONTROL"` for any other.

### The signature and the block

Read only the lines that start with a method: that is the whole API, as its user sees it. The block under each line is for the wire.

- `id: u64`: required. An argument, in the order written.
- `count?: u32`: optional. A method with its own name; not sent when not set.
- `x: Option<T>`: a required argument that may be `None`.
- `milestone?: Option<u64>`: three states. Not set, set to nothing, set to a number.
- `..page`: a part, named at the end.
- `-> T`: what a success gives. With no arrow, `()`.

Every input is used. An argument takes the declared type, a reference to it, text for a `String` or a `Secret`, or an array or a slice for a `Vec`.

### Twelve statements and one declaration

| Statement | What it says | Smallest example |
|---|---|---|
| `base` | Where the URL starts. | `base "https://uploads.example.com";` |
| `path` | Segments added to the URL. | `path "files/{name}";` |
| `query` | Pairs of the query. | `query { count; }` |
| `headers` | Headers. | `headers { "x-api-key" = key; }` |
| `body` | The one body. | `body json(task);`, `body json { text; cursor; }` |
| `read` | How an answer becomes a value. | `read Json;` |
| `status` | Arms: what a status means here. | `status { 404 => None }` |
| `problem` | How the API says no. | `problem RiotProblem;` |
| `middleware` | What every request goes through. | `middleware Signed::new(key.clone());` |
| `paginate` | How a list comes in pages. | `paginate by_offset_limit(start, count);` |
| `idempotent` | Whether it may be sent again. | `idempotent;`, `idempotent false;` |
| `idle_timeout` | The clock for silence below. | `idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);` |
| `part` | A named group of inputs, with the lines that place them. | `part regional(route: Region) { base "https://{route}.api.riotgames.com"; }` |

`problem`, `middleware` and `part` stand on the client only; `body`, `paginate` and `idempotent` on an endpoint or in a part; `path` anywhere but in a part; the rest anywhere. A statement is written at most once in a node, except `middleware`.

An entry is `name;` (key and input have one name) or `"key" = value;`. A value is an input, fixed text, a template (`"v{version}"`) or any Rust expression. In a path, `{x}` fills one segment or a piece of one; in the host, one label or a piece of one. `{*x}` is a whole segment that may hold several, in a path only. An arm binds what it uses, `200 { body } => Some(body)`: the fields are `body`, `headers`, `status`, and naming `body` reads it.

### A name on a right side

No word of the language stands on the right: after `body`, `read`, `middleware`, `paginate`, `idempotent` and `idle_timeout`, in an arm and in an entry that is not a template, comes ordinary Rust.

- A bare name is an input of the node that wrote the line; an input above is `client.key`, `matches.route`. In Rust it is a reference: `&Option<u32>` for `count?: u32`.
- A bare name that is not an input is whatever Rust finds in your module, with no word.
- An input hides an item of its name in its node: write the path, `body concord::form(form);`. The `f` of `body f { … }` is a path, never hidden.
- In `paginate`, an input names its place on the call. In `middleware`, only the client's required inputs are there.
- `?` ends the call: with a Build error in an entry or a `body`, with the API's problem or a Mismatch in an arm. Elsewhere, `rustc` refuses it.

### What a node inherits

What a node sets holds for everything below it.

- `base`, `read`, `idle_timeout`: the nearest one. `path` only adds, root first.
- An entry or an arm set above is set again only with `override`, and taken away only with `remove`. In one node, a key is written once.
- Arms add to the simple rule. An exact status wins over a class; under a class arm from above, it needs `override`.
- A part's inputs come after the node's own, its statements before. A key that both write does not compile.
- A middleware holds for every request; nothing takes it away.
- An entry or a `path` that holds a `Secret` stays with the `base` where it was written: under another `base` below, it does not compile until it is written again ([rule 16](rules.md#behaviour)).

→ [The grammar](grammar.md); [the rules](rules.md), rules 1 to 4, 7, 10, 13, 16, 25; [the cases](cases.md), tables 1 to 3.

## The call

```rust
let riot = Riot::new(key);
let ids  = riot.matches(Europe).ids(&puuid).count(5).send().await?;   // one page
```

One level of the tree is one call. A required input is an argument. An optional input is a method with its own name; it takes a `T`, a `&T` or an `Option<T>` (`None` un-sets it).

Names follow the tree: the client `Riot` gives the module `riot`, its scope `matches` the handle `riot::Matches`, the endpoint `ids` in it the call `riot::matches::Ids`. The error is `riot::Error`, with `riot::Result<T>`; the retry policy is `riot::Retry` (`concord::Retry<RiotProblem>`).

### The verbs

| On a call | What it does |
|---|---|
| `.await`, `send()` | Sends; gives the value. |
| `send_full()` | The value with the status and the headers: an `Answer<T>`. |
| `send_raw()` | The `reqwest::Response`, with nothing read and no arm asked. |
| `send_if_none_match(etag)` | On a `GET` or a `HEAD`: an `Answer<Option<T>>`, `None` on a `304` that no arm takes. |
| `pages()`, `pages().items()` | With `paginate`: every page; every item. |
| `request()`, `into_request()` | The built `reqwest::Request`: to look at; to take. |
| `edit_request(…)` | Changes the built request. |
| `read_response(response)` | Reads an answer that you got yourself, with the endpoint's arms and reader. |
| `with_retry`, `with_timeout`, `with_idle_timeout` | Concord's settings, for this call. |

A client has `new(required inputs)`, the same settings, and `with_base`, `with_transport` (what really sends) and `with_middleware`. An option of the API keeps its own name; a setting of Concord starts with `with_`. Concord's verbs are a closed list, which no child or optional input may take, and Concord adds none in a minor version unless it starts with `with_`.

### What `.await` gives

| The answer | What the caller gets |
|---|---|
| A status that an arm takes | What the arm gives. |
| A 2xx that no arm takes, `-> T` | The body, read into `T` by the nearest `read`. |
| The same, `-> Option<T>` or `-> Result<T, E>` | The body read as `T`, given as `Some` or `Ok`. A `null` body is not a `None`. |
| A 2xx, no arrow | `()`. The body is ignored. |
| A 2xx that no arm takes, `HEAD` with an arrow | A Mismatch. |
| Any other status that no arm takes | An error. |

A call to an endpoint with `paginate` is **not awaited directly**, so that nobody takes one page for the whole list: `.send().await` gives one page, `.pages()` all of them. Awaited by mistake, `rustc` says it is not a future.

### The error

One error type for a client, with four kinds. **Build**: nothing was sent. **Transport**: the exchange failed; its class is timeout, connect, body, blocked or other. **Refused**: the API said no, with a status that no arm takes. **Mismatch**: not what was promised, such as a body that does not read.

It keeps the status, the headers, at most 64 KiB of the body, and the API's own error as your type: `e.problem()`. Also `e.is_status(404)`, `e.class()`, `e.tries()`, `e.retry_after()`, `e.details()`.

→ [The rules](rules.md), rules 2, 11, 12, 15, 17, 20, 21, 24; the crates, [What a generated client has](crates.md#what-a-generated-client-has); [the contracts](contracts.md), C5, C6, C8.8.

## The promises

They are about what the description builds.

| # | Concord promises | It stands on |
|---|---|---|
| G1 | It never prints a value: errors, logs and `Debug` show names. | What Concord itself prints. `e.details()` and `request()` give values on purpose. |
| G2 | A value stays in its slot: it cannot change the structure of the request. | The request as built. After `.edit_request(…)`, it is not checked again. |
| G3 | Nothing declared goes to another origin; a secret does not follow a change of `base`. | Concord's own client, which follows no redirect. A client of yours that follows them is outside. |
| G4 | The body of a refusal is never read as the success type. | No arm of yours saying otherwise. |
| G5 | A body read whole stops at 10 MiB; what an error keeps is bounded. | A body read whole. A stream has no limit. |
| G6 | Same inputs, same request: no hidden state, clock or randomness. | Your own code doing the same: a middleware can compute anything. |
| G7 | No call waits for ever on a silent server: 30 seconds of silence at most. | Silence of the transport, or of a body being read. A middleware's or a reader's own wait is bounded only by `timeout`. |
| G8 | A request is sent again only when you asked, and only when repeating it changes nothing more. | What Concord itself hands on. What a middleware or `reqwest` sends again is below it. |
| G9 | No arrow: the status is the answer. | No arm in reach that reads a body: else it does not compile. |
| G10 | Pages never look whole when they are not: a next page that cannot be asked is an error. | Five checks, the request just sent among them. A longer cycle, or a list that never ends, is not seen. |

**A signed API is signed by construction:** its signer is a `middleware`, and nothing takes it away. It stands on the author writing the signer last.

Four things are your own code: `.edit_request(…)`, a pager, a middleware or a transport, and an answer that you got yourself.

→ [What Concord promises](../guide/promises.md): the promises, the safe defaults, and where the promises stop.

## What is silent

A wrong guess about these meets no message. Learn them first. Each number is a rule of [the rules](rules.md).

- **5.** The URL is the nearest `base`, then every `path`. It ends with `/` when the last piece does.
- **8.** A value is written as `serde` writes it. A list repeats its key. A `None` in an entry writes nothing.
- **15.** An error keeps 64 KiB and prints no value. `details()` when you want them.
- **18.** Retry is off. When on, only what may be repeated goes again: `GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE` unless `idempotent false;`, an endpoint marked `idempotent;`, a status listed with `not_applied_on`. An unmarked `POST` is not sent again after a failed connection. `with_retry` on a call replaces the client's policy.
- **19.** 30 seconds of silence at most: the transport, and a body while it is read; a middleware's time is not silence. A body in memory goes out in one piece. No limit in all until you set `timeout`.
- **22.** `concord::Http`: 30 seconds to connect, no redirect followed, no cookies by default. A `reqwest::Client` of yours keeps its own settings.
- **23.** Names in the logs, never values.

And the silent half of seven rules that are otherwise checked:

- **9.** In a body made by an expression, a `None` is what the type writes (`null` in `json!`). A body of entries leaves out every `None`; `"k" = Some(x);` writes `null`. A form has no `null`.
- **10.** An `override` with `None` sends nothing for that key. For each send: `idle_timeout`, then `idempotent`, then the request by kind, from the client down. An entry keeps the place where its key was first written.
- **11.** No format by default: the nearest `read`. `Json`, `Text` and `Binary` stop at 10 MiB. An alias of `Option` after the arrow is not seen.
- **12.** No arrow: success gives `()`, and the body goes unseen.
- **16.** A secret is read once, when the request is built.
- **20.** `request()` shows the values that the description places, a `Secret` among them; not what a middleware or a transport adds.
- **26.** A middleware is made once, in `new`, and shared by every handle and every call.

→ The rules, [The silent ones, again](rules.md#the-silent-ones-again).

## What Concord does not do

A fact of the API that every caller shares goes in the description; a state that lives across calls (a cache of answers) goes in a Rust value of yours.

| Concord does not | The door |
|---|---|
| Sign a request, renew or get a token (OAuth2), limit a rate, share a quota | A middleware that you write. |
| Try again by default | `with_retry(Retry::standard())`. |
| Follow a redirect | An arm for the `3xx` and a second request; or a `reqwest::Client` of yours that follows them, outside G3. |
| Send a stream body twice | A loop of yours. |
| Follow pages to another host, or a poll that repeats its request | A loop of your own. |
| Send to a URL that the server gives whole | Same origin: `call.at_url(url)`. Another: a plain `reqwest` client, which has none of your keys. |
| Name a key that the caller chooses (`x-amz-meta-*`) | A method of yours on the call, with `.edit_request(…)`. |
| Put a `middleware` on a scope | A middleware on the client that looks at `info.endpoint()`. |
| Make a raw call on a handle | An endpoint, or a method of yours beside the generated ones. |
| Decompress | Turn it on in `concord::Http::with(…)`, or a reader of yours. |
| Cut a stream into records (JSON Lines, SSE) | `read Stream;` gives the bytes; a crate cuts the records. |
| Have a blocking form | A blocking program runs it on a Tokio runtime of its own. |

→ [Scope and limits](../design/scope.md).

Limits of this release, `5.0.0-rc.1`: the crates are not on crates.io yet; no test calls a real API; editor support inside `api! { }` has not been verified.

Why each choice was made, and the strongest other design for each: [for each choice, the strongest other choice](../design/alternatives.md).
