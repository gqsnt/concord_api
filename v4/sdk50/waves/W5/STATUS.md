# Wave 5 status

Library baseline: `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77` (owner instruction R-2: Waves 4–6 on
the same baseline, no between-wave changes). Each role was verified by the coordinator
(owned files only, no removed test line, campaign files read: none for every Codex role after the
P-1 capsule fix, package tests/clippy/rustdoc/fmt re-run) and committed by the coordinator.

| SDK | Worktree / branch | Author (requested) | Status |
|---|---|---|---|
| SDK41 DigitalOcean | `work/w5-sdk41-digitalocean`, `sdk50/w5-sdk41-digitalocean` | Codex `gpt-6.1-sol` xhigh | complete: consumer (Opus), maintainer (Opus); merged |
| SDK42 Datadog Events | `work/w5-sdk42-datadog-events`, `sdk50/w5-sdk42-datadog-events` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Sol); merged |
| SDK43 Sentry | `work/w5-sdk43-sentry`, `sdk50/w5-sdk43-sentry` | Codex `gpt-6.1-sol` xhigh | complete: consumer (Opus), maintainer (Opus); merged |
| SDK44 Backblaze B2 Native | `work/w5-sdk44-backblaze-b2-native`, `sdk50/w5-sdk44-backblaze-b2-native` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Sol); merged |
| SDK45 arXiv | `work/w5-sdk45-arxiv`, `sdk50/w5-sdk45-arxiv` | Codex `gpt-6.1-sol` xhigh | complete: consumer (Opus), maintainer (Opus); merged |

Integration with every Wave 4–6 SDK: `af6840c9`, gate `scripts/verify.sh --package --bench` 66/66
PASS (7m44s), `--nightly-only` PASS (16). Observed settings: `../../MODELS.md` (all rows observed =
requested). Findings: `FINDINGS.md` (collected for the cross-50 analysis).
