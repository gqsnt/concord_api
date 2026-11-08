# notion / e2: rename a shared header

**Provider changelog.** Notion renamed the request header that selects the API version:
requests now send `Notion-API-Version` instead of `Notion-Version`. The value and its format are
unchanged, and it is still sent on every request. Responses are unchanged (a response may still
carry its own `Notion-Version` header). Nothing else changed.

Public names the hidden check uses:
- `NotionClient::new(http, token)` sends `Notion-API-Version: <NOTION_VERSION>` and no
  `Notion-Version` request header;
- `NotionClient::with_version(v)` changes the value of `Notion-API-Version`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
