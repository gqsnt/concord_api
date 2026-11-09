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
//! use concord::reqwest::{self, header::{AUTHORIZATION, HeaderName}};
//! use concord::{Json, Text};
//! use serde::{Deserialize, Serialize};
//!
//! /// A header of this API, declared the native way.
//! pub const X_API_VERSION: HeaderName = HeaderName::from_static("x-api-version");
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
//!         #[sensitive] header(AUTHORIZATION) token: String,
//!         header(X_API_VERSION) version: String = "2024-01-01",
//!     ) {
//!         base "https://api.forge.example";
//!         format Json; // bodies `body x: T` and typed outputs `-> T` are JSON
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
//!             GET readme() -> String { path "readme"; response Text; } // an exception, on its endpoint
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
//! | `client Forge(..)` | `Forge`, a cheap-to-clone handle | `Forge::new(http, ForgeConfig { .. })` (`Forge::new(http)` without required values), `with_base_url` (literal base only), `http()`, `base_url()`, value getters, `set_*`/`maybe_*`, `repo(..)` |
//! | its required values | `ForgeConfig`, one public field per value | — |
//! | `scope repo(..)` | `ForgeRepo`, a handle | `client()`, value getters, `set_*`/`maybe_*`, `issues()`, `create_issue(..)` |
//! | `GET issues(..)` | `ForgeRepoIssues`, a request value | `set_*`/`maybe_*`, `prepare()`, `send()` |
//!
//! Each value `x` gets `set_x(v)`; an optional value (`x?: T`) also gets `maybe_x(Option<T>)`.
//! A required client or scope value is fixed for the handle's life unless declared `mut x: T`.
//! Client and scope values are read with `x()` (crate-only when sensitive).
//! Required scope and endpoint values are positional arguments of the navigation method, in
//! declaration order. Generated items carry documentation: which values are required, the initial
//! value of each and what absence means.
//!
//! # Runtime types
//!
//! | Item | Role |
//! |---|---|
//! | [`api!`] | the declaration macro |
//! | [`Prepared<T>`](Prepared) | a built request, not yet sent: `request()`, `receive()`, `send()`, `send_raw()`, `into_parts()` |
//! | [`Received<T>`](Received) | any-status response, body unread: `head()`, `meta()`, then `decode()`, `decode_limited()`, `bytes_limited()`, `into_incoming()` or a native exit |
//! | [`Incoming`] | an unread 2xx body with its operation identity, for an incremental reader chosen at run time |
//! | [`Decoder<T>`](Decoder) | a response handling (an endpoint's, or [`Decoder::new`] for a format of your choice), for a response you obtained natively: `decode`, `finish` |
//! | [`Endpoint<T>`](Endpoint), [`SendWith<T>`](SendWith) | generated operations implement `Endpoint`; a client's `send policy;` delegates generated `send()` to the policy's `SendWith::send`, returning its `Output` |
//! | [`SendOperation<T, O>`](SendOperation) | under `operations Name;`, the policy's trait instead of `SendWith`: generated `send()` also passes its endpoint's variant of the generated enum `Name`, for a `match` without a wildcard arm |
//! | [`Error`], [`ErrorKind`] | Concord's error type: `kind`, `endpoint`, `status`, `response`, `into_response`, `codec_error`, `transport` |
//! | [`Encode<T>`](Encode), [`Decode<T>`](Decode) | the open format traits, implemented by `Json` (feature `json`), `Text`, `Binary`, `IgnoreContentType<F>` (feature `extras`), `Form` (feature `form`) and by your own types; a statement names a format by its type path (`format Json;`) |
//! | [`Incremental<T>`](Incremental) | the open format contract for `-> stream T`: checks the head, then opens its reader over an unread `Incoming` |
//! | [`ResponseHead`], [`ContentTypeField`] | what a [`Decode`] or [`Incremental`] implementation reads before the body: status, version, header fields |
//! | [`ResponseMeta`] | an owned copy of a response head with its [`Operation`], kept after the body is consumed |
//! | [`CodecError`] | a format's failure: a static label, numeric positions, an unredacted source |
//! | [`IntoValue<T>`](IntoValue) | the closed set of conversions accepted by arguments and setters |
//! | [`ToHeaderValue`] | the closed set of header values: native `HeaderValue`, text, integers, `bool` (another type is sent through a template) |
//! | `concord::reqwest`, `concord::bytes` | the re-exported [`reqwest`](https://docs.rs/reqwest/0.13) and [`bytes`](https://docs.rs/bytes/1) crates |
//!
//! # The declaration at a glance
//!
//! ```text
//! client Name(values) { base "https://..."; statements; children }
//! scope name(values) { statements; children }
//! GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS name(values) [-> T | -> stream T | -> stream] { statements }
//!
//! values:      x: T  |  x: T = expr  |  x?: T  |  x?: T = expr
//! placements:  query x  |  query("name") x  |  header(KEY) x  |  ... each x
//! KEY:         "x-name"  |  a native HeaderName constant or static: AUTHORIZATION, X_API_KEY, ...
//! body:        body b: T  |  body b?: T   (at most one, endpoints only)
//! output:      -> T (finite)  |  -> stream T (Incremental<T> reader)  |  -> stream (Incoming)
//!              no `->`: no typed output, any 2xx, the body is drained
//! FORMAT:      a type path implementing Encode, Decode and/or Incremental: Json, Text, yours
//! attributes:  /// docs  |  #[sensitive] (values)  |  #[cfg(..)], #[deprecated] (scopes, endpoints)
//! statements:  path "a/{x}";  subdomain "{x}";  query { .. }  headers { .. }
//!              format FORMAT;  request FORMAT;  request FORMAT "media/type";  response FORMAT;
//!              (on clients, scopes and endpoints; the nearest one wins)
//!              send policy;  (client only: a policy value or a stateless policy type)
//!              operations Name;  (client only: the enum `Name` of its endpoints; with `send`,
//!                                 each `send()` passes its variant to `SendOperation<T, Name>`)
//!
//! query / headers blocks (a header key is a literal or a native constant):
//!     KEY = v;                        set a key no enclosing owner sets
//!     append KEY = v;                 add occurrences after the inherited ones
//!     override KEY = v;               replace the inherited occurrences (v always present)
//!     override KEY = v else inherit;  v can be absent: replace when present, keep when absent
//!     remove KEY;                     drop the inherited occurrences (acts first in its owner)
//! ```
//!
//! `?` is Concord's presence marker; no meaning is inferred from type names. Every body and typed
//! output has a format: the nearest `request`/`response` (or `format`) statement of the endpoint,
//! an enclosing scope or the client. Path segments, subdomain labels, query values and template
//! holes are written with `Display`; a value placed directly as a header goes through
//! [`ToHeaderValue`], so a native `HeaderValue` is sent as is. Header names and values are checked
//! by `http` itself. A bare name refers to the current owner; `client.x` or `scope.x` reaches an
//! enclosing owner. Composition is decided at compile time, except which header a native
//! `HeaderName` constant names, compared by name when the request is built.
//!
//! # Native requests
//!
//! ```no_run
//! # concord::api! { pub client Api { base "http://x.test"; GET item(id: u64) { path "items/{id}"; } } }
//! # async fn run(api: Api) -> Result<(), Box<dyn std::error::Error>> {
//! let prepared = api.item(7).prepare()?; // no I/O
//! println!("{prepared:?}"); // redacted rendering
//! let (client, mut request, decoder) = prepared.into_parts();
//! *request.timeout_mut() = Some(std::time::Duration::from_secs(2));
//! decoder.finish(client.execute(request).await).await?; // handled as declared (here: any 2xx)
//!
//! let raw: concord::reqwest::Response = api.item(8).prepare()?.send_raw().await?;
//! # let _ = raw; Ok(()) }
//! ```
//!
//! [`Prepared::send`] and generated `send()` without a client send policy each perform one
//! `reqwest::Client::execute`, with no Concord retry or extra request. A client `send policy;`
//! instead calls [`SendWith::send`]; the policy may sign, make sub-requests or repeat requests.
//! A request's `Clone` says nothing about whether sending it again is safe.
//!
//! # Sensitive values
//!
//! `#[sensitive]` marks a value. It is sent as is, but Concord withholds it — and every key, path
//! segment or host label it feeds — from the output **Concord itself produces**: the generated
//! `Debug` of clients, config structs, scopes and requests, [`Prepared`]'s `Debug`, and
//! [`Error`]'s `Display` and `Debug`. Keys named `authorization`, `cookie`, `x-api-key`, `token`,
//! `password` and a few more are treated as sensitive without the marker. A native `HeaderValue`
//! whose own sensitivity flag is set stays flagged and is withheld the same way, marker or not.
//! `Error` has no `source()` chain: the underlying reqwest or format error is reachable only through
//! [`Error::transport`] and [`CodecError::source_unredacted`] (from [`Error::codec_error`]), and the
//! unread response of a status or metadata error through [`Error::response`]; none of them is
//! redacted. A format's [`CodecError`] label is static text by construction, not a secrecy proof.
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
//! - `docs/CUSTOM_FORMATS.md` — writing a format: `Encode`, `Decode`, response head checks,
//!   `CodecError`, the `Accept` suggestion;
//! - `docs/RECIPES.md` — error documents, streaming, custom formats, retries, pagination;
//! - `docs/DIAGNOSTICS.md` — the compile errors and how to fix them;
//! - `docs/MIGRATION.md` — from `via` to formats, from the V4 alpha and from V3;
//! - `v4/SEMANTICS.md` — the precise rules.
//!
//! # Features
//!
//! - `json` (default): the `Json` format, via serde and serde_json. Implies `extras`.
//! - `extras`: the standard formats without serializer dependencies, `Text` and `Binary`, the
//!   `IgnoreContentType` adapter and the `Encoded` format of pre-encoded bodies.
//!   Without `extras` (`default-features = false`), the facade has no standard format: formats
//!   of your own work the same way.
//! - `form`: the `Form` format, `application/x-www-form-urlencoded` via serde and
//!   serde_urlencoded. Off by default, like reqwest's own `form` feature. Implies `extras`.
//! - `stream`, `csv`, `jsonl`, `sse`: incremental bodies (`concord::stream`), `concord::csv`,
//!   `concord::jsonl` and `concord::sse` (server-sent events); each implies `extras`, **not**
//!   `json` — with `default-features = false`, add `json` too if the same client also decodes
//!   JSON documents.
//! - `multipart`: `concord::multipart`, `multipart/form-data` and `multipart/related` request
//!   bodies (`request Multipart;` with a `MultipartBody`) whose boundary is chosen per body and
//!   sent as its runtime media type. Implies `extras`; no dependency (reqwest's own `multipart`
//!   feature stays off).
//! - `reqwest-defaults` (default): reqwest's default features (TLS, HTTP/2, system proxy).
//!   Disable it to choose reqwest's features in your own `Cargo.toml`.

