# Call a client

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Describe an API](describing-an-api.md) · next: [Read responses and errors, send bodies](responses-and-errors.md)*

This page is for the caller of a generated client. It shows how to build the client and call it, what a call returns, and how long a call waits.

## What calling a client looks like

```rust
let riot = Riot::new(key);

// one level = one call; required values are the arguments
let account = riot.account(Europe).by_riot_id("Faker", "KR1").await?;        // an Option<Account>

// optional values: one setter each, named like the value
// (`ids` is paginated, so it is not awaited directly: `.send()` fetches one page)
let ids = riot
    .matches(Asia)
    .ids(&puuid)
    .count(5)
    .queue(420)
    .send()
    .await?;

// a handle is a value: hold it and reuse it
let asia = riot.matches(Asia);
for id in ids {
    let game = asia.get(id).await?;
}

// a paginated endpoint: all of its pages
let mut ids = asia.ids(&puuid).count(100).pages().items();
while let Some(id) = ids.try_next().await? {
    println!("{id}");
}
```

What a scope's method returns (`riot.matches(Asia)`) is a **handle**: a value that holds the inputs of the scope, and whose methods are the endpoints and scopes below it.

## The rules for using a client

| # | Rule |
|---|---|
| U1 | One level of the tree is one method call. |
| U2 | Required values are the arguments of that call, in the order they are declared. |
| U3 | An optional value (marked `?` in the description) has one setter, with exactly the name of the value and no prefix. |
| U4 | An argument is a value of the declared type, or a reference to one (`&puuid`; it is cloned internally). Text (`"Faker"`) is also accepted where a `String` or a `Secret` is declared, and an array or a slice where a `Vec` is declared. Nothing else is accepted: for `puuid: Puuid`, `ids("p-1")` does not compile ([arguments](#arguments)). |
| U5 | The setter of an optional input accepts whatever an argument of its type accepts, or an `Option` of it. `None` unsets it: `.count(5).count(None)` sends nothing. |
| U6 | `.await` sends the call: one request, or a few when retries are enabled. `send()` does the same thing, as a method. A call to a paginated endpoint is not awaited directly: `.send()` fetches one page, on purpose, and `.pages()` yields them all ([what a call returns](#what-a-call-returns)). |
| U7 | A handle and a call own their data. You can keep them, clone them, and move a call to another task. |
| U8 | A call is never reset: it is built, sent, and then consumed. |
| U9 | A required value has no method to change it later: for another route, make another handle. The only value that changes in place is a `Secret`: `key.replace(…)` ([a credential is a value](credentials.md#a-credential-is-a-value)). |

## The client

```rust
let riot = Riot::new(key);                 // the usual case: Concord's own HTTP client, with safe settings
```

`new` takes the required inputs of the client. An optional input of the client becomes a setter on the client, as it does on a call: GitHub's `token?: Secret` is `Github::new("my-tool/1.0").token(token)`.

**A `Secret` from text.** Where a `Secret` is declared, text is accepted, as a `&str` or a `String`: `Riot::new("RGAPI-…")`, `Riot::new(std::env::var("RIOT_KEY")?)`. `Secret::new(text)` creates one that you keep yourself (U9).

Every client has six more methods. Each returns the client with the change applied. They all start with `with_`, as does every Concord setting on a call: **an API option keeps its own name; a Concord setting starts with `with_`.**

| Method | For |
|---|---|
| `with_retry(retry)` | Retries. Off until this is called. A policy starts from `riot::Retry::standard()` (or `riot::Retry::none()`, which has no rule of its own) and is adjusted with `.tries(n)`, `.when(f)` and `.unless(f)`. `riot::Retry` is `concord::Retry<RiotProblem>`, so a rule can read the API's own error as a `RiotProblem`. `.not_applied_on(429)` lists a status after which a rejected request may be retried whatever its method, such as a `429` on a `POST`. It never panics: a number outside 100 to 999 is stored but matches no response. An author may ship the API's own policy as a function (`slack::retry()`), and the caller passes its result here. On a call, a policy replaces the client's whole policy for that call ([retries](retries.md)). |
| `with_timeout(duration)` | The maximum duration of a whole call. No limit until this is called ([timeouts](#timeouts)). |
| `with_idle_timeout(duration)` | How long a call may wait with no data moving. 30 seconds until this is called ([timeouts](#timeouts)). |
| `with_base("http://localhost:8080")` | Every request goes there, whatever the `base` statements say: a local test server, a gateway, another deployment. It may include a path prefix (`"https://example.com/gitlab"`). Every entry follows it, secrets included, because the caller chose it explicitly. |
| `with_middleware(m)` | One more middleware layer that every request passes through, before the description's middleware: logging, a limit of your own ([a middleware](extending.md#a-middleware)). |
| `with_transport(t)` | The transport that sends the requests, beneath the middleware: `concord::Http` with your own settings, your own `reqwest::Client`, a fake for tests, or another HTTP library ([testing a client](testing.md), [a transport](extending.md#a-transport)). |

**The names that Concord reserves.** A child or an optional input becomes a method, so it may not take the name of a method that its receiver already has. These names are matched exactly. On a call, they are Concord's own methods (`send`, `send_raw`, `request`, `pages`, …) and `at_url`, which pagers use. On a client or a handle: `call` (reserved for future use) and `clone`; on a client, also `new`. One prefix is reserved, `with_`, and no other: Telegram's `send_message` is a name like any other. Concord adds no new name to a call, a handle or a client in a minor version, unless it starts with `with_`. A required input is an argument, so its name is free. A scope or an endpoint may not generate a type named `Error`, `Result` or `Retry` in the module of its client. See [contracts](../reference/contracts.md) C1.6 for the full table. Finally, names that begin with `__` are reserved for Concord: in a module that contains `concord::api!`, declare no item (a `const`, a `static`, a function, a type) whose name begins with `__`.

**Concord's own HTTP client is `concord::Http`**, a Concord type that wraps `reqwest`. It waits at most 30 seconds for a connection, follows no redirects and adds no `Referer` header. It keeps no cookies and does not decompress by default ([below](#compression-proxies-and-tls)); a build of `reqwest` with cookie support can turn cookies on in `Http::with`. It reads the proxy settings from the environment. See [Concord's own HTTP client](../reference/crates.md#concords-own-http-client) for the exact settings.

Because `concord::Http` follows no redirects, a `3xx` is a final response, handled like any other status ([when a status means something](responses-and-errors.md#when-a-status-means-something); see [scope and limits](../design/scope.md) for the cost of this choice).

**Your own settings.** A certificate belongs to the connection, so it is set on the HTTP client, with `reqwest`'s own builder:

```rust
let http = concord::Http::with(|builder| builder.add_root_certificate(private_authority))?;

let billing = Billing::new().with_transport(http.clone());
let other   = Other::new(token).with_transport(http);          // two clients share one engine
```

Concord applies its defaults before your closure, and "no redirects" after it, so nothing in the closure can make this client follow a redirect.

**A `reqwest::Client` that you bring** is used as it is, and you are responsible for what it does on its own, such as following redirects or keeping cookies. Promise [G3](promises.md#three-promises) is stated for a client that follows no redirects. [The timeouts](#timeouts) are Concord's own, and apply with any client.

### Compression, proxies and TLS

By default, `concord::Http` does not decompress: it sends no `Accept-Encoding`, and a reader receives the body exactly as the server sent it. To decode gzip responses, turn on the `gzip` feature of `concord-api`, then turn decoding on for the client that needs it:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api", features = ["gzip"] }
```

```rust
let http = concord::Http::with(|builder| builder.gzip(true))?;
let tasks = Tasks::new().with_transport(http);
```

The `deflate` feature works the same way, with `builder.deflate(true)`. A feature alone changes nothing: it only makes the decoder available. Concord turns every decoder off before your closure runs, so only a client whose closure turns one on sends `Accept-Encoding` and decodes that format. With decompression on, a reader's limit counts the decoded bytes, not the compressed bytes on the wire. So a gzip response that expands past the default limit of `Json`, `Text` and `Binary`, 10 MiB, is a Mismatch, even when its body on the wire is small. `read Stream;` still has no limit.

The other features of `concord-api` that concern `concord::Http`:

- `socks` (off by default) lets `Http::with` use a SOCKS proxy that you configure with `reqwest::Proxy` (also reachable as `concord::reqwest::Proxy`). The feature sets no proxy by itself; `concord::Http` still reads the proxy settings of the environment, as described above.
- `rustls` (on by default) is the TLS stack of `concord::Http`. For another stack, turn the default features off (then list again the defaults you still need, such as `json`, `form`, `http2` and `system-proxy`) and pass `with_transport` a `reqwest::Client` built with that stack.
- `http2` (on by default) lets `concord::Http` use HTTP/2.
- `system-proxy` (on by default) reads the proxy settings of the system on macOS and Windows, as `reqwest` does.

`brotli`, `zstd`, cookies and `native-tls` are features of `reqwest` that `concord-api` does not forward yet. Until it does, add `reqwest` to your own `Cargo.toml` with the feature you need, at the same version, 0.13, so that Cargo builds a single `reqwest` with the feature on. Then turn the feature on in `Http::with`, or bring your own `reqwest::Client` with `with_transport`. See [the crates](../reference/crates.md#the-features-of-concord-api) for every feature.

## Looking without sending

`request()` builds the request without sending it, and returns the `reqwest::Request` itself: the method, the URL, the headers, the body.

```rust
let request = riot.matches(Europe).ids(&puuid).count(5).request()?;          // `puuid` is written as `p-1`
assert_eq!(request.url().as_str(), "https://europe.api.riotgames.com/lol/match/v5/matches/by-puuid/p-1/ids?count=5");
```

It is the request as the description builds it, with every value that the description places, including a key or a token that is a `Secret`, in a header marked sensitive. It does not include what middleware adds as the request is sent: `request()` returns the request before any middleware runs, and a signature belongs to a single attempt ([a middleware](credentials.md#a-middleware)). To see exactly what is sent, put a fake in place of the transport: it receives the request as sent ([a fake transport](testing.md#a-fake-transport)).

`request()` only inspects: the call is intact afterwards, and can still be sent. (A call whose body is a stream cannot show its request without consuming the stream, so `request()` returns an error for it; use a fake transport to inspect such a call.) `into_request()` consumes the call and returns the request. To send it yourself and still get the typed result, use `read_response`, described in [send a request yourself](extending.md#send-a-request-yourself-request-and-read_response).

## What a call returns

A call returns `riot::Result<T>`, where `T` is the endpoint's return type and the error is `riot::Error` ([the API's own error](responses-and-errors.md#the-apis-own-error)).

The error has four kinds:

| Kind | Meaning | The caller |
|---|---|---|
| Build | The request could not be built. Nothing was sent. | Fixes the input. |
| Transport | No response arrived, or the response broke off before the end of a body that was needed. `class()` says which: connect, timeout, body, blocked, other. | May retry. |
| Refused | The API rejected the request: a status that the endpoint does not handle (by default, anything that is not 2xx), or an arm that ended with the API's own problem. | Looks at the status, and at `problem()`. |
| Mismatch | The status is one that the endpoint handles, but the response does not match the declaration: the body is too large, it does not decode, or an arm or a pager rejected it. It is also a Mismatch when a `HEAD` endpoint with a return type receives a 2xx that no arm matches, since there is no body to read ([rule 11](../reference/rules.md)). | Reports it. |

**What an error contains.**

| | |
|---|---|
| `kind()`, `endpoint()`, `tries()` | The kind; the name of the endpoint (`riot.matches.get`); how many attempts were made. An attempt is counted when the request is handed to the middleware. |
| `status()`, `is_status(404)`, `headers()` | What the server returned, when a response arrived. A request id for the provider's support is in the headers. |
| `body()`, `kept()` | The start of the body, at most 64 KiB, and its state ([what an error keeps of a body](responses-and-errors.md#what-an-error-keeps-of-a-body)). |
| `problem()` | The API's own error, as your type ([the API's own error](responses-and-errors.md#the-apis-own-error)). |
| `class()`, `is_timeout()` | For a Transport error. |
| `source()` | The underlying cause, if there is one. It never contains a value from the response. For a Transport error: the HTTP library's error, with its URL removed. For a body that failed to read: a small cause that Concord builds from the location and the kind of failure. For a Build error that came from your own code (a body, a `?` in an entry): that failure. |
| `retry_after()` | The longer of two waits, when there is one: the `Retry-After` of the response, and the wait that a `when` rule requested for the attempt that was not made. |
| `reason()` | A message supplied by the author's code (`concord::mismatch("…")`), such as why a pager stopped or why an arm rejected the response. Only such a message sets it: for a body that failed to read or to decode, it is `None`. |
| `details()` | A single string to print, values included: the line that `{e}` prints, the reason, the headers, the API's own error or else the start of the body, the reader's own message (which can quote the body; at most 1 KiB), and the inner error. It is available when the error type implements `Debug`. |

```rust
match asia.get(id).await {
    Ok(game) => …,
    Err(e) if e.is_status(403) => {
        let problem = e.problem();                           // the API's own error, typed
        …
    }
    Err(e) => {
        tracing::warn!("{e}");                               // names only: safe in any log
        tracing::debug!("{}", e.details());              // everything, by your choice
        return Err(e.into());
    }
}
```

**Printing.** `{e}` prints one line in Rust's usual style: the kind, the status, the name of the endpoint, the number of attempts, and, where it can, a name for the cause. It never shows a value: not the body, not a URL, not a secret, not a message that your own code supplied ([rule 15](../reference/rules.md); see [the messages](../reference/messages.md) for the exact texts). `e.details()` is the opt-in alternative: call it where you decide to print values. Its output may contain a URL.

**The whole response.** A call returns the value, which is what most callers want. When the status and headers matter, a second form keeps them:

```rust
let answer = docs.get(&id).send_full().await?;
let etag   = answer.headers().get("etag").map(String::from);   // for the `If-Match` of the next call
let status = answer.status();                           // 200, 201, …
let doc    = answer.into_value();                       // the same value `.await` returns; `answer.value()` borrows it
```

**Only if it changed.** On a call whose method is `GET` or `HEAD`, `send_if_none_match(etag)` takes a `&str` and sends it as `If-None-Match`, replacing any that the description sets ([rule 20](../reference/rules.md)). It returns an `Answer<Option<T>>`: on a `304` that no arm handles, the value is `None`, and the headers are still there (a new `ETag`, `Cache-Control`).

```rust
let answer = docs.get(&id).send_if_none_match(&etag).await?;
match answer.into_value() {
    Some(doc) => …,                                     // it changed
    None => …,                                          // 304: keep the one you have
}
```

**A paginated call.** A call to an endpoint with `paginate` is not awaited directly, so that one page is never mistaken for the whole list. `.send().await?` fetches one page, on purpose. `.pages()` yields every page, one per `try_next()`, and `.pages().items()` yields their items one by one ([pages](pages.md)). `.items()` is available when the page type implements `concord::page::IntoItems`: `Vec<T>` does, and an author implements it for their own list type ([a page type of your own](pages.md#a-page-type-of-your-own-intoitems)).

## Timeouts

Concord has two timeouts, each with one job: the *idle timeout* limits silence (time with no data moving), and the *call timeout* limits the whole call.

| | What it limits | Default | How to set it |
|---|---|---|---|
| The idle timeout, `idle_timeout` | Silence. It watches two things: the transport, from the moment it receives the request until the response head arrives; and a body while it is being read. Each chunk sent or received resets it. | 30 seconds. | `with_idle_timeout(…)`, on the client or on a call. An author sets it in the description for the calls that need a different value (`idle_timeout`). |
| The call timeout, `timeout` | The whole call: every attempt, every wait between attempts, and reading the body. Once it has expired, no further request is handed to the transport. | No limit. | `with_timeout(…)`, on the client or on a call. |

Setting one never changes the other.

```rust
// long polling: Telegram holds the request for up to 50 seconds, on purpose.
// The description says how long this endpoint may stay idle, so the caller writes nothing
let updates = bot.get_updates().offset(next).timeout(50).await?;

// a download: ten minutes in all, and still a quick stop if it stalls
let file = files.download(&id).with_timeout(Duration::from_secs(600)).await?;
```

In the first call, `.timeout(50)` is Telegram's own parameter. The idle timeout is the first of these that is set: `with_idle_timeout` on the call, the `idle_timeout` of the description, `with_idle_timeout` on the client, and otherwise 30 seconds.

**An endpoint that is idle on purpose declares it.** A long poll, or a stream that is quiet for minutes, is a fact about the endpoint, and its callers should not have to know about it. `idle_timeout <expression>;` may be placed on an endpoint, a scope or the client (a part may hold it too), and the nearest one applies. The expression is Rust, with the inputs as variables, and evaluates to a `std::time::Duration`; see Telegram's `get_updates` in [the block](describing-an-api.md#the-block).

**The idle timeout watches two things, and nothing else.** First, the transport, from the moment it receives the request until the response head arrives: each chunk of a streamed request body that is sent resets the timer. Second, a body while it is being read: each chunk resets it. Time spent in a middleware, before or after the transport, and a reader's own waits never count as silence: only the call timeout, `timeout`, bounds them. The idle timeout is Concord's own, so it applies with any HTTP client.

| | |
|---|---|
| When a timeout expires | The call ends with a Transport error. The error says which timeout expired, and how to change it ([the messages](../reference/messages.md); [a timeout](troubleshooting.md#the-call-failed)). |
| An error response is not lost to a timeout | When a timeout ends an attempt that had already received an error status, the result is that Refused error, and `kept()` says that its body was interrupted. |
| A large upload | A streamed body counts as progress: each chunk of it that is sent resets the idle timeout. A body held in memory is sent as a single chunk, so a large one on a slow connection needs a longer `idle_timeout` on that call. |
| A stream handed to the caller | The call timeout, `timeout`, keeps running until the stream ends. The idle timeout runs while a `try_next()` is waiting, not between two calls to it. |
| Your own code | A timeout ends a wait; it cannot interrupt code that does not `.await`. |
| After `send_raw()` | Concord's work ends at the response head: the `reqwest::Response` is yours, and so is the time spent reading it. Neither timeout applies to it. |
| A dropped call | A call that is dropped stops, and Concord starts no further attempt. What the HTTP library does with the connection is up to the library, and the request may already have reached the server. |

### The steps of one call

| Step | What happens | If it fails |
|---|---|---|
| 1 | The expressions of the description run: `idle_timeout`, then `idempotent`; then the request is built from the inputs, kind by kind (the base, the paths, the query, the headers, the body). Then the caller's `.edit_request(…)` modifies it. This happens once per send, and each page is one send. | Build error. Nothing was sent. |
| 2 | For each attempt, a copy passes through the middleware: the caller's layers, then the description's in the order written (for example, one adds a token and the last one signs). Then it is handed to the transport, which sends it. | No response: the attempt has failed. A middleware layer or a transport that refuses to send: no further attempt, and the result is that blocked error. |
| 3 | The response head arrives. The status selects an arm ([how a response is read](responses-and-errors.md#how-a-response-is-read-status-reader-arm)), the reader reads the body if the arm binds it, and the arm produces the result. | A Refused error (also when an arm for a status that is not a 2xx binds a `body` that fails to read), a Mismatch, or a failed attempt if the body broke off. |
| 4 | The retry policy looks at how the attempt ended ([retries](retries.md)). | Another attempt from step 2, or the result. |

The other expressions run at their own time: `read` for each response, the expression of `paginate` once, at `.pages()`, and a `middleware` expression once, in `new` ([when an expression runs](../reference/grammar.md#when-an-expression-runs)). Within each kind, the order follows the tree ([rules](../reference/rules.md), rule 10).

The reading step alone is `call.read_response(response)`: the endpoint's arms, reader and problem type, applied to a response that you obtained yourself with `call.request()?` ([send a request yourself](extending.md#send-a-request-yourself-request-and-read_response)). It has no retry, no timeout and no pagination, and it takes `&self`, so a call can read several responses.

See [the engine page](../internals/engine.md) for how the engine implements the two timeouts.

## When you need more control

Each layer has an extension point or an escape hatch for the cases it does not cover: your own reader, body writer or pager in the description; `edit_request`, `send_raw`, `request()` and `read_response` on a call; a middleware layer or a transport on the client; and, below all of them, plain `reqwest`. Each one is shown, with code to copy, in [When Concord does not cover what you need](extending.md).

## Testing without a network

`with_transport(concord::fake(|request| …))` replaces the network with a function: it receives the request as it would be sent, and returns the response. Everything else in Concord runs for real.

See [testing a client](testing.md) for a fake transport, a scripted transport on virtual time, a local server, and the lines a test needs in `Cargo.toml`.

## The public API of a client

```rust
// what the docs would list
// arguments are shown with their declared types (see U4 and U5 for what else is accepted)

impl Riot {
    pub fn new(key: Secret) -> Riot;
    // with_retry, with_timeout, with_idle_timeout, with_base, with_middleware, with_transport

    pub fn account(&self, route: AccountRoute) -> riot::Account;
    pub fn matches(&self, route: MatchRoute) -> riot::Matches;
}

impl riot::Matches {
    pub fn ids(&self, puuid: Puuid) -> riot::matches::Ids;
    pub fn get(&self, id: MatchId) -> riot::matches::Get;
}

impl riot::matches::Ids {
    pub fn start(self, start: u32) -> Self;
    pub fn count(self, count: u32) -> Self;
    pub fn queue(self, queue: i32) -> Self;
    pub fn start_time(self, start_time: i64) -> Self;

    pub fn send(self) -> concord::Sending<Vec<MatchId>, RiotProblem>;   // one page: riot::Result<Vec<MatchId>>. (With no `paginate`, `.await` on the call does this.)
    pub fn send_full(self) -> concord::Sending<concord::Answer<Vec<MatchId>>, RiotProblem>;   // with the status and the headers
    pub fn pages(self) -> concord::Pages<Vec<MatchId>, RiotProblem>;    // because of `paginate`; `.pages().items()` yields each `MatchId`
    // send_if_none_match(etag): an Answer<Option<Vec<MatchId>>>, because the method is `GET`
    pub fn request(&self) -> riot::Result<reqwest::Request>;         // look
    pub fn into_request(self) -> riot::Result<reqwest::Request>;      // take
    pub async fn read_response(&self, response: reqwest::Response) -> riot::Result<Vec<MatchId>>;   // a response that you got yourself
    pub fn send_raw(self) -> concord::Sending<reqwest::Response, RiotProblem>;
    // with_retry, with_timeout, with_idle_timeout, edit_request
}
```

Generated types follow the tree, and a scope may share its name with one of your own types: `riot::Account` is the handle of the scope `account`, while `Account` is still your struct ([rule 24](../reference/rules.md)). Everything generated has the visibility written on the client. Your own types and imports resolve as they do where `api!` is invoked; see [the macro page](../internals/macro.md#where-the-generated-items-live-and-how-they-reach-the-authors-types) for how the macro achieves this. In a crate that is itself named `riot`, write `use ::riot::*;`.

## Arguments

The trait behind U4 and U5:

```rust
/// What an argument for an input of type `T` may be. Sealed: only Concord implements it.
pub trait Arg<T>: sealed::Sealed<T> { fn into_input(self) -> T; }     // sealed per target type

impl<T>        Arg<T>           for T       { /* the type itself */ }
impl<T: Clone> Arg<T>           for &T      { /* a reference: cloned inside */ }
impl           Arg<String>      for &str    { /* text */ }
impl           Arg<Secret>      for &str    { /* a secret from text; also from a String */ }
impl<T: Clone> Arg<Vec<T>>      for &[T]    { /* a list from a slice; also from an array */ }
impl           Arg<Vec<String>> for &[&str] { /* a list of text; also from an array */ }

// the setter of an optional input (`count?: u32`) takes each of these, and an `Option<T>`: `None` un-sets it
impl<T>        Arg<Option<T>>   for T       { /* Some(self) */ }
```

So `.ids(&puuid)`, `.ids(puuid)`, `.count(5)`, `.count(None)`, `.by_riot_id("Faker", "KR1")` and `.labels(["bug", "ui"])` (for `labels?: Vec<String>`) are all accepted.

**An optional input whose type is an `Option`** has three states ([a signature](describing-an-api.md#a-signature)). For `milestone?: Option<u64>`: `.milestone(5)` and `.milestone(Some(5))` set the number, and `.milestone(None::<u64>)` sets the input to `None`, which is not the same as leaving it unset. A bare `None` does not compile there, because Rust cannot tell which of the two is meant. The setter for `name?: Option<String>` accepts a `&str` too. A setter converts through these two levels and no further: for `n?: Option<Option<u64>>`, it accepts only the exact type.

**The trait is sealed.** Code outside Concord cannot add a conversion: for `puuid: Puuid`, `ids("p-1")` does not compile. The caller writes `Puuid::from("p-1")`, or the author declares `puuid: String`. A general "anything that converts" bound (`impl Into<T>`) would not work for a plain number: for an input declared `u32`, `.count(5)` would not compile.

## The generated documentation of an endpoint

Each generated method is documented from the description: your own `///` text, then how the endpoint is called, its request line (the method, the URL with its placeholders, the keys of the query), the headers it sets, and what each input accepts. All of it is generated from the declaration, so it cannot go stale. It is also the quickest way to check that a description is right: read the request line and compare it with the provider's documentation. The inputs of a part are shown like the endpoint's own. The crate documentation includes `concord::sample`, a small client generated by the macro, as rustdoc renders it.
