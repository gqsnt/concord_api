# sentry / e2: shared header of a scope

**Provider changelog.** Sentry now requires every organization-scoped request (everything under
`/api/0/organizations/{organization}/`, issues, events and status updates included) to repeat
the organization id or slug in the header `X-Sentry-Organization: <organization>`, exactly as
given in the path (unencoded). Nothing else changed.

Public names the hidden check uses:
- every operation reached through `sentry.organization(org)` sends
  `X-Sentry-Organization: <org>`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
