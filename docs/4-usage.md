# 4. The usage

*Concord V5 design · chapter 4 of 9 · [index](../README.md) · previous: [3. The language](3-language.md) · next: [5. The response](5-response.md)*

How a generated client is called, built and tested, what comes back, how long it waits, and how to step down a layer.

## 4.1 The sketch

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

## 4.2 The rules of use

| # | Rule |
|---|---|
| U1 | One level of the tree is one method call. |
| U2 | Required values are the arguments of that call, in the order of the declaration. |
| U3 | A value that is not required (marked `?` in the description) has one method, with exactly the name of the value. No prefix. |
| U4 | An argument is the declared type, or a reference to it (`&puuid`; it is cloned inside). Text (`"Faker"`) is also accepted where a `String` or a `Secret` is declared, and an array or a slice where a `Vec` is declared. Nothing else is: for `puuid: Puuid`, `ids("p-1")` does not compile (section 4.10). |
| U5 | The method of an optional input takes what an argument of its type takes, or an `Option` of it. `None` un-sets it: `.count(5).count(None)` sends nothing. |
| U6 | `.await` sends: one request, or a few when retry is set. `send()` is the same thing as a method. A call to an endpoint with pages is not awaited directly: `.send()` gives one page, on purpose, and `.pages()` gives them all (section 4.5). |
| U7 | A handle and a call own their data. Hold them, clone them, move a call to another task. |
| U8 | Nothing is ever reset. A call is built, sent, and gone. |
| U9 | A required value has no method to change it later. For another route, make another handle. The one thing that changes in place is a `Secret`: `key.replace(…)` ([chapter 6](6-behaviour.md), section 6.2). |

## 4.3 The client

```rust
let riot = Riot::new(key);                 // the usual case: Concord's own HTTP client, with safe settings
```

`new` takes the required inputs of the client. An optional input of the client is a setter on the client, as it is on a call: GitHub's `token?: Secret` is `Github::new("my-tool/1.0").token(token)`.

**A `Secret` from text.** Where a `Secret` is declared, text is accepted, as a `&str` or a `String`: `Riot::new("RGAPI-…")`, `Riot::new(std::env::var("RIOT_KEY")?)`. `Secret::new(text)` makes one that you hold (U9).

Six more methods are on every client. Each returns the client, changed. They all start with `with_`, and so does every setting of Concord on a call: **an option of the API keeps its own name, a setting of Concord starts with `with_`.**

| Method | For |
|---|---|
| `with_retry(retry)` | Trying again. Off until this is called. A policy is `riot::Retry::standard()` (or `riot::Retry::none()`, which has no rule of its own), with `.tries(n)`, `.when(f)` and `.unless(f)`: `riot::Retry` is `concord::Retry<RiotProblem>`, so a rule reads the API's own error, typed. `.not_applied_on(429)` lists a status after which a refused request may be repeated whatever its method: a `429` on a `POST`. It never panics: a number outside 100 to 999 is kept and matches no answer. An author may ship the policy of the API (`slack::retry()`), and the caller names it. On a call, a policy replaces the whole policy of the client for that call ([chapter 6](6-behaviour.md), section 6.3). |
| `with_timeout(duration)` | The longest time for a whole call. No limit until this is called (section 4.6). |
| `with_idle_timeout(duration)` | How long the server may say nothing. 30 seconds until this is called (section 4.6). |
| `with_base("http://localhost:8080")` | Every request goes there, whatever the `base` lines say: a local test server, a gateway, another deployment. It may have a path prefix (`"https://example.com/gitlab"`). Every entry follows it, a secret too: it is the caller's own word. |
| `with_middleware(m)` | One more thing that every request goes through, before the middlewares of the description: a log, a limit of your own (section 4.8). |
| `with_transport(t)` | What really sends the requests, under the middlewares: `concord::Http` with settings of yours, a `reqwest::Client` of yours, a fake for a test, another HTTP library (section 4.8). |

