# kubernetes / e5: one field of the provider error document

**Provider changelog.** The `Status` error document's `details` object carries `name`, the name
of the object the failure concerns (`{"kind":"Status",..,"reason":"NotFound","details":
{"name":"web","kind":"pods"},"code":404}`); this SDK must now surface it. It may be absent.
Nothing else changed.

Public names the hidden check uses:
- `model::StatusDetails::name: Option<String>` (public field), reachable from
  `ApiError::read(error).await?.status()?.details`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
