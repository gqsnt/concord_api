# Concord v2.1 release-gate status

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
| 4 — query/header/atom properties | **In remediation** | Historical alias-neutral, atom, patch, ordering, bounds, and noninterference tests exist. | Release stale `with_*` reservations, correct `host` diagnostic, add focused fixtures, and rerun. |
| 5 — normative runtime trace | **Passed mechanically; exact-candidate rerun pending** | Historical deterministic direct/body/retry/auth/physical-recovery/observer/cancellation suites. | Exact-candidate and platform reruns; observer panic diagnostics must be truthful. |
| 6 — pagination proof suite | **Passed mechanically; exact-candidate rerun pending** | Historical generated/core controller, safety/completeness, limit, identity, retry, cancellation, and all-or-error suites. | Exact-candidate/platform rerun and explicit controller metadata-authority documentation. |
| 7 — rate/server-delay behavior | **In remediation** | Historical key/plan/admission/feedback/cooldown tests; audit found ambient production `SystemTime::now()` and incomplete extension accounting wording. | Inject/capture wall time, share one typed delay fact, document/test whole-plan accounting, then rerun. |
| 8 — stream lifecycle/performance | **Passed for private implementation selection; portability evidence pending** | Historical `stream_driver_comparison` met one-construction-allocation, zero-per-poll-allocation, throughput, lifecycle, compile, and size budgets on Windows. | Exact-candidate Windows/Linux/macOS lifecycle execution; portability remains an observation only if budgets continue to pass. |
| 9 — information-flow security | **In remediation** | Historical sentinel suites prove response-header narrowing and explicit caller authority. | Remove unnecessary complete URL authority from automatic observer/limiter contexts, add unknown-value sentinels, and rerun. |
| 10 — docs, scale, hygiene, independent review, and supported platforms | **Blocked — external reviewer/platform evidence plus local remediation** | Historical scale/package/docs/analyzer evidence exists at or before `9470e24`; audited HEAD is `adad5326`. Zero qualifying independent reviews and no exact-candidate Linux/macOS execution exist. | Canonical cross-platform docs/release checks, clean branch diff, final package evidence, exact-HEAD full suite, Windows/Linux/macOS execution, and one independent Rustdoc/diff reviewer. |

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
