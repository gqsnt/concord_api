# Concord

Concord turns one nested description of an HTTP API into a typed Rust client built on `reqwest`.

```rust
use concord::{json, Json};

concord::api! {
    pub client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;

        GET list(done?: bool) -> Vec<Task> { query { done; } }
        POST create(task: NewTask) -> Task { body json(task); }
        DELETE delete(id: u64) { path "{id}"; }
    }
}

let open = Tasks::new().list().done(false).await?;
```

An endpoint's signature becomes the method that the caller sees; the block that follows it describes the HTTP request that the call sends. `Json` reads a response and `json` writes a body. Both are ordinary Rust items, not keywords: the macro has no built-in data format, so your own function or type can take their place.

## What you get

- **Typed inputs and responses.** Each endpoint becomes a method, with an argument for each required input and a setter named after each optional one. [Describe an API](docs/guide/describing-an-api.md)
- **The API's errors as your own type.** Declare `problem MyProblem;` once, and `error.problem()` gives you the API's error, decoded as `MyProblem`, for every endpoint. [The API's own error](docs/guide/responses-and-errors.md#the-apis-own-error)
- **Retries that never repeat a `POST` by accident.** Retries are off until you turn them on, and even then only a request that may be repeated is retried. [Retries](docs/guide/retries.md)
- **Pagination as a stream.** Declare it with one line in the description, then iterate with `.pages()` or `.pages().items()`. [Pages](docs/guide/pages.md)
- **Concord never prints a secret in an error or a log.** An input of type `Secret` is sent on the wire; Concord's errors, logs and `Debug` output never show it. [What Concord promises](docs/guide/promises.md)
- **Tests without a network.** `with_transport(fake(…))` replaces the network with a function; the test kit adds a local server and virtual time. [Testing a client](docs/guide/testing.md)

## Install

Concord is a release candidate, and it is **not on crates.io yet**. Add it as a Git dependency:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api" }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt"] }
```

Once Concord is published on crates.io, the first line becomes `concord-api = "5.0.0-rc.1"`.

The package is named `concord-api`, but in your code the library is `concord` (`concord::api!`): another project already uses the name `concord` on crates.io.

## Quick start

This example describes a client and makes one call. `tasks.example.com` stands for your API. A function replaces the network, so the program runs as it is, without a server:

```rust
use concord::{fake, json, reply, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Task {
    id: u64,
    title: String,
    done: bool,
}

// An input is `Clone`: a call can be cloned and sent again.
#[derive(Clone, Serialize)]
struct NewTask {
    title: String,
}

concord::api! {
    client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;

        POST create(task: NewTask) -> Task {
            body json(task);
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> tasks::Result<()> {
    // A function replaces the network: it receives the request and returns the response.
    // Against your real API, write `Tasks::new()` alone.
    let tasks = Tasks::new().with_transport(fake(|request| {
        assert_eq!(request.url().as_str(), "https://tasks.example.com/api/tasks");
        Ok(reply(201, r#"{ "id": 7, "title": "Write the docs", "done": false }"#))
    }));

    let task = tasks.create(NewTask { title: "Write the docs".into() }).await?;
    println!("task {} ({}) is {}", task.id, task.title, if task.done { "done" } else { "open" });
    Ok(())
}
```

It prints `task 7 (Write the docs) is open`. Without `with_transport`, `Tasks::new()` sends requests through `concord::Http`, Concord's own `reqwest` client. `.await` sends the call, and every endpoint of the client returns the same error type, `tasks::Error`. A longer version ships as an example in the repository: `cargo run -p concord-api --example quickstart`. Put the same fake inside `#[tokio::test]` and you have a test: see [testing a client](docs/guide/testing.md).

## Where to go next

- [Concord in ten minutes](docs/guide/tour.md): the whole library, code first.
- [The documentation](docs/README.md): every page, by what you want to do.
- The language reference, with an example for each statement, is the [`concord::api!`](https://docs.rs/concord-api/latest/concord/macro.api.html) page on docs.rs. Until Concord is published, read [its source](concord-macros/src/reference.md).
- [Real APIs](docs/examples/README.md) described in the language: GitHub, Stripe, Riot, Slack and more.
- [Troubleshooting](docs/guide/troubleshooting.md): when a description does not compile, or a call fails.
- Examples to run, with no network: `cargo run -p concord-api --example bookshop` (also `quickstart`, `pages`, `errors`).

Five crates share one version. You depend on `concord-api`, and on `concord-testkit` in your tests; the others come in as dependencies. See [the crates](docs/reference/crates.md) for what each one contains.

## Requirements

Concord requires Rust 1.96 or later and Tokio 1.x. Sending a call needs a Tokio runtime with time and IO enabled; building clients and calls needs no runtime.

## Known limits

- A release candidate, not on crates.io yet.
- No test calls a real API: every test runs against a fake, a script or a local server.
- Editor support inside `api! { }` (completion, go to definition) has not been checked.

See [scope and limits](docs/design/scope.md) for what Concord leaves out (a blocking client, the browser through `wasm`, the flows that obtain an OAuth token, rate limiting) and for what may change without breaking your code.

## Licence

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.

See [CHANGELOG.md](CHANGELOG.md) for the changes, including what changed from Concord 4. To work on Concord, start with [CONTRIBUTING.md](CONTRIBUTING.md).
