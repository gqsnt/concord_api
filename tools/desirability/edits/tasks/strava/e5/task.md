# strava / e5: one field of the provider error document

**Provider changelog.** In Strava's error document (`{"message": .., "errors": [{"resource": ..,
"field": .., "code": ..}]}`) each entry of `errors` may now carry an optional `detail` string, a
human-readable explanation of that field failure. Entries without it are unchanged. Nothing
else changed.

Public names the hidden check uses:
- `ProblemField::detail: Option<String>` (public field), reachable from
  `Error::Http(failure)` → `failure.document` → `ErrorDocument::Problem(problem)` →
  `problem.errors[i].detail`.
