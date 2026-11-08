# notion / e5: one field of the provider error document

**Provider changelog.** Notion error documents (`{"object":"error","status":..,"code":..,
"message":..}`) now carry an optional `documentation_url` string linking the documentation of the
error. It may be absent. Nothing else changed.

Public names the hidden check uses:
- `ProviderError::documentation_url: Option<String>` (public field), reachable from
  `Error::Provider { error, .. }`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