#[cfg(feature = "form")]
pub use concord_codecs::Form;
#[cfg(feature = "json")]
pub use concord_codecs::Json;
#[cfg(feature = "csv")]
pub use concord_codecs::csv;
#[cfg(feature = "jsonl")]
pub use concord_codecs::jsonl;
#[cfg(feature = "multipart")]
pub use concord_codecs::multipart;
#[cfg(feature = "sse")]
pub use concord_codecs::sse;
#[cfg(feature = "stream")]
pub use concord_codecs::stream;
#[cfg(feature = "extras")]
pub use concord_codecs::{Binary, Encoded, IgnoreContentType, Text};
pub use concord_core::field;
pub use concord_core::{
    BaseUrl, CODEC_ERROR_CONTEXT, CodecError, ContentTypeField, Decode, Decoder, Delivery, Encode,
    EncodedBody, Endpoint, Error, ErrorKind, FieldError, HeaderNameError, HeaderNameErrors,
    Incoming, Incremental, IntoValue, Operation, Prepared, QueryPairs, QueryPairsWriter, Received,
    Records, ResponseHead, ResponseMeta, SendOperation, SendWith, Sends, ToHeaderValue, bytes,
    reqwest,
};

/// Declares an HTTP API as a client → scopes → endpoints tree and generates its typed client.
///
/// See the [crate documentation](crate) for the language.
///
/// The expansion refers to `concord_core` through this crate's own path (`$crate`), so the macro
/// works under a renamed dependency and needs no other import. Format types are not part of the
/// bridge: a statement's format is the path you write, resolved where you write it.
#[macro_export]
macro_rules! api {
    ($($tokens:tt)*) => {
        $crate::__macro::api! { $crate::__macro::concord_core; $($tokens)* }
    };
}

/// The bridge used by [`api!`]: the procedural macro and the runtime crate its expansion names.
/// Not a public API.
#[doc(hidden)]
pub mod __macro {
    pub use concord_core;
    pub use concord_macros::api;
}