**The names that Concord keeps.** A child or an optional input becomes a method, so it may not have the name of a method that its receiver already has. These are exact names. On a call, they are Concord's own verbs (`send`, `send_raw`, `request`, `pages`, …) and `at_url`, which pagers use. On a client or a handle: `call` (kept for later) and `clone`; on a client also `new`. One prefix is kept, `with_`, and no other: Telegram's `send_message` is a name like any other. And Concord adds no name to a call, a handle or a client in a minor version, unless it starts with `with_`. A required input is an argument, and is free. A scope or an endpoint may not give a type named `Error` or `Result` in the module of its client. The whole table is [contracts](contracts.md) C1.6. And names that begin with `__` are Concord's: in a module that holds `concord::api!`, declare no item (a `const`, a `static`, a function, a type) whose name begins with `__`.

**Concord's own HTTP client is `concord::Http`:** `reqwest`, in a type of Concord's own. It waits at most 30 seconds for a connection, follows no redirect and adds no `Referer`. It keeps no cookies by default: a build of `reqwest` that has cookies may turn them on in `Http::with`. It reads the proxy settings of the environment. The exact settings are in the [engine note](engine.md), section 6.

A `3xx` is then an answer ([chapter 5](5-response.md), section 5.5; the other side of this choice: [chapter 8](8-scope.md), section 8.2).

**A setting of yours.** A certificate is a fact of the connection, so it is set on the HTTP client, with `reqwest`'s own builder:

```rust
let http = concord::Http::with(|builder| builder.add_root_certificate(private_authority))?;

let billing = Billing::new().with_transport(http.clone());
let other   = Other::new(token).with_transport(http);          // two clients share one engine
```

Concord's defaults go before your closure, and "no redirect" after it: nothing in the closure can make this client follow one.

**A `reqwest::Client` that you bring** is used as it is. What it does on its own is yours: its redirects, its cookies. Promise G7 is stated for a client that follows no redirect. The clocks of section 4.6 are Concord's own, and hold for any client.

## 4.4 Looking without sending

`request()` builds the request and does not send it. It gives the `reqwest::Request` itself: the method, the URL, the headers, the body.

```rust
let request = riot.matches(Europe).ids(&puuid).count(5).request()?;          // `puuid` is written as `p-1`
assert_eq!(request.url().as_str(), "https://europe.api.riotgames.com/lol/match/v5/matches/by-puuid/p-1/ids?count=5");
```

It is the request as the description builds it, with every value that the description places: a key or a token that is a `Secret` is in it, in a header marked sensitive. What a middleware adds on the way out is not in it: `request()` gives the request before any middleware, and a signature belongs to one try ([chapter 6](6-behaviour.md), section 6.2). To see exactly what goes out, a fake takes the place of the transport and gets it (section 4.8).

`request()` only looks: the call is whole after it, and can still be sent. (A call whose body is a stream has nothing to show without spending the stream, so `request()` refuses it; a fake transport is the way to look at such a call.) To take the request away, `into_request()` spends the call and gives it. To send it yourself and still get the typed answer: `read_response`, in section 4.7.

## 4.5 What comes back

A call gives `riot::Result<T>`: `T` is the type after the arrow, and the error is `riot::Error` (section 5.6).

The error has four kinds:

| Kind | Meaning | The caller |
|---|---|---|
| Build | The request could not be built. Nothing was sent. | Fixes the input. |
| Transport | No answer, or an answer that broke before the end of a body that was wanted. `class()` says which: connect, timeout, body, blocked, other. | May try again. |
| Refused | The API said no: a status that the endpoint gives no meaning to (by default, anything that is not 2xx), or an arm ended with the API's own problem. | Looks at the status, and at `problem()`. |
| Mismatch | The status is one the endpoint gives a meaning to, and the answer is not what the declaration says: the body is too large, it does not decode, an arm or a pager refused it, or it is a success that no arm takes where the endpoint has no simple rule ([rule 11](rules.md)). | Reports it. |

**What an error holds.**

