# Concord

Concord turns one nested description of an HTTP API into a typed Rust client.
You write the API as a tree in `concord::api!`: a base, scopes, endpoints, and where each input goes on the wire.
The macro writes a client type, one method for each endpoint, and one error type; `reqwest` sends the requests.

This is a release candidate, not on crates.io yet. Until it is, depend on the repository:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api" }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt"] }
```

After the first upload, the first line becomes `concord-api = "5.0.0-rc.1"`.
The package is `concord-api`; in code the library is `concord`.

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

`json` and `Json` are ordinary Rust items. The macro knows no format: a reader or a writer of yours takes the same place.
Without `with_transport`, the client sends with `concord::Http`, Concord's own `reqwest` client.

## What you get

- **Errors.** Every endpoint of a client fails with one type, `tasks::Error`, of four kinds: Build, Transport, Refused, Mismatch.
- **Refusals.** An answer that is not a 2xx, and that no arm of `status { }` takes, is a Refused error. It keeps the status, the headers and at most 64 KiB of the body.
- **Problems.** `problem E;` reads the API's own error body into your type `E`; `error.problem()` gives it.
- **Retries.** Off until you call `with_retry`. `Retry::standard()` makes at most 3 tries. It sends a request again only when its method (`GET`, `HEAD`, `OPTIONS`, `PUT`, `DELETE`) or `idempotent;` says that it may be repeated.
- **Pages.** `paginate by_offset(start);` on an endpoint gives `.pages()` and `.pages().items()`. The same request twice in a row ends the pages with an error.
- **Secrets.** An input of type `Secret` is written on the wire. `Debug` and the printed error show names, never values.
- **Clocks.** 30 seconds of silence from the server end a call (`idle_timeout`). `with_timeout` bounds the whole call.
- **Redirects.** Concord's own client follows none: a `3xx` is the answer.
- **Tests.** `with_transport(fake(...))` puts a function in place of the network.

## Panics

Sending requires Tokio 1.x with time and IO enabled; polling elsewhere panics with Tokio's message.
Building clients and calls needs no runtime. This is the one panic that Concord documents outside
the helpers for tests: `reply` and `reply_with` panic on a status outside 100 to 999, or on a
header that is not valid.

## Features

| Feature | What it turns on | Default |
|---|---|---|
| `json` | `json(x)` to write a body, `Json` to read one | on |
| `form` | `form(x)` to write a form body | on |
| `rustls` | TLS for `concord::Http`, through `reqwest/rustls` | on |
| `http2` | HTTP/2 for `concord::Http`, through `reqwest/http2` | on |
| `system-proxy` | The proxy settings of the system (macOS, Windows), as `reqwest` reads them | on |
| `tower` | A `tower` service as a transport: `concord::tower::Tower` | off |
| `multipart` | `multipart()` to write a `multipart/form-data` body | off |

The TLS stack is the application's choice. Turn `rustls` off with `default-features = false`, and give
`with_transport` a `reqwest::Client` that you built with another one.

## Examples

Four programs in `examples/` run with no network:

```text
cargo run --example quickstart   # a client, one call
cargo run --example pages        # a paged endpoint: by page, and by item
cargo run --example errors       # a typed problem, a timeout, a refused answer
cargo run --example bookshop     # a whole client: pages, a creation, an upload, retries,
                                 # timeouts, a stream, several tasks; on virtual time
```

The whole language, with an example for each statement, is the documentation of
`concord::api!`. What the macro writes is shown on the page `concord::sample`, and the
boundaries of a client (what is tried again, the clocks, a query) are on the front page of
the crate documentation.

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

Read [Concord in ten minutes](https://github.com/gqsnt/concord_api/blob/main/docs/guide/tour.md) first. Then [the documentation](https://github.com/gqsnt/concord_api/blob/main/docs/README.md),
by what you want to do, and everything a description may say on one page: [Concord on one page](https://github.com/gqsnt/concord_api/blob/main/docs/reference/overview.md).
When it does not compile, or a call failed: [troubleshooting](https://github.com/gqsnt/concord_api/blob/main/docs/guide/troubleshooting.md).
What changed, and what changed from Concord 4: [the changelog](https://github.com/gqsnt/concord_api/blob/main/CHANGELOG.md).
