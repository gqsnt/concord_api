# Desirability loop — baseline (L0.7)

Measured commit **B = `734991e9`** (`concord-desirability`: `main` 04ac9902 + PR #5 merge receipt,
the instruments, records, T1). Every number below was produced by an instrument on B's tree
(throwaway checkouts `work/wt/base`, `work/wt/gate`); raw outputs live in the loop workspace
(`work/baseline/`), the sealed banks outside the repository. Model readers and editors are
models, not people: task outcomes are evidence about the contract's predictability and
editability, not about human preference or adoption, which stay unmeasured.

## Gates on B

| Gate | Result |
|---|---|
| `scripts/verify.sh --package --bench` (local, warm) | **66/66 PASS**, 7m23s (`test-examples-isolated`: 2727 passed, 0 failed) |
| CI `ci.yml` on `734991e9` (run 42447568341) | **green**: verify ubuntu-latest, verify windows-latest, nightly-clone |

## Instrument versions

| Instrument | Version at B | Acceptance |
|---|---|---|
| I1 scoreboard | `tools/desirability/scoreboard.py` "I1 scoreboard v1" + `rustdoc_metrics.py` (public view, reachability) | no open blocking finding since review 2; reachability review 13 |
| I2 canary | `checks/canary` + 55 `examples/*-sdk/tests/canary.rs` | review 2 |
| I3 reader probes | `tools/desirability/probes/` (truth format 3; R2 judged by rustc, invariant marker, alias preconditions) | review 12 (rounds 1–8), follow-ups review 13 |
| I4 edit tasks | `tools/desirability/edits/` (sealed bank `e7d963b9…`, 40 tasks) | review 11 (rounds 1–5), follow-ups review 13 |
| I5 diagnostics | `tools/desirability/diagnostics/` | review 2 |

Toolchains: rustc 1.99.1 (gate, truth, scoring); nightly `rustc 1.101.0-nightly (e9bbadf68
2026-10-12)` for rustdoc JSON (format 61).

## I1 scoreboard (diagnostics; `scoreboard/734991e9.json`)

| ID | Metric | B | Seed (`c1b75b9`) | Note |
|---|---|---|---|---|
| D1a | Forwarders | 98 in 32 SDKs | 98 in 32 | equal |
| D1b | Client wrappers | 22 in 18 SDKs | ~22 | |
| D1c | Generated share of the public request-making surface | 0.727 | (L0) | public rustdoc view; 13 SDKs expose no generated endpoint publicly (crate-private clients: airtable, backblaze-b2-native, clickhouse, distribution-registry, dropbox, google-drive, mastodon, microsoft-graph, open-meteo, slack, snowflake, twilio, xero-accounting), so only their forwarders count; 0 unreachable generated endpoint fns |
| D2a | Version paths / shared heads / shared keys | 18 / 5 / 5 | 18 / 5 / – | |
| D3 | Layer code: error.rs / policies / forwarders / redaction / result wrappers (lines) | 11,102 / 1,423 / 2,285 / 1,512 / 143 | ≈11.1k / ≈1.4k / – | total 16,065 |
| S2 | Hand-maintained redaction impls | 192 in 44 SDKs | ~164 of 357 | I1 counts every manual `Debug`/`Display` impl that withholds or replaces a field (361 manual `Debug`, 314 manual `Display` impls in total); the seed's script classified more narrowly |
| S3 | Unmarked credential-like values (candidates before triage) | 1: `clickhouse-sdk:quota_key` | 0 (2 false positives) | triage: a ClickHouse quota grouping key ("any string"), not an authentication secret → false positive; S3 = 0 after triage (reviewer to confirm) |
| R4a | DSL lines per endpoint | 6.554 (2,071 / 316) | 2,071 / 316 | equal |
| R4b | Declaration doc lines on Concord mechanics | 73 | 74 | |
| R4c | Generated rustdoc words per endpoint | 543.8 | (L0) | public view |

Inventory: 55 SDKs, 63 clients, 91 scopes, 316 endpoints; 130 sensitive values; 41 SDKs declare a
`send` policy (42 policy impls).

## I2 canary (S1, S1+)

All 55 SDK canary suites pass on B inside the gate's `test-examples-isolated`: no tested canary
leaked on the enumerated surfaces and scenarios (Concord-owned and SDK-owned views; transform-aware
wire controls), S1 = 0, S1+ = 100% (each canary present on the wire in the form the SDK
transmits). Seven SDK leaks found while building I2 were fixed before B (listed in
`MERGE_RECEIPT.md` / the L0 commit). This is a claim about the tested canaries and scenarios, not
a universal proof.

## I3 reader probes (main bank, seed 1; 32 eligible SDKs)

Readers: Claude `claude -p` (requested `claude-opus-5-5`, observed `claude-opus-5-5` in 128/128
sessions, effort not visible) and Codex (requested and observed `gpt-6.1-sol` / `xhigh` /
`read-only` in 128/128 sessions). Fresh sessions, no tools, no repository; 0 tool calls, 0
retries, 0 malformed answers. Two runs per provider on the same bank.