| | |
|---|---|
| `kind()`, `endpoint()`, `tries()` | The kind; the name of the endpoint (`riot.matches.get`); how many tries were made: a try is counted when the request is handed to the middlewares. |
| `status()`, `is_status(404)`, `headers()` | What the server said, when an answer arrived. A request id for the provider's support is in the headers. |
| `body()`, `kept()` | The start of the body, at most 64 KiB, and its state (section 5.7). |
| `problem()` | The API's own error, typed (section 5.6). |
| `class()`, `is_timeout()` | For a Transport error. |
| `source()` | The cause, when there is one. It never holds a value of the answer. For a Transport error: the error of the HTTP library, with its URL taken out. For a body that did not read: a small cause that Concord makes from the place and the kind of failure. For a Build error that came from your own code (a body, a `?` in an entry): that failure. |
| `retry_after()` | The longer of two waits, when there is one: the `Retry-After` of the answer, and the wait that a rule of `when` asked for the try that did not happen. |
| `reason()` | A text that the author's code gave (`concord::mismatch("…")`): why a pager stopped, why an arm refused. For a body that did not read, it is `None`. |
| `details()` | One text to print, with the values: the line that `{e}` prints, the reason, the headers, the API's own error or else the start of the body, the reader's own message (it can quote the body; at most 1 KiB), and the inner error. It is there when the error type has `Debug`. |

```rust
match asia.get(id).await {
    Ok(game) => …,
    Err(e) if e.is_status(403) => {
        let problem = e.problem();                           // the API's own error, typed: see 5.6
        …
    }
    Err(e) => {
        tracing::warn!("{e}");                               // names only: safe in any log
        tracing::debug!("{}", e.details());              // everything, by your choice
        return Err(e.into());
    }
}
```

**Printing.** `{e}` is one line in Rust's own style: the kind, the status, the name of the endpoint, how many tries, and where it can a name for the cause. It never shows a value: not the body, not a URL, not a secret, not a text that your own code gave ([rule 15](rules.md); the texts are in the [messages sheet](messages.md), section 2). `e.details()` is the other choice, made by you at the place where you print. A URL can be in there.

**The whole answer.** A call gives the value, which is what most calls want. When the head matters, a second form keeps it:

```rust
let answer = docs.get(&id).send_full().await?;
let etag   = answer.headers().get("etag").map(String::from);   // for the `If-Match` of the next call
let status = answer.status();                           // 200, 201, …
let doc    = answer.into_value();                       // the same value `.await` gives; `answer.value()` borrows it
```

**Only if it changed.** On a call whose method is `GET` or `HEAD`, `send_if_none_match(etag)` takes a `&str` and writes `If-None-Match`, in place of any that the description wrote ([rule 20](rules.md)). It gives an `Answer<Option<T>>`: on a `304` that no arm takes, the value is `None`, and the headers are there (a new `ETag`, `Cache-Control`).

```rust
let answer = docs.get(&id).send_if_none_match(&etag).await?;
match answer.into_value() {
    Some(doc) => …,                                     // it changed
    None => …,                                          // 304: keep the one you have
}
```

**A call with pages.** An endpoint with `paginate` gives a call that is not awaited directly, so that one page is never taken for the whole list. `.send().await?` gives one page, on purpose. `.pages()` gives them all, one for each `try_next()`, and `.pages().items()` gives their items one by one ([chapter 6](6-behaviour.md), section 6.4). `.items()` is there when the type of a page has `concord::page::IntoItems`: a `Vec<T>` has it, and an author writes it for a list type of their own ([chapter 6](6-behaviour.md), section 6.4, shows it in full).

## 4.6 Time

Two clocks, each with one job.

| | What it limits | With nothing set | You set it |
|---|---|---|---|
| `idle_timeout` | Silence. It watches two things: the transport, from the moment it gets the request until the head of the answer; and a body while it is read. Each piece that moves starts it again. | 30 seconds. | `with_idle_timeout(…)`, on the client or on a call. An author sets it in the description, for the calls that need another one (`idle_timeout`). |
| `timeout` | The whole call: every try, every wait between tries, the reading of the body. | No limit. | `with_timeout(…)`, on the client or on a call. |

Setting one never changes the other.

```rust
// long polling: Telegram holds the request for up to 50 seconds, on purpose.
// The description says how long this endpoint may be silent, so the caller writes nothing
let updates = bot.get_updates().offset(next).timeout(50).await?;

// a download: ten minutes in all, and still a quick stop if it stalls
let file = files.download(&id).with_timeout(Duration::from_secs(600)).await?;
```

In the first call, `.timeout(50)` is Telegram's own parameter. The clock for silence is the first of these that is set: `with_idle_timeout` on the call, the `idle_timeout` of the description, `with_idle_timeout` on the client, 30 seconds.

