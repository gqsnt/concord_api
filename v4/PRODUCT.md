# Concord V4 — product charter

## What Concord is

A small, typed, declarative layer over `reqwest`. One `api!` declaration describes one
HTTP API as a tree — **client → nested scopes → endpoints** — and generates ordinary Rust
types whose methods build `reqwest::Request`s deterministically and decode typed responses.

**The fixed core** (owner, 2026-11-07): the DSL as the source of the usage — one declaration
whose client → scope → endpoint structure becomes the generated signatures — built on top of
`reqwest`. Everything else (the responsibilities below, the current scope, the shape of the
generated surface) can change when evidence shows a change makes Concord more useful at a
proportionate cost (`v4/desirability/amendments/A2-owner-charter.md`).

Concord owns exactly these responsibilities:

1. **Domain structure.** The declared tree is the generated navigation surface
   (`api.repo("rust-lang", "rust").issue(42)`).
2. **Values.** Required, optional and defaulted inputs on every owner, with precise
   meaning that never depends on Rust type names such as `Option<T>`.
3. **Request composition.** Base URL, path segments, subdomain labels, query parameters
   and headers contributed along the tree, composed in a documented, deterministic order,
   with explicit conflict rules decided at compile time.
4. **Representations.** Formats are ordinary implementations of the public traits: `Encode`
   for request bodies (buffered or streamed), `Decode` for finite responses and `Incremental`
   for streamed responses; the standard ones
   live in `concord-codecs` (JSON, text, bytes, form, CSV, JSON Lines, server-sent events,
   multipart). `Json` refuses a response without a JSON `Content-Type` unless an endpoint opts
   out explicitly. A typed error separates preparation, transport, status, metadata, decoding
   and limits.
5. **Sensitivity-aware diagnostics.** Values marked sensitive are sent, but are withheld
   from output that Concord itself produces (its `Debug`, error `Display`, redacted
   request rendering).
6. **Native interoperability.** The application supplies the `reqwest::Client`; every
   endpoint can produce a native `reqwest::Request` (and a `reqwest::RequestBuilder`), and
   can return the native `reqwest::Response`.

## Current scope (not a permanent boundary)

Core Concord does not implement these policies today. Each is a statement of current scope, and
may change through the desirability loop (`v4/desirability/`) when a measured, reviewed change
earns its place:

- **Authentication orchestration** — no credential providers, refresh, stores or challenge
  handling in core. A bearer token or API key is an ordinary (optionally sensitive) header or
  query value; `concord-auth` (pilot) offers sensitive credentials and a token cache for SDK
  code.
- **Rate limiting** — no policies, windows, budgets or admission in core; `concord-ratelimit` is
  a pilot crate.
- **Retries** — no retry engine or replay in core; `concord-retry` (pilot) owns bounded
  application attempts when an SDK opts in.
- **Pagination** — no cursor DSL or page collectors in core; `concord-pagination` provides
  bounded traversal for SDK code.
- **A managed executor** — no journal, deadlines-as-policy, observer framework, middleware stack
  or second transport configuration surface. Timeouts, redirects, proxies, TLS, default headers
  and connection pooling are configured on the `reqwest::Client` the application provides.
- **Contract export / inspection tooling.**

Concord offers neutral seams an SDK may use for these jobs: an SDK send policy (`SendWith`,
`Sends`), operation identity (`Operation`, `Endpoint`, inherent `operation()`), prepared requests
and unread responses (`Prepared`, `Received`), and the native request/response exits. Core
implements none of the policies above itself; SDKs and applications compose them in ordinary Rust,
optionally with the capability crates.

## Success criteria

- The minimal example is teachable in a few lines; required inputs are exactly the
  declared required values.
- A realistic nested API composes without repeated boilerplate and without runtime state
  beyond the selected values and the `reqwest::Client`.
- Two readers of a declaration predict the same request bytes (method, URL, ordered
  query, headers, body) for the same selected values.
- The generated surface is shorter and clearer than an equivalent hand-written reqwest
  wrapper, at a measured compile-time cost.
- Sensitivity guarantees are stated narrowly and honestly: they cover Concord-produced
  output only.
