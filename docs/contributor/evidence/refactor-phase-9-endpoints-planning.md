# Phase 9 endpoint descriptions and planning evidence

Phase 9 was implemented in the worktree based on commit `48908cc`.
Validation used Rust/Cargo 1.97.1 on Windows, with the declared Rust 1.85
minimum checked separately. The observed host was
`x86_64-pc-windows-msvc`, Windows 10.0.26200.0, on an Intel64 Family 6 Model
183 processor. The installed editor engine reports
`rust-analyzer 1.97.1 (8bab26f4 2026-07-14)`.

## Resulting endpoint contract

Core now owns `EndpointId`, `EndpointDescription`, `SafeRequestPlan`, and the
endpoint timeout, request-body, response-acquisition, and capability
vocabularies. Generated and runtime-defined endpoints expose the same safe
description shape.

`EndpointId::from_static` keeps valid generated IDs allocation-free.
`EndpointId::new` validates and owns runtime IDs behind a cloneable bounded
allocation. IDs contain at most 128 ASCII bytes, use dot-separated segments
of at most 63 bytes, and allow lowercase letters, digits, `_`, and `-` with an
alphanumeric first and last byte. Static and owned forms have the same
equality, ordering, hashing, display, and debug semantics. Invalid runtime
input is reduced to a typed configuration error without echoing the value.

Every generated endpoint owns one lazily initialized static description from
the normalized endpoint model. It contains only:

- the public stable contract ID;
- HTTP method and source topology;
- a route template with dynamic values replaced by structural placeholders;
- static request-body category and specification identity;
- sealed core response-acquisition category;
- exact generated capability flags;
- declared authentication/rate-limit presence without materials or keys;
- endpoint timeout selection.

Runtime descriptions use the same type while owning only bounded/static
shape data. They deliberately describe each dynamic path value as a runtime
segment and contain no path, query, header, body, credential, or rate-key
value.

The façade exposes these types through `concord::endpoint` and curated
advanced/prelude paths. The generated façade bridge contract is version 22.
Native Reqwest requests/responses remain private.

## Sanitized no-effect planning

Each generated client now has the predictable borrowing API:

```rust
pub fn plan<E>(
    &self,
    endpoint: &E,
) -> Result<concord::endpoint::SafeRequestPlan, concord::ApiError>
where
    E: generated_scope::Endpoint;
```

Planning calls the same endpoint-specific public-head function used by
execution. Core supplies the same `HeadBuilder`, base URL, immutable client
snapshot, URL/path/query/header validation, safe URL rendering, typed auth
plan validation, and rate-key bounds. The execution path delegates to that
same generated head function, so URL shaping has one semantic owner.

Planning intentionally stops before operation-ID allocation, request-body
metadata and preparation, factory creation, stream polling, hooks,
credential-provider work, limiter acquisition/observation, native request
construction, transport, response handling, or completion. It returns the
safe logical URL, sanitized public headers, endpoint and method, selected and
resolved timeout, zero-based initial page index, lexical rate-rule count,
sealed response acquisition, and static body/capability facts. Dynamic body
metadata is explicitly unavailable rather than obtained through an
extension callback.

A seeded effect-counter test proves all ten callback/transport categories
remain at zero before and after planning. Separate execution of the same
endpoint proves its hook-visible safe URL and sanitized headers exactly match
the plan, while body, limiter, hooks, transport, and completion then run once
through the managed lifecycle. Protected query/header, provider, and body
sentinels are absent from plan/description `Debug`, safe URL, and header
output.

## Runtime endpoint and timeout semantics

`RuntimeEndpoint` and `RuntimeEndpointBuilder` replace the ambiguous prepared
endpoint terminology. Their canonical constructor is:

```rust
RuntimeEndpoint::builder(endpoint_id: EndpointId, method: Method)
```

The default timeout state is `InheritClient`. `.timeout(duration)`,
`.disable_timeout()`, and `.inherit_timeout()` select the three states with
last-call-wins composition. The immutable core runtime snapshot now contains
a real optional client operation timeout. Plans expose both the endpoint
selection and the value resolved against that snapshot. Tests observe
inheritance of 23 seconds, an explicit 7-second override, and disabled
resolution separately.

Runtime build validation freezes:

- at most 256 combined path, query, header, auth, and rate-rule components;
- at most 64 KiB of public path/query/header text;
- at most 64 authentication placements;
- validated public headers, authentication structure, URL/query structure,
  response acquisition, body specification identity, and timeout selection.

Boundary and first-excess tests own these limits. Runtime construction invokes
no executor effects and keeps body specification callbacks lazy. Runtime
pagination remains unsupported, and runtime endpoints do not receive
generated pagination, credential-storage, or dangerous raw terminals.

Runtime endpoints implement the same sealed generated endpoint contract and
bind into the same core `Request` and executor as generated endpoint values.
A deterministic loopback test proves equivalent generated and runtime
operations produce the same `before_send → response_head → completion`
ordering and the same method/target. No alternate executor was introduced.

