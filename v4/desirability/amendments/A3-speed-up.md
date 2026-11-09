# Plan amendment A3 — speed-up (owner instruction, 2026-11-08 ~17:30 CET)

Effective immediately. **Does not change the decision rule (`contracts/DECISION_RULE.md` A1–A9,
kill conditions, park, stop) or the gates** (local full gate; CI `verify` on Linux and Windows
before a merge counts). Amendments A1 and A2 stay in force; where A3 speaks, it supersedes the
process parts of the plan and of A1 (instrument review rounds, review structure, sequencing).
No new push, merge, tag, publication or deletion right.

## Owner text (verbatim)

> Speed-up amendment A3, effective now, without changing the decision rule or gates:
>
> Threat model: candidate authors are cooperating, isolated agents. An instrument finding is
> blocking only if it would change the verdict on the actual data. Cap instrument review at 2
> rounds, then record the remaining findings as known limits and proceed.
> Generalise the H04 bank, sandbox, clearance and scoring into one reusable pilot harness, reviewed
> once. Later hypotheses add task files only.
> Don't wait for CI between steps. Measure and review while CI runs, and require green CI (Linux
> and Windows) only on the commit being merged. Check whether CI caches its builds, and propose
> rust-cache or sccache if it doesn't. Never reduce Windows coverage.
> Do a flake sweep (T3, the 21 short-timeout tests and the 63 bind-then-drop sites) as the next
> repair item.
> Use one combined Codex review per hypothesis (candidate, measurement and decision together)
> unless it asks for a separate one.
> Keep two hypotheses in flight: survey, card and candidate for N+1 while N waits.
> Prefer compiler or static evidence when it decides the card. Drop the I3 R2/R3 regression runs
> while they're at the ceiling.
> New cards list the expected breakages explicitly in the kill condition.
> Record this as PLAN_AMENDMENT_A3, and report the time per hypothesis after the next two.

## Application (coordinator's reading; each point traceable to the text above)

1. **Threat model and instrument findings.** Candidate authors are cooperating, isolated agents:
   isolation (blind to `work/sealed`, `work/baseline`, `tools/desirability`) and the transcript
   audit stay; adversarial-author hardening is no longer pursued. An instrument-review finding is
   *blocking* only when it would change the verdict on the actual data; otherwise it is recorded
   as a known limit in the measurement record. **At most 2 instrument-review rounds** per
   instrument; after round 2 the remaining findings are recorded as known limits and the loop
   proceeds. (H04 needed 9 rounds; that pattern ends.)
2. **One reusable pilot harness.** The H04 materials (sealed bank layout, bubblewrap sandbox,
   candidate clearance/transcript audit, scorer) and the I3 R3 compile/run sandbox are generalised
   into one harness under `tools/desirability/pilot/` with a task-file format; it gets one review
   (≤ 2 rounds). Later hypotheses add task files (sealed) only. The H03 measurement lane, already
   running when A3 arrived, finishes on its own tooling; its corpus is ported to task files when
   the harness exists if that is cheap, otherwise kept as is.
3. **CI.** No step waits for CI. Measurement and review run while CI runs. Green CI on Linux and
   Windows is required only on the commit being merged (into `concord-desirability`), as the
   decision rule's A1 already states; intermediate commits (records, candidates, fixes) are not
   waited on. Windows coverage is never reduced. **CI caching check (done 2026-11-08):** the
   workflow (`.github/workflows/ci.yml`) has no cache step; the repository has 0 Actions caches;
   `verify` takes 31–36 min on Ubuntu and 65–93 min on Windows, twice per push (push + PR).
   Proposal to the owner: `Swatinem/rust-cache` (or `sccache` with the GitHub Actions backend) in
   the `verify` job — see the proposal in the coordinator's report; adopted only on the owner's yes.
4. **Flake sweep = next repair item (T3).** T3 widens from the stream-stress cancellation test to
   the sweep: that test, the 21 short-timeout tests sharing a token/retry timeout, and the 63
   bind-then-drop port sites. Same repair standard as T2 (fail-before/pass-after or a demonstrated
   race, controls, saved evidence, no weakened assertion), one review.
5. **One combined Codex review per hypothesis** (`gpt-6.1-sol` xhigh read-only): candidate,
   measurement and decision in one review, after the measurement, unless the reviewer asks for a
   separate one. A missing response, a timeout or an unavailable model is still not a verdict
   (DECISION_RULE A9). Migration review stays part of A8/A9 when a WIN leads to migration (it may
   be combined with the gate check of the migrated commit).
