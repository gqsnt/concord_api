# Concord

Concord turns one declaration of an HTTP API into typed Rust methods that build native
[`reqwest`](https://docs.rs/reqwest) requests and decode typed responses.

Compared with a small handwritten reqwest client, the declaration replaces:

- URL assembly for every endpoint: joining the base URL, percent-encoding each path value as
  exactly one segment, building subdomain labels;
- the plumbing of shared query parameters and headers (an API version, an authorization header)
  into every endpoint, with explicit rules for overriding or removing them further down — header
  names written as literals or as native `HeaderName` constants (`header(AUTHORIZATION)`, your own
  `X_API_KEY`), composed by the header they name, and checked by `http` itself;
- bookkeeping of which optional parameters are present;
- body encoding (stated once for a mostly-JSON API with `format Json;`, and on an endpoint that
  differs: `response Text;`), the status check and response decoding, with the format checking a
  response's `Content-Type` before it parses the body;
- keeping secrets out of the client's `Debug` output and out of error messages.

Mistakes in the declaration — an unknown value, an invalid header name or base URL, a type without
a wire spelling — are compile errors that point at the declaration.

Concord deliberately does **not** manage authentication flows (obtaining or refreshing
credentials), retries, rate limiting, pagination or transport configuration (timeouts, proxies,
TLS, redirects, connection pools). Those stay in your code and in the `reqwest::Client` you pass
in; [`examples/native`](examples/native) shows how to compose them, and
[`examples/riot`](examples/riot) is a complete, sourced client for part of the Riot Games API,
tested offline (its README says how to copy it out of this workspace).

## Example

Excerpt of [`examples/quickstart/src/lib.rs`](examples/quickstart/src/lib.rs), compiled and run
against a local stand-in server by `cargo test -p concord-example-quickstart`:

```rust
use concord::reqwest::{self, header::AUTHORIZATION};
use concord::{Decode, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct NewTask {
    pub title: String,
}

/// The error document the API sends with a non-2xx status.
#[derive(Debug, Deserialize)]
pub struct Problem {
    pub title: String,
}

concord::api! {
    /// A small task-tracking API.
    pub client Tasks(#[sensitive] token: String) {
        base "https://tasks.example.com/api";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;

        scope project(project: String) {
            path "projects/{project}";

            GET tasks(query done?: bool, query limit: u32 = 20) -> Vec<Task> {
                path "tasks";
            }
            POST create(body task: NewTask) -> Task {
                path "tasks";
            }
        }
    }
}

pub async fn run(base_url: &str, token: String) -> Result<(), Box<dyn std::error::Error>> {
    // Transport settings (timeouts, proxies, TLS) belong to your reqwest::Client.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    // `with_base_url` points the client at the local stand-in; leave it out for the real API.
    let api = Tasks::new(http, TasksConfig { token }).with_base_url(base_url)?;
    println!("{api:?}"); // the token is shown as <redacted>

    let project = api.project("demo");
    let title = "Write the quick start".to_owned();
    let task = project.create(NewTask { title }).send().await?; // POST .../projects/demo/tasks
    println!("created task #{}: {}", task.id, task.title);

    let open = project.tasks().set_done(false).send().await?; // GET ...?done=false&limit=20
    println!("{} open task(s)", open.len());

    // This API rejects an empty title: a non-2xx status is an error that keeps the response.
    match project.create(NewTask::default()).send().await {
        Ok(task) => println!("unexpectedly created task #{}", task.id),
        Err(error) if error.status().is_some() => {
            let response = error.into_response()?; // a status error keeps its response
            let status = response.status();
            let problem = <Json as Decode<Problem>>::decode_body((), response.bytes().await?)?;
            println!("rejected with {status}: {}", problem.title);
        }
        Err(error) => return Err(error.into()), // any other kind of error propagates
    }
    Ok(())
}
```

`cargo run -p concord-example-quickstart` runs it against a server on 127.0.0.1; the
[quick start](docs/QUICKSTART.md) walks through it.

## Formats

A format is an ordinary Rust type that implements `Encode<T>` and/or `Decode<T>`, named by its
path in a statement: `format Json;`, `response Text;`, `request my_sdk::Csv;`. Concord ships
`Json`, `Text`, `Binary`, `Form` and the adapter `IgnoreContentType<F>`; they have no privilege
over a format of yours.

- On a successful response the format checks the response head **before the body is read**.
  `Json` requires exactly one `Content-Type`, `application/json` or `application/*+json`; a
  response labelled `text/html`, or not labelled, is an `ErrorKind::Metadata` error that keeps the
  response unread. Where a server cannot be fixed, `response IgnoreContentType<Json>;` on that
  endpoint skips only that check.
- An endpoint with a typed output sends its format's `Accept` suggestion (`Json`:
  `application/json`) unless you declare an `accept` header, which is sent as written. The
  response `Content-Type` is never compared with it.
- Your own format reads the head (a version header, say), keeps typed state for its body decoder
  and reports positions without content: [Writing a format](docs/CUSTOM_FORMATS.md).

