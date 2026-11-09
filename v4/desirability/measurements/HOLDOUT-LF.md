# LF holdout — sealed validation bank (I3) and the 8 control SDKs (I4)

The mid-loop holdout checkpoints (PLAN step 7: every 3 accepts; due after H06, the 3rd accept, and again later)
were **not run** — a procedural gap, disclosed in the stop row. This LF holdout compares the baseline commit
B = `734991e9` with the final exact commit F = `f178e63d` (gate 66/66). It does not retroactively satisfy the missed
checkpoints.

## I3 on the sealed validation bank (`work/sealed/bank`, 29 SDKs, 69 scored items per session type)
Method (`work/tools/holdout_i3.sh`): per arm, a fresh copy of the sealed bank, truth regenerated against the arm, the
arm's own default I3 instrument (the opt-in modes added since B off; defaults proven byte-identical), rustdoc built in
the arm's tree with a private target, 2 runs per provider (Codex `gpt-6.1-sol` xhigh — observed; Claude
`claude-opus-5-5` effort medium — observed model, effort not visible), wire + rc sessions; outputs sealed in
`work/sealed/holdout/bank-{B,F}/`. One Zendesk call item is dropped in both arms (no local receiver; known limit).

| arm | reader | run | R1.critical | R1.exact | R2.send_returns | R2.error_type | R2.base_url | R3.pass |
|---|---|---|---|---|---|---|---|---|
| B | Claude | 1 | 62/69 | 58/69 | 68/69 | 68/69 | 19/19 | 69/69 |
| B | Claude | 2 | 61/69 | 57/69 | 64/69 | 69/69 | 19/19 | 69/69 |
| B | Codex | 1 | 68/69 | 63/69 | 69/69 | 69/69 | 19/19 | 69/69 |
| B | Codex | 2 | 66/69 | 61/69 | 64/69 | 69/69 | 19/19 | 69/69 |
| F | Claude | 1 | 62/69 | 60/69 | 68/69 | 68/69 | **16/19** | 69/69 |
| F | Claude | 2 | 65/69 | 64/69 | 69/69 | 69/69 | **16/19** | 69/69 |
| F | Codex | 1 | 67/69 | 65/69 | 64/69 | 69/69 | 19/19 | 69/69 |
| F | Codex | 2 | 68/69 | 66/69 | 64/69 | 69/69 | 19/19 | 69/69 |

Noise bands (DESIRABILITY § I3: the larger of the baseline two-run spread and 2 points): R1.exact rises for both
providers (Claude 57–58 → 60–64, Codex 61–63 → 65–66); R1.critical and R3 hold; R2.send_returns for Codex (69/64 →
64/64) stays within its baseline spread (5 items). **Claude R2.base_url falls from 19/19 to 16/19 in both runs** — the
baseline runs agree exactly (band 2 points), the fall is ≈ 16 points: **a regression beyond the noise band.**

### Overfitting finding (DECISION_RULE § Overfitting check)
The three misses per run are the same error (`fred/r1`, `openai/r1` in both runs; `cloudflare-zones/r1`, then
`square-payments/r1`): at F, Claude answers `with_base_url("https://staging.example.test/fred")` (`/v1`, `/client/v4`,
`/v2`), which doubles the declared path prefix; the truth (identical in both arms) is the host alone, because the prefix
belongs to the declared path. At B Claude answered the host in every run. The materials differ in exactly this place:
- B: "Request: `GET https://api.stlouisfed.org/fred/series/search` (the declared base URL; the client's `with_base_url`
  replaces it)"; `with_base_url`: "Replaces the declared base URL (for example with a staging server …)".
- F: "Declared base: `https://api.stlouisfed.org`. Declared path: `/fred/series/search`. `with_base_url` replaces the
  whole base, **including its path**; declared paths are appended."; `with_base_url`: "Replaces the whole declared base
  URL, including its path …".
The statement is true (a base's own path is replaced; declared paths are appended), but "including its path" is read as
"include the path prefix in the new base". It was introduced by **H04** (`4145921f`, accepted `070ae0ae`), later edited
by H06 (`d39ef1df`); H16s builds on the same generated-doc structure. Codex is not affected.

Rule: "Revert the responsible accepted change with a new commit and a decision row." **Pending owner decision on
scope** (a full revert of H04 would also remove H16s's and H06's dependent doc text and the R1 gains above; a narrow
revert would restore the baseline's base-URL wording only). The final confirmation bank is **not** used until the
candidate is frozen again.

## I4 on the 8 control SDKs
Running (`work/tools/holdout_i4.sh`; bank copy, hash `e7d963b9…` verified; re-validation at F, then one fresh Codex
agent per task in two sequential queues with private measure targets). Results to follow.
