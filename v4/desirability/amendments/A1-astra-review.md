# Plan amendment A1 — adopt the Astra desirability review (owner request, 2026-11-07)

**Authority.** The owner asked in chat (2026-11-07, ~21:15 CET): "here is a review made by
chatgpt astra pro about concord desirability, direction etc. … review them, reflect what you
should change in current plan, incorporate, ignore." Inputs copied to
`inputs/astra_desirability_review/` (written at `c1b75b95`, before the F-phase close).

This file **supersedes the named parts** of `MISSION.md`, `PLAN.md` and `contracts/` where they
conflict. The pack files are kept unchanged as the original record. Grants in `AUTHORITY.md` are
unchanged: this amendment adds no push, merge, tag, publication or deletion right. The F phase
(close and merge PR #5) is unaffected.

Coordinator assessment of the review: its central correction is right. The original contract
rewarded structural shapes (fewer wrappers, more generated surface, fewer lines/words/hunks) as if
they were developer benefits; each can improve while the SDK gets worse. Several of its findings
were already handled in the close (capability status, canary transform controls); a few points are
out of scope for automatic loop decisions (listed in §8).

## 1. Objective (supersedes MISSION "Goal", DESIRABILITY intro)

Desirability is a contextual reason to choose Concord and keep using it. The loop tests the
engineering hypothesis that, for a named actor and task, Concord makes the provider contract
easier to **discover, use correctly, diagnose, change and extend** than the current design (and
its best documented workaround), at a proportionate cost. Task outcomes are evidence *for*
desirability; human attraction, preference and adoption stay **unmeasured** and are stated as such
in every record. No "sexiness" score exists.

Every card records: actor (SDK author, caller, maintainer, extension author, reviewer); task with
materials; comparator (current Concord + best workaround); independently checked success
condition; unlike witnesses, a contrary case and a simple control; costs across Concord,
capability crates, SDK, callers, docs and migration.

## 2. Three evidence classes (supersedes the "metrics are a vector" acceptance role)

1. **Hard gates** (cannot be traded): local full gate + CI Linux/Windows on the exact commit;
   no weakened test; truthful public contracts; preserved provider semantics and counterexample
   behaviour; scoped sensitivity (§5); north star (§4); complete migration of what is accepted;
   independent review.
2. **Primary task outcomes** (decide a design hypothesis): pick 1–3 per card from
   - *discover the contract*: required inputs, destination (base vs path), public result, error
     owner, decisive exception; first answer kept separate from assisted recovery;
   - *complete ordinary use*: construction + call compile **and** pass independent wire/result
     checks;
   - *diagnose and recover*: correct failure classification and permitted recovery from retained
     non-secret facts;
   - *change one provider fact*: authoritative semantic edit sites, missed propagation, and the
     stage that detects an omission;
   - *trustworthy construction*: invalid tenant/destination/credential combinations unavailable or
     rejected at the intended stage, valid replacements still possible;
   - *extend and maintain economically*.
   Critical facts are scored separately: a right method cannot compensate for a wrong destination,
   result or recovery decision.
3. **Diagnostics and bounded costs**: D1a, D1b, D1c, D2a, D3, S2, R4 (lines, words), R5 counts,
   hunks, test totals. Always reported; **never decide acceptance by themselves**. A card may
   pre-register a specific cost with a bound (it is then a gate at that bound). A diagnostic
   moving the "wrong" way needs the reviewer's explanation, not automatic rejection. Exception kept
   as a default kill condition: the simple controls (Nager, PokeAPI, Frankfurter) gain no required
   ceremony (P9).

## 3. Decisions (supersedes DECISION_RULE A3/A4 and the outcome set; A1, A2, A5–A9 stay)

- Outcomes: **WIN → ACCEPT** (after full migration and gates), **LOSS → REJECT**,
  **INCONCLUSIVE** (variability, ceiling, provider disagreement, missing oracle) and
  **KEEP_CURRENT** (the existing mechanism solves the task as well at lower total burden).
  INCONCLUSIVE never counts as a win and never closes a family; for stop rule 3 it counts as
  "not accepted".
- **Three decision paths:**
  1. *Correctness repair* (a reproduced defect): fail-before/pass-after with controls + review.
     No probe campaign needed.
  2. *Documentation truth correction* (a docs statement false against source): source evidence +
     review.
  3. *Design hypothesis*: full card, pilot, decision rule.
- **Pilot, then migrate.** Prototype on ≥2 unlike witnesses, ≥1 contrary case and ≥1 simple
  control; decide WIN/LOSS/INCONCLUSIVE on the pilot; only a WIN gets full migration of every
  affected SDK, the broad gate and CI before ACCEPT. A LOSS can be decided on the pilot.
- **Stochastic comparisons** (model readers/editors): paired items across baseline and candidate,
  balanced order, ≥3 fresh repetitions where the decision rests on them, results per task, SDK
  and provider; repetitions are not independent participants. A two-run range is not a confidence
  interval; pre-registered thresholds are pilot triage rules.
- **Concept budget** counts rules a developer must remember (conversion behaviour, ordering
  constraints, policy bypasses, meaningless generated controls), not only keywords and types.
- **Wrappers are classified before judged**: mechanical adaptation (subtraction target), provider
  semantics, deliberate public/safety boundary (Xero tenant, Mastodon instance+token, Notion
  completeness), or unexplained residue. Only the first and last are removal targets.

## 4. North star (supersedes NORTH_STAR P3 wording)

P3 becomes: **"The tree owns shared request context and makes required parent identity explicit
where it is declared."** Navigation often mirrors the resource path, but a scope may carry
context on the wire elsewhere (Xero's tenant scope is a header). P1 is checked by
*prepared-request prediction* (§6). Other properties unchanged.

## 5. Sensitivity (refines DESIRABILITY S1/S1+ and I2)

- Claim scope: "no tested canary leaked on these enumerated surfaces and scenarios" — never a
  universal proof.
- Three observability boundaries: Concord-owned views; SDK-owned public views; native, application
  and external output (excluded, with owners named).
- Transform-aware exercise controls (already in `checks/canary`): bearer literal on the wire;
  Basic via its decoded encoding; signing secrets via an independently verified signature **and**
  absence of the raw key; response-only values via a reached decoder; refused connections via the
  error path, no delivery asserted.
- A server echoing a caller secret into a field the SDK documents as printed is out of scope;
  echoes are checked only against fields the SDK promises to withhold.
- Pair with diagnosis: errors keep safe facts (operation, status, stage, delivery) where the SDK
  documents them.

## 6. Instruments (amends DESIRABILITY § Instruments)

- **I1**: diagnostics only (§2.3).
- **I3** is *docs-assisted* prediction (not "at first sight"). Every item names its observation
  boundary: `prepared` (truth = `prepare()` dump), `operation` (what the SDK's public `send()`
  puts on the wire when a policy changes the request — truth from a local receiver, else the item
  is excluded), `transport` (not used for decisions). R3 = *complete ordinary use*: compile **and**
  an independent wire/result check; compile-only is a diagnostic. No answer-bearing examples in
  materials. Banks: main (seed 1); **validation** bank (sealed, consulted at holdout checkpoints);
  **final confirmation** bank (separate seed, sealed, used once at LF after the candidate freezes;
  if it drives a change, record that and generate fresh confirmation).
- **I4**: authoritative semantic edit sites + missed propagation + detection stage are the
  outcome; hunks are a diagnostic.
- **I5**: diagnostic; for an input/diagnostics card the outcome is first-call success and repair.
- **Instrument validation**: each instrument must score a seeded known-bad case as bad and a
  known-good case as good before its baseline counts (I2 already did this with two temporary
  mutations in lane 2).

## 7. Queue (supersedes the initial ranking in `queue/HYPOTHESES.md`)

1. **H04 docs-only pilot on the current API**, independent of H01–H03, pre-registered from
   `inputs/astra_desirability_review/First_Experiment.md`: witnesses Cloudflare, Sentry, Kraken;
   controls Nager, Xero, Strava; 4 tasks per SDK (24); 2 providers × 3 fresh repetitions per SDK
   and arm (72 sessions, 288 answers); paired and interleaved; critical-fact scoring; its WIN /
   LOSS / INCONCLUSIVE thresholds frozen before candidate outcomes are read. Candidate source: the
   `sdk50/x50-docs` prototype rebased on current main, plus corrections from the owner's six-run
   trial. Generator edits change documentation only. Docs are revalidated after any later
   signature change.
2. **H08 policy completeness** (Strava omitted-edit witness): compare an SDK-local exhaustive
   mapping/trait with one narrow opt-in macro design; classification meaning stays SDK-owned; the
   no-policy control gains no ceremony.
3. **H03 exact inputs** (caller task: first correct call, required annotations, repair).
4. **H06 construction and controls**, after classifying wrappers (§3).
5. **H01 public result** (accurate docs/aliases vs a representation change), **H02a** a reusable
   value+metadata carrier, **H02b** provider-error-document ownership (higher semantic risk; split
   from the old H02).
6. Bounded helpers, each with its contrary case: H09a, H09b, H10, H11, H05 (scoped redaction
   helper paired with diagnosis), H12, H13. **H14** is records reconciliation (largely done in
   `v4/sdk50/CLOSING.md`).
7. **Docs-truth item T1**: `v4/PRODUCT.md` "What Concord is not" says V4 has "no hooks" and "no
   operation identity", while `SendWith`, `Sends`, `Operation` and `Endpoint` are public. It sits
   in the charter's non-responsibilities section, which `AUTHORITY.md` reserves to the owner, so
   the proposed wording goes to the owner queue (O4); the loop does not edit it.

## 8. What is not adopted, and why

- **Comparisons with hand-written reqwest clients or OpenAPI generators**: not needed for local
  loop decisions (the comparator is current Concord); recorded in FINAL as unmeasured. A
  comparative product claim would need a separate matched study.
- **Rendered visual checks of documentation** for H04: the probes measure content and
  information hierarchy as text; rustdoc builds and link checks run in the existing gate. Visual
  layout stays unmeasured and is stated as a limit.
- **The review's proposed patch** (`Proposed_Desirability_Contract.patch`, three proposal files
  under `v4/desirability/review/`) is applied on `concord-desirability` as the review record,
  next to this amendment, not instead of it.
- Nothing in the review changes the F phase; PR #5 proceeds under its own preconditions.
- Already done in the close (no new work): capability-crate status, codecs/extras description,
  canary transform-aware controls and echo rule.
