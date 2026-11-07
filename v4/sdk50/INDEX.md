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
| M0 SDK20 integration | **complete**: merged as `8cae5d4c`; local gates and Linux/Windows/nightly CI on the PR head and on main | `M0_RECEIPT.md` |
| P0 retrospective, carryovers | registry expiry fixed (M0-1); 17 stale guide passages corrected; obligation/residue inventory; 88-scenario denominator | `P0_INVENTORY.md`, `SCENARIOS.md`, `DECISIONS.md` |
| P1 F1–F3 and first integrations | **complete**: F1 retained with compiled comparison; F2 head retention, `decode_with_meta`, `document` adopted in 24 SDKs/examples; F3 parameter groups and `Sends<O>`; codecs package, Link parser, retry and auth pilots, pagination provider error / item events | `DECISIONS.md`, `checks/core-concepts/` |
| P2 waves SDK21–50 | **Wave 1 (SDK21–25) complete**; between-waves changes A–E, H, `query(..)` accepted and adopted, ratelimit pilot not accepted (its review defects fixed); Wave 2 next | `waves/W1/{PLAN,STATUS,FINDINGS,CANDIDATES,SYNTHESIS}.md` |
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
| Pagination provider error / item metadata | `sdk50/p1-pagination` | Codex `gpt-6.1-terra` xhigh | merged (frozen first-use tests restored by the coordinator) |
| concord-auth pilot | `sdk50/p2-auth` | Codex `gpt-6-astra` xhigh | merged |
| concord-ratelimit pilot | `sdk50/p2-ratelimit` | Codex `gpt-6.1-terra` xhigh | merged as a pilot; not accepted (B12-G) |
| Wave 1 SDK21–25 | `sdk50/w1-sdk2N-<slug>` | 15 roles, see `MODELS.md` | merged |
| Declared limit, generated docs, header fields | `sdk50/b12-limit` | coordinator | merged |
| Codecs media check, offset helper, FRED/USGS adoption | `sdk50/b12-x` | Codex `gpt-6-astra` xhigh | merged |
| Limit/field adoption in 12 SDKs, Discord overlap fix | `sdk50/b12-y` | Codex `gpt-6.1-sol` xhigh (Discord ratelimit migration dropped by the coordinator) | merged |
| `query(..)` and docs H | `sdk50/b12-pairs` | coordinator | merged |
| `query(..)` adoption (Discord, ClickHouse) | `sdk50/b12-pairs-adopt` | Codex `gpt-6.1-terra` xhigh | merged |
| Ratelimit review fixes | `sdk50/b12-ratefix` | Codex `gpt-6-astra` xhigh | merged |

Independent evidence on P1: docs-only prediction probe (Terra, `roles/p1-predict.md`), independent
review (Sol, `roles/p1-review.md`), first-use probe building a Desk SDK from the docs (Opus
subagent). Their findings and the corrections are in `DECISIONS.md`. Model/session ledger:
`MODELS.md`. Resources: `RESOURCES.md`.
