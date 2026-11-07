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
| | **Wave 2 (SDK26–30) complete**; between-waves changes B23-A2 (`send P -> Result<_, Error>`), B23-R (`Error::delivery()`), B23-C (`concord::field`), B23-Q, B23-G accepted and adopted; repeated-key guard not accepted (0 adopters); routing R-1 (Codex = Sol only) from Wave 3 | `waves/W2/{PLAN,STATUS,FINDINGS,SYNTHESIS}.md` |
| | **Wave 3 (SDK31–35, payments and accounting) complete**: 15 roles; between-waves B34-D (generated docs), B34-R (`Retry::receive_with`), B34-O (inherent `operation()`) accepted; B34-A (auth rotation) not accepted; B34-V (declared values) experiment continues | `waves/W3/{PLAN,STATUS,FINDINGS,SYNTHESIS}.md` |
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
| Wave 2 SDK26–30 | `sdk50/w2-sdk2N-<slug>`, `sdk50/w2-sdk30-hubspot-crm` | 15 roles, see `MODELS.md` | merged |
| Declared send result, `delivery()`, `field`, query docs, `.gitattributes`, review fixes | `sdk50/b23-core` | coordinator (review: Codex `gpt-6.1-sol` xhigh) | merged |
| Earlier SDKs adopt `send -> ..` and `field` | `sdk50/b23-oldadopt` | Codex `gpt-6-astra` xhigh | merged |
| Wave-2 SDKs adopt; repeated-key guard prototype | `sdk50/b23-w2adopt` | Codex `gpt-6.1-sol` xhigh; reconciled by the coordinator (`a46f490a`) | SDK adoption merged; guard not merged |
| Wave 3 SDK31–35 | `sdk50/w3-sdk3N-<slug>` | 15 roles (Sol, Opus), see `MODELS.md` | merged |
| Generated docs; inherent `operation()` | `sdk50/b34-core` | coordinator (reviews: Codex `gpt-6.1-sol` xhigh) | merged |
| `Retry::receive_with`, PayPal adoption | `sdk50/b34-retry` | Codex `gpt-6.1-sol` xhigh | merged |
| Auth rotation prototype | `sdk50/b34-auth` | Codex `gpt-6.1-sol` xhigh | not merged (grows 2 of 3 SDKs) |
| Declared-value experiment | `sdk50/b34-values` | Codex `gpt-6.1-sol` xhigh | not merged (experiment) |

Independent evidence on P1: docs-only prediction probe (Terra, `roles/p1-predict.md`), independent
review (Sol, `roles/p1-review.md`), first-use probe building a Desk SDK from the docs (Opus
subagent). Their findings and the corrections are in `DECISIONS.md`. Model/session ledger:
`MODELS.md`. Resources: `RESOURCES.md`.
