# strava / e3: optional query value on the list operation

**Provider changelog.** The athlete's activity list (`GET /api/v3/athlete/activities`) accepts an
optional `sport_type` query value (case-sensitive, for example `Ride`) that restricts the page to
that sport. Unset, every sport is listed, as before. Nothing else changed.

Public names the hidden check uses:
- `strava.athlete().activities().set_sport_type(sport)` sends `sport_type=<sport>` next to the
  existing `page` and `per_page` values (`sport`: `&str` or `String`).

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
