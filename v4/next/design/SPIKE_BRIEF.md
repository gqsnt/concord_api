# Spike lane brief (P3.3.2) — Claude subagent

You implement one idea in Concord far enough that its three target files compile and their tests
pass, then migrate a few more SDKs, then measure. A clear "this does not work because …" with
evidence is a good result too.

Inputs (the coordinator fills these in your prompt): the idea entry (`work/ideas/merged/<L>.md § <id>`),
its target files and NOTE (`work/design/<slug>/`), your worktree and branch, your cargo slot, the
extra SDKs to migrate.

Rules
- Charter `/home/canarit/projects/Concord_Next/CHARTER.md` (C1–C9); method
  `/home/canarit/projects/Concord_Next/METHOD.md`. Facts in the declaration, behavior in Rust.
- Work only in your worktree. Build with `CARGO_TARGET_DIR=<slot> CARGO_BUILD_JOBS=6
  CARGO_INCREMENTAL=0`, always `--locked`. Never write under `/tmp` except your scratchpad.
- Build and test only the crates you touch (`-p concord -p concord-macros -p concord-core -p <sdk>...`);
  never run the full gate (the coordinator does).
- Implement the mechanism in `concord-core` / `concord-macros` / `concord` as small as the targets
  allow. Apply the three target SDKs as written; if a target cannot work, change it minimally and
  say why.
- Never weaken a test or change an expected value just to make it pass. If an expected value
  changes because the design changes the surface (for example a generated doc string, a UI
  `.stderr`), list each one with its reason.
- No negation statements (M5 = 0). Nager, PokeAPI and Frankfurter must not grow.
- Commit nothing: leave the tree for the coordinator, who commits. Do not push.
- Time ceiling: about 90 minutes of work. On reaching it, stop and report how far you got.

Measure
- `python3 -I /home/canarit/projects/Concord_Next/work/glue-dev/measure.py <base worktree> <your worktree> --sdk <migrated sdks> --md <your worktree>/../<slug>-measure.md`
  (base worktree: `/home/canarit/projects/Concord_Next/work/wt/rv-p1`). That gives M1, M2, M4, M5, M6.
- M3 concepts added (DSL keywords/statements, public traits and types in core/macros), M9 call
  sites changed in examples/apps/tests (count and list), M10 test results (commands and counts),
  M11: write 5 seeded mistakes against the new feature as a scratch crate or UI cases and report
  where each error points and whether it names internals.
- M7 (doc words) and M8 (compile time) only if cheap; otherwise say "not measured".

Report (subagents cannot write report files; return it, the coordinator saves it as `work/design/<slug>/REPORT.md`):
what changed per crate and SDK; commands and results; every changed expected value; M1–M11 as
measured; surprises; each counterexample and how it is handled; your honest assessment (3 lines).
Your final message: first 10 lines — status (works / partly / does not work), glue removed, concepts
added, tests, and the one thing the coordinator must know; then the full report text.
