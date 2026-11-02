//! Typed HTTP API clients over [`reqwest`](https://docs.rs/reqwest/0.13), generated from one
//! declaration.
//!
//! One [`api!`] declaration describes an HTTP API as a tree — **client → scopes → endpoints** —
//! and generates ordinary Rust types whose methods build native `reqwest::Request`s and decode
//! typed responses. Concord owns the structure of the API, the meaning of required, optional and
//! initialized values, how paths, subdomains, query parameters and headers compose, the formats
//! of bodies and typed responses, and its own redacted output. You supply the `reqwest::Client`;
//! authentication flows, retries, rate limiting, pagination and transport configuration stay in
//! your code.
//!
//! # Quick start
//!
//! ```
//! # #[cfg(feature = "json")] mod quick_start {
//! use concord::reqwest;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Deserialize)]
//! pub struct Issue {
//!     pub number: u64,
//!     pub title: String,
//! }
//!
//! #[derive(Serialize)]
//! pub struct NewIssue {
//!     pub title: String,
//! }
//!
//! concord::api! {
//!     /// A slice of a code-hosting API.
//!     pub client Forge(
//!         #[sensitive] header("authorization") token: String,
//!         header("x-api-version") version: String = "2024-01-01",
//!     ) {
//!         base "https://api.forge.example";
//!         request json;  // bodies declared `body x: T` are JSON
//!         response json; // so are typed outputs `-> T`
//!
//!         scope repo(owner: String, name: String) {
//!             path "repos/{owner}/{name}";
//!
//!             GET issues(query state?: String, query per_page: u32 = 30) -> Vec<Issue> {
//!                 path "issues";
//!             }
//!             POST create_issue(body issue: NewIssue) -> Issue {
//!                 path "issues";
//!             }
//!             GET readme() -> text String { path "readme"; } // an exception, written in place
//!             DELETE lock() { path "lock"; } // no `->`: any 2xx, the body is drained
//!         }
//!     }
//! }
//!
//! # pub fn main() -> Result<(), concord::Error> {
//! let http = reqwest::Client::new(); // your client: timeouts, proxies, TLS, default headers
//! let forge = Forge::new(http, ForgeConfig { token: "Bearer <token>".into() });
//! let repo = forge.repo("rust-lang", "rust");
//!
//! // Build a request without sending it:
//! let prepared = repo.issues().set_state("open").prepare()?;
//! assert_eq!(
//!     prepared.request().url().as_str(),
//!     "https://api.forge.example/repos/rust-lang/rust/issues?state=open&per_page=30",
//! );
//!
//! // Send: one request, a 2xx check, JSON decoding.
//! async fn open_issues(repo: &ForgeRepo) -> Result<Vec<Issue>, concord::Error> {
//!     repo.issues().set_state("open").send().await
//! }
//! # Ok(()) }
//! # }
//! # #[cfg(feature = "json")] fn main() -> Result<(), concord::Error> { quick_start::main() }
//! # #[cfg(not(feature = "json"))] fn main() {}
//! ```
//!
//! `concord::reqwest` is the reqwest crate Concord is built on; depend on reqwest directly (same
//! major version) to choose its features.
//!
//! # What a declaration generates
//!
//! For the declaration above:
//!
//! | Declared | Generated | Main methods |
//! |---|---|---|
//! | `client Forge(..)` | `Forge`, a cheap-to-clone handle | `Forge::new(http, ForgeConfig { .. })` (`Forge::new(http)` without required values), `with_base_url`, `set_*`/`maybe_*`, `repo(..)` |
//! | its required values | `ForgeConfig`, one public field per value | — |
//! | `scope repo(..)` | `ForgeRepo`, a handle | `set_*`/`maybe_*`, `issues()`, `create_issue(..)` |
//! | `GET issues(..)` | `ForgeRepoIssues`, a request value | `set_*`/`maybe_*`, `prepare()`, `send()` |
//!
//! Each value `x` gets `set_x(v)`; an optional value (`x?: T`) also gets `maybe_x(Option<T>)`.
//! Required scope and endpoint values are positional arguments of the navigation method, in
//! declaration order. Generated items carry documentation: which values are required, the initial
//! value of each and what absence means.
//!
//! # Runtime types
//!
//! | Item | Role |
//! |---|---|
//! | [`api!`] | the declaration macro |
//! | [`Prepared<T>`](Prepared) | a built request, not yet sent: `request()`, `send()`, `send_raw()`, `into_parts()` |
//! | [`Decoder<T>`](Decoder) | an endpoint's response handling, for a response you obtained natively: `decode`, `finish` |
//! | [`Error`], [`ErrorKind`] | the one error type: `kind`, `endpoint`, `status`, `into_response`, `transport`, `body_source` |
//! | [`Encode<T>`](Encode), [`Decode<T>`](Decode) | format traits: the words `json`, `text`, `bytes`, `form` name `Json` (feature `json`), [`Text`], [`Binary`], `Form` (feature `form`); any other format is your own type, named by its path |
//! | [`BodyError`] | an encoding or decoding failure with a content-free description |
//! | [`IntoValue<T>`](IntoValue) | the closed set of conversions accepted by arguments and setters |
//! | [`ToHeaderValue`] | how a placed value becomes a header value: native `HeaderValue`, text, integers, `bool`, or your own type |
//! | `concord::reqwest`, `concord::bytes` | the re-exported [`reqwest`](https://docs.rs/reqwest/0.13) and [`bytes`](https://docs.rs/bytes/1) crates |
//!
//! # The declaration at a glance
//!
//! ```text
//! client Name(values) { base "https://..."; statements; children }
//! scope name(values) { statements; children }
//! GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS name(values) [-> output] { statements }
//!
//! values:      x: T  |  x: T = expr  |  x?: T  |  x?: T = expr
//! placements:  query x  |  query("name") x  |  header("name") x  |  ... each x
//! body:        body b: T  |  body("media/type") b: T  |  FORMAT b: T  |  FORMAT("media/type") b: T
//!              (at most one, endpoints only; `b?: T` for an optional body)
//! output:      -> T  |  -> FORMAT T  (no `->`: no typed output, any 2xx, the body is drained)
//! FORMAT:      json  |  text  |  bytes  |  form  |  a type path implementing Encode/Decode
//! attributes:  /// docs  |  #[sensitive] (values)  |  #[cfg(..)], #[deprecated] (scopes, endpoints)
//! statements:  path "a/{x}";  subdomain "{x}";  query { .. }  headers { .. }
//!              request FORMAT;  request FORMAT("media/type");  response FORMAT;  (clients, scopes)
//!
//! query / headers blocks:
//!     "k" = v;                        set a key no enclosing owner sets
//!     append "k" = v;                 add occurrences after the inherited ones
//!     override "k" = v;               replace the inherited occurrences (v always present)
//!     override "k" = v else inherit;  v can be absent: replace when present, keep when absent
//!     remove "k";                     drop the inherited occurrences (acts first in its owner)
//! ```
//!
//! `?` is Concord's presence marker; no meaning is inferred from type names. Every body and typed
//! output has a format: written in place (`json item: T`, `-> text String`) or, for `body b: T` and
//! a bare `-> T`, the nearest `request`/`response` default of an enclosing scope or the client. Path segments,
//! subdomain labels, query values and template holes are written with `Display`; a value placed
//! directly as a header goes through [`ToHeaderValue`], so a native `HeaderValue` is sent as is.
//! A bare name refers to the current owner; `client.x` or `scope.x` reaches an enclosing owner.
//! All composition is decided at compile time; generated code only checks whether optional values
//! are present.
//!
//! # Native requests
//!
//! ```no_run
//! # concord::api! { pub client Api { base "http://x.test"; GET item(id: u64) -> text String { path "items/{id}"; } } }
//! # async fn run(api: Api) -> Result<(), Box<dyn std::error::Error>> {
//! let prepared = api.item(7).prepare()?; // no I/O
//! println!("{prepared:?}"); // redacted rendering
//! let (client, mut request, decoder) = prepared.into_parts();
//! *request.timeout_mut() = Some(std::time::Duration::from_secs(2));
//! let text = decoder.finish(client.execute(request).await).await?; // decoded as declared
//!
//! let raw: concord::reqwest::Response = api.item(8).prepare()?.send_raw().await?;
//! # let _ = (text, raw); Ok(()) }
//! ```
//!
//! One `send()` is one `reqwest::Client::execute`: Concord performs no retry, and a request's
//! `Clone` says nothing about whether sending it again is safe.
//!
//! # Sensitive values
//!
//! `#[sensitive]` marks a value. It is sent as is, but Concord withholds it — and every key, path
//! segment or host label it feeds — from the output **Concord itself produces**: the generated
//! `Debug` of clients, config structs, scopes and requests, [`Prepared`]'s `Debug`, and
//! [`Error`]'s `Display` and `Debug`. Keys named `authorization`, `cookie`, `x-api-key`, `token`,
//! `password` and a few more are treated as sensitive without the marker. A native `HeaderValue`
//! whose own sensitivity flag is set stays flagged and is withheld the same way, marker or not.
//! `Error` has no `source()` chain: the underlying reqwest or decoder error is reachable only through
//! [`Error::transport`] and [`Error::body_source`], which are not redacted.
//!
//! **Not covered:** the native `reqwest::Request`, `Url` and `Response` `Debug` output (it prints
//! full URLs), reqwest errors from requests you execute yourself, redirects (reqwest forwards
//! custom headers and a `Referer` with the query), tracing, middleware, proxies, server logs and
//! the base URL itself.
//!
//! # Guides
//!
//! In the source repository:
//!
//! - `docs/QUICKSTART.md` — from a new crate to a call against a local server;
//! - `docs/AUTHOR_GUIDE.md` — writing declarations: values, paths, composition, formats;
//! - `docs/CALLER_GUIDE.md` — using a generated client: snapshots, options, errors, native requests;
//! - `docs/RECIPES.md` — error documents, streaming, custom formats, retries, pagination;
//! - `docs/DIAGNOSTICS.md` — the compile errors and how to fix them;
//! - `docs/MIGRATION.md` — from `via` to formats, from the V4 alpha and from V3;
//! - `v4/SEMANTICS.md` — the precise rules.
//!
//! # Features
//!
//! - `json` (default): the `json` format (`Json`), via serde and serde_json.
//! - `form`: the `form` format (`Form`), `application/x-www-form-urlencoded` via serde and
//!   serde_urlencoded. Off by default, like reqwest's own `form` feature.
//! - `reqwest-defaults` (default): reqwest's default features (TLS, HTTP/2, system proxy).
//!   Disable it to choose reqwest's features in your own `Cargo.toml`.

pub use {bytes, reqwest};

mod error;
mod header;
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
pub use header::ToHeaderValue;
pub use prepared::{Decoder, Prepared};
#[cfg(feature = "form")]
pub use repr::Form;
#[cfg(feature = "json")]
pub use repr::Json;
pub use repr::{Binary, BodyError, Decode, Encode, Text};
pub use value::IntoValue;
