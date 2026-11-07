# notion / e1: API version

**Provider changelog.** Notion released API version `2026-09-03`. This SDK now targets it: every
request must carry `Notion-Version: 2026-09-03` by default (callers can still choose another
version explicitly with `with_version`). Paths, documents and every other behaviour are
unchanged for this SDK's operations. Nothing else changed.

Public names the hidden check uses:
- `concord_sdk_notion::NOTION_VERSION` must be `"2026-09-03"`;
- `NotionClient::new(http, token)` sends `Notion-Version: 2026-09-03` on every request.
