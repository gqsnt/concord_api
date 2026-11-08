# strava / e4: add an endpoint like its sibling, with its policy classification

**Provider changelog.** This SDK now needs `POST /api/v3/activities` ("create a manual
activity"): a JSON body `{"name": .., "sport_type": .., "start_date_local": .., "elapsed_time": ..}`
creates an activity without a file and returns the created activity document (the same
projection as `GET /api/v3/activities/{id}`). Strava documents for it:
- it requires the `activity:write` scope;
- it is a write: a request that may have reached the server and got no answer (or a 5xx) may
  have created the activity, so it must never be resent automatically;
- per Strava's rate-limit page, activity creation (like uploads) is **exempt from the non-upload
  ("read") windows**; it is charged to the overall windows only.
Nothing else changed.

Add it next to `activities().update(..)`.

Public names the hidden check uses:
- `concord_sdk_strava::NewActivity` with public fields `name: String`, `sport_type: String`,
  `start_date_local: String`, `elapsed_time: u64`, sent as those JSON members;
- `strava.activities().create(activity: NewActivity)`, whose `send().await` returns
  `Result<Response<Activity>, Error>`;
- a stored token whose granted scopes lack `activity:write` makes it fail with
  `Error::MissingScope("activity:write")` before anything is sent;
- a failure after dispatch (5xx, no answer) is reported as uncertain (`error.is_ambiguous()`)
  and is never resent.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
