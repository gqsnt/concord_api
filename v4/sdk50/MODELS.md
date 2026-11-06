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
| p1-pagination-terra | gpt-6.1-terra xhigh | (in progress) | workspace-write + network | — | `54d3e494` | 2026-11-07 00:12 – |
