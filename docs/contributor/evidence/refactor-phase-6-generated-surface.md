# Phase 6 generated endpoint/request surface evidence

Phase 6 was implemented in the worktree based on commit `02644ef`. Validation
used Rust/Cargo 1.97.1 on Windows; the installed rust-analyzer reports
`rust-analyzer 1.97.1 (8bab26f4 2026-07-14)`.

## Resulting public contract

Every resolved endpoint now has one compiler-owned `EndpointSurface` containing
its final endpoint and request type names, module path, required arguments,
optional/default setters and helpers, legal terminals, endpoint-backed
credential storage terminals, pagination/raw capability, clone policy, and
documentation identity. Code emission consumes that surface rather than
discovering public names or capabilities independently.

For an endpoint such as `resources::ById`, generated users receive:

```rust
let direct: renamed_api::resources::ByIdRequest =
    client.resources("tenant".to_owned()).by_id("id".to_owned());

let endpoint =
    renamed_api::resources::ById::new("tenant".to_owned(), "id".to_owned());
let explicit: renamed_api::resources::ByIdRequest = client.request(endpoint);
```

The one scope-oriented module tree contains each scope facade, child module,
endpoint value, and named request wrapper. The former public `facades`,
`endpoints`, and per-client `prelude` trees are no longer emitted.

The client module owns a documented sealed `Endpoint` trait. Its `Request`
associated type enables generic binding while preserving compile-time client
ownership; its `Response` associated type supports public, sealed response-mode
capability checks without exposing generated specification aliases. Binding
remains hidden and is reachable through `Client::request`.

Named request wrappers:

- privately contain the existing core `Request<E>` execution carrier;
- implement safe, value-free `Debug`;
- delegate `IntoFuture` to the one core executor;
- expose timeout controls, debug selection, and `.response()` inherently;
- expose optional/default setters and clear/reset helpers inherently;
- expose `.paginate(...)` only for declared pagination;
- expose `.store_as_*()` only on the selected endpoint credential source;
- expose dangerous raw acquisition only with the feature enabled and a sealed
  buffered/discard response-mode proof;
- expose neither `Deref` nor a public conversion to the generic carrier.

`.response()` returns the universal core-owned `concord::ResponseRequest<E>`.
A second generated response-wrapper family would add names and expansion
without adding capability control, so the universal terminal is the smaller
ordinary surface selected by Phase 6.

Endpoint values are reusable snapshots. Bodyless endpoints derive `Clone`.
Body-bearing endpoint values use an explicit `BodyInput: Clone` implementation,
so cloneable buffered inputs work while one-shot `StreamBody` inputs remain
non-cloneable. Request wrappers themselves are not cloneable.

The generated bridge contract is version 19. The temporary direct-core bridge
used by macro-owned compatibility tests remains scheduled for Phase 14
removal; final caller-visible expansion continues to resolve renamed façade
dependencies and emit through `concord::__private`.

## Capability and lifecycle proofs

Façade-owned compile-pass/fail cases prove:

- named direct return types and predictable unified module paths;
- optional/default completion without a generated import;
- explicit generic endpoint binding;
- conditional endpoint cloning;
- absence of pagination and storage terminals on incompatible endpoints;
- cross-client endpoint rejection;
- private named-wrapper fields;
- dangerous feature absence and focused sealed-mode rejection.

The deterministic generated runtime test constructs equivalent direct and
explicit requests with identical timeout and endpoint-local state. Before
polling, transport, body encoding, before-send, response, and completion
counters are all zero. Both wrappers format as `SubmitRequest { .. }` without
the seeded scope, endpoint, or body sentinels. Once awaited, both requests
produce identical method, target, header, and body shaping through the same
managed lifecycle, with exactly one encode, before-send, response, and
completion event per operation.

The complete 324-test core library suite and the focused 17-test seeded
redaction/status-body matrix pass. They retain the established authentication
ordering, bounded acquisition, retry/recovery, pagination, cancellation,
completion, native privacy, and safe formatting proofs.

Generated rustdoc checks inspect the real renamed-façade output. Public pages
contain the named wrapper methods and associated types, contain no generated
prelude or split module trees, and contain none of the private endpoint,
response handler, native Reqwest, credential, limiter-key, or seeded sentinel
names in the reviewed matrix.

## Expansion and clean-build measurements

The before and after expansion is the same multi-API
`concord-macros/tests/generated_surface.rs` target. Characters include the
captured newline delimiters; the separate character count removes them.
Fragments are whitespace-delimited. The declaration-shaped count uses the
same Phase 6 regex for public/private structs, enums, traits, impls, functions,
types, constants, and modules.

