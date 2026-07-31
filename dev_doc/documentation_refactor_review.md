# Documentation refactor review

This file records the review of the documentation refactor, the defects found
across two correction passes, and the exact checks executed.

- **Base commit:** `9815770e6d81f346d1fbeab9b8cf166ada00dbae` ("fix md files", 2026-07-30)
- **Reviewed state:** working tree on top of that commit (mixed index: staged, unstaged, and new files)

## Verdict

APPROVED

Every required and strongly recommended correction was reproduced, fixed, and
validated. No check in the repository gate is failing, and no external blocker
remains.

## Scope statement

No public DSL syntax, generated capability, runtime semantics, extension trait
contract, error semantics, executor ordering, compatibility alias, or production
behavior changed.

Changes are confined to documentation, documentation fixtures, the documentation
checker, one documentation-focused test call, one stale UI fixture input and its
regenerated snapshot, and repository metadata required by those changes
(`.gitattributes`).

The one production-adjacent edit is the tutorial fixture's base URL
(`https` → `http`). That fixture is an example declaration in
`crates/concord-examples`, not product code, and the change makes its documented
feature requirement true rather than altering any Concord behavior.

## Changes made

### 1. Tutorial required TLS while documenting itself as `json`-only

**Problem.** `crates/concord-examples/src/getting_started/domain_tree.rs`
declared `base "https://catalog.example.test/"` while its module header claimed
`Required feature: json`. In `concord-examples`, `json` expands only to
`concord/json`, `serde`, `serde_json`, `serde/derive` — it does not enable
`default-tls`. The quick start, README, and mental model all render this
contract from the same anchors, so all three documented an untrue requirement.

**Evidence.** Reproduced directly; default features had been masking it:

```text
cargo nextest run -p concord-examples --test construction --no-default-features --features json

panicked at crates\concord-examples\tests\construction.rs:52:14:
domain tree tutorial construction: ClientBuildError { kind: TlsUnavailable, input_field: None, input_kind: None }
```

The rejection is real and intended: `crates/concord-core/src/http/build.rs:139`
returns `ClientBuildErrorKind::TlsUnavailable` when a `https` base is combined
with `TlsCapability::Unavailable`.

**Fix.** Changed the base to `http://catalog.example.test/` and recorded why in
the module header. This matches the established repository convention: every
example fixture that is construction-only or locally executed and is *not* gated
on `default-tls` already uses `http` (`minimal.rs`, `pagination/mod.rs`,
`bodies.rs`, `acquisition.rs`, `records.rs`, `lexical.rs`, `partitioning.rs`,
`hooks.rs`, `retries.rs`, `generated_and_runtime.rs`). The `https` fixtures are
either `default-tls`-gated or live in `docs_spec.rs`, which is never built into
a client.

The change was made in the fixture; the three rendered copies were updated to
match the anchor, which the checker then verified byte-for-byte.

**Files.** `crates/concord-examples/src/getting_started/domain_tree.rs`,
`docs/quick_start.md`, `docs/mental_model.md`, `README.md`.

**Validation.** `construction` passes with `--no-default-features --features json`
on stable and on 1.86.0, and also with `--no-default-features` alone.

### 2. Stale rate-limit opacity UI fixture

**Problem.** `crates/concord-core/tests/ui/fail/rate_limit_opaque_values.rs`
constructed `RateLimitPlan { rules: Box::new([]) }`, but the field is `policies`
(`crates/concord-core/src/rate_limit/types.rs:348-350`). The Phase 4 rename to
policy terminology left the fixture behind.

**Evidence.** The fixture failed for the wrong reason. Expected two `E0451`
private-field errors; actual output was a single `E0560` "struct `RateLimitPlan`
has no field named `rules`". A test that proves a field does not exist proves
nothing about opacity. Confirmed pre-existing: the snapshot's last commit is
`1ffb54c`, and `crates/concord-core` has no other change in this change set.

**Fix.** Corrected the fixture input to `policies` and regenerated only that
snapshot with `TRYBUILD=overwrite`. The regenerated output was inspected before
acceptance and now proves the intended property:

```text
error[E0451]: field `_private` of struct `RateLimitPermit` is private
error[E0451]: field `policies` of struct `RateLimitPlan` is private
```

