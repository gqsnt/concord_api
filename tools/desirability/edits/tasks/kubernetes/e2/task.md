# kubernetes / e2: shared header (add an optional client-wide header)

**Provider changelog.** This SDK now supports user impersonation: a client configured to act as
another user sends `Impersonate-User: <user>` on **every** request (lists, reads, watches and the
version endpoint); a client without it sends no such header, as before. Nothing else changed.

Public names the hidden check uses:
- `KubeContext::with_impersonate_user(self, user) -> KubeContext` (`user`: `&str` or `String`),
  chainable like `with_base_url` / `with_bearer`; namespace contexts created from it inherit it;
- without it, no `Impersonate-User` header is sent.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
