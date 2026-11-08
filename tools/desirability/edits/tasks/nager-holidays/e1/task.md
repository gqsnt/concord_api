# nager-holidays / e1: version path prefix

**Provider changelog.** Nager.Holidays moved the holiday endpoint from `/api/v4/` to `/api/v5/`:
`GET /api/v4/Holidays/{CountryCode}/{Year}` is now `GET /api/v5/Holidays/{CountryCode}/{Year}`.
The response document, statuses and every other behaviour are unchanged. Nothing else changed.

The SDK pins the provider contract it targets; after this change it targets v5.

Public names the hidden check uses (unchanged names, new behaviour):
- `NagerHolidays::new(http).with_base_url(origin)?.holidays(country_code, year).send()` must
  request `GET /api/v5/Holidays/{country_code}/{year}`;
- `concord_sdk_nager_holidays::API_VERSION` must be `"v5"`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
