# Repository change map for v2.1

This map is based on the supplied v1 repository. Paths may be split/renamed during implementation, but responsibilities must remain clear.

## `crates/concord-macros`

### Syntax

| Current path | v2.1 work |
|---|---|
| `syntax/ast.rs` | new client-contained tree, value axes, representation defaults/fences, status/effect, word patches, `via`, optional body |
| `syntax/declaration.rs` | child containment/order, methods, endpoint result/body grammar |
| `syntax/values.rs` | postfix `?`, defaults, sensitivity; delete type-name semantics |
| `syntax/request.rs` | atom/adapter/concat, subdomain/path/query/header patch grammar |
| `syntax/credentials.rs` | optional secret, grouped auth, endpoint-source path |
| `syntax/pagination.rs` | `paginate via`, bindings and initializer syntax |
| `syntax/rate_limits.rs` | policy/id/key/cost/allow/bind syntax |
| `syntax/keyword.rs` | remove old vocabulary, add v2.1 words |
| `syntax/diagnostics.rs` | migration and semantic diagnostics |

### Semantic

| Current path | v2.1 work |
|---|---|
| `semantic/model.rs` | canonical IR: owner IDs, exact types, presence/init/sensitivity, representation/status/effect/capabilities/provenance |
| `semantic/symbols.rs` | explicit owner references, endpoint source paths, generated collision namespace |
| `semantic/inheritance.rs` | family-specific effective contracts and representation fences |
| `semantic/surface.rs` | one direct/unbound/control/terminal surface model |
| `semantic/auth.rs` | source graph, one challenge, private target ownership |
| `semantic/pagination.rs` | safe effect, complete page, live destinations/header interests |
| `semantic/rate_limits.rs` | stable IDs, canonical dimensions, additive policy validation |
| `semantic/head_limits.rs` | adjust static/dynamic head bounds and atom metadata |

### Emit

| Current path | v2.1 work |
|---|---|
| `emit/assertions.rs` | focused representation/atom/sequence/page/material/rate obligations |
| `emit/bridge.rs` | static effective metadata and one core bridge |
| `emit/client.rs`, `scopes.rs`, `endpoints.rs`, `requests.rs` | v2.1 generated tree, controls, capabilities, unbound parity |
| `emit/shaping.rs` | atom/adapter/word patch program, real+sanitized one-pass output |
| `emit/credentials.rs` | slot/source/store/facade changes |
| `emit/descriptions.rs`, `docs.rs` | effective representation/status/effect/provenance and task-first Rustdoc |

## `crates/concord-core`

### Contracts and construction

- `contracts.rs`, `generated_contract.rs`, `endpoint.rs`, `construction.rs`: new static/effective descriptors, status/effect, representation metadata, build errors.
- `client.rs`: explicit limiter selection, security-context identity, clone semantics.

### Request/head/atoms

- `request/head.rs`, `request/prepare.rs`, `request/prepared.rs`: atom writer/adapters, patch program, real/sanitized views, logical URL, reserved ownership.
- add/rename modules for `RequestAtom`, `AtomEncoder`, `OrderedValues`, `RequestPlan` as appropriate.

### Body and response

- `body/spec.rs`, `body/prepared.rs`, `body/stream.rs`, `body/multipart.rs`: generic request representation, per-page prepare, replay/page capabilities.
- `response/spec.rs`, `response/handler.rs`, `response/stream.rs`, `records/*`: generic response representation, sealed drivers, exact streams, endpoint status externalization.

### Execution

- `execute/mod.rs`: one normative v2.1 state machine.
- `operation/*`: IDs/deadline/dispatch knowledge and page/attempt topology.
- `error/*`: stable categories/stages/context and safe structured diagnostics.

### Auth/retry/rate/pagination

- `auth/*`: preserve versioned slots/systematic model; update source availability/challenge/store/facade.
- `retry/*`: effect-aware gates, jitter/backoff, shared load budget timing, central server delay.
- `rate_limit/*`: whole-plan admission, canonical key, non-fallible current-result feedback.
- `pagination/*`: controller page response/step/header interests/safe effect/aggregate limits/per-page prepare.

### Hooks/observation

- replace `hooks/*` broad veto model with minimal read-only operation and stream events;
- migrate debug policy to bounded diagnostics/observer configuration.

### HTTP/runtime

- `http/*`: fixed origin, proxy separation, transport-private recovery, content decoding and phase limits;
- `runtime/*`: three configuration owners, finite defaults, runtime endpoint limits;
- remove automatic redirect/native escape behavior if any.

## `crates/concord-extras`

| Current path | v2.1 work |
|---|---|
| `codec/json.rs` | zero-sized generic `Json`, safe path diagnostics |
| `records/ndjson.rs`, `records/csv.rs` | collected/lazy representation pairs without public driver wrappers |
| `pagination.rs` | controller-owned page interpretation, header/link variants |
| `auth/oauth2_client.rs` | injected time, isolated provider HTTP, slot recovery contract |
| `rate_limit/local.rs` | whole-plan admission, bounded state/fairness/clock |
| `rate_limit/observer.rs` | remove/replace with non-result-determining feedback support |
| new atom module | `DisplayAtom`, common canonical adapters |
| new tracing module | `OperationObserver` adapter, no exporter/runtime queue in core |

## `crates/concord`

- re-export v2.1 macro, semantic types, representations, streams, errors, plans, policies;
- remove old wrapper/trait/hook/response-bytes/safe-plan names;
- keep normal generated use inherent without a mandatory prelude;
- maintain additive feature semantics and no-default builds.

## Tests and examples

- migrate all `crates/concord/tests/generated/*` surface/runtime tests;
- rewrite UI fixtures for new grammar/diagnostics and removed language;
- extend `runtime/deterministic.rs`, core ordering/retry/status/stream tests;
- preserve/extend auth systematic tests;
- rewrite pagination/rate/record tests to new contracts;
- rewrite all examples and docs anchors;
- migrate Riot to inherited JSON and local representation exceptions;
- add 500/1,000-endpoint fixtures and stream/driver benchmarks.

## Tools and CI

- `xtask`: semantic descriptor diff, ambiguous migration report, expanded fixture generation, stale-vocabulary policy checks;
- `tools/perf`: representation driver, large expansion, shaping, retry/rate/pagination benchmarks;
- `scripts/check_docs_examples.ps1`: new anchors/vocabulary;
- CI/just: retain full gates, update package names/features/version only as required.

