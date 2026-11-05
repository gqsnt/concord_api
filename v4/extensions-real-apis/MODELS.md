# Models, roles and observed settings

Requested settings are what the coordinator asked for; observed settings come only from harness
metadata (the Claude Code environment statement for the coordinator, the Codex session header
lines `model:` / `reasoning effort:` / `sandbox:`), never from a model's self-description. Where
the harness exposes nothing, the cell says so.

## Coordinator

| Role | Requested | Observed |
|---|---|---|
| Implementation coordinator (integration, public-interface ownership, test server, stream foundation, gates, stress harness, records) | the configured Claude Code session | `claude-opus-5-5` (environment statement of the session); reasoning effort not exposed |

## Implementers and SDK authors (Claude Code `Agent` tool, background subagents)

The Agent tool was called without a model override, so each subagent inherits the coordinator's
model by the tool's documented default; the harness does not report a per-agent model or effort.
"Observed" is therefore **not exposed** for every row. Each ran in its own worktree and branch.

| Work | Branch (final commit) | Independence from the abstraction author |
|---|---|---|
| `concord-pagination` crate | `ext44/pag` (`366aa3e`) | is the pagination author |
| CSV codec + extension example migration | `ext44/csv` (`e5bc3db`) | is the CSV author |
| JSON Lines / NDJSON / JSON values | `ext44/jsonl` (`6b6ce4c`) | is the JSON Lines author |
| GitHub SDK + GH-A/GH-B | `ext44/sdk-github` (`21c6a7e`) | separate session; did not design pagination/codecs |
| Stripe SDK + ST-A/ST-B | `ext44/sdk-stripe` (`8a42aa9`) | separate session |
| Shopify SDK + SH-A/SH-B | `ext44/sdk-shopify` (`ef93111`) | separate session |
| Salesforce SDK + SF-A/SF-B | `ext44/sdk-salesforce` (`a99a91d`) | separate session |
| Kubernetes SDK + KU-A/KU-B | `ext44/sdk-kubernetes` (`8816c4a`) | separate session |
| Maintenance exercise, GitHub | `ext44/maint-github` (`b7daf50`) | did not write the GitHub SDK |
| Maintenance exercise, Stripe | `ext44/maint-stripe` (`3dcb3ac`) | did not write the Stripe SDK |
| Maintenance exercise, Shopify | `ext44/maint-shopify` (`4f2f664`) | did not write the Shopify SDK (resumed once after a machine restart) |
| Maintenance exercise, Salesforce | `ext44/maint-salesforce` (`962567c`) | did not write the Salesforce SDK |
| Maintenance exercise, Kubernetes | `ext44/maint-kubernetes` (`d903a1f`) | did not write the Kubernetes SDK |

Limitation: all of these are the same model family as the coordinator; separate sessions give
separate authorship, not provider diversity.

## Cross-provider implementers and reviewers (Codex CLI 0.166.0)

Available models (local `~/.codex/models_cache.json`): `gpt-6.1-sol`, `gpt-6.1-terra`,
`gpt-6-astra` (no 6.1 Astra is offered), each with `xhigh`.

| Work | Requested | Observed (session header) | Notes |
|---|---|---|---|
| Review of the 4.3 CI correction (`d4ebd34..2cdecd3`) | `-m gpt-6.1-sol -c model_reasoning_effort=xhigh -s read-only` | `model: gpt-6.1-sol`, `reasoning effort: xhigh`, `sandbox: read-only` | ACCEPT_WITH_FIXES, 8 findings, all fixed |
| Re-check (`2cdecd3..f9d8d2d`) | same | same | 8 RESOLVED, 1 new medium finding, fixed |
| Re-check (`f9d8d2d..ae5618a`) | same | same | ACCEPT |
| DX-03 (b) static/runtime CSV caller task | `-m gpt-6.1-terra -c model_reasoning_effort=xhigh -s workspace-write` | `model: gpt-6.1-terra`, `reasoning effort: xhigh`, `sandbox: workspace-write` (session `01a1a8d6…`) | sandbox refused loopback sockets in round 1; the coordinator ran the tests |
| (aborted) resume of that session | `codex exec resume` without `-m` | `model: gpt-6.1-sol` | the resume fell back to the config default; it only read files and was stopped; repeated with `-m gpt-6.1-terra` (observed `gpt-6.1-terra`, `xhigh`) |
| DX-03 (a) fanout/sink caller task, stages 1–2 | `-m gpt-6-astra -c model_reasoning_effort=xhigh -s workspace-write -c sandbox_workspace_write.network_access=true` | `model: gpt-6-astra`, `reasoning effort: xhigh`, `sandbox: workspace-write … (network access enabled)` (session `01a1a978…`) | stage 2 resumed with `-m gpt-6-astra` (observed the same) |

The 4.4 review strands and their observed settings are recorded in `NEXT_REVIEW_4_4.md`.
