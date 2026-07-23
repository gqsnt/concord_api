# Phase 8 DSL v2 evidence

Phase 8 was implemented in the worktree based on commit `5ee17d5`.
Validation used Rust/Cargo 1.97.1 on Windows, with the declared Rust 1.85
minimum checked separately. The installed editor engine reports
`rust-analyzer 1.97.1 (8bab26f4 2026-07-14)`.

## Resulting declaration contract

Endpoint and scope source identifiers are now snake_case, including raw Rust
identifiers where needed. Those identifiers directly select the generated
method and module names. The resolved surface model derives deterministic
PascalCase endpoint, request, and scope type names from the same source
identifier. Endpoint aliases were removed so source syntax cannot introduce a
second naming authority.

Named client, scope, and endpoint clauses are order-independent. The parser
collects clauses by contract concept rather than advancing through an expected
sequence. Repeated scalar clauses produce one combined diagnostic attached to
both the duplicate and original clause keys. Order remains meaningful inside
paths, queries, headers, authentication placements, rate windows, and other
ordered lists. Authentication entries may be interleaved with other clauses
without changing their relative order; `replace` remains first-only and `off`
remains exclusive.

Endpoint request bodies use exactly one bare `#[body]` parameter annotation.
The body can have any valid parameter name and occur anywhere in the declared
argument list. Its position is retained in the AST, resolved model, direct
client method, and explicit endpoint constructor. Name-based `body` inference,
multiple body annotations, malformed body attributes, and defaults on a body
parameter are focused declaration errors.

Optionality is deliberately syntactic. `Option<T>`,
`std::option::Option<T>`, and leading-absolute `::std::option::Option<T>` are
optional controls. An alias to `Option<T>` is an ordinary required type because
a proc macro cannot resolve aliases. Defaults continue to use
`name: Type = expression`, remain distinct from syntactic optionality, and
retain declaration order.

Arbitrary request body, response, provider, and pagination-controller types
remain opaque Rust types. Production macro code has no JSON, CSV, NDJSON,
OAuth2, governor, or built-in controller name branch. The old
`oauth2_client` source receives the generic unknown-credential-source
diagnostic. Contract-level custom transport retry and rate-limit observer
selection were removed; applications configure those operational choices on
the generated client builder.

## Stable contract and rate-limit identity

An endpoint may declare:

```rust
id "riot.match_v5.match_by_id";
```

An explicit ID is 1–128 ASCII bytes split into dot-separated segments of 1–63
bytes. Every segment starts and ends with a lowercase ASCII letter or digit
and may additionally contain lowercase letters, digits, `_`, or `-`.
Endpoint IDs must be unique across the complete client; duplicate diagnostics
identify both declarations.

When the clause is absent, the resolved model derives
`snake_client.scope.endpoint`. Renaming a source client, scope, or endpoint
therefore changes the generated default and is documented in generated
Rustdoc. An explicit ID remains unchanged when the Rust-facing endpoint name
changes.

The contract ID is the endpoint identity supplied to `RequestMeta`, hooks,
errors, completion events, and generated endpoint rate-limit attachment.
Endpoint-local generated rate-limit identity is `<contract-id>.rate_limit`.
Client- and scope-level generated rate identities use the same deterministic
dotted source topology. No generated identity depends on `module_path!` or a
Rust type name.

Direct methods and explicit endpoint values resolve through the same endpoint
model, retain identical contract IDs, and continue through the same managed
executor. Deterministic runtime tests prove the explicit stable ID appears in
the same lifecycle event sequence for both call forms.

The generated façade bridge contract is version 21. The renamed-façade fixture
expands through `runtime::__private`; macros still do not emit a direct core
path for a one-dependency user.

## Compiler and public-surface proofs

The macro unit suite now covers:

- five-factor client and endpoint clause permutations and four-factor
  scope/child permutations;
- interleaved authentication and configuration clauses;
- duplicate diagnostics containing both spans;
- snake_case, acronym, and raw-identifier conversion;
- body-first, body-middle, body-last, invalid, inferred, and multiple-body
  declarations;
- unqualified, qualified, absolute-qualified, and alias-based optionality;
- stable-ID format, uniqueness, rename stability, and rate identity;
- required argument order in direct and explicit construction;
- removed custom retry, observer, and OAuth2-special syntax.

Public Trybuild cases prove a complete DSL v2 declaration compiles with only
the façade dependency and no generated prelude. Focused failures cover
duplicate clauses, inferred and multiple bodies, malformed body attributes,
PascalCase source identifiers, duplicate contract IDs, and removed custom
retry syntax. Former clause-order failure fixtures were replaced with
compile-pass programs.

The non-final body surface test declares
`update(id, #[body] payload, revision)` and type-checks both direct and explicit
construction in exactly that order. Canonical examples, the renamed façade
fixture, feature-absence fixtures, performance declarations, and current
documentation were converted to the same source language.

No Trybuild output was blanket-accepted. New snapshots were inspected from
their first compiler output, and source-name-only churn in existing snapshots
was reviewed. The known compiler-version rendering differences remain
unmodified.

## Lifecycle, security, and dependency proofs

The complete 325-test core library suite passes after removing two obsolete
generated custom-retry factory tests. It retains the existing inert
construction, immutable snapshot, authentication/limiter ordering, bounded
recovery, status-before-decode, bounded acquisition and streaming,
sequential/all-or-error pagination, result fixation, completion, cancellation,
and native transport privacy proofs.

The 17-test seeded redaction/status-body matrix passes. Stable endpoint and
rate-limit identities are static contract metadata and never include protected
path, query, header, credential, body, provider-response, or native request
material. Generated descriptions and Rustdoc expose the contract ID and
source-shape metadata only.

