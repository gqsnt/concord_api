# notion / e1: API version

**Provider changelog.** Notion released API version `2026-09-03`. This SDK now targets it: every
request must carry `Notion-Version: 2026-09-03` by default (callers can still choose another
version explicitly with `with_version`). Paths, documents and every other behaviour are
unchanged for this SDK's operations. Nothing else changed.

Public names the hidden check uses:
- `concord_sdk_notion::NOTION_VERSION` must be `"2026-09-03"`;
- `NotionClient::new(http, token)` sends `Notion-Version: 2026-09-03` on every request.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
