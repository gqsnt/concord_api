# Concord V4 — product charter

## What Concord is

A small, typed, declarative layer over `reqwest`. One `api!` declaration describes one
HTTP API as a tree — **client → nested scopes → endpoints** — and generates ordinary Rust
types whose methods build `reqwest::Request`s deterministically and decode typed responses.

Concord owns exactly these responsibilities:

1. **Domain structure.** The declared tree is the generated navigation surface
   (`api.repo("rust-lang", "rust").issue(42)`).
2. **Values.** Required, optional and defaulted inputs on every owner, with precise
   meaning that never depends on Rust type names such as `Option<T>`.
3. **Request composition.** Base URL, path segments, subdomain labels, query parameters
   and headers contributed along the tree, composed in a documented, deterministic order,
   with explicit conflict rules decided at compile time.
4. **Minimal representations.** JSON / text / bytes request bodies; JSON / text / bytes /
   empty responses; a typed error that separates preparation, transport, status and
   decoding.
5. **Sensitivity-aware diagnostics.** Values marked sensitive are sent, but are withheld
   from output that Concord itself produces (its `Debug`, error `Display`, redacted
   request rendering).
6. **Native interoperability.** The application supplies the `reqwest::Client`; every
   endpoint can produce a native `reqwest::Request` (and a `reqwest::RequestBuilder`), and
   can return the native `reqwest::Response`.

## What Concord is not

These are non-responsibilities, not deferred features, and V4 contains no hooks designed
for them:

- **Authentication orchestration** — no credential providers, refresh, stores or
  challenge handling. A bearer token or API key is an ordinary (optionally sensitive)
  header or query value.
- **Rate limiting** — no policies, windows, budgets or admission.
- **Retries** — no retry engine, replay, budgets or attempt journals.
- **Pagination** — no cursor DSL, traversal, or page collectors.
- **A managed executor** — no operation identity, journal, deadlines-as-policy, observer
  framework, middleware stack or second transport configuration surface. Timeouts,
  redirects, proxies, TLS, default headers and connection pooling are configured on the
  `reqwest::Client` the application provides.
- **Contract export / inspection tooling.**

Applications implement any of the above in ordinary Rust around Concord, typically with the
native request/response escape hatches.

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
