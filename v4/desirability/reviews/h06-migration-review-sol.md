# Review h06-migration-review-sol (MIGRATION review, H06, on 74cad812)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T19:15:39Z–19:23:20Z, thread 01a1bc96-758b-7190-bca7-3d3026e56aab.
(Migration authored by a Codex lane: independent session, same provider.)
**Verdict: REJECT** (2 blocking, 1 non-blocking). Confirmed: every adoption rests on an invariant the
SDK documents (Adyen lib.rs:82, Discord api.rs:40, Sentry README:110, DigitalOcean lib.rs:69,
QuickBooks auth.rs:33, Datadog credentials.rs:9, USGS lib.rs:29, Salesforce context.rs:83, Zendesk
lib.rs:44, Shopify api.rs:36, Mastodon README:26); restrictions have replacement routes and keep
documented configuration and rotation; QuickBooks `set_connection`, Cloudflare/Zendesk `set_policy`,
Shopify/Mastodon rotation, S3/Kubernetes controls remain; every migrated test site reviewed, no
weakened assertion or deleted test; the four added constructors are documented replacements with
correct validation; 55-row impact table complete; 44 non-adopters + 2 unchanged pilots byte-identical;
17 removals / 4 additions independently extracted; 3,973 tests, clippy pass.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | QuickBooks' policy rewrites the destination of a non-withheld call at send (`policy.rs:36`): `connect(sandbox).set_connection(production)` used to keep the sandbox host, now goes to production — fires the card's wire-change kill condition; visibility does not require it | blocking | valid — split out of H06 as a separate correctness repair (A1 §3 path 1, fail-before/pass-after); remove from the H06 migration |
| 2 | All 17 public-method removals introduced by the migration fall outside E1–E6 (incl. `SalesforceApi::new`, `Zendesk::with_base_url`); the card's full-migration clause requires broader coverage but does not exempt these removals from the kill condition | blocking | valid — a conflict inside the frozen card (expected breakages written for the pilot vs a migration clause that needs more); needs an owner decision (reviewer); asked |
| 3 | Strava impact-row evidence overstated (one server, destination `127.0.0.1:1`, not a two-server test); non-adoption still justified by the origin guard (`auth.rs:341`) | non-blocking | fix the impact row text |
