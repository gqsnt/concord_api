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
| A5 Salesforce validated parts | in progress (worker) | | IN_PROGRESS |
| Migration of the five SDKs | in progress (workers) | | IN_PROGRESS |
| Independent packaged consumer, review, gates, CI | | | NOT_STARTED |

Cost check (A1): `concord-bench --bin alloc-count`, release, base `a95fb2fb` vs candidate
`d83b4782`: all 94 rows identical (prepare and decode stages, encode through `encode_body`).

## SDK cohort (Phase B)

Not started. Roster: handoff `cohort/providers.json`.

## Model/session ledger

See `MODELS.md` (this directory).
