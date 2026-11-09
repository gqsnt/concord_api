# Concord desirability loop — final retrospective

Branch `concord-desirability` (draft PR #6, never merged by the loop). Baseline B = `734991e9`; final exact commit
F = `6ab300c1` (gate 66/66; CI: {{CI}}). Charter (owner, A2): the fixed core is using the `api!` DSL — client → scope → endpoint — on
reqwest; everything else is challengeable. Decisions follow `contracts/DECISION_RULE.md` as amended by A1 (outcomes
WIN / LOSS / KEEP_CURRENT / INCONCLUSIVE, pilot then migrate) and A3 (speed-up). Every decision had an independent Codex
`gpt-6.1-sol` xhigh read-only review (same provider as the Codex-authored lanes; never labelled cross-provider). The loop
stopped by **Stop 1** (`DECISIONS.md` STOP row): no queued hypothesis has a measured deficit behind it.

**Version.** Unchanged (4.2.0), no tag: the versioned `concord-*` crates changed additively (`controls`, `operations` /
`SendOperation`), by fixes (T3) and in documentation; the API removals are in the unpublished example SDKs (0.0.0),
with replacement routes in `docs/MIGRATION.md`.

Readers and editors are models, not people: every outcome below is evidence about how predictable and editable the
contract is for model callers, not about human preference or adoption, which stay unmeasured.

## 1. Generated documentation — the contract a caller sees (DOCS, REQUEST)
**Responsibility.** The docs `api!` generates must let a caller predict the request and the result without reading the
macro.
**Tried.** H04 (contract-first information hierarchy); H16 → H16r → H16s (request composition: query order, path-hole
encoding, header provenance; SDK-local notes where the macro cannot see the value); the LF narrow revert of H04's
base-URL wording.
**Decisions and evidence.**
- H04 **ACCEPTED (WIN)**: pilot-3 witness net +6 (Claude +3, Codex +3, 0 regressions), control lost 0; docs only.
- H16 **INCONCLUSIVE**: the I3 materials did not contain generated rustdoc, so the mechanism was never shown to readers.
- H16r **INCONCLUSIVE**: pooled W +19 (both providers) but the per-SDK guard failed on one ambiguous scored field (the
  question said "the server receives", the key scored the SDK/Concord `accept`).
- H16s **ACCEPTED (WIN)**: with a stated header boundary — witness R1.exact Codex 100 → 110, Claude 95 → 110 of 115
  (pooled +25 ≥ 16), controls 20 → 20; full migration (76 SDK-local notes in 55 SDKs); a post-pilot truth correction for
  S3 ("prepared" + a preparation-boundary sentence) judged within the claim.
- **Overfitting finding at LF** (validation bank): Claude R2.base_url 19/19 → 16/19 in both runs; H04's "replaces the
  whole base, including its path" led readers to pass the declared path prefix. **Owner: narrow revert** (H04r) — the
  base-URL wording is the baseline's again (`f513806f`); H04 stays accepted otherwise. On the final confirmation bank,
  after the freeze, R2.base_url is 17/17 for both providers in both arms. The restored shorthand ("the declared base
  URL" after the full URL) keeps the baseline's own ambiguity.
**Surviving counterexamples.** ClickHouse 0/5 per provider in every I3 pilot: its wrapper header note is not in the packed
reader materials (instrument limit). Kubernetes/MediaWiki query order was already predicted; World Bank (order is not a
provider requirement) unchanged.
**Public and caller changes.** Documentation only (no API, wire or behavior change); doc-text assertions changed.
**Remaining SDK code.** SDK-local notes stay hand-written where the value lives in SDK code (custom format constants,
wrapper and policy headers).
**Costs and limits.** Generated words per endpoint 543.8 → 888.7 (I1 R4c); SDK doc lines on Concord mechanics 73 → 83.

## 2. Operation classification and policy completeness (POLICY)
**Responsibility.** An SDK whose send policy or paging classifies operations must not silently miss one.
**Tried.** H08: opt-in `operations` enum + `SendOperation` (B) vs naming the output type (A).
**Decision.** **ACCEPTED (WIN)**: 6/6 seeded omissions in Strava, Sentry, Kraken move to an SDK build error (E0004
naming the endpoint), baseline: run-time refusal or wrong behaviour; I4 e4 9/9 vs 9/9 (ceiling); full migration (PayPal,
Zendesk, Datadog, Square, GitHub).
**Public changes.** One DSL statement and one core trait (`SendOperation`), both opt-in and additive.
**Costs.** Compile cost < 5 %; one concept.

## 3. Inputs — what a caller may pass (INPUT; family closed)
**Tried.** H03 exact inputs infer (A hybrid, B + named generics); H03b under an enumerated compatibility break.
**Decisions.** H03 **REJECTED** by its kill condition (newly rejected caller spellings: borrowed `&str` into `String`
aliases, generic `IntoValue<u16>` forwarders). H03b **KEEP_CURRENT**: witnesses 134/285 → 187/285 (+18.6 points < the
frozen 20). INPUT family closed (H03 LOSS, H03b KEEP_CURRENT). The `IntoValue<T>` baseline is retained.

## 4. Generated controls and construction (SURFACE)
**Responsibility.** A generated public control must not let a caller break an invariant the SDK documents.
**Tried.** H06 (`controls { pub(crate) X; remove Y; }` vs `#[fixed]`), H06c (adoption where a control contradicts a
documented invariant); H06b (scope representation) and H07 (facade residue) were not carded (no measured deficit).
**Decisions.** H06 **ACCEPTED (WIN; pilot scope by owner decision O7)**: FM1–FM4 unavailable at caller compile (baseline
sends each forbidden request), 16 valid moves and 3 controls pass. H06c **ACCEPTED (WIN)**: 23 withheld controls each fail
outside the SDK crate; valid moves (rotation, `set_connection`, `set_policy`) compile; 17 removals + 4 constructors.
**Public and caller changes.** Breaking in 11 example SDKs (unpublished, version 0.0.0): removed or crate-visible
setters/`with_base_url`, replacement routes in `docs/MIGRATION.md`. Concord crates: one additive DSL statement.
**Repair split out.** T4 (QuickBooks sends to the current connection's environment).

## 5. Traversal and resume (CAPABILITY)
**Tried.** H09a baseline (`Pages::starting_at` would remove 18 `_from` twins); H09c (resume guidance at the methods).
**Decisions.** H09a **not carded as formulated**: once the materials show what a docs.rs reader reaches (the linked
`Resume` definition), resume ≈ fresh; the review found the residual Codex import failures support only a narrow docs card.
H09c **ACCEPTED (WIN)**: resume Codex 62 → 70, Claude 63 → 70 of 70; every baseline miss an import-path compile error.
**Public changes.** Documentation of 38 public `Resume`-taking items in 27 SDKs + the Kubernetes app.
**Surviving counterexamples / limits.** 4 of 12 twin SDKs do not re-export `concord_pagination` (callers add the
dependency); the H04 example filter hides `Resume::At(..)` prose spans in reader materials; Codex PokeAPI simple-fresh
control 1/5 (an explicit `offset=0`). H09b (recovery after a failed continuation) unmeasured.

## 6. Results and errors (RESULT, ADOPTION)
**Tried.** H01 survey (public send result), H02a survey (value+metadata carrier), T5 (docs truth), H11 baseline
(wrapper error stages).
**Decisions.** H01 not carded (result and error-owner facts at ceiling); H02a deprioritised (no task-level deficit);
T5 **ACCEPTED** (33 false or misleading send-result statements corrected in 25 files); H11's wrapper slice not carded
(wrappers 12/12 like direct-error controls 6/6; every wrapper already maps `Error::kind()`). H02b (provider
error-document ownership): **no measured task deficit; unmeasured** — counterexamples kept (Slack 200 `ok:false`,
Cloudflare diagnostics on success, S3/B2 XML errors, Snowflake 202, Kraken envelopes).

## 7. Correctness of the transport and the tests (CORRECTNESS)
T2 **ACCEPTED** (test server accept loop and chunk accounting; paused-clock and held-socket tests), T3 sweep **ACCEPTED**
(64 bind-then-drop sites, ~41 timeout tests, 12 stalled-body cases on the paused clock), T3 cancellation **ACCEPTED**
(a dropped `send()` stops polling its streamed upload body: 262,144 polls / 32 MiB after the drop before the fix),
T4 **ACCEPTED** (QuickBooks environment after `set_connection`). Proposals left: SDK deadlines on `std::time::Instant`
(Sentry, PayPal, B2/concord-retry, Zendesk, Spotify …).

## 8. Charter and scope
T1 **ACCEPTED**: `v4/PRODUCT.md` states the fixed core; former exclusions become current scope. O1 / H15 (Concord-produced
observability inside the safe place) parked: an owner objective without a measured task deficit.

## 9. Versioning and shared prefixes (DOMAIN)
H12 not carded: a client `path` already gives a shared prefix; its one recorded behavioral failure (baseline I4
Kubernetes e1) was a shared-target instrument artifact (replayed alone: 4/4).

## 10. Not carded, retired or unmeasured
H05 (declarative redaction), H10 (single-attempt transport builder), H11's other adoption items, H06b, H07, H09b, H02b:
no measured task deficit; unmeasured. H13, H14 retired as formulated; H15 parked.

## Scoreboard: baseline B → final F
**I1 scoreboard** (diagnostic, `scoreboard/734991e9.json` → `scoreboard/f178e63d.json`; the H04 narrow revert after
`f178e63d` changes generated doc text only):

| Metric (better) | B `734991e9` | F `f178e63d` | Note |
|---|---|---|---|
| D1a facade forwarders (↓) | 98 | 98 | unchanged |
| D1b (↓) | 22 | 22 | unchanged |
| D1c generated share of the public request surface (↑) | 0.727 | 0.7315 | H06/H06c controls, H08 |
| D2a version paths / shared heads / shared keys (↓) | 18 / 5 / 5 | 18 / 5 / 5 | H12 not carded |
| D3 (↓) | 16,065 | 15,666 | |
| S2 hand-maintained redaction impls (↓) | 192 | 192 | H05 unmeasured |
| R4a DSL lines per endpoint (↓) | 6.554 | 6.627 | `controls`, `operations` statements |
| decl_doc_mech SDK doc lines on Concord mechanics (↓) | 73 | 83 | H16s SDK-local notes (a cost) |
| R4c generated rustdoc words per endpoint (↓) | 543.8 | 888.7 | H04, H16s richer docs (a cost) |

**I3 main bank** (seed 1; baseline in `BASELINE.md`, R1.exact Claude 0.86/0.86, Codex 0.94/0.90; R2 and R3 at ceiling)
served the pilots; it was not re-run as a whole at F (A3: R2/R3 regression runs dropped at ceiling). The REQUEST pilots
measured its witness items directly: H16s witness R1.exact Codex 100 → 110, Claude 95 → 110 of 115.
**I4 controls** (Codex only, owner O6): baseline 40/40 oracle passes after the H12 correction; at F 24/40 tasks remain
valid (bank drift), 24/24 in both arms.

## Holdout and final confirmation
All in `measurements/HOLDOUT-LF.md`. The mid-loop checkpoints were missed (procedural gap); these LF results do not
retroactively satisfy them.
- **Validation bank (I3, B vs F, 2 runs per provider):** R1.exact up for both providers (Claude 58/57 → 60/64, Codex
  63/61 → 65/66); R1.critical and R3 hold; Codex R2.send_returns within its baseline spread; **Claude R2.base_url 19/19 →
  16/19 — an overfitting finding** traced to H04's base/path wording → owner decision, narrow revert (H04r).
- **I4 on the 8 control SDKs:** 24 of 40 tasks still validate at F (12 reference patches no longer apply, Sentry e1/e3/e5
  oracles fail to compile after H06c, Strava e1); the 40 agent runs were invalidated by a runner artifact (a detached
  auto-gc during sessions) and re-scored by replaying the saved edits with the fixed runner: oracle 24/24 and suite
  24/24 in both arms, missed-propagation tasks 4 → 4. No overfitting finding.
- **Final confirmation bank (once, after the freeze at `6ab300c1`):** no metric below the baseline spread; Codex R1.exact
  55/60 → 63/63; Claude R1.exact 51/57 → 54/56; R2.send_returns 64/64 for both; R2.base_url 17/17 for both; R3 65/65.
  No overfitting finding; the bank drove no change.

## Instrument limits
- Model readers and editors are not humans; preference and adoption are unmeasured.
- Ground truth comes from the implementation (truth harnesses, oracles), not from providers.
- The held-out separation is partial: the validation bank was consulted once at LF (the mid-loop checkpoints were
  missed — a procedural gap) and drove the H04 narrow revert; the final confirmation bank was used once after the freeze
  and drove no change.
- I3 materials are text renderings: the H04 example filter hides code-like prose spans; linked definitions were added
  only where measured (H09a round 2); generated docs only with `--generated-docs` (H16r/H16s).
- I4 shared measurement targets contaminated one baseline run (Kubernetes e1); LF used private targets. The other 39
  baseline records were not replayed. At LF the runner's own export crossed git's auto-gc threshold and invalidated all 40
  agent runs; the saved edits were re-scored by replay with the fixed runner (no new agents).
- Every review is same-provider (Codex reviewing Codex-authored lanes): independent sessions, not cross-provider.

## Owner queue
- **O3 — merging the draft PR into `main`, any tag or release:** not granted; the loop stops here.
- **O8 (new) — what is left unmeasured:** H02b (provider error documents; counterexamples kept), H05, H06b, H07, H09b, H10,
  H11's other adoption items: no measured task deficit; prioritise only on a measured deficit. H15/O1 stays parked.
- **O9 (new) — instruments:** the I4 control bank is pinned to the baseline (16/40 tasks no longer validate at F) and needs a
  new bank before further use; the I4 runner fix (`gc.auto=0`) is merged; the mid-loop holdout cadence was missed and
  should be enforced by the coordinator next time.
- Resolved during the loop: O4 (→ T1), O5 (→ T2/T3), O6 (I4 Codex-only), O7 (H06 pilot scope → H06c). O2 (deleting old
  campaign targets) was not needed; disk cleanups of this loop's own finished targets were approved case by case.
