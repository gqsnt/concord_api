# Wave 1 status — complete

Frozen library baseline: `56a16ee34f8aa714bd51dca67f744e5870e813b6` (`sdk50/integration`), whose
tree is the campaign branch's tree after P1. Gate on it: `scripts/verify.sh --package --bench`
62/62 PASS, clean, 6m39s; `--nightly-only` PASS (16). Started 2026-11-07 00:22 CET; last role
finished 00:59; each SDK branch merged into `sdk50/integration` after the coordinator re-ran its
tests outside the sandbox.

| SDK | Branch | Author | Fresh consumer | Maintainer | Result |
|---|---|---|---|---|---|
| SDK21 Open Library | `sdk50/w1-sdk21-open-library` | Codex `gpt-6.1-sol` | Codex `gpt-6.1-terra` (docs-only) | Codex `gpt-6-astra` | merged; simple control held |
| SDK22 NASA NeoWs | `sdk50/w1-sdk22-nasa-neows` | Codex `gpt-6.1-terra` | Codex `gpt-6-astra` (docs-only) | Opus subagent | merged; `send_metered` via `Sends` |
| SDK23 USGS | `sdk50/w1-sdk23-usgs-earthquake-catalog` | Codex `gpt-6-astra` | Opus subagent (docs-only) | Codex `gpt-6.1-sol` | merged |
| SDK24 Discord | `sdk50/w1-sdk24-discord-rest` | Opus subagent | Codex `gpt-6.1-sol` (docs-only) | Codex `gpt-6.1-terra` | merged; limiter evidence for B12-G |
| SDK25 FRED | `sdk50/w1-sdk25-fred` | Codex `gpt-6.1-sol` | Codex `gpt-6.1-terra` (docs-only) | Codex `gpt-6-astra` | merged; XML/ZIP evidence |

All requested and observed settings: `../../MODELS.md` (observed = requested for every role,
effort xhigh). Capsules, tickets and lane briefs: `../../roles/`. Findings: `FINDINGS.md`;
dispositions: `SYNTHESIS.md`.
