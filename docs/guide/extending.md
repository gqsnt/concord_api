# When Concord does not say what you need

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [What Concord promises](promises.md) · next: [Troubleshooting](troubleshooting.md)*

For the author or the user of a client who needs something that the description cannot say: another format, another way to page, a signature, another HTTP library, one request changed by hand. Every layer has a way down, and each way down is plain Rust. This page gives each one, with code to copy.

## Every layer has a way down

| Layer | The usual way | The door |
|---|---|---|
| The description | The placement words | A Rust value of yours behind `body`, `read`, `paginate`, `middleware` or an arm. Your own methods beside the generated ones. |
| An endpoint that is not described | — | Add it to the description, or write your own method beside the generated ones. (There is no raw call on a handle; the name `call` is reserved for it.) |
| One call | `.await` | `.edit_request(\|request\| …)` changes the built `reqwest::Request` before it is handed to the transport. `.with_retry(…)`, `.with_timeout(…)` and `.with_idle_timeout(…)` for this call only. `request()` to look. |
| A request that you send yourself | — | `request()`, then `read_response(response)`. |
| The answer | A typed value | `send_full()`. `send_if_none_match(etag)`. `send_raw()`: the `reqwest::Response` after retry, with no status rule and nothing read (retry then looks at the head only). `read Stream;` for bytes as they come. |
| The client | `Riot::new(key)` | `with_middleware(…)` for what every request goes through. `with_transport(…)` for what really sends, and `concord::Http::with(…)` for a setting of the HTTP client. `with_base(…)`. |
| Below | — | It is plain `reqwest`: `Request`, `Response`, `Client`. |

These doors are public and stable: the traits `Reader`, `Middleware` and `Transport`, a body as a function that gives a `Body`, a pager as a function (with `Slot` and `Call`), a page type of yours (with `IntoItems`), `edit_request`, `into_request`, `read_response`. What Concord ships is made with the same doors: `json`, `Json`, `by_offset` and the others are in a crate of their own, `concord-extras`, and each of its files but `tower.rs` is an example to read. (Only the `tower` adapter also uses a hidden helper of the engine, for the URL of an answer; that helper is not a door.)

## A writer of bodies

A writer is a function that gives a `concord::Body`, or a `Result` of one. The description names it after `body`, as it names `json`:

```rust
// plain Rust, beside the macro: `body csv(rows);`
fn csv(rows: &[(u32, &str)]) -> concord::Body {
    let text: String = rows.iter().map(|(id, name)| format!("{id},{name}\n")).collect();
    concord::Body::new(text, "text/csv")
}
```

A writer that takes anything that `serde` can write also takes a body made of entries: `body xml { a; b; }` calls `xml` with the entries. To fail as `json` and `form` fail (the input named in the Build error, at most 128 nested levels for each entry), wrap the value in `concord::body::Bounded` before the format sees it, and turn the format's error into a `concord::body::WriteError`:

```rust
use concord::body::{Bounded, WriteError};

/// `body xml(task);` or `body xml { title; author; }`.
pub fn xml<T: serde::Serialize>(value: T) -> Result<concord::Body, WriteError> {
    let value = Bounded::new(&value);
    let text = quick_xml::se::to_string(&value).map_err(|error| value.error(error))?;
    Ok(concord::Body::new(text, "application/xml"))
}
```

