# notion / e4: add a list endpoint like its sibling

**Provider changelog.** This SDK now needs Notion's comments list:
`GET /v1/comments?block_id=<id>&page_size=<n>[&start_cursor=<cursor>]` returns a standard Notion
list envelope (`object: "list"`, `results`, `next_cursor`, `has_more`) whose results are comment
objects (`{"object":"comment","id":..,"discussion_id":..,"rich_text":[..],..}`). Errors use the
ordinary Notion error document. Like block children, the cursor travels in the query string.
Nothing else changed.

Add it like `children` / `list_block_children`.

Public names the hidden check uses:
- `concord_sdk_notion::models::Comment` with public fields `id: String` and
  `discussion_id: String`;
- `NotionClient::list_comments(block_id, size: PageSize, cursor: Option<String>)` returning
  `Result<Response<List<Comment>>, Error>`; a contradictory envelope (`has_more` true without a
  `next_cursor`, an empty cursor, or a cursor that did not advance) is `Error::Envelope`, a Notion
  error document is `Error::Provider`.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
