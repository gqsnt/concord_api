# Prior art

*[Concord](../../README.md) · [docs](../README.md) · design · previous: [scope and limits](scope.md)*

This page is for a developer who knows other HTTP clients and SDKs. It shows which of Concord's choices they share, the facts about `reqwest` that Concord relies on, and where Concord goes against the majority.

## What other libraries do

In the "Source" column, **checked** means read in the library's source, at the version given; **reported** means taken from the public sources named in the row (an issue, an advisory, a paper, the library's documentation). An empty cell names no source.

| Choice | Same choice in, or the fact behind it | Source |
|---|---|---|
| A plain signature; the wire in a block | gRPC HTTP annotations | |
| No prefix on methods; a call that you `.await` | `reqwest`, AWS SDK for Rust; `axum::serve` is a value that you `.await` | checked, for `axum` (0.8.9) |
| An optional value accepts a value or an `Option` | `reqwest` (`pool_idle_timeout` and seven others) | checked (`reqwest` 0.13.5) |
| A type recognized by its spelling (`Option` and `Result` after the arrow) | `clap`, for `Option` | checked (`clap_derive` 4.6.7) |
| `{x}` and `{*x}` in a route | `axum` routes | checked (`axum` 0.8.9) |
| A Refused error that keeps the body (at most 64 KiB) | Neither `reqwest` (`error_for_status`) nor `ureq` keeps it; a known pain point | checked |
| An error whose printed line holds no URL and no chain of sources (`source()` is there when you need it) | Concord 4 (`error.rs`). `reqwest`'s error prints `for url (…)`, and `anyhow` prints every source. | checked |
| Changing an inherited setting takes a keyword | Kotlin, C#, Swift (`override`) | |
| A value from above names its owner | Rust itself (`self.x`) | |
| A default size limit | axum 2 MB; `ureq` 10 MB | checked |
| A secret as a type that hides its value | The `secrecy` crate; `HeaderValue::set_sensitive` | checked, for `HeaderValue::set_sensitive` (`http` 1.5.0) |
| One trait where a request goes in and a response comes out | Go's `http.RoundTripper`; `tower::Service` | |
| A response built in memory, for a test | `reqwest::Response: From<http::Response<T>>` | checked (`reqwest` 0.13.5) |
| The status rule kept separate from decoding | `reqwest` (`error_for_status` then `json`), `ureq` | checked |
| Retry as a value on the client; the author says what is safe to repeat | `tower::retry::Policy` separates "may this be retried" from "can it be cloned"; Stripe's libraries retry a `POST` that carries an `Idempotency-Key` | checked, for `tower` (0.5.3) |
| One struct for each operation, setters, then send | AWS SDK for Rust, `octocrab`, `progenitor` | |
| The method at the start of the line | Every API reference (`GET /users`); Retrofit's `@GET`; `axum`'s `get(handler)` | |
| A value serialized by one serializer, wherever it is placed | Smithy fixes the wire form of a value for each place. Retrofit added string converters in 2016, because `toString()` was not enough. | reported |
| A list repeats its key | The default of OpenAPI 3 (`style: form`, `explode: true`); `requests` | reported |
| A space is `%20` | Smithy's HTTP bindings; AWS SigV4 | reported |
| A body as a value that carries its content type | Retrofit converters, Feign's `Encoder`. Retrofit 1.6 took `Content-Type` back out of the header list and derived it from the body again. | reported |
| Retry decided by what happened, not only by the method | Go, issue 19943: a missing response does not prove that the server did nothing. HTTP/2 `REFUSED_STREAM` is the case where it does. RFC 9110, section 9.2.2. | reported |
| A random wait | Amazon, "Exponential Backoff and Jitter" (2015); now in the AWS SDKs | reported |
| An attempt ends where data reaches the caller | gRPC commits at the response headers; `curl` warns about `--retry-all-errors` | reported |
| Signing, and a token that is renewed, as a layer around what sends | `reqwest-middleware`; `tower`'s layers; the interceptors of the AWS SDKs and of OkHttp. (OkHttp's `Authenticator` and Azure's bearer policy send the request once more after a `401`: in Concord that is the layer's own code.) | reported |
| A credential that can be replaced in place | Azure's `AzureKeyCredential.Update`; the credential providers of the AWS SDKs | reported |
| The end of a list is stated for each operation | Smithy's `@paginated`. In 2024 the AWS SDK for Rust looped forever on S3 `ListParts`, because a general rule ("an empty token is the end") was wrong for it. | reported |
| A short page is not the end | Slack, DynamoDB, Google's AIP-158 | reported |
| Pages, and items | AWS SDK for Rust, Azure, Stripe | reported |
| The same request twice in a row ends the pages | The default of the AWS SDK for Rust. botocore makes it an error; Go makes it opt-in. | reported |
| A next URL is a decision about where credentials go | Wazuh's advisory of November 2026 | reported |
| An error that prints no value, and a full text on demand | .NET 9 redacts URLs and header values in its logs by default. AWS ships `DisplayErrorContext`, after "dispatch failure" hid the cause. | reported |
| Keep the body of an error response | `curl --fail-with-body` (7.76.0); .NET stopped disposing the content in 3.0 | reported |
| Timeouts with separate names | AWS SDK for Rust (attempt, operation), OkHttp (read, call), Envoy | reported |

