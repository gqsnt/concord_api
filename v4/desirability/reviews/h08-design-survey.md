# H08 design survey — how SDK send policies classify operations (base `50c239a8`)

Read-only survey by a Claude subagent (Explore) for the coordinator, not a review and not a
decision. It read the SDK sources and the seven public I4 `e4/task.md` texts; nothing sealed.

## What core offers a policy to identify the operation
- `concord::Endpoint<T>` (`crates/concord-core/src/prepared.rs:187-197`): `prepare(self)` and
  `fn operation() -> Operation` (associated, not `const`); implemented by every generated type
  (`crates/concord-macros/src/emit.rs:1933-1940`); inherent `operation()` on every endpoint type.
- `concord::Operation` (`crates/concord-core/src/response.rs:34-124`): `Copy`; `name()`,
  `method()`, `is_endpoint()`; `PartialEq/Eq` on (is_endpoint, name, method); no `Hash`, no
  const equality, so it cannot be a `match` pattern — only `==` chains or name tables.
- `SendWith<T>` (`prepared.rs:230-245`) is keyed on the endpoint's output type; an impl cannot add
  bounds on the endpoint (E0276). The only per-endpoint check at SDK build: each generated
  `send()` requires `P: SendWith<Out>` (`emit.rs:1835-1851`).
- Each endpoint has its own SDK-local type, so SDK traits can be implemented for it; the macro
  generates no per-client operation enum, no const identity, no per-endpoint metadata statement.

## Per SDK (forgotten classification → stage)
| SDK | Mechanism (where) | Forgotten → |
|---|---|---|
| Strava | `Class::of`: `==` against three endpoint `operation()`s + `method()=="GET"` (`examples/strava-sdk/src/auth.rs:259-290`); error wording (`src/error.rs:115-118`) | silent default (e4: charged to read windows, no write scope, not uncertain) |
| Kraken | `match` on (HTTP method, built URL path) allow-list (`examples/kraken-spot-rest-sdk/src/policy.rs:59-88`); envelope via `SendWith<Envelope<T>>` | run-time refusal (`SigningInput`); a base URL with a path prefix refuses every call (`tests/first_use.rs:620-655`) |
| Sentry | `keys()` name table (`examples/sentry-sdk/src/policy.rs:86-107`), write vs read by method (`:110`), paged by table position (`:253`), `.pages()` per type (`src/pagination.rs:15-43`) | run-time refusal (not in table); silent default (paging flag); caller compile (`.pages()`) |
| GitHub (no policy) | listing id constants + `LISTINGS` table (`examples/github-sdk/src/paging.rs:70-122`), `*_pages` methods (`src/pages.rs:47-198`) | silent default (id); caller compile (method) |
| Kubernetes (no policy) | `ListOperation` trait per endpoint type (`examples/kubernetes-sdk/src/list.rs:51-117`) | caller compile (E0277) |
| Mastodon | uniform policy; per-endpoint wrappers (`examples/mastodon-sdk/src/lib.rs:155-237`) | caller compile (wrapper); never (alias) |
| Notion (no policy) | wrappers + optional `validate()` (`examples/notion-sdk/src/client.rs:90-243`) | caller compile (wrapper); never (forgotten `validate`) |
| Nager (control) | `impl SendWith<Vec<Holiday>> for KeepHead` (`examples/nager-holidays-sdk/src/lib.rs:59-71`) | SDK build (output type) |
| Cloudflare (control) | envelope via `SendWith<Single<R>>`/`SendWith<Paged<R>>` (`examples/cloudflare-zones-sdk/src/policy.rs:156-240`); read vs create by method (`:152-161`) | SDK build (output); silent default (method) — correct for this provider |

## Designs that make an omission a compile-time error
- **SDK-local with today's core:** an exhaustive SDK enum cannot be reached from `Operation`
  without a fallback; a per-endpoint trait fails only at the caller's compile; **carrying the
  class in the output type** (`-> Classed<C, T>`, policy implements only classified outputs) is
  the one route to an SDK build error — at the cost of class markers in public types
  (`Prepared<Classed<..>>`, `Endpoint<T>`), losing the `send P -> Result<Response<_>, Error>`
  template, and one marker per class combination.
- **Opt-in `api!` addition:** a generated per-client operation enum (`#[non_exhaustive]`, `#[cfg]`
  on variants, a const per endpoint) delivered to the policy (a sibling trait or an associated
  type), so a policy `match` without a wildcard fails with E0004 at SDK build; or a required class
  statement per endpoint with an expansion-time error. Cost: one exhaustive match (or one line per
  endpoint); none for SDKs that do not opt in. Both catch an omission only: a present but wrong
  class still compiles (Strava e4: copying `update`'s class misses the read-window exemption).