| Reader | Run | R1.critical (88) | R1.exact (88) | R1.query | R1.accept | R2.send_returns (86) | R2.error_type (86) | R2.base_url (19) | R3.pass (88) |
|---|---|---|---|---|---|---|---|---|---|
| Claude | 1 | 0.92 | 0.86 | 0.93 | 0.95 | 1.00 | 1.00 | 1.00 | 1.00 |
| Claude | 2 | 0.92 | 0.86 | 0.93 | 0.95 | 1.00 | 1.00 | 1.00 | 1.00 |
| Codex | 1 | 1.00 | 0.94 | 1.00 | 0.95 | 1.00 | 1.00 | 1.00 | 1.00 |
| Codex | 2 | 0.95 | 0.90 | 0.97 | 0.95 | 1.00 | 1.00 | 1.00 | 1.00 |

**Noise bands** (larger of the two-run spread and 2 pts; DESIRABILITY § I3): Claude — every
metric 2 pts (identical runs); Codex — R1.critical 5 pts, R1.exact 4 pts, R1.query 3 pts, others
2 pts.

**Ceiling.** R2 and R3 are 1.00 for both providers in both runs: with the rustdoc in the
materials R2 is largely a lookup (the leak scan finds the exact truth type in 82 of 86 items),
and docs-assisted call writing succeeds everywhere. They cannot show an improvement on this bank;
a hypothesis that targets them needs its own task bank (H04 has one).

**What readers get wrong (R1).** Claude, both runs: (a) *query order* — Kubernetes w1/w2/w6 and
MediaWiki w1–w3: an endpoint's fixed query pair (`watch=1`, `list=categorymembers`) is sent after
the caller's values, readers place it first; (b) *path-value encoding* — Nager w1: the path value
`x&y=z` is sent as `x%26y%3Dz`, the reader wrote it raw. Codex run 2 repeats some of (a). These
are candidate problems for the queue (composition order and encoding are documented rules that
readers do not predict), not decisions.

Truth: regenerated on B (`truth_format` 3; the committed truth predated the diagnostic
identity format: only `aliases` and the R2 diagnostic canonical forms changed, no truth value).
Drops: Zendesk's subdomain client has no local operation truth (known limit).

## I4 edit tasks (8 control SDKs × 5 kinds; Codex only, owner decision O6)

Bank `e7d963b9…` re-validated on B: 40/40 tasks kept (oracle fails before the reference, the whole
package suite passes after, 96/96 oracle tests executed and `ok`, harness control caught 40/40).

One fresh Codex agent per task on B (requested `gpt-6.1-sol` / `xhigh` / `danger-full-access`
inside bubblewrap; observed the same in 40/40 rollouts), isolation check passed before every
launch, measurement outside the agent's reach. Agent time: median 120 s (82–287 s); 47 min wall,
two at a time.

| Kind | Tasks | Valid | Oracle pass | All facts at a recognised site | Missed propagation (tasks) |
|---|---|---|---|---|---|
| e1 version/prefix | 8 | 8 | 7 | 8 | 6 (github, kraken-spot-rest, kubernetes, nager-holidays, notion, sentry) |
| e2 shared header | 8 | 8 | 8 | 6 | 2 (kraken-spot-rest, notion) |
| e3 optional list query | 8 | 8 | 8 | 8 | 0 |
| e4 sibling + classification | 8 | 8 | 8 | 7 | 0 |
| e5 document field | 8 | 8 | 8 | 8 | 0 |
| **all** | 40 | 40 | 39 | 37 | 8 |

- The one oracle failure is Kubernetes e1 (version/prefix): 1 of 4 oracle tests passed (the
  change reached some but not all request paths of the moved API group).
- e4 classification: 7/7 applicable tasks classified at the reference site with the oracle
  passing (Nager e4 has no classification); no omission occurred, so no detection stage is
  observed at baseline.
- *Missed propagation* (the old fact left in code where the change should also apply, while the
  oracle passes) concentrates on **version/prefix changes (6 of 8)**: the agent changed the place
  the task named and left the old literal elsewhere — the repeated-fact pattern D2a counts (18
  version paths). This is the strongest baseline signal for the scope/prefix hypotheses.
- One run per task (L0.6); edit-site counts and hunks are diagnostics in the raw records.

## I5 diagnostics (R5)

| Bank | Cases | Pass | R5 | Span in user code | Internal-free |
|---|---|---|---|---|---|
| UI (`crates/concord/tests/ui`, 102 `.stderr`) | 102 | 93 | 0.912 | 102 | 93 |
| Caller bank (5 SDKs × 6 seeded mistakes, recompiled on B) | 30 | 25 | 0.833 | 30 | 25 |

Caller bank: "names the expected type" 21/30; the 5 failures are the `default_body` cases
(`Default::default()` for a body whose generated argument type cannot be inferred): the first
error names `IntoValue`, `__macro` and `concord_core::` paths. The caller bank's controls compile
(5/5), no case dropped.

UI failures (9): internal names in the first error — `ApiConfig`, `ApiItem`, `CItem` (generated
names the user never wrote) and `__macro` / `__private` / `concord_core::` paths (sealed-trait
cases `native_key_sealed`, `to_header_value_sealed`, …).

## Known limits

- Readers and editors are models; repetitions of one model are correlated, not independent
  participants; a two-run range is not a confidence interval.
- I3 measures docs-assisted prediction, with a ceiling on R2/R3 (above); 23 SDKs are not eligible.
- I4 runs once per task (L0.6); Claude editors not run (owner: Codex only for I4).
- Visual documentation layout, human preference and adoption are unmeasured.
- Instrument limits are listed in each README (`tools/desirability/*/README.md`).