The existing `rules` error was deliberately not blessed.

**Files.** `crates/concord-core/tests/ui/fail/rate_limit_opaque_values.rs`,
`crates/concord-core/tests/ui/fail/rate_limit_opaque_values.stderr`.

**Validation.** `cargo nextest run -p concord-core --test ui --no-default-features`
passes, and the full `just ui` recipe passes (197 + 5 + 6 + 1 tests). This
removes the only known-failing gate; it is no longer a known failure.

### 3. Source-anchor validation hardening

**Problem.** The checker claimed documentation examples come from compiled
source, but an anchor reference is untrusted metadata and was resolved with a
bare `Join-Path`. Absolute paths, `..` traversal, non-Rust files, and files
outside the examples crate would all have been read.

**Fix.** Added `Resolve-AnchorSourcePath` (rejects absolute paths, `..`
traversal, escapes from the repository root, non-`.rs` extensions, and anything
outside the approved roots) and `Get-AnchorRegions` (per-file integrity, cached).
Approved roots are `crates/concord-examples/src` and
`crates/concord-examples/tests`; both are compiled by `concord-examples` Cargo
targets, documented in `dev_doc/internals.md`.

**Deviation from the brief, with evidence.** The brief asked to fail on nested
anchors. The repository relies on proper nesting in nine fixtures, up to depth
three, and several documents quote inner anchors (`docs/pagination.md`
`#offset_endpoint`, `docs/riot_walkthrough.md` `#riot_paginated_request`).
Rejecting nesting would break current, correct documentation. The checker
therefore validates well-formedness — duplicate names, unopened `ANCHOR_END`,
unclosed anchors, overlapping regions, and empty regions all fail — while
permitting proper nesting and excluding nested markers from the compared source.

**Files.** `scripts/check_docs_examples.ps1`, `dev_doc/internals.md`,
`dev_doc/documentation_contract.md`.

**Validation.** Every rule was proven by injecting a fault and observing the
error, then restoring the tree (see the negative-test table below).

### 4. Markdown fragment validation

**Problem.** The checker resolved local Markdown file paths but ignored
`#fragment` targets, so `spec.md#pagination` was accepted whether or not that
heading existed.

**Fix.** Added ATX heading parsing (fence-aware), GitHub-compatible slug
normalization, duplicate-slug detection, and fragment checking for local
Markdown links. Five fragments are in use, all into `docs/spec.md`, and all
resolve.

**Files.** `scripts/check_docs_examples.ps1`, `dev_doc/documentation_contract.md`.

### 5. Validation documentation did not match the real gate

**Problem.** `README.md` and `dev_doc/internals.md` presented a flat list ending
in `cargo test --workspace` as the gate. That command does not run the UI
suites at all: `crates/concord-core/Cargo.toml` sets `test = false` on the `ui`
target, which is precisely why the stale fixture in change 2 went unnoticed. MSRV,
feature-isolation, fixture, and documentation checks are also separate.

**Fix.** Documented three levels against the real recipe names — fast loop
(`just fmt`, `just docs-check`), pre-commit (`just check`, `just test`,
`just ui`, `just docs`, `just msrv`), and release equivalent (`just verify`) —
and mapped them to the CI jobs in `.github/workflows/ci.yml`. No new recipe was
invented; `just verify` already existed.

**Files.** `README.md`, `dev_doc/internals.md`.

**Gap recorded, not silently fixed.** CI does not invoke
`scripts/check_docs_examples.ps1`. The `documentation` job runs doc tests and
rustdoc only. The checker is owned by `just docs` / `just docs-check` locally.
This is documented as the current state rather than papered over; adding a CI
step is a maintainer decision outside this review.

### 6. Awaited quick-start usage

**Problem.** The quick start demonstrated declaration and inert construction but
never showed the desirable generated usage the product argument rests on.

**Fix.** Added a compile-checked `execute` anchor showing a one-page `.await`, a
finite `.paginate(PageLimit::Take(2)).await`, and the typed `Vec<ItemSummary>`
outputs of both. Method names were taken from the generated surface and verified
by compilation, not invented. The function is compiled, never executed, so no
live HTTP or async runtime is required and the fixture stays `json`-only.

