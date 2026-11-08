# strava / e2: shared header (add a header to every API request)

**Provider changelog.** Strava now requires every API request (everything under `/api/v3/`) to
name the calling application in the header `X-Strava-Client-Id: <client id>`, the numeric client
ID of the application's OAuth credentials. The OAuth token endpoint is not affected. Requests
without it will be rejected. Nothing else changed.

Public names the hidden check uses:
- a `Strava` client built from `Session::connect(.., Credentials::new(client_id, secret), ..)`
  sends `X-Strava-Client-Id: <client_id>` on every API request (reads, the activity update, an
  upload with `UploadFile::new(..)` and its status), with each session's own client ID;
- the OAuth token requests (the session's refresh, and an exchange through
  `OAuth::new(transport(..)?).with_base_url(..)?.oauth().token(TokenRequest::authorization_code(..)?)`)
  are unchanged.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
