# Model and session ledger

Requested settings are what the coordinator asked for. Observed settings come only from harness
metadata: the Claude Code environment statement (coordinator) and the Codex session header lines
`model:`, `reasoning effort:`, `sandbox:`. The Claude `Agent` tool is called without a model
override; it reports no per-agent model or effort, so those cells say `UNOBSERVED (inherits)`.

Families available on this host (`~/.codex/models_cache.json`, Codex CLI 0.166.0): `gpt-6.1-sol`,
`gpt-6.1-terra`, `gpt-6-astra` (no 6.1 Astra is offered), each with `xhigh`. "Opus" work runs as
Claude Code subagents. Codex `workspace-write` keeps `.git` read-only: Codex sessions cannot
commit; the coordinator commits their files and records it.

| Role / task | Requested | Observed (source) | Worktree / branch | Result commit | Notes |
|---|---|---|---|---|---|
| Coordinator, core/macros/extras integrator, Phase A tests and docs | Claude Opus, xhigh | `claude-opus-5-5` (environment statement); effort not exposed | `work/implementation` `concord-dx-sdk20` | — | |
| Phase A: pagination facade + lossless reports | Opus subagent | UNOBSERVED (inherits) | `worktree-agent-a3d81d9b…` | `708f405f` (merged) | crate-only ownership |
| Phase A: Salesforce + dx03 migration, A5 | Opus subagent | UNOBSERVED (inherits) | agent worktree | pending | |
| Phase A: GitHub/Stripe/Shopify metadata migration | Opus subagent | UNOBSERVED (inherits) | agent worktree | pending | |
| Phase A: Kubernetes watch + multipart migration | Opus subagent | UNOBSERVED (inherits) | agent worktree | pending | |
| Gate A independent packaged consumer (`checks/consumers/phase-a-dx`) | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh`, `workspace-write … (network access enabled)` (session `01a1ae4a…`) | `work/wt-gatea-consumer` `sdk20/gatea-consumer` at `c16e6faf` | pending | outcome-only brief: `work/sessions/gatea-consumer.prompt.md` |
