# Concord

Concord turns one nested description of an HTTP API into a typed Rust client over `reqwest`.

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

The first line of an endpoint (its signature) is what a caller sees; the block under it says how it goes on the wire. `Json` reads an answer and `json` writes a body: both are ordinary Rust items, the macro knows no format, and a function or a type of yours takes the same place.

## What you get

- **Typed inputs and answers.** Each endpoint is a method; a required input is an argument, an optional one is a method with its name. [Describe an API](docs/guide/describing-an-api.md)
- **The API's errors as your own type.** `problem MyProblem;` once, and `error.problem()` gives it, typed, for every endpoint. [The API's own error](docs/guide/responses-and-errors.md#the-apis-own-error)
- **Retries that never repeat a `POST` by accident.** Off until you ask; then only a request that may be repeated goes out again. [Retries](docs/guide/retries.md)
- **Pages as a stream.** One line in the description, then `.pages()` or `.pages().items()`. [Pages](docs/guide/pages.md)
- **Secrets that never appear in an error or a log.** An input of type `Secret` goes on the wire, and nowhere else. [What Concord promises](docs/guide/promises.md)
- **Tests with no network.** `with_transport(fake(…))` puts a function in place of the network; a test kit adds a local server and virtual time. [Testing a client](docs/guide/testing.md)

## Install

Concord is a release candidate, and it is **not on crates.io yet**. Depend on the repository:

```toml
[dependencies]
concord-api = { git = "https://github.com/gqsnt/concord_api" }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt"] }
```

After the first upload, the first line becomes `concord-api = "5.0.0-rc.1"`.

The package is `concord-api`; in your code the library is `concord` (`concord::api!`). The name `concord` is taken on crates.io by another project.

## Quick start

One description, one call. `tasks.example.com` stands for your API; here a function takes the place of the network, so the program runs as it is, with no server:

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
    // A function takes the place of the network: it gets the request, and gives the answer.
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

It prints `task 7 (Write the docs) is open`. Without `with_transport`, `Tasks::new()` sends with `concord::Http`, Concord's own `reqwest` client. `.await` sends the call, and every endpoint of the client fails with one type, `tasks::Error`. A longer form of it is an example of the repository: `cargo run -p concord-api --example quickstart`. The same fake inside `#[tokio::test]` is a test: [testing a client](docs/guide/testing.md).

## Where to go next

- [Concord in ten minutes](docs/guide/tour.md): the whole library, code first.
- [The documentation](docs/README.md): every page, by what you want to do.
- The language, with an example for each statement: the page of [`concord::api!`](https://docs.rs/concord-api/latest/concord/macro.api.html) on docs.rs (after the first upload; until then, [its source](concord-macros/src/reference.md)).
- [Real APIs](docs/examples/README.md) written in the language: GitHub, Stripe, Riot, Slack and more.
- [Troubleshooting](docs/guide/troubleshooting.md): when it does not compile, or a call failed.
- Examples to run, with no network: `cargo run -p concord-api --example bookshop` (also `quickstart`, `pages`, `errors`).

Five crates share one version. You depend on `concord-api`, and on `concord-testkit` in your tests; the others come with it. What each one holds: [the crates](docs/reference/crates.md).

## Requirements

Rust 1.96 or later. Tokio 1.x: sending a call needs a Tokio runtime with time and IO; building clients and calls needs none.

## Known limits

- A release candidate, not on crates.io yet.
- No test calls a real API: every test runs against a fake, a script or a local server.
- Editor support inside `api! { }` (completion, go to definition) has not been checked.

What Concord leaves outside (a blocking form, a browser with `wasm`, the flows that get an OAuth token, rate limiting), and what can change without breaking you: [scope and limits](docs/design/scope.md).

## Licence

Under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.

Changes, and what changed from Concord 4: [CHANGELOG.md](CHANGELOG.md). To work on Concord: [CONTRIBUTING.md](CONTRIBUTING.md).
