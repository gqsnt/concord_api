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
| R4b | Declaration doc lines on Concord mechanics | 73 | 74 | the seed scanner also counted a commented-out declaration example in NASA NeoWs that the current scanner excludes (classification difference, verified by the baseline review) |
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
`read-only` in 128/128 sessions). Fresh sessions with the materials only; 0 tool calls observed,
0 retries, 0 malformed answers. Two runs per provider on the same bank. Isolation limit: tools
and repository access were not mechanically removed — Claude ran with `--tools ""`, Codex
`read-only` in an empty directory under the checkout (it receives Codex's standard instructions
and skill list, no project file); a session with any tool call would have been discarded and
re-run, and none occurred. The H04 runner adds a bubblewrap sandbox that hides the home and the
repository.

| Reader | Run | R1.critical (88) | R1.exact (88) | R1.query | R1.accept | R2.send_returns (86) | R2.error_type (86) | R2.base_url (19) | R3.pass (88) |
|---|---|---|---|---|---|---|---|---|---|
| Claude | 1 | 0.92 | 0.86 | 0.93 | 0.95 | 1.00 | 1.00 | 1.00 | 1.00 |
| Claude | 2 | 0.92 | 0.86 | 0.93 | 0.95 | 1.00 | 1.00 | 1.00 | 1.00 |
| Codex | 1 | 1.00 | 0.94 | 1.00 | 0.95 | 1.00 | 1.00 | 1.00 | 1.00 |
| Codex | 2 | 0.95 | 0.90 | 0.97 | 0.95 | 1.00 | 1.00 | 1.00 | 1.00 |

**Noise bands** (larger of the two-run spread and 2 pts; DESIRABILITY § I3), computed from item
counts and kept exact for comparisons: Claude — every metric 2 pts (identical runs); Codex —
R1.critical 4.55 pts (88/88 vs 84/88), R1.exact 4.55 pts (83/88 vs 79/88), R1.query 3.41 pts
(88/88 vs 85/88), every other metric 2 pts.

**Ceiling.** R2 and R3 are 1.00 for both providers in both runs: with the rustdoc in the
materials R2 is largely a lookup (the leak scan finds the exact truth type in 82 of 86 items),
and docs-assisted call writing succeeds everywhere. They cannot show an improvement on this bank;
a hypothesis that targets them needs its own task bank (H04 has one).

**What readers get wrong (R1).** *Critical* misses (method + destination + query): (a) *query
order* — Kubernetes w1/w2/w6 (Claude both runs, Codex run 2) and MediaWiki w1–w3 (Claude both
runs): an endpoint's fixed query pair (`watch=1`, `list=categorymembers`) is sent after the
caller's values, readers place it first; (b) *path-value encoding* — Nager w1 (Claude both runs,
Codex run 2): the path value `x&y=z` is sent as `x%26y%3Dz`, the reader wrote it raw. *Other
exact-match misses*, the same items for both providers in every run: header names and `Accept` on
ClickHouse w1 and OpenAI w1 (Claude run 2: Backblaze B2 w2 instead of OpenAI w1), header values and
`Accept` on FRED w4/w6 (e.g. the format sends `accept: text/xml`, readers predict
`application/xml`), and the body of HubSpot w1. Field rates (every run): method 1.00,
content_type 1.00, header_names 0.977, header_values 0.976 (82 items), body 0.989, accept 0.955;
destination 0.989 except Codex run 1 (1.00); query 0.932 (Claude), 1.00 / 0.966 (Codex). These are
candidate problems for the queue (composition order, encoding and the format-implied `Accept` are
documented rules that readers do not predict), not decisions.

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

> **Erratum (2026-11-09, `measurements/H12-kubernetes-e1-replay.md`):** the Kubernetes e1 oracle failure below was an
> instrument artifact — that run overlapped Kubernetes e2 on the shared measure target `work/targets/i4-measure`, and
> the oracle ran against base-tree artifacts. Replayed alone on a fresh private target, the agent's saved edit passes
> the oracle 4/4 and the whole suite with no missed propagation (reference 4/4; a no-op control reproduces the 1/4).
> Corrected: **oracle pass 40/40**, missed propagation **7** tasks (Kubernetes e1 removed). The table keeps the
> numbers as originally recorded. The other 39 runs were not replayed; shared-target contamination could also have
> produced a false pass, so their independence is not certified (instrument limitation).

- The one oracle failure is Kubernetes e1 (version/prefix): 1 of 4 oracle tests passed (the
  change reached some but not all request paths of the moved API group).
- e4 classification: 7/7 applicable tasks classified at the reference site with the oracle
  passing (Nager e4 has no classification); no omission occurred, so no detection stage is
  observed at baseline.
- *Missed-propagation flags* (an old fact still stated somewhere the change should also reach):
  8 tasks, 6 of them version/prefix (e1) and 2 shared header (e2). Seven of the eight are **test
  expectations** — an existing SDK test still names the old version, prefix, path or header
  (these tests still pass, so they do not check the changed request) —
  and one is **package documentation** (Kubernetes e1, which also fails its oracle). Behaviour
  checked by the oracles passed in 39/40 tasks. So the flags show that version/prefix facts are
  duplicated into SDK tests and docs that editors do not update; they do **not** show missed
  production request paths, and they do not directly measure the repeated endpoint literals D2a
  counts. A scope/prefix hypothesis (H12) may use them as supporting evidence only.
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
