# mastodon / e3: optional query value on every list operation of a resource

**Provider changelog.** Both timeline list operations, the public timeline
(`GET /api/v1/timelines/public`) and the hashtag timeline (`GET /api/v1/timelines/tag/{hashtag}`),
accept an optional boolean `exclude_replies` query value that leaves out statuses that are
replies. Unset, nothing is sent and the server default (replies included) applies. Nothing else
changed.

Public names the hidden check uses:
- `mastodon.public_timeline().set_exclude_replies(bool)` and
  `mastodon.hashtag_timeline(tag).set_exclude_replies(bool)` send `exclude_replies=<bool>`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
