# mastodon / e1: version path prefix of one endpoint family

**Provider changelog.** Mastodon moved the timeline endpoints from `/api/v1/timelines/` to
`/api/v2/timelines/`: `GET /api/v1/timelines/public` is now `GET /api/v2/timelines/public` and
`GET /api/v1/timelines/tag/{hashtag}` is now `GET /api/v2/timelines/tag/{hashtag}`. Their query
values, documents and statuses are unchanged; every other endpoint (statuses, instance,
streaming) keeps its path. Nothing else changed.

Public names the hidden check uses (unchanged names, new behaviour):
- `Mastodon::public_timeline()` requests `/api/v2/timelines/public`;
- `Mastodon::hashtag_timeline(tag)` requests `/api/v2/timelines/tag/{tag}`;
- `Mastodon::status(id)` still requests `/api/v1/statuses/{id}`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
