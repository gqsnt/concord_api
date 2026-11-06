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
| Maintained gates, CI | Gate A candidate `94d13527` (tree `d44edada`) | local `work/gate`: `verify.sh --package --bench` 43/43 PASS, clean; `--nightly-only` PASS (16 passed). CI [PR #4](https://github.com/gqsnt/concord_api/pull/4): push run `42141742060` on `94d13527` — `verify (ubuntu-latest)` success, `verify (windows-latest)` success with 43/43 `PASS` lines and 0 `FAIL` in the job log (`head: 94d13527… dirty: 0`, rustc 1.99.1, 01:08→01:40 UTC), `nightly-clone` success with step log `PASS nightly-clone-compat`, `16 passed, 0 failed` on nightly `1.101.0-nightly (2d20b42b8 2026-11-04)`; pull_request run `42141748868` on synthetic merge `6fa955a0` (parents `a95fb2fb`, `94d13527`; tree `d44edada` = candidate tree) — same three jobs success, Windows 43/43 PASS | **GATE_A_PASS** |

Cost check (A1): `concord-bench --bin alloc-count`, release, base `a95fb2fb` vs candidate
`d83b4782`: all 94 rows identical (prepare and decode stages, encode through `encode_body`).

## Build cache budget

Campaign-owned, reproducible build output only: `work/gate/target` (the one full-gate cache,
reused for every full gate), `work/implementation/target` (integration), `work/target-a` and
`work/target-b` (shared by the Phase B role sessions, at most two concurrent heavy builds), plus
the Phase A agent worktrees' own targets (about 17 GB, idle). At Phase B start: 372 GB free of
915 GB. Pruning, if needed, is limited to these directories and recorded here.

Disk exhaustion during wave 4 (2026-11-06): the root filesystem reached 100% (0 of 915 GB
free); builds failed with `No space left on device`, and one role session (SDK13 maintainer)
ended with exit 101 (resumed and finished afterwards). With the owner's answer "Yes, delete it
(option 1)", the coordinator deleted only campaign build output: the `target/` directories of 12
finished agent worktrees under `concord_api/.claude/worktrees/agent-*/` (~158 GB; the running
agent's kept) and the `consumers.*` scratch copies in `work/gate/target` and
`work/implementation/target` (~37 GB). No source, branch, commit or worktree was removed; the
`work/baseline/target/consumers.*` copies were not on the list and remain. Free afterwards:
200 GB.
Second approval (free space down to 87 GB before wave 5): "Yes, delete it (both agent
targets)" — `agent-ac998292…/target` (39 GB, R5 finished) deleted; `agent-ae98124f…/target`
(33 GB, R4) to be deleted after its follow-up session ends.

## SDK cohort (Phase B)

Wave 5 (held-out first-pass validation) frozen at `47a25fbf` (between-wave 4-5: R4 CSV quote at
EOF + TER-W45-1, R5 readers over a wrapped body, `consumers.sh` cleanup, docs; reviews Terra (CSV,
ACCEPT after recheck) and Sol (readers, ACCEPT)): `verify.sh --package --bench` 49/49 PASS, clean
tree, 4m36s (`work/logs/gate-47a25fbf-verify.txt`). Pushed to PR #4 at `e34c3c08` (records and
wave 5 capsules only after `47a25fbf`). Wave 5 capsules add a held-out rule: no campaign records
(`v4/sdk20/`) and no other SDK's FRICTION/FIRST_USE/MAINTENANCE/INTEGRATION notes. First-pass
results are kept separately from any later fix.

Wave 4 integrated at `0347edfc` (all 16 cohort SDKs; library `0983a349` plus the between-wave 4-5
CSV fix R4 and `consumers.sh` scratch cleanup): `verify.sh --package --bench` 49/49 PASS, clean
tree, 4m55s, no consumer scratch left behind (`work/logs/gate-0347edfc-verify.txt`). Synthesis:
DECISIONS "Wave 4" (W4-1..15).

Wave 4 frozen at `0983a349` (between-wave 3-4 refinements: `sse`, `multipart`, `ResponseMeta`,
`Error::meta`, `Content-Length: 0` at execution, test-server `Gate`; migrations M1/M2; reviews
SOL-W34 and AST-W34 fixed and rechecked): `verify.sh --package --bench` 49/49 PASS, clean tree,
4m00s (`work/logs/gate-0983a349-verify.txt`); pushed to PR #4. Earlier gate on `92775757` (merged
refinements before fixes) 49/49.

Wave 3 integrated at `961db5c8` (library still `6d6d66aa`): `verify.sh --package --bench`
43/43 PASS, clean tree (`work/logs/gate-961db5c8-verify.txt`, 3m37s). Synthesis: DECISIONS
"Wave 3" (W3-1..18).