## The transport boundary

You build the `reqwest::Client` and pass it to `new`; Concord uses it for every request of that
client and never configures it. One `send()` is one `reqwest::Client::execute` — no retry, no
extra request. For anything the declaration does not cover there is a native exit:

- `prepare()` builds the native `reqwest::Request` without any I/O;
- `prepare()?.into_parts()` returns your client, that request and the endpoint's decoder, so you
  can change the request (a per-request timeout with `*request.timeout_mut() = Some(..)`, an extra
  header, another body), execute it yourself and still decode the response as declared
  (`decoder.finish(client.execute(request).await).await`). The native request's `Debug` is
  reqwest's: it prints the full URL and every header value not flagged sensitive;
- `prepare()?.send_raw()` returns the native `reqwest::Response`, unchecked and unread.

## Status

- Version `4.2.0` ([changelog](CHANGELOG.md)), **not published** on crates.io
  (`publish = false`). Depend on a checkout by path, or on a git dependency pinned to a
  commit or a release tag (4.1.0 is `v4.1.0`). Breaking changes and upgrade steps are in
  [`docs/MIGRATION.md`](docs/MIGRATION.md).
- Requires Rust 1.99.1 or newer (`rust-version`, edition 2024); the same release is pinned in
  `rust-toolchain.toml` for development. CI runs the complete gate
  (`scripts/verify.sh --package --bench`) natively on Linux x86_64 and Windows x86_64; the
  evidence of each release is recorded under `v4/`.

## Installation and features

```toml
[dependencies]
concord = { path = "../concord_api/crates/concord" } # a checkout of this repository
serde = { version = "1", features = ["derive"] }     # for your JSON payload types
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

| Feature | Default | Effect |
|---|---|---|
| `json` | yes | the `Json` format (`format Json;`, `request Json;`, `response Json;`), using serde and serde_json; implies `extras` |
| `extras` | yes (through `json`) | the standard formats without serializer dependencies: `Text`, `Binary` and the adapter `IgnoreContentType<F>` |
| `form` | no | the `Form` format (`request Form;`, `application/x-www-form-urlencoded`), using serde and serde_urlencoded; implies `extras` |
| `reqwest-defaults` | yes | reqwest's own default features (TLS, HTTP/2, system proxy) |

The runtime lives in `concord-core` and the standard formats in `concord-extras`; `concord`
re-exports both with the `api!` macro, so an application depends on `concord` only. With
`default-features = false` there is no standard format at all (a format of your own works the
same: implement `concord::Encode`/`concord::Decode`); enable `extras`, `json` or `form` for them.
A crate that only offers formats can depend on `concord-core` instead.

With `default-features = false`, choose reqwest's features yourself: depend on `reqwest` (the
same major version that Concord re-exports as `concord::reqwest`) with the features you need.

reqwest is part of Concord's public API (`concord::reqwest`, `Prepared::request`, the native
exits). A Concord release keeps one reqwest minor series (0.13 for 4.x); moving to another is a
breaking Concord release.

## Documentation

| Task | Guide |
|---|---|
| Set up a crate and make a first call | [Quick start](docs/QUICKSTART.md) |
| The whole language on one page | [Cheat sheet](docs/CHEATSHEET.md) |
| Write a declaration: scopes, values, paths, query and headers, body and response formats | [Author guide](docs/AUTHOR_GUIDE.md) |
| Use a generated client: construction, options, errors, native requests | [Caller guide](docs/CALLER_GUIDE.md) |
| Write your own `Encode`/`Decode` format; read the response head; test it | [Writing a format](docs/CUSTOM_FORMATS.md) |
| What goes on the wire: `Accept` and `Content-Type` | [Author guide, 5.8](docs/AUTHOR_GUIDE.md#58-what-goes-on-the-wire-accept-and-content-type) |
| Error documents, timeouts, streaming, retries, pagination; patterns from other APIs | [Recipes](docs/RECIPES.md), [`examples/compat`](examples/compat) |
| Build an SDK around a declaration: domain values to operations, named workflows, bounded traversal, response metadata, operations the SDK does not declare | [Recipes: SDK and application patterns](docs/RECIPES.md#sdk-and-application-patterns), [`examples/app`](examples/app), [`examples/extension`](examples/extension) |
| Read a compile error | [Diagnostics](docs/DIAGNOSTICS.md) |
| Upgrade from format words and the body-only codec traits, from in-place formats, from `via`, from the V4 alpha or from V3 | [Migration](docs/MIGRATION.md) |
| A complete real-world client (Riot Games), offline tests, sources | [`examples/riot`](examples/riot) |
| The precise rules (truth tables, ordering, sensitivity) | [Semantics](v4/SEMANTICS.md) |
| Work on Concord itself | [Maintainers](docs/MAINTAINERS.md) |

The API reference is the crate documentation: `cargo doc -p concord --open`.

## License

MIT OR Apache-2.0, at your option.
