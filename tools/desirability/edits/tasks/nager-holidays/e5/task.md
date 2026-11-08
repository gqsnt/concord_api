# nager-holidays / e5: one provider document field (nearest variant: the response document)

**Provider changelog.** In every holiday object of the holiday array, the provider renamed the
boolean field `nationalHoliday` to `global` (same meaning, same values). The old name is no
longer sent. Nothing else changed.

Public names the hidden check uses:
- `Holiday::national_holiday` keeps its Rust name and meaning; it is decoded from `global`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
