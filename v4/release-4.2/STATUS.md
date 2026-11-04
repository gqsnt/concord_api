# Release 4.2.0 — status

A focused maintenance release on branch `concord-v4.2`, started from `main` at
`07dcdd03c61a1822341e92d3820e66117f1aef14` (records-only successor of `v4.1.0` =
`afcec400f7bdb044b0f9dd3c42c0575091f9e26f`; the only difference is `v4/release-4.1/STATUS.md`).
The public DSL, the four crates, codec contracts and policies, request types, `IntoValue` borrowing
and the Rust 1.99.1 minimum/pin are unchanged; the crates are at **4.2.0** with exact `=4.2.0`
internal requirements, `publish = false`. Released by the owner's authorization: merged into
`main`, pushed and tagged `v4.2.0` (see Release below). Not published on crates.io. Handoff: `Concord_V4_2_Opus_Implementation_Handoff/concord_v42_opus_handoff`
(its handoff check script, outside this repository: PASS, 184 files). Work directory: `/home/canarit/projects/concord-v42/`.

**State: RELEASED** as `v4.2.0` (merge `b2ba341` on `main`), after ACCEPTED_NATIVE_LINUX_AND_WINDOWS
for the code at `ed6703a29d6f798f13e06379d488feeac33410f4`: mandatory work and all six stage
dispositions complete, native Linux and Windows gates passed, independent reviews completed (final:
ACCEPT). Not published on crates.io. Final report: [FINAL.md](FINAL.md).

## Stages

| Stage | Disposition | Record |
|---|---|---|
| 1 Baseline | baseline gate on `07dcdd0`: 35/35 PASS, `verify-test` 651 passed ([log](evidence/baseline-gate-07dcdd0.log)); 18/18 recipes reproduced on the frozen 4.1 control; repaired-state replay 24/24 | [RESOLUTIONS.md](RESOLUTIONS.md) |
| 2 Compiler | F01–F04 resolved; four independent review rounds, all findings fixed and qualified | [RESOLUTIONS.md](RESOLUTIONS.md), [REVIEWS.md](REVIEWS.md) |
| 3 Tooling | M01, M02, M03, T01 resolved; M04 retired | [RESOLUTIONS.md](RESOLUTIONS.md) |
| 4 Docs and coverage | corrections and task notes; two documentation-first trials; D16/D22/I20 follow-ups; I22/P08 prepared, Windows below; original statuses unchanged | [RESOLUTIONS.md](RESOLUTIONS.md) |
| 5 Performance | O01 and O02: DEFER_WITH_EVIDENCE (lane + coordinator + independent review); nothing adopted | [PERF.md](PERF.md) |
| 6 Release | 4.2.0 versions, locks, changelog, migration notes; gate, review and Windows below | this file |

## Revisions (first parent of `concord-v4.2`)

