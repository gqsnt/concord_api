# Quick start

From an empty crate to a typed call against a local server. Every Rust block below is an excerpt
of [`examples/quickstart`](../examples/quickstart), a workspace member that is compiled, run and
checked on the wire by `cargo test -p concord-example-quickstart`; the same test fails if an
excerpt here stops matching its source.

You need Rust 1.99.1 or newer. The examples use edition 2024; other editions are not exercised.

## 1. Create a crate and add the dependencies

```sh
cargo new tasks-client
cd tasks-client
```

Concord is not published on crates.io, so depend on a checkout of this repository (or on a git
dependency pinned to a commit):

```toml
[dependencies]
concord = { path = "../concord_api/crates/concord" }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- `concord`'s default features are `json` (the `json` format) and `reqwest-defaults`
  (reqwest's TLS, HTTP/2 and proxy support); `form` (URL-encoded form bodies) is opt-in. Concord re-exports reqwest as `concord::reqwest`;
  you only need your own `reqwest` dependency to choose its features.
- `serde` derives your payload types.
- `tokio` runs the async calls (reqwest needs a Tokio runtime).

The example crate also enables Tokio's `net` and `io-util` features and depends on `serde_json`,
but only for its local stand-in server.

## 2. Describe the payloads

Plain serde types. Excerpt of
[`examples/quickstart/src/lib.rs`](../examples/quickstart/src/lib.rs):

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
```

## 3. Declare the API

Excerpt of [`examples/quickstart/src/lib.rs`](../examples/quickstart/src/lib.rs):

```rust
concord::api! {
    /// A small task-tracking API.
    pub client Tasks(#[sensitive] token: String) {
        base "https://tasks.example.com/api";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format json;

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
```

Read it top-down:

- `pub client Tasks(#[sensitive] token: String)` — the client and its one **required** value. You
  supply it through the generated `TasksConfig { token }` when creating the client.
  `#[sensitive]` keeps it out of Concord's `Debug` output and error messages; it is still sent.
- `base` — the URL every path is appended to; `with_base_url` replaces it at runtime.
- `headers { AUTHORIZATION = "Bearer {token}"; }` — sent with every request of this client.
  `AUTHORIZATION` is reqwest's own `HeaderName` constant (imported from `concord::reqwest::header`);
  a literal name works too (`"authorization" = ..`). `{token}` is a template hole that reads the
  client's value.
- `format json;` — the **format** of the endpoints below, in both directions (`request json;` and
  `response json;` say one direction each): a body declared `body x: T` is sent as JSON, a typed
  output `-> T` is decoded from JSON. An endpoint that differs says so with the same statements in its block —
  `GET readme() -> String { path "readme"; response text; }`,
  `PUT image(body png: Vec<u8>) { path "image"; request bytes "image/png"; }` — and the nearest
  statement wins; one without `->` decodes nothing and accepts any 2xx. A body's `Content-Type` is
  its format's (`application/json`, ...) unless its `request` statement names another
  (`request json "application/merge-patch+json";`). Nothing checks that the server agrees — the
  generated docs of each endpoint state the format and `Content-Type` it uses and where they come
  from. The format, not the Rust type, decides the wire: a `String` or `Vec<u8>` body left under
  `format json;` is sent as a JSON string or array, without any error.
- `scope project(project: String)` — a navigation level, `api.project("demo")`, that adds the path
  `projects/demo`. The value is formatted with `Display` and percent-encoded as one segment.
