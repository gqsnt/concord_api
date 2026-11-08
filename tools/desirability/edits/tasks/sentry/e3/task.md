# sentry / e3: optional query value on every list operation

**Provider changelog.** Both list operations of this slice, the organization issue list
(`GET /api/0/organizations/{org}/issues/`) and an issue's event list
(`GET /api/0/organizations/{org}/issues/{id}/events/`), accept an optional `environment` query
value restricting results to one environment (for example `production`). Unset, nothing is sent
and every environment is included, as before. The pagination `Link` keeps the filter. Nothing
else changed.

Public names the hidden check uses:
- `organization(org).issues().set_environment(env)` and
  `organization(org).issue(id).events().set_environment(env)` send `environment=<env>`
  (`env`: `&str` or `String`).

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
