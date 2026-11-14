# Concord

Concord turns one nested description of an HTTP API into a typed Rust client.
You write the API as a tree in `concord::api!`: a base, scopes, endpoints, and where each input goes on the wire.
The macro generates a client type, one method for each endpoint, and one error type; `reqwest` sends the requests.

This is a release candidate, not on crates.io yet. Until it is, add it as a Git dependency:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api" }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt"] }
```

Once Concord is published on crates.io, the first line becomes `concord-api = "5.0.0-rc.1"`.
The package is named `concord-api`, but in code the library is `concord`.

## One description, one call

```rust
use concord::{fake, json, reply, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Task { pub id: u64, pub title: String, pub done: bool }

#[derive(Clone, Serialize)]
pub struct NewTask { pub title: String }

concord::api! {
    pub client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;

        GET list(done?: bool) -> Vec<Task> {
            query { done; }
        }
        POST create(task: NewTask) -> Task {
            body json(task);
        }
        GET get(id: u64) -> Task {
            path "{id}";
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> tasks::Result<()> {
    // A function takes the place of the network: this example opens no socket.
    let tasks = Tasks::new().with_transport(fake(|request| {
        assert_eq!(request.url().as_str(), "https://tasks.example.com/api/tasks?done=false");
        Ok(reply(200, r#"[{ "id": 7, "title": "Write the docs", "done": false }]"#))
    }));

    // `done` is optional: it is a method. The call is sent by `.await`.
    let open: Vec<Task> = tasks.list().done(false).await?;
    assert_eq!(open[0].title, "Write the docs");
    Ok(())
}
```

`json` and `Json` are ordinary Rust items. The macro has no built-in format, so your own reader or writer can take their place.
Without `with_transport`, the client sends requests through `concord::Http`, Concord's own `reqwest` client.

## What you get

- **Errors.** Every endpoint of a client returns the same error type, `tasks::Error`, which has four kinds: Build, Transport, Refused, Mismatch.
- **Error responses.** A response that is not a 2xx, and that no arm of `status { }` matches, is a Refused error. It keeps the status, the headers and at most 64 KiB of the body.
- **Problems.** `problem E;` reads the API's own error body into your type `E`; `error.problem()` returns it.
- **Retries.** Retries are off until you turn them on with `with_retry`. `Retry::standard()` makes at most 3 attempts, and it retries a request only when its method (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`) or `idempotent;` says that it may be repeated.
- **Pagination.** `paginate by_offset(start);` on an endpoint adds `.pages()` and `.pages().items()` to its call. If the next page would be the same request as the one just sent, the pages end with an error.
- **Secrets.** An input of type `Secret` is sent on the wire. `Debug` and the printed error show names, never values.
- **Timeouts.** A call ends after 30 seconds of silence from the server (`idle_timeout`). `with_timeout` bounds the whole call.
- **Redirects.** Concord's own client follows no redirects: a `3xx` is the response.
- **Tests.** `with_transport(fake(...))` replaces the network with a function.

## Panics

Sending requires Tokio 1.x with time and IO enabled; polling a call outside such a runtime panics with Tokio's message.
Building clients and calls needs no runtime. This is the only panic that Concord documents, apart
from the test helpers: `reply` and `reply_with` panic on a status outside 100 to 999, or on an
invalid header.

## Features

| Feature | What it turns on | Default |
|---|---|---|
| `json` | `json(x)` to write a body, `Json` to read one | on |
| `form` | `form(x)` to write a form body | on |
| `rustls` | TLS for `concord::Http`, through `reqwest/rustls` | on |
| `http2` | HTTP/2 for `concord::Http`, through `reqwest/http2` | on |
| `system-proxy` | The system's proxy settings (macOS, Windows), as `reqwest` reads them | on |
| `gzip` | The gzip decoder of `reqwest`, for `builder.gzip(true)` in `concord::Http::with` | off |
| `deflate` | The deflate decoder of `reqwest`, for `builder.deflate(true)` in `concord::Http::with` | off |
| `socks` | SOCKS proxies, configured with `reqwest::Proxy` in `concord::Http::with` | off |
| `tower` | A `tower` service as a transport: `concord::tower::Tower` | off |
| `multipart` | `multipart()` to write a `multipart/form-data` body | off |

The TLS stack is the application's choice. Turn `rustls` off with `default-features = false` (then list again
the defaults you still need, such as `json`, `form`, `http2` and `system-proxy`), and give
`with_transport` a `reqwest::Client` that you built with another TLS stack.

`gzip`, `deflate` and `socks` only make a capability available: a feature alone changes nothing,
because `concord::Http` turns every decoder off before your closure runs. Decompression starts
only for a client built with `concord::Http::with(|builder| builder.gzip(true))?`, and a reader's
limit then counts the decoded bytes. `brotli`, `zstd`, cookies and `native-tls` are not forwarded
yet: add `reqwest` to your own `Cargo.toml` with that feature, or bring your own `reqwest::Client`
with `with_transport`.

## Examples

Four programs in `examples/` run without a network:

```text
cargo run --example quickstart   # a client, one call
cargo run --example pages        # a paginated endpoint: by page, and by item
cargo run --example errors       # a typed problem, a timeout, a rejected request
cargo run --example bookshop     # a whole client: pages, a creation, an upload, retries,
                                 # timeouts, a stream, several tasks; on virtual time
```

The reference of the whole language, with an example for each statement, is the rustdoc page
of `concord::api!`. The module `concord::sample` shows the code that the macro generates, and the
section "The boundaries", further down the front page of the crate documentation, lists the
boundaries of a client (what is retried, the timeouts, a query). The crate documentation will be
on docs.rs once Concord is published; until then, run `cargo doc -p concord-api --open`.

## The crates

| Crate | What it is |
|---|---|
| `concord-api` | This crate, used as `concord`. Depend on it, and on `serde`. |
| `concord-core` | The traits and the engine. |
| `concord-macros` | `api!`. |
| `concord-extras` | `json`, `form`, `multipart`, the readers, the pagers, `basic`, `tower`. |
| `concord-testkit` | For your tests: a scripted local server, and a transport with virtual time. |

## Minimum Rust version

Rust 1.96.

## The guide

Read [Concord in ten minutes](https://github.com/gqsnt/concord_api/blob/main/docs/guide/tour.md) first. Then read [the documentation](https://github.com/gqsnt/concord_api/blob/main/docs/README.md),
organised by task, and [Concord on one page](https://github.com/gqsnt/concord_api/blob/main/docs/reference/overview.md), everything a description can say.
When a description does not compile, or a call fails, see [troubleshooting](https://github.com/gqsnt/concord_api/blob/main/docs/guide/troubleshooting.md).
See [the changelog](https://github.com/gqsnt/concord_api/blob/main/CHANGELOG.md) for the changes in each release, and since Concord 4.
