# Source and fixture provenance

Access date for every source below: **2026-11-06**. The four locked sources were
re-opened through the web tool during this author pass. The returned article bodies
were readable despite a generic sign-in banner. Version pin: Microsoft Graph REST
**v1.0**, not beta and not a particular Microsoft SDK version. These are documentation
observations as of the access date, not immutable provider contracts.

| Key | Official page / sections used | Displayed last-updated date |
| --- | --- | --- |
| D | [Delta query overview](https://learn.microsoft.com/en-us/graph/delta-query-overview): collection flow, state tokens, optional parameters, replays, synchronization reset, token duration | 2025-04-30 |
| U | [Users delta](https://learn.microsoft.com/en-us/graph/delta-query-users): initial request/response, nextLink/final nextLink, deltaLink response, removed annotations | 2025-01-15 |
| G | [List groups](https://learn.microsoft.com/en-us/graph/api/group-list?view=graph-rest-1.0): HTTP request, optional query parameters, headers/body, response, example 1 | 2025-07-23 |
| M | [List group members](https://learn.microsoft.com/en-us/graph/api/group-list-members?view=graph-rest-1.0): direct membership, v1.0 service-principal issue, limited-information permissions, query parameters, headers/body, response | 2025-07-23 |
| P | [Graph paging](https://learn.microsoft.com/en-us/graph/paging): server-side paging, whole nextLink URLs and continuation headers | 2025-04-03 |
| E | [Graph errors](https://learn.microsoft.com/en-us/graph/errors): HTTP status codes, JSON error resource, recursive inner errors and details | 2025-08-06 |

No fixture was downloaded from Graph, captured from a tenant, or copied byte for
byte from a documented example. **Every fixture is hand-authored** from the named
shape/semantics. All identifiers, people, timestamps, request IDs, tokens, extension
properties and directory contents are synthetic. Email addresses use `.invalid`.
No live provider request or credential was used. The tests deliberately do not
claim that the service emits these exact values or page arrangements.

All paths in the table are relative to `fixtures/`. Every reference below inherits
the v1.0 version and 2026-11-06 access date above.

| Fixture file | Source section(s) | Construction / transformation |
| --- | --- | --- |
| `users-initial.json` | U initial response; D state tokens | Two invented users, sparse fields, null mail and nextLink. Query deliberately includes percent escapes, duplicate unknown query keys and literal plus for byte-preservation tests. |
| `users-empty-intermediate.json` | D collection flow | Synthetic required perturbation: empty `value` with a nextLink; not terminal. |
| `users-final.json` | U final nextLink response; U `/microsoft.graph.delta` note | One invented user and a final deltaLink, using the documented fully qualified path spelling. |
| `users-incremental.json` | U deltaLink response and removed annotations | Invented sparse update, explicit null/false, both removal reasons, one new user and a synthetic extension. |
| `users-no-changes.json` | D delta round flow, opaque state tokens | Synthetic empty completed round with the same final delta URL as the prior request. Used to ensure delta repetition is not mistaken for a next-page loop. |
| `groups-first.json` | G HTTP response/example 1; P server-side paging | Invented security group and paged nextLink. |
| `groups-final.json` | G response/example 1 | Invented Unified group with optional properties absent; no nextLink. |
| `members-first.json` | M direct membership, limited information, query parameters; P continuation headers | Required perturbation: visible user, id/type-only user and device, and an invented future type. Synthetic count, opaque nextLink and unknown property. |
| `members-final.json` | M direct membership and response | Invented user, nested group and organizational contact; no nextLink. The nested group is never traversed. |
| `members-second-group.json` | M limited information and type model | Synthetic user with null name plus a device whose id intentionally collides with the test's known user id, to verify that joins also require a user type. Not a claim about real directory id collisions. |
| `error-unauthorized.json` | E 401 and error resource | Invented authentication code/message, inner request-id/date. |
| `error-forbidden.json` | E 403/insufficient claims, innererror and details; M permissions | Invented permission denial and diagnostic detail. Uses lowercase `innererror` to cover the documented alternative spelling. |
| `error-throttled.json` | E 429 and error resource | Invented throttling envelope. Tests add synthetic Retry-After and request-id response headers. |
| `error-gone.json` | D synchronization reset/token duration; E 410 | Invented expired-state envelope using documented `syncStateNotFound`; tests preserve Location without following it. |
| `error-service.json` | E 503 and error resource | Invented transient failure envelope. |
| `error-bad-request.json` | M unsupported-query response; E 400 | Synthetic Request_UnsupportedQuery error shape; the SDK does not expose unsupported casts. |
| `error-not-found.json` | E 404 and error resource | Invented missing-resource envelope. |
| `delta-both-links.json` | D mutually exclusive nextLink/deltaLink contract | Intentionally invalid synthetic envelope with both links. |
| `delta-no-links.json` | D delta completion contract | Intentionally invalid delta envelope without either link. |
| `collection-with-delta.json` | G ordinary collection response | Intentionally invalid regular-list envelope with a deltaLink. |
| `member-missing-type.json` | M directoryObject/limited-information shape | Intentionally incomplete member missing `@odata.type`; no guessed user type. |
| `missing-value.json` | G collection response | Intentionally invalid envelope missing the required array. |
| `truncated-json.json` | JSON framing for the above responses | Intentionally truncated synthetic body; complete HTTP transfer but invalid JSON. |
| `non-json-error.txt` | E expected JSON shape | Synthetic gateway response outside the expected schema. |

The only fixture substitution is `{{BASE}}` → the current test server's
`http://127.0.0.1:PORT/v1.0` (derived from its received Host header). After this
substitution, opaque link query bytes are unchanged. Files are UTF-8 with LF;
`.gitattributes` marks `fixtures/** -text` to preserve checkout bytes. Response
status/media type/headers are constructed by the loopback responder, not part of
the JSON fixture bytes. Every declared request is GET with exactly zero body
bytes, checked in `tests/protocol.rs` and the workflow wire assertions.

Inline negative cases in the tests are also synthetic: unsafe origins/paths,
user information/fragments/whitespace/backslashes, malformed storage/version,
redirects, unexpected 204, empty identifiers, nested expired-state codes, hung-up
connections, truncated HTTP bodies, page cycles and gated cancellation. Body-size
tests lower the configured cap against fixture bytes. None are vendor examples.

## Implementation sources and dependencies

Concord's library remains at **47a25fbf**. Read material included the workspace
README, caller/author/cheat-sheet/recipes documentation, crate READMEs and the
public API/rustdoc for Prepared, Received, ResponseMeta and Pages. Generated
rustdoc was built with `cargo doc -p concord --no-deps --features csv,jsonl`, using
the prescribed environment prefix. Existing **GitHub SDK** source supplied the
listing/context design pattern; **Salesforce SDK** source supplied the
receive-before-decode metadata pattern. Their campaign records were not read.
No other SDK's friction/first-use/maintenance/integration records and no
`v4/sdk20/` records were consulted.

Direct dependencies: `concord` for the declaration, reqwest re-export and Json
format facade; `concord-pagination` for bounded page/item traversal; `serde` and
`serde_json` for typed JSON, sparse properties and stored links. The existing
reqwest URL parser supplies URL handling; no vendor SDK, handwritten URL parser,
cryptography, XML/signing package or alternate HTTP executor is involved.
Development dependencies are the path `concord-example-testserver`, Tokio and
futures-util (standard Stream consumer tests). All versions already existed in
the workspace lock; the lock change adds only this package's dependency entry.

Author runner requested in the task: Codex gpt-6-astra, xhigh. No sub-agent or
alternate runner was spawned; no tool-reported runtime model/effort evidence was
available to independently attest those settings.
