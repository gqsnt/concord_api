# mastodon / e5: one field of the provider error document

**Provider changelog.** Mastodon error documents may now carry an optional `error_description`
string next to `error` (`{"error": "invalid_token", "error_description": "The access token is
invalid"}`). Documents without it are unchanged. Nothing else changed.

Public names the hidden check uses:
- `ErrorDocument::error_description(&self) -> Option<&str>`, reachable from
  `Error::Api(api)` via `api.document()`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
