# Wave 6 status

Library baseline: `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77` (owner instruction R-2: Waves 4–6 on
the same baseline, no between-wave changes). This wave was held out: no slice notes; first results committed as such. Each role was verified by the coordinator
(owned files only, no removed test line, campaign files read: none for every Codex role after the
P-1 capsule fix, package tests/clippy/rustdoc/fmt re-run) and committed by the coordinator.

| SDK | Worktree / branch | Author (requested) | Status |
|---|---|---|---|
| SDK46 Anthropic Messages | `work/w6-sdk46-anthropic-messages`, `sdk50/w6-sdk46-anthropic-messages` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Sol); merged |
| SDK47 Kraken Spot REST | `work/w6-sdk47-kraken-spot-rest`, `sdk50/w6-sdk47-kraken-spot-rest` | Codex `gpt-6.1-sol` xhigh | complete: consumer (Opus), maintainer (Opus); merged |
| SDK48 Elasticsearch | `work/w6-sdk48-elasticsearch`, `sdk50/w6-sdk48-elasticsearch` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Sol); merged |
| SDK49 Europe PMC | `work/w6-sdk49-europe-pmc`, `sdk50/w6-sdk49-europe-pmc` | Codex `gpt-6.1-sol` xhigh | complete: consumer (Opus), maintainer (Opus); merged |
| SDK50 REST Countries | `work/w6-sdk50-rest-countries`, `sdk50/w6-sdk50-rest-countries` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Sol); merged |

Integration with every Wave 4–6 SDK: `af6840c9`, gate `scripts/verify.sh --package --bench` 66/66
PASS (7m44s), `--nightly-only` PASS (16). Observed settings: `../../MODELS.md` (all rows observed =
requested). Findings: `FINDINGS.md` (collected for the cross-50 analysis).