`value.error(e)` takes the `Bounded` by value: the name of the input goes into one error only. To put bodies together, `Body::concat(parts)` gives one body made of several, in their order; `body.as_bytes()` gives the bytes of a body that is in memory, and `body.into_stream()` takes any body as a stream. What a body may give, and how it is sent again, is in [a body that is sent](responses-and-errors.md#a-body-that-is-sent). The shipped writers are in [`concord-extras/src/body.rs`](../../concord-extras/src/body.rs) (`json` is three lines) and [`multipart.rs`](../../concord-extras/src/multipart.rs).

## A reader

A reader is a type with one trait, `concord::Reader<T>`. The description names it after `read`, on the client or on one endpoint:

```rust
use concord::read::{Incoming, ReadError, Reader};

/// `read Upper;` for an endpoint whose arrow is `-> String`.
#[derive(Clone, Copy)]
struct Upper;

impl Reader<String> for Upper {
    async fn read(&self, body: Incoming) -> Result<String, ReadError> {
        let bytes = body.bytes(concord::MIB).await?;
        match std::str::from_utf8(&bytes) {
            Ok(text) => Ok(text.to_uppercase()),
            Err(e) => Err(ReadError::not_the_type(bytes).with_details(e.to_string())),
        }
    }
}
```

`body.bytes(limit)` takes the whole body, up to a limit; `body.stream()` hands it over as it comes. `?` on what `bytes` gives is already the right error. A decoder's message can quote the body, so it goes in `with_details`, which keeps 1 KiB of it for `e.details()` (`concord::read::details` cuts a text the same way). A reader for a whole format, XML, with every piece of `Incoming` and `ReadError` explained, is in [reading a success](responses-and-errors.md#reading-a-success). The shipped readers are in [`concord-extras/src/read.rs`](../../concord-extras/src/read.rs).

## A pager

A pager is a function from the call that was answered and its answer to the next call. In `paginate`, an input of the endpoint is given as its place on the call, a `Slot`: the pager reads it on the call that was answered, and sets it on the next one.

```rust
use concord::page::{Call, IntoItems, Slot};
use concord::Answer;

/// The next page starts one page further: `paginate next_page(page);`.
pub fn next_page<C: Call, S: Slot<C, Value = u32>>(page: S) -> impl Fn(C, &Answer<C::Value>) -> Option<C>
where
    C::Value: IntoItems,
{
    move |call, answer| {
        if answer.value().count() == 0 {
            return None;
        }
        let at = page.get(&call).copied().unwrap_or(1);
        Some(page.set(call, at + 1))
    }
}
```

A pager gives `Some(next call)`, or `None` at the end. A pager that can refuse a page gives a `Result` whose error is a `concord::Fail` (the same type that a function used in an arm fails with; `concord::mismatch("…")` makes one): a page that says "there is more" and gives no cursor is an error, not the end. That pager, `more`, is written out in [a pager of your own](pages.md#a-pager-of-your-own), with what `Call::at_url` does for an API that hands out addresses. To take the fragment off a next URL, the shipped pagers use `concord::page::fragmentless`, which is public too. They are in [`concord-extras/src/page.rs`](../../concord-extras/src/page.rs).

**A list type of yours as a page.** `.pages().items()` and the counting pagers (`by_offset`, `by_offset_limit`, `by_page`) ask one trait of the type after the arrow, `concord::page::IntoItems`: `into_items` and `count`, a few lines. It is written out in [a page type of your own](pages.md#a-page-type-of-your-own-intoitems).

## A middleware

A middleware is what every request goes through as it leaves: a signature, a token that is renewed, a quota, a count. It is a type with `concord::Middleware`. The author of a client names it in the description (`middleware Signed::new(key.clone());`: written out in [a middleware](credentials.md#a-middleware)). A caller adds one of their own with `with_middleware`:

```rust
use concord::transport::{TransportError, TryInfo};
use concord::{Middleware, Next};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Counts what goes out: each try and each page.
#[derive(Clone, Default)]
pub struct Count(Arc<AtomicU64>);

impl Middleware for Count {
    async fn handle(&self, request: concord::reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<concord::reqwest::Response, TransportError>
    {
        self.0.fetch_add(1, Ordering::Relaxed);
        next.send(request, info).await          // on to the next middleware, then to what really sends
    }
}
```

```rust
let count = Count::default();
let riot = Riot::new(key).with_middleware(count.clone());     // `count` still sees every request
```

A caller's middleware comes before the ones of the description, so it cannot change a request after it was signed. `info.endpoint()` is the name of the endpoint (`riot.matches.ids`) and `info.number()` the number of the try. `next.send(request, info)` may be called more than once, or not at all: a middleware that will not send gives `TransportError::blocked()`, and the tries end there. The whole set of rules (when it is made, when it runs, what the clocks count) is in [a middleware](credentials.md#a-middleware).

## A transport

A transport is what really sends. `concord::Http` is the one of a client that was given none; a `reqwest::Client` of yours is one too, and keeps its own settings. A type of yours has one method:

```rust
use concord::reqwest::{Client, Request, Response};
use concord::transport::{TransportError, TryInfo};
use concord::Transport;

/// Sends with a `reqwest::Client` of yours, and says which endpoint and which try.
pub struct Logged {
    http: Client,
}

impl Transport for Logged {
    async fn send(&self, request: Request, info: TryInfo) -> Result<Response, TransportError> {
        eprintln!("{} (try {})", info.endpoint(), info.number());     // names and numbers only
        self.http.execute(request).await.map_err(TransportError::from)
    }
}
```

```rust
let riot = Riot::new(key).with_transport(Logged { http: Client::new() });
```

`with_transport` replaces only what sends: every middleware still runs above it. `TransportError::from` takes the URL out of a `reqwest::Error`. A transport must not change what the last middleware signed. With the feature `tower`, a `tower` service is a transport: `concord::tower::Tower::new(stack)`, and its layers (a rate limit, a limit of concurrent requests) sit under the client.

**A setting of the HTTP client** (a certificate, a proxy) needs no transport of your own: `concord::Http::with(|builder| …)` gives Concord's client with your setting, and `with_transport` takes it ([the client](calling.md#the-client)).

**A transport for a test** is a function: `concord::fake(|request| …)`. A failure is tested the same way, with no socket ([testing a client](testing.md)). A server that never answers is one line:

```rust
struct Silent;
impl Transport for Silent {
    async fn send(&self, _: Request, _: TryInfo) -> Result<Response, TransportError> {
        std::future::pending().await
    }
}
```

## Change one request: `edit_request`

```rust
let call = riot.matches(Europe).get(&id).edit_request(|request| {
    request.headers_mut().insert("x-extra", concord::http::HeaderValue::from_static("yes"));
});
let game = call.await?;
```

`.edit_request` takes a function `Fn(&mut reqwest::Request)`. It cannot fail. It runs once for each request that is built: once for a call, once for each page, and the changed request is used on every try. Several `.edit_request` on one call all run, in the order they were written; a later one sees what an earlier one did. It runs before the middlewares. A header value that such a function marks sensitive is left out, like a secret, when two pages are compared. What it changes is your own code: the promises about where a request goes are not checked again ([where the promises stop](promises.md#where-the-promises-stop)).

## The raw answer: `send_raw`

```rust
let response: concord::reqwest::Response = riot.matches(Europe).get(&id).send_raw().await?;
let status = response.status();
```

`send_raw()` gives the `reqwest::Response` as it came, after retry: no status rule runs and no byte of the body is read. A head of `400` or above is still looked at by the retry policy, as a refusal is. Concord's part ends at the head: the body, and the time to read it, are yours ([time](calling.md#time)).

## Send a request yourself: `request` and `read_response`

`read_response` is the way back up from `request()`. It runs the arms, the reader and the problem type of the endpoint on a `reqwest::Response` that came from anywhere. Concord sent nothing: no retry, no clock, no page.

```rust
let call = riot.matches(Europe).get(&id);
let response = my_client.execute(call.request()?).await?;
let game = call.read_response(response).await?;          // the same value that `call.await?` gives
```

`request()` only looks: the call is whole after it. To take the request away, `into_request()` spends the call and gives it; a call whose body is a stream can only be taken this way ([looking without sending](calling.md#looking-without-sending)).

## Your own methods beside the generated ones

A generated type is a type of your crate, so it takes an `impl` of yours, in the crate that holds the description:

```rust
impl riot::Matches {
    /// The last five matches of a player.
    pub async fn last_five(&self, puuid: &Puuid) -> riot::Result<Vec<MatchId>> {
        self.ids(puuid).count(5).send().await
    }
}
```

In another crate, a trait of yours for `riot::Matches` does the same.
