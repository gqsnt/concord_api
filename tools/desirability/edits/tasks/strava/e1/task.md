# strava / e1: version path prefix

**Provider changelog.** Strava moved its API from `/api/v3/` to `/api/v4/`: every API endpoint
(`/api/v3/athlete/activities`, `/api/v3/activities/{id}`, `/api/v3/uploads`, ...) is now under
`/api/v4/` with the same remainder. The OAuth token endpoint (`/oauth/token`) did not move.
Documents, statuses, headers and rate limits are unchanged. Nothing else changed.

Public names the hidden check uses (unchanged names, new behaviour):
- `strava.athlete().activities()`, `strava.activities().get(id)`, `strava.uploads().status(id)`
  request `/api/v4/...`;
- the token refresh still posts to `/oauth/token`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
