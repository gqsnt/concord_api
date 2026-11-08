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

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
