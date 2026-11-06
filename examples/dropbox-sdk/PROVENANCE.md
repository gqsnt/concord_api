# Sources and fixture provenance

Access date for every source below: **2026-11-06**. Target: **Dropbox API v2**
(`https://api.dropboxapi.com/2`), using the documentation as served on that date.
No live Dropbox API calls, credentials, SDK recordings, or provider responses were
used. All 28 fixture files were hand-authored. Examples present in the official
pages informed their shape; none is represented as an unchanged captured response.
Fixture assertions establish this SDK's behavior, not Dropbox conformance.

## Official source pins

The two required legacy guide URLs were re-opened and redirected to the current
documentation site. The guides and endpoint references have no release number;
the access date and SHA-256 of their downloaded `.md` representation pin the
observed content. The Team Files guide itself labels an update **November 10,
2023**. The other guide/reference pages show no comparable revision date.

| Key | Official page and relevant sections | Version/revision used |
|---|---|---|
| T | [Original Team Files URL](https://developers.dropbox.com/en-us/dbx-team-files-guide) → [Team Files](https://docs.dropboxapi.com/dropbox-api/docs/team-files): API Call Roots; Using the Dropbox-API-Path-Root Header; Managing Team Member Content | API v2, page update 2023-11-10, retrieved 2026-11-06 |
| E | [Original Error Handling URL](https://developers.dropbox.com/en-us/error-handling-guide) → [Error Handling](https://docs.dropboxapi.com/dropbox-api/docs/error-handling): Conventions; 400/401/403/409/429/500; Logging; Use Prefix Matching on Errors | Unversioned current guide, retrieved 2026-11-06 |
| L | [List Folder](https://docs.dropboxapi.com/dropbox-api/api-reference/user-endpoints/files/list-folder): request, response, errors, Metadata, AuthError, AccessError, LookupError and RateLimitError types | `/2/files/list_folder`, retrieved 2026-11-06 |
| C | [List Folder Continue](https://docs.dropboxapi.com/dropbox-api/api-reference/user-endpoints/files/list-folder-continue): cursor request, response, ListFolderContinueError/reset | `/2/files/list_folder/continue`, retrieved 2026-11-06 |
| R | [Path Root Header Modes](https://docs.dropboxapi.com/dropbox-api/docs/technical-reference/path-root-header-modes): Home, Root, Namespace, 422 samples | API v2, unversioned current reference, retrieved 2026-11-06 |
| N | [Dropbox's official common.stone](https://github.com/dropbox/dropbox-api-spec/blob/66971685d041dd6ea3acabdcfcf6dc77d09cfe40/common.stone#L33): NamespaceId | Last change to this file: commit `66971685d041dd6ea3acabdcfcf6dc77d09cfe40`, 2026-07-13T18:08:22Z; retrieved 2026-11-06 |

Source content fingerprints (Markdown for T/E/L/C/R; raw Stone for N):

```text
T b742eee45fa9f948c845ddfe582f33c30a359ba70dc33060085b6849572f8d36
E c74ec8697e2ff6d9f82bf66f1254c543347530c43c47e7eef62474343495ed38
L a7adbf7a8fffd429e85639f166929ba78b61ef12553f033e9d7168bd44bbab45
C 15c6e121c974145cfc5b6147693c7abd7247fd5cbe74a76ef77e041c4ba61d09
R c1350c06200cf00aa899d24eff740ccd3de65660ae072f2e31852f96d925467b
N 9768cd1590f13858e80cc233e4266fb9dda7bc46854594db9bb4763271dc99e6
```

L and C specify `files.metadata.read`. T and the endpoint header descriptions
require `team_data.member` for Select-User with a team-linked token. Full Dropbox
access and the selected member's rights govern team content. These are setup
requirements, not permissions granted by the SDK. No team management or account
discovery endpoint is implemented. Root mismatch and namespace denial use HTTP
422; they are not endpoint-specific 409 errors. Current N permits opaque namespace
IDs matching `[-_0-9a-zA-Z:]+`; guide examples containing digits alone do not narrow
that schema.

## Byte fixtures

Every row inherits the version/revision and access date of its cited source key.
JSON response files are UTF-8, two-space indentation and a final LF. Exact request
files are compact UTF-8 with no final newline. `.gitattributes` sets `fixtures/**
-text` so checkout conversion cannot change the bytes. IDs, names, timestamps,
cursor contents, summaries, messages and HTTP header values are synthetic.

| Fixture under `fixtures/` | Official basis | Transformation / synthetic case |
|---|---|---|
| `request-list-root.json` | L request; T/R root listing examples | Empty path; explicitly serialize the supported Boolean defaults |
| `request-list-projects.json` | L request | Same defaults; invented `/Projects` path |
| `request-list-unicode.json` | L request | Invented Unicode/quote/percent-text path; recursive/deleted true, limit 2; byte-encoding stress |
| `request-continue-a.json` | C request | Invented opaque cursor with plus, slash, equals, literal percent text, quote and Unicode; no pre-encoding |
| `request-continue-b.json` | C request | Separate invented B cursor |
| `list-a-first.json` | L response; Metadata.file | One invented file, required file fields, omitted optional fields; has_more true |
| `list-a-last.json` | C response; L Metadata.folder/deleted | Invented folder and tombstone; terminal cursor and has_more false |
| `list-b-first.json` | L response; C continuation rule | Synthetic empty intermediate page with a nonempty cursor and has_more true |
| `list-b-last.json` | L/C response; Metadata.folder | Separate B folder, nullable paths and terminal cursor |
| `path-not-found.json` | L LookupError.not_found; E 409 envelope/user_message | Invented summary suffix, localized message and nested path tag |
| `path-not-folder.json` | L LookupError.not_folder | Hand-authored nested error and summary |
| `reset.json` | C ListFolderContinueError.reset | Hand-authored 409 expired-continuation error |
| `invalid-user.json` | L AuthError.invalid_select_user | Hand-authored 401 member selection failure |
| `missing-scope.json` | L AuthError.missing_scope | Hand-authored 401 requiring files.metadata.read |
| `forbidden.json` | L AccessError.team_access_denied; E 403 | Hand-authored team access failure |
| `invalid-root.json` | R Root sample error | Hand-authored 422 sample shape with root changed to synthetic 300; lean root object follows the guide sample |
| `no-permission.json` | R Namespace sample error | Hand-authored 422 inaccessible namespace shape |
| `rate-limited.json` | L RateLimitError/RateLimitReason; E 429 | Hand-authored struct-shaped error, reason too_many_requests, body delay 7 |
| `future-error.json` | L error unions' forward-compatible other cases | Invented future tag and extra detail; not a documented existing error |
| `plain-error.txt` | E Conventions and 400 | Invented plaintext invalid-argument body, including CRLF; reused for body-independent 429 tests |
| `proxy-error.html` | E 500; L 500 any body | Entirely synthetic HTML intermediary failure with CRLF |
| `malformed.json` | L/C required success JSON; E JSON/plaintext distinction | Deliberately truncated JSON; reused as malformed HTTP error body |
| `missing-entries.json` | L/C required entries | Synthetic invalid success, entries omitted |
| `missing-has-more.json` | L/C required has_more | Synthetic invalid success, completion flag omitted |
| `missing-cursor.json` | L/C required cursor | Synthetic invalid success, cursor omitted |
| `empty-cursor.json` | L/C nonempty cursor constraint | Synthetic invalid success with an empty terminal cursor |
| `missing-file-id.json` | L Metadata.file required id | Synthetic file success shape with required id removed |
| `unknown-entry.json` | L Metadata union | Invented future success tag; deliberately rejected rather than silently omitted |

Test code supplies synthetic response headers and statuses around these bytes.
Request IDs use `synthetic-*`; the sole normal bearer value is
`SYNTHETIC_DROPBOX_TOKEN_NOT_A_CREDENTIAL`. Namespace/member IDs are fixture values,
not real authorized users. R's root JSON modes are asserted as exact header text.
The opaque `ns:Team_A-9` header test is generated from N's current allowed alphabet,
not an assertion that such a namespace exists.

Resource and transport perturbations are synthetic arrangements in the tests:
missing/wrong Content-Type, duplicate/invalid/overflowing Retry-After, header
injection, connection hangup, chunked body truncation, an error body of 64 KiB + 1,
and a success body of 16 MiB + 1. Large homogeneous bodies are generated in test
code rather than committed as large files. The two byte caps are SDK policy.
The concurrency test gates A's body on receipt of B; it is not a recorded provider
timing. Pagination cap and cancelled-poll tests exercise Concord/application behavior.

## Implementation inputs and dependencies

Frozen Concord source: `6d6d66aa8ff408c61b718bea70c3fda9d2e1fb5e`.
Read the workspace README, author/caller/recipe/cheat-sheet/custom-format guides,
crate READMEs and public rustdoc; built `concord` rustdoc with `csv,jsonl` enabled.
The existing Slack SDK supplied a reading pattern for a common response adapter,
owned metadata, safe error-body diagnostics and `Pages` projection. GitHub's SDK
module overview was inspected. Dropbox's declaration, context/cursor model,
provider types and tests were authored for this slice; no vendor SDK executes HTTP.

Direct dependencies are Concord (macro, JSON codec, HTTP/metadata public API),
concord-pagination (page/item/stream completion and partial outcomes), serde derive
and serde_json (standard JSON model serialization and provider error parsing).
No signing, XML, custom HTTP executor or extra protocol library is required.
Dev dependencies are the workspace loopback testserver, Tokio and futures-util
for async tests and standard stream consumption. All registry dependencies were
already locked; Cargo.lock gained only this SDK's package entry.