## The facts of `reqwest` that Concord relies on

All of them were read in the source of `reqwest` 0.13.5.

| Fact | Where | What Concord does with it |
|---|---|---|
| On a redirect to another host it removes `Authorization`, `Cookie`, `cookie2`, `Proxy-Authorization` and `WWW-Authenticate`, and nothing else. | `redirect.rs`, `remove_sensitive_headers` | A key in `x-riot-token` would follow the redirect. So Concord's own client follows no redirect ([G3](../guide/promises.md)). |
| It follows ten redirects by default. A policy can be a function that sees the next URL and the URLs before it, and decides whether to follow or stop. | `redirect.rs`, `Policy::custom`, `Attempt` | Concord's own client sets the policy that follows nothing, after any setting that you make. |
| It has no timeout by default: `timeout`, `read_timeout` and `connect_timeout` all start empty. | `async_impl/client.rs` | Concord's own client sets `connect_timeout` to 30 seconds. |
| Before the head, its read timeout is one timer that starts with the request, and it is not reset while a body is sent. After the head, it is reset at each chunk of the body. A request can carry its own total `timeout`, but not its own read timeout. | `async_impl/client.rs` (`read_timeout_fut`), `async_impl/body.rs` (`ReadTimeoutBody`), `async_impl/request.rs` | A long upload, a long poll or a slow response could not be given more time on one call only. So Concord runs the wait for a response itself ([G7](../guide/promises.md)). |
| It adds `Referer` by default, and keeps no cookies by default. | `async_impl/client.rs` | Concord's own client switches `Referer` off. |
| It sends `Accept: */*` when the request has no `Accept`. | `async_impl/client.rs` | Concord adds no `Accept` of its own (see [Describe an API](../guide/describing-an-api.md)). |
| By default, its own retry resends only what the protocol says was not processed, at most twice. | `retry.rs`, the default builder | Left as it is. It happens below what Concord counts. |
| `Request::try_clone` returns `None` when the body is a stream. | `async_impl/request.rs` | That is the test behind "a stream body is never sent twice". |
| It marks an error as a failed connection by looking for a connector's mark in the chain of causes. | `error.rs`, `is_connect` | The mark is not proven for every platform and TLS backend, so Concord infers nothing from it: a failed connection does not mean that nothing was sent. |
| `reqwest::Url` is the `url` crate's type. | `lib.rs` | — |

Two more facts: the `url` crate silently drops a path segment equal to `.` or `..`, and `serde_urlencoded` writes nothing for a `None`.

## Where Concord goes against the majority

- **Placement in the block.** Retrofit, axum, FastAPI, OpenAPI and Smithy put it on the parameter. The cost is that an input is written twice.
- **No redirect is followed.** `reqwest` and `ureq` follow them, and on another host they drop only the standard credential headers. Concord cannot know whether a provider's own header is safe to send there, and a followed redirect is a second request that nobody described. So its client follows none.
- **No `Accept` header from a reader.** `curl --json` and most SDKs send one. Concord states the content type of what it sends, and does not negotiate.
- **Retries are off until you turn them on.** The SDKs of the large API providers retry by default; HTTP libraries mostly do not.

## Where no precedent is known

On the client side: a client written as a tree of scopes and composed at compile time, and a compile error when an inherited key is written again without `override`.

## Coming from Concord 4

Concord 5 is a new design: the description, the calls and the extension traits all change. What changed is listed in [the changelog](../../CHANGELOG.md).
