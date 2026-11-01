//! Concord: a small, typed, declarative layer over [`reqwest`].
//!
//! One [`api!`] declaration describes an HTTP API as a tree — **client → scopes → endpoints** —
//! and generates ordinary Rust types whose methods build native `reqwest::Request`s and decode
//! typed responses. Concord owns the domain structure, the meaning of required/optional/defaulted
//! values, how paths, subdomains, query parameters and headers compose, minimal body and response
//! representations, and its own redacted diagnostics. Everything about transport is reqwest's.
//!
//! # Quick start
//!
//! ```no_run
//! # #[cfg(feature = "json")] mod quick_start {
//! use concord::{Json, reqwest};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Deserialize)]
//! pub struct Issue { pub number: u64, pub title: String }
//!
//! #[derive(Serialize)]
//! pub struct NewIssue { pub title: String }
//!
//! concord::api! {
//!     /// A slice of the GitHub REST API.
//!     pub client GitHub(
//!         #[sensitive] header("authorization") token: String,
//!         header("x-github-api-version") version: String = "2022-11-28",
//!     ) {
//!         base "https://api.github.com";
//!         headers { "accept" = "application/vnd.github+json"; }
//!
//!         scope repo(owner: String, name: String) {
//!             path "repos/{owner}/{name}";
//!
//!             GET issues(query state?: String, query per_page: u32 = 30) -> Json<Vec<Issue>> {
//!                 path "issues";
//!             }
//!             GET issue(number: u64) -> Json<Issue> { path "issues/{number}"; }
//!             POST create_issue(body issue: Json<NewIssue>) -> Json<Issue> { path "issues"; }
//!         }
//!     }
//! }
//!
//! # async fn run() -> Result<(), concord::Error> {
//! let http = reqwest::Client::new(); // configure timeouts, proxies, TLS... here
//! let gh = GitHub::new(http, "Bearer ghp_...");
//! let rust = gh.repo("rust-lang", "rust");
//!
//! let open = rust.issues().set_state("open").set_per_page(50).send().await?;
//! let one = rust.issue(1).send().await?;
//! let created = rust.create_issue(NewIssue { title: "hi".into() }).send().await?;
//! # let _ = (open, one, created);
//! # Ok(()) }
//! # }
//! ```
//!
//! `concord::reqwest` is the reqwest crate Concord is built on; depend on reqwest directly (same
//! major version) to choose its features yourself.
//!
//! # The declaration
//!
//! - `client Name(values) { base "..."; ... }` — the root. `Name::new(http, required...)` creates
//!   it from **your** `reqwest::Client`; `with_base_url(..)` points it elsewhere (staging, a test
//!   server).
//! - `scope name(values) { ... }` — a navigation level: `parent.name(required...)`.
//! - `GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS name(values) -> Repr { ... }` — an endpoint:
//!   `parent.name(required...)` returns a request value with `send()` and `prepare()`.
//! - Statements, before any child: `path "a/{x}"`, `subdomain "{region}"`, `query { ... }`,
//!   `headers { ... }`.
//!
//! ## Values
//!
//! | Declaration | Meaning | Generated |
//! |---|---|---|
//! | `x: T` | required | positional argument of the owner's entry call |
//! | `x: T = expr` | initialized | `set_x(v)` |
//! | `x?: T` | optional, absent by default | `set_x(v)`, `maybe_x(Option<T>)` (`None` = absent) |
//! | `x?: T = expr` | optional, present by default | `set_x(v)`, `maybe_x(Option<T>)` |
//! | `body b: Repr` / `body b?: Repr` | request body | positional / `set_b`, `maybe_b` |
//!
//! `?` is Concord's own presence marker: `x: Option<T>` is a *required* value whose payload happens
//! to be an `Option`. Concord never infers meaning from type names. Values are passed as the exact
//! type, or `&str`/`&String` for `String` ([`IntoValue`]). `expr` runs once each time its owner is
//! created (`new`, a scope entry, an endpoint call) — never on clone or send. Handles are immutable
//! snapshots: setters return a new handle, and requests already created keep their values.
//!
//! A value is placed directly (`query x`, `query("per-page") x`, `header("x-key") x`, adding
//! `each` to repeat the key per item) or referenced from templates and blocks: `{x}` for the
//! current owner, `{client.x}` / `{scope.x}` for an enclosing one. Values are written with
//! `Display`; a type without `Display` (such as `Option<T>` or `Vec<T>`) cannot be placed without
//! `each` or a newtype that decides its spelling.
//!
//! ## Composition
//!
//! Paths append root first; subdomain labels prepend (deepest leftmost); a path value is always
//! exactly one segment (`a/b` → `a%2Fb`). Query parameters and headers are composed per key along
//! the chain:
//!
//! ```text
//! query {
//!     "k" = v;            // introduce (error if an enclosing owner already sets "k")
//!     append "k" = v;     // add occurrences
//!     override "k" = v;   // replace inherited occurrences when v is present; keep them when absent
//!     remove "k";         // drop inherited occurrences
//! }
//! ```
//!
//! All of this is decided at compile time; generated code only checks whether optional values are
//! present, and a replaced or removed value is never formatted.
//!
//! # Native escape hatches
//!
//! ```no_run
//! # concord::api! { pub client Api { base "http://x.test"; GET item(id: u64) -> concord::Text { path "items/{id}"; } } }
//! # async fn run(api: Api) -> Result<(), Box<dyn std::error::Error>> {
//! let prepared = api.item(7).prepare()?;            // no I/O
//! println!("{prepared:?}");                         // redacted rendering
//! let (client, mut request, decoder) = prepared.into_parts();
//! *request.timeout_mut() = Some(std::time::Duration::from_secs(2));
//! let response = client.execute(request).await?;    // you execute it
//! let text = decoder.decode(response).await?;       // decoded as declared
//!
//! let raw: concord::reqwest::Response = api.item(8).prepare()?.send_raw().await?;
//! # let _ = (text, raw); Ok(()) }
//! ```
//!
//! `reqwest::RequestBuilder::from_parts(client, request)` turns the parts back into a builder.
//!
//! # Bodies, responses and errors
//!
//! `Json<T>` (feature `json`), [`Text`], [`Binary`] and `()` (no output; the body is drained) are
//! built in; [`Encode`] and [`Decode`] are public for your own formats. `send()` accepts any 2xx
//! status. A non-2xx status is an [`Error`] of kind [`ErrorKind::Status`] that keeps the unread
//! response ([`Error::into_response`]) so you can decode the API's error document. `Option<T>`
//! outputs carry no status meaning: a 404 is an error, JSON `null` is `None`.
//!
//! # Sensitive values
//!
//! `#[sensitive]` marks a value. It is sent as is, but Concord withholds it — and every key, path
//! segment or host label it feeds — from the output **Concord itself produces**: generated `Debug`
//! of clients, scopes and endpoints, [`Prepared`]'s `Debug`, and [`Error`]'s `Display`, `Debug`
//! and `source()` chain. Keys named `authorization`, `cookie`, `x-api-key`, `token`, `password`
//! and a few more are treated as sensitive even without the marker.
//!
//! **Not covered:** the native `reqwest::Request`, `Url` and `Response` `Debug` output (it prints
//! full URLs), reqwest errors from requests you execute yourself, redirects (reqwest forwards
//! custom headers and a `Referer` with the query), tracing, middleware, proxies, server logs and
//! the base URL itself.
//!
//! # What Concord does not do
//!
//! No retries, authentication flows, token refresh, rate limiting, pagination, timeouts or
//! middleware — those belong to your code or to the `reqwest::Client` you pass in. One `send()` is
//! one `reqwest::Client::execute`. A retry, for example, is ordinary Rust:
//!
//! ```no_run
//! # concord::api! { pub client Api { base "http://x.test"; GET ping(); } }
//! # async fn run(api: Api) -> Result<(), concord::Error> {
//! let request = api.ping(); // endpoints without a body are `Clone`
//! let mut attempts = 0;
//! loop {
//!     match request.clone().send().await {
//!         Err(e) if e.kind() == concord::ErrorKind::Transport && attempts < 3 => attempts += 1,
//!         result => break result,
//!     }
//! }
//! # }
//! ```
//!
//! # Features
//!
//! - `json` (default): `Json<T>` via serde/serde_json.
//! - `reqwest-defaults` (default): enables reqwest's default features (TLS, HTTP/2, system proxy).
//!   Disable it to choose reqwest features in your own `Cargo.toml`.

pub use {bytes, reqwest};

mod error;
mod prepared;
mod repr;
mod value;

#[doc(hidden)]
pub mod __private;

/// Declares an HTTP API as a client → scopes → endpoints tree and generates its typed client.
///
/// See the [crate documentation](crate) for the language.
#[macro_export]
macro_rules! api {
    ($($tokens:tt)*) => {
        $crate::__private::api_impl! { $crate; $($tokens)* }
    };
}

pub use error::{Error, ErrorKind};
pub use prepared::{Decoder, Prepared};
#[cfg(feature = "json")]
pub use repr::Json;
pub use repr::{Binary, BodyError, Decode, Encode, Text};
pub use value::IntoValue;
