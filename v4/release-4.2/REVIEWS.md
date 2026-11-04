# 4.2 — model runs, independent reviews and dispositions

Observed settings come from each run's own startup record: for Codex sessions the CLI banner
(`model:` / `reasoning effort:` in `stderr.log`, read by `evidence/launch.py`); for Claude
subagents (Agent tool) the model is the session's, and reasoning effort is **not observable**. No
reviewer certified code it wrote. Times are UTC on 2026-11-04. Reports: [reports/](reports/) (copies;
the originals and their logs are in `/home/canarit/projects/concord-v42/lanes/`).

| Run | Role | Requested | Observed | Source | Result |
|---|---|---|---|---|---|
| coordinator | lead implementer, integrator, author of the compiler changes and records | Claude Opus 5.5, XHigh | `claude-opus-5-5`; effort not observable | — | — |
| m01-bench | M01 implementation | `gpt-6.1-sol`, xhigh | `gpt-6.1-sol`, `xhigh` | `07dcdd0` | `875f21a` (bundle; sandbox Git read-only) |
| runtime-tests | T01, I20-A, D16-sol tests | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `07dcdd0` | `4dd2adf` (committed by coordinator) |
| tooling | M02, M03, M04, fresh extraction, gate wiring | Claude Opus, xhigh (Agent) | `claude-opus-5-5`; effort n/o | `07dcdd0` | `44193b6` |
| docs | Stage 4 documentation | Claude Opus, xhigh (Agent) | `claude-opus-5-5`; effort n/o | `07dcdd0` | `0aa5942` |
| docs (CLI attempt) | — | Claude CLI, xhigh | `claude-opus-5-5` | — | stopped at start (permission classifier refused a sibling CLI launch; the Agent tool was used instead) |
| o01-header-plan | Stage 5 O01 experiment | `gpt-6.1-sol`, xhigh | `gpt-6.1-sol`, `xhigh` | `4874405` | DEFER_WITH_EVIDENCE |
| o02-metadata | Stage 5 O02 experiment | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `4874405` | DEFER_WITH_EVIDENCE |
| rv-compiler | compiler review (Brief A) | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `f0693b5` | REVISE (2 findings) |
| rv-compiler2 | widened re-review | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `aae6218` | REVISE (4 findings) |
| rv-compiler3 | widened re-review | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `e5082c4` | REVISE (1 finding, pre-existing) |
| rv-compiler4 | re-review | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `0670240` | REVISE (1 finding, introduced by `0670240`) |
| rv-compiler5 | re-review | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `9c32294` | REVISE (2 findings, introduced by `1dad499`/`0670240`) |
| rv-final-sol | final executable review (Brief D) | `gpt-6.1-sol`, xhigh | `gpt-6.1-sol`, `xhigh` | `9c32294` | REVISE (2 findings + 1 polish) |
| rv-compiler6 | re-review | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `c69b9b4` | REVISE (1 finding, since `e5082c4`) |
| rv-stage5 | Stage 5 adoption review (Brief C) | Claude Opus, xhigh (Agent) | `claude-opus-5-5`; effort n/o | `4874405` + patches | concur DEFER_WITH_EVIDENCE ×2 |
| trial-opus | documentation-first trial (Brief E) | Claude Opus, medium (Agent) | `claude-opus-5-5`; effort n/o | `7a2819a` | 7 doc issues, 0 library defects |
| trial-terra | documentation-first trial (Brief E) | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | `7a2819a` | 1 doc issue, 0 library defects |

The final executable review (fresh Sol, Brief D) and the last compiler re-review are recorded in
[STATUS.md](STATUS.md) with their source identities.

## Finding dispositions

| Review | Finding | Disposition |
|---|---|---|
| rv-compiler 1 | separate punctuation joined (`choose!(. .)` advised as `..`) | fixed `aae6218`; qualified R1 |
| rv-compiler 2 | invisible group printed bare (`1 + 2 * 2`) | fixed `aae6218` (renderer), superseded by `e5082c4` (advice names `T`); qualified R2 |
| rv-compiler (boundary) | body/output statics regrouped a forwarded fragment (E0308, pre-existing) | fixed `aae6218` (`located`); qualified R3 |
| rv-compiler2 1 | `>` `>` inside macro input joined as generics | fixed `e5082c4` (macro input token for token); qualified R4 |
| rv-compiler2 2 | `1. . 2` re-lexed as a range | fixed `e5082c4`; qualified R5 |
| rv-compiler2 3 | forwarded fragment opaque to a downstream macro, flattened in advice | fixed `e5082c4` (advice names `T`, with a note); qualified R6 |
| rv-compiler2 4 | initializer prose 6 vs generated 5 (pre-existing grouping loss) | fixed `e5082c4` (initializers and types regrouped); qualified R7 |
| rv-compiler2 (boundary) | forwarded `dyn A + B` behind `&` ambiguous `+` (pre-existing) | fixed by `e5082c4`; regression in `forwarded_types.rs` |
| rv-compiler3 1 | `&VALUE` fragment treated as a type in expression positions (pre-existing) | fixed `0670240`; qualified R8 |
| rv-compiler4 1 | type fragment in a turbofish parenthesized (`unused_parens`, introduced by `0670240`) | fixed `1dad499`; qualified R9 |
| rv-compiler5 1 | `<<` taken for an opening type bracket (value change, introduced by `1dad499`) | fixed `c69b9b4` (no guessing of type positions in expressions); qualified R11 |
| rv-compiler5 2 | type behind `as *const` parenthesized (`unused_parens`) | fixed `c69b9b4` (added parentheses at the call-site span); qualified R10 |
| rv-final-sol R1 | forwarded path heading a struct literal parenthesized (compatibility regression) | fixed `c2cf3a9` (a path stays bare); qualified R12 |
| rv-final-sol R2 | workflow guard accepted `set -n` / `set -o noexec` in the gate step | fixed `c2cf3a9` (only safe `set` options); qualified R13 |
| rv-final-sol R3 | changelog lost its `## 4.1.0` heading | fixed `c2cf3a9` |
| rv-compiler6 1 | forwarded lifetime parenthesized in an initializer (since `e5082c4`) | fixed `033e08e` (only expression fragments parenthesized); qualified R14 |
| trials | `&HeaderValue` missing from the `IntoValue` list; `status()` None for Metadata unstated | fixed `c621b6c` |
| trial-opus | unused `Json` import warning undocumented; no Config rationale; §11 table row; README version (snapshot predates bump); guides cite `v4/SEMANTICS.md` | not changed: lint behaviour of Rust, design rationale, covered by §11.1, already fixed in `bd5d25e`, existing design |
| docs lane | `v4/SEMANTICS.md` HTAB wording, caller-guide excerpt comment, `timing_plan.sh` toolchain | fixed `7a2819a` |
| tooling lane | `consumers.sh` lost a FAIL line under `set -e` (pre-existing) | fixed `baa4204` |
| rv-stage5 | numeric decision triggers suggested | adopted in [PERF.md](PERF.md) |
| rv-stage5 (side note) | conservative sensitive promotion of a text value reading a native key (baseline) | no change: errs toward redaction, documented behaviour |
