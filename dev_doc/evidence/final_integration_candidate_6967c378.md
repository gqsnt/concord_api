# Exact final-integration implementation evidence — `6967c378`

**Status:** Historical v2.1 evidence, superseded by the v3 lifecycle in
[`../release_gate_status.md`](../release_gate_status.md). It was recorded in
the v2.1 repository for the v2.1 candidate below and is not evidence for the
v3 tree. (Original v2.1 status: current implementation-candidate evidence; the
evidence-seal commit that contains this record must repeat the complete matrix
before branch movement.)

**Evidence date:** 2026-09-04

This record reports observed results for
`6967c378fad651ada9798580dd15c2b72168aa7c` on the selected Windows/Linux
support boundary. It does not claim macOS support, human comprehension,
independent human review, license authority, registry control, stable-release
readiness, or publication authority.

## Candidate and repair history

- Branch: `fix/v2.1-final-integration-truth`
- Main merge base: `f7464c58f4a60b7332547e8f91039a269171a6ff`
- Exact implementation candidate: `6967c378fad651ada9798580dd15c2b72168aa7c`
- An earlier Windows aggregate at `bbac56c0...` correctly failed one of 140
  no-default UI fixtures: the neutral maintenance-name rename had left three
  `v2_api` type paths in `current_language.rs`. Commit `6967c378...` replaced
  them with `current_api`, renamed the fixture's explicit ID, and the targeted
  harness plus both complete platform aggregates then passed.
- `git diff --check "$(git merge-base main HEAD)" HEAD`: exit 0.
- `git status --porcelain --untracked-files=all`: empty.

## Windows result

Environment: Microsoft Windows NT 10.0.26200.0, AMD64;
`rustc 1.97.1 (8bab26f4f)`, LLVM 22.1.6; `cargo 1.97.1 (c980f4866)`;
MSRV `rustc 1.86.0 (05f9846f8)`; just 1.58.0; cargo-nextest 0.9.143;
cargo-deny 0.20.2; actionlint 1.7.12; rust-analyzer 1.97.1;
PowerShell 7.6.5; Git 2.55.0.windows.4; bsdtar 3.8.4.

| Command | Exit | Observed result |
|---|---:|---|
| `just verify` | 0 | Every maintained recipe passed: fmt, check, lint, test, UI, MSRV, fixtures, docs, package, deny, workflow, analyzer, bench, source export, and release audit. |
| `cargo test --workspace --all-features --locked` | 0 | 1,103 harness-level non-doc tests and six doctests passed; the UI harness count includes its nested trybuild cases. |
| `cargo run --locked -p concord-xtask -- docs-check --locked` | 0 | 1 compiled DSL fence, 48 labeled pseudocode fences, 36 source anchors, 50 links, six machine-owned canonical anchors, and 107 active-file retired-language scans passed. |
| `cargo run --locked -p concord-xtask -- release-audit --locked` | 0 | Exact SHA, clean tree, branch diff, lifecycle, publication safety, process-artifact, clock, and authority checks passed. |
| terminal Git diff/status checks | 0 | Branch diff was whitespace-clean and status was empty. |

The maintained nextest runs were 888/888 under the default profile and 913/913
with all features. Macro parser tests were 190/190. The no-default and
all-feature generated-surface UI harnesses covered 140 and 151 trybuild cases;
the core source-only harness covered 14. Package verification built, extracted,
normalized, documented, checked, and ran a downstream consumer against all four
`2.1.0-beta.1` archives. Source export extracted and audited 887 entries.

## Linux result

Environment: Ubuntu 26.04.1 LTS under WSL2, Linux
6.18.33.2-microsoft-standard-WSL2, x86_64; native
`x86_64-unknown-linux-gnu` binaries; `rustc 1.97.1 (8bab26f4f)`, LLVM 22.1.6;
`cargo 1.97.1 (c980f4866)`; MSRV `rustc 1.86.0 (05f9846f8)`; just 1.58.0;
cargo-nextest 0.9.143; cargo-deny 0.20.2; actionlint 1.7.12;
rust-analyzer 1.97.1; PowerShell 7.6.5; Git 2.53.0; GNU tar 1.35.

The actionlint and PowerShell binaries missing from the minimal WSL image were
downloaded from their upstream release pages into an isolated user-local gate
directory. Their published SHA-256 values were verified before use. Ordinary
and package artifacts used a Linux-only Cargo target.

| Command | Exit | Observed result |
|---|---:|---|
| `just verify` | 0 | The same complete maintained recipe graph passed natively on Linux. |
| `cargo test --workspace --all-features --locked` | 0 | The same 1,103 harness-level non-doc tests and six doctests passed. |
| `cargo run --locked -p concord-xtask -- docs-check --locked` | 0 | Canonical-source, links, active release truth, diagnostics, and retired-language counts matched Windows. |
| `cargo run --locked -p concord-xtask -- release-audit --locked` | 0 | Exact SHA, clean tree, branch diff, lifecycle, publication safety, process-artifact, clock, and authority checks passed. |
| terminal Git diff/status checks | 0 | Branch diff was whitespace-clean and status was empty. |

Linux nextest counts were also 888/888 and 913/913; macro tests were 190/190;
all three UI configurations passed. Native Linux packaged-downstream
verification and the 887-entry exact-commit source-export extraction/audit both
passed. No resource failure occurred.

## Evidence-seal rule

This file and the accompanying active-status updates are documentation-only,
but they create a later commit. Before `release/v2.1-readiness` moves, that
exact evidence-seal commit must receive `just verify`, the explicit locked
workspace test, docs-check, release-audit, source export, diff check, and clean
status on both supported platforms. The terminal command output and final
operator handoff record that self-referential exact SHA; embedding it here
would create another SHA and is therefore intentionally not required.

Publication remains disabled. Stable release remains blocked by owner-selected
license/package authority and unobtained human-freeze evidence. No push, tag,
publication, license choice, package-name claim, or human result occurred.
