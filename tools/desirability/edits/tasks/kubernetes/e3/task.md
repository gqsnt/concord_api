# kubernetes / e3: optional query value on every list operation of a resource

**Provider changelog.** The Pod list operations (`GET /api/v1/pods` and
`GET /api/v1/namespaces/{namespace}/pods`) accept an optional boolean `pretty` query value that
asks the server to pretty-print the response (the same parameter the other list endpoints
document). Unset, nothing is sent, as before. Nothing else changed.

Public names the hidden check uses:
- `kube.core().pods().set_pretty(bool)` and `kube.namespace(ns).core().pods().set_pretty(bool)`
  send `pretty=<bool>` (adding it to every list operation is allowed).

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
