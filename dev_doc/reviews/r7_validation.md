# Concord V3 R7 validation

Implementation candidate: `e19c7bf0a096d77713c0c8b413f0a7f678080245`.
The library/tool/test trees and root dependency lock retain that qualified identity.
Post-push CI follow-ups change only fixture dependencies and validation setup;
scoped repeat checks below qualify those inputs. The terminal receipt compares
both identities and records exact final-commit audits and export.
Native commands, exits, UTC times, tracked overlay digests, untracked test hashes
and log hashes are in [the evidence index](r7_evidence_index.json). Full logs remain
outside Git in `concord-r7-artifacts-20261012/logs` and accompany the returned handoff.

## Native BEFORE and AFTER

All defect BEFORE runs use import baseline
`16f61c0753b313b8c559a4a57dd3dfbe1d2e7345`, whose production tree matches R6.
Only test overlays differ, recorded per run. AFTER qualification includes these
same discriminating tests plus the expanded controls on the committed candidate.
Prefix the commands below with `cargo +1.97.1`; all use `--locked`.

| Finding | Native probe arguments | BEFORE receipt and exit | Observed BEFORE | AFTER |
|---|---|---|---|---|
| R7-01 | `test -p concord-core --lib --all-features r7_short` | `r7-01-before-native`, 101 | 4 acquisitions, 3 admissions, 0 handoffs for the short provider | Pass; 30/60/120s, no-provider, replacement/expiry and followers |
| R7-01 storage | `test -p concord --test r7_short_storage --all-features` | `r7-01-public-storage-before-tls`, 101 | TLS storage succeeded; protected request failed preflight | Pass |
| R7-02 | `test -p concord --test r7_qualified_paths --all-features` | `r7-02-qualified-value-before`, 101 | Compiled; authored parent 7 became inner 9 | Pass, plus `r7_relative_paths` type/value controls |
| R7-03 | `test -p concord-core --lib --all-features r7_decoder` | `r7-03-before-all-boundaries`, 101 | Record, EOF record/none and repeated consuming-none crossed lease | Pass, plus cancellation/idle/abandoned-next controls |
| R7-04 | `test -p concord --test r7_cooldown --all-features` | `r7-04-before`, 101 | Actual empty-plan endpoint sent during cooldown | Pass; sensitivity, origin/object isolation, expiry and invalid delays |
| R7-05 | `test -p concord-extras --lib --all-features r7_public` | `r7-05-before`, 101 | Capacity-one feedback for B was lost | Pass; extension, multi-key recovery/cancellation and bounded state |
| R7-06 | `test -p concord --test r7_ndjson --all-features` | `r7-06-before-complete`, 101 | Complete 1 MiB payload + LF sent; buffer and stream refused it | Pass; delimiter/chunk/EOF/excess/body/frame/record controls |
| R7-07 | `test -p concord-inspect --lib --all-features r7_duplicate` | `r7-07-before`, 101 | Changed duplicate owner compared equal | Pass; top/nested/composite/fallback keys, safe errors, CLI check |
| R7-08 | `test -p concord --test r7_oauth_delay --all-features` | `r7-08-before`, 101 | Real TLS Retry-After rejection made 2 immediate helper exchanges | Pass; 429/503 recovery, date/delta/invalid/cap/elapsed controls |

R7-02's direct `super::` example was explicitly rejected by R6 (C3-GENERATED).
That native compile rejection refutes that particular example; the qualified
runtime counterexample confirms the underlying defect. Initial probe compile
errors, the first incorrect TLS base and chunked fixture truncation are retained
as setup failures, not runtime proof. The complete-body R7-06 probe is independent
of the subsequently repaired Windows loopback socket mode.

## Maintained final qualification

| Gate | Receipt | Exit | Source |
|---|---|---:|---|
| Windows implementation/MSRV/export | `final-windows-implementation2` | 0 | `e19c7bf` |
| Linux implementation/MSRV/export | `final-linux-implementation3` | 0 | `e19c7bf` |
| Windows extracted-package rehearsal | `final-windows-package2` | 0 | `e19c7bf` |
| Windows workflow syntax | `final-windows-workflow2` | 0 | `e19c7bf` |
| Windows dependency policy | `final-windows-deny2` | 0 | `e19c7bf` |
| Linux dependency policy | `final-linux-deny` | 0 | `e19c7bf` |
| Windows analyzer | `final-windows-analyzer` | 0 | `e19c7bf` |
| Linux analyzer | `final-linux-analyzer` | 0 | `e19c7bf` |
| Linux standalone package/workflow tools | `pwsh` and `actionlint` absent from Linux PATH; both recipes passed on native Windows | Unavailable on Linux | Recorded tool inventory |

Windows: Windows 11 Professional, build 26200, x86_64 MSVC. Pin:
`rustc 1.97.1 (8bab26f4f 2026-07-14)`, LLVM 22.1.6. MSRV:
`rustc 1.86.0 (05f9846f8 2025-03-31)`, LLVM 19.1.7. Linux:
Ubuntu 26.04 in WSL2, kernel 6.18.40.1, x86_64 GNU; the same pin and MSRV.
Linux evidence is explicitly Linux/WSL evidence; the Windows gate ran natively.

`just verify-implementation` covers default/no-default/all-feature checks,
warning-denied Clippy and tests, explicit facade/core UI, MSRV libraries and
MSRV UI, isolated feature fixtures, docs/rustdoc/doc-audit, downstream clients,
xtask/migration/contract tooling, benchmark compilation and clean source export.
The MSRV recipe ran as a constituent. Windows also ran `just package`,
`just workflow`, `just deny`, and `just analyzer` separately. The packaged
rehearsal resolves newly generated extracted-only downstream locks, as designed;
it proves packaging mechanics rather than the workspace lockfile selection.
Linux also ran `just deny` and `just analyzer`; standalone Linux `pwsh` and
`actionlint` were absent. Analyzer exit 0 does not mean zero informational
diagnostics: the maintained scan reports inactive-code weak warnings and the
pre-existing dev-dependency cycle.

