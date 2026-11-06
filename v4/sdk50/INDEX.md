# Concord SDK50 — campaign index

Handoff: `Concord_SDK50_Ecosystem_Handoff/` revision 2 (input hashes in `HANDOFF_INPUTS.sha256`;
mission record `MISSION.md`). Live provider tier: NOT_RUN (no live writes or paid calls; local
fixtures only).

## Identities

| What | Commit | Tree | Note |
|---|---|---|---|
| Handoff pin (PR #4 head) | `90b69011` | `6a654331` | matched the remote at start |
| Main before M0 | `a95fb2fb` | `5a034acf` | |
| M0 candidate (PR #4 head merged) | `5c3ef5f6` | `da1de36d` | + earlier records `a57f020a` + registry fix |
| **Main integration of SDK20** | `8cae5d4c` | `da1de36d` | merge commit of PR #4 (parents `a95fb2fb`, `5c3ef5f6`); see `M0_RECEIPT.md` |
| Campaign branch | `concord-ecosystem-sdk50` | | created at `8cae5d4c`; merges `sdk50/integration` |

## Phase status

| Phase | Status | Records |
|---|---|---|
| M0 SDK20 integration | merged; local gates and CI in `M0_RECEIPT.md` | `M0_RECEIPT.md` |
| P0 retrospective, carryovers | registry expiry fixed (M0-1); 17 stale guide passages corrected; obligation/residue inventory; 88-scenario denominator | `P0_INVENTORY.md`, `SCENARIOS.md`, `DECISIONS.md` |
| P1 F1–F3 and first integrations | F1 retained with compiled comparison; F2 head retention, `decode_with_meta`, `document` adopted in 24 SDKs/examples; F3 parameter groups and `Sends<O>`; codecs package, Link parser, retry pilot; pagination lane in progress | `DECISIONS.md`, `checks/core-concepts/` |
| P2 waves SDK21–50 | not started | `waves/` |
| P3 final adoption and retrospective | not started | — |

## Lanes (branches; each merged into `sdk50/integration` after coordinator verification)

| Lane | Branch | Author (observed) | Result |
|---|---|---|---|
| F2/F3 core and macro prototypes, docs | `sdk50/p1-draft`, `sdk50/p1-docs2` | coordinator | merged |
| F2 adoption (20 SDKs/examples) | `sdk50/p1-f2-adoption` | Codex `gpt-6-astra` xhigh | merged |
| concord-codecs | `sdk50/p1-codecs` | Codex `gpt-6.1-terra` xhigh | merged |
| Link parser | `sdk50/p1-link` | Codex `gpt-6-astra` xhigh | merged |
| Stale docs | `sdk50/p0-docs` | Codex `gpt-6.1-sol` xhigh | merged |
| concord-retry pilot | `sdk50/p1-retry` | Codex `gpt-6.1-sol` xhigh | merged |
| Pagination provider error / item metadata | `sdk50/p1-pagination` | Codex `gpt-6.1-terra` xhigh | in progress |

Independent evidence on P1: docs-only prediction probe (Terra, `roles/p1-predict.md`), independent
review (Sol, `roles/p1-review.md`), first-use probe building a Desk SDK from the docs (Opus
subagent). Their findings and the corrections are in `DECISIONS.md`. Model/session ledger:
`MODELS.md`. Resources: `RESOURCES.md`.
