# Provenance

Example-assisted SDK, authored against frozen Concord commit `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. Provider documentation read 2026-11-07:

- [API overview](https://www.postmarkapp.com/developer/api/overview): server-token authentication, status classes and error format.
- [Email API](https://postmarkapp.com/developer/api/email-api): single-send request and acceptance.
- [Messages API](https://postmarkapp.com/developer/api/messages-api): count/offset search, limits, message details and events.

| Fixture | Derivation |
| --- | --- |
| `acceptance.json` | Email API single-send response example, reproduced as JSON. |
| `list.json` | Messages API outbound search example, reproduced as JSON. |
| `details.json` | Messages API outbound details example, shortened to one delivery event and its delivery message. |
| `error.json` | Overview's 422 error-document example. |
| Workflow detail responses | Synthetic variants of `details.json` with ID substituted to match the documented send and list examples; all other values retained. |
| Test-generated malformed, truncated, oversized, wrong-media, delayed, and sensitive-message responses | Synthetic, authored for failure testing. |

No new third-party packages were added. Existing workspace dependencies: `concord` 4.2.0 (MIT OR Apache-2.0), `serde` 1.0.229 (MIT OR Apache-2.0), `serde_json` 1.0.154 (MIT OR Apache-2.0), `tokio` 1.54.1 (MIT); `concord-example-testserver` 0.0.0 (MIT OR Apache-2.0) is test-only. Cargo.lock adds only this SDK package. Live provider calls: **NOT_RUN**.
