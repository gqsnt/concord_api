# Wave 1 status

Frozen library baseline: `56a16ee34f8aa714bd51dca67f744e5870e813b6` (`sdk50/integration`), whose
tree is the campaign branch's tree after P1. Gate on it: `scripts/verify.sh --package --bench`
62/62 PASS, clean, 6m39s; `--nightly-only` PASS (16). Started 2026-11-07 00:22 CET.

| SDK | Worktree / branch | Author (requested → observed) | Status |
|---|---|---|---|
| SDK21 Open Library | `work/w1-sdk21-open-library`, `sdk50/w1-sdk21-open-library` | Codex `gpt-6.1-sol` xhigh | authoring |
| SDK22 NASA NeoWs | `work/w1-sdk22-nasa-neows`, `sdk50/w1-sdk22-nasa-neows` | Codex `gpt-6.1-terra` xhigh | authoring |
| SDK23 USGS | `work/w1-sdk23-usgs-earthquake-catalog`, `sdk50/w1-sdk23-usgs-earthquake-catalog` | Codex `gpt-6-astra` xhigh | authoring |
| SDK24 Discord | `work/w1-sdk24-discord-rest`, `sdk50/w1-sdk24-discord-rest` | Claude subagent (Opus) xhigh | authoring |
| SDK25 FRED | `work/w1-sdk25-fred`, `sdk50/w1-sdk25-fred` | Codex `gpt-6.1-sol` xhigh | authoring |

Capsules: `work/briefs/w1-SDK2x-author.md` (copied into `v4/sdk50/roles/` when the wave closes).
