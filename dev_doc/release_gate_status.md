# Concord v2.1 release-gate status

<!-- concord-release-status-owner -->

- **Canonical lifecycle status:** Concord v2.1 integration candidate / beta
- **Workspace version:** `2.1.0-beta.1`
- **Publication:** disabled in every intended public package manifest

**Decision:** NOT MERGEABLE while Gate 0, Gate 3, or Gate 10 is blocked

This is the repository's one authoritative lifecycle and gate-status document.
README files and `FINAL.md` may summarize and link here, but do not define a
second status. `Passed mechanically` means only that the cited historical
command or test supports its scoped claim; all machine gates require an
exact-final-candidate rerun before stable finalization.

| Gate | Status | Exact evidence | Remaining boundary |
|---|---|---|---|
| 0 — package identity, legal authority, and release namespace | **Blocked — owner authority required** | Official Cargo and crates.io API queries at 2026-09-04T10:45:42Z disconfirm the current façade identity: registry `concord` is an unrelated Discord TUI owned by `chojs23`. Candidate `concord-api*` names returned no records but are neither approved nor secured. No license grant exists. See `dev_doc/evidence/package_namespace_2026-09-04.md`. | Owner must complete `dev_doc/owner_release_decisions.md`; final names must be reverified/controlled, license and metadata implemented, and final archives rehearsed. |
| 1 — representation trait/driver | **Passed mechanically; exact-candidate rerun pending** | Historical Rust 1.86, workspace/UI, downstream multi-pair/custom representation, sealed-driver privacy, and built-in response matrix evidence at or before `9470e24`. | Rerun on the exact final candidate and supported platforms. |
| 2 — representation inheritance | **Passed mechanically; human evidence pending** | Client/scope defaults, directional overrides, inline exceptions, explicit fences, invalid pairs, provenance descriptions, and semantic diff tests at or before `9470e24`. | Gate 3 and Gate 10 must establish human discoverability; exact-candidate rerun required. |
| 3 — unfamiliar-human final-surface comprehension | **Blocked — external participants required** | Zero qualifying unfamiliar humans have completed the final-surface protocol. The packet exists under `dev_doc/dx_study/`; AI and implementer review do not count. | At least three qualifying humans, required scores, fixes, and retests. |
| 4 — query/header/atom properties | **Passed mechanically; exact-candidate rerun pending** | `b2a88d4` removed false `with_*` reservations, added pass/fail fixtures, and corrected the `origin` diagnostic; macro library 189/189 and façade UI 139/139 passed. | Full exact-final-candidate and platform reruns; Gate 3 remains human-only. |
| 5 — normative runtime trace | **Passed mechanically; exact-candidate rerun pending** | `860d2ad` passed the all-feature workspace suite and focused completion/panic tests. Contained observer panics increment a private bounded counter and never change results. | Exact-candidate/platform reruns; public observer-health reporting is explicitly Deferred by ADR 0013. |
| 6 — pagination proof suite | **Passed mechanically; exact-candidate rerun pending** | Historical generated/core controller, safety/completeness, limit, identity, retry, cancellation, and all-or-error suites; `860d2ad` all-feature workspace run remained green. Controller header/URI authority is explicit in active docs. | Exact-candidate/platform rerun. |
| 7 — rate/server-delay behavior | **Passed mechanically; exact-candidate rerun pending** | `860d2ad`: the production path captures injected wall time once, parses one typed `ServerDelayHint`, and the deterministic full-path test observes the same 7-second fact at observer, limiter, and status error. Local whole-plan admission is atomically tested and extension rollback limits are documented. | Full exact-final-candidate and platform reruns. |
| 8 — stream lifecycle/performance | **Passed for private implementation selection; portability evidence pending** | Historical `stream_driver_comparison` met one-construction-allocation, zero-per-poll-allocation, throughput, lifecycle, compile, and size budgets on Windows. | Exact-candidate Windows/Linux/macOS lifecycle execution; portability remains an observation only if budgets continue to pass. |
| 9 — information-flow security | **Passed mechanically; exact-candidate rerun pending** | `860d2ad`: observers receive only value-free `RequestTarget`; limiters receive no URL/path/query/header data and only conditional normalized origin. Compile-fail negative-space and unknown-value runtime sentinel tests passed while explicit caller access remained. `RequestPlan`/`ApiResponse` Debug omit exact metadata. | Full exact-final-candidate and platform reruns. |
| 10 — docs, scale, hygiene, independent review, and supported platforms | **Blocked — external reviewer/platform evidence plus exact-candidate suite required** | `5577c4a` release-audit passed the real `main...HEAD` whitespace check, clean status, process-artifact, clock/authority, single-status-owner, parser/spec/name, 29 source-anchor, 56 conceptual-fence, 48-link, and retired-language checks on Windows. Historical scale/package/analyzer evidence remains scoped to earlier SHAs. Human reviewers completed: zero. Exact-candidate Linux/macOS executions: zero. | Complete exact-HEAD machine/package suite, Windows/Linux/macOS execution, and one independent first-contact Rustdoc/diff reviewer. |

## Current evidence boundary

The historical command-by-command record is
`dev_doc/release_verification_2_1.md`. It records a `2.1.0-rc.1` implementation
state and is not evidence for the current beta HEAD. The last aggregate run at
`9470e24` stopped at `cargo deny` because the public packages had no authorized
license metadata; later `adad5326` documentation changes never received that
aggregate. The current beta must receive fresh exact-SHA evidence after all
independent remediation and mandatory external inputs exist.

No push, tag, publication, license selection, registry ownership, human result,
or platform execution may be inferred from this table.
