# Review h06-combined-review-sol (COMBINED review, H06: candidate + measurement + decision; A3 §5)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T18:38:08Z–18:46:15Z, thread 01a1bc74-1fa0-7462-bda1-07d7515aea45.
(B authored by a Claude subagent: cross-provider for B; A authored by a Codex lane: same-provider,
independent session, for A.)
**Verdict: ACCEPT_WITH_FIXES. DECISION: WIN (B)** pending quiet-machine compile-cost confirmation,
full migration and its review, and Linux/Windows CI on the commit being merged; **A is not WIN**
(FM3 still available, its forbidden request delivered).

Deciding items (summary): B closes FM1–FM4 at caller compilation (baseline permits all four, one
forbidden request each); all 16 valid moves and 3 simple controls pass on every arm, both
repetitions; B's public removals are exactly Shopify `set_shop` (E1), Zendesk `set_subdomain` (E2),
Salesforce `set_version` and public `with_base_url` (E3); Mastodon's 19 added public-view entries
fit E5 (inventory entries: `Api`, its impl and methods, auto/Clone/Debug, `Mastodon::api`); A removes
exactly the three E1–E3 setters; doc changes explain the restrictions; nameable `new` is part of the
single E6 form (no extra rule, no public core item); the Zendesk test move is E4, assertions intact;
Mastodon nameable with docs pointing to `Mastodon::new`, generated constructor and host control
crate-visible (forwarders unchanged; "B instead of A" moot since A fails WIN); Zendesk public
`with_base_url` and Salesforce public `new` remain outside the registered FMs (limits the claim; not a
kill); one visibility rule meets the concept budget; A7 met (unsafe controls removed from 3 SDKs);
no adapter/VM/wire/north-star/sensitivity/control kill; 109/113 non-adopter JSONs identical; scale
expansions byte-identical; gate 66/66.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Full migration (A8) required before acceptance: 55-row impact table; migrate every documented invariant a public control contradicts (Discord/Sentry/Adyen credential-state pairs, DigitalOcean fixture boundary, QuickBooks construction, Xero, other survey §3 cases), preserving coordinated replacement and deliberate safety boundaries; MIGRATION.md, guides, CHANGELOG; review and gate the migrated commit | blocking | in progress — Codex migration lane (dz/h06), then migration review, gate, CI |
| 2 | Cost and final CI gates open: timing samples had over-bound rows; quiet-machine comparison pending; CI pending | blocking | in progress — check-scale1000 re-run with RUNS=10 at load < 6; CI on the merge commit required |
| 3 | FM2 compile probe `h06/c02` imports the wrong crate (E0432 on every arm) | non-blocking | known limit; FM2 decided by `h06/r02` (baseline delivers to evil; A/B E0599); fix in a future versioned bank |
