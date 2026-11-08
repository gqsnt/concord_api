# H04 candidate — boundary audit (coordinator, not a review)

Implementer: Claude subagent (Opus 5.5), committed by the coordinator as `4145921f` on `dz/h04`
(from `1d519c73`); diff of `crates/concord-macros/src/emit.rs` SHA-256 `ea946ee3…` equals the
implementer's report.

Transcript audit (the subagent's JSONL transcript, every tool call input):
- tool calls naming `work/sealed`, `baseline/h04` or `tools/desirability/h04`: one — a recursive
  grep in `work/wt/h04c` with `grep -v "^./tools/desirability/h04"`; that worktree (branch
  `dz/h04`, from `1d519c73`) does not contain `tools/desirability/h04/` (the instrument lives only
  on the local `dz/h04-bank`), so nothing of the instrument was reachable. Disclosed by the
  implementer.
- card reads: headings list, then lines 33–43 (Comparator), the Gates bullet, Kill conditions —
  not Problem, Tasks or Sample.
- other reads: `work/records/docs-trial/` (the six-run trial, as briefed), `ls work/` (directory
  names only).
Conclusion: no access to the sealed bank, the baseline H04 materials or the instrument.
