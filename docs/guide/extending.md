# When Concord does not cover what you need

*[Concord](../../README.md) · [docs](../README.md) · guide · previous: [What Concord promises](promises.md) · next: [Recipes](recipes.md)*

This page is for the author or the caller of a client who needs something that the description cannot express: another format, another way to paginate, a signature, another HTTP library, one request changed by hand. Every layer has extension points (public traits and functions that you implement) and escape hatches (ways to step around Concord's usual path), all in plain Rust. This page shows each one, with code to copy, after what Concord ships and when to use which.

## What ships beside the macro

Before you write your own, check what Concord ships. The macro knows none of these items: each is an ordinary item of `concord`, from the crate `concord-extras`, used in a description or on a client as your own would be. [Concord on one page](../reference/overview.md#what-ships-beside-the-macro) says what each one does; this table says when to use it, and which feature of `concord-api` turns it on.

| Item | Feature | Use it when |
|---|---|---|
| `json(x)`, `Json` | `json` (on by default) | The API sends and receives JSON. |
| `form(x)` | `form` (on by default) | The API takes a flat form, `application/x-www-form-urlencoded`. |
| `bracket_form(x)`, `bracket_pairs(x)` | `form` (on by default) | The API reads nested keys, `a[b]` and `a[0]` ([the shipped writers](bodies.md#the-shipped-writers)). |
| `multipart()` | `multipart` | The API takes a form with files ([a body that is sent](bodies.md#a-body-that-is-sent)). |
| `multipart_mixed(parts)`, `multipart_related(parts)` | `multipart` | The API takes parts with headers of their own: a batch, a document with its attachments. |
| `gzip(body)` | `gzip-body` | The API accepts a compressed request body. |
| `json_lines(records, max_bytes)` | `json` (on by default) | A bulk endpoint takes one JSON value per line. |
| `Text`, `Binary`, `read::Stream` | none | The body is text, raw bytes, or too large to hold in memory ([reading a success](responses-and-errors.md#reading-a-success)). |
| `read::JsonLines`, `read::Sse` | `json` for `JsonLines` | The body is a sequence of records: JSON Lines, or server-sent events ([a stream of records](responses-and-errors.md#a-stream-of-records)). |
| `by_offset`, `by_offset_limit`, `by_page`, `by_cursor`, `by_link`, `by_next_url` | none | A list arrives over several responses ([the shipped pagers](pages.md#the-shipped-pagers)). |
| `basic(user, password)` | none | The API uses HTTP Basic. |
| `auth::Renewed`, `auth::after_401` | `auth` | A token expires, and a renewal that was cancelled can simply be started again ([a token that is renewed](credentials.md#a-token-that-is-renewed)). |
| `auth::Rotating` | `rotating` (turns on `auth`) | Each exchange consumes the refresh token and returns its replacement ([a refresh token that rotates](credentials.md#a-refresh-token-that-rotates)). |
| `sign::SigV4`, `sign::body_bytes`, `sign::check_origin` | `sign` | The API uses AWS Signature Version 4; the two helpers serve a provider's own proof ([a signature](credentials.md#a-signature)). |
| `limit::Limit` | `limit` | The API limits the rate of calls, and requests should wait before the server refuses them ([rate limits](limits.md)). |
| `transfer::at(call, url)` | none | The API returned a URL on another host ([below](#a-url-that-the-api-returned)). |
| `tower::Tower` | `tower` | You already send requests through a stack of `tower` layers ([a transport](#a-transport)). |

The optional features are off by default. Turn on the ones that you use:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api", features = ["auth", "rotating", "sign", "limit", "multipart", "gzip-body", "tower"] }
```

The features that concern Concord's own HTTP client (`gzip`, `deflate`, `socks` and the others) are in [compression, proxies and TLS](calling.md#compression-proxies-and-tls).

**`Renewed` or `Rotating`.** Use `Renewed` when a renewal that was cancelled can simply be started again. Use `Rotating` when each exchange consumes the refresh token and returns its replacement: reusing a consumed refresh token can lose access to the account, so `Rotating` saves each new pair before either token is used, and it needs a store and a driver that the application polls.

**`Limit`, a `tower` layer or a `when` rule.** They wait at different places:

- `Limit` waits before a request leaves, for every call that shares it: after a `429` with `Retry-After`, under a cap on the requests in flight, or within buckets known in advance or learned from the headers of responses. It waits above the transport, so the idle timeout does not count its wait, and a place under its `max_in_flight` lasts until the body has been read.
- A rate limit or a concurrency limit of `tower`, through `tower::Tower`, waits inside the transport: the idle timeout counts its wait as silence, and a concurrency limit there ends at the response head.
- A `when` rule reacts to one response: it asks for another attempt of that call, after a wait, for example a wait that the API puts in the body (Telegram, [your own retry rules](retries.md#your-own-retry-rules)). It holds no other call back.

A `Limit` keeps a budget that every call spends; a `when` rule follows a wait that one response asks for. The two work together.

## Extension points and escape hatches

| Layer | The usual way | Extension point or escape hatch |
|---|---|---|
| The description | The placement statements | Your own Rust value after `body`, `read`, `paginate`, `middleware` or in an arm. Your own methods beside the generated ones. |
| An endpoint that is not described | — | Add it to the description, or write your own method beside the generated ones. (There is no raw call on a handle; the name `call` is reserved for one.) |
| One call | `.await` | `.edit_request(\|request\| …)` changes the built `reqwest::Request` before it is handed to the transport. `.with_retry(…)`, `.with_timeout(…)` and `.with_idle_timeout(…)` apply to this call only. `request()` lets you inspect the request. |
| A request that you send yourself | — | `request()`, then `read_response(response)`. |
| The response | A typed value | `send_full()`. `send_if_none_match(etag)`. `send_raw()`: the `reqwest::Response` after retry, with no status rule and nothing read (retry then looks at the head only). `read Stream;` for bytes as they arrive. |
| The client | `Riot::new(key)` | `with_middleware(…)` for what every request passes through: your own layer, or one that Concord ships (`auth::Renewed`, `auth::Rotating`, `sign::SigV4`, `limit::Limit`). `with_transport(…)` for the transport, and `concord::Http::with(…)` for a setting of the HTTP client. `with_base(…)`. |
| A URL that the API returned, on another host | — | A second description with its own `base`, and `concord::transfer::at(call, url)` ([below](#a-url-that-the-api-returned)). |
| Below | — | Plain `reqwest`: `Request`, `Response`, `Client`. |

These extension points and escape hatches are public and stable: the traits `Reader`, `Middleware` and `Transport`, a body as a function that returns a `Body`, a pager as a function (with `Slot` and `concord::page::Call`), your own page type (with `IntoItems`), `edit_request`, `into_request`, `read_response`. What Concord ships is built on the same extension points: `json`, `Json`, `by_offset`, the readers of records, `auth::Renewed`, `limit::Limit`, `transfer::at` and the others are in a crate of their own, `concord-extras`, and every file in it except `tower.rs` is an example worth reading. Only the `tower` adapter uses a hidden helper of the core, for the response URL. That helper is not a public extension point. Wrapping writers use the public `Body::get_content_type`, `Body::map_stream` and `Body::is_repeatable` methods.

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

`value.error(e)` takes the `Bounded` by value, so the input's name goes into only one error. To combine bodies, `Body::concat(parts)` returns one body made of several, in order; `body.as_bytes()` returns the bytes of a body held in memory; `body.into_stream()` turns any body into a stream; and `body.get_content_type()` returns the content type of a body that your writer wraps ([a writer that wraps another body](bodies.md#the-shipped-writers)). See [a body that is sent](bodies.md#a-body-that-is-sent) for what a body expression may return and how a body is resent. The shipped writers are in [`concord-extras/src/body.rs`](../../concord-extras/src/body.rs) (`json` is three lines), [`multipart.rs`](../../concord-extras/src/multipart.rs) and [`body/`](../../concord-extras/src/body).

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

`body.bytes(limit)` reads the whole body, up to a limit; `body.stream()` hands it over as it arrives. Applying `?` to the result of `bytes` already produces the right error. A decoder's message can quote the body, so it goes in `with_details`, which keeps 1 KiB of it for `e.details()` (`concord::read::details` truncates a string the same way). See [reading a success](responses-and-errors.md#reading-a-success) for a reader of a complete format, XML, with every part of `Incoming` and `ReadError` explained. The shipped readers are in [`concord-extras/src/read.rs`](../../concord-extras/src/read.rs), and the readers of records in [`read/records.rs`](../../concord-extras/src/read/records.rs).

## A pager

A pager is a function from the call that fetched a page, and its response, to the next call. In `paginate`, an input of the endpoint is passed as a `Slot`, which stands for the input on the call rather than its value: the pager reads it on the call that fetched the page, and sets it on the next one.

```rust
use concord::page::{IntoItems, Slot};
use concord::Answer;

/// The next page starts one page further: `paginate next_page(page);`.
pub fn next_page<C: concord::page::Call, S: Slot<C, Value = u32>>(page: S) -> impl Fn(C, &Answer<C::Value>) -> Option<C>
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

A middleware layer is something that every request passes through as it is sent: a signature, a token that is renewed, a quota, a counter. It is a type that implements `concord::Middleware`. Concord ships four: `concord::auth::Renewed` for a token that is renewed ([a token that is renewed](credentials.md#a-token-that-is-renewed)), `concord::auth::Rotating` for a refresh token that rotates ([a refresh token that rotates](credentials.md#a-refresh-token-that-rotates)), `concord::sign::SigV4` for AWS Signature Version 4 ([a signature](credentials.md#a-signature)), and `concord::limit::Limit` for a rate limit ([rate limits](limits.md)). For anything else, you write your own. The author of a client names it in the description (`middleware Signed::new(key.clone());`, written out in [a signature](credentials.md#a-signature)). A caller adds their own with `with_middleware`:

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

A caller's middleware runs before the description's middleware, so it cannot change a request after it was signed ([the order of layers](middleware.md#the-order-of-layers)). `info.endpoint()` is the name of the endpoint (`riot.matches.ids`), and `info.number()` is the attempt number. `next.send(request, info)` may be called more than once, or not at all: a middleware that will not send returns `TransportError::blocked()`, and no more attempts are made. See [how a middleware layer works](middleware.md#how-a-middleware-layer-works) for the full set of rules: when it is created, when it runs, what the timeouts measure.

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

`with_transport` replaces only the transport: every middleware layer still runs above it. `TransportError::from` removes the URL from a `reqwest::Error`. A transport must not change what the last middleware signed. With the `tower` feature, a `tower` service is a transport, `concord::tower::Tower::new(stack)`, and its layers (a rate limit, a limit on concurrent requests) sit beneath the client. A `tower` layer waits inside the transport, so the idle timeout counts its wait as silence, and a limit on concurrent requests there ends at the response head; `concord::limit::Limit` waits above the transport, and its `max_in_flight` holds a place until the body has been read ([rate limits](limits.md)).

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

## A URL that the API returned

Some APIs return a URL on another host: an upload URL with a signature in its query, a link to download a file. Such a URL is not a call of the API's client, which sends every request to its own origin and carries the API's credentials (`call.at_url(url)` never changes the origin). It belongs to a second description, with its own `base` and none of the API's credentials or middleware:

```rust
pub client Uploads(origin: concord::Base) {
    // the host that receives the bytes: its own base, nothing of the API's client
    base origin;
    POST send(bytes: concord::Body) { body bytes; }
}
```

```rust
use concord::http::{HeaderMap, HeaderName, HeaderValue};

let ticket = files.upload_ticket().await?;                 // the API returns { url, headers }
let origin: concord::Base = "https://uploads.example.com".parse()?;   // where you expect the bytes to go
let uploads = Uploads::new(origin);

let mut returned = HeaderMap::new();                       // headers that the API returned, for this upload
for header in &ticket.headers {
    returned.append(HeaderName::from_bytes(header.name.as_bytes())?, HeaderValue::from_str(&header.value)?);
}
concord::transfer::at(uploads.send(bytes), &ticket.url)?
    .edit_request(move |request| request.headers_mut().extend(returned.clone()))
    .await?;
```

| | |
|---|---|
| `concord::transfer::at(call, url)` | It checks the text of the URL, then returns `call.at_url(url)`: the same call, with the path and the query of that URL. It sends nothing. It returns a `TransferError`, which never holds the URL, for a URL that would not be sent as it is written: a relative URL, a scheme other than HTTP or HTTPS, user information, a fragment, a lone `?` or a query made only of `&`, a spelling that parsing would change, or more than 65,534 bytes. An accepted URL reaches the transport with its path and its query byte for byte, a signature included, for a second description with no `Secret` query entry: the engine adds such an entry again to a URL given to `at_url`. |
| The origin | The URL must be at the origin of the second description's `base`. A URL at another origin is a Build error, and nothing is sent, as with any `at_url` ([a pager of your own](pages.md#a-pager-of-your-own)). |
| What travels with it | Only what the second description says. Nothing of the API's client: no key, no header, no middleware. |
| Headers that the API returned | They are known only at run time, so `edit_request` adds them. `HeaderMap::extend` replaces each name that it is given, and keeps several values of one name. These headers are your code's: send only the ones that you trust and that agree with the body. |
| The rest of the call | It is a generated call like any other: its retry policy, its timeouts, its reader, and the rules of its body: a `Body::stream` is sent once, and a `Body::repeatable` is opened again for each attempt that the rules of retry allow ([a body that is opened again](bodies.md#a-body-that-is-opened-again-for-each-attempt)). The headers that `edit_request` adds are kept for every attempt. To send a `Body::stream` again, your own loop builds the call again, with a new stream ([a body that is a stream](bodies.md#a-body-that-is-a-stream)). |
| Not done | No redirect is followed (Concord's own client follows none), and no upload session protocol is run: a large upload in several requests, or a download that resumes, is a loop in your own code. |

## A documented download redirect

Concord follows no redirect: a `3xx` is the response, with its `Location`. When a provider documents a GET that returns a download URL, read that response with `send_raw()`, check the documented status, and make an explicit second call through a description with no credential or middleware. For a fixed download host, choose that description's base independently of the returned URL.

Some providers choose a download host for each object. Then the origin may come from `Location`, but only after your own allowlist and only for a description with no credential and no middleware. This example expects one absolute HTTPS URL under the provider's download domain, on the default HTTPS port. Replace that domain with the provider's documented hosts:

```rust
concord::api! {
    pub client Archives(token: concord::Secret) {
        base "https://api.example.com";
        headers { "authorization" = "Bearer {token}"; }
        GET download(id: u64) { path "archives/{id}"; }
    }
}
concord::api! {
    pub client Downloads(origin: concord::Base) {
        base origin;
        GET fetch() -> concord::ByteStream { read concord::read::Stream; }
    }
}

/// `downloads` makes the client of the chosen host: `Downloads::new` in a program, and
/// a client with a test transport in a test.
async fn download(api: &Archives, id: u64, downloads: impl FnOnce(concord::Base) -> Downloads)
    -> Result<concord::ByteStream, Box<dyn std::error::Error + Send + Sync>>
{
    let response = api.download(id).send_raw().await?;
    if response.status() != concord::http::StatusCode::FOUND {
        return Err("the archive did not return its documented 302".into());
    }
    let location = response.headers().get("location")
        .ok_or("the download Location is missing")?.to_str()?.to_owned();
    let url = concord::reqwest::Url::parse(&location)?;
    if url.scheme() != "https" || url.port_or_known_default() != Some(443)
        || !url.host_str().is_some_and(|host| host.ends_with(".downloads.example.com"))
    {
        return Err("the download host is not allowed".into());
    }
    let origin: concord::Base = url.origin().ascii_serialization().parse()?;
    drop(response);
    let downloads = downloads(origin);
    Ok(concord::transfer::at(downloads.fetch(), &location)?.await?)
}
```

Tested: [`redirect.rs`](../../tests/real-apis/fixtures/recipes/redirect.rs), run by [`writer_recipes.rs`](../../tests/real-apis/tests/writer_recipes.rs), which compares this block with the file.

The function does not build the second client itself: its caller passes the function that does. A program passes `Downloads::new`, and the bytes come from the provider's host:

```rust
let archive = download(&archives, 7, Downloads::new).await?;
```

A test passes a function that gives the second client the same scripted transport as the first, so both calls are played and recorded in order ([a script](testing.md#a-script-a-transport-with-virtual-time)):

```rust
use concord_testkit::Script;

let script = Script::new();
script.answer(302).header("location", "https://eu.downloads.example.com/archives/7?sig=a%2Fb");
script.answer(200).body("the archive");
let archives = Archives::new("PRIVATE_API_KEY").with_transport(script.clone());

let mut archive = download(&archives, 7, |origin| {
    Downloads::new(origin).with_transport(script.clone())
}).await?;
assert_eq!(archive.try_next().await?.as_deref(), Some(&b"the archive"[..]));

let sent = script.requests();
assert_eq!(sent[1].url(), "https://eu.downloads.example.com/archives/7?sig=a%2Fb");
assert!(!sent[1].headers().contains_key("authorization"));   // the API's token stayed with the API
```

`transfer::at` validates the original URL spelling; the engine checks its origin against the download call's base before sending. The recipe copies none of the API client's credentials, headers or middleware. It does not replace the URL with an unchecked request edit. A relative Location needs an explicit resolution policy from the author. This function follows one Location: another redirect is returned as the second call's error, and the author decides whether another call is appropriate. Each call keeps its own retry and timeout policy; the downloaded bytes are streamed without buffering the whole file.

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

`request()` only inspects: the call is still intact afterwards. `into_request()` consumes the call and returns the request; a call whose body is a `Body::stream` can be taken only this way ([looking without sending](calling.md#looking-without-sending)). Both take a `Body::repeatable` without opening it: it opens when the body of the request is read, and a request that you send yourself gets none of Concord's retries.

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

To add a method to one call, write an `impl` for that call type in the crate that holds the description.

```rust
impl riot::Call<riot::matches::Ids> {
    pub fn ranked_solo(self) -> Self {
        self.queue(420)
    }
}
```

In another crate, your own trait implemented for `riot::Matches` does the same. To add a method to one call there, implement your trait for `riot::Call<riot::matches::Ids>`.
