# Concord v3 release-gate status

<!-- concord-release-status-owner -->

- **Canonical lifecycle status:** Concord v3 unpublished local beta — implemented source candidate with named validation gaps (integration candidate / beta lifecycle)
- **Workspace version:** `3.0.0-beta.1`
- **Publication:** disabled — every package manifest in the workspace sets `publish = false`
- **Supported-platform boundary:** Windows and Linux are supported and mandatory. macOS is not currently a supported or release-gated platform.
- **Recorded:** 2026-09-26 (integration state updated 2026-09-27)

This is the repository's only authoritative lifecycle and gate-status document.
`README.md`, [`FINAL.md`](../FINAL.md), the
[claim/evidence ledger](claim_evidence_ledger.md), the
[ADR index](adrs/README.md) and evidence records may summarize it but do not
define another status.

**Beta integration decision:** OPEN — NOT MADE BY THIS IMPLEMENTATION. The v3
source is implemented in this tree and was exercised only in the validation
environment below. The specification names native Windows as the authoritative
integration environment (§20.2) and requires the pinned and MSRV toolchain
checks (§28.2); none of those ran. Merging the beta into `main` or a release
branch is an owner decision taken on the evidence in the implementation report.

**Current integration state** (recorded 2026-09-27):

| Item | State |
|---|---|
| Complete v3 source committed on the integration branch | **Done** — branch `concord-v3-beta`, integration commit `60d7de41b848cd561bd6597ae2593e41ee56b0a1` ("feat!: implement Concord v3 beta"), made directly on top of the real `main` commit `3765907aa2e424997bfd8f2fcdcc95275e6142f5`; `main` itself was not advanced |
| R5-01 follow-up (removed generated members referenced as function values inside unexpanded macros) | **Corrected** in the follow-up commit on `concord-v3-beta` that contains this line; tests and the actual-tool differential are recorded in the R6 return, not asserted here |
| Native Windows, pinned `1.97.1`, MSRV `1.86` | **Pending** — not run |
| Performance (V3-O07) | **Not accepted** |
| Merge into `main` | **Not performed** |
| Stable/public release and publication | **Not authorized** |

**Stable publication decision:** BLOCKED — owner package/license authority,
official metadata and a stable-version decision are open. Stable `3.0.0`, tags,
public package names, registry publication, pushes and publication claims are
not authorized.

## Scope of this implementation

- Version `3.0.0-beta.1`, an **unpublished local beta**. All eleven workspace
  package manifests (`crates/concord`, `crates/concord-core`,
  `crates/concord-macros`, `crates/concord-extras`, `crates/concord-inspect`,
  `crates/concord-test-support`, `crates/concord-examples`, `tools/xtask`,
  `tools/perf`, `tests/downstream/concord-downstream-api`,
  `tests/downstream/concord-downstream-app`) set `publish = false` (checked by
  `grep` on 2026-09-26).
- Publication, licensing and remote-release decisions (license selection,
  package names, registry ownership, repository/homepage metadata, pushes, tags)
  are **separate owner decisions that this implementation does not make**. The
  open questions are listed in [`owner_release_decisions.md`](owner_release_decisions.md).
- Command evidence — every command, its exit status, test counts and the exact
  tested source hash — is recorded in the implementation report
  (`IMPLEMENTATION_REPORT.md`, produced separately and not stored in this
  repository). This record does not restate those results and asserts no gate
  outcome it did not observe.

## Validation environment

| Fact | Value |
|---|---|
| Operating system / architecture | Linux x86_64 (kernel 6.18) |
| Compiler used | `rustc 1.95.0 (59807616e 2026-04-14)`, stable channel, with `cargo 1.95.0` |
| Why not the pinned toolchain | `rust-toolchain.toml` pins `1.97.1`; it was not installable offline in the validation environment |
| Why no MSRV run | The declared `rust-version = "1.86"` toolchain was not installable offline |
| Native Windows | **Not run** |
| macOS | **Not run** (not a supported or release-gated platform) |
| Dependencies | Offline, vendored, locked (`Cargo.lock`) |