**Files.** `crates/concord-examples/src/getting_started/domain_tree.rs`,
`docs/quick_start.md`.

### 7. Whitespace, line endings, and a reliable patch

**Problem.** Three separate defects.

*Incomplete patch.* The working tree has a mixed index — 27 staged-only, 4
staged-and-unstaged, 2 unstaged-only, 4 deletions. Plain `git diff` sees only
unstaged changes and produced a 24 KB patch against a real change set of ~197 KB,
omitting all 17 new documentation pages and all 4 deletions. This is the most
likely cause of the reported "did not behave as a reliable directly applicable
patch". The correct command is `git diff HEAD`.

*Trailing whitespace.* `git diff --check` appeared clean but only inspects
unstaged changes. `git diff --check HEAD` reported 42 errors — the two-space
Markdown hard breaks on the `Audience:` / `Purpose:` header lines the refactor
added to 21 documents. `git diff --check` was probe-tested to confirm it detects
injected trailing whitespace, so the earlier clean result was scope, not a
broken check.

*Inconsistent line endings.* `.gitattributes` pinned `eol=lf` for `*.rs`,
`*.md`, `*.toml` and others but not `*.ps1`, so with `core.autocrlf=true` the
checker script produced a CRLF conversion warning on every diff and would be
stored inconsistently with every other text file.

**Fix.** Converted the metadata headers to bullet lists
(`- **Audience:** …`), which removes the trailing whitespace at the source
rather than suppressing the check, renders equivalently, and keeps the embedded
links intact. Added `*.ps1 text eol=lf`. Regenerated the patch with
`git diff HEAD --binary --no-ext-diff`.

*Encoding audit.* Every changed file was scanned for BOM, CRLF, mojibake
(`Ã`, `â€`, `ï»¿`, replacement characters), NUL bytes, and trailing whitespace.
All clean. Unicode arrows, box-drawing characters, and typographic quotes in the
documentation are intact UTF-8. The only CRLF file in the tree is the untracked
`pr_1.diff` artifact, which is not part of this change set.

**Files.** 21 documentation files, `.gitattributes`.

### 8. Review-report contradictions

**Problem.** The previous report stated in one section that link extraction now
ignores fenced and inline code, and in the remaining-risks section that it does
not respect code fences. It also implied the deleted page names never appear.

**Fix.** This report is rewritten. Resolved items are stated as resolved and are
absent from remaining limitations. Deleted-file wording is now precise: no active
user-facing documentation links to the deleted pages; the remaining occurrences
are the checker's removed-file list, `dev_doc/finalization_decisions.md`
historical entries, and this review report.

## Accuracy findings

Consequential claims were verified against source, not accepted as plausible.

| Claim | Verification |
|---|---|
| `Client::new()` gating | emitted only under `!client.has_required_construction_inputs()` (`concord-macros/src/emit/client.rs:126`) |
| `PageLimit::Take` / `HardCap` | `concord-core/src/pagination/pending.rs:14-21` matches the documented wording exactly |
| Paginated body support | `PageRequestBodySpec: RequestBodySpec where Self::Input: Clone` (`body/spec.rs:150-154`); page responses require `ResponseBodySpec<Mode = Buffered>` (`response/spec.rs:410`) |
| `response_bytes()` modes | `ResponseBytesMode` implemented only for `Buffered` and `Discard` (`response/spec.rs:127-131`); the emitted method carries that bound |
| Response mode mapping | `Bytes`/`Text`/`Json<T>` buffered, `Stream`/`Records<D>` streaming, `NoContent` discard |
| Fallback limiter fail-closed | `FallbackRateLimiter` grants empty plans, returns `InvalidConfiguration` otherwise (`rate_limit/limiter.rs:175-192`); generated clients additionally fail at build with `LimiterUnavailable` |
| Explicit limiter precedence | `install_generated_default_limiter` returns early when selection is `Explicit` |
| Generated value controls | `clear_*`, `set_*`, `reset_*` exercised against the real generated surface in a sibling crate |
| Retry builder API | `max_retries_per_operation`, `max_extra_load`, `classify_fn` (`retry/policy.rs:136,145,185`) |
| Façade modules | the list in `docs/generated_client.md` matches the `pub mod` set in `crates/concord/src/lib.rs`; `concord::prelude` exists |
| Runtime endpoint surface | `concord-core/src/request/prepared.rs:80-368` |
| TLS requirement | `http/build.rs:139-141` — the defect in change 1 |
| Rust 1.86 | the documented quick start compiles on 1.86.0 in a fresh sibling crate |

