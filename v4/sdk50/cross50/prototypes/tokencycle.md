# HubSpot and Strava: an abandoned token refresh kept the client alive

Found by the cross-50 crates sweep (`analysts/a6-crates.md`, "A correctness concern"), as an
inference: each SDK moved its `Arc<Inner>` into the acquisition future that `Inner.cache`
(`concord_auth::TokenCache`) stores while it is in flight. The cache README warns against
exactly this cycle. Xero, QuickBooks and Spotify capture their inputs separately.

**Test** (one per SDK, in-crate): a loopback listener accepts the refresh connection and never
answers; the only caller is aborted mid-refresh; the client is dropped; a `Weak` to the shared
state must no longer upgrade.

**Before the fix (observed):** both tests fail at that final assertion —
```
== concord-sdk-hubspot-crm
test tests::abandoned_refresh_does_not_keep_the_installation_alive ... FAILED
thread 'tests::abandoned_refresh_does_not_keep_the_installation_alive' (390769) panicked at examples/hubspot-crm-sdk/src/lib.rs:658:9:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p concord-sdk-hubspot-crm --lib`
== concord-sdk-strava
test auth::tests::abandoned_refresh_does_not_keep_the_session_alive ... FAILED
thread 'auth::tests::abandoned_refresh_does_not_keep_the_session_alive' (391478) panicked at examples/strava-sdk/src/auth.rs:424:9:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p concord-sdk-strava --lib`
```

**Fix:** capture only the acquisition inputs (HubSpot: `Arc<Mutex<Secrets>>`, the HTTP
client and OAuth base; Strava: `Arc<Mutex<Tokens>>`, the OAuth client and credentials).
No Concord change; no public API change.

**After the fix (coordinator, `CARGO_BUILD_JOBS=4`):**
```
== concord-sdk-hubspot-crm tests
  passed 23 failed 0
== concord-sdk-hubspot-crm clippy
== concord-sdk-hubspot-crm doc
== concord-sdk-strava tests
  passed 61 failed 0
== concord-sdk-strava clippy
== concord-sdk-strava doc
fmt ok
```

## Round 2 — two older defects confirmed by the independent review

The Sol review of this fix and of the Pages prototype (`roles/x50-r-pages-tokencycle-sol`)
confirmed by reading two defects that predate both changes:

1. **Strava held its token mutex across the refresh request.** A refresh cancelled mid-request
   stays paused in the cache with the guard alive, so `latest_tokens()` — and every activity
   update, which reads the granted scope first — waited forever. Fix: read the refresh token
   under the lock, release it during the provider call, retake it to commit the rotation (the
   cache already serialises acquisitions). New test: cancel a refresh, keep the session, and
   require `latest_tokens()` within 5 s.
2. **Europe PMC's mandatory 1..=20 page ceiling could be raised** by `.max_pages(n)` on the
   returned traversal (the cap was only checked at construction). Fix: count requests inside
   the page factory and refuse the 21st unsent. New test: cap 20 raised to 21 → exactly 20
   requests and `InvalidInput`.

**Without the fixes (new tests on the old source):**
```
== WITHOUT the fixes (new tests, old src)
test auth::tests::an_abandoned_refresh_does_not_block_token_readers ... FAILED
thread 'auth::tests::an_abandoned_refresh_does_not_block_token_readers' (627373) panicked at examples/strava-sdk/src/auth.rs:468:14:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 5.02s
test a_raised_cap_still_stops_at_the_twenty_page_ceiling_without_sending ... FAILED
thread 'a_raised_cap_still_stops_at_the_twenty_page_ceiling_without_sending' (628578) panicked at examples/europe-pmc-sdk/tests/workflows.rs:282:5:
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.02s
```
**With the fixes:**
```
== WITH the fixes
-- concord-sdk-strava
  passed 62 failed 0
-- concord-sdk-europe-pmc
  passed 53 failed 0
-- concord-sdk-hubspot-crm
  passed 23 failed 0
fmt ok
 examples/europe-pmc-sdk/src/pagination.rs  | 16 +++++++++
 examples/europe-pmc-sdk/tests/workflows.rs | 27 ++++++++++++++
 examples/strava-sdk/src/auth.rs            | 56 ++++++++++++++++++++++++++----
 3 files changed, 93 insertions(+), 6 deletions(-)
```
(clippy `-D warnings` and rustdoc `-D warnings` printed no diagnostics for the three packages.)
