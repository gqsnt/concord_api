# Concord v2.1 release-gate status

<!-- concord-release-status-owner -->

- **Canonical lifecycle status:** Concord v2.1 integration candidate / beta
- **Workspace version:** `2.1.0-beta.1`
- **Publication:** disabled in every intended public package manifest

**Decision:** NOT MERGEABLE while Gate 0, Gate 3, or Gate 10 is blocked

This is the repository's one authoritative lifecycle and gate-status document.
README files and `FINAL.md` may summarize and link here, but do not define a
second status. `Passed on the exact beta candidate` means only that the cited
machine evidence supports its scoped claim at `0f72e865...`; it does not turn
the beta into a stable candidate or replace an owner, human, or macOS gate.

| Gate | Status | Exact evidence | Remaining boundary |
|---|---|---|---|
| 0 — package identity, legal authority, and release namespace | **Blocked — owner authority required** | Official Cargo and crates.io API queries at 2026-09-04T10:45:42Z disconfirm the current façade identity: registry `concord` is an unrelated Discord TUI owned by `chojs23`. Candidate `concord-api*` names returned no records but are neither approved nor secured. No license grant exists. See `dev_doc/evidence/package_namespace_2026-09-04.md`. | Owner must complete `dev_doc/owner_release_decisions.md`; final names must be reverified/controlled, license and metadata implemented, and final archives rehearsed. |
| 1 — representation trait/driver | **Passed on the exact beta candidate** | `0f72e865...`: Windows `just verify`, Rust 1.86 checks, UI, warning-denied docs, extracted-package downstream; Linux all-feature check and 910/910 tests. | No mechanical residual at this beta SHA; final stable inputs would require a fresh final-candidate run. |
| 2 — representation inheritance | **Passed mechanically on the exact beta candidate** | `0f72e865...`: client/scope defaults, directional overrides, local exceptions, explicit fences, invalid pairs, provenance descriptions, and semantic-diff suites passed on Windows and Linux. | Human discoverability is not inferred; Gates 3 and 10 remain independently blocked. |
| 3 — unfamiliar-human final-surface comprehension | **Blocked — external participants required** | Zero qualifying unfamiliar humans have completed the final-surface protocol. The packet exists under `dev_doc/dx_study/`; AI and implementer review do not count. | At least three qualifying humans, required scores, fixes, and retests. |
| 4 — query/header/atom properties | **Passed on the exact beta candidate** | `b2a88d4` removed false `with_*` reservations and added pass/fail fixtures; `246dc5d` corrected reflected `origin`; `0f72e865...` UI, property, docs-truth, Windows, and Linux suites passed. | Gate 3 remains human-only; a later final stable input requires rerun. |
| 5 — normative runtime trace | **Passed on the exact beta candidate** | `0f72e865...`: Windows full aggregate and Linux 910/910 all-feature tests passed. Observer panics increment a private saturating counter and cannot change results. | Public observer-health reporting is explicitly Deferred by ADR 0013; no result-integrity limitation. |
| 6 — pagination proof suite | **Passed on the exact beta candidate** | `0f72e865...`: controller, safety/completeness, explicit-limit, identity, retry, cancellation, and all-or-error suites passed on Windows and Linux. Controller header/URI authority is explicit. | No mechanical residual at this beta SHA. |
| 7 — rate/server-delay behavior | **Passed on the exact beta candidate** | `0f72e865...`: controlled-clock full path, one typed `ServerDelayHint`, shared 7-second observer/limiter/status fact, whole-plan atomic local admission, extension contract, Windows aggregate, and Linux all-feature tests passed. | No mechanical residual at this beta SHA. |
| 8 — stream lifecycle/performance | **Blocked — macOS portability execution unavailable** | `0f72e865...`: Windows and Linux each observed `(1,0)` boxed construction/poll allocations, complete lifecycle/early drop, and all throughput budgets passing; Linux 910/910 tests included stream coverage. | Required macOS execution is zero; private erasure remains selected, not redesign-open. |
| 9 — information-flow security | **Passed on the exact beta candidate** | `0f72e865...`: value-free observer target, limiter negative space, unknown-value sentinels, explicit caller authority, Debug redaction, Windows full aggregate, and Linux all-feature tests passed. | No mechanical residual at this beta SHA. |
| 10 — docs, scale, hygiene, independent review, and supported platforms | **Blocked — independent human reviewer and macOS execution required** | `0f72e865...`: Windows `just verify`; Linux CI smoke plus 910/910 all-feature tests, docs, Rustdoc, packages; scale through 1,000 endpoints; analyzer 3,196 MiB; exact branch diff and clean tree. Human reviewers: zero. macOS runs: zero. See `dev_doc/evidence/beta_candidate_0f72e865.md`. | One qualifying first-contact reviewer and an actual macOS run remain mandatory. |

## Current evidence boundary

The current command record is
`dev_doc/evidence/beta_candidate_0f72e865.md`. The older
`dev_doc/release_verification_2_1.md` records a historical implementation state
and is not evidence for this beta. A complete Windows aggregate and real Linux
execution now exist for `0f72e865...`; no command can substitute for the open
owner, human, and macOS inputs.

No push, tag, publication, license selection, registry ownership, human result,
or platform execution may be inferred from this table.
