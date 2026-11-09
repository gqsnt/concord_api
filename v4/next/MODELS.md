# Models — requested vs observed

Observed values come from harness metadata only: Codex rollout `turn_context`
(`~/.codex/sessions/…/rollout-*-<thread>.jsonl`), the Claude Code process flags, or the Agent
tool's result. Never from a profile name or a model's self-report.

| Role / item | Requested | Observed | Source | Duration | Result |
|---|---|---|---|---|---|
| Coordinator | Claude `claude-opus-5-5`, xhigh | `--model=claude-opus-5-5 --effort=xhigh` | process flags of pid 3610714 (desktop app) | — | — |
| P3.1 friction catalogue (Claude subagent, general-purpose) | inherits coordinator (`claude-opus-5-5`) | not reported by the Agent result | Agent tool result | 1,896 s, 56 tool uses, 346,480 tokens | work/inventory/FRICTION.md (190 entries) |
| P1.6 review round 1 `p1-merge-review-1` on `b84335ff` | Codex `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol` / `xhigh` / `read-only` | rollout `01a1bf74-060f-7282-bc87-49ed0635b283` turn_context, 08:36:56Z | 283 s | ACCEPT_WITH_FIXES (1 blocking, 1 non-blocking) |
| P1.6 review round 2 `p1-merge-review-2` on `ba10189b` | Codex `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol` / `xhigh` / `read-only` | rollout `01a1bf79-e484-7373-aa17-cc246db2111e` turn_context, 08:43:20Z | 52 s | ACCEPT (both round-1 findings fixed) |
| P3.2 idea lane AB (Codex) | Codex `gpt-6.1-sol`, xhigh, workspace-write | `gpt-6.1-sol` / `xhigh` / `workspace-write` | rollout `01a1bf7f-4808-7463-8141-4507e7b2f772` | 774 s | work/ideas/AB-codex.md |
| P3.2 idea lane CG (Codex) | same | `gpt-6.1-sol` / `xhigh` / `workspace-write` | rollout `01a1bf7f-480d-71e3-b352-fac9bd496374` | 750 s | work/ideas/CG-codex.md |
| P3.2 idea lane DJM (Codex) | same | `gpt-6.1-sol` / `xhigh` / `workspace-write` | rollout `01a1bf7f-4809-73d1-af4b-454ddb056e7b` | 962 s | work/ideas/DJM-codex.md |
| P3.2 idea lane EF (Codex) | same | `gpt-6.1-sol` / `xhigh` / `workspace-write` | rollout `01a1bf7f-4809-7090-984f-7ae34547c60a` | 759 s | work/ideas/EF-codex.md |
| P3.2 idea lane HKL (Codex) | same | `gpt-6.1-sol` / `xhigh` / `workspace-write` | rollout `01a1bf7f-4809-77c3-b9d0-06154922cf0b` | 623 s | work/ideas/HKL-codex.md |
| P3.2 idea lane IN (Codex) | same | `gpt-6.1-sol` / `xhigh` / `workspace-write` | rollout `01a1bf7f-480c-7290-bcd3-8ce34928c9a4` | 489 s | work/ideas/IN-codex.md |
| P3.2 idea lane AB (Claude subagent, general-purpose) | inherits coordinator (`claude-opus-5-5`) | not reported: the Agent result gives tokens and duration only | Agent tool result | 1,354 s, 99 tool uses, 450,633 tokens | work/ideas/AB-claude.md (10 designs) |
| P3.2 idea lane CG (Claude subagent) | same | not reported | Agent tool result | 1,674 s, 121 tool uses, 496,602 tokens | work/ideas/CG-claude.md (9 designs) |
| P3.2 idea lane DJM (Claude subagent) | same | not reported | Agent tool result | 1,550 s, 119 tool uses, 413,521 tokens | work/ideas/DJM-claude.md (13 designs) |
| P3.2 idea lane EF (Claude subagent) | same | not reported | Agent tool result | 1,258 s, 102 tool uses, 441,307 tokens | work/ideas/EF-claude.md (10 designs) |
| P3.2 idea lane HKL (Claude subagent) | same | not reported | Agent tool result | 1,662 s, 106 tool uses, 413,339 tokens | work/ideas/HKL-claude.md (13 designs) |
| P3.2 idea lane IN (Claude subagent) | same | not reported | Agent tool result | 1,510 s, 140 tool uses, 394,675 tokens | work/ideas/IN-claude.md (10 designs) |
| P3.1 inventory review round 1 `p3-inventory-review-1` | Codex `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol` / `xhigh` / `read-only` | rollout `01a1bf99-e88a-7c30-a21b-d6647974a054` | 406 s | REJECT: classifier 10/60 (16.7%, seed 20261109); friction A4, D3, O7 false, K2 inconsistent |
| P3.1 inventory review round 2 `p3-inventory-review-2` | Codex `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol` / `xhigh` / `read-only` | rollout `01a1bfa2-6ee5-7b42-9807-24d3beda572a` | — | REJECT: classifier 9/60 (15.0%, seed 20261110), sub-category 0/60; friction corrections all ok. Last round: recorded limit |

From 12:36 (owner instruction): worker lanes run on Codex `gpt-6.1-sol` xhigh through
`work/tools/codex_lane.sh` (outer bubblewrap jail, `-s danger-full-access` inside); requested and
observed values are written per lane to `work/lanes/<role>/meta.json` from the rollout
`turn_context`. Smoke lane `smoke-jail`: observed `gpt-6.1-sol` / `xhigh` / `danger-full-access`.
Spike and direction lanes before 12:36 were Claude subagents (inherit the coordinator model; the
Agent result reports no model id).
