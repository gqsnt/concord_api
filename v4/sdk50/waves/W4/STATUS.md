# Wave 4 status

Frozen library baseline: `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77` (`sdk50/integration`): Wave 3
SDKs, B34-D (generated docs), B34-R (`Retry::receive_with`), B34-O (inherent `operation()`), the
Windows test fix and their reviews. Gate on it: `scripts/verify.sh --package --bench` 66/66 PASS,
clean, 5m19s; `--nightly-only` PASS (16 tests). (The gate on `9960eda3` failed clippy on an import
the inherent `operation()` made unused; fixed in `c3fd4f1e`.) Routing R-1; capsules under P-1.

| SDK | Worktree / branch | Author (requested) | Status |
|---|---|---|---|
| SDK36 Spotify Web API | `work/w4-sdk36-spotify-web-api`, `sdk50/w4-sdk36-spotify-web-api` | Claude subagent (Opus) xhigh | authoring |
| SDK37 Strava | `work/w4-sdk37-strava`, `sdk50/w4-sdk37-strava` | Codex `gpt-6.1-sol` xhigh | authoring |
| SDK38 Mastodon | `work/w4-sdk38-mastodon`, `sdk50/w4-sdk38-mastodon` | Claude subagent (Opus) xhigh | authoring |
| SDK39 MediaWiki Action API | `work/w4-sdk39-mediawiki-action-api`, `sdk50/w4-sdk39-mediawiki-action-api` | Codex `gpt-6.1-sol` xhigh | authoring |
| SDK40 Cloudflare Zones | `work/w4-sdk40-cloudflare-zones`, `sdk50/w4-sdk40-cloudflare-zones` | Claude subagent (Opus) xhigh | authoring |

Observed settings are recorded in `../../MODELS.md` when each role ends. Slice notes:
`../../roles/w4-extra-*.md`.