| Metric | Before | After | Change |
|---|---:|---:|---:|
| Expanded lines | 7,419 | 10,105 | +36.20% |
| Characters | 421,898 | 540,178 | +28.04% |
| Characters without newlines | 414,478 | 519,968 | +25.45% |
| Whitespace-delimited fragments | 25,692 | 34,314 | +33.56% |
| Declaration-shaped lines | 846 | 1,366 | +61.47% |
| Fresh façade all-target/all-feature check | 13.727 s | 14.760 s | +7.53% |
| Fresh façade all-feature rustdoc | 14.165 s | 14.254 s | +0.63% |

The expansion increase is the expected cost of concrete named wrappers and
their explicit inherent methods. The one clean check sample increased by
7.53%, while the rustdoc sample remained effectively flat. Phase 10 owns
internal emission consolidation after the public model stabilizes; Phase 6
deliberately does not reorganize the macro implementation to optimize this
result.

The after commands used fresh `target/phase6-release-check` and
`target/phase6-release-rustdoc` directories. Peak memory and incremental
rust-analyzer timing were not measured by the available shell workflow.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo test -p concord --test generated_surface --all-features` | PASS; 1 named-surface test |
| `cargo test -p concord --test ui --all-features` | PASS; 1 compile-pass group and 5 compile-fail cases |
| `cargo test -p concord --test deterministic_generated_runtime --all-features` | PASS; 1 inertness/equivalence test |
| `cargo test -p concord --all-features` | PASS |
| `cargo test -p concord --no-default-features` | PASS; the raw feature-specific case is correctly excluded |
| `cargo test -p concord-macros --lib` | PASS; 33 compiler/model tests |
| `cargo test -p concord-macros --all-features` | PASS after the validation-driven corrections below; 33 compiler tests, 17 compiler/graph/rustdoc fixtures, all generated runtime suites, 9 dependency-policy tests, 26 UI groups, and doctests |
| `cargo test -p concord-core --lib --all-features` | PASS; 324 lifecycle/security/boundedness tests |
| `cargo test -p concord-core --all-features --test deterministic_redaction --test error_redaction --test hook_redaction --test proxy --test sanitized_metadata --test status_body_contract` | PASS; 17 seeded security/status-body tests |
| `cargo test -p concord-core --doc --all-features` | PASS; 4 privacy doctests |
| `cargo test -p concord-examples --all-features` | PASS; canonical construction remains inert |
| `cargo check -p concord-examples --all-targets --all-features` | PASS |
| `cargo check -p concord --all-targets --no-default-features` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo rustdoc -p concord --all-features -- -D warnings` | PASS |
| `cargo rustdoc -p concord-core --all-features -- -D warnings` | PASS |
| `cargo +1.85.0 check -p concord --all-targets --all-features` | PASS |
| `cargo +1.85.0 test -p concord --no-default-features` | FAIL only in three reviewed Trybuild rendering differences; all runtime tests, the named compile-pass case, and one compile-fail case pass, while the three remaining invalid programs still fail for the intended capability reasons. No snapshot was overwritten. |
| `cargo expand -p concord-macros --test generated_surface` with the recorded PowerShell counters | PASS; after metrics shown above |
| fresh timed `cargo check -p concord --all-targets --all-features --target-dir target/phase6-release-check` | PASS; 14.760 s |
| fresh timed `cargo rustdoc -p concord --all-features --target-dir target/phase6-release-rustdoc -- -D warnings` | PASS; 14.254 s |
| `rust-analyzer --version` | PASS; version recorded above |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to the installed 1.97.1 toolchain |

The first full macro run failed because one records rustdoc assertion still
looked under the deleted `endpoints/` tree. Inspection found the actual unified
paths, the assertion was corrected, and the complete command then passed.
A later audit run also rejected a redundant `ResponseBodySpec` bound on the
public endpoint associated type because it duplicated and worsened the focused
multipart diagnostic. The bound was removed, its focused UI case passed, and
the complete macro command passed again on the final worktree.

The reference validation spelling `-p concord_examples` is not the workspace
package ID and Cargo rejected it. The observed package is `concord-examples`;
the corrected required check passes.

The manual rust-analyzer completion, hover, suggestion ranking, and navigation
protocol cannot be performed from this shell-only session. The executable
version is recorded, but the manual interaction remains an explicit release
gate rather than a claimed pass.

## Deferred ordered work and deviations

Safe endpoint descriptions and the public `description` method are Phase 9
work. `PreparedEndpoint` remains as the existing advanced compatibility type
until the Phase 9 `RuntimeEndpoint` conversion. Client construction and
positional configured inputs remain Phase 7 work. These are ordered phase
boundaries, not Phase 6 design deviations.

The raw terminal uses a public sealed response-mode bound. Unsupported modes
produce a focused compiler diagnostic instead of successful capability
selection. A stable-Rust experiment with a mode-parameterized public request
wrapper was rejected because it exposed response-mode generics in every named
request type and unrelated diagnostic, weakening the primary predictable
surface. No architectural deviation was required.