| | |
|---|---|
| When time is up | The call ends with a Transport error. It says which clock ended, and what to write to change it ([messages sheet](messages.md), section 2). |
| A large upload | A body that is a stream counts as progress: each piece of it that goes out starts the clock again. A body held in memory goes out in one piece: a large one on a slow line needs a longer `idle_timeout` on that call. |
| A stream that was handed over | `timeout` goes on until its end. The clock for silence runs while a `try_next()` waits, not between two of them. |
| Your own code | A clock ends a wait; it cannot stop code that does not `.await`. A middleware's time, before or after the transport, and a reader's own waits are never silence: only `timeout` bounds them. |
| After `send_raw()` | Concord's part ends at the head: the `reqwest::Response` is yours, and so is the time to read it. Neither clock follows it. |

How the engine holds the two clocks is in the [engine note](engine.md), section 4.

## 4.7 The doors

Every layer has a way down, for the case that the layer does not cover. This is what "full control" means here.

| Layer | The usual way | The door |
|---|---|---|
| The description | The placement words | A Rust value of yours behind `body`, `read`, `paginate`, `middleware` or an arm. Your own methods beside the generated ones: `impl riot::Matches { … }` in the crate that holds the description, or a trait of yours for `riot::Matches` in any other crate. |
| An endpoint that is not described | — | Add it to the description, or write your own method beside the generated ones. (A raw call on a handle is not in 5.0; the name `call` is kept for it.) |
| One call | `.await` | `.edit_request(\|request\| …)` changes the built `reqwest::Request` before it is handed to the transport. `.with_retry(…)`, `.with_timeout(…)` and `.with_idle_timeout(…)` for this call only. `request()` to look. |
| A request that you send yourself | — | `request()`, then `read_response(response)`. |
| The answer | A typed value | `send_full()`. `send_if_none_match(etag)`. `send_raw()`: the `reqwest::Response` after retry, with no status rule and nothing read (retry then looks at the head only). `read Stream;` for bytes as they come. |
| The client | `Riot::new(key)` | `with_middleware(…)` for what every request goes through. `with_transport(…)` for what really sends, and `concord::Http::with(…)` for a setting of the HTTP client. `with_base(…)`. |
| Below | — | It is plain `reqwest`: `Request`, `Response`, `Client`. |

`.edit_request` takes a function `Fn(&mut reqwest::Request)`. It cannot fail. It runs once for each request that is built: once for a call, once for each page. Several `.edit_request` on one call all run, in the order they were written; a later one sees what an earlier one did. A header value that such a function marks sensitive is left out, like a secret, when two pages are compared ([engine note](engine.md), section 7).

**`read_response` is the way back up from `request()`.** It runs the arms, the reader and the problem type of the endpoint on a `reqwest::Response` that came from anywhere. Concord sent nothing: no retry, no clock, no page ([engine note](engine.md), section 8).

```rust
let call = riot.matches(Europe).get(&id);
let response = my_client.execute(call.request()?).await?;
let game = call.read_response(response).await?;          // the same value that `call.await?` gives
```

The doors are public and stable: the traits `Reader`, `Middleware` and `Transport`, a body as a function that gives a `Body` (`Body::concat(parts)` makes one body of several), a pager as a function (with `Slot` and `Call`), a page type of yours (with `IntoItems`), `edit_request`, `into_request`, `read_response`. A pager, or a function used in an arm, that can fail names one type for its error: `concord::Fail`. `concord::mismatch(…)` gives one. What Concord ships is made with these doors and the public helpers beside them (`concord::body::Bounded` for a writer, `concord::read::details` for a reader, `concord::page::fragmentless` for a pager): `json`, `Json`, `by_offset` and the others are in a crate of their own, `concord-extras`. Only its `tower` adapter also uses a hidden helper of the engine, for the URL of an answer. An upload in parts is there too, under the feature `multipart`: `multipart().text("chat_id", id).file("photo", "a.jpg", "image/jpeg", photo).finish()` gives a `Result` of a `Body` ([chapter 5](5-response.md), section 5.8). [The crate sheet](crate.md) is the map.

## 4.8 A test with no network

```rust
let riot = Riot::new("test-key").with_transport(concord::fake(|request| {
    assert_eq!(request.url().host_str(), Some("europe.api.riotgames.com"));
    Ok(concord::reply(200, r#"{"puuid":"abc","gameName":"Faker","tagLine":"KR1"}"#))
}));

let account = riot.account(Europe).by_riot_id("Faker", "KR1").await?;
```