Dependency-tree inspection proves:

- `concord-macros` has no normal Concord runtime dependency;
- `concord-core` has no edge to macros, extras, or façade;
- `concord-extras` depends on core but not macros or façade;
- expansion through a renamed façade contains 30
  `::runtime::__private` paths, two `use ::runtime as __core` bridge aliases,
  zero direct `::concord_core` paths, and zero extras implementation names.

The direct macro development target still exercises the recorded Phase 14
direct-core fallback. It is not used as evidence for the final user path.

## Expansion measurement

The before and after expansion uses the same multi-API
`concord-macros/tests/generated_surface.rs` target and the Phase 7 counters.
Fragments are whitespace-delimited; declaration-shaped lines use the same
regex for structs, enums, traits, impls, functions, types, constants, and
modules.

| Metric | Phase 7 | Phase 8 | Change |
|---|---:|---:|---:|
| Expanded lines | 9,901 | 9,901 | 0 |
| Characters | 530,745 | 538,466 | +1.45% |
| Characters without newlines | 510,943 | 518,664 | +1.51% |
| Whitespace-delimited fragments | 33,878 | 34,742 | +2.55% |
| Declaration-shaped lines | 1,366 | 1,366 | 0 |

The increase is contract-ID documentation and identity metadata, not new
generated items. Peak memory, clean/incremental timing, and the complete
1/10/100/1,000-endpoint protocol were not measured in this phase and remain a
Phase 14 release gate.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo test -p concord-macros --quiet` | PASS; 39 compiler/model tests, 17 compiler/graph/Rustdoc fixtures, all generated suites, dependency-policy tests, 27 UI groups, and doctests |
| `cargo test -p concord-macros --test compile --quiet` | PASS; 17 compiler, dependency-graph, fixture, expansion, and Rustdoc proofs |
| `cargo test -p concord-macros --test ui` | PASS; all 27 UI groups |
| `cargo test -p concord --all-features --quiet` | PASS; façade runtime tests and all DSL v2 compile-pass/fail cases |
| `cargo test -p concord --no-default-features --quiet` | PASS on Rust 1.97.1 |
| `cargo test -p concord --test ui --all-features` | PASS |
| `cargo test -p concord --test generated_surface --all-features` | PASS |
| `cargo test -p concord --test deterministic_generated_runtime --all-features` | PASS; direct/explicit identity and lifecycle equivalence |
| `cargo check -p concord-examples --all-targets --all-features` | PASS |
| `cargo test -p concord-examples --all-features --quiet` | PASS |
| `cargo test -p concord-core --lib --all-features --quiet` | PASS; 325 lifecycle/security/boundedness tests |
| `cargo test -p concord-core --all-features --quiet` | FAIL only in the recorded eight Rust 1.97.1 Trybuild rendering differences after all library and non-UI integration tests pass; no snapshot was overwritten |
| `cargo test -p concord-core --all-features --test deterministic_redaction --test error_redaction --test hook_redaction --test proxy --test sanitized_metadata --test status_body_contract --quiet` | PASS; 17 seeded security/status-body tests |
| `cargo test -p concord-extras --all-features --quiet` | PASS; 37 implementation tests plus downstream and doctest groups |
| `cargo test --workspace --doc` | PASS; all 5 workspace doctests |
| `cargo rustdoc -p concord --all-features -- -D warnings` | PASS |
| `cargo rustdoc -p concord-core --all-features -- -D warnings` | PASS |
| `cargo rustdoc -p concord-extras --all-features -- -D warnings` | PASS |
| `cargo +1.85.0 check --workspace --all-targets --all-features` | PASS |
| `cargo +1.85.0 test -p concord-macros --lib --quiet` | PASS; all 39 compiler/model tests |
| `cargo +1.85.0 test -p concord --no-default-features --quiet` | FAIL only in four reviewed Trybuild rendering differences: `secret_requires_explicit_wrapper`, `non_paginated_no_paginate`, `wrong_client_endpoint`, and `non_clone_body_endpoint`; all runtime tests and all three compile-pass fixtures, including DSL v2, pass |
| `cargo expand -p concord-macros --test generated_surface` plus the recorded PowerShell counters | PASS; Phase 8 metrics shown above |
| renamed-façade expansion path counters | PASS; 30 façade-private paths, two bridge aliases, no direct core path, no extras implementation name |
| `cargo tree -p concord-macros -e normal`; all-feature core/extras tree audits | PASS; all forbidden Concord dependency edges absent |
| `rust-analyzer --version` | PASS; executable version recorded above |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to the installed 1.97.1 toolchain |
| `git diff --check` | PASS |

The exact full-core command retains the baseline Rust 1.97.1 diagnostic
rendering drift, and the Rust 1.85 façade command retains four known
compiler-version rendering differences. Each invalid program still fails for
its intended public capability reason. No snapshot was changed to conceal
toolchain differences.

The manual rust-analyzer completion, hover, suggestion ranking, and navigation
protocol cannot be performed from this shell-only session. The executable
version is recorded; manual interaction remains an explicit release gate.

## Deferred ordered work and deviations

Phase 9 owns safe generated/runtime descriptions, no-effect planning, the
public `PreparedEndpoint` to `RuntimeEndpoint` rename, bounded owned runtime
identity, and the final timeout tri-state surface. Phase 10 owns macro file
reorganization after those public contracts stabilize. Current broader
documentation still containing pre-refactor names is deliberately owned by
Phase 13.

No architectural deviation was required.
