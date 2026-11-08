# strava / e5: one field of the provider error document

**Provider changelog.** In Strava's error document (`{"message": .., "errors": [{"resource": ..,
"field": .., "code": ..}]}`) each entry of `errors` may now carry an optional `detail` string, a
human-readable explanation of that field failure. Entries without it are unchanged. Nothing
else changed.

Public names the hidden check uses:
- `ProblemField::detail: Option<String>` (public field), reachable from
  `Error::Http(failure)` → `failure.document` → `ErrorDocument::Problem(problem)` →
  `problem.errors[i].detail`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
