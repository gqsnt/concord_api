# sentry / e4: add a paged endpoint like its sibling, with its policy classification

**Provider changelog.** This SDK now needs an issue's grouping hashes,
`GET /api/0/organizations/{org}/issues/{id}/hashes/`: a paged list exactly like the issue's
events (`per_page`, default 100, and an opaque `cursor`; Sentry's `Link` header with
`results="true"`/`"false"` drives the next page). Each item is `{"id": "<hash>", ..}`. It is a read:
like the other reads it shares the SDK's local admission (its own endpoint bucket) and its
bounded retries of 429 and 502–504. Nothing else changed.

Add it next to `events`.

Public names the hidden check uses:
- `organization(org).issue(id).hashes()` with `set_per_page(u32)` and `set_cursor(..)`, whose
  `send().await` returns `Result<Response<Vec<IssueHash>>, Error>` with `Response::next_cursor`
  taken from the `Link` header like the event list;
- `concord_sdk_sentry::IssueHash` with a public field `id: String`;
- `.pages(from, max_pages)` on the operation, like `events()`.
