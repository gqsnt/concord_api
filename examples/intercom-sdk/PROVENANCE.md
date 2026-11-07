# Provenance and assistance

Author date: **2026-11-07**. Frozen workspace baseline:
`8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. Live provider calls: **NOT_RUN**.
No request was sent to `api.intercom.io`; only developer documentation was fetched.
Fixtures and test expectations were authored from the contracts below before
their corresponding protocol tests, rather than generated from SDK output.

## Official pages retrieved

All pages below were read on **2026-11-07**, using the documentation's then-current
2.17 reference. Summaries are paraphrases; fixture identities and contact data
are synthetic. Navigation links alone were not treated as consulted content.

| Exact page | Used for |
| --- | --- |
| https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/listcontacts | GET /contacts, Bearer/version headers, per_page 10/150, opaque starting_after, list envelope; 400 and 401; invalid_per_page and client_error |
| https://developers.intercom.com/docs/references/rest-api/errors/rate-limiting | 429 can lack headers; limit/remaining/reset names; reset is UNIX seconds; rate_limit_exceeded |
| https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/createcontact | POST /contacts, email create mode, selected contact response fields, 200/400/401/409/422 |
| https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/showcontact | GET /contacts/{contact_id}; 401/404/410; merged contact and canonical Link; numeric error codes. Web extraction failed on markdown media; retrieved successfully read-only with curl and Accept: text/markdown |
| https://developers.intercom.com/docs/build-an-integration/learn-more/rest-apis/pagination | Cursor pagination overview; followed the list-pagination guide |
| https://developers.intercom.com/docs/build-an-integration/learn-more/rest-apis/pagination-cursor | pages.next.starting_after and absent next as termination; an empty page is not used as the continuation rule |
| https://developers.intercom.com/docs/references/rest-api/errors/error-codes | Extensible provider code vocabulary; fixture code matrix. Status/code associations in synthetic matrix are illustrative, not a claim of exhaustive provider mapping |
| https://developers.intercom.com/docs/references/rest-api/errors/error-objects | error.list, optional request_id/message/data, errors array |
| https://developers.intercom.com/docs/references/rest-api/errors/http-responses | All listed HTTP failure classes, rate/version header availability and Retry-After |
| https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/showcontactbyexternalid | Accidentally opened via navigation while locating get-by-ID. Confirmed this is a different operation; no endpoint or fixture behavior taken from it |

## Fixture derivation

Every fixture is **authored synthetic**, shaped from the cited contract; none is
a captured live response. IDs, emails, names, cursors and secrets are invented.
Bodies intentionally omit fields outside the SDK's selected model. JSON member
names are protocol vocabulary. Negative specimens intentionally violate it.

| Fixture path under `fixtures/` | Basis and intentional construction |
| --- | --- |
| `contact-first.json`, `contact-created.json` | Get/create contact schemas; synthetic 24-character Intercom IDs, nullable external_id, selected role/email/name; unknown custom_attributes exercises forward compatibility |
| `create-email.json` | Create contract's email mode; synthetic email with optional name and lead role; independent expected POST body |
| `list-first.json` | List and pagination guide; first synthetic contact plus punctuation-bearing next cursor to verify encoding |
| `list-last.json` | List and pagination guide; second synthetic contact, no next property despite present numeric counts |
| `list-empty-next.json` | Synthetic empty intermediate page retaining a valid next cursor |
| `list-null-pages.json` | List schema permits pages=null; synthetic terminal empty response |
| `list-missing-pages.json` | Synthetic malformed list envelope, distinct from explicit null |
| `list-empty-cursor.json` | Synthetic empty next cursor; adapter must refuse rather than refetch start |
| `rate-error.json` | Error Objects and Rate Limiting; synthetic rate_limit_exceeded envelope with no request_id |
| `echo-error.json` | Synthetic request ID, code, message and details echoing SYNTHETIC_SECRET; redaction oracle |
| `error-cases.json` | Independent synthetic table covering every code in Error Codes, list's invalid_per_page, get's contact_merged, integer 422, every HTTP response failure listed above and unknown status/code 418/future_error_code |
| `malformed.json`, `truncated.json`, `wrong-shape.json` | Synthetic invalid syntax, incomplete JSON and wrong identifier type; all contain a secret sentinel |
| `tests/protocol.rs::damaged` | Synthetic fixture generator for empty, wrong/missing/repeated media, limit+1 bytes with Content-Length and chunked transfer, and transport truncation via Body::FailAfter. Large bodies generated at runtime to avoid storing MiB fixtures |
| Inline test responses/headers | Synthetic 307 Location, 410 canonical Link, wrong discriminator and missing cursor member, absent/invalid/duplicate/overflow rate headers, numeric Retry-After and echoed cookie/header. Based on the HTTP/rate/error contracts; negative cases are not claimed provider examples |

## Files read and example assistance

Read the workspace `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, and `docs/RECIPES.md`. Read all five capability READMEs:
`crates/concord-{auth,pagination,retry,ratelimit,codecs}/README.md`.
Read source rustdoc for `concord`, `concord-core`'s `Endpoint`, `SendWith`,
`Prepared`, `Received`, `ResponseMeta`, `Error`; `concord-codecs::Json`;
`concord-auth::bearer` and `CredentialError`; `concord-pagination::{Pages,Page,Resume,PagesError}`;
and `concord-retry::http::{retry_after,RetryAfterError}`.
Manifest reads: root `Cargo.toml`, `crates/concord/Cargo.toml`, and the testserver
manifest. File listings/search results were used to locate these, not as SDK evidence.