## Product-model assessment

The intended product model is preserved and stated concretely.

- **Domain tree first.** `overview.md`, `mental_model.md`, and
  `riot_walkthrough.md` present the client/scope/endpoint tree as an ownership
  and visibility graph; `mental_model.md` states that a scope "is not merely a
  path prefix or visual folder".
- **Signature as the primary domain statement.** The quick start teaches a
  two-pass reading; the Riot walkthrough reads one real endpoint from consumer,
  author, compiler, and runtime perspectives.
- **Generated Rust as a co-equal surface.** `generated_client.md` is framed as
  "a product surface, not an implementation detail". The quick start now ends in
  awaited, typed generated usage rather than an inert value.
- **Four distinct actions.** Declaration, attachment, binding, and execution stay
  separated and are reused consistently by the auth, rate-limit, and pagination
  guides.
- **Structured information becomes guardrails.** Contract/derived tables tie each
  piece of contract information to a generated control, an absence, or a
  validation.
- **Local authoring with named inherited context.** `modeling_apis.md` closes
  with "Limits to locality" rather than overclaiming.
- **Extensions escape syntax, not semantics.** Stated with reasons in
  `extensions.md` and `runtime_endpoints.md`.
- **Internal cost as consequence.** `overview.md` accepts the compiler,
  one-executor, and proof-surface costs; `design_invariants.md` frames the
  invariants as the cost of keeping both surfaces truthful.

Ownership boundaries hold: `spec.md`, `runtime.md`, and `design_invariants.md`
remain normative, and guides link to them instead of restating rules.
`execution_policy.md` reproduces the 15-stage order as explanation; it was
compared stage by stage with `runtime.md` and agrees.

## Validation table

Run on Windows 11, `rustc 1.97.1` (pinned by `rust-toolchain.toml`) unless a
toolchain is named. Every command below was executed and its exit code observed.

| Command | Result | Notes |
|---|---|---|
| `cargo fmt --all -- --check` | pass (0) | |
| `git diff --check` | pass (0) | unstaged scope only |
| `git diff --check HEAD` | pass (0) | complete change set; was 42 errors before change 7 |
| `cargo check --workspace --all-targets` | pass (0) | |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (0) | |
| `cargo nextest run --workspace` | pass (0) | 1052 run, 1052 passed, 0 skipped |
| `cargo test --workspace --doc` | pass (0) | all doc-test targets ok |
| `cargo doc --workspace --no-deps` | pass (0) | |
| `pwsh -File scripts/check_docs_examples.ps1` | pass (0) | 13 DSL fences, 28 anchors, 137 links, 5 fragments, inventory |
| `cargo nextest run -p concord-examples --test construction --no-default-features --features json` | pass (0) | 1 passed; **failed before change 1** with `TlsUnavailable` |
| `cargo +1.86.0 test -p concord-examples --test construction --no-default-features --features json` | pass (0) | 1 passed |
| `cargo nextest run -p concord-examples --test construction --no-default-features` | pass (0) | 1 passed |
| `cargo check -p concord-examples --all-features --all-targets` | pass (0) | |
| `cargo test -p concord-examples --all-features` | pass (0) | |
| `cargo check -p concord-examples --no-default-features --features json` | pass (0) | proves the awaited example needs no TLS |
| `cargo nextest run -p concord-core --test ui --no-default-features` | pass (0) | 1 passed; **failed before change 2** |
| `just check` | pass (0) | includes `concord-xtask -- policy`: "repository policy checks passed" |
| `just test` | pass (0) | 855 default-feature, 880 all-feature |
| `just ui` | pass (0) | 197 + 5 + 6 + 1 |
| `just msrv` | pass (0) | five `+1.86.0` checks plus `xtask msrv-ui` |
| `just docs` | pass (0) | documentation checker, all-feature doc tests, 14 rustdoc gates |
| `git diff HEAD --binary --no-ext-diff --output=documentation-refactor.diff` | pass (0) | 197,341 bytes; 55 files, 17 new, 4 deleted |
| `git apply --check --binary` in a base worktree | pass (0) | against `9815770` in a clean `git worktree` |
| `git apply --binary` then checker in that worktree | pass (0) | patched tree validates identically |

