# Call a client

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [Describe an API](describing-an-api.md) · next: [Read answers and errors, send bodies](responses-and-errors.md)*

For the user of a generated client: how to call it and build it, what comes back, and how long it waits.

## A first look

```rust
let riot = Riot::new(key);

// one level = one call; required values are the arguments
let account = riot.account(Europe).by_riot_id("Faker", "KR1").await?;        // an Option<Account>

// values that are not required: one method each, named like the value
// (`ids` has pages, so it is not awaited directly: `.send()` gives one page)
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

// an endpoint with pages: all of them
let mut ids = asia.ids(&puuid).count(100).pages().items();
while let Some(id) = ids.try_next().await? {
    println!("{id}");
}
```

What a scope's method returns (`riot.matches(Asia)`) is a **handle**: a value that holds the inputs of the scope, and whose methods are the endpoints and scopes below it.

## The rules of use

| # | Rule |
|---|---|
| U1 | One level of the tree is one method call. |
| U2 | Required values are the arguments of that call, in the order of the declaration. |
| U3 | A value that is not required (marked `?` in the description) has one method, with exactly the name of the value. No prefix. |
| U4 | An argument is the declared type, or a reference to it (`&puuid`; it is cloned inside). Text (`"Faker"`) is also accepted where a `String` or a `Secret` is declared, and an array or a slice where a `Vec` is declared. Nothing else is: for `puuid: Puuid`, `ids("p-1")` does not compile ([arguments](#arguments)). |
| U5 | The method of an optional input takes what an argument of its type takes, or an `Option` of it. `None` un-sets it: `.count(5).count(None)` sends nothing. |
| U6 | `.await` sends: one request, or a few when retry is set. `send()` is the same thing as a method. A call to an endpoint with pages is not awaited directly: `.send()` gives one page, on purpose, and `.pages()` gives them all ([what comes back](#what-comes-back)). |
| U7 | A handle and a call own their data. Hold them, clone them, move a call to another task. |
| U8 | Nothing is ever reset. A call is built, sent, and gone. |
| U9 | A required value has no method to change it later. For another route, make another handle. The one thing that changes in place is a `Secret`: `key.replace(…)` ([a credential is a value](credentials.md#a-credential-is-a-value)). |

## The client

```rust
let riot = Riot::new(key);                 // the usual case: Concord's own HTTP client, with safe settings
```

`new` takes the required inputs of the client. An optional input of the client is a setter on the client, as it is on a call: GitHub's `token?: Secret` is `Github::new("my-tool/1.0").token(token)`.

**A `Secret` from text.** Where a `Secret` is declared, text is accepted, as a `&str` or a `String`: `Riot::new("RGAPI-…")`, `Riot::new(std::env::var("RIOT_KEY")?)`. `Secret::new(text)` makes one that you hold (U9).

Six more methods are on every client. Each returns the client, changed. They all start with `with_`, and so does every setting of Concord on a call: **an option of the API keeps its own name, a setting of Concord starts with `with_`.**

| Method | For |
|---|---|
| `with_retry(retry)` | Trying again. Off until this is called. A policy is `riot::Retry::standard()` (or `riot::Retry::none()`, which has no rule of its own), with `.tries(n)`, `.when(f)` and `.unless(f)`: `riot::Retry` is `concord::Retry<RiotProblem>`, so a rule reads the API's own error, typed. `.not_applied_on(429)` lists a status after which a refused request may be repeated whatever its method: a `429` on a `POST`. It never panics: a number outside 100 to 999 is kept and matches no answer. An author may ship the policy of the API (`slack::retry()`), and the caller names it. On a call, a policy replaces the whole policy of the client for that call ([retries](retries.md)). |
| `with_timeout(duration)` | The longest time for a whole call. No limit until this is called ([time](#time)). |
| `with_idle_timeout(duration)` | How long the server may say nothing. 30 seconds until this is called ([time](#time)). |
| `with_base("http://localhost:8080")` | Every request goes there, whatever the `base` lines say: a local test server, a gateway, another deployment. It may have a path prefix (`"https://example.com/gitlab"`). Every entry follows it, a secret too: it is the caller's own word. |
| `with_middleware(m)` | One more thing that every request goes through, before the middlewares of the description: a log, a limit of your own ([a middleware](extending.md#a-middleware)). |
| `with_transport(t)` | What really sends the requests, under the middlewares: `concord::Http` with settings of yours, a `reqwest::Client` of yours, a fake for a test, another HTTP library ([testing a client](testing.md), [a transport](extending.md#a-transport)). |

**The names that Concord keeps.** A child or an optional input becomes a method, so it may not have the name of a method that its receiver already has. These are exact names. On a call, they are Concord's own verbs (`send`, `send_raw`, `request`, `pages`, …) and `at_url`, which pagers use. On a client or a handle: `call` (kept for later) and `clone`; on a client also `new`. One prefix is kept, `with_`, and no other: Telegram's `send_message` is a name like any other. And Concord adds no name to a call, a handle or a client in a minor version, unless it starts with `with_`. A required input is an argument, and is free. A scope or an endpoint may not give a type named `Error`, `Result` or `Retry` in the module of its client. The whole table is [contracts](../reference/contracts.md) C1.6. And names that begin with `__` are Concord's: in a module that holds `concord::api!`, declare no item (a `const`, a `static`, a function, a type) whose name begins with `__`.

**Concord's own HTTP client is `concord::Http`:** `reqwest`, in a type of Concord's own. It waits at most 30 seconds for a connection, follows no redirect and adds no `Referer`. It keeps no cookies by default: a build of `reqwest` that has cookies may turn them on in `Http::with`. It reads the proxy settings of the environment. The exact settings: [Concord's own HTTP client](../reference/crates.md#concords-own-http-client).

A `3xx` is then an answer ([when a status means something](responses-and-errors.md#when-a-status-means-something); the other side of this choice: [scope and limits](../design/scope.md)).

**A setting of yours.** A certificate is a fact of the connection, so it is set on the HTTP client, with `reqwest`'s own builder:

```rust
let http = concord::Http::with(|builder| builder.add_root_certificate(private_authority))?;

let billing = Billing::new().with_transport(http.clone());
let other   = Other::new(token).with_transport(http);          // two clients share one engine
```

Concord's defaults go before your closure, and "no redirect" after it: nothing in the closure can make this client follow one.

**A `reqwest::Client` that you bring** is used as it is. What it does on its own is yours: its redirects, its cookies. Promise [G3](promises.md#three-promises) is stated for a client that follows no redirect. [The clocks](#time) are Concord's own, and hold for any client.

## Looking without sending

`request()` builds the request and does not send it. It gives the `reqwest::Request` itself: the method, the URL, the headers, the body.

```rust
let request = riot.matches(Europe).ids(&puuid).count(5).request()?;          // `puuid` is written as `p-1`
assert_eq!(request.url().as_str(), "https://europe.api.riotgames.com/lol/match/v5/matches/by-puuid/p-1/ids?count=5");
```

It is the request as the description builds it, with every value that the description places: a key or a token that is a `Secret` is in it, in a header marked sensitive. What a middleware adds on the way out is not in it: `request()` gives the request before any middleware, and a signature belongs to one try ([a middleware](credentials.md#a-middleware)). To see exactly what goes out, a fake takes the place of the transport and gets it ([a fake transport](testing.md#a-fake-transport)).

`request()` only looks: the call is whole after it, and can still be sent. (A call whose body is a stream has nothing to show without spending the stream, so `request()` refuses it; a fake transport is the way to look at such a call.) To take the request away, `into_request()` spends the call and gives it. To send it yourself and still get the typed answer: `read_response`, in [send a request yourself](extending.md#send-a-request-yourself-request-and-read_response).

## What comes back

A call gives `riot::Result<T>`: `T` is the type after the arrow, and the error is `riot::Error` ([the API's own error](responses-and-errors.md#the-apis-own-error)).

The error has four kinds:

| Kind | Meaning | The caller |
|---|---|---|
| Build | The request could not be built. Nothing was sent. | Fixes the input. |
| Transport | No answer, or an answer that broke before the end of a body that was wanted. `class()` says which: connect, timeout, body, blocked, other. | May try again. |
| Refused | The API said no: a status that the endpoint gives no meaning to (by default, anything that is not 2xx), or an arm ended with the API's own problem. | Looks at the status, and at `problem()`. |
| Mismatch | The status is one the endpoint gives a meaning to, and the answer is not what the declaration says: the body is too large, it does not decode, an arm or a pager refused it, or it is a success that no arm takes where the endpoint has no simple rule ([rule 11](../reference/rules.md)). | Reports it. |

**What an error holds.**

| | |
|---|---|
| `kind()`, `endpoint()`, `tries()` | The kind; the name of the endpoint (`riot.matches.get`); how many tries were made: a try is counted when the request is handed to the middlewares. |
| `status()`, `is_status(404)`, `headers()` | What the server said, when an answer arrived. A request id for the provider's support is in the headers. |
| `body()`, `kept()` | The start of the body, at most 64 KiB, and its state ([what an error keeps of a body](responses-and-errors.md#what-an-error-keeps-of-a-body)). |
| `problem()` | The API's own error, typed ([the API's own error](responses-and-errors.md#the-apis-own-error)). |
| `class()`, `is_timeout()` | For a Transport error. |
| `source()` | The cause, when there is one. It never holds a value of the answer. For a Transport error: the error of the HTTP library, with its URL taken out. For a body that did not read: a small cause that Concord makes from the place and the kind of failure. For a Build error that came from your own code (a body, a `?` in an entry): that failure. |
| `retry_after()` | The longer of two waits, when there is one: the `Retry-After` of the answer, and the wait that a rule of `when` asked for the try that did not happen. |
| `reason()` | A text that the author's code gave (`concord::mismatch("…")`): why a pager stopped, why an arm refused. For a body that did not read, it is `None`. |
| `details()` | One text to print, with the values: the line that `{e}` prints, the reason, the headers, the API's own error or else the start of the body, the reader's own message (it can quote the body; at most 1 KiB), and the inner error. It is there when the error type has `Debug`. |

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

**Printing.** `{e}` is one line in Rust's own style: the kind, the status, the name of the endpoint, how many tries, and where it can a name for the cause. It never shows a value: not the body, not a URL, not a secret, not a text that your own code gave ([rule 15](../reference/rules.md); the texts are in [the messages](../reference/messages.md)). `e.details()` is the other choice, made by you at the place where you print. A URL can be in there.

**The whole answer.** A call gives the value, which is what most calls want. When the head matters, a second form keeps it:

```rust
let answer = docs.get(&id).send_full().await?;
let etag   = answer.headers().get("etag").map(String::from);   // for the `If-Match` of the next call
let status = answer.status();                           // 200, 201, …
let doc    = answer.into_value();                       // the same value `.await` gives; `answer.value()` borrows it
```

**Only if it changed.** On a call whose method is `GET` or `HEAD`, `send_if_none_match(etag)` takes a `&str` and writes `If-None-Match`, in place of any that the description wrote ([rule 20](../reference/rules.md)). It gives an `Answer<Option<T>>`: on a `304` that no arm takes, the value is `None`, and the headers are there (a new `ETag`, `Cache-Control`).

```rust
let answer = docs.get(&id).send_if_none_match(&etag).await?;
match answer.into_value() {
    Some(doc) => …,                                     // it changed
    None => …,                                          // 304: keep the one you have
}
```

**A call with pages.** An endpoint with `paginate` gives a call that is not awaited directly, so that one page is never taken for the whole list. `.send().await?` gives one page, on purpose. `.pages()` gives them all, one for each `try_next()`, and `.pages().items()` gives their items one by one ([pages](pages.md)). `.items()` is there when the type of a page has `concord::page::IntoItems`: a `Vec<T>` has it, and an author writes it for a list type of their own ([a page type of your own](pages.md#a-page-type-of-your-own-intoitems)).

## Time

Two clocks, each with one job. This is their one full description; other pages link here.

| | What it limits | With nothing set | You set it |
|---|---|---|---|
| `idle_timeout` | Silence. It watches two things: the transport, from the moment it gets the request until the head of the answer; and a body while it is read. Each piece that moves starts it again. | 30 seconds. | `with_idle_timeout(…)`, on the client or on a call. An author sets it in the description, for the calls that need another one (`idle_timeout`). |
| `timeout` | The whole call: every try, every wait between tries, the reading of the body. No request is handed to the transport after it has ended. | No limit. | `with_timeout(…)`, on the client or on a call. |

Setting one never changes the other.

```rust
// long polling: Telegram holds the request for up to 50 seconds, on purpose.
// The description says how long this endpoint may be silent, so the caller writes nothing
let updates = bot.get_updates().offset(next).timeout(50).await?;

// a download: ten minutes in all, and still a quick stop if it stalls
let file = files.download(&id).with_timeout(Duration::from_secs(600)).await?;
```

In the first call, `.timeout(50)` is Telegram's own parameter. The clock for silence is the first of these that is set: `with_idle_timeout` on the call, the `idle_timeout` of the description, `with_idle_timeout` on the client, 30 seconds.

**An endpoint that is silent on purpose says so.** A long poll, or a stream that is quiet for minutes, is a fact of an endpoint, and its callers should not have to know it. `idle_timeout <expression>;` stands on an endpoint, a scope or the client (a part may hold it too), and the nearest one is used. The expression is Rust with the inputs as variables, and gives a `std::time::Duration`: Telegram's `get_updates` is written out in [the block](describing-an-api.md#the-block).

**The clock for silence watches two things, and nothing else.** The transport, from the moment it gets the request until the head of the answer: each piece of a stream body on its way out starts the clock again. And a body while it is read: each piece starts it again. A middleware's time, before or after the transport, and a reader's own waits are never silence: only `timeout` bounds them. It is Concord's own clock, so it holds for any HTTP client.

| | |
|---|---|
| When time is up | The call ends with a Transport error. It says which clock ended, and what to write to change it ([the messages](../reference/messages.md); [a timeout](troubleshooting.md#the-call-failed)). |
| A refusal is not lost to a clock | When a clock ends a try whose status already said no, the result is that Refused error, and `kept()` says that its body was interrupted. |
| A large upload | A body that is a stream counts as progress: each piece of it that goes out starts the clock again. A body held in memory goes out in one piece: a large one on a slow line needs a longer `idle_timeout` on that call. |
| A stream that was handed over | `timeout` goes on until its end. The clock for silence runs while a `try_next()` waits, not between two of them. |
| Your own code | A clock ends a wait; it cannot stop code that does not `.await`. |
| After `send_raw()` | Concord's part ends at the head: the `reqwest::Response` is yours, and so is the time to read it. Neither clock follows it. |
| A drop | A call that is dropped stops, and Concord starts no other try. What the HTTP library does with the connection is its own: the request may have reached the server. |

### The steps of one call

| Step | What happens | If it fails |
|---|---|---|
| 1 | The expressions of the description run: `idle_timeout`, then `idempotent`, then the request is built from the inputs, by kind (the base, the paths, the query, the headers, the body). Then the caller's `.edit_request(…)` changes it. This happens once for a send, and each page is one send. | Build error. Nothing was sent. |
| 2 | For each try, a copy goes through the middlewares: the caller's, then the description's in the order written (one writes a token, the last one signs). Then it is handed to the transport, which sends it. | No answer: the try ended. A middleware or a transport that will not send: no more tries, and the result is that blocked error. |
| 3 | The head arrives. The status picks an arm ([the layers](responses-and-errors.md#the-layers)), the reader reads the body if the arm binds it, and the arm gives the result. | A Refused error (also when an arm on a status that is not a 2xx binds a `body` that does not read), a Mismatch, or the try ended if the body broke. |
| 4 | Retry looks at how the try ended ([retries](retries.md)). | Another try from step 2, or the result. |

The other expressions have their own moment: `read` runs for each answer, the expression of `paginate` once, at `.pages()`, and a `middleware` expression once, in `new` ([when an expression runs](../reference/grammar.md#when-an-expression-runs)). Inside a kind, the order is the order of the tree ([rules](../reference/rules.md), rule 10).

The reading step alone is `call.read_response(response)`: the arms, the reader and the problem type of the endpoint, on an answer that you got yourself with `call.request()?` ([send a request yourself](extending.md#send-a-request-yourself-request-and-read_response)). It has no retry, no clock and no page, and it takes `&self`: a call can read several answers.

How the engine holds the two clocks is on [the engine page](../internals/engine.md).

## When the usual way is not enough

Every layer has a way down, for the case that the layer does not cover: a reader, a writer of bodies or a pager of yours in the description; `edit_request`, `send_raw`, `request()` and `read_response` on a call; a middleware or a transport on the client; and below all of them, plain `reqwest`. Each one, with code to copy, is on [when Concord does not say what you need](extending.md).

## A test with no network

`with_transport(concord::fake(|request| …))` puts a function in place of the network: it gets the request as it would go out, and gives the answer. Everything else of Concord runs for real.

A fake, a scripted transport on virtual time, a local server, and the lines a test needs in `Cargo.toml`: [testing a client](testing.md).

## The surface of a client

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
    pub fn pages(self) -> concord::Pages<Vec<MatchId>, RiotProblem>;    // because of `paginate`; `.pages().items()` gives each `MatchId`
    // send_if_none_match(etag): an Answer<Option<Vec<MatchId>>>, because the method is `GET`
    pub fn request(&self) -> riot::Result<reqwest::Request>;         // look
    pub fn into_request(self) -> riot::Result<reqwest::Request>;      // take
    pub async fn read_response(&self, response: reqwest::Response) -> riot::Result<Vec<MatchId>>;   // an answer that you got yourself
    pub fn send_raw(self) -> concord::Sending<reqwest::Response, RiotProblem>;
    // with_retry, with_timeout, with_idle_timeout, edit_request
}
```

Generated types follow the tree, and a scope may have the name of one of your own types: `riot::Account` is the handle of the scope `account`, and `Account` is still your struct ([rule 24](../reference/rules.md)). All of it has the visibility written on the client. Your own types and imports are seen as they are seen where `api!` stands; how the macro does it is on [the macro page](../internals/macro.md#where-the-generated-items-live-and-how-they-reach-the-authors-types). In a crate that is itself named `riot`, write `use ::riot::*;`.

## Arguments

The trait behind U4 and U5:

```rust
/// What an argument for an input of type `T` may be. Closed: only Concord implements it.
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

**An optional input whose type is an `Option`** has three states ([a signature](describing-an-api.md#a-signature)). For `milestone?: Option<u64>`: `.milestone(5)` and `.milestone(Some(5))` give the number, and `.milestone(None::<u64>)` gives "nothing". A bare `None` does not compile there: Rust cannot tell which of the two is meant. The setter for `name?: Option<String>` takes text too. A setter converts through these two levels and no more: for `n?: Option<Option<u64>>` it takes the exact value.

**The set is closed.** A type from outside cannot add a conversion: for `puuid: Puuid`, `ids("p-1")` does not compile. The caller writes `Puuid::from("p-1")`, or the author declares `puuid: String`. A general "anything that converts" (`impl Into<T>`) does not work for a plain number: for an input declared `u32`, `.count(5)` would not compile.

## The documentation of an endpoint

Each generated method is documented from the description: your own `///` text, then how the endpoint is called, its request line (the method, the URL with its holes, the keys of the query), the headers it writes, and what each input accepts. Everything is produced from the declaration, so it cannot go stale. It is also the quickest answer to "is my description right?": the request line is there to read, to compare with the provider's documentation. The inputs of a part are shown like the endpoint's own. The page `concord::sample`, in the documentation of the crate, is a small client written by the macro, as rustdoc shows it.