6. **Two hypotheses in flight.** While hypothesis N waits (measurement, CI, review), N+1 gets its
   survey, card and candidate. Card pre-registration before candidates and the blind measurement
   order are unchanged.
7. **Evidence preference.** When compiler or static evidence decides the card (a kill condition, a
   compile-level outcome, a public-API diff), it is used and model-agent runs are not added on top.
   The I3 R2/R3 regression runs are dropped while R2/R3 are at the ceiling (baseline: ceiling in
   both runs); they return if a change could plausibly move them off the ceiling.
8. **Cards.** New cards list the expected breakages explicitly in the kill condition (the classes
   and the known sites a candidate is expected to break, so an expected break is not a kill and an
   unexpected one is). (Lesson from H03.)
9. **Reporting.** Wall-clock time per hypothesis (card commit, or survey start where the survey
   precedes the card, to decision row) is reported after the next two hypotheses (H03b and H06),
   from `work/state/HYPOTHESIS_TIMES.md`.

## Addendum A3.1 — lanes run on Codex (owner instruction, 2026-11-08 ~18:40 CET)

Owner: "i have 57k credit that can be used (codex credit), i would like you to use them instead of
me using a reset." Application:
- Every new working lane — candidate implementation, measurement (task files, harness runs),
  surveys, repairs and their fix rounds — runs as a Codex `gpt-6.1-sol` session through
  `work/tools/codex_role.sh` (`workspace-write`, the lane's cargo target, evidence folder and
  `~/.cargo` added with `--add-dir`, `CARGO_NET_OFFLINE=true`). Codex cannot commit (`.git` is
  read-only in its sandbox); the Claude coordinator audits the lane (transcript audit, diff check)
  and commits. Smoke test passed 2026-11-08 17:36Z.
- Claude subagents are no longer started for working lanes. The five Claude lanes already deep in
  their work when this arrived (H03 measurement, H03b B′, H06 B, T3 round 2, T3 sweep) finish their
  current stage only; their next stages move to Codex.
- Reviews stay Codex `gpt-6.1-sol` xhigh read-only (DECISION_RULE A9), in a separate session from
  any Codex author. For Codex-authored work this review is **independent but same-provider**, not
  cross-provider; records say so (CLAUDE.md rule 6).
- **Sandbox finding (2026-11-08 ~19:00):** Codex's own `workspace-write` sandbox on this host blocks
  every socket (tests need 127.0.0.1) and user namespaces; AppArmor also denies nested user
  namespaces, so Codex's sandbox cannot run inside bubblewrap. Working lanes therefore run through
  `work/tools/codex_jail.sh`: Codex `-s danger-full-access` inside an outer bubblewrap jail (root
  read-only; writable only the worktree, the lane's cargo target and evidence folders, `~/.codex`;
  `.git` read-only; `work/sealed`, `work/baseline`, `<worktree>/tools/desirability`, `~/.claude`
  hidden; network shared) — the same confinement the I4 runner uses. Smoke test passed (local
  sockets, socket tests 13/13, hidden dirs empty, `.git` and workspace state read-only). The pilot
  harness itself needs bubblewrap, so measurement lanes author sealed task files in Codex and the
  coordinator runs `pilot.py` (plain commands).

## Addendum A3.2 — model mix and effort (owner instruction, 2026-11-08 ~19:15 CET)

Owner: "use opus medium instead of xhigh for subagents / background tasks. try to use more codex less
claude (75% codex, 25% claude)." Application:
- Claude subagents are started with `effort: medium` (Agent tool), not the inherited xhigh. Lanes
  already running keep the effort they started with (recorded in the model ledger).
- Working lanes are split about 75% Codex / 25% Claude, counted by lanes started from now on
  (`work/state/LANE_MIX.md`). Claude gets the lanes Codex cannot do on this host — those that must
  run the pilot harness or another bubblewrap sandbox — and coordination-heavy work; everything else
  (implementation, task-set authoring, repairs, surveys) goes to Codex in `codex_jail.sh`.
- Unchanged: reviews are Codex `gpt-6.1-sol` xhigh read-only (DECISION_RULE A9, CLAUDE.md rule 6);
  Codex working lanes keep xhigh unless the owner says otherwise.
