# Model runs (requested vs observed)

Local harnesses: Claude Code subagents (Agent tool, `model` = opus/sonnet; transcripts under
`~/.claude/projects/-home-canarit-projects-concord-api/<session>/subagents/`); Codex CLI
`codex-cli 0.166.0` via `/home/canarit/projects/concord_v4_finish_work/tools/run_codex_review.py` (explicit `--model`, `--effort`; the model
must be in `~/.codex/models_cache.json`, which lists `gpt-6.1-sol` and `gpt-6.1-terra` with
low/medium/high/xhigh). Observed settings: Codex — `turn_context` of the run's own session log
(`/home/canarit/projects/concord_v4_finish_work/tools/observe_settings.py`); Claude — `message.model` of every assistant record in the
subagent transcript. Claude subagent effort is not separately configurable by the Agent tool and
is not observable in the transcript (recorded as NOT_OBSERVABLE).

| Run | Role | Requested | Observed | Source scope | Status |
|---|---|---|---|---|---|
| r1-opus | Designer O, blind | Claude Opus | claude-opus-5-5 (108/108 assistant records); tools used: Bash 60, Write 1, mark_chapter 1 | snapshot r1-opus (625ac49) + brief | completed 15:39:44→16:12:27Z, tracked sources unchanged (git status clean); 36 probe tests + 6 expected-failure crates; frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r1/opus.md` |
| r1-sonnet | Designer N, blind | Claude Sonnet | claude-sonnet-5-5 (193/193 assistant records); an `advisor` tool backed by claude-opus-5-5 was available in the session and was invoked 0 times | snapshot r1-sonnet | completed 15:39:44→16:14:10Z; prototype diff + proposal frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r1/sonnet.md`, `/home/canarit/projects/concord_v4_finish_work/proposals/r1/sonnet.prototype.diff` |
| r1-sol-xhigh | Designer S, blind | gpt-6.1-sol / xhigh | gpt-6.1-sol / xhigh, workspace-write, network on (turn_context) | snapshot r1-sol-xhigh | completed 15:39:44→16:07:29Z, production source unchanged; frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r1/sol-xhigh.md` |
| r1-terra-xhigh | Designer T, blind | gpt-6.1-terra / xhigh | gpt-6.1-terra / xhigh, workspace-write, network on (turn_context) | snapshot r1-terra-xhigh | completed 15:39:45→15:42:31Z, source unchanged; frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r1/terra-xhigh.md` |
| r1-sol-medium | Builder M: pragmatic assessment, corpus, control | gpt-6.1-sol / medium | gpt-6.1-sol / medium, workspace-write, network on (turn_context) | snapshot r1-sol-medium | completed 15:39:45→15:51:38Z, production source unchanged; PRAGMATIC frozen 15:40:16Z; corpus 33/35 (18/18 operations); frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r1/sol-medium/` |

Note (r1): the private registry lacked `criterion` and reqwest's multipart dependencies, so the Sol XHigh and builder runs could not run the full workspace suite or the multipart helper offline; refreshed with `cargo fetch --locked` from 625ac49 at 16:10Z for later rounds.

## Round 2 — executable candidates and blind cross-review

| Run | Role | Requested | Observed | Source scope | Status |
|---|---|---|---|---|---|
| r2-impl-a | Implement Candidate A (uniform statements; authors Opus/Sonnet/Terra) | gpt-6.1-sol / medium | gpt-6.1-sol / medium (turn_context) | snapshot r2-impl-a (625ac49 + corpus) | 16:19:07→16:29:38Z; patch +162/−113 in macro only; corpus 36/2 = control; frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r2/impl-a.*` |
| r2-impl-b | Implement Candidate B (nodes; author Sol XHigh) | Claude Opus | claude-opus-5-5 (214/214 assistant records) | snapshot r2-impl-b | 16:19:07→16:47:31Z; patch +912/−608 (macro + repr.rs wording); corpus 36/2 = control; report delivered as message (file write refused by harness), frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r2/impl-b.*` |
| xr-terra | Blind cross-review of X/Y/Z | gpt-6.1-terra / xhigh | gpt-6.1-terra / xhigh (turn_context) | snapshot of `review-r2-src` 429ba31 | 16:50:20→16:53:16Z, packet unchanged; ranking X > Z > Y (labelled taste); frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r2/xreview/terra.*` |
| xr-sol | Blind cross-review of X/Y/Z | gpt-6.1-sol / xhigh | gpt-6.1-sol / xhigh (turn_context) | same packet | 16:50:20→16:59:59Z, packet unchanged (1,147 files hash-checked); ranking X > Z > Y (labelled taste); frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r2/xreview/sol.*` |
| xr-sonnet | Blind cross-review (author/caller lens) | Claude Sonnet | claude-sonnet-5-5 (all assistant records) | same packet | completed 17:0xZ; ranking Z > Y > X with compiler-fact evidence; report delivered as message (file write refused); frozen `/home/canarit/projects/concord_v4_finish_work/proposals/r2/xreview/sonnet.md` |

| migrate | Migrate tests, examples, checks, benches and `gen.py` to the selected surface (not a designer of it) | Claude Sonnet | claude-sonnet-5-5 (166/166 assistant records), 0 advisor calls | worktree `concord_v4_migrate`, branch `concord-v4-final-migrate` from `33b1f0f` | 17:07:15→17:20:59Z; commits `ab5453d`, `2503090`, `25c941e`, merged as `70f24a8` after the coordinator's review |

Packet mapping (not given to reviewers): X = Candidate B, Y = control (current grammar), Z = Candidate A.

## Round 3 — fresh use (docs first) on the integrated candidate

Brief: `/home/canarit/projects/concord_v4_finish_work/briefs/FRESH_USE.md` (MODEL_BRIEFS §5 and
the ten SCENARIOS §5 tasks over a fictional mostly-JSON API; no library source, macro expansion or
design records unless blocked). Each participant had its own snapshot of `389392f`.

| Run | Role | Requested | Observed | Source scope | Status |
|---|---|---|---|---|---|
| fu-terra | Fresh user, author + caller | gpt-6.1-terra / xhigh | gpt-6.1-terra / xhigh, workspace-write, network on (turn_context) | snapshot fu-terra (`389392f`); 28 commands; no transition to library source, tests, expansions or `v4/` | 17:33:57→17:36:26Z, tracked files unchanged; all ten tasks run with loopback tests; report `trials/terra-xhigh.md`, archive `/home/canarit/projects/concord_v4_finish_work/evidence/fresh-use/terra/` (SHA256SUMS) |
| fu-sonnet | Fresh user, author + caller | Claude Sonnet | claude-sonnet-5-5 (119/119 assistant records), 0 advisor calls; 63 tool calls (59 Bash), none on library source, expansions or `v4/` | snapshot fu-sonnet (`389392f`) | 17:33:57→17:48:24Z, tracked files unchanged; all ten tasks with loopback tests and expected-failure probes; report returned as a message (file write refused by the harness), `trials/sonnet.md`, archive `/home/canarit/projects/concord_v4_finish_work/evidence/fresh-use/sonnet/` (SHA256SUMS) |

## Round 4 — review-pack reconciliation

The owner supplied an independent review pack (`concord_next_update_direction(2).zip`, written for
`6bc5032`, nothing executed) with AGENT_ADDENDUM.md and DISPOSITIONS.md after the fresh-use trials
were frozen; no designer or fresh-use participant saw it. Reconciliation: `RECONCILIATION.md`.

| Run | Role | Requested | Observed | Source scope | Status |
|---|---|---|---|---|---|
| fu-delta-terra | Fresh user, three tasks on the changed journeys (not exposed to the pack) | gpt-6.1-terra / xhigh | gpt-6.1-terra / xhigh (turn_context) | snapshot trial-delta-terra (`ab3cb85`); brief `/home/canarit/projects/concord_v4_finish_work/briefs/FRESH_USE_DELTA.md` | 21:15:27→21:17:04Z, tracked files unchanged; all tasks run on loopback, no wrong prediction; report `trials/terra-delta.md` |
| compat | Fixture builder: the eight compatibility cases (not a designer or reviewer) | gpt-6.1-sol / medium | gpt-6.1-sol / medium, workspace-write, network on (turn_context) | snapshot compat-sol-medium (`0e56a65`); brief `/home/canarit/projects/concord_v4_finish_work/briefs/COMPAT_BUILD.md` (written by the coordinator, not the pack) | 19:50:37→19:52:55Z; wrote `examples/compat` + the workspace member and lock entry only; 8/8 cases, clippy, fmt, `+1.85` pass (its report); imported after the coordinator's review with one added case; report and receipt `/home/canarit/projects/concord_v4_finish_work/evidence/compat/` (SHA256SUMS) |

Frozen outputs and their SHA-256 sums: `/home/canarit/projects/concord_v4_finish_work/proposals/r1/FROZEN.sha256`, `/home/canarit/projects/concord_v4_finish_work/proposals/r2/FROZEN-a.sha256`, `/home/canarit/projects/concord_v4_finish_work/proposals/r2/FROZEN-b.sha256`, `/home/canarit/projects/concord_v4_finish_work/proposals/r2/xreview/FROZEN.sha256`.
