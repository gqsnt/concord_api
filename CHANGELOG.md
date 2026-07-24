# Changelog

## Unreleased — `0.1.0` architecture reset

### Product contract

- Added the `concord` user façade as the only documented application
  dependency.
- Reframed Concord as a remote-contract compiler whose generated typed API,
  explicit endpoint values, and runtime endpoints share one managed HTTP
  lifecycle.
- Added predictable scope-oriented modules, direct endpoint methods, public
  endpoint values, named request wrappers, safe descriptions, and sanitized
  planning.
- Added named generated client builders; `Client::new()` now exists only for
  zero-input clients.

### Declaration compiler

- Added order-independent named clauses with duplicate-span diagnostics.
- Made endpoint and scope source identifiers `snake_case`, with deterministic
  generated PascalCase types.
- Made request-body roles explicit with `#[body]` and syntactic optionality
  explicit with `Option<T>`.
- Added optional stable contract IDs.
- Removed fixed-order compatibility, PascalCase source identifiers,
  name-inferred bodies, OAuth-specific sources, custom-retry/observer policy
  syntax, and macro knowledge of concrete extras.
- Removed the generated direct-core path fallback and bumped the private
  generated bridge contract to version 23.

### Runtime and security

- Kept endpoint construction inert and client clones backed by one immutable
  runtime snapshot.
- Added bounded owned runtime endpoint IDs and the inherit/explicit/disabled
  timeout tri-state.
- Runtime endpoint builders now account public shape incrementally, latch the
  first safe structural error, cap each Concord-owned formatted value, and
  stop repeated-query iterators at the withheld first excess.
- Runtime query-name arguments now use `AsRef<str>` so Concord can validate
  byte length before copying; ordinary `&str` and `String` calls are
  unchanged.
- Rate-limit IDs, key parts, keys, windows, and plans now enforce core-owned
  hard structural ceilings incrementally. `RateLimitKeyPart::new` is
  fallible, `RateLimitPlan::from_rules` is replaced by
  `RateLimitPlan::try_from_rules`, duplicate rule IDs are rejected, and
  limiter preparation limits above the hard key cap fail client construction.
- Generated/public-head dynamic host, path, query, header, and
  pagination-sensitive formatting now uses the same capped capture primitive
  as runtime endpoints; query shaping returns a safe configuration failure on
  excess output.
- OAuth2 client-credentials configuration now has public finite URL, client
  ID, client secret, scope, and Authorization-header ceilings. Its builder
  validates borrowed text before copying, exposes explicit owned-secret
  setters, checks percent/Base64/form lengths before allocation, and compares
  the form body with the active provider request limit.
- Preserved deterministic preparation, hook, limiter, authentication,
  challenge-recovery, status, acquisition, completion, and cancellation
  behavior.
- Preserved redaction, provider HTTP isolation, one-additional-attempt
  authentication recovery, finite bodies/streams/status capture/records,
  fail-closed generated rate plans, and sequential all-or-error pagination.
- Kept native requests, responses, multipart construction, and authentication
  material private to the runtime kernel.

### Crates and features

- Added `concord-extras` as a downstream implementation package depending only
  on public `concord-core` semantic contracts.
- Moved JSON, CSV, NDJSON, built-in pagination controllers, governor/default
  observer, and OAuth2 client credentials to extras.
- Kept primitive wire types, multipart, generic record/pagination execution,
  authentication/rate-limit contracts, transport, redaction, and the executor
  in core.
- Made core and extras defaults empty. The façade owns defaults and feature
  forwarding; ordinary defaults remain TLS, HTTP/2, and governor, with JSON
  opt-in.

### Compatibility removals

- Removed application-level two-crate setup and direct generated
  `concord-core` paths.
- Removed `PreparedEndpoint`, `PreparedEndpointBuilder`, and
  `DecodedResponse` aliases.
- Removed configured positional constructors, generated request extension
  traits/preludes, and separate generated façade/endpoint trees.
- See [MIGRATION.md](MIGRATION.md) for before/after forms.

### Examples, tests, documentation, and tooling

- Rebuilt examples as importable modules with thin binaries and deterministic
  local proofs; split the Riot narrative into domain, declaration, usage, and
  test layers.
- Reassigned proofs to core, façade, extras, and Cargo-graph fixture owners.
- Reorganized the workspace under `crates`, `fixtures`, and `tools`.
- Added a product-path manual, generated feature/limit checks, portable
  contributor commands, CI matrices, security gates, and observational
  performance tooling.

### Release status

- All packages remain `publish = false`. No license/repository metadata or
  package-name ownership was approved in the reviewed repository, so registry
  publication remains an explicit release blocker rather than an inferred
  success.