Wave 3 frozen at `6d6d66aa` (waves 1-2 integrated; between-wave refinements D-W23-1/2, review
SOL-W23 fixed): `verify.sh --package --bench` 43/43 PASS on a clean tree
(`work/logs/gate-6d6d66aa-verify.txt`, 4m19s); pushed to PR #4. CI: pull_request run
`42162457510` on synthetic merge `0c0748fa` (parents `a95fb2fb`, `6d6d66aa`; tree `b858ffa2` =
candidate tree) — ubuntu success; windows success, 43 `PASS` / 0 `FAIL` lines, rustc 1.99.1;
nightly-clone success, step log `PASS nightly-clone-compat`, 16 passed, rustc
`1.101.0-nightly (249c1e4e3 2026-11-05)` (logs `work/logs/ci-6d6d66aa-pr-*.txt`). Push run
`42162457389` on `6d6d66aa` itself: all three jobs success; windows 43 `PASS` / 0 `FAIL`
(`work/logs/ci-6d6d66aa-push-windows.txt`). Earlier at `634b135b` the gate
failed 3 `test` steps (one UI case, W23-5), fixed in `08df6614` (43/43). The S3 move to the
value-form policy ran in parallel (SDK-only) and merged as `bbbf6c97`.

Wave 2 frozen at `e04236ca` (wave 1 integrated): `verify.sh --package --bench` 43/43 PASS on a
clean tree (`work/logs/gate-e04236ca-verify.txt`), the four wave 1 SDKs pass alone.

Roster: handoff `cohort/providers.json`; capsules `v4/sdk20/tasks/wave*/`. Wave 1 frozen at
`94d13527`; started while Gate A's remote CI was still pending; CI then passed on that exact
commit, so no baseline intervention was needed.

