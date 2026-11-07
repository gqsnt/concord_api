# Wave 4 status

Library baseline: `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77` (owner instruction R-2: Waves 4–6 on
the same baseline, no between-wave changes). Each role was verified by the coordinator
(owned files only, no removed test line, campaign files read: none for every Codex role after the
P-1 capsule fix, package tests/clippy/rustdoc/fmt re-run) and committed by the coordinator.

| SDK | Worktree / branch | Author (requested) | Status |
|---|---|---|---|
| SDK36 Spotify Web API | `work/w4-sdk36-spotify-web-api`, `sdk50/w4-sdk36-spotify-web-api` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Sol); merged |
| SDK37 Strava | `work/w4-sdk37-strava`, `sdk50/w4-sdk37-strava` | Codex `gpt-6.1-sol` xhigh | complete: consumer (Opus), maintainer (Opus); merged |
| SDK38 Mastodon | `work/w4-sdk38-mastodon`, `sdk50/w4-sdk38-mastodon` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Sol); merged |
| SDK39 MediaWiki Action API | `work/w4-sdk39-mediawiki-action-api`, `sdk50/w4-sdk39-mediawiki-action-api` | Codex `gpt-6.1-sol` xhigh | complete: consumer (Opus), maintainer (Opus); merged |
| SDK40 Cloudflare Zones | `work/w4-sdk40-cloudflare-zones`, `sdk50/w4-sdk40-cloudflare-zones` | Claude subagent (Opus) xhigh | complete: consumer (Sol), maintainer (Sol); merged |

Integration with every Wave 4–6 SDK: `af6840c9`, gate `scripts/verify.sh --package --bench` 66/66
PASS (7m44s), `--nightly-only` PASS (16). Observed settings: `../../MODELS.md` (all rows observed =
requested). Findings: `FINDINGS.md` (collected for the cross-50 analysis).
