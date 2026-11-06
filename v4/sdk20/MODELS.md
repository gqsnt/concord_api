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
| W1 author SDK01 Nager | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1ae69-4eb8…`) | `work/w1-sdk01-nager-holidays` | `38dbaa29` | capsule `tasks/wave1/SDK01-*-author.md` |
| W1 author SDK02 PokéAPI | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh`, workspace-write + `work/target-b`, network (session `01a1ae69-4eba…`) | `work/w1-sdk02-pokeapi` | `2079f14b` | |
| W1 author SDK03 Twilio | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1ae69-4eb9…`) | `work/w1-sdk03-twilio` | `b1e00c12` | |
| W1 author SDK04 S3 | Opus subagent | UNOBSERVED (inherits) | `work/w1-sdk04-amazon-s3` | `b30d9994` | |
| W1 consumer SDK01 Nager | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh` (session `01a1ae72-9d45…`) | same worktree | `9b99ef07` | |
| W1 consumer SDK02 PokéAPI | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh` (session `01a1ae72-9d44…`) | same | `729a8dda` | |
| W1 consumer SDK03 Twilio | Opus subagent | UNOBSERVED (inherits) | same | `96ff9b2b` | |
| W1 consumer SDK04 S3 | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh` (session `01a1ae95-6f18…`) | same | `715538c6` | |
| W1 maintainer SDK01 Nager | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh` (session `01a1ae78-1be4…`) | same | `2e23393b` | |
| W1 maintainer SDK02 PokéAPI | Opus subagent | UNOBSERVED (the agent quoted its system prompt's model id; not harness metadata) | same | `5f72d206` | |
| W1 maintainer SDK03 Twilio | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh` (session `01a1ae7d-9e31…`) | same | `497367eb` | |
| W1 maintainer SDK04 S3 | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh` (session `01a1ae9c-6a24…`) | same | `41e24beb` | |
| Review of between-wave changes after wave 1 (`94d13527..6a9dcb19`, library/test support/docs) | `gpt-6-astra`, xhigh, read-only | `gpt-6-astra`, `xhigh`, `read-only` (session `01a1ae9f-2c62…`) | `work/rv-w1` (detached `6a9dcb19`) | ACCEPT_WITH_FIXES, AST-W1-1 fixed `337da135` | |
