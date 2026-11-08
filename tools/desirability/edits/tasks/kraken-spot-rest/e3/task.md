# kraken-spot-rest / e3: optional query value on the list operation

**Provider changelog.** The asset-pair list (`GET /0/public/AssetPairs`) accepts an optional
`country_code` query value (for example `GB:LND` or `US:TX`) that restricts the result to the
pairs available in that country or region. Unset, every pair is listed, as before. Nothing else
changed.

Public names the hidden check uses:
- `kraken.public().pairs().set_country_code(code)` sends `country_code=<code>`
  (`code`: `&str` or `String`), alone or next to `pair`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
