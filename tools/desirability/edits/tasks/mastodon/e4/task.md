# mastodon / e4: add an endpoint like its sibling

**Provider changelog.** This SDK now needs `GET /api/v1/statuses/{id}/context`, which returns the
thread around a status: `{"ancestors": [Status, ..], "descendants": [Status, ..]}` (each entry the
same status document as `GET /api/v1/statuses/{id}`). Errors use the ordinary Mastodon error
document. Nothing else changed.

Expose it like `status`, through the same public client and with the same error handling.

Public names the hidden check uses:
- `Mastodon::status_context(id: StatusId)`, whose `send().await` returns
  `Result<StatusContext, Error>`;
- `concord_sdk_mastodon::StatusContext` with public fields `ancestors: Vec<Status>` and
  `descendants: Vec<Status>`;
- a non-2xx answer is `Error::Api` with the error document, as for `status`.
