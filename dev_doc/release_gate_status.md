# Concord V3 release-gate status

<!-- concord-release-status-owner -->

- **Canonical lifecycle status:** Concord V3 integration candidate / beta; unpublished source branch.
  The corrected implementation has completed the source-bound Linux and Windows
  machine gates below. Final delivery qualification remains tied to its own receipt.
- **Publication:** disabled — the source beta is unpublished.
- **Workspace version:** `3.0.0-beta.1`; every workspace package has `publish = false`.
- **Recorded:** 2026-10-17, using public source
  `2bba4e3797ece2f8254efd84ef31640605b65ff8` and the exact earlier qualification inputs below.
  A later delivery commit must be identified by its generated final receipt.
- **Supported platforms:** Windows and Linux are supported and mandatory.
  macOS is not currently a supported or release-gated platform.
- **Version scope:** V3 only, selected by the owner;
  [ADR 0027](adrs/0027-v3-only-product.md) retires conversion and older-binary obligations.

This is the sole authoritative lifecycle record. The
[final report](reviews/2026_10_17_closure/CONCORD_V3_FINAL_REPORT.md),
[native qualification report](reviews/2026_10_17_closure/NATIVE_QUALIFICATION_REPORT.md) and
[performance report](reviews/2026_10_17_closure/PERFORMANCE_REPORT.md) explain the completed
work and its evidence boundaries. The [16 October correction report](reviews/2026_10_16_corrections.md)
and earlier dated records retain their original source scope.

## Current correction candidate

The seven retained code findings have direct corrections and discriminating
native regression evidence. V3R-01 is closed by retiring the unsupported
converter, not by certifying a conversion algorithm. The current product has
one V3 grammar, runtime and state schema; its former transition tests and
compatibility paths are not part of the supported surface. See the
[V3-only report](reviews/2026_10_17_closure/V3_ONLY_REPORT.md).

| Gate | Current source-bound state |
|---|---|
| Targeted corrections and integrated runtime tests | Passed on the recorded correction inputs; historical failed attempts and their repairs remain in the native report |
| Linux maintained implementation gate and analyzer | **Pass** on `579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31`, [qualification run 38981477842](https://github.com/gqsnt/concord_api/actions/runs/38981477842/job/117243098047); full gate finished 16 October at 23:48:11 UTC, analyzer at 23:49:52 UTC, followed by source/lifecycle audit and artifact upload |
| Windows maintained implementation gate and analyzer | **Pass** on the same `579e8ea` source, [CI #29 job](https://github.com/gqsnt/concord_api/actions/runs/38981477798/job/117243098210); full gate finished 17 October at 00:23:52 UTC, analyzer at 00:27:33 UTC, job at 00:29:48 UTC |
| Earlier complete Windows control | **Pass** on `5b3772236d7c1b71f86ebeba35f7d33a30cf3b64`, [CI #28 job](https://github.com/gqsnt/concord_api/actions/runs/38978327078/job/117233004279); a separate source-bound result |
| Performance V3-O07 | Paired runtime/scale campaign and bounded runtime/compiler attribution completed; retained costs and interpretation are in the [performance report](reviews/2026_10_17_closure/PERFORMANCE_REPORT.md) |
| Source/evidence return | Native qualification and historical CI archives are retained; the final public source export, integrated checks and archive integrity require the delivery commit's receipt. The private acceptance annex is delivered separately; completion of one combined archive containing it is not claimed. |
| Human study and independent human Rustdoc review | **Unknown**; V3 protocols and implementer review are separate from participant or independent-human results |
| Main integration / stable publication | Separate owner actions; not performed by this correction |
| Dependency license policy | Three package-specific BSD-3-Clause exceptions authorized on 17 October 2026; the updated candidate must pass all ten CI jobs, including dependency policy, with results bound to its own receipt |

The complete `579e8ea` qualification includes the maintained feature, isolated
package, compiler, fixture, documentation, downstream/package and source-export
checks. Its ordinary [CI #29](https://github.com/gqsnt/concord_api/actions/runs/38981477798)
is nevertheless **failure**: nine jobs succeeded and the all-features dependency
license-policy job failed. A green implementation gate does not turn that
workflow into a green CI result.

Later changes strengthen the inspection mutation assertions, capture two required
body-arity diagnostics, reject ignored contract-CLI options and complete public
documentation. Their final integration must be checked on its own source. The
[final-return workflow](../.github/workflows/v3-final-return.yml) generates
`FINAL_RETURN_RECEIPT.json` in its artifact; this document does not assert that a
future run has passed. The receipt, source archive and final run identify the
delivered commit and its exact checks.

The earlier R7 statement that Edition 1 was not supplied describes that
commission. Later authorized authority review resolved the missing-input
uncertainty for this correction; it does not erase the earlier limit or imply
that private source material belongs in a public archive. The public package
contains repository evidence; the separate private authority annex is not
published here.

**Beta integration decision:** ordinary `concord-v3` source update authorized; main integration remains a separate owner action.
**Stable publication decision:** **Blocked — owner authority required** for the separate release decisions below.

## Historical R8 integration state

All four R8 findings are CONFIRMED_FIXED on native discriminating paths. New
Windows native and Linux/WSL qualification passes `just verify-implementation`
with pin 1.97.1 and MSRV 1.86.0. Windows package/workflow/default deny and both
analyzers pass. All-feature deny on both platforms retains POL-01 (exit 4,
BSD-3-Clause rejection for alloc-no-stdlib/alloc-stdlib/brotli); no license policy
was changed. [R8 validation](reviews/r8_validation.md) and
[handoff](reviews/r8_handoff.md) bind the new source/evidence. Later Markdown/JSON
changes retain the exact compiled inputs; final SHA/tree, ordinary branch push,
remote verification and CI are in the delivered post-commit receipt.

The R8 commission authorizes ordinary source-branch push to concord-v3 only.
Main/user/foreign work is preserved. Publication remains disabled; beta
integration, legal authority, namespace, broader V3-O07 performance, external
Edition 1, human review and stable publication decisions remain separate and open.

## Historical R7 integration state

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

## Historical R7/R8 validation environment and limits

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
Final documentation and exact-commit closure results must be read from the generated
final-return receipt for that delivery; full implementation qualification belongs
to the source-bound candidate above.

## Stable-publication gates

| Gate | State |
|---|---|
| Gate 0 — legal authority, package namespace and official publication metadata | **Blocked — owner authority required** |
| Supported-platform machine qualification | See source-bound matrix above; machine success alone does not authorize integration or publication. |
| Unfamiliar-human comprehension and independent human review | **Unknown** — no qualifying participant/reviewer evidence supplied. |
| Stable compatibility freeze, version, tags and registry publication | **Blocked** — reserved owner decisions in [the release packet](owner_release_decisions.md). |

Default-feature dependency license-policy success ignores private unpublished
workspace packages as configured; it does not supply the project's license or
registry authority. CI #25 through #33 retain their recorded BSD-3-Clause
rejections for `alloc-no-stdlib`, `alloc-stdlib` and `brotli`.

On 17 October 2026 the owner authorized correcting that dependency policy.
[deny.toml](../deny.toml) records explicit exceptions for those three packages;
the `brotli` exception also permits MIT to cover its `BSD-3-Clause AND MIT`
expression. BSD-3-Clause is not added to the global allowlist. The updated
candidate requires a successful CI run with all ten jobs successful and a native
dependency-policy summary with all four checks successful. Its result belongs
to the new source-bound receipt; no future PASS is asserted here. The project's
own license and `publish = false` remain unchanged. Locked fixture fetching and
explicit pinned Rust sources corrected the earlier CI setup failures.

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
