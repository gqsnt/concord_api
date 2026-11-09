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

Rule: "Revert the responsible accepted change with a new commit and a decision row." **Owner decision (2026-11-09):
"Narrow revert"** — restore the baseline's base-URL wording only (lane `h04r-revert-codex`, branch `dz/h04-base-revert`). Options were (a full revert of H04 would also remove H16s's and H06's dependent doc text and the R1 gains above; a narrow
revert would restore the baseline's base-URL wording only). The final confirmation bank is **not** used until the
candidate is frozen again.

## I4 on the 8 control SDKs
Method (`work/tools/holdout_i4.sh`): a copy of the sealed bank (hash `e7d963b9…`, 141 files, 40 tasks — verified
against the repository's `BANK.sha256`; the original untouched), re-validation at F in a throwaway checkout, then one
fresh Codex agent per task (requested `gpt-6.1-sol` xhigh — observed in all 40 rollouts), the I4 tools unchanged since
B, two sequential queues with private measure targets (the baseline's shared-target contamination avoided).

**Validation at F: 24 of 40 tasks kept.** The bank is pinned to B's code; accepted changes drifted 16 tasks: 12 reference
patches no longer apply (files edited by H06c, H08, H09c, H16s, T5 …); Sentry e1/e3/e5 oracles no longer compile
(E0624: `with_base_url` is private — H06c withheld it); Strava e1's oracle fails after its reference. These 16 are not
scored (no valid reference at F) — a limit of an edit bank tied to the baseline.

**Agent runs: all 40 invalid by the runner's tamper check — a runner artifact.** Every flag is `.git` housekeeping only
(1,960 identical removed loose objects and `gc.pid` per run; no SDK-tree, cargo or symlink signal): the runner exports
each task as a fresh one-commit repo, and at F (5,373 files vs 5,199 at B) its own export commit crosses git's
auto-gc threshold (≈ 6,700 loose objects), starting a detached `gc --auto` that packs objects during the session.
Fix (instrument, branch `dz/i4-gcauto` `1f952284`): `gc.auto=0` in the export's config and the trusted git prefix.
**Re-score without new agents** (`work/tools/holdout_i4_replay.sh`): each saved agent edit (`agent.patch`) replayed with
the fixed runner at F, sequentially on one private target, for the 24 tasks valid at F; controls on Nager e1: reference
valid and passing, a no-op change failing the oracle (as designed).

| 24 tasks valid at F | B (corrected: Kubernetes e1 by the H12 replay) | F (replayed agent edits) |
|---|---|---|
| oracle pass | 24/24 | 24/24 |
| existing suite pass | 24/24 | 24/24 |
| tasks with missed propagation | 4 (GitHub e1, Nager e1, Notion e1, Notion e2) | 4 (GitHub e1, Nager e5, Notion e1, Notion e2) |

No regression on I4 (Nager e1's flag cleared; Nager e5 gained one test-expectation flag: a test document still uses the
old field name). One run per task; no noise band beyond the counts. **No overfitting finding on I4.**

## Final confirmation bank (`work/sealed/final`, used once after the candidate froze)
Candidate frozen at F′ = `6ab300c1` (F + the H04 narrow revert `597cc8fc` + the I4 runner fix `f837cf03`; gate 66/66).
Same method as the validation bank (`holdout_i3.sh`, both arms on the sealed final bank, B then F′; 28 SDKs, 65 scored
items per session type, 64 for R2 result items; outputs `work/sealed/holdout/final-{B,F}/`).

| arm | reader | run | R1.critical | R1.exact | R2.send_returns | R2.error_type | R2.base_url | R3.pass |
|---|---|---|---|---|---|---|---|---|
| B | Claude | 1 | 55/65 | 51/65 | 61/64 | 64/64 | 17/17 | 65/65 |
| B | Claude | 2 | 61/65 | 57/65 | 64/64 | 64/64 | 17/17 | 65/65 |
| B | Codex | 1 | 61/65 | 55/65 | 61/64 | 64/64 | 17/17 | 65/65 |
| B | Codex | 2 | 65/65 | 60/65 | 64/64 | 64/64 | 17/17 | 65/65 |
| F′ | Claude | 1 | 55/65 | 54/65 | 64/64 | 64/64 | 17/17 | 65/65 |
| F′ | Claude | 2 | 57/65 | 56/65 | 64/64 | 64/64 | 17/17 | 65/65 |
| F′ | Codex | 1 | 65/65 | 63/65 | 64/64 | 64/64 | 17/17 | 65/65 |
| F′ | Codex | 2 | 65/65 | 63/65 | 64/64 | 64/64 | 17/17 | 65/65 |

No metric falls below the baseline's two-run spread: Claude R1.critical 55/57 against 55/61 (band 6 items) and R1.exact
54/56 against 51/57 stay inside it; Codex R1.exact 63/63 against 55/60 and R1.critical 65/65 rise; R2.send_returns
reaches 64/64 for both; R2.base_url stays 17/17 for both providers in both arms (no path doubling after the narrow
revert); R3 65/65 everywhere. **No overfitting finding; the bank drove no change.** Readers: requested Codex
`gpt-6.1-sol` xhigh — observed; Claude `claude-opus-5-5` effort medium — observed model, effort not visible.
