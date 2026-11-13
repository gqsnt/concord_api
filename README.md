# Concord V5

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

The signature is for the user; the block is for the wire. `json` and `Json` are ordinary Rust items: the macro knows no format, and a reader or a writer of yours takes the same place.

## Where it stands

| | |
|---|---|
| The design | Finished on paper, in [`docs/`](docs). The whole contract on one page: [docs/5.0.md](docs/5.0.md). |
| The library | Built in [`api/`](api), as five crates of version `5.0.0-rc.1`. Not published: every crate says `publish = false`. |
| The crates | `concord-core` (the engine), `concord-macros` (`api!`), `concord-extras` (`json`, `form`, `multipart`, the readers, the pagers, `basic`, `tower`), `concord-testkit` (for tests), and `concord`, the facade. |
| The clients of the tour | Written by the macro in [`api/tour-gen`](api/tour-gen/src), and by hand in [`api/tour`](api/tour/src). Both pass the same tests. |
| The contracts | 99, of which 93 hold with a test that carries their number and 6 hold with no test yet ([docs/contracts.md](docs/contracts.md)). |
| The messages | 46 at compile time, each compared with [docs/messages.md](docs/messages.md) by a test; 52 lines at run time, of which a test produces each one that a client of the tour can give. |
| The gates | Every milestone, M0 to M6, is passed. The last run of `just release --with-sockets` (13 November 2026) passed whole: the gate (1,587 tests, the doc tests, the docs, Rust 1.96, the features), the list of places that could panic, the five packages, 554 feature checks, and 147 of 147 mutations caught. The report is [RUN-BUILD.md](RUN-BUILD.md). |
| Only the owner | Check the editor inside `api! { }`. Make a real call, with a key: no test calls a real API. Choose the licence, the repository and the names of the crates, before any upload. |

## Read

| Page | What it answers |
|---|---|
| [The tour](docs/tour.md) | Ten minutes, code first. Then chapters [1](docs/1-why.md) to [9](docs/9-background.md). |
| [5.0 on one page](docs/5.0.md) | What a description may say, what a caller gets, what Concord promises. |
| [The grammar](docs/grammar.md), [the rules](docs/rules.md) | The language, and the things a user cannot guess. |
| [The contracts](docs/contracts.md), [the cases](docs/cases.md) | What the engine and the generated code must hold, and every case with its one outcome. |
| [The engine](docs/engine.md), [the macro](docs/macro.md) | How a call runs, and how a description becomes code; what a call costs, and the size of a client. |
| [The crates](docs/crate.md), [engineering](docs/engineering.md) | The public surface, and the properties it is held to. |
| [Real APIs](docs/apis/README.md) | Eight APIs written in the language. |
| [DECISIONS.md](DECISIONS.md) | Who decided what, in order. |

A signature in `api/` wins over a page; `docs/` wins over the rest.

## Build and test

From `api/`, offline, with Rust 1.96 or later (some tests open a socket on `127.0.0.1`):

```bash
cargo test --offline --workspace --features concord/tower,concord/multipart
just gate        # the properties, format, check, lints, tests, docs, Rust 1.96, features, TODOs, the matrix
```

The working material of the design (the runs, the experiments, the prototype, the briefs and reports of the agents that built the library) is not in this repository. Where a page names one of those files, the name is kept as plain text.

Licensed under either of Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)) or MIT license ([LICENSE-MIT](LICENSE-MIT)) at your option.
