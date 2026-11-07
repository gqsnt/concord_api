# Concord desirability loop — index

Owner instruction (2026-11-07): "finish → merge PR #5 → loop". No human in the loop: every
measurement is automatic (instruments I1–I5), every decision follows a fixed rule with an
independent review. Records:

| Record | What |
|---|---|
| `MERGE_RECEIPT.md` | PR #5 merge and `main` verification (first commit of this branch) |
| `BASELINE.md` | all metrics at the start, noise bands, instrument versions |
| `QUEUE.md` | hypothesis queue (re-ranked after each decision) |
| `cards/HNN.md` | pre-registered hypothesis cards (committed before implementation) |
| `impact/HNN.md` | 55-row impact tables |
| `measurements/HNN.md`, `scoreboard/<sha>.json` | measurements |
| `reviews/<task>.md` | independent review verdicts and triage |
| `DECISIONS.md` | one row per decision |
| `MODELS.md` | requested vs observed model and effort per session |
| `OWNER_QUEUE.md` | items that need the owner |
| `FINAL.md` | retrospective by concept (at the end) |

Instruments: `tools/desirability/` (I1 scoreboard, I3 probes, I4 edits, I5 diagnostics),
`checks/canary` + `examples/*-sdk/tests/canary.rs` (I2), `checks/probe-truth` (I3 ground truth).
They are tools, not gates: nothing is added to `scripts/verify.sh` (canary tests run as ordinary
SDK tests).

Plan amendments: `amendments/A1-astra-review.md` (the owner-adopted desirability review: task
outcomes primary, structural counts diagnostic, WIN/LOSS/INCONCLUSIVE/KEEP_CURRENT,
pilot-then-migrate) and `amendments/A2-owner-charter.md` (the owner's charter: the fixed core is
the DSL usage — client → scope → endpoint — on reqwest; everything else is challengeable). The
review itself: `review/`. Instrument reviews and their triage: `reviews/`.