### Negative tests proving the checker actually fires

Each fault was injected and the tree restored afterward.

| Injected fault | Result |
|---|---|
| absolute anchor path | `source path must be repository-relative, not absolute` |
| `..` traversal in anchor path | `source path must not traverse upward with ".."` |
| anchor into `crates/concord-core` | `source path is outside the approved anchor roots (...)` |
| anchor into `Cargo.toml` | `source path must be a .rs file` |
| anchor into a nonexistent file | `source file does not exist` |
| undefined anchor name | `anchor 'nope' is not defined in ...` |
| duplicate anchor name | `duplicate anchor 'a' in ...` |
| unclosed anchor | `unclosed anchor 'a' in ...` |
| `ANCHOR_END` with no opening | `anchor 'a' ... is closed but was never opened` |
| overlapping anchor regions | `anchor regions overlap ...: 'a' closes while 'b' is still open` |
| empty anchor region | `anchor 'a' ... selects no source lines` |
| missing heading fragment | `docs\spec.md has no heading for fragment '#no-such-heading'` |
| duplicate heading slug | `ambiguous fragment '#same': ... duplicate headings with that slug` |
| fragment inside inline code | correctly ignored, no false positive |
| DSL fence retagged `rust` | 13 language errors |
| anchored example edited | `fenced source differs from ...` |
| link to a nonexistent file | `missing local link target` |
| reference to a deleted page | `references removed documentation file` |
| docs page absent from index | `must be linked exactly once; found 0` |
| docs page linked twice | `must be linked exactly once; found 2` |
| injected trailing whitespace | `git diff --check` exit 2 |

## Deleted pages

`docs/advanced_endpoints.md`, `docs/bodies_and_responses.md`,
`docs/retry_and_rate_limit.md`, and `docs/customization.md` are absent from the
working tree and from the patched base worktree.

No active user-facing documentation links to the deleted pages. Remaining
references are historical records, validation metadata, or this review report:
the checker's `$removedDocs` list, two entries in
`dev_doc/finalization_decisions.md`, and the text above. The checker enforces
their absence from `README.md` and `docs/`.

## Generated patch

`documentation-refactor.diff` in the repository root, deliberately left
untracked. The repository has no convention of committing patches, so it is not
added. It was produced with `git diff HEAD --binary --no-ext-diff` and verified
by applying it to a clean `git worktree` at `9815770`.

The untracked `pr_1.diff` in the repository root is a pre-existing artifact, not
part of this change set. It is the only CRLF file in the tree and is neither
tracked nor ignored; a maintainer may want to delete it or add it to
`.gitignore`.

## Remaining limitations

1. **CI does not run the documentation checker.** `just docs` and
   `just docs-check` do. A documentation-contract regression would reach `main`
   without a local run. Adding a step to the `documentation` job is a
   maintainer decision.
2. **The DSL fence heuristic inspects only the first non-blank line.** A
   schematic block starting with a label bypasses the `concord` fence
   requirement. `docs/overview.md` relies on this deliberately for an
   abbreviated signature that uses a bare `DEFAULT` placeholder. The heuristic
   is a guard, not a proof.
3. **Heading-slug normalization is deterministic but not a full GitHub
   implementation.** It covers the repository's current heading syntax
   (text, inline code, links, emphasis). Exotic headings could slug differently
   than GitHub.
4. **`docs_spec.rs` DSL matching is substring-based.** A `concord` fence is
   accepted if its whitespace-normalized text appears anywhere in the fixture,
   so a fence could match across unrelated fixture regions. Anchored fences are
   exact and unaffected.
5. **The working tree has a mixed index.** This is how the change set was
   received. Anyone regenerating a patch must use `git diff HEAD`, not
   `git diff`.

No unresolved defect remains. Items 1 and 5 are situational notes; items 2–4 are
bounded properties of the checker's design.
