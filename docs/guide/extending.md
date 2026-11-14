# When Concord does not cover what you need

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [What Concord promises](promises.md) · next: [Troubleshooting](troubleshooting.md)*

This page is for the author or the caller of a client who needs something that the description cannot express: another format, another way to paginate, a signature, another HTTP library, one request changed by hand. Every layer has extension points (public traits and functions that you implement) and escape hatches (ways to step around Concord's usual path), all in plain Rust. This page shows each one, with code to copy.

## Extension points and escape hatches

| Layer | The usual way | Extension point or escape hatch |
|---|---|---|
| The description | The placement statements | Your own Rust value after `body`, `read`, `paginate`, `middleware` or in an arm. Your own methods beside the generated ones. |
| An endpoint that is not described | — | Add it to the description, or write your own method beside the generated ones. (There is no raw call on a handle; the name `call` is reserved for one.) |
| One call | `.await` | `.edit_request(\|request\| …)` changes the built `reqwest::Request` before it is handed to the transport. `.with_retry(…)`, `.with_timeout(…)` and `.with_idle_timeout(…)` apply to this call only. `request()` lets you inspect the request. |
| A request that you send yourself | — | `request()`, then `read_response(response)`. |
| The response | A typed value | `send_full()`. `send_if_none_match(etag)`. `send_raw()`: the `reqwest::Response` after retry, with no status rule and nothing read (retry then looks at the head only). `read Stream;` for bytes as they arrive. |
| The client | `Riot::new(key)` | `with_middleware(…)` for what every request passes through. `with_transport(…)` for the transport, and `concord::Http::with(…)` for a setting of the HTTP client. `with_base(…)`. |
| Below | — | Plain `reqwest`: `Request`, `Response`, `Client`. |

These extension points and escape hatches are public and stable: the traits `Reader`, `Middleware` and `Transport`, a body as a function that returns a `Body`, a pager as a function (with `Slot` and `Call`), your own page type (with `IntoItems`), `edit_request`, `into_request`, `read_response`. What Concord ships is built on the same extension points: `json`, `Json`, `by_offset` and the others are in a crate of their own, `concord-extras`, and every file in it except `tower.rs` is an example worth reading. (Only the `tower` adapter also uses a hidden helper of the engine, for the URL of a response; that helper is not a public extension point.)

## A writer of bodies

A writer is a function that returns a `concord::Body`, or a `Result` of one. The description names it after `body`, just as it names `json`:

```rust
// plain Rust, beside the macro: `body csv(rows);`
fn csv(rows: &[(u32, &str)]) -> concord::Body {
    let text: String = rows.iter().map(|(id, name)| format!("{id},{name}\n")).collect();
    concord::Body::new(text, "text/csv")
}
```

A writer that accepts anything `serde` can serialize also accepts a body made of entries: `body xml { a; b; }` calls `xml` with the entries. To fail the way `json` and `form` fail (with the input named in the Build error, and at most 128 levels of nesting per entry), wrap the value in `concord::body::Bounded` before the format sees it, and convert the format's error into a `concord::body::WriteError`:

```rust
use concord::body::{Bounded, WriteError};

/// `body xml(task);` or `body xml { title; author; }`.
pub fn xml<T: serde::Serialize>(value: T) -> Result<concord::Body, WriteError> {
    let value = Bounded::new(&value);
    let text = quick_xml::se::to_string(&value).map_err(|error| value.error(error))?;
    Ok(concord::Body::new(text, "application/xml"))
}
```

`value.error(e)` takes the `Bounded` by value, so the input's name goes into only one error. To combine bodies, `Body::concat(parts)` returns one body made of several, in order; `body.as_bytes()` returns the bytes of a body held in memory; and `body.into_stream()` turns any body into a stream. See [a body that is sent](responses-and-errors.md#a-body-that-is-sent) for what a body expression may return and how a body is resent. The shipped writers are in [`concord-extras/src/body.rs`](../../concord-extras/src/body.rs) (`json` is three lines) and [`multipart.rs`](../../concord-extras/src/multipart.rs).

## A reader

A reader is a type that implements one trait, `concord::Reader<T>`. The description names it after `read`, on the client or on a single endpoint:

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

`body.bytes(limit)` reads the whole body, up to a limit; `body.stream()` hands it over as it arrives. Applying `?` to the result of `bytes` already produces the right error. A decoder's message can quote the body, so it goes in `with_details`, which keeps 1 KiB of it for `e.details()` (`concord::read::details` truncates a string the same way). See [reading a success](responses-and-errors.md#reading-a-success) for a reader of a complete format, XML, with every part of `Incoming` and `ReadError` explained. The shipped readers are in [`concord-extras/src/read.rs`](../../concord-extras/src/read.rs).

## A pager

A pager is a function from the call that fetched a page, and its response, to the next call. In `paginate`, an input of the endpoint is passed as a `Slot`, which stands for the input on the call rather than its value: the pager reads it on the call that fetched the page, and sets it on the next one.

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

A pager returns `Some(next call)`, or `None` at the end. A pager that can reject a page returns a `Result` whose error is a `concord::Fail` (the same type that a function used in an arm fails with; `concord::mismatch("…")` creates one): a page that says "there is more" but gives no cursor is an error, not the end. That pager, `more`, is written out in [a pager of your own](pages.md#a-pager-of-your-own), along with what `Call::at_url` does for an API that hands out URLs. To remove the fragment from a next-page URL, the shipped pagers use `concord::page::fragmentless`, which is public too. The shipped pagers are in [`concord-extras/src/page.rs`](../../concord-extras/src/page.rs).

**Your own list type as a page.** `.pages().items()` and the counting pagers (`by_offset`, `by_offset_limit`, `by_page`) require one trait on the return type, `concord::page::IntoItems`: `into_items` and `count`, a few lines. It is written out in [a page type of your own](pages.md#a-page-type-of-your-own-intoitems).

## A middleware

A middleware layer is something that every request passes through as it is sent: a signature, a token that is renewed, a quota, a counter. It is a type that implements `concord::Middleware`. The author of a client names it in the description (`middleware Signed::new(key.clone());`, written out in [a middleware](credentials.md#a-middleware)). A caller adds their own with `with_middleware`:

```rust
use concord::transport::{TransportError, TryInfo};
use concord::{Middleware, Next};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Counts what is sent: each attempt and each page.
#[derive(Clone, Default)]
pub struct Count(Arc<AtomicU64>);

impl Middleware for Count {
    async fn handle(&self, request: concord::reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<concord::reqwest::Response, TransportError>
    {
        self.0.fetch_add(1, Ordering::Relaxed);
        next.send(request, info).await          // on to the next middleware layer, then to the transport
    }
}
```

```rust
let count = Count::default();
let riot = Riot::new(key).with_middleware(count.clone());     // `count` still sees every request
```

A caller's middleware runs before the description's middleware, so it cannot change a request after it was signed. `info.endpoint()` is the name of the endpoint (`riot.matches.ids`), and `info.number()` is the attempt number. `next.send(request, info)` may be called more than once, or not at all: a middleware that will not send returns `TransportError::blocked()`, and no more attempts are made. See [a middleware](credentials.md#a-middleware) for the full set of rules: when it is created, when it runs, what the timeouts measure.

## A transport

A transport is the layer that sends the request. `concord::Http` is the transport of a client that was given none; your own `reqwest::Client` is a transport too, and keeps its own settings. Your own type implements one method:

```rust
use concord::reqwest::{Client, Request, Response};
use concord::transport::{TransportError, TryInfo};
use concord::Transport;

/// Sends with your own `reqwest::Client`, and logs the endpoint and the attempt.
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

`with_transport` replaces only the transport: every middleware layer still runs above it. `TransportError::from` removes the URL from a `reqwest::Error`. A transport must not change what the last middleware signed. With the `tower` feature, a `tower` service is a transport, `concord::tower::Tower::new(stack)`, and its layers (a rate limit, a limit on concurrent requests) sit beneath the client.

**A setting of the HTTP client** (a certificate, a proxy) needs no transport of your own: `concord::Http::with(|builder| …)` returns Concord's client with your setting, and you pass it to `with_transport` ([the client](calling.md#the-client)).

**A transport for a test** is a function: `concord::fake(|request| …)`. A failure is tested the same way, without a socket ([testing a client](testing.md)). A server that never responds takes one line:

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

`.edit_request` takes a function `Fn(&mut reqwest::Request)`. It cannot fail. It runs once for each request that is built: once per call, once per page, and the changed request is used on every attempt. Several `.edit_request` calls on one call all run, in the order they were written, and a later one sees what an earlier one did. They run before the middleware. A header value that such a function marks sensitive is left out, like a secret, when two pages are compared. What it changes is your own code's responsibility: the promises about where a request goes are not checked again ([where the promises stop](promises.md#where-the-promises-stop)).

## The raw response: `send_raw`

```rust
let response: concord::reqwest::Response = riot.matches(Europe).get(&id).send_raw().await?;
let status = response.status();
```

`send_raw()` returns the `reqwest::Response` as it arrived, after retry: no status rule runs and no byte of the body is read. A head with a status of `400` or above is still checked by the retry policy, as an error response is. Concord's work ends at the head: the body, and the time it takes to read it, are yours to handle ([timeouts](calling.md#timeouts)).

## Send a request yourself: `request` and `read_response`

`read_response` is the way back up from `request()`. It runs the endpoint's arms, reader and problem type on a `reqwest::Response` from any source. Concord sent nothing, so there is no retry, no timeout and no pagination.

```rust
let call = riot.matches(Europe).get(&id);
let response = my_client.execute(call.request()?).await?;
let game = call.read_response(response).await?;          // the same value that `call.await?` returns
```

`request()` only inspects: the call is still intact afterwards. `into_request()` consumes the call and returns the request; a call whose body is a stream can be taken only this way ([looking without sending](calling.md#looking-without-sending)).

## Your own methods beside the generated ones

A generated type is a type of your own crate, so you can add an `impl` block for it in the crate that holds the description:

```rust
impl riot::Matches {
    /// The last five matches of a player.
    pub async fn last_five(&self, puuid: &Puuid) -> riot::Result<Vec<MatchId>> {
        self.ids(puuid).count(5).send().await
    }
}
```

In another crate, your own trait implemented for `riot::Matches` does the same.