No AGENTS.md or CLAUDE.md exists in the worktree or SDK subtree. Read the parent
handoff's `AGENTS.md`, `CLAUDE.md`, `START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`,
`AUTHORITY.md`, `contracts/PHASES.md` and `contracts/RESOURCES.md` for scope and
resource rules. The user's narrower SDK capsule controls ownership and forbids
commits/shared changes. The managed cloud runtime skill and networking reference
were read; no environment_status tool or local network-policy file was available.

This is **example-assisted**, not an unaided first-use claim. Every example
file whose content or embedded excerpt was consulted is listed below. Linked
files merely mentioned by a guide were not opened and contributed no code.

| Example file | How consulted and what was taken |
| --- | --- |
| `examples/testserver/src/lib.rs` | Direct source/rustdoc read: TestServer::start, Recorded, Reply, Body::Chunks/FailAfter, requests and shutdown. Used as required loopback harness; not copied or modified |
| `examples/testserver/Cargo.toml` | Direct manifest read: package name and Tokio test support |
| `examples/quickstart/src/lib.rs` | Embedded README/Quickstart excerpts only: declaration syntax, sensitive bearer header, generated constructors/setters, bounded error-document pattern |
| `examples/quickstart/src/main.rs` | Embedded Quickstart excerpt only: local base URL override and synthetic token |
| `examples/shapes/tests/wire.rs` | Embedded Quickstart excerpt only: subdomain resolution; considered but not needed by Intercom |
| `examples/fred-sdk/src/lib.rs` | Author Guide's attributed transport recipe only: disable automatic redirects/retries for explicit wire accounting; Intercom uses header auth, not FRED's query auth |
| `examples/openai-sdk/src/lib.rs` | Embedded Author Guide multipart excerpts only: considered format ownership; no multipart code used |
| `examples/google-drive-sdk/src/upload.rs` | Embedded Author Guide multipart-related excerpt only: no behavior/code used for this JSON-only slice |
| `examples/frankfurter-sdk/src/lib.rs` | Embedded Author Guide send-policy excerpt only: bounded document with authoritative status and boxed provider error; shaped the generic ResponsePolicy |
| `examples/native/src/native_edit.rs` | Embedded Recipes snippet only: native query mutation; not needed because Intercom fits declared query/path placement |
| `examples/compat/src/lib.rs` | Embedded Recipes resource-segment snippet only: not needed because contact ID is exactly one encoded path segment |

No existing `examples/*-sdk` source file was directly opened. The choices and
their Concord costs are recorded in [FRICTION.md](FRICTION.md).

## Dependency provenance

No new registry crate or version was introduced. Cargo added only the
`concord-sdk-intercom` 0.0.0 package block; every pre-existing package/version,
checksum and dependency list remains unchanged. Workspace versions reused:
serde 1.0.229 and serde_json 1.0.154 (MIT OR Apache-2.0), Tokio 1.54.1 (MIT, tests),
plus the existing Concord 4.2.0 packages (MIT OR Apache-2.0) and testserver 0.0.0.
Concord's existing reqwest/TLS dependency tree is reused. `concord-retry` is
used without optional features, only for its strict HTTP header parser.

No sub-agents were used for this bounded author task. No model-settings or
independent-review claim is made. The records contain observable actions and
decisions, not private reasoning transcripts.

## Maintenance: exact-email search (2026-11-07)

Maintainer baseline: `8a8befa6ebed33904bde7d28bbfbcc9acbcc4919`. Live provider
calls: **NOT_RUN**. The initial implementation and fixture run preceded reading
`FRICTION.md` and `FIRST_USE.md`; the maintenance exposure and file ledger is in
[MAINTENANCE.md](MAINTENANCE.md).

Official pages read on **2026-11-07**:

| Exact page | Maintenance use |
| --- | --- |
| https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/searchcontacts | POST search; email equality query; search default 50, range 1..=150; contact-list response; indexing delay |
| https://developers.intercom.com/docs/build-an-integration/learn-more/rest-apis/pagination | Search pagination section: body pagination object, opaque next cursor, same query on subsequent pages, no snapshot guarantee |
| https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/listcontacts | Rechecked listing's distinct query parameters and default 10; kept existing wire contract |
| https://developers.intercom.com/docs/references/rest-api/errors/rate-limiting | Rechecked that 429 may lack rate headers; unknown reset must stay unknown |

`tests/search.rs` reuses the existing synthetic list/error fixtures because
search has the same response envelope. Its request oracle is independently
specified from these pages. `tests/search_consumer.rs` supplies synthetic
same-email matches, an opaque cursor, an intentionally unhelpful total count
and get-by-ID replies. Neither test captures a live response.

No dependency was added or upgraded. The package and workspace member already
existed at the maintenance baseline; the initial unlocked offline build left
`Cargo.lock` unchanged. No crate/license justification is therefore needed.
