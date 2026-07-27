# Changelog

## [Unreleased]

### Added

- One complete mdBook documentation site for new users, feature users,
  extension authors, and maintainers. It includes a request-executing Start
  path, the Core Guide, focused feature guides, ten compiled-example recipes,
  public extension paths, exact reference navigation, architecture/compiler
  internals, runtime contracts, and maintenance procedures. The root
  `book.toml`, `just site`, `just site-serve`, strict xtask inventory/link/include
  checks, and the dedicated Documentation CI job use mdBook 0.5.4 and render
  under `target/concord-docs/`.
- A single `concord` application façade with generated clients, explicit
  endpoint values, runtime endpoints, and one managed executor.
- Named generated client builders, inherent request terminals, safe endpoint
  descriptions, and sanitized request planning.
- Finite public limits for endpoint identities, authentication placement and
  material, encoded authentication output, multipart structure and metadata,
  runtime endpoint shape, rate-limit structures, provider traffic, response
  acquisition, and pagination.
- Public structural ceilings for final request heads, proxy URL text, custom
  root-certificate count and bytes, and application identity PEM bytes.
- First-party JSON, record codecs, pagination policies, rate limiting, and
  OAuth2 client credentials as downstream implementations of public semantic
  contracts.
- Explicit ordered rate-limit bucket dimensions through `by [...]`, supporting
  final `host`, stable `endpoint`, and required/defaulted scalar `vars.*`,
  visible `scope.*`, and `ep.*` values.
- Recoverable `RateLimitErrorKind::StateCapacity` reporting when bounded local
  quota or cooldown state cannot admit a new key without discarding active
  history.
- Explicit Concord-managed `RetryPolicy` with a required safe classifier,
  hard per-operation maximum, body-replay gating, and a bounded
  logical-origin rolling load budget. The runtime default remains no managed
  retry policy.
- Generated endpoint Rustdoc backed by one semantic documentation model, with
  named value inputs, flattened scope inputs, safe query/header names, route
  shape, human-readable deadlines, and only the terminals each endpoint
  actually supports.
- Type-configurable CSV decoding while preserving `Csv<T>` as header-bearing,
  comma-delimited CSV. `HasHeader` and `NoHeader` select header presence;
  `CommaDelimiter`, `SemicolonDelimiter`, and `TabDelimiter` select parsing
  delimiters. The `CsvHeaderMode` and `CsvDelimiter` policy traits are sealed.
  Configured aliases are supported:

  ```rust
  type ConfCsv<T> =
      Csv<T, HasHeader, SemicolonDelimiter>;
  ```

  `TabDelimiter` retains the `text/csv` media contract.

### Changed

- Built-in credential constructors are fallible and bounded before storage;
  generated static construction and replacement validate before slot mutation,
  and protected-string clones share immutable storage.
- Custom `CredentialMaterial` no longer requires `Clone`; Concord shares ready
  material through private slot storage.
- `concord::api!` is the sole application declaration entry, including for a
  renamed façade dependency.
- Generated declarations, runtime endpoints, and core authentication plans
  enforce one shared 64-placement hard limit.
- `EndpointId` validates borrowed input before allocation.
- Bearer, Basic, Base64, and authentication-query construction use checked
  arithmetic and reject excess output before native request mutation.
- Generic authentication headers reject every transport- or request-owned
  name consistently in static declarations, `Auth::header`, and defensive
  authentication-plan validation.
- JSON request serialization writes directly through the active body cap, so
  it stops before retaining the first excess byte. Codec failures expose only
  finite encode, decode, and limit categories without native or
  application-authored error text.
- Generated and runtime-defined requests share final core-owned component,
  logical and credential-sanitized URL, header-count, and aggregate-header
  byte ceilings.
- Empty transport response chunks are suppressed across buffered success,
  streaming success, terminal status capture, and provider HTTP before
  application code can observe them. One response accepts at most 1,024 empty
  chunks, and bounded per-poll or per-turn work yields cooperatively before
  continuing. Nonempty response-byte limits are unchanged.
- Empty request chunks are suppressed before native transport across direct,
  factory-produced, and multipart-backed managed streams. One request body
  accepts at most 1,024 empty chunks; bounded per-poll work preserves
  fairness, while nonempty byte and exact-length accounting is unchanged.
- Host-prefix shaping is rejected at compile time when an endpoint's final
  normalized shape uses an IPv4 or IPv6 fixed base. IP-literal bases remain
  valid without shaping, including when `host off` clears an inherited
  prefix. DNS-domain shaping is unchanged.
- `RecordDecoder` now documents the enforceable net staging-buffer contract:
  the resulting length cannot increase, `Some(record)` requires a strict net
  decrease, and `None` may consume bytes.
- The IP host-shaping rule changes source-language acceptance without changing
  the generated bridge.
- Ordered rate-limit key generation now formats referenced dimensions through
  the core-owned bounded bridge, raising the generated-contract version to 27.
- Normal `.response_bytes()` acquisition now preserves declared status
  acceptance and `Accept`, skips validation and decoding, returns
  `ApiResponse<Bytes>`, and works for generated, explicit, and runtime
  endpoints. Removing the old raw-terminal bridge raises the generated-contract
  version to 28.