| ID | Provider | Size | Wave | Author (observed) | Consumer | Maintainer | Status |
|---|---|---|---|---|---|---|---|
| SDK01 | Nager.Holidays | small | 1 | `gpt-6.1-sol` xhigh (`38dbaa29`) | `gpt-6.1-terra` xhigh, 9 tests, unassisted (`9b99ef07`) | `gpt-6-astra` xhigh (`2e23393b`) | WAVE_1_DONE, integrated `6a9dcb19` |
| SDK02 | PokéAPI | small | 1 | `gpt-6.1-terra` xhigh (`2079f14b`) | `gpt-6-astra` xhigh, 14 tests; first intervention: read the testserver API (`729a8dda`) | Opus subagent (`5f72d206`; its `follow_with_limit` is reverted in integration, see W1-6) | WAVE_1_DONE, integrated |
| SDK03 | Twilio Messaging | medium | 1 | `gpt-6-astra` xhigh (`b1e00c12`) | Opus subagent, 22 tests, unassisted (`96ff9b2b`) | `gpt-6.1-sol` xhigh (`497367eb`) | WAVE_1_DONE, integrated |
| SDK04 | Amazon S3 | large | 1 | Opus subagent (`b30d9994`) | `gpt-6.1-sol` xhigh, 13 tests, unassisted (`715538c6`) | `gpt-6.1-terra` xhigh (`41e24beb`) | WAVE_1_DONE, integrated; value-form policy `bbbf6c97` |
| SDK05 | Frankfurter | small | 2 | `gpt-6-astra` xhigh (`d66eb228`) | Opus subagent, 24 tests, unassisted (`3aefaf5c`) | `gpt-6.1-sol` xhigh (`64a12a6e`) | WAVE_2_DONE, merged `ab4caefe` |
| SDK06 | GitLab | medium | 2 | Opus subagent (`debaefb7`) | `gpt-6.1-sol` xhigh, 18 tests, unassisted (`5d15bc2b`) | `gpt-6.1-terra` xhigh (`e1b813b5`) | WAVE_2_DONE, merged `8288e3e6` |
| SDK07 | Slack Web API | medium | 2 | `gpt-6.1-sol` xhigh (`ae8ddfb1`) | `gpt-6.1-terra` xhigh, 6 tests, unassisted (`31cad62d`) | `gpt-6-astra` xhigh (`c844a878`) | WAVE_2_DONE, merged |
| SDK08 | OpenAI | large | 2 | `gpt-6.1-terra` xhigh (`8a659204`) | `gpt-6-astra` xhigh, 18 tests, unassisted (`08f97864`) | Opus subagent (`51d0eee3`) | WAVE_2_DONE, merged |
| SDK09 | ntfy | small | 3 | Opus subagent (`f255d4df`) | `gpt-6.1-sol` xhigh, 12 tests, unassisted (`bd7861d1`) | `gpt-6.1-terra` xhigh (`92e32d43`) | WAVE_3_ROLES_DONE, merged `961db5c8` |
| SDK10 | Linear | medium | 3 | `gpt-6.1-terra` xhigh (`ed548bd5`) | `gpt-6-astra` xhigh, 18 tests, unassisted (`3ae13c1c`) | Opus subagent (`9466d1eb`) | WAVE_3_ROLES_DONE, merged `2c1d5295` |
| SDK11 | Dropbox | medium | 3 | `gpt-6-astra` xhigh (`69692433`; flaky test fixed by the same session) | Opus subagent, 18 tests, unassisted (`373362dc`) | `gpt-6.1-sol` xhigh (`72249e49`) | WAVE_3_ROLES_DONE, merged `0bc35457` |
| SDK12 | Google Drive | large | 3 | `gpt-6.1-sol` xhigh (`6d9d7fb9`) | `gpt-6.1-terra` xhigh, 5 tests, unassisted (`e8968c80`) | `gpt-6-astra` xhigh (`9bc413b0`, no production change needed) | WAVE_3_ROLES_DONE, merged `7354d5a3` |
| SDK13 | Open-Meteo | small | 4 | `gpt-6.1-sol` xhigh (`0d94ba92`) | `gpt-6.1-terra` xhigh, 8 tests, unassisted (`1f6ec3e0`) | `gpt-6-astra` xhigh (`7cd5e536`) | WAVE_4_ROLES_DONE, merged `fb3f519c` |
| SDK14 | Airtable | medium | 4 | `gpt-6.1-terra` xhigh (`e0425cc8`) | `gpt-6-astra` xhigh, 14 tests; first intervention: corrected a doubled base path (`173e4f01`) | Opus subagent (`a4163a07`) | WAVE_4_ROLES_DONE, merged `eb801635` |
| SDK15 | ClickHouse | medium | 4 | Opus subagent (`66f3cf00`) | `gpt-6.1-sol` xhigh, 17 tests, unassisted (`699239c2`) | `gpt-6.1-terra` xhigh (`2b584d4f`) | WAVE_4_ROLES_DONE, merged `0e0bc3df` |
| SDK16 | Snowflake | large | 4 | `gpt-6-astra` xhigh (`04f32a9a`; first delivery enabled reqwest `gzip`, which broke a Concord test under workspace feature unification; reworked by the same session to SDK-local flate2) | Opus subagent, 18 tests, unassisted (`19ef84e3`) | `gpt-6.1-sol` xhigh (`1a78be4f`) | WAVE_4_ROLES_DONE, merged `0347edfc` |
| SDK17 | World Bank | small | 5 | `gpt-6.1-terra` xhigh (`9b7f4e56`) | `gpt-6-astra` xhigh, 25 tests, unassisted (`3bb3d67c`) | Opus subagent (`9a3da827`) | WAVE_5_ROLES_DONE (first pass), merged |
| SDK18 | Notion | medium | 5 | `gpt-6.1-sol` xhigh (`7e13ff1a`) | `gpt-6.1-terra` xhigh, 6 tests; first intervention: read the test server's source (`e76187ff`) | `gpt-6-astra` xhigh (`b8d3c230`) | WAVE_5_ROLES_DONE (first pass), merged |
| SDK19 | Microsoft Graph | large | 5 | `gpt-6-astra` xhigh (`e80ef9e3`) | Opus (running) | `gpt-6.1-sol` | WAVE_5_CONSUMER |
| SDK20 | Docker Distribution Registry | large | 5 | Opus subagent (`bdea8b71`) | `gpt-6.1-sol` xhigh, 15 tests, unassisted (`e22a77be`) | `gpt-6.1-terra` xhigh (`a3c0b53a`) | WAVE_5_ROLES_DONE (first pass), merged `3479a367` |

## Open issues (not fixed; tracked)

| ID | Where | Issue | Status |
|---|---|---|---|
| W3-13 | `examples/linear-sdk` | a connection nulled by a GraphQL partial-data error becomes a decode error and loses the diagnostics, contrary to the README | FIXED `ca1d1ee0` (final migration): `Error::Page` keeping the diagnostics |
| W4-10 | `examples/snowflake-sdk` | a missing `partitionInfo` is a decode error, an empty one `InvalidManifest`, contrary to the README | FIXED `1e0685ba` (final migration): both `InvalidManifest` |
| W3-18 | `checks/stream-stress` | one timing-sensitive smoke test failed once under heavy parallel load | OBSERVED |
| W23-6/7 | Concord | a per-request input for the send policy cannot be declared; a decorated request cannot re-enter the policy | DEFERRED (one consumer, S3) |
| — | all SDKs | live provider tier | NOT_RUN (no credentials, no live calls) |

## Model/session ledger

See `MODELS.md` (this directory).
