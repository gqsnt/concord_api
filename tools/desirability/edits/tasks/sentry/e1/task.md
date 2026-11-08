# sentry / e1: version path prefix

**Provider changelog.** Sentry moved its web API from `/api/0/` to `/api/1/`: every endpoint
(`/api/0/organizations/{org}/issues/`, `.../issues/{id}/`, `.../events/`, ...) is now under
`/api/1/` with the same remainder. Pagination `Link` headers point to the new paths. Documents,
statuses, headers and rate limits are unchanged. Nothing else changed.

Public names the hidden check uses (unchanged names, new behaviour):
- `sentry.organization(org).issues()`, `.issue(id).get()` and `.issue(id).events()` request
  `/api/1/organizations/{org}/...`, and a `rel="next"` link under `/api/1/` still yields
  `Response::next_cursor`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
