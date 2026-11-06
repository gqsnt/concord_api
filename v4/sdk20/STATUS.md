# Concord consolidation + 20 SDKs — campaign index

Handoff: `Concord_Consolidation_SDK20_Handoff/` (PROMPT, AUTHORITY, contracts/, cohort/). Live
provider tier: NOT_RUN (no credentials, no live writes; all workflows use local fixtures).

## Identities

| What | Commit | Tree | Note |
|---|---|---|---|
| Pinned review head (PR #3) | `bbadc60c` | `5a034acf` | handoff `BASELINE.json` |
| Selected base (`main`) | `a95fb2fb` | `5a034acf` | owner merged PR #3; tree identical to the pinned head, so no drift |
| Candidate branch | `concord-dx-sdk20` | | worktree `work/implementation` |

Baseline gate at `a95fb2fb` (detached worktree `work/baseline`, 2026-11-06):
`scripts/verify.sh --package --bench` 43/43 PASS (6m18s), `--nightly-only` PASS
(`nightly-clone-compat` 16 passed). Logs: `work/logs/baseline-a95fb2fb-*.txt` (outside the repo).

## Phase A

| Requirement | Implementation | Evidence | Status |
|---|---|---|---|
| A1 response boundary | `concord-core/src/response.rs` (`Received`, `Incoming`, `Operation`), `prepared.rs` (`receive`, `Decoder::execute/receive/transport_error`) | `checks/phase-a/tests/received.rs`; A1 comparison `v4/sdk20/a1-boundary/` + DECISIONS D-A1 | IMPLEMENTED |
| A2 runtime body/media | `Encode::encode_body`, `EncodedBody`, `__private::Parts::finish`; `concord-extras` `Encoded` | `checks/phase-a/tests/bodies.rs` (CSV runtime upload + generated multipart boundary) | IMPLEMENTED |
| A3 truthful incremental output | DSL `-> stream T` / `-> stream`; `Incremental<T>`; readers open `Openable`; `RecordStream`; `completion()` | `checks/phase-a/tests/streams.rs`; UI `ui_json/stream_outputs`, `ui/stream_keyword_paths` | IMPLEMENTED |
| A4 pagination facade / lossless reports | `concord-pagination/src/pages.rs`, `JobsReport::into_resume`, `IndependentReport::into_resume` | `crates/concord-pagination/tests/{pages,resume}.rs` | IMPLEMENTED |
| A5 Salesforce validated parts | `examples/salesforce-sdk/src/bulk_query.rs` `ResultPartReader`/`ResultPartRows::finish -> PartComplete`, used by `ResultSets`, `salesforce-app` and `checks/dx03-fanout-sink/src/stage2.rs` | `examples/salesforce-sdk/tests/result_parts.rs` (7), stage2 tests (8) | IMPLEMENTED |
| Migration of the five SDKs | GitHub/Stripe/Shopify metadata (`3abbbba9`), Kubernetes `-> stream WatchEvent<T>` (`9703f3c9`), Salesforce (`54685858`..`8efd185a`), multipart declared body (`7520fb8d`), dx03 runtime CSV; Stripe/GitHub pagination facade in progress | per-SDK tests | IN_PROGRESS |
| Independent packaged consumer | `checks/consumers/phase-a-dx` (Codex Terra), in `scripts/consumers.sh` | 9 tests; `REPORT.md`; findings FA-1..6 | DONE |
| Independent review | Codex Sol read-only on `a95fb2fb..4bcd8980` | ACCEPT_WITH_FIXES, SOL-A-1..3 fixed | DONE |
| Maintained gates, CI | | | IN_PROGRESS |

Cost check (A1): `concord-bench --bin alloc-count`, release, base `a95fb2fb` vs candidate
`d83b4782`: all 94 rows identical (prepare and decode stages, encode through `encode_body`).

## Build cache budget

Campaign-owned, reproducible build output only: `work/gate/target` (the one full-gate cache,
reused for every full gate), `work/implementation/target` (integration), `work/target-a` and
`work/target-b` (shared by the Phase B role sessions, at most two concurrent heavy builds), plus
the Phase A agent worktrees' own targets (about 17 GB, idle). At Phase B start: 372 GB free of
915 GB. Pruning, if needed, is limited to these directories and recorded here.

## SDK cohort (Phase B)

Not started. Roster: handoff `cohort/providers.json`.

## Model/session ledger

See `MODELS.md` (this directory).
