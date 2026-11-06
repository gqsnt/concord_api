# Model and session ledger

Requested settings are what the coordinator asked for. Observed settings come only from harness
metadata: the Codex session header lines `model:`, `reasoning effort:`, `sandbox:`, and for Claude
work the Claude Code transcripts (`~/.claude/projects/<project>/<session>.jsonl` and its
`subagents/agent-<id>.jsonl`): every `api-request` record's `params.model` and
`params.output_config.effort` (what the harness sent) and every assistant message's
`message.model` (what the API answered). The `Agent` tool is called without a model override.
Read on 2026-11-06 for all fourteen Opus subagents and the coordinator: every request is
`claude-opus-5-5` with effort `xhigh`, every response model `claude-opus-5-5`. The response does
not echo the effort, so effort is the harness-sent value. (Earlier revisions of this file marked
these cells unobserved, before these records were found.)

Families available on this host (`~/.codex/models_cache.json`, Codex CLI 0.166.0): `gpt-6.1-sol`,
`gpt-6.1-terra`, `gpt-6-astra` (no 6.1 Astra is offered), each with `xhigh`. "Opus" work runs as
Claude Code subagents. Codex `workspace-write` keeps `.git` read-only: Codex sessions cannot
commit; the coordinator commits their files and records it.

| Role / task | Requested | Observed (source) | Worktree / branch | Result commit | Notes |
|---|---|---|---|---|---|
| Coordinator, core/macros/extras integrator, Phase A tests and docs | Claude Opus, xhigh | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (session `ddfdad53…` transcript) | `work/implementation` `concord-dx-sdk20` | — | |
| Phase A: pagination facade + lossless reports | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a3d81d9b…`) | `worktree-agent-a3d81d9b…` | `708f405f` (merged) | crate-only ownership |
| Phase A: Salesforce + dx03 migration, A5 | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-aef060da…`) | `worktree-agent-aef060da…` | `8efd185a` (merged) | did not design the boundary |
| Phase A: GitHub/Stripe/Shopify metadata migration | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a05d3d34…`) | `worktree-agent-a05d3d34…` | `3abbbba9` (merged); pagination follow-up pending | |
| Phase A: Kubernetes watch + multipart migration | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-accc2cb9…`) | `worktree-agent-accc2cb9…` | `7520fb8d` (merged) | |
| Gate A independent packaged consumer (`checks/consumers/phase-a-dx`) | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh`, `workspace-write … (network access enabled)` (session `01a1ae4a…`) | `work/wt-gatea-consumer` `sdk20/gatea-consumer` at `c16e6faf` | files committed by the coordinator in `8bcd1015` | outcome-only brief: `work/sessions/gatea-consumer.prompt.md` |
| Phase A library review (diff `a95fb2fb..4bcd8980`, crates/checks/docs) | `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol`, `xhigh`, `read-only` (session `01a1ae51…`) | `work/rv-phase-a` (detached `4bcd8980`) | ACCEPT_WITH_FIXES, 3 findings (SOL-A-1..3), fixed in `4fc5e26e`'s parent | prompt `work/sessions/review-phase-a-core.prompt.md` |
| Phase A second review (diff `4bcd8980..756f408e`: later library additions + five SDK migrations) | `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol`, `xhigh`, `read-only` (session `01a1ae61…`) | `work/rv-phase-a2` (detached `756f408e`) | ACCEPT_WITH_FIXES, 1 finding (SOL-B-1), fixed | prompt `work/sessions/review-phase-a-delta.prompt.md` |
| W1 author SDK01 Nager | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1ae69-4eb8…`) | `work/w1-sdk01-nager-holidays` | `38dbaa29` | capsule `tasks/wave1/SDK01-*-author.md` |
| W1 author SDK02 PokéAPI | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh`, workspace-write + `work/target-b`, network (session `01a1ae69-4eba…`) | `work/w1-sdk02-pokeapi` | `2079f14b` | |
| W1 author SDK03 Twilio | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1ae69-4eb9…`) | `work/w1-sdk03-twilio` | `b1e00c12` | |
| W1 author SDK04 S3 | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-ae9c9d95…`) | `work/w1-sdk04-amazon-s3` | `b30d9994` | |
| W1 consumer SDK01 Nager | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh` (session `01a1ae72-9d45…`) | same worktree | `9b99ef07` | |
| W1 consumer SDK02 PokéAPI | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh` (session `01a1ae72-9d44…`) | same | `729a8dda` | |
| W1 consumer SDK03 Twilio | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a12ab7ad…`) | same | `96ff9b2b` | |
| W1 consumer SDK04 S3 | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh` (session `01a1ae95-6f18…`) | same | `715538c6` | |
| W1 maintainer SDK01 Nager | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh` (session `01a1ae78-1be4…`) | same | `2e23393b` | |
| W1 maintainer SDK02 PokéAPI | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-adb95ff1…`) | same | `5f72d206` | |
| W1 maintainer SDK03 Twilio | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh` (session `01a1ae7d-9e31…`) | same | `497367eb` | |
| W1 maintainer SDK04 S3 | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh` (session `01a1ae9c-6a24…`) | same | `41e24beb` | |
| Review of between-wave changes after wave 1 (`94d13527..6a9dcb19`, library/test support/docs) | `gpt-6-astra`, xhigh, read-only | `gpt-6-astra`, `xhigh`, `read-only` (session `01a1ae9f-2c62…`) | `work/rv-w1` (detached `6a9dcb19`) | ACCEPT_WITH_FIXES, AST-W1-1 fixed `337da135` | |
| W2 author SDK05 Frankfurter | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh` (session `01a1aec4-4657-79d0…`) | `work/w2-sdk05-frankfurter` | `d66eb228` | |
| W2 author SDK06 GitLab | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a3f05111…`) | `work/w2-sdk06-gitlab` | `debaefb7` | |
| W2 author SDK07 Slack | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh` (session `01a1aec4-4657-71a0…`) | `work/w2-sdk07-slack` | `ae8ddfb1` | |
| W2 author SDK08 OpenAI | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh` (session `01a1aec4-4657-7690…`) | `work/w2-sdk08-openai` | `8a659204` | |
| W2 consumer SDK05 | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a4d2bd8c…`) | same | `3aefaf5c` | |
| W2 consumer SDK06 | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh` (session `01a1aedc-227c…`) | same | `5d15bc2b` | |
| W2 consumer SDK07 | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh` (session `01a1aecf-9c4b-7753…`) | same | `31cad62d` | |
| W2 consumer SDK08 | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh` (session `01a1aecf-9c4b-7741…`) | same | `08f97864` | |
| W2 maintainer SDK05 | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh` (session `01a1aeda-3170…`) | same | `64a12a6e` | |
| W2 maintainer SDK06 | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh` (session `01a1aee4-2677…`) | same | `e1b813b5` | |
| W2 maintainer SDK07 | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh` (session `01a1aed6-b266…`) | same | `c844a878` | |
| W2 maintainer SDK08 | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a3dc4587…`) | same | `51d0eee3` | |
| Review of between-wave changes after wave 2 (`e04236ca..1f71432c`, library/docs) | `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol`, `xhigh`, `read-only` (session `01a1aee8-0314…`) | `work/rv-w2` (detached `1f71432c`) | ACCEPT_WITH_FIXES, SOL-W2-1 fixed | |
| Between waves 1-2: wave 1 SDK migration + consumer fixes | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a0b72bc7…`) | `worktree-agent-a0b72bc7…` | merged `8d434143` | |
| Between waves 2-3: wave 1 SDKs to send policy / follow | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a776f091…`) | `worktree-agent-a776f091…` | merged `0f096e63` | |
| Between waves 2-3: wave 2 SDKs to send policy / follow | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-aadef365…`) | `worktree-agent-aadef365…` | merged `91103d63` | |
| Between waves 2-3: S3 on a stateful signing send policy | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-aa6a8748…`) | `worktree-agent-aa6a8748…` | `38e6333d`, merged `bbbf6c97` | |
| Review of between-wave changes before wave 3 (`1f71432c..08df6614`) | `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol`, `xhigh`, `read-only` (session `01a1af0c-7a3c…`) | `work/rv-w23` (detached `08df6614`) | ACCEPT_WITH_FIXES, SOL-W23-1/2 fixed `610145e1` | |
| W3 author SDK09 ntfy | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a42bc1f8…`) | `work/w3-sdk09-ntfy` | `f255d4df` (4 unit + 21 protocol + 5 workflow + 5 doc) | |
| W3 author SDK10 Linear | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh`, workspace-write + `work/target-b`, network (session `01a1af1b-4f4c…`) | `work/w3-sdk10-linear` | `ed548bd5` (7 protocol + 4 workflow tests) | coordinator commit |
| W3 author SDK11 Dropbox | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1af1b-4f4d-71d1…`) | `work/w3-sdk11-dropbox` | `69692433` (18 protocol + 7 workflow + 2 doc); first delivery had a flaky test (1/8 full runs), same session resumed with the failure and fixed it (tests only); 10/10 coordinator runs | coordinator commit |
| W3 author SDK12 Google Drive | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh`, workspace-write + `work/target-b`, network (session `01a1af1b-4f4d-7413…`) | `work/w3-sdk12-google-drive` | `6d9d7fb9` (25 protocol + 4 workflow + 3 doc tests) | coordinator commit |
| W3 consumer SDK10 Linear | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh`, workspace-write + `work/target-b`, network (session `01a1af1d-9595…`) | same | `3ae13c1c` (18 first-use tests, no intervention) | coordinator commit |
| W3 maintainer SDK10 Linear | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a1153881…`) | same | `9466d1eb` (31 tests; one fact in 20 places / 15 files) | a search with a failed path filter printed 12 lines of FIRST_USE.md before its first attempt (recorded in MAINTENANCE.md) |
| W3 consumer SDK12 Google Drive | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh`, workspace-write + `work/target-b`, network (session `01a1af29-6b0c…`) | `work/w3-sdk12-google-drive` | `e8968c80` (5 first-use tests, no intervention) | coordinator commit |
| W3 consumer SDK11 Dropbox | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-a041ab48…`) | `work/w3-sdk11-dropbox` | `373362dc` (18 first-use tests, no intervention) | |
| W3 maintainer SDK12 Google Drive | `gpt-6-astra` xhigh | `gpt-6-astra`, `xhigh`, workspace-write + `work/target-b`, network (session `01a1af2b-3b13…`) | `work/w3-sdk12-google-drive` | `9bc413b0` (3 maintenance tests; rustdoc-only source edits), coordinator commit | |
| W3 maintainer SDK11 Dropbox | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1af36-4c81…`) | `work/w3-sdk11-dropbox` | `72249e49` (consuming selection methods + maintenance wire tests), coordinator commit | |
| W3 consumer SDK09 ntfy | `gpt-6.1-sol` xhigh | `gpt-6.1-sol`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1af36-e071…`) | `work/w3-sdk09-ntfy` | `bd7861d1` (12 first-use tests, no intervention), coordinator commit | |
| W3 maintainer SDK09 ntfy | `gpt-6.1-terra` xhigh | `gpt-6.1-terra`, `xhigh`, workspace-write + `work/target-a`, network (session `01a1af3c-8a6f…`) | `work/w3-sdk09-ntfy` | `92e32d43` (replay-boundary wire test; doc-only source edits), coordinator commit | |
| Between waves 3-4: R1 SSE reader + OpenAI migration | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-ad280ddd…`) | `worktree-agent-ad280ddd…` | `2ea817e1`, merged `92775757` | |
| Between waves 3-4: R2 multipart format + OpenAI/Drive migration | Opus subagent | `claude-opus-5-5`, effort `xhigh` sent; answered by `claude-opus-5-5` (`agent-aaa17b8c…`) | `worktree-agent-aaa17b8c…` | `238ca378`, merged `c72bb5d6` | |
| Between waves 3-4: M1 ntfy onto the SSE reader | Opus subagent | running (transcript read at completion) | `worktree-agent-ac82248d…` | | |
| Between waves 3-4: M2 seven SDKs onto `ResponseMeta` | Opus subagent | running (transcript read at completion) | `worktree-agent-a0d0a2ba…` | | |
| Review between waves 3-4: SSE reader + core (`961db5c8..92775757`) | `gpt-6.1-sol`, xhigh, read-only | `gpt-6.1-sol`, `xhigh`, `read-only` (session `01a1af60-c6be-7ac3…`) | `work/rv-w34` (detached `92775757`) | running | |
| Review between waves 3-4: multipart + test server + docs | `gpt-6-astra`, xhigh, read-only | `gpt-6-astra`, `xhigh`, `read-only` (session `01a1af60-c6be-7de1…`) | `work/rv-w34` | ACCEPT_WITH_FIXES, AST-W34-1 fixed `9be6134b` | |