UI (trybuild) expectations in this tree were produced on `rustc 1.95.0`; two
`concord-core` UI expectations were re-blessed on that toolchain (commit
`29e2cb6`). The specification (§28.2) regenerates version-specific output only
on the pinned compiler, so these snapshots are not pinned-compiler evidence.

## Beta integration gates

| Integration gate | State in this record | Evidence boundary |
|---|---|---|
| Workspace build, tests, UI suites, Clippy, Rustdoc and xtask policy on Linux x86_64 / rustc 1.95.0 | Not asserted here | Results, counts and exit codes: `IMPLEMENTATION_REPORT.md` |
| Two-crate downstream package (`tests/downstream/`) | Not asserted here | `IMPLEMENTATION_REPORT.md` |
| Acceptance obligations V3-L01…V3-O08 (spec §27) | Not asserted here; V3-E15, V3-E21, V3-P10 and V3-O04 were reopened by the external review and repaired, pending final recorded validation (next section) | Mapped to tests and results in the implementation report; ADRs 0014–0026 name the tests behind each selected decision |
| Native Windows (authoritative integration environment, spec §20.2) | **Not run** | No Windows run was performed for v3; no Windows claim is made for v3 |
| Pinned toolchain `1.97.1` | **Not run** | Toolchain not installable offline |
| MSRV `1.86` public-library checks | **Not run** | Toolchain not installable offline |
| `docs-check` and `release-audit` | Not asserted here | See the next section; a result belongs only to the exact commit it ran on |

The documentation slice's own observed runs (a subset: macro, core, extras,
xtask and downstream tests, plus `docs-check`/`release-audit`) are recorded,
with commits and exit codes, in
[`evidence/v3_documentation_slice_observations.md`](evidence/v3_documentation_slice_observations.md).
They are scoped to those commands and are not the gate evidence above.

## External review of the `fc3e4d1` return

