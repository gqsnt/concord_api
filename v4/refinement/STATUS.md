# Refinement status

Live, resumable record of the refinement campaign (handoff `concord_v4_refinement/`, kept outside
the repository). Earlier V4 records (`v4/STATUS.md`, `v4/evidence/`) describe the alpha and are
historical.

- **Original baseline:** `concord-v4` at `8f7adc2019f3ee31af087c69286d559bca1c173a` (tree
  `26e6a14b83e0a7ca299ef2bbed62080da070d83b`), preserved as tag `concord-v4-alpha1-baseline`.
  Local `concord-v4` = `origin/concord-v4` = that SHA; nothing newer existed locally.
- **Working branch:** `concord-v4-refine` in worktree `/home/canarit/projects/concord_api_v4`.
- **Lockfile:** `Cargo.lock` sha256 `7278f1c08f1ca206fa89da96c33c905f8f333800c0f766c7dde1b4b16c119363`
  at the baseline.
- **Preserved, untouched:** V3 checkout `concord_api` (branch `concord-v3`, 570eec4), worktree
  `concord_api_v3ref`, tag `concord-v3-preserved`, untracked `.claude/` and `concord_v4_handoff/`
  in the V4 worktree (first-campaign inputs).
- **Settings observed:** coordinator Claude Opus 5.5 (`claude-opus-5-5`), effort xhigh. Specialists:
  transcripts record `claude-opus-5-5`, effort `xhigh`. Codex CLI 0.166.0, model `gpt-6.1-sol`;
  the effective model and effort of every run are read from its session log (`turn_context`).
  No `CLAUDE_CODE_EFFORT_LEVEL`/subagent model override in the environment. Codex `memories`
  feature is off and `~/.codex/AGENTS.md` is empty (no cross-session context).
- **Codex execution sandbox (probed, no model request except two tiny probes):**
  `workspace-write` alone forbids socket creation, so loopback tests cannot run; with
  `-c sandbox_workspace_write.network_access=true` loopback works, writes stay confined to the
  workspace, `~/.cargo` is read-only and the internet is reachable. Execution reviews therefore
  use that per-run setting and a private `CARGO_HOME` inside the disposable snapshot.

| Gate | Status | Evidence |
|---|---|---|
| M0 Baseline | PASS | `scripts/verify.sh --msrv` at 8f7adc2, clean tree, all 9 steps PASS (warm cache): `evidence/m0_baseline_verify_8f7adc2.txt` |
| M1 Boundary fixes | PASS | L1 `650a174`, L2+L5 `4f3ff85`, header-map capacity `ab0278c`, failing `Display` `b4ce491` |
| M2 Design decisions | PASS | `DECISIONS.md`; four tracks, fresh-use control, Codex design passes 1–2 |
| M3 Implementation | PASS | D08 `c2871f9`, D01 `2720d67`, D06/D13 `f29e9ba`, D04/D14 `0864d38`, `433abf9`, D16 `8628622`, D11 `f0e203b`, docs/diagnostics `d1844be` |
| M4 Boundaries/adversarial | PASS | `refine-m4-tests` merged `f2968e8` (oracles, surfaces, 9/9 mutations killed), `refine-m4-native` merged `5ee0d96` (29 recipe tests) |
| M5 Performance | IN PROGRESS | paired runs `concord_v4_refine_work/perf/ab-*`, `attr-*`, `final-*`; regression attributed and reduced `79a0faf`; final candidate run pending |
| M6 Docs/fresh use | IN PROGRESS | `refine-m6-docs` merged `e43c9ed`; fresh-use trial of the refined surface pending |
| Milestone review | DONE, triaged | Codex `6604a53`: F-01..F-04 fixed (`e05d2f1`, docs) — `REVIEW_TRIAGE.md` |
| M7 Independent acceptance | NOT_RUN | final frozen candidate review pending |
| Platforms | Linux only; Windows BLOCKED for new commits (no Windows host/VM/wine; CI needs a push, not authorized) | |

## Leads (handoff `guides/02_BASELINE.md`)

| Lead | Category | Result |
|---|---|---|
| L1 native header-name precondition | defect | **Reproduced** at 8f7adc2: a 65,536-byte literal name compiles and `prepare()` panics in `http-1.5.0` (`evidence/m1_l1_panic_at_8f7adc2.txt`). **Fixed** `650a174`: the native constructor runs in `const` (always-evaluated guard + `const` block), so the name is rejected at compile time and the runtime path has no panic. Regression `ui/header_name_exceeds_native_limit` (compiled before the fix), boundary `tests/native_limits.rs` (65,535 bytes works). Limitation: the error spans the whole `api!` invocation. |
| L2 cross-name header order | contract over-claim | **Confirmed**: `c09` asserted `x-tag` before `x-other` on the wire and SEMANTICS §5 said header slots keep first-appearance order. **Corrected** `4f3ff85`: contract = exact query order + value order within one header name; no order across names. |
| L3 static/runtime base asymmetry | design question | D11 KEEP strict subset; invariant tested `f0e203b` |
| L4 performance evidence | evidence gap | Compiler track builds the generator/consumers; timing slot after implementation |
| L5 DNS in initializer test | test reliability | **Fixed** `4f3ff85`: loopback send with a positive assertion |
| L6 process failures | process | checked edits (`rep.py` asserts counts), `--locked`, disposable snapshots |
| HeaderMap capacity (C07) | defect | **Reproduced** (Codex pass 2, core track; 24,576 distinct names); **fixed** `ab0278c` (`try_append`/`try_insert`, build error) |

## Active work

- M5 final paired compile-time run (alpha vs `d1844be`), then the runtime benchmark.
- Fresh-use candidate trial prepared at `concord_v4_refine_work/fresh/candidate` (lib = `d1844be`).

## Process events

- 2026-11-02 ~00:43: the host's Claude Code process restarted, killing the four specialist
  tracks and Codex pass 2 mid-run (pass 2 had run 44 commands, no report yet). Specialists were
  resumed from their saved transcripts; Codex pass 2 was resumed with `codex exec resume` on the
  same session id (snapshot tracked source verified unchanged first). Fresh-use control trial had
  finished before the restart (`concord_v4_refine_work/reports/fresh_control.md`).

## Next action

Finish M5 (record results); fresh-use trial of the refined surface and fixes; frozen candidate for
the Codex final acceptance review (execution mode, loopback, `CARGO_HOME` outside the snapshot);
triage, fix, re-review; final `verify.sh --msrv`; FINAL.md.

## Resume instructions

Reread `git log`, `git status` and this file; check `concord_v4_refine_work/reports/` and the
Codex output directories for finished runs. Do not infer completion from this table alone.
