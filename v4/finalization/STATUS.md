# Campaign status — final V4

Branch `concord-v4-final` (worktree `/home/canarit/projects/concord_v4_final`), from `625ac49` on
`concord-v4-native-repr` (code `1d9ffe7`). Not pushed, not merged. Preserved unchanged:
`concord-v4-native-repr` (`625ac49`, and `6bc5032` on the remote), `concord-v4-refine`
(`a915c8d`), `concord-v4` (`8f7adc2`), V3 (`concord-v3`), the side branches of the native-repr
campaign. Work directory (snapshots, proposals, reviews, raw measurements):
`/home/canarit/projects/concord_v4_finish_work/`.

| Step | State | Where |
|---|---|---|
| Baseline reproduced | `verify.sh --msrv --nightly --package` 22/22 on `625ac49` | work dir `evidence/baseline/verify_625ac49.txt` |
| Review leads reproduced | regex migration corruption: CONFIRMED; dual-use guard lost when the header is removed: CONFIRMED; `TextHeader` unsealed: CONFIRMED | work dir `evidence/leads/LEADS.md` |
| Model roster verified | Opus, Sonnet (transcripts); Sol 6.1 medium/xhigh, Terra 6.1 xhigh (session `turn_context`) | `MODEL_RUNS.md` |
| Blind first-round proposals | 4 families + builder assessment, frozen | `MODEL_RUNS.md` |
| Executable candidates + control on the common corpus | statements (Sol Medium), nodes (Opus): both 36/2 = control | `MODEL_RUNS.md`, `DECISIONS.md` F01 |
| Blind cross-review | Terra, Sol XHigh, Sonnet | `MODEL_RUNS.md` |
| Native header names (F06–F09) | implemented, tested | `ee4560d`, `6c53c3f` |
| Representation statements (F01–F05) | implemented, tested | `33b1f0f`, migration merge `70f24a8` |
| Riot example (F11) | implemented, tested (16 offline tests) | `1c8aadb` |
| Docs, migration guide, script withdrawal (F10) | done | `c11c476` and later |
| Full verification of the final candidate | `verify.sh --msrv --nightly --package` on `81836ec` (clean): 21/22 PASS, 601 workspace tests; `records` failed only on the final record, which is written by the records commit after it (and then passes) | `/home/canarit/projects/concord_v4_finish_work/evidence/verify_81836ec.txt` |
| Performance (Criterion, compile, allocations) | done: baseline, candidate ×2, compile A/B/C; re-measured after the F09 amendments (`d3e26e4`, `d884461`) and on the final candidate (`81836ec`) | `PERF.md` |
| Fresh-use trials (docs first, ten tasks, author + caller) | Terra XHigh and Sonnet, both completed on `389392f`; every actual problem fixed or documented in `23c21d6`, `4e01aa4`, `663dbe9`, `3b5c5ed` | `trials/terra-xhigh.md`, `trials/sonnet.md`, `MODEL_RUNS.md` |
| Review-pack reconciliation | every D-* item dispositioned; bounded changes `0e56a65`, `e9ce41f`, `d884461` | `RECONCILIATION.md`, `FINDINGS.md` |
| Independent final reviews | Sol XHigh (executable) and Opus (semantic) on `ab3cb85`; re-reviews of `0f559d8`; confirmations of `5195cd1`; every finding resolved, documented or deferred with a trigger | `FINDINGS.md`, `MODEL_RUNS.md` |
| Final record | `81836ec` (code) | `FINAL.md` |
| Windows | NOT_RUN for this campaign (no push authorized) | — |
