# Review h06c-combined-review-sol (COMBINED review, H06c, on a6ab7c22)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-09T00:30:04Z–00:34:37Z, thread 01a1bdb6-51c5-7ed2-bd67-b86525c2c793.
(Candidate authored by Codex lanes: independent session, same provider.)
**Verdict: ACCEPT_WITH_FIXES. DECISION: WIN** — conditional on the documentation fixes and on the full
gate and Linux/Windows CI on the final merged commit; no kill condition or per-SDK drop.
Confirmed: every hunk of `34dfc3ff..a6ab7c22` and the merge's documentation resolutions; saved JSON
changes extracted independently — exactly 17 removals and 4 constructors within X1–X9, no other
signature/structural change, 46 JSONs byte-identical (44 non-adopters, Shopify, Mastodon);
Salesforce/Zendesk keep their accepted pilot restrictions with only registered extensions; all nine
adoption grounds, constructor validation, rotation/configuration/fixture routes and migrated
assertions (incl. both relocated assertion blocks); QuickBooks' non-test policy identical to the
baseline (no other non-withheld wire change); restrictions remove controls rather than relocating
obligations; zero new core concepts; 55 impact rows, Strava corrected; coordinator probe results
(23 distinct lines, 11 E0599, 12 E0624; positive probes compile); 3,972 tests passed, clippy/fmt pass.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | QuickBooks README still promises the removed send-time environment selection (`README.md:7`) | non-blocking | fixed (coordinator, trivial docs) in the follow-up commit |
| 2 | Additional-adopter count wrong: the nine affected SDKs include the Salesforce/Zendesk pilots (`CHANGELOG.md:10`, `docs/MIGRATION.md:8`) | non-blocking | fixed: seven new adopters + two pilot extensions = 11 |
