# Phase 7 client construction and runtime policy evidence

Phase 7 was implemented in the worktree based on commit `f6d6b93`. Validation
used Rust/Cargo 1.97.1 on Windows. The installed editor engine reports
`rust-analyzer 1.97.1 (8bab26f4 2026-07-14)`.

## Resulting construction contract

Generated clients now have one construction rule:

- every client exposes `Client::builder()`;
- `Client::new()` is emitted only when the resolved client has no required
  value, protected secret, or custom credential provider;
- configured clients have no positional constructor;
- required values, secrets, and providers use predictable named setters;
- protected setters accept exactly `SecretString`;
- missing inputs fail at `build()` with `ClientBuildErrorKind::MissingInput`,
  a static `missing_field()`, and a `ClientInputKind` of `Value`, `Secret`, or
  `CredentialProvider`.

The missing field is the generated Rust setter identifier selected by the
resolved macro model. The error retains neither a supplied value nor protected
material. Seeded tests inspect both `Display` and `Debug`.

`configure_http` and `configure_runtime` are consuming, repeatable
transformations. Each closure receives the current builder, runs immediately,
and returns the next builder. Calls compose in source order; the last
selection for one setting wins. The former duplicate-call flags, suppression,
and synthetic duplicate-configuration errors were removed.

Repeated provider setters also replace in order. A deterministic drop-state
test proves that the previous provider is dropped immediately and the selected
provider remains owned by the immutable built client. Repeated explicit
limiter selections similarly prove that only the last explicit limiter runs
and that façade-generated defaults never override it.

The hidden generated bridge contract is version 20. Missing-input metadata and
runtime construction still flow through the façade bridge, including renamed
façade dependencies; macros do not name an extras implementation.

## Runtime policy and timeout ownership

Dangerous custom transport retry is selected explicitly with:

```rust
Api::builder()
    .configure_http(|http| http.dangerous_custom_retry(factory()))
    .build()?;
```

Canonical declarations and examples no longer use `retry custom PATH`.
Likewise, canonical rate-limit examples construct an extras governor with an
observer and install the limiter through `configure_runtime`. The old custom
retry and observer DSL clauses remain documented as legacy parser surface for
their ordered Phase 8 removal.

`RuntimeConfig` now owns the built client's operation-timeout default.
Generated client-level timeout declarations seed that immutable snapshot, and
runtime endpoints genuinely inherit it. The existing advanced
`PreparedEndpoint` compatibility type now retains an internal
`Inherit | Explicit | Disabled` selection instead of collapsing inheritance
and disablement into `None`; Phase 9 still owns its public rename to
`RuntimeEndpoint`.

Deterministic gated-loopback tests prove:

- generated endpoint declaration defaults remain effective;
- endpoint and request overrides remain distinct;
- disabled timeouts remain disabled;
- runtime endpoint inheritance uses the built snapshot;
- request-local explicit and disabled selections override that snapshot;
- client clones retain the same immutable runtime owner.

No timeout lookup, hook, provider, limiter, body work, transport, or completion
work was moved into endpoint or request construction.

## Security and lifecycle proofs

The complete 327-test core library suite passes. It retains the existing
authentication ordering, bounded acquisition, one-recovery limit, limiter
ordering, status handling, streaming/pagination limits, cancellation, result
fixation, completion, and native transport privacy proofs while adding:

- structured value-free missing-input errors;
- immutable client operation-timeout snapshot checks;
- exact runtime endpoint timeout tri-state behavior.

The focused seeded redaction/status-body matrix passes all 17 tests. Generated
authentication tests additionally seed the first and last protected builder
values and prove neither appears in missing-provider/secret `Display` or
`Debug`. OAuth/provider and explicit-limiter façade tests continue to use only
the managed lifecycle and semantic extension contracts.

Core public Rustdoc was also rebuilt from a clean fixture during validation.
That exposed a stale Phase 6 assertion expecting terminal methods on the
generic core carrier. Phase 6 deliberately moved ordinary terminal discovery
to generated named request wrappers and removed documentation-only fake
methods. The corrected core assertion now proves the private generic endpoint
bounds stay hidden; the generated renamed-façade Rustdoc fixture remains the
owner of public terminal documentation.

## Expansion measurement

The before and after expansion uses the same multi-API
`concord-macros/tests/generated_surface.rs` target and the Phase 6 counters.
Fragments are whitespace-delimited; declaration-shaped lines use the same
regex for structs, enums, traits, impls, functions, types, constants, and
modules.