The first platform gates were deliberately interrupted for the Rustls lockfile
repair. Linux attempt 2 failed doctest linking with SIGBUS when shared `/tmp`
was 100% full. Attempt 3 uses only a task-owned disk TMPDIR and preserves all
foreign temporary files. The initial dependency audit failed RUSTSEC-2026-0285;
patched Rustls 0.23.45/WebPKI 0.103.15 passes with no ignored advisory.
UI refresh reviewed 14 facade and 2 core expectations under 1.97.1; rejection
obligations remain. No old failure or interrupted receipt was overwritten.
The initial handoff docs check also rejected missing mandatory lifecycle/policy
phrases; those truthful canonical phrases were restored and the check passed.

## Post-push CI follow-ups and reserved policy result

The first pushed commit `6764dfd` completed six CI jobs successfully. Fixture and
MSRV semantic checks failed with missing `zlib-rs`: their separate fixture lock
was not fetched before offline compilation. The UI job lacked Rust source excerpts
used by the reviewed pinned expectations. Commits `b71e19f` and `1950dca` add locked
fixture fetches and explicitly install `rust-src`; no offline or diagnostic check
was weakened. The fixture lock also moves Rustls 0.23.43 to patched 0.23.45.

Native scoped repeats pass: `ci-followup-windows` and `ci-followup-linux` run MSRV
and fixtures; `ci-followup-ui-windows` and `ci-followup-ui-linux` run the complete
UI matrix, with Windows workflow syntax. `ci-cold-fetch` followed by
`ci-cold-offline-metadata` resolves the entire fixture graph in a new task Cargo
home. `ci-fixture-deny2` passes the standalone default-feature dependency audit.
The full implementation matrices remain bound to `e19c7bf`; runtime Rust source
and the root lock have not changed. Exact receipt sources are in the evidence index.

The corrected CI run `38193487598` passed its fixture and MSRV stages but its
all-features dependency license check rejects BSD-3-Clause in `alloc-no-stdlib`,
`alloc-stdlib`, and `brotli`. `cargo deny --all-features check` reproduces this
natively (`ci-native-all-feature-deny`, exit 4): advisories, bans and sources pass;
licenses fail. The earlier `just deny` passes cover default features only. The
license allowlist is unchanged. This is a reserved owner policy/publication
decision, not a correctness-test waiver. Options are owner-approved narrow
dependency exceptions or a separately authorized feature/dependency change;
the beta review branch preserves the rejection and existing feature surface.

The Windows Git archive prototype converted unspecified text to CRLF. Final
export disables that conversion and checks every ZIP entry against committed
blob bytes. The incomplete prototype is retained separately and is not delivered.

## R7-S01 performance scope

Narrow disposition: **CONFIRMED_FIXED**. Final paired allocation probes compare
clean committed control `3b1e6c13e4e7bf14677e754ca07f745d52ed75c7` (candidate
with only S01 reversed) to `e19c7bf0a096d77713c0c8b413f0a7f678080245`.
Toolchain, dependency lock, features, inputs and host match; build directories
and worktrees are separate. Receipts: `s01-patched-{before,after}-{head,future,cold}`.

| Measurement | BEFORE | AFTER |
|---|---:|---:|
| Warm managed buffered execution, allocations/op | 64 | 63 |
| Warm allocation bytes/op | 26,298 | 26,274 |
| Cold first execution, allocations | 169 | 168 |
| Cold allocated bytes | 80,543 | 80,519 |
| Cold peak live bytes | 53,881 | 53,881 |
| Cold net allocations | 51 | 51 |
| Boxed operation future | 5,888 bytes | 5,888 bytes |

Warm timing medians were 60,972/67,376 ns; cold single samples 1,048,500/1,056,800
ns. Other qualification ran concurrently. No latency benefit or timing acceptance
is claimed. The selected driver controls construction, so accepted-bytes overrides
and wrappers retain their actual acquisition mode. The full gate retains stream
correlation, conversion-failure, handoff and cancellation tests.

The initial 500-endpoint debug fixture at `17e105c` overflowed the default native
Windows stack. Sequential statements alone also failed. Separate private endpoint
constructor frames fixed it. The earlier S01 pair (`38a18db`/`37ed019`) kept
178,095 expansion lines, 6,398,431 characters, 3,873,792 binary bytes and 2,914,068
description bytes on both sides. The AFTER scale run overlapped only UI expectation
file updates; compiler inputs were unchanged. Its timing is diagnostic, not an
acceptance result. `final-patched-scale` rechecks the patched final candidate;
exit 0, unchanged expansion/binary/description sizes; observed final scale row:

| Endpoints | Lines | Characters | Fragments | Declarations | Methods | Expand ms | Build ms | Edit ms | Binary bytes | Description bytes | Parent edit ms | Cold ms | Warm ms |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 500 | 178095 | 6398431 | 572256 | 33564 | 17037 | 1714 | 23923 | 26774 | 3873792 | 2914068 | 7444 | 65107 | 386 |

Compile/edit/cold/warm timing is not an S01 acceptance claim.

Broader V3-O07 performance, human comprehension, independent human review,
package namespace/legal authority, merge and stable publication remain unaccepted
or reserved. Edition 1 was absent and full external-spec conformance was not
independently re-established. CI and exact pushed SHA are in the post-commit
push receipt returned with the ZIP, avoiding a self-referential source commit.
