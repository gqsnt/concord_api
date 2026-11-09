# H11 baseline — maintaining a wrapper error stage (task baseline before any card)

Purpose (queue triage 2026-11-09, owner "Measure H11/H09a first"): measure whether SDKs whose errors wrap
Concord's in their own diagnostic stage enum cost more to maintain than SDKs that expose `concord::Error`
directly, before H11 ("SDKs adopt what already exists": `Error::kind`, …) may be carded. No card, threshold or
candidate exists; this record states measurements and a proposed disposition.

Instrument: I4 edit tasks (`tools/desirability/edits/run.py`, bank-local public materials; isolated Codex agents
in their own bubblewrap; one deep isolation preflight, then the standard isolation probe before every launch). Sealed bank `work/sealed/h11-bank`
(`bf390e09…`), authored by a Codex lane (`h11-baseline-codex`), base `475a7867`. One seeded maintenance change
per SDK — a new inclusive diagnostic-body budget for non-2xx responses, a new public stage for a bounded-read
limit failure (wrappers) or the same budget with the existing Concord error (controls), README and consumer
assertions — scored by a hidden oracle, the SDK's existing suite, authoritative-site and propagation checks.
- Wrapper cohort (the four diagnostic stage enums SDK50 named): Frankfurter, Open-Meteo, REST Countries,
  Datadog Events. Bank census: **all four already classify by `concord::Error::kind()`** at this base (no
  non-kind reconstruction left; Frankfurter doubles as the kind()-classification control).
- Direct controls: FRED (typed diagnostics; the closer comparison), Europe PMC (opaque diagnostic bytes,
  reported separately).
- Coordinator runs: deep isolation preflight passed; reference replays (Frankfurter, FRED), the omission replay
  (caught by the existing tests) and the no-op-runner replay (invalid) behaved as declared; then 6 tasks × 3
  repetitions, Codex `gpt-6.1-sol` xhigh requested — observed `gpt-6.1-sol` xhigh in all 18 rollouts
  (`work/logs/h11-runs.log`, 84 min).

## Results (18 runs, all valid)

| SDK | role | oracle | existing suite | authoritative budget site | wrapper classification | patch size (files; +/− lines), r1 · r2 · r3 |
|---|---|---|---|---|---|---|
| Frankfurter | wrapper | 3/3 | 3/3 | 3/3 | 3/3 | 5; +241/−13 · 6; +380/−72 · 6; +235/−65 |
| Open-Meteo | wrapper | 3/3 | 3/3 | 3/3 | 3/3 | 5; +281/−9 · 5; +228/−15 · 6; +337/−9 |
| REST Countries | wrapper | 3/3 | 3/3 | 3/3 | 3/3 | 5; +251/−7 · 5; +291/−7 · 5; +274/−7 |
| Datadog Events | wrapper | 3/3 | 3/3 | 3/3 | 3/3 | 4; +250/−14 · 5; +293/−12 · 6; +305/−12 |
| FRED | control | 3/3 | 3/3 | 3/3 | — (native `kind()`) | 4; +250/−4 · 4; +265/−5 · 4; +261/−6 |
| Europe PMC | control (opaque body) | 3/3 | 3/3 | 3/3 | — (native `kind()`) | 4; +224/−9 · 4; +183/−9 · 4; +237/−6 |

Wrapper runs 12/12 complete the change (oracle and existing suite), as do the controls 6/6. Patch sizes are
diagnostic only (the wrapper task carries an extra public variant by design): wrappers 4–6 files, +228 to +380
lines; controls 4 files, +183 to +265.

**Instrument finding (checked by hand, not a task failure):** the bank's propagation check reported "Limit
omitted from the Concord-to-wrapper mapping" in 7 wrapper runs (Frankfurter 3, REST Countries 3, Datadog 1).
Its pattern (`ErrorKind::Decode => Self::Decode(error),\s*_ => Self::Read(error)`) only recognises a new arm
placed after `Decode`. In all 7 patches the agent inserted `ErrorKind::Limit => Self::Limit(error)` directly
before `Decode`; the mapping is present, and the hidden oracle (which exercises the limit failure) passed.
These are false positives of an order-sensitive pattern; the corrected count of missed propagation is 0/12.
The remaining "facts needing review" (Limit participating in `Display`/`source` delegation) are present in the
patches as well.

## Disposition (h11-baseline-review-sol: ACCEPT)
**H11's wrapper slice is not carded.** No measured task deficit: with today's code (every wrapper already maps
`Error::kind()`), the seeded error-stage maintenance change succeeds in every wrapper run like the direct
controls; authoritative budget sites 18/18, wrapper classification 12/12, no missed site; patch sizes are
diagnostic and the extra wrapper sites accompany the task's required public-variant extension. The comparator
that stands is the current design. The broader H11 items (inherent `operation()`, `receive_with`, `document`
adoption) were not measured here and keep "needs a measured baseline".

The review (Codex `gpt-6.1-sol` xhigh, read-only) independently confirmed the run sequence (deep preflight,
both reference replays, the omission replay caught by existing tests, the invalid no-op replay, then 18
sequential agent runs with no replacement), all 18 runs valid with intact oracles and observed `gpt-6.1-sol`
xhigh, the recounts, the reconstructed patches against the base, and the seven mapping flags as recognition
failures (Limit immediately before Decode; the oracles exercise the mapping, source identity and Display
delegation). Its two non-blocking wording fixes are applied above. Not measured: wrapper-removal benefits,
human preference.
