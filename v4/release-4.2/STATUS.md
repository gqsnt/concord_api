# Release 4.2.0 — status

A focused maintenance release on branch `concord-v4.2`, started from `main` at
`07dcdd03c61a1822341e92d3820e66117f1aef14` (records-only successor of `v4.1.0` =
`afcec400f7bdb044b0f9dd3c42c0575091f9e26f`; the only difference is `v4/release-4.1/STATUS.md`).
The public DSL, the four crates, codec contracts and policies, request types, `IntoValue` borrowing
and the Rust 1.99.1 minimum/pin are unchanged; the crates are at **4.2.0** with exact `=4.2.0`
internal requirements, `publish = false`. Nothing was pushed, merged into `main`, tagged or
published by this work, except the branch push for native Windows CI authorized by the owner after
Stage 6 (see below). Handoff: `Concord_V4_2_Opus_Implementation_Handoff/concord_v42_opus_handoff`
(its handoff check script, outside this repository: PASS, 184 files). Work directory: `/home/canarit/projects/concord-v42/`.

**State: IN_PROGRESS** until the final gate, the final executable review and native Windows CI are
recorded below (records-only receipt).

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

## Gate inventory change

`scripts/verify.sh --package --bench` now has 37 steps (4.1: 35): added `tool-tests`,
`output-dirs`, `test-examples-isolated`; removed `test-multipart-alone` (covered). Packaged steps
write to `target/packaged/`. Pre-freeze run on `db0af48`: 36 PASS, 1 FAIL (`tool-tests`: the
changelog named the retired helper's file — corrected in `1dad499`); `verify-test` 676 passed.

## Final gate, final review, platforms

(records-only receipt appended after the frozen-candidate runs)