Deprecated `PreparedEndpoint` and `PreparedEndpointBuilder` aliases delegate
to the new types only to keep intermediate phases buildable. A
`#![deny(deprecated)]` compile-fail case proves the old name is migration-only;
Phase 14 owns its deletion. Historical fixture filenames are likewise
scheduled for the Phase 12/14 cleanup.

## Explicit endpoint reuse and compiler surface

Generated endpoint values now implement `Clone` unconditionally when all
stored fields always permit it and conditionally on body input when required.
Surface tests store bodyless endpoint values in structs and collections,
clone/rebind them, and retain the focused non-Clone diagnostic for a
non-cloneable body.

The documented sealed generated `Endpoint` trait exposes the associated named
request type, response specification, and safe description. Generic
composition can therefore bind an endpoint through `E::Request` without a
prelude or extension-trait import. Direct client calls and explicit binding
continue to construct the same endpoint model.

Trybuild coverage proves:

- `RuntimeEndpoint` is available without deprecated names;
- the prepared aliases are deprecated;
- runtime fields and unsupported builder/capability methods remain absent;
- runtime pagination and generated dangerous streaming/raw misuse remain
  absent;
- forged and cross-client endpoints cannot implement/use the sealed
  contract;
- diagnostics remain in public response/body/capability vocabulary.

The multipart misuse snapshot gained one additional
`ResponseBodySpec`/`DecodeBody` diagnostic because static response acquisition
is now type-checked for the public description. That output was inspected
individually. No Trybuild output was blanket-accepted.

## Lifecycle, security, and dependency proofs

The complete 329-test core library suite passes with the new owned/borrowed
endpoint identity propagated through request metadata, hooks, errors,
pagination, response handling, and completion. Existing lifecycle proofs
continue to cover inert construction, immutable snapshots, auth/limiter
ordering, bounded challenge recovery, status-before-decode, bounded bodies
and streams, sequential all-or-error pagination, result fixation,
exactly-once completion, cancellation, and native transport privacy.

The 17-test seeded redaction/status-body matrix passes. New Phase 9 tests add
protected plan/description values and verify endpoint-ID validation errors do
not echo rejected input. Public Rustdoc inspection passes and finds no native
request/response, authentication material, limiter-key, or private executor
surface.

Dependency and expansion inspection proves:

- `concord-macros` has no normal Concord runtime dependency;
- `concord-core` has no edge to macros, extras, or façade;
- `concord-extras` depends on core but not macros or façade;
- the caller-renamed façade expansion contains 30
  `::runtime::__private` paths, two façade bridge aliases, zero direct
  `::concord_core` paths, and zero extras implementation names;
- production macro source contains no JSON, CSV, NDJSON, OAuth2, governor, or
  built-in pagination-controller name branch.

The one direct `::concord_core` occurrence in `codegen.rs` is a macro unit-test
input for the recorded direct-core development fallback, not emitted
one-dependency user code. Phase 14 owns removal of that fallback.

## Expansion measurement

The before and after expansion uses the same multi-API
`concord-macros/tests/generated_surface.rs` target and the Phase 8 PowerShell
counters. Fragments are whitespace-delimited; declaration-shaped lines use
the same regex for structs, enums, traits, impls, functions, types, constants,
and modules.

| Metric | Phase 8 | Phase 9 | Change |
|---|---:|---:|---:|
| Expanded lines | 9,901 | 12,566 | +26.92% |
| Characters | 538,466 | 665,616 | +23.61% |
| Characters without newlines | 518,664 | 640,484 | +23.49% |
| Whitespace-delimited fragments | 34,742 | 39,294 | +13.10% |
| Declaration-shaped lines | 1,366 | 1,536 | +12.45% |

The growth is the generated description static, shared public-head function,
sealed planning method, and explicit description/plan bridge for every
endpoint. It is recorded rather than treated as free: Phase 10 owns emission
consolidation after the public contract is stable. Peak-memory,
clean/incremental, rustdoc-timing, and the complete 1/10/100/1,000-endpoint
protocol remain the Phase 14 release gate.

## Performance smoke

`cargo bench -p concord-perf --bench concord_overhead --no-run` passed in
8.68 seconds in the observed incremental release build. The one-iteration
smoke completed and audited all 12 named categories:

| Category | Observed one-iteration duration |
|---|---:|
| Core client construction | 324.3 µs |
| Generated client/facade/request construction | 6.1 µs |
| Safe public planning | 37.0 µs |
| Public-head preparation | 26.8 µs |
| Codec encode/decode | 0.5 µs |
| Body-limit polling | 2.7746 ms |
| Ready credential preparation/finalization | 45.8 µs |
| Ready limiter acquisition | 10.9607 ms |
| Pagination loop | 3.9065 ms |
| Stream wrapping | 36.0 µs |
| Generic specification selection | 18.7 µs |
| Record framing | 0.5 µs |