An independent review (26 September 2026) of the implementation return
associated with commit `fc3e4d1` (`3.0.0-beta.1`) retained the candidate and
requested bounded corrections before integration or implementation
acceptance. It found three source-level defects — F01 (credential storage
observed as finished before local installation), F02 (ready data or an end of
body erasing an expired pending-read idle deadline) and F03 (`migrate-v3`
renaming an unrelated type's setter) — gave the specific dispositions below,
and marked every other obligation `NOT_INDEPENDENTLY_REACCEPTED`. This record
asserts the acceptance of none of them.

| Obligation | Review disposition | State in this tree |
|---|---|---|
| V3-E15 | Extend the oracle for F01: the failed-install error kept its contact facts, but its final event could still report Success | **Reopened by the external review for an extended oracle; repaired, pending final recorded validation.** The installation failure's one final observation reports that failure for the same operation; the contact checks are kept ([ADR 0021](adrs/0021-credential-intent-epoch-installed-version-and-store-outcome.md), claim C68) |
| V3-E21 | Reopen for F01: a slow final observer could change a credential-storage outcome | **Reopened by the external review; repaired, pending final recorded validation.** Storage finalizes after local installation; a slow final observer cannot change Installed, Superseded or a fixed error ([ADR 0021](adrs/0021-credential-intent-epoch-installed-version-and-store-outcome.md), claim C68) |
| V3-P10 | Reopen for F02: an idle deadline that expired while a read was pending could be erased by ready data or EOF on resume | **Reopened by the external review; repaired, pending final recorded validation.** One `StreamLease::check(now)` at every poll and every observed completion ([ADR 0011](adrs/0011-public-stream-carriers.md), claim C69) |
| V3-O04 | Reopen for F03: receiver inference lost qualification and field-owner identity | **Reopened by the external review; repaired, pending final recorded validation.** Default-deny receiver identity ([decision record](decision_record.md#migrate-03-receiver-identity-external-review-f03), claim C70). Later reviews added generated-member additions (R3-01) and removals (R4-01) and, in the follow-up commit, removed members referenced as function values inside unexpanded macros (R5-01) |
| V3-R10, V3-R11 | Correct two cited core test paths that omit `protected::` | Reference correction only; ADR 0024 and claim C51 name the tests with their `protected::` module |
| V3-O06 | Keep pending | **Pending** — native Windows, pinned `1.97.1`, MSRV `1.86` and packaged downstream evidence remain absent |
| V3-O07 | Keep not accepted | **Not accepted** — performance is measured, acceptance is not claimed |

"Repaired" means the corrected source and its discriminating tests are in this
tree; their results are recorded in the r2 validation run and are not
asserted here. Acceptance of the reopened obligations requires recorded
validation on the final corrected source digest (review §8); no intermediate
audit is promoted to acceptance. Nothing in this section publishes or releases
anything: the workspace stays `3.0.0-beta.1` with `publish = false`.

## Repository audit commands

`cargo run -p concord-xtask -- release-audit` first runs `docs-check`, then
requires a local `main` branch as the branch-diff merge base, a clean worktree,
the placeholder/clock/authority scans, `publish = false` in the four product
manifests, and this file to name the exact workspace version together with the
beta blocker text. This record satisfies the last condition truthfully (the
workspace version above and the `Blocked — owner authority required` Gate 0 row
below).

On 2026-09-26 the isolated implementation repository had no local `main`
branch (its branches were `baseline-v2.1`, `v3-implementation` and per-engineer
worktree branches). Since the integration of 2026-09-27 the audit runs in the
real repository history: branch `concord-v3-beta` with the real `main`
(`3765907`) as its merge base. `main` is not created, moved or merged by this
work.

## Stable-publication gates

| Stable gate | State | Required evidence or authority |
|---|---|---|
| Gate 0 — package identity, legal authority and release namespace | **Blocked — owner authority required** | Owner-selected license policy, approved and controlled package names, official metadata and a final-name package rehearsal. The registry name `concord` was observed on 2026-09-04 to belong to an unrelated project. |
| Native Windows execution of the exact v3 candidate | **Not run** | Complete maintained matrix on Windows, recorded with exit codes and the exact source hash |
| Pinned-toolchain and MSRV matrix | **Not run** | `1.97.1` and `1.86` checks, including UI output regenerated on the pinned compiler |
| Unfamiliar-human comprehension of the v3 surface | **Unknown** | No participants; the v2.1 study packet (`dx_study/`) targets the v2.1 surface and would need revision for v3 |
| Independent Rustdoc/semantic-diff review | **Unknown** | No independent review; implementer or AI review is not substituted |
| Stable lifecycle and publication mechanics | **Blocked** | Gate 0 and the owner-selected human-freeze policy must close before stable `3.0.0`, final names, tags or registry action |

## History

The previous version of this record owned the Concord `2.1.0-beta.1`
lifecycle, recorded in the v2.1 repository. This repository's v3 work starts
from the v2.1 source export on branch `baseline-v2.1` (associated with v2.1
commit `3765907aa2e424997bfd8f2fcdcc95275e6142f5`). The v2.1 lifecycle is
superseded by this v3 beta; its evidence remains historical and scoped to its
own SHAs:
[`evidence/final_integration_candidate_6967c378.md`](evidence/final_integration_candidate_6967c378.md),
[`evidence/local_beta_merge.md`](evidence/local_beta_merge.md),
[`evidence/beta_candidate_0f72e865.md`](evidence/beta_candidate_0f72e865.md) and
[`release_verification_2_1.md`](release_verification_2_1.md). None of it is
evidence for the v3 tree.

No push, tag, publication, license selection, registry ownership, human result,
Windows execution or pinned/MSRV toolchain execution may be inferred from this
status.
