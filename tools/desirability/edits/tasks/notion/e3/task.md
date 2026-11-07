# notion / e3: optional query value on every list operation

**Provider changelog.** Both list operations of this slice, the data source query
(`POST /v1/data_sources/{id}/query`) and block children (`GET /v1/blocks/{id}/children`), accept
an optional `archived` query value (`true` or `false`) that filters archived results. Without it
the provider's default applies, as before. Nothing else changed.

Public names the hidden check uses (the generated operations, reached through
`NotionClient::api()`):
- `client.api().query(data_source_id, body).set_archived(bool)` sends `?archived=<bool>`;
- `client.api().children(block_id, page_size).set_archived(bool)` sends `archived=<bool>`
  next to its other query values;
- without the setter nothing new is sent.
