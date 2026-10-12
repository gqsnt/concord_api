# Concord v3 release-gate status

<!-- concord-release-status-owner -->

- **Canonical lifecycle status:** Concord v3 unpublished beta source branch, R7 machine qualification (integration candidate / beta lifecycle).
- **Workspace version:** `3.0.0-beta.1`; every workspace package has `publish = false`.
- **Publication:** disabled — all workspace packages remain unpublished.
- **Recorded:** 2026-10-12. Implementation candidate `e19c7bf0a096d77713c0c8b413f0a7f678080245`.
- **Supported platforms:** Windows and Linux are supported and mandatory. macOS is not currently a supported or release-gated platform.

**Beta integration decision:** OPEN — the owner retains merge/integration decisions;
this commission authorizes only the qualified source-branch push.

**Stable publication decision:** BLOCKED — owner legal/package authority and
stable-release decisions remain open.

This is the sole authoritative lifecycle record. [R7 validation](reviews/r7_validation.md)
and [handoff](reviews/r7_handoff.md) record source-specific commands and limits;
[the ledger](claim_evidence_ledger.md), README and FINAL summarize this record.
The final documentation commit preserves all compiled inputs. Its terminal
audit/export and pushed SHA belong to the separate post-commit receipt.

## Current integration state

| Item | State and evidence |
|---|---|
| Exact R6 import | **Done** — commit `16f61c0753b313b8c559a4a57dd3dfbe1d2e7345`, exact R6 tree `0b1688a9f1e22898328d7d28eeedf87d7d44e5e1`, 1,019 verified source hashes. Original claimed R6 commit/parent absent; [provenance](reviews/r7_import.json) records reconciliation. |
| R7-01 through R7-08 | **CONFIRMED_FIXED** on native discriminating paths with controls; R7-02's simple direct-parent example is refuted but a qualified path confirmed the defect. See [findings](reviews/r7_findings.md). |
| R5-01 migration follow-up and earlier migration/storage/idle fixes | **Retained and exercised** by the native migration, recording, fixture and composed runtime suites. No independent human reacceptance is inferred. |
| Native Windows, pin 1.97.1, MSRV 1.86.0 | **Pass, exit 0** — complete `just verify-implementation`, including MSRV; receipt `final-windows-implementation2`. |
| Linux/WSL, pin 1.97.1, MSRV 1.86.0 | **Pass, exit 0** — complete same gate with task TMPDIR; receipt `final-linux-implementation3`. |
| Packaged downstream, workflow, dependency policy and analyzer | **Pass, exit 0** — Windows package/workflow/deny/analyzer and Linux deny/analyzer; standalone Linux pwsh/actionlint unavailable. Exact scope in R7 validation. |
| R7-S01 | **Accepted in narrow allocation scope** — one allocation and 24 bytes saved in warm/cold buffered execution; boxed future unchanged. No latency benefit claimed. |
| V3-O07 broader performance | **Not accepted** — the bounded S01 result and 500-endpoint stack repair do not close the broader obligation. |
| Source branch push | **Authorized** by the R7 commission for `concord-v3`; exact final remote SHA and CI state are recorded after the commit in the delivered receipt. |
| Merge into main, tags, releases, stable version and publication | **Not performed or authorized** by this commission. |

Main remains `3765907aa2e424997bfd8f2fcdcc95275e6142f5`. Work is isolated in a
separate worktree; primary checkout and foreign/unknown work are preserved.
The R6 archive's prior no-push restriction describes an earlier commission and
is superseded only for this authorized ordinary source-branch push.

## Validation environment and limits

Windows 11 Professional build 26200, native x86_64 MSVC; Linux Ubuntu 26.04 in
WSL2, kernel 6.18.40.1, x86_64 GNU. Both use pin
`rustc 1.97.1 (8bab26f4f 2026-07-14)` and MSRV
`rustc 1.86.0 (05f9846f8 2025-03-31)`. Separate checkouts/targets retain BEFORE
and AFTER evidence. UI differences were inspected before refreshing affected
expectations under the pin; unchanged expectations are qualified by the matrix.

Fresh dependency policy identified RUSTSEC-2026-0285 in imported Rustls 0.23.42;
the final lock uses patched 0.23.45 and WebPKI 0.103.15 without policy exceptions.
Linux's shared full `/tmp` caused an intermediate doctest linker SIGBUS;
the recorded rerun uses a task-owned disk TMPDIR and preserves foreign files.
Failed, setup and interrupted attempts remain separate historical receipts.

The external Edition 1 specification was not supplied for R7; full external-spec
conformance was not independently re-established. No macOS, human comprehension,
independent human Rustdoc/diff review, package ownership or stable-release result
is inferred from these machine gates. CI is distinct from local qualification.

## External review of the `fc3e4d1` return

The 26 September source review retained the beta and reopened F01 storage
finalization (V3-E15/E21), F02 idle/read completion (V3-P10) and F03 migration
receiver identity (V3-O04). Their repairs and later R3/R4/R5 macro/member controls
are preserved in R6 and pass the R7 machine suites. Claims C68–C70 retain the
original evidence history; current native evidence is mapped separately in C74.
This does not fabricate independent human acceptance. R7 adds the decoder
completion boundary to the shared stream precheck contract.

## Repository audit commands

`just release-audit` runs docs checks, requires the preserved local main merge
base, a clean worktree, placeholder/clock/authority scans and the exact unpublished
beta lifecycle. `just source-export` archives and audits an exact clean commit.
Final documentation and exact-commit closure results are in the delivered terminal
receipt; full implementation qualification belongs to the candidate above.

## Stable-publication gates

| Gate | State |
|---|---|
| Gate 0 — legal authority, package namespace and official publication metadata | **Blocked — owner authority required** |
| Supported-platform machine qualification | See source-bound matrix above; machine success alone does not authorize integration or publication. |
| Unfamiliar-human comprehension and independent human review | **Unknown** — no qualifying participant/reviewer evidence supplied. |
| Stable compatibility freeze, version, tags and registry publication | **Blocked** — reserved owner decisions in [the release packet](owner_release_decisions.md). |

Dependency license-policy success ignores private unpublished workspace packages
as configured; it does not supply the project's license or registry authority.

## History

The unmodified R6 lifecycle file is permanently preserved in import commit
`16f61c0`; it reported only historical Linux/rustc 1.95 validation, unrun native
Windows/pin/MSRV and absent local-main history in the earlier isolated repository.
Those statements remain historical, not the current gate state. R6's claimed
integration commits `60d7de4`/`5c1c13c` were unavailable in this repository.
The [v3 documentation slice](evidence/v3_documentation_slice_observations.md) and
v2.1 records ([local beta merge](evidence/local_beta_merge.md),
[final candidate](evidence/final_integration_candidate_6967c378.md),
[verification](release_verification_2_1.md)) retain their original source scope.
The preserved R6 archive and all R7 receipts remain outside the source tree.
