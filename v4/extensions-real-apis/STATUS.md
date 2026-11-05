# Concord 4.4 extensions: status

"4.4 is the requested review label; the package/release version remains a separate decision."
Crate versions stay `4.2.0`; no tag, release or publication was made. The extension pull request
is a draft and is not merged.

## Identities

| What | Value |
|---|---|
| Repository | `gqsnt/concord_api` |
| 4.3 integration (PR #2) | merge commit `fd4771aee360fcad61f09a780829817f3ad474a1` (parents `5ed600b` main, `ae5618a` verified PR head) — **INTEGRATED_VERIFIED** (v4/dx-4.3/STATUS.md) |
| 4.3 receipt (records only) | `d1802547381ee604118024a6985513d1f90d54e7` on `main`; CI run [41956577954](https://github.com/gqsnt/concord_api/actions/runs/41956577954): Linux 37/37, native Windows 37/37, nightly `PASS nightly-clone-compat` 16/0 (read from the job logs) |
| Extension base | `d1802547` (verified main). The work started on `7eefdee` (a 4.3 PR commit, now an ancestor of main) while the integration was pending; `07e051d` merges that development history onto the receipt, so the branch's merge-base with `main` is `d1802547` |
| Extension branch | `concord-4.4-extensions-evidence` |
| History rewrite | before the first push, 98 unpublished commits were rewritten to remove a token-shaped synthetic test constant (HISTORY_REWRITE.md) |
| Toolchain | Rust 1.99.1 (pinned and minimum), edition 2024 |
| Final implementation commit | `fcf5573ad3a5e062fd336ab4e830ae897961d16f` (tree `4f0dd14`); local gate 43/43 and nightly PASS; CI in VERIFICATION.md §Final |
| Draft pull request | [#3](https://github.com/gqsnt/concord_api/pull/3), `concord-4.4-extensions-evidence` → `main`, not merged |

## Work packages

| WP | Deliverable | Status | Where |
|---|---|---|---|
| W0 | 4.3 CI diagnosis, correction, merge, receipt; extension base | done; INTEGRATED_VERIFIED | `v4/dx-4.3/` |
| W1 | `Csv<HasHeader, Delimiter>` finite + incremental read/write, profiles, runtime bridge, raw records; extension example migrated | done | `crates/concord-extras/src/csv/`, `examples/extension/` |
| W2 | JSON Lines / NDJSON finite + incremental, consecutive JSON values, response opening, lifecycle, uploads | done | `crates/concord-extras/src/{jsonl,stream}/` |
| W3 | `concord-pagination`: sequential, items with checkpoints, independent plans, first-page discovery, manifest jobs | done | `crates/concord-pagination/` |
| W4–W8 | Five SDK slices, ten workflows (offline verified; live NOT_RUN) | done | `examples/{github,stripe,shopify,salesforce,kubernetes}-{sdk,app}/` |
| W9 | fixtures/provenance, stress, maintenance exercises, independent caller tasks, test server, gates | done | FIXTURES.md, STRESS.md, USAGE_AND_MAINTENANCE.md, `examples/testserver/`, `checks/` |
| W10 | 4.4 review, defect fixes, readout, delivery | done: four review strands and two resolution re-checks (final Sol follow-up: ACCEPT); 22 findings fixed; readout compiled against the final code; delivery package (diff, bundle, ZIP) built outside the repository from the final branch state | NEXT_REVIEW_4_4.md, IMPLEMENTATION_READOUT.md, VERIFICATION.md |

## New packages

| Package | Kind | Tests (`cargo test -p`, at `d2681065`) |
|---|---|---|
| `concord-pagination` | maintained crate (packaged by `package-check`) | 69 |
| `concord-extras` features `stream`, `csv`, `jsonl` | maintained modules, facade features `concord/{stream,csv,jsonl}` | stream 5, csv 110, jsonl 73 (feature alone, gate steps `test-extras-{stream,csv,jsonl}`) |
| `concord-example-testserver` | test support | 7 |
| `concord-example-{github,stripe,shopify,salesforce,kubernetes}-sdk` | SDK slices | 46, 22, 18, 29, 24 |
| `concord-example-{github,stripe,shopify,salesforce,kubernetes}-app` | external callers, two workflows each | 21, 10, 21, 30, 27 |
| `concord-check-maintenance-{github,stripe,shopify,salesforce,kubernetes}` | staged maintenance consumers | 11, 7, 7, 7, 6 |
| `concord-check-dx03-csv-runtime`, `concord-check-dx03-fanout-sink` | independent caller tasks (other provider's models) | 8, 12 |
| `concord-check-stream-stress` | generated workloads (CI-sized smoke + N/2N binary; JSON Lines full jobs) | 10 |
| `checks/consumers/streaming` | packaged downstream consumer of `concord/{csv,jsonl}` and `concord-pagination` (own workspace; `scripts/consumers.sh`, path and `--packaged`) | 3 |

## Evidence tiers

Ten workflows OFFLINE_VERIFIED; LIVE_PROVIDER_VERIFIED NOT_RUN for all (no credentials; mutation
and job-creation lanes need explicit opt-ins); Kubernetes REAL_SERVER_VERIFIED NOT_RUN (no
disposable cluster tooling on the machine). See FIXTURES.md.
