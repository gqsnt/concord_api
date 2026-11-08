# nager-holidays / e3: optional query value on the list operation

**Provider changelog.** The holiday list (`GET /api/v4/Holidays/{CountryCode}/{Year}`) accepts an
optional `subdivisionCode` query value (an ISO 3166-2 code such as `DE-BY`) that restricts the
result to that subdivision. Without it the whole country is returned, as before. Nothing else
changed.

Public names the hidden check uses:
- `client.holidays(country_code, year).set_subdivision_code(code)` sends
  `?subdivisionCode=<code>` (`code`: `&str` or `String`);
- without the setter no query string is sent.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
