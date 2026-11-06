# Sources and fixture provenance

Official documentation was re-opened on **2026-11-06**. Slack publishes these as unversioned
Web API documentation; no API version or page revision identifier was advertised in the rendered
pages. The contract pin for this SDK is the content observed on that date, for the `users.list`
and `conversations.info` method names below, not an invented numbered Slack release.

| Key | Official page | Sections used | Version / access date |
| --- | --- | --- | --- |
| P | [Pagination](https://docs.slack.dev/apis/web-api/pagination/) | Cursor-based pagination: Just the facts, Walkthrough, Error conditions | Unversioned Web API docs / 2026-11-06 |
| R | [Rate limits](https://docs.slack.dev/apis/web-api/rate-limits/) | Responding to rate limiting conditions | Unversioned Web API docs / 2026-11-06 |
| W | [Slack Web API](https://docs.slack.dev/apis/web-api/) | Basic overview, Evaluating responses, Authentication | Unversioned Web API docs / 2026-11-06 |
| U | [users.list](https://docs.slack.dev/reference/methods/users.list/) | Facts, Arguments, Response, Errors | Unversioned method reference / 2026-11-06 |
| C | [conversations.info](https://docs.slack.dev/reference/methods/conversations.info/) | Facts, Arguments, Response (public channel, direct message, missing channel), Errors | Unversioned method reference / 2026-11-06 |

All committed fixture bytes are **hand-authored**, not captured from a workspace. The pages
were actually consulted, including their displayed success/error examples, but no fixture is
an unchanged vendor example. JSON is compact UTF-8 (ASCII subset) with a final LF; there is no
BOM. IDs, names, timestamps, messages, cursors, and request IDs were replaced or invented.
The synthetic auth string `synthetic-slack-token` is supplied by tests, never copied from docs.
`.gitattributes` sets `fixtures/** -text` to preserve these bytes on Windows.

Each filename below is relative to `fixtures/`. Keys refer to the official page/section and
the version/access-date pin above. “Synthetic” describes the actual fixture bytes, including
when their shape is documented.

| Fixture file | Source keys / section | Transformation and synthetic cases |
| --- | --- | --- |
| `users-first.json` | P / Walkthrough; U / Response; W / Evaluating responses | Synthetic short first page based on the documented envelope. Reduced user projection; invented user/profile/cache values and warnings. Cursor `opaque=+/&?%2F# space` is a deliberate reserved-character perturbation, not a vendor cursor. |
| `users-empty-continuing.json` | P / Just the facts | Synthetic zero-item page with a nonempty continuation `second=cursor`; tests continuation independent of item count. |
| `users-last-empty.json` | P / Walkthrough | Synthetic final user with empty-string cursor. Deactivated-user flag follows U / Usage info. |
| `users-last-null.json` | P / Just the facts | Synthetic empty page with null cursor. |
| `users-last-missing-cursor.json` | P / Just the facts | Synthetic metadata containing a warning and no cursor. |
| `users-last-missing-metadata.json` | P / Walkthrough | Synthetic empty page with no metadata envelope. |
| `users-last-null-metadata.json` | P / termination rules (adjacent defensive case) | Synthetic null metadata envelope. This tolerance is an SDK choice; the docs explicitly discuss null cursors, not null metadata objects. |
| `users-whitespace-cursor.json` | P / Just the facts, Walkthrough | Synthetic nonempty whitespace/padding cursor. Tests that the SDK never trims opaque values; not claimed as a Slack-issued value. |
| `conversation.json` | C / public-channel Response and optional member count | Synthetic public-channel projection with locale/member count and a nested topic. Omitted unrelated fields and replaced content/IDs. |
| `conversation-dm.json` | C / direct-message Response | Synthetic reduced DM shape, omitting `name` and retaining `user` and `created`. |
| `channel-not-found.json` | C / missing-channel Response | Hand-authored compact rendering of the documented `ok: false` and `channel_not_found` shape; not a captured response. |
| `invalid-cursor.json` | P / Error conditions; U / Errors | Synthetic failure envelope using the documented error code. |
| `missing-scope.json` | C / Errors; W / Evaluating responses | Synthetic `missing_scope` failure. The `needed`, `provided`, and response messages in these bytes are test additions, not copied from a displayed example on these pages. |
| `unknown-error.json` | W / Evaluating responses; U / Errors | Synthetic future code and warning to test forward compatibility. |
| `rate-limited.json` | R / Responding to rate limiting conditions; U and C / Errors | Synthetic JSON failure using the documented `ratelimited` code. R supplies the HTTP 429 and `Retry-After: 30` example, not this JSON body. Headers are scripted in tests. |
| `missing-members.json` | U / Response (negative mutation) | Synthetic malformed success envelope with its users payload removed. |
| `missing-channel.json` | C / Response (negative mutation) | Synthetic malformed success envelope with its conversation removed. |
| `missing-ok.json` | W / Evaluating responses (negative mutation) | Synthetic malformed envelope with required `ok` removed. |
| `missing-error.json` | W / Evaluating responses (negative mutation) | Synthetic failure envelope with required machine-readable error removed. |
| `conflicting-error.json` | W / Evaluating responses (negative mutation) | Synthetic contradictory success/error envelope, rejected by SDK policy. |
| `invalid-cursor-type.json` | P / cursor envelope (negative mutation) | Synthetic numeric cursor; must fail decoding rather than silently terminate. |
| `malformed.json` | W / JSON envelope (negative mutation) | Intentionally truncated synthetic JSON. Also reused as a prefix in a broken chunked transfer. |
| `proxy-error.html` | No Slack example; adjacent HTTP robustness test | Entirely synthetic HTML proxy response. Used as both a mislabeled success and an opaque 429 body. |

The loopback scripts add synthetic response IDs and HTTP headers. Missing/invalid/duplicate
`Retry-After` headers (including overflow and an HTTP date), empty 429 bodies, a 65,537-byte
error body, and interrupted chunked transfers are generated in `tests/protocol.rs`. These are
synthetic robustness cases, not claims about documented Slack behavior. The virtual-clock test
in `tests/workflows.rs` models an application waiting 30 seconds and explicitly trying again;
there is no SDK retry implementation.

Concord evidence: frozen checkout `e04236ca`, root `README.md`, the caller/author guides,
custom-format and recipe docs, the `concord-pagination` and `concord-extras` READMEs, and
generated `concord` rustdoc (`cargo doc -p concord --no-deps --features csv,jsonl`). The existing
PokéAPI metadata adapter and GitHub declarations were read as references. This SDK uses the
public response boundary rather than copying their native URL-following behavior.

Dependencies: `concord` supplies the macro, transport boundary and JSON facade;
`concord-pagination` supplies completion-aware page/item traversal; `serde`/`serde_json` decode
the provider envelopes and preserve unmodeled fields. No generic signing/XML protocol is
needed for bearer-authenticated JSON. Dev-only `concord-example-testserver`, `tokio` (including
virtual time), and `futures-util` provide deterministic transport/stream tests. No vendor SDK,
new external package version, or live provider call was introduced.
