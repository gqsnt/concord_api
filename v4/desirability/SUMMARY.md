# Desirability loop (2026-11-07 → 2026-11-09): summary

The loop ran on branch `concord-desirability` (PR #6). It proposed hypotheses about Concord's
surface, measured each with model readers and editors (instruments I1–I5), decided with a frozen
rule and an independent Codex `gpt-6.1-sol` review, and stopped by its own Stop 1 rule. Every
record, card, review, measurement, impact table, scoreboard and instrument is on the branch
[`archive/desirability-loop-1`](https://github.com/gqsnt/concord_api/tree/archive/desirability-loop-1)
(`v4/desirability/`, `tools/desirability/`, `checks/probe-truth/`). Its own retrospective is
[`v4/desirability/FINAL.md`](https://github.com/gqsnt/concord_api/blob/archive/desirability-loop-1/v4/desirability/FINAL.md)
there.

## What landed in `main`

| ID | Change | Evidence (one line) |
|---|---|---|
| canary | `checks/canary` and an `examples/*/tests/canary.rs` in all 55 SDKs: secrets planted in every input never appear in `Debug`, `Display` or error chains | found and fixed 7 leaks, all in SDK-owned types |
| T2 | test server accept loop and chunk accounting; flaky PayPal and transport tests on the paused clock or a held socket | fail before / pass after; lifecycle suite 200/200 |
| T3 sweep | 64 bind-then-drop "refused" sites hold a never-listening socket; ~41 timeout tests and 12 stalled bodies on the paused clock | load reproductions 2–10/40 → 200/200 |
| T3 cancel | a dropped `send()` stops polling its streamed upload body (`concord-core` guard) | before: 262,144 polls / 32 MiB after the drop |
| T4 | QuickBooks sends to the current connection's environment after `set_connection` | `tests/environment.rs` fails before, passes after (both directions) |
| T5 | 33 send-result statements in 25 files corrected against the source | each old statement false against the generated signature |
| H08 | opt-in `operations Name;` enum and `concord::SendOperation<T, Name>`; 8 SDKs adopt it | 6/6 seeded omissions become an E0004 build error naming the endpoint |
| H04 / H04r | generated docs state the contract first; base-URL wording narrowed back to the baseline's | pilot witness +6; holdout found the "whole base, including its path" wording misled readers |
| H16s | generated docs state query order, path-hole encoding and header provenance; 76 SDK-local notes | pilot witness +25 pooled; docs only |
| H09c | every public `Resume`-taking SDK method states its import path and constructors | resume 62/63 → 70 of 70; docs only |
| tooling | CI dependency cache; `http-body` graph allowance; the four scoreboard scripts in `tools/glue/` | — |

## What was removed before the merge

- **`controls { … }` (H06, H06c)**, reverted by the owner (2026-11-09): a statement listing
  generated controls to withhold is a negation statement about Concord, not a fact about the
  provider (charter rule C1). The 11 adopting SDKs are back to their pre-H06 surface, except T4.
  The invariant breaks it closed are open again and recorded in
  [`v4/next/inputs/CONSTRUCTION_PROBLEM.md`](../next/inputs/CONSTRUCTION_PROBLEM.md).
- The loop's instruments, probes, pilots, edit banks and records (≈ 64k lines of tooling, 117
  record files): on the archive branch.

## What was rejected or left open

- H03 exact inputs: REJECTED by its kill condition (borrowed `&str` into `String` aliases and
  generic `IntoValue<u16>` forwarders stop compiling). H03b: KEEP_CURRENT (+18.6 points against a
  frozen +20).
- H16, H16r: INCONCLUSIVE (instrument validity); H16s superseded them.
- Not carded for lack of a measured task deficit: provider error documents (H02b), declarative
  redaction (H05), scope representation (H06b), facade residue (H07), recovery after a failed
  continuation (H09b), single-attempt transport (H10), wrapper adoption (H11). H15
  (observability) parked.

## Scoreboard (static heuristics, baseline `734991e9` → loop end `f178e63d`)

| Measure | Baseline | Loop end |
|---|---|---|
| Facade forwarders / wrapper structs | 98 / 22 | 98 / 22 |
| Hand-maintained redaction impls | 192 | 192 |
| `error.rs` lines | 11,102 | 11,123 |
| DSL lines per endpoint | 6.55 | 6.63 |
| Generated rustdoc words per endpoint | 543.8 | 888.7 |
| Version literals in endpoint paths | 18 | 18 |

## Lessons

- Model readers at ceiling cannot see the largest problems: errors, redaction and facades were
  never tried because no reader task failed on them. Glue in SDK code is the better measure.
- Measuring cost more than changing: ≈ 64k lines of instruments against ≈ 4k lines of Concord
  change, with up to 12 review rounds on one instrument.
- Docs grew 63% to satisfy readers; documentation is not a fix for a design.
- A list of things to turn off (`controls`) solved a real problem with the wrong shape.
- Kill conditions written before the measurement led to rescue hypotheses; trade-offs are better
  shown to the owner than decided by a threshold.