| Metric | Phase 6 | Phase 7 | Change |
|---|---:|---:|---:|
| Expanded lines | 10,105 | 9,901 | -2.02% |
| Characters | 540,178 | 530,745 | -1.75% |
| Characters without newlines | 519,968 | 510,943 | -1.74% |
| Whitespace-delimited fragments | 34,314 | 33,878 | -1.27% |
| Declaration-shaped lines | 1,366 | 1,366 | 0 |

Removing the positional constructor and duplicate-configuration state more
than offsets the named missing-input and runtime-timeout additions for this
fixture. Peak memory, clean timing, and incremental rust-analyzer timing were
not measured in this phase; the full 1/10/100/1,000-endpoint performance
protocol remains a Phase 14 release gate.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo test -p concord --all-features` | PASS; configured-constructor absence, explicit secret type, named builder, façade composition, provider, limiter, and runtime suites |
| `cargo test -p concord --no-default-features` | PASS; the named builder and explicit-secret proof are feature-neutral |
| `cargo test -p concord-macros --all-features` | PASS; 33 compiler/model tests, 17 compiler/graph/Rustdoc fixtures, all generated runtime suites, 9 dependency-policy tests, 26 UI groups, and doctests |
| `cargo test -p concord-core --lib --all-features` | PASS; 327 lifecycle/security/boundedness tests |
| `cargo test -p concord-core --all-features --test public_rustdoc -- --test-threads=1` | PASS; 2 clean rendered-Rustdoc contract tests |
| `cargo test -p concord-core --all-features` | FAIL only in the recorded eight Rust 1.97.1 Trybuild rendering differences after the 327 library tests and non-UI integrations pass; no snapshot was overwritten |
| `cargo test -p concord-core --all-features --test deterministic_redaction --test error_redaction --test hook_redaction --test proxy --test sanitized_metadata --test status_body_contract` | PASS; 17 seeded security/status-body tests |
| `cargo test -p concord-extras --all-features` | PASS; 37 implementation tests, 2 downstream contract tests, and 1 doctest |
| `cargo check -p concord-examples --all-targets --all-features` | PASS |
| `cargo test -p concord-examples --all-features` | PASS; canonical construction remains inert |
| `cargo test --workspace --doc` | PASS; all 5 workspace doctests |
| `cargo rustdoc -p concord --all-features -- -D warnings` | PASS |
| `cargo rustdoc -p concord-core --all-features -- -D warnings` | PASS |
| `cargo rustdoc -p concord-extras --all-features -- -D warnings` | PASS |
| `cargo +1.85.0 check --workspace --all-targets` | PASS |
| `cargo +1.85.0 check --workspace --all-targets --all-features` | PASS |
| `cargo +1.85.0 test -p concord --no-default-features` | FAIL only in four reviewed Trybuild rendering differences: the three recorded Phase 6 cases plus whitespace rendering in the new explicit-secret diagnostic. Both compile-pass cases and every runtime test pass; each invalid program still fails for its intended reason. |
| `cargo +1.85.0 test -p concord-extras --no-default-features` | PASS |
| `cargo expand -p concord-macros --test generated_surface` plus the recorded PowerShell counters | PASS; Phase 7 metrics shown above |
| `rust-analyzer --version` | PASS; executable version recorded above |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to the installed 1.97.1 toolchain |
| `git diff --check` | PASS |

The first generated-builder compile exposed one missing local setter identifier
in secret error emission; it was corrected before the recorded checks. Initial
new Trybuild cases had no snapshots: their diagnostics were inspected and the
expected files were written deliberately, not blindly blessed.

Validation then found three migration omissions:

1. two macro privacy cases still invoked configured positional constructors
   and therefore failed before reaching their intended credential-handle
   absence; they now build through named setters and their reviewed line-only
   snapshots pass;
2. one provider replacement test attempted authenticated cleartext execution
   and correctly failed the HTTPS preflight; it was replaced by deterministic
   ownership/drop-state evidence that tests replacement without weakening the
   transport invariant;
3. generated-contract mismatch fixtures still expected version 19; they now
   intentionally test version 20.

An all-feature versus no-default compiler-help difference affected the
configured-`new` E0599 candidate list. Its exact Trybuild snapshot runs in the
all-feature matrix, while feature-neutral builder success and configured
construction semantics run in both matrices. This is test-harness
normalization, not a product-design deviation.

The manual rust-analyzer completion, hover, suggestion ranking, and navigation
protocol cannot be performed from this shell-only session. The executable
version is recorded, but manual interaction remains an explicit release gate.

## Deferred ordered work and deviations

Phase 8 removes the legacy custom-retry and observer DSL clauses while
implementing the complete order-independent DSL v2 grammar. Phase 9 performs
the public `PreparedEndpoint` to `RuntimeEndpoint` rename and description/plan
work. Those are ordered phase boundaries, not Phase 7 deviations.

No architectural deviation was required.