| Commit | Kind | Change |
|---|---|---|
| `f0693b5` | code | F01–F04 compiler fixes and regressions |
| `c820bdb` | merge (tests) | T01 metadata stage, lifecycle (I20-A), D16-sol follow-ups (`4dd2adf`) |
| `4874405` | merge (benches) | M01 equivalence registry and S3 repair (`875f21a`) |
| `d17a675` | merge (docs) | Stage 4 documentation (`0aa5942`) |
| `7a2819a` | docs | follow-ups from the docs lane |
| `bd5d25e` | release | version 4.2.0 for the four crates |
| `aae6218` | code | review round 1 corrections |
| `c621b6c` | docs | documentation-trial findings |
| `e5082c4` | code | review round 2 corrections |
| `ea9b31a` | docs | changelog and migration notes (compiler part) |
| `0670240` | code | review round 3 correction |
| `de132dc` | merge (tooling) | M02, M03, M04, fresh extraction, gate wiring (`44193b6`) |
| `db0af48` | release | consumer lock 4.2.0, maintainers' notes, changelog maintenance |
| `1dad499` | code | review round 4 correction |
| `9c32294` | records | candidate records (frozen for the first final gate: 37/37 PASS; pushed for CI) |
| `c69b9b4` | code | review round 5 correction |
| `c2cf3a9` | code, tooling, records | final executable review R1–R3 |
| `033e08e` | code | review round 6 correction |
| `a6b78a1` | records | candidate records (second final gate: 37/37 PASS; pushed for CI) |
| `81b4ce2` | code, tooling | combined re-review C1, C2, T1 |
| `67006bb` | code | negative literal fragments grouped before a postfix operator (found by the coordinator rerunning the review's matrix) |
| `4619334` | records | candidate records (third final gate: 37/37 PASS; pushed for CI) |
| `c80bf40` | code, tooling | third re-review C3, T2 |
| `ed6703a` | records | candidate records (fourth final gate: 37/37 PASS; pushed for CI; final review ACCEPT) |
| receipt | records only | this receipt: final gate, CI and review results; `FINAL.md` |

## Gate inventory change

`scripts/verify.sh --package --bench` now has 37 steps (4.1: 35): added `tool-tests`,
`output-dirs`, `test-examples-isolated`; removed `test-multipart-alone` (covered). Packaged steps
write to `target/packaged/`. Pre-freeze run on `db0af48`: 36 PASS, 1 FAIL (`tool-tests`: the
changelog named the retired helper's file — corrected in `1dad499`); `verify-test` 676 passed.

## Final gate, final review, platforms

Every gate below is `scripts/verify.sh --package --bench` (37 steps) on rustc 1.99.1 (`58cf8f9c3`),
tracked tree clean (`dirty: 0`), except the nightly lane (`--nightly-only`, non-blocking, as in 4.1).

| Where | Source | Run / job | Result |
|---|---|---|---|
| Local Linux x86_64 (gate worktree) | `9c32294` | log SHA-256 `c54d914e…` | 37/37 PASS; 676 workspace tests; 126 isolated-example tests |
| Local Linux x86_64 | `a6b78a1` | `83761861…` | 37/37 PASS; 676 |
| Local Linux x86_64 | `4619334` | `cc893e96…` | 37/37 PASS; 676 |
| Local Linux x86_64 | **`ed6703a`** | [log](evidence/final-gate-ed6703a.log) `1e2ba226…` | **37/37 PASS; 676; 126** |
| CI ubuntu-latest | `9c32294` / `a6b78a1` / `4619334` | 41781326873 / 41782844691 / 41785255822 | success ×3 |
| CI windows-latest, native | `9c32294` / `a6b78a1` / `4619334` | jobs 126413304131 / 126418142558 / 126425808805 | success ×3 |
| CI ubuntu-latest | **`ed6703a`** | run 41786978999, job 126431257072 ([log](evidence/ci/run41786978999-job126431257072.log)) | **success: 37 PASS, 676, 126** |
| CI windows-latest, native | **`ed6703a`** | run 41786978999, job 126431257049 ([log](evidence/ci/run41786978999-job126431257049.log)) | **success: 37 PASS, 676, 126** |
| CI nightly lane | **`ed6703a`** | run 41786978999, job 126431256718 ([log](evidence/ci/run41786978999-job126431256718.log)) | success: `nightly-clone-compat` 16 passed |

The Windows job runs every step natively, including the new `tool-tests`, `output-dirs`,
`test-examples-isolated` and the `package-check` build from a deep non-ASCII extraction path (the
I22-A/P08-A follow-up); the workflow prints a step's detailed log only on failure, so the CI log
shows each step's PASS line. Full CI logs: [evidence/ci/](evidence/ci/) (SHA-256 in
`evidence/ci/full-logs.sha256`).

Final independent review: `rv-final4-sol` (fresh `gpt-6.1-sol`, `xhigh`) on `ed6703a` — ACCEPT for
the compiler/tooling scope, no new finding, no polish item; it inspected the central Linux gate and
noted native Windows as the remaining prerequisite, supplied by run 41786978999 above.

## Release

| Item | Value |
|---|---|
| Merge into `main` | `b2ba34199b4221906c49d2ed5eb60bd85dca3fcd`, non-fast-forward, parents `07dcdd0` (previous `main`) and `85c5002` (`concord-v4.2`); no conflicts; tree identical to `85c5002` |
| Local gate on the merge | `scripts/verify.sh --package --bench`: 37/37 PASS, 676 workspace tests, 126 isolated-example tests ([log](evidence/release-gate-b2ba341.log)) |
| CI on the merge (`main` push) | run 41842508773: `verify (ubuntu-latest)` success (job 126617477907, 37 PASS, 676), `verify (windows-latest)` success (job 126617477207, native, 37 PASS, 676), `nightly-clone` success (job 126617477747, 16) — [logs](evidence/ci/) |
| CI on the tag push (same commit) | run 41846133324: Linux, Windows and nightly success |
| Tag | `v4.2.0`, annotated (tag object `cf6743b210b1132ca9644ceba64c264f3fb73176`), pointing to `b2ba341`, created after run 41842508773 passed |
| Publication | none: `publish = false`, nothing uploaded to crates.io |

This section is a records-only commit on top of the tagged merge; the tag stays on `b2ba341`.
Branches and tags from earlier releases are unchanged.