These are observational Windows loopback measurements, not product
thresholds. The safe-planning benchmark invokes the public
`client.plan(&endpoint)` contract; its lower-level comparison invokes the
same production public-head preparation and validation path.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS after `cargo fmt --all` corrected one import-order difference found by an earlier check |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo test -p concord-macros --quiet` | PASS; 39 compiler/model tests, 17 compiler/graph/Rustdoc fixtures, generated suites, 27 UI groups, and doctests |
| `cargo test -p concord-macros --test ui -- --nocapture` | PASS; all 27 UI groups |
| `cargo test -p concord --all-features --quiet` | PASS; all façade runtime and compiler tests |
| `cargo test -p concord --no-default-features --quiet` | PASS |
| `cargo test -p concord --test ui --all-features -- --nocapture` | PASS; four compile-pass cases and all focused compile-fail cases |
| `cargo test -p concord --test generated_surface --all-features` | PASS |
| `cargo test -p concord --test endpoint_planning --all-features` | PASS; five description/planning/runtime/lifecycle tests |
| `cargo test -p concord --test deterministic_generated_runtime --all-features --quiet` | PASS |
| `cargo test -p concord --test type_neutrality --all-features --quiet` | PASS; two tests |
| `cargo test -p concord --test public_namespaces --all-features --quiet` | PASS; eight tests |
| `cargo test -p concord-examples --all-features --quiet` | PASS |
| `cargo test -p concord-extras --all-features --quiet` | PASS; 37 implementation tests plus integration/doctest groups |
| `cargo test -p concord-core --lib --all-features --quiet` | PASS; 329 lifecycle/security/boundedness tests |
| `cargo test -p concord-core --lib --no-default-features --quiet` | PASS; 254 tests |
| `cargo test -p concord-core --all-features --quiet` | FAIL only in the recorded eight Rust 1.97.1 core Trybuild rendering differences after all library and non-UI integration tests pass; no snapshot was overwritten |
| `cargo test -p concord-core --all-features --test deterministic_redaction --test error_redaction --test hook_redaction --test proxy --test sanitized_metadata --test status_body_contract --quiet` | PASS; 17 seeded security/status-body tests |
| `cargo test -p concord-core --test public_rustdoc --all-features --quiet` | PASS; two public-surface tests |
| `cargo test --workspace --doc --quiet` | PASS; all workspace doctests |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord-core --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord-extras --all-features --no-deps` | PASS |
| `cargo +1.85.0 check --workspace --all-targets --all-features` | PASS |
| `cargo +1.85.0 test -p concord-core --lib --no-default-features --quiet` | PASS; 254 tests |
| `cargo +1.85.0 test -p concord-extras --no-default-features --quiet` | PASS |
| `cargo +1.85.0 test -p concord-core --no-default-features --quiet` | FAIL only in the known `response_mode_is_sealed` candidate-order rendering after all non-UI tests pass |
| `cargo +1.85.0 test -p concord --no-default-features --quiet` | FAIL only in the four recorded compiler-version renderings after every runtime and compile-pass case, including `RuntimeEndpoint`, passes |
| `cargo metadata --format-version 1 --all-features --no-deps` | PASS |
| `cargo tree -p concord-macros -e normal`; all-feature core/extras/façade tree audits | PASS; forbidden Concord edges absent |
| `cargo deny check` | PASS with the two recorded duplicate-version warnings |
| `cargo expand -p concord-macros --test generated_surface` plus the recorded PowerShell counters | PASS; Phase 9 metrics shown above |
| renamed-façade expansion path counters | PASS; 30 façade-private paths, two bridge aliases, no direct core path, no extras implementation name |
| `cargo bench -p concord-perf --bench concord_overhead --no-run` | PASS |
| `cargo bench -p concord-perf --bench concord_overhead -- --smoke` | PASS; 12/12 categories executed |
| `rust-analyzer --version` | PASS; executable version recorded above |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to the installed 1.97.1 toolchain |
| `git diff --check` | PASS |

The first aggregate macro test attempt exceeded its command timeout. The next
run exposed one genuine stale contract-version fixture, which was updated
from 21 to 22; the complete aggregate then passed. This was not reported as a
baseline or infrastructure success until the passing rerun.

The exact full-core commands retain compiler-version diagnostic rendering
drift only. Each invalid program still fails for its intended privacy or
capability reason. The core response-mode fixture was updated to include the
new sealed acquisition constant so its failure remains focused on sealing;
the remaining Rust 1.85 difference is candidate ordering. No snapshot was
changed to conceal that difference.

The manual rust-analyzer completion, hover, suggestion ranking, and navigation
protocol cannot be performed from this shell-only session. The executable
version is recorded; manual interaction remains an explicit release gate.

## Deferred ordered work and deviations

Phase 10 owns syntax/semantic/emission macro reorganization and consolidation
of the measured per-endpoint expansion increase. Phase 11 owns the runtime
endpoint example filename/narrative conversion. Phase 12 owns historical test
fixture naming. Phase 14 removes the deprecated prepared aliases, direct-core
macro fallback, and performs the complete scale/performance/editor release
matrix.

Runtime pagination remains the selected bounded deferral: no partial dynamic
pagination API or second executor was introduced.

No architectural deviation was required.