- `GET tasks(query done?: bool, query limit: u32 = 20) -> Vec<Task>` — an endpoint with an
  **optional** query parameter (`?`: absent unless set) and an **initialized** one (`= 20`: present
  unless changed). The response is decoded from JSON (the client's `format json;`) as
  `Vec<Task>`.
- `POST create(body task: NewTask) -> Task` — a JSON request body supplied as an argument (sent
  with `Content-Type: application/json`), and a JSON response.
- Not shown: a value can also be placed as a header, `header("x-request-id") id: String`
  (shorthand for `headers { "x-request-id" = id; }`). A header name can be a native
  `reqwest::header::HeaderName` constant instead of a literal — `header(AUTHORIZATION) token: String`,
  `headers { ACCEPT = "application/json"; }`, or your own
  `const X_API_KEY: HeaderName = HeaderName::from_static("x-api-key")`. A value can be a native
  `HeaderValue`, sent as is with its sensitivity flag. Concord adds no `Accept` header: declare one
  if the API needs it (a reqwest client without one sends its default, `accept: */*`).

The macro generates ordinary types: `Tasks`, `TasksConfig`, `TasksProject` (the scope) and one
request type per endpoint, `TasksProjectTasks` and `TasksProjectCreate`. Their main signatures, as
rustdoc shows them (`impl IntoValue<T>` accepts a `T`, or a `&str`/`&String` where `T` is
`String`; `examples/quickstart/tests/quickstart.rs` checks these signatures):

```text
Tasks::new(http: reqwest::Client, config: TasksConfig) -> Tasks
Tasks::with_base_url(self, url: impl AsRef<str>) -> Result<Tasks, concord::Error>
Tasks::set_token(self, value: impl IntoValue<String>) -> Tasks
Tasks::project(&self, project: impl IntoValue<String>) -> TasksProject
TasksProject::tasks(&self) -> TasksProjectTasks
TasksProject::create(&self, task: impl IntoValue<NewTask>) -> TasksProjectCreate
TasksProjectTasks::set_done(self, value: impl IntoValue<bool>) -> TasksProjectTasks
TasksProjectTasks::maybe_done(self, value: Option<bool>) -> TasksProjectTasks
TasksProjectTasks::set_limit(self, value: impl IntoValue<u32>) -> TasksProjectTasks
TasksProjectTasks::prepare(self) -> Result<concord::Prepared<Vec<Task>>, concord::Error>
async TasksProjectTasks::send(self) -> Result<Vec<Task>, concord::Error>
```

`cargo doc --open` in your crate shows them with the documentation Concord generates (which
values are required, initial values, what absence means).

## 4. Configure the client, call, handle the result

Excerpt of [`examples/quickstart/src/lib.rs`](../examples/quickstart/src/lib.rs):

```rust
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
        Err(error) => {
            let response = error.into_response()?; // any other kind of error propagates
            let status = response.status();
            let problem = <Json as Decode<Problem>>::decode(response.bytes().await?)?;
            println!("rejected with {status}: {}", problem.title);
        }
    }
    Ok(())
}
```

- **The client.** `Tasks::new` takes *your* `reqwest::Client` and the required values. Build the
  client once and reuse it: handles are cheap to clone and share (`Arc` snapshots).
- **A call** is navigation plus an endpoint: `api.project("demo").tasks()` returns a request value;
  setters such as `set_done(false)` return the changed request; `send()` executes it once and
  decodes a 2xx response.
- **Errors.** Every call returns `Result<_, concord::Error>`. `error.kind()` tells the stage
  (`Build`, `Transport`, `Status`, `Decode`). A non-2xx status keeps the **unread** response:
  `into_response()` returns it (and returns any other error unchanged, so `?` propagates it), and
  `Decode` reads the API's error document with any format. The
  [caller guide](CALLER_GUIDE.md#6-errors) covers the rest.

## 5. Run it locally

The example's binary starts a stand-in for the service on 127.0.0.1 and runs the code above
against it. Excerpt of [`examples/quickstart/src/main.rs`](../examples/quickstart/src/main.rs):

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = "local-demo-token";
    // A stand-in for https://tasks.example.com on 127.0.0.1, so the example runs offline.
    let server = LocalServer::start(tasks_service(token)).await?;
    concord_example_quickstart::run(&format!("{}/api", server.url()), token.to_owned()).await
}
```

```sh
cargo run -p concord-example-quickstart
```

Output (the port varies):

```text
Tasks { base: "http://127.0.0.1:39623/api", token: <redacted> }
created task #1: Write the quick start
1 open task(s)
rejected with 422 Unprocessable Entity: a task needs a title
```

The stand-in (`examples/quickstart/src/local_server.rs`) is example scaffolding, not part of
Concord: a short Tokio program that answers one request per connection, keeps the tasks in memory
and records what reqwest sent. In your own crate, point `with_base_url` at whatever local or staging server you use, or
leave it out to call the declared base.

## 6. When the API uses subdomains

`with_base_url("http://127.0.0.1:8080")` works for declarations without `subdomain`. With
subdomain labels the host must be a domain name (an IP base is a build error when the request is
prepared). Keep a domain in the base URL and map it to the local address with
`reqwest::ClientBuilder::resolve`. Excerpt of
[`examples/shapes/tests/wire.rs`](../examples/shapes/tests/wire.rs), where the `regional` scope
declares `subdomain "{region}"`:

```rust
fn regional_client(port: u16) -> reqwest::Client {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    reqwest::Client::builder()
        .resolve("eu.api.example.test", addr)
        .build()
        .unwrap()
}
```

Excerpt of [`examples/shapes/tests/wire.rs`](../examples/shapes/tests/wire.rs):

```rust
    let api = s2_scoped::CatalogApi::new(
        regional_client(port),
        s2_scoped::CatalogApiConfig {
            api_key: "k-123".into(),
        },
    )
    .with_base_url(format!("http://api.example.test:{port}/v2"))
    .unwrap();
```

## Next steps

- [Author guide](AUTHOR_GUIDE.md): scopes, the three kinds of values, paths and subdomains, query
  and header composition, body and response formats, sensitivity.
- [Caller guide](CALLER_GUIDE.md): handles and snapshots, optional values, errors, native requests.
- [Recipes](RECIPES.md): per-request timeouts, streaming, custom body formats, retries,
  pagination, rate admission — written as application code.
- [Diagnostics](DIAGNOSTICS.md): what the compile errors look like and how to fix them.
