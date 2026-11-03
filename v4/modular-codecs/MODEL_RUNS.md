# Model runs

Observed models come from each run's own evidence: for Codex runs, the session's `turn_context`
(read by the work directory's tools/observe_settings.py); for Claude subagents, the `model` field
of their transcript. Claude reasoning effort is not recorded in those transcripts:
NOT_OBSERVABLE. Reports are frozen (SHA-256 below) before any synthesis; copies are in [reports/](reports/)
(`reports/SHA256SUMS`), the originals in the work directory's `evidence/frozen/`.

| Run | Role | Requested | Observed | Mode | Source | Frozen output (SHA-256 prefix) |
|---|---|---|---|---|---|---|
| root | coordinator, integrator, write owner of code | Claude Opus | `claude-opus-5-5` | — | — | — |
| arch-sol | independent architecture and codec/HTTP challenge | `gpt-6.1-sol`, xhigh | `gpt-6.1-sol`, `xhigh` | execute, loopback | `dfdeed8` | [`arch-sol-REPORT.md`](reports/arch-sol-REPORT.md) `bdac2c00` |
| arch-opus | independent architecture and codec/HTTP challenge | Claude Opus | `claude-opus-5-5` | subagent | `dfdeed8` | [`arch-opus-REPORT.md`](reports/arch-opus-REPORT.md) `a8c534f9` |
| journey-terra | author-journey probe (docs only) | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | static (read-only) | `dfdeed8` | [`journey-terra-REPORT.md`](reports/journey-terra-REPORT.md) `470888e1` |
| journey-sonnet | author-journey probe (docs only) | Claude Sonnet | `claude-sonnet-5-5` | subagent | `dfdeed8` | [`journey-sonnet-REPORT.md`](reports/journey-sonnet-REPORT.md) `ada899cd` |
| docs-sonnet | documentation rewrite (write owner: docs only) | Claude Sonnet | `claude-sonnet-5-5` | subagent, worktree | `d3c6e55` | commit `97c8011` (cherry-picked as `d5c93f0`) |
| consumers | isolated consumer crates and script (bounded implementation) | `gpt-6.1-sol`, medium | `gpt-6.1-sol`, `medium` | execute, loopback | `25a52f7` | imported as `fbf0ac2` |
| fresh-terra | fresh-use trial: a vendor format and header-name validation from the docs (R07) | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | execute, loopback | `9f7eb83` | [`fresh-terra-REPORT.md`](reports/fresh-terra-REPORT.md) `9586e878` |
| fresh-sonnet | fresh-use trial, same tasks (R07) | Claude Sonnet | `claude-sonnet-5-5` | subagent, private copy | `9f8939f` | [`fresh-sonnet-REPORT.md`](reports/fresh-sonnet-REPORT.md) `03f711b6` |
| final-exec-sol | final executable audit of the frozen candidate (different provider) | `gpt-6.1-sol`, xhigh | `gpt-6.1-sol`, `xhigh` | execute, loopback | `9f8939f` | [`final-exec-sol-REPORT.md`](reports/final-exec-sol-REPORT.md) `714729c7` |
| final-sem-opus | final semantic and architecture review of the frozen candidate | Claude Opus | `claude-opus-5-5` | subagent, private target | `9f8939f` | [`final-sem-opus-REPORT.md`](reports/final-sem-opus-REPORT.md) `4f3d2c4e` |
| rereview-sol | targeted re-review of the audit corrections (fresh session) | `gpt-6.1-sol`, xhigh | `gpt-6.1-sol`, `xhigh` | execute, loopback | `4ce3d01` | [`rereview-sol-REPORT.md`](reports/rereview-sol-REPORT.md) `9af50bf4` |
| rereview-opus | re-review of the semantic-review corrections (the same side Opus session, resumed) | Claude Opus | `claude-opus-5-5` | subagent, private target | `530b6c1` | [`rereview-opus-REPORT.md`](reports/rereview-opus-REPORT.md) `8907739e` |
| rereview2-sol | second targeted re-review of all corrections (fresh session) | `gpt-6.1-sol`, xhigh | `gpt-6.1-sol`, `xhigh` | execute, loopback | `3fa14b7` | [`rereview2-sol-REPORT.md`](reports/rereview2-sol-REPORT.md) `29cfcd8f` |
| rereview3-sol | narrow re-review of the last correction delta (fresh session) | `gpt-6.1-sol`, xhigh | `gpt-6.1-sol`, `xhigh` | execute, loopback | `d28ad34` | [`rereview3-sol-REPORT.md`](reports/rereview3-sol-REPORT.md) `8d2a5e5f` |
| rereview4-sol | static review of the final lint/tooling delta (fresh session) | `gpt-6.1-sol`, medium | `gpt-6.1-sol`, `medium` | static (read-only) | `fcf343f` | [`rereview4-sol-REPORT.md`](reports/rereview4-sol-REPORT.md) `e4a06f7b` |
| path-terra | bounded hierarchical-path prototype (optional gate) | `gpt-6.1-terra`, xhigh | `gpt-6.1-terra`, `xhigh` | execute, loopback | `25a52f7` | [`path-terra-report.md`](reports/path-terra-report.md) `ba7a1b02`; recipe imported as `1f26e91` |

The two architecture challengers worked from the same brief and source, without the other's
output; their first reports were frozen before the contract was decided (DECISIONS MC08–MC12). No
reviewer certifies code it wrote: the final executable audit is a fresh Sol XHigh session, the
semantic review a side Opus session that only proposed (did not implement) the contract.
