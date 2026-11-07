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
