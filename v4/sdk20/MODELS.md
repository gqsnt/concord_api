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
| Phase A: Salesforce + dx03 migration, A5 | Opus subagent | UNOBSERVED (inherits) | `worktree-agent-aef060da…` | `8efd185a` (merged) | did not design the boundary |
| Phase A: GitHub/Stripe/Shopify metadata migration | Opus subagent | UNOBSERVED (inherits) | `worktree-agent-a05d3d34…` | `3abbbba9` (merged); pagination follow-up pending | |
| Phase A: Kubernetes watch + multipart migration | Opus subagent | UNOBSERVED (inherits) | `worktree-agent-accc2cb9…` | `7520fb8d` (merged) | |
| Gate A independent packaged consumer (`checks/consumers/phase-a-dx`) | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh`, `workspace-write … (network access enabled)` (session `01a1ae4a…`) | `work/wt-gatea-consumer` `sdk20/gatea-consumer` at `c16e6faf` | files committed by the coordinator in `8bcd1015` | outcome-only brief: `work/sessions/gatea-consumer.prompt.md` |
| Phase A library review (diff `a95fb2fb..4bcd8980`, crates/checks/docs) | `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol`, `xhigh`, `read-only` (session `01a1ae51…`) | `work/rv-phase-a` (detached `4bcd8980`) | ACCEPT_WITH_FIXES, 3 findings (SOL-A-1..3), fixed in `4fc5e26e`'s parent | prompt `work/sessions/review-phase-a-core.prompt.md` |
| Phase A second review (diff `4bcd8980..756f408e`: later library additions + five SDK migrations) | `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol`, `xhigh`, `read-only` (session `01a1ae61…`) | `work/rv-phase-a2` (detached `756f408e`) | ACCEPT_WITH_FIXES, 1 finding (SOL-B-1), fixed | prompt `work/sessions/review-phase-a-delta.prompt.md` |
| W1 author SDK01 Nager | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1ae69-4eb8…`) | `work/w1-sdk01-nager-holidays` | pending | capsule `tasks/wave1/SDK01-*-author.md` |
| W1 author SDK02 PokéAPI | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh`, workspace-write + `work/target-b`, network (session `01a1ae69-4eba…`) | `work/w1-sdk02-pokeapi` | pending | |
| W1 author SDK03 Twilio | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1ae69-4eb9…`) | `work/w1-sdk03-twilio` | pending | |
| W1 author SDK04 S3 | Opus subagent | UNOBSERVED (inherits) | `work/w1-sdk04-amazon-s3` | pending | |