- Lifecycle hooks, limiter callbacks, and built-in diagnostics now use
  `AttemptInfo` to distinguish visible-attempt order, policy-retry order, and
  authentication recovery. Successful metadata and request/page completion
  report started-attempt and policy-retry totals; pagination completion reports
  checked aggregate totals.
- Managed policy retries now re-enter the ordinary hook, body reconstruction,
  current-authentication preflight, limiter, private materialization,
  response observation, and challenge/status lifecycle under one operation
  deadline and completion.
- Generated builders now install managed policy with `.retry(RetryPolicy)`.
  Application HTTP uses private known-safe protocol recovery by default and
  can disable it with `protocol_recovery(false)`. Removing the old native
  retry DSL/baseline raises the generated-contract version to 29.
- Every generated public value mutation now uses
  `value(value) -> set_value(value)`. Absence-capable values retain
  `clear_value()`; values with declaration defaults retain `reset_value()`.
  Client/scope/endpoint construction and navigation names,
  secret/credential/provider methods, timeout controls, and handwritten
  builders are unchanged. The unpublished bare mutation methods have no
  compatibility aliases.
- Absence and declaration defaults are now independent generated-control
  capabilities. A defaulted `Option<T>` or `Option<Vec<T>>` exposes
  `set_value(inner)`, `clear_value()`, and `reset_value()` on every surface
  that owns the value. Client-builder clear selects an explicit `None`, while
  reset discards the override; snapshot reset restores the retained evaluated
  default. Exact recognized `Option` syntax remains optional, while opaque
  aliases remain required opaque values.
- Effective inherited and local rate-limit partitions now reject direct
  `vars.*`, `scope.*`, or `ep.*` dimensions controlled by pagination. A
  `by [host]` dimension is also rejected when final host shaping depends
  recursively on a pagination-controlled value.
- Generated documentation now calls each source-derived limiter identity a
  deterministic declaration rule ID. It is deterministic within one generated
  client contract, not a globally unique namespace across unrelated `api!`
  expansions.
- The default façade feature set now installs the first-party
  `LocalRateLimiter`; `RetryAfterObserver` is its built-in final-429 observer.
  These replace the unpublished `rate-limit-governor`,
  `GovernorRateLimiter`, and `DefaultRateLimitObserver` names without aliases.
- Local quota state now prunes only fully replenished idle entries and returns
  recoverable `StateCapacity` pressure instead of evicting active quota or
  unexpired cooldown history.
- A sanitized response head reaches limiter observation before the user
  response hook. Observation uses the remaining operation deadline, and its
  failure prevents the hook and body acquisition.
- Multipart part and recipe construction is fallible. Factory output is
  revalidated at the private transport boundary.
- Proxy and TLS startup material is structurally validated before parsing or
  copying. Native URL and TLS parsers may remain stricter.
- Reader-backed request streams reuse one 8 KiB scratch buffer. Pagination
  identity encoding borrows normalized header names, enforces the remaining
  identity budget during encoding, and moves encoded storage into cycle
  detection.
- Tokio capabilities are selected per crate; multi-thread test runtimes no
  longer enlarge ordinary production dependency declarations.
- Generated direct methods, explicit endpoint values, runtime endpoints, and
  every pagination page use the same lifecycle and completion protocol.
- External dependencies use compatible semver requirements; Concord sibling
  packages retain exact version requirements.

### Removed

- Direct application paths through implementation crates, generated request
  extension traits, generated preludes, positional secret inputs, alternate
  executors, native Reqwest escapes, and runtime pagination.
- The façade `advanced` namespace, the core prelude, and the public benchmark
  bridge and partial-lifecycle conversions.
- Unbounded or partially managed response acquisition.
- The unpublished `rate-limit-governor`, `GovernorRateLimiter`, and
  `DefaultRateLimitObserver` public names.
- The `dangerous-raw-response` feature, `.raw_response()` terminal, raw response
  wrapper, and generated capability-token protocol. The normal
  `.response_bytes()` terminal replaces them without compatibility aliases.
- The `dangerous-custom-retry` feature, `concord::dangerous` namespace,
  Reqwest retry-builder reexport, native retry builder methods, and retry DSL.
  Managed `RetryPolicy` is the application retry surface.

### Safety guarantees

- Construction and planning remain inert.
- Authentication and native transport values remain private and redacted.
- Authentication cannot be placed in `Host`, framing, proxy authorization,
  cookie, `Accept`, `Content-Type`, `User-Agent`, or generic `Authorization`
  headers.
- The first JSON, request-head, or startup excess is rejected without
  retaining the rejected unit or exposing supplied content.
- Empty request chunks are never native transport items, and empty response
  chunks are never application items. Their total counts do not reset after
  nonempty data; the first excess produces one redacted terminal error, while
  bounded per-poll/per-turn work preserves executor fairness.
- Authentication recovery permits at most one additional attempt.
- Managed policy retry is bounded per operation and by default across each
  logical origin; capacity or replayability suppression preserves the current
  failure.
- Limiter response observation is core-first and deadline-governed.
- Pagination is sequential, finite, cycle-detecting, first-excess
  withholding, and all-or-error.
- Completion remains exactly once, including cancellation-sensitive paths.

All packages remain `publish = false`. Publication requires separate approval
of legal metadata, repository identity, package-name ownership, and the exact
publish operation.
