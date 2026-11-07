# Model and session ledger (SDK50)

Requested settings are what the coordinator asked for. Observed settings come only from harness
metadata: for Codex sessions the rollout file's first `turn_context` (`model`, `effort`) of the
thread named in the launch output; for Claude subagents the transcript
`~/.claude/projects/<project>/<session>/subagents/agent-<id>.jsonl` (`params.model`,
`params.output_config.effort` of every request, `message.model` of every response). Daybreak:
`~/.codex/config.toml` sets `daybreak = true`, but `codex exec` reports "Daybreak is unavailable for
model …" for `gpt-6.1-sol`, `gpt-6.1-terra` and `gpt-6-astra` (probed 2026-11-06 23:07); every
session below runs with `-c daybreak=false`. No Daybreak capability was used. Codex
`workspace-write` keeps `.git` read-only: the coordinator commits Codex work and says so in the
commit. Sessions without `network_access` cannot bind loopback sockets; the coordinator re-ran
their socket tests outside the sandbox (stated in each commit).

The coordinator is this Claude Code session (`claude-opus-5-5`; session `cb65e1e7-4f76-4dce-b2a6-707218f97250`).

| Role | Requested | Observed | Sandbox | Thread / agent | Start commit | Time (CET) |
|---|---|---|---|---|---|---|
| p0-inventory-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b2ec-21e0-7502-813d-b7a358cfeb20 | 5c3ef5f6 | 2026-11-06 23:13 – 23:23 |
| p0-docs-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write | 01a1b2f9-0dbd-7122-9ce1-71d4325c17d2 | 5c3ef5f6 | 2026-11-06 23:27 – 23:32 |
| p1-codecs-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write | 01a1b2f9-0dbd-7ee0-b69d-fd4e6f3bd70b | 5c3ef5f6 | 2026-11-06 23:27 – 23:31 |
| p1-link-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write | 01a1b2f9-0dbb-7b71-9fe0-b93d482b69b4 | 5c3ef5f6 | 2026-11-06 23:27 – 23:32 |
| p1-predict-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | read-only | 01a1b308-91e0-7c61-8cb9-957cfc45df23 | b0896b87 | 2026-11-06 23:44 – 23:44 |
| p1-review-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b308-91e0-7853-8ecf-a67a03e158db | b0896b87 | 2026-11-06 23:44 – 23:48 |
| p1-f2-adoption-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write | 01a1b309-94e6-7b60-8a69-a7418a712f54 | b0896b87 | 2026-11-06 23:45 – 23:50 |
| p1-retry-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write | 01a1b30c-b677-70e3-a932-bd57a752978e | 9b4fd287 | 2026-11-06 23:48 – 00:03 |
| p1-firstuse-desk-opus (first-use probe, docs only) | Claude subagent, xhigh | `claude-opus-5-5` sent ×34 with effort `xhigh`; answered `claude-opus-5-5` ×63 | Claude Code (same permissions) | agent `afb26b83…` | `b0896b87` (frozen worktree) | 2026-11-06 23:45 – 23:52 |
| p1-pagination-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write + network | 01a1b321-1531-7430-9cdf-c3b2319f66f6 | `54d3e494` | 2026-11-07 00:10 – 00:13 |
| p1-edit-k8s-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write | 01a1b327-e45a-7031-8544-8e8003f49ef5 | `56a16ee3` | 2026-11-07 00:18 – 00:21 |
| p2-auth-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write | 01a1b326-9759-7aa0-8b17-bbe247f8f119 | `56a16ee3` | 2026-11-07 00:16 – 00:27 |
| w1-sdk21-author-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write | 01a1b32b-8448-7653-8ec4-72e9a23a075c | `56a16ee3` | 2026-11-07 00:22 – 00:31 |
| w1-sdk22-author-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write | 01a1b32b-8448-7c53-9452-a1641e9f36b2 | `56a16ee3` | 2026-11-07 00:22 – 00:24 |
| w1-sdk23-author-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write | 01a1b32b-8447-7773-ad95-d1e74102409d | `56a16ee3` | 2026-11-07 00:22 – 00:36 |
| w1-sdk25-author-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write | 01a1b32b-e78e-7a90-b747-b07573d33ae7 | `56a16ee3` | 2026-11-07 00:22 – 00:34 |
| w1-sdk22-consumer-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write | 01a1b32e-1b21-7d40-994c-f86dc3bc7b5c | `0c8444d6` | 2026-11-07 00:25 – 00:31 |
| w1-sdk21-consumer-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write | 01a1b334-4b37-7311-a4dc-0e19fc88b4b7 | `df9ba9e4` | 2026-11-07 00:31 – 00:32 |
| w1-sdk21-maintainer-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write | 01a1b335-e7bc-7f70-8453-ce74141bdda7 | `ba6ac11b` | 2026-11-07 00:33 – 00:37 |
| w1-sdk25-consumer-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write | 01a1b337-895f-7813-afb0-4244ea72c40c | `4f3cc65f` | 2026-11-07 00:35 – 00:37 |
| w1-sdk25-maintainer-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write | 01a1b339-8df6-78e0-aed0-6350956f5d55 | `e1995ff7` | 2026-11-07 00:37 – 00:42 |
| w1-sdk23-maintainer-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write | 01a1b344-c628-78d1-ba5e-b0927d07cd82 | `9f59f9a6` | 2026-11-07 00:49 – 00:55 |
| w1-sdk24-consumer-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write | 01a1b344-c626-7453-a1f6-b335e89747ed | `d188ef77` | 2026-11-07 00:49 – 00:55 |
| p2-ratelimit-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write | 01a1b345-66b1-7d23-99f3-7bbe3647523f | `f4b4129f` | 2026-11-07 00:50 – 00:56 |
| w1-sdk24-maintainer-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write | 01a1b34b-afee-7dc0-afaa-89612097ac99 | `9fbd2aca` | 2026-11-07 00:57 – 00:59 |
| b12-x-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write | 01a1b358-9659-7192-a1fb-aa8291f4d1cf | `8683ef33` | 2026-11-07 01:11 – 01:18 |
| b12-y-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write | 01a1b358-9659-7681-82d3-01e65443f54d | `8683ef33` | 2026-11-07 01:11 – 01:22 |
| w1-sdk24-author-opus (Discord) | Claude subagent, xhigh | `claude-opus-5-5` sent ×88 with effort `xhigh`; answered `claude-opus-5-5` ×166 | Claude Code (same permissions) | agent `ae8cac72…` | W1 frozen `56a16ee3` lineage | 2026-11-07 00:22 – 00:48 |
| w1-sdk22-maintainer-opus (NASA) | Claude subagent, xhigh | `claude-opus-5-5` sent ×81 with effort `xhigh`; answered `claude-opus-5-5` ×150 | Claude Code (same permissions) | agent `ac6c0c60…` | W1 frozen `56a16ee3` lineage | 2026-11-07 00:31 – 00:48 |
| w1-sdk23-consumer-opus (USGS, docs only) | Claude subagent, xhigh | `claude-opus-5-5` sent ×48 with effort `xhigh`; answered `claude-opus-5-5` ×89 | Claude Code (same permissions) | agent `a59ab30f…` | W1 frozen `56a16ee3` lineage | 2026-11-07 00:36 – 00:49 |
| b12-review-sol (review of `8dc53558..d3808b29`) | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b367-e8e9-7410-b51e-fb04b3a82af2 | `d3808b29` | 2026-11-07 01:28 – 01:32 |
| b12-pairs-adopt-terra (`query(..)` adoption) | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write + network | 01a1b368-e6a5-7b01-8237-4a22a37ce4fd | `d3808b29` | 2026-11-07 01:29 – 01:31 |
| b12-ratefix-astra (ratelimit review fixes) | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write + network | 01a1b36e-7d18-77a0-b826-53877344ebd5 | `e169ad41` | 2026-11-07 01:35 – 01:42 |
| b12-review2-sol (review of `d3808b29..78b1ff77`) | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b376-476d-7992-a4c9-a82b761f416e | `78b1ff77` | 2026-11-07 01:43 – 01:47 |
| w2-sdk26-author-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write + network | 01a1b385-57c0-74a2-a10d-e465e71ed6d3 | `8c27cd1a` | 2026-11-07 02:00 – 02:02 |
| w2-sdk27-author-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write + network | 01a1b385-57c0-79e0-b22a-3120ee0ff521 | `8c27cd1a` | 2026-11-07 02:00 – 02:13 |
| w2-sdk28-author-opus | Opus xhigh (Agent subagent) | claude-opus-5-5, effort xhigh (194 responses) | Agent tool | agent a45e8d75aaed60100 | `8c27cd1a` | 2026-11-07 02:00 – 02:26 |
| w2-sdk29-author-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write + network | 01a1b385-57c0-7441-ba72-b25448ec6534 | `8c27cd1a` | 2026-11-07 02:00 – 02:10 |
| w2-sdk30-author-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write + network | 01a1b385-57c0-7553-9244-1d5d9372eca4 | `8c27cd1a` | 2026-11-07 02:00 – 02:03 |
| w2-sdk26-consumer-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write + network | 01a1b388-47df-7e11-ad46-95fc61c6d4c8 | `4f29a817` | 2026-11-07 02:03 – 02:09 |
| w2-sdk26-maintainer-opus | Opus xhigh (Agent subagent) | claude-opus-5-5, effort xhigh (106 responses) | Agent tool | agent ae34ab2768a724e12 | `75139f95` | 2026-11-07 02:10 – 02:22 |
| w2-sdk29-consumer-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write + network | 01a1b38f-ea39-7262-8b4f-c8d2a6d34118 | `8a8d5209` | 2026-11-07 02:11 – 02:13 |
| w2-sdk30-consumer-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write + network | 01a1b38f-39e3-7130-9f1e-ef797a19478d | `a5e03c44` | 2026-11-07 02:11 – 02:17 |
| w2-sdk27-consumer-opus | Opus xhigh (Agent subagent) | claude-opus-5-5, effort xhigh (119 responses) | Agent tool | agent a3e3a69b1a573f89e | `80cd6cf4` | 2026-11-07 02:13 – 02:27 |
| w2-sdk30-maintainer-opus | Opus xhigh (Agent subagent) | claude-opus-5-5, effort xhigh (126 responses) | Agent tool | agent ab9cd6bff3343cea0 | `a171f377` | 2026-11-07 02:19 – 02:33 |
| w2-sdk29-maintainer-astra | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write + network | 01a1b396-6f66-78f0-bf27-90cba63ed49a | `8a8befa6` | 2026-11-07 02:19 – 02:24 |
| w2-sdk28-consumer-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write + network | 01a1b39e-2487-7250-8fad-ffc47b3667af | `a3764fdf` | 2026-11-07 02:27 – 02:34 |
| w2-sdk27-maintainer-sol | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write + network | 01a1b39f-11fa-7810-87bd-747d40aa0ec0 | `497bf896` | 2026-11-07 02:28 – 02:33 |
| w2-sdk28-maintainer-terra | gpt-6.1-terra xhigh | gpt-6.1-terra xhigh | workspace-write + network | 01a1b3a5-458e-7b40-8474-d7afd1193a95 | `7c2479ee` | 2026-11-07 02:35 – 02:36 |
| b23-oldadopt-astra (earlier SDKs adopt `send -> ..` and `field`) | gpt-6-astra xhigh | gpt-6-astra xhigh | workspace-write + network | 01a1b3b0-a780-7892-84cd-6c3ca6a969e1 | `162fc869` | 2026-11-07 02:47 – 02:54 |
| b23-review-sol (review of `c06cef7b..53096208`) | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b3b0-ef24-75a1-989e-0c980a1f3820 | `162fc869` | 2026-11-07 02:47 – 02:54 |
| b23-w2adopt-sol (wave-2 SDK adoption, repeated-key guard) | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | workspace-write + network | 01a1b3b0-a780-7500-a40d-9bf22b659a7f | `162fc869` | 2026-11-07 02:47 – 02:56 |
| b23-review2-sol (review of `95ac36de`, `0e4a245e`, `3ff63633..4870f864`) | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b412-c896-7b03-8a48-04b5f83dc07a | `4870f864` | 2026-11-07 04:34 – 04:38 |
| b23-review3-sol (review of `20517134`) | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b424-7cb1-7d71-87b8-42c8ebbee37a | `20517134` | 2026-11-07 04:54 – 04:58 |
| b23-review4-sol (review of `c3517aaf`) | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b430-42d1-79a0-971d-341b195fc2f5 | `c3517aaf` | 2026-11-07 05:07 – 05:10 |
| b23-review5-sol (review of `435ad897`) | gpt-6.1-sol xhigh | gpt-6.1-sol xhigh | read-only | 01a1b437-d735-7a42-9912-aa2b36756a65 | `435ad897` | 2026-11-07 05:15 – 05:20 |

Opus subagent times are converted from the transcripts' UTC timestamps. Loopback access, read from
each rollout's first `turn_context.sandbox_policy.network_access`: true for every Wave-1 Codex
role and every `p1-edit`, `p1-retry`, `p1-pagination`, `p2-*` and `b12-*` lane; false for
`p0-docs`, `p1-codecs`, `p1-f2-adoption`, `p1-link` (their socket tests were re-run by the
coordinator); read-only roles have no network policy.

**Routing from Wave 3 on (owner instruction, 2026-11-07 ~04:30 CET):** Codex roles use only
`gpt-6.1-sol` at requested xhigh; `gpt-6.1-terra` and `gpt-6-astra` are no longer requested. Every
Terra/Astra row above was run before that instruction. The rotation becomes two families (Sol and
Opus): see `DECISIONS.md` (R-1).
