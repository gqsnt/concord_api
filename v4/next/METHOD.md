# Method

## Lessons from the desirability loop (do not repeat)

| What happened | Rule now |
|---|---|
| About 64k lines of measurement tooling, against ≈4k lines of Concord change | **No new instruments**, except the glue classifier (P3.1). Anything else needs a reason in one sentence and stays under about 300 lines. |
| 12 review rounds on one instrument; 9 on one question bank | **At most 2 review rounds per item.** Remaining findings become recorded limits. |
| Sealed banks and clearance gates against an imagined attacker | **No sealed banks, no clearance gates.** Lanes are cooperating agents. |
| Model readers at ceiling decided what was "a deficit", so the largest problems (errors, redaction, facades) were never tried | **Glue is the deficit.** An idea needs no reader experiment to be tried. |
| Docs grew 63% to satisfy model readers | **Docs never fix a design problem.** Generated docs have a word budget. |
| `controls {}`: a list of things to turn off | Charter rule C1. The review checks for negation statements explicitly. |
| A card, frozen thresholds, separate reviews and impact tables for every idea, so 4–9 hours per idea | **Light records:** one spike file per idea and one review. Owner judgment happens once, at the gallery. |
| Unsure kill conditions led to rescue hypotheses (H03b) | No kill conditions. Measure, show the trade-offs, and the owner picks. |

## Lanes

| Lane | Provider | Sees | Output |
|---|---|---|---|
| Coordinator | Local Claude Opus 5.5, xhigh | Everything | Commits, records, the gallery |
| Idea lane | Claude and Codex, separately; other providers if available | `CHARTER.md`, the family section of `IDEAS.md`, `GLUE.md` and `FRICTION.md` excerpts, the witness and counterexample SDK sources | `ideas/<family>-<provider>.md` |
| Target lane | Codex `gpt-6.1-sol` (workspace-write) or a Claude subagent | One idea, 3 SDK sources, the charter | 3 *after* files plus a 10-line note |
| Spike lane | Codex `gpt-6.1-sol` (workspace-write) or a Claude subagent | One idea, its targets, the repository | A buildable spike tree plus `REPORT.md`. Codex can't commit; the coordinator commits. |
| Reviewer | Codex `gpt-6.1-sol` xhigh, read-only | Diff, report, measurements, charter | Verdict plus findings |

- Record the requested and observed model and effort from harness metadata in
  `v4/next/MODELS.md`. Never guess model IDs.
- Never certify your own work.
- Do not substitute providers silently: record any substitution.
- Codex `workspace-write` keeps `.git` read-only, so the coordinator commits.
- Headless `claude -p` needs explicit `--allowedTools`.

## Measurements (all automatic, per spike and in P4 on all 55 SDKs)

| ID | Measure | Direction |
|---|---|---|
| M1 | **Glue lines** per SDK and category (`tools/glue`) | ↓ (primary) |
| M2 | Model and behavior lines | ≈ (should not grow) |
| M3 | Concepts added: DSL keywords and statements, public traits, public types in `concord-core` and `concord-macros` | small; report **glue lines removed per concept** |
| M4 | DSL lines per endpoint; tokens per endpoint | ↓ or = |
| M5 | Negation statements in declarations (`remove` of generated surface, opt-outs) | **= 0** |
| M6 | Simple controls (Nager, PokeAPI, Frankfurter): declaration lines and concepts used | ↓ or = |
| M7 | Generated doc words per endpoint on the caller path | ≤ budget (family H sets it; the default for other families is "not higher") |
| M8 | Compile time (`benches/scale/ab.sh`), expanded lines per endpoint, `alloc-count` | report; flag > +5% |
| M9 | Caller-visible breaking changes: call sites changed in apps, examples and tests (count and list) | report |
| M10 | Tests: workspace and SDK suites, canary tests, UI tests | all pass; no test weakened; changed expected values listed |
| M11 | Compile-error quality on 5 seeded mistakes per idea: the span is in user code, and no internal path appears | report |

There is no single score. The gallery shows the table and the before/after code side by side.

## Spike record (`v4/next/spikes/<slug>.md`)

```
# <slug> — <family> — <one-line idea>
Branch / commit:
Targets (3 files): links; verdict against C1–C9 (one line each)
What disappears: <glue categories, lines, SDKs>
Measurements: M1–M11 table (baseline → spike), on the migrated SDKs
Counterexample handling: <SDK: how>
Breaking changes: <list>
Review: <verdict, findings, limits>
Agent's assessment: <3 lines>
```

## Review modes (`prompts/reviewer.md`)

| Mode | When |
|---|---|
| `MERGE` | P1.6 |
| `INVENTORY` | P3.1 |
| `SPIKE` | P3.3 |
| `DIRECTION` | P3.5 |
| `MERGE_IDEA` | P4 |

At most 2 rounds each.

## Resources

- **Never under `/tmp`.**
- At most 3 spike lanes at once, at most 2 cargo-heavy jobs at once, and one full gate at a time.
- `CARGO_BUILD_JOBS=6` for lanes.
- Use fixed target slots under `$WS/work/targets/`: `gate`, `lane-a`, `lane-b`, `lane-c`.
- Free-space floor before a heavy job: 20 GiB plus the predicted growth (65 GB for a cold full
  gate) plus 15%. Below it, wait or reclaim this mission's inactive slots (G11).

## Checkpoint (`$WS/work/state/CHECKPOINT.md`)

```
# Checkpoint <ISO time>
Phase/step:
Identities: main=<sha> concord-desirability=<sha> concord-next=<sha> spikes=<…>
Done (with identity):
Active jobs: <pid> <cmd> slot=<…> log=<…>   (or none)
Resources: free=<GiB>
Waiting on: CI run <id> | review <id> | owner pick | nothing
NEXT ACTION: <one exact step>
```

On a restart or "continue":
1. Read the checkpoint.
2. Check the real state: refs, worktrees, processes, CI and lanes.
3. Recover anything interrupted.
4. Report in 2–3 lines.
5. Continue.

Never redo a recorded check on an unchanged identity.

## Reporting to the owner

Short messages:
- when a phase ends;
- when a merge happens;
- when something changes what the owner gets;
- at the gallery.

No running commentary.