The fake takes the place of what really sends. It gets the request as it leaves, after the middlewares: for a signed API, the signed one. It gives a response, or a failure with its class. Everything of Concord runs for real on that answer: retry, the status rule, the arms, the reader, the clock. Only the network is replaced.

Behind it is one trait with one method:

```rust
pub trait Transport: Send + Sync + 'static {
    async fn send(&self, request: reqwest::Request, info: TryInfo) -> Result<reqwest::Response, TransportError>;
}
```

`concord::Http` and `reqwest::Client` have it, a function wrapped by `concord::fake` has it, and a type of yours can have it: another HTTP library. **A transport is what really sends.**

**What a request goes through on the way is a middleware.** The author of an API writes the ones of the API in the description ([chapter 3](3-language.md), section 3.9). A caller adds one of their own:

```rust
let riot = Riot::new(key).with_middleware(RequestLog::new());     // a type of yours with `concord::Middleware`
```

It comes before the ones of the description. The author's are the last before the wire, so nothing changes a request after it was signed. A middleware sees every request and every answer, also each try and each page. So it is the place for what Concord leaves outside: counting requests ahead of a quota, reading a quota header on every answer, recording traffic. Nothing takes a middleware away, and `with_transport` changes only what is under them.

With each request, a middleware and a transport are told the name of the endpoint and the number of the try (`info`), so they can count by endpoint and tell a first try from a later one. `info.endpoint()` is the names of the tree joined with `.` (`riot.matches.ids`), the same text as in a printed error. One that will not send (a budget that is empty) answers `TransportError::blocked()`: the tries end there, and the call ends with that error.

With the feature `tower`, a `tower` service is a transport (`concord::tower::Tower::new(stack)`): its layers, a rate limit or a limit of concurrent requests, then sit under the client.

A fake can also fail before any head, or give a body that breaks halfway or never ends: a refused connection, a cut body and a silent server are tested without a socket.

## 4.9 The Riot surface

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

Generated types follow the tree, and a scope may have the name of one of your own types: `riot::Account` is the handle of the scope `account`, and `Account` is still your struct ([rule 24](rules.md)). All of it has the visibility written on the client. How a generated type reaches your own types and imports is in the [macro note](macro.md), section 3. In a crate that is itself named `riot`, write `use ::riot::*;`.

## 4.10 Arguments

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

**An optional input whose type is an `Option`** has three states ([chapter 3](3-language.md), section 3.3). For `milestone?: Option<u64>`: `.milestone(5)` and `.milestone(Some(5))` give the number, and `.milestone(None::<u64>)` gives "nothing". A bare `None` does not compile there: Rust cannot tell which of the two is meant. The setter for `name?: Option<String>` takes text too. A setter converts through these two levels and no more: for `n?: Option<Option<u64>>` it takes the exact value.

**The set is closed.** A type from outside cannot add a conversion: for `puuid: Puuid`, `ids("p-1")` does not compile. The caller writes `Puuid::from("p-1")`, or the author declares `puuid: String`. A general "anything that converts" (`impl Into<T>`) does not work for a plain number: for an input declared `u32`, `.count(5)` would not compile.

## 4.11 The page of one endpoint

```text
pub fn ids(&self, puuid: Puuid) -> riot::matches::Ids

Match ids of a player, newest first.

Options
    .start(u32)          not sent unless given
    .count(u32)          not sent unless given
    .queue(i32)          not sent unless given
    .start_time(i64)     not sent unless given

Returns
    Vec<MatchId>         in pages: .send() for one page, .pages() for all

Request
    GET https://{route}.api.riotgames.com/lol/match/v5/matches/by-puuid/{puuid}/ids
        ?start&count&queue&startTime
```

The first lines are for the user. The last block is for someone who compares with the provider's documentation. Everything is produced from the declaration, so it cannot go stale. It is also the quickest answer to "is my description right?": the request line is there to read. The inputs of a part are shown like the endpoint's own. The rules for this text are in the [macro note](macro.md), section 9.

(Written by hand in [`api/tour`](../api/tour/src): the macro is not built.)
