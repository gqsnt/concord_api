# notion / e2: rename a shared header

**Provider changelog.** Notion renamed the request header that selects the API version:
requests now send `Notion-API-Version` instead of `Notion-Version`. The value and its format are
unchanged, and it is still sent on every request. Responses are unchanged (a response may still
carry its own `Notion-Version` header). Nothing else changed.

Public names the hidden check uses:
- `NotionClient::new(http, token)` sends `Notion-API-Version: <NOTION_VERSION>` and no
  `Notion-Version` request header;
- `NotionClient::with_version(v)` changes the value of `Notion-API-Version`.
