# Provenance

Example-assisted SDK, authored against frozen Concord commit `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. Provider documentation read 2026-11-07:

- [API overview](https://www.postmarkapp.com/developer/api/overview): server-token authentication, status classes and error format.
- [Email API](https://postmarkapp.com/developer/api/email-api): single-send request and acceptance.
- Maintenance, read 2026-11-07: [Email API, "Send batch emails"](https://postmarkapp.com/developer/api/email-api) (`POST /email/batch`, a JSON array of single-send messages, up to 500 messages and 50 MB per call; the response is an array in submission order, 200-level even when individual messages fail validation, each with `ErrorCode` and `Message`; the rejected example (406) omits `MessageID`, `SubmittedAt` and `To`) and the [API overview](https://www.postmarkapp.com/developer/api/overview) "HTTP response codes" (413: 50 MB batch limit) and "API error codes" (406 inactive recipient, 410 batch limit of 500, both HTTP 422).
- [Messages API](https://postmarkapp.com/developer/api/messages-api): count/offset search, limits, message details and events.

| Fixture | Derivation |
| --- | --- |
| `acceptance.json` | Email API single-send response example, reproduced as JSON. |
| `list.json` | Messages API outbound search example, reproduced as JSON. |
| `details.json` | Messages API outbound details example, shortened to one delivery event and its delivery message. |
| `error.json` | Overview's 422 error-document example. |
| `batch.json` | Email API batch response example: field names, the accepted entry's values and the rejected entry's code reproduced; the rejected entry's `Message` shortened to one sentence that still names a recipient address. |
| Workflow detail responses | Synthetic variants of `details.json` with ID substituted to match the documented send and list examples; all other values retained. |
| Test-generated malformed, truncated, oversized, wrong-media, delayed, and sensitive-message responses | Synthetic, authored for failure testing. |
| Test-generated batch responses (mixed consumer batch, count mismatches, accepted entries without IDs, a non-zero code with acceptance fields, the 410 refusal of 501 messages) | Synthetic, built from the documented batch entry shapes and error codes. |

No new third-party packages were added (batch counting uses `serde::de::IgnoredAny`, already a dependency). Existing workspace dependencies: `concord` 4.2.0 (MIT OR Apache-2.0), `serde` 1.0.229 (MIT OR Apache-2.0), `serde_json` 1.0.154 (MIT OR Apache-2.0), `tokio` 1.54.1 (MIT); `concord-example-testserver` 0.0.0 (MIT OR Apache-2.0) is test-only. Cargo.lock adds only this SDK package. Live provider calls: **NOT_RUN**.
