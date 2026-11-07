# SDK44 maintenance — Backblaze B2 Native

Date: **2026-11-07**. Live provider calls: **NOT_RUN**. All application traffic used
`examples/testserver::TestServer::start` loopback fixtures with synthetic credentials.
Work is left in the working tree; no commit was attempted.

## Prior exposure and order of work

No prior authorship or exposure to this SDK is claimed. At the start of this maintenance turn,
the only supplied SDK context was the user's capsule. The working tree was clean, workspace
membership and the package lock entry already existed, and no worktree `AGENTS.md` was found.

Before reading `FRICTION.md` or `FIRST_USE.md`, inspected the root/package manifests and lock,
the SDK implementation (`src/{lib,wire,model,error,upload,download}.rs`),
`tests/{first_use,protocol,workflows}.rs`, `tests/common/mod.rs`, `README.md` and
`PROVENANCE.md`. Read the six official references below. Ran the original tests, then wrote
the storage implementation, declaration changes and error additions and attempted to compile
the ticket. That attempt failed on an incorrect generated optional-setter name. Only after
this real implementation attempt were `FRICTION.md` and `FIRST_USE.md` read.

Other worktree sources consulted: `crates/concord-macros/src/{syntax,model,emit}.rs` (searches
for body/query declaration and generated setter syntax; the setter implementation was then
read), `examples/testserver/src/lib.rs` (responder bounds, delayed/hung-up replies and cleanup),
and `crates/concord-pagination/README.md` (after implementation, to assess remaining generic
duplication). No other example implementation, parent campaign file, external worktree,
skill or sub-agent was used. Compiler diagnostics also referenced Concord's `IntoValue`
implementations; those references were diagnostics, not additional source research.

## Provider references and decisions

All exact pages below were read on **2026-11-07** through the read-only web tool:

- [b2_list_file_names](https://www.backblaze.com/apidocs/b2-list-file-names)
- [b2_delete_file_version](https://www.backblaze.com/apidocs/b2-delete-file-version)
- [b2_authorize_account](https://www.backblaze.com/apidocs/b2-authorize-account)
- [b2_get_upload_url](https://www.backblaze.com/apidocs/b2-get-upload-url)
- [b2_upload_file](https://www.backblaze.com/apidocs/b2-upload-file)
- [b2_download_file_by_id](https://www.backblaze.com/apidocs/b2-download-file-by-id)

The new endpoints use the existing account-authorized storage declaration at the issued
`apiUrl`. Listing is GET with `bucketId` and optional `startFileName`, `prefix`, `delimiter`,
`maxFileCount`. The traversal keeps the complete query, advances the opaque name cursor only
after a valid page, and completes only on explicit `nextFileName: null`. The listing page's
example loop checks `response.startFileName` for completion, contradicting its schema/prose;
the implementation follows `nextFileName`. Empty pages do not imply exhaustion. Folder IDs
and checksums can be null; upload's stricter verified `FileVersion` was kept separate.

Delete is one JSON POST with name and ID. B2's `file_not_present` is modeled as a provider
error, including for a mismatched pair; it is not converted to success. A success document
must identify the requested pair. `DeleteError` retains the underlying error and a
`may_have_deleted()` flag. Unknown delivery, 5xx, 408 and invalid/unreadable successes stay
uncertain. Known unsent failures and other 4xx refusals do not claim this attempt deleted
anything. No delete, listing request or account authorization is automatically repeated.
This relies on the documented existing HTTP-client contract: redirects and reqwest retries
disabled through `http_client_builder()` or an equivalent transport.

The mandatory request cap counts attempted pages, including failures and cancellation once
the future is polled. A cap returns no more pages but leaves `is_complete()` false and a
full resume query available. Invalid count input sends nothing and consumes no request.
Each page retains the existing 1 MiB JSON bound; requesting 10,000 entries can exceed that
SDK bound. Timeout settings remain caller-owned; cancelling a delete leaves its outcome
unknown even though no `DeleteError` can be returned to a dropped future.

After an uncertain delete the caller lists again. Name listing exposes only current visible
names; removal of the latest version can reveal an older one, and this endpoint cannot
enumerate hidden/older versions. The SDK does not claim that another listing proves every
version's fate, provides a snapshot or establishes exactly-once behavior. Version-history
reconciliation would need a separate endpoint outside this ticket.

## Files and locations edited

All paths in this table are relative to `examples/backblaze-b2-native-sdk/`.

| File / location | Change |
| --- | --- |
| `src/lib.rs`: module/exports and `Session::storage` | Export the new query/page/traversal/result/error types and expose the public storage scope. Existing methods keep their signatures and behavior. |
| `src/wire.rs`: `B2Wire.storage`, `ListFileNamesWire`, delete request/response models | Add the two endpoint declarations and their JSON envelopes; require a present nullable continuation field; reuse `FileVersionWire` and `B2Policy`. |
| `src/storage.rs` (new) | Scope facade, immutable traversal query context, bounded page iteration and resume, nullable folder model, delete response validation and uncertainty wrapper. |
| `src/error.rs`: `InputError`, `ErrorCode`, parse/as_str, `Violation` | Add local count refusal, four provider codes (`invalid_bucket_id`, `out_of_range`, `file_not_present`, `too_many_requests`) and list/delete response violations; broaden `access_denied` documentation. |
| `src/model.rs`: `AccountInfo::api_url` rustdoc | Describe all operations now using the account's API URL. No model behavior changed. |
| `tests/storage.rs` (new) | Seven focused regression tests, including timeout after a deletion that did happen and the same failure when it did not. |
| `tests/storage_consumer.rs` (new) | Public-scope consumer: authorize, list to cap, resume, delete the discovered version, then list again. |
| `tests/storage_support/mod.rs` (new) | Synthetic list/delete fixtures and separate account/API/download origins; every API request checks the account token and issued path prefix. |
| `README.md`: introduction, host table, new traversal/delete example, failures/tests | Explain the scope calls, cap versus completion, complete resume query and limits of reconciliation. Add a compiling caller example. |
| `Cargo.toml`: package description | Include the added operations; no dependency/version changes. |
| `PROVENANCE.md`: original-capture wording and maintenance addendum | Cite new references/date, distinguish original hashes from this read, describe synthetic fixtures and dependency/lock observations. |
| `MAINTENANCE.md` (new) | This record. |

Root `Cargo.toml` already contained the required member and was not edited. `Cargo.lock` is
byte-identical to its pre-maintenance copy: no packages added/removed or versions changed.
No shared crate, check, script, other example, existing consumer assertion or pre-existing
test file was edited. `src/upload.rs`, `src/download.rs`, `FRICTION.md` and `FIRST_USE.md`
are unchanged.

## Facts changed in several places and diagnostics

- The endpoint parameter spellings exist in the macro declaration, public query/request
  projection, independent fixture expectations and caller documentation. Prefix/delimiter and
  count are projected into each generated request; the resume query retains them together.
- Each new provider code needs an enum variant, parser arm and `as_str` arm, plus fixture
  expectations. Enum exhaustiveness would catch an omitted formatter arm, but the parser's
  unknown-code fallback cannot catch a missing known-code arm. The new tests check that the
  four codes produce their intended typed variants rather than `Other`.
- Count maximum/default facts appear in input validation, public rustdoc, README and tests.
  Continuation field/completion facts appear in response decoding, traversal, docs and tests.
- Scope/operation coverage changed in exports, package description, README's host table,
  `AccountInfo::api_url` rustdoc and the wire declaration's introductory comment.
- Initial compile diagnostic E0599 caught `.start_file_name(...)`: generated setters use
  `set_...` / `maybe_...`. The next compile reported E0277 when `Option<String>`/`Option<u16>`
  were passed to `set_...`, including a useful `IntoValue` diagnostic. Reading the generated
  setter implementation established that `maybe_...` accepts `Option`. Correcting all four
  setters resolved those failures. These were API-use mistakes, not a missing provider field.
- No omitted source/declaration edit was exposed by a failing runtime assertion: the first
  compiling implementation passed the existing tests and the added regressions. This does
  not establish that every conceivable omission would be detected. The suite explicitly
  checks missing continuation, preserved filters/cursor, request counts, typed mismatches
  and both possible states behind an indistinguishable timeout.

## Verification

- First Cargo command: `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-backblaze-b2-native --offline`
  (unlocked as requested): **39 runtime tests and 3 doctests passed**.
- Initial locked implementation builds failed only on the setter diagnostics above.
- Final full suite:
  `CARGO_BUILD_JOBS=4 cargo test --locked --offline -p concord-sdk-backblaze-b2-native`:
  **47 runtime tests and 4 doctests passed**, including the original two consumer tests,
  both connected workflows and the consumed-stream/ambiguous-upload failures. Eight runtime
  tests and one compiling documentation example were added.
- `CARGO_BUILD_JOBS=4 cargo clippy --offline --locked -p concord-sdk-backblaze-b2-native --all-targets -- -D warnings`:
  **passed**.
- Formatted only owned source/new test files with `rustfmt --edition 2024`. Git diff confirms
  formatting did not change upload/download or any existing tests. Whitespace and ownership
  boundary checked with Git; the lock was also compared byte-for-byte and by package keys.

All Cargo commands ran one at a time, with four build jobs, offline, using the default
worktree `target/`; after the first build they used `--locked`. No live provider request,
new dependency, version change, approval request or commit was needed.

## Remaining duplication and candidate Concord improvements

The SDK hand-writes generic traversal mechanics: budget charging, cursor update, completion
flag and resumability. Reading `concord-pagination` afterward confirmed that its existing
`sequential` engine already handles bounded dependent traversal, reports and checkpoints.
A follow-up could adapt that engine with `FileNameQuery` as the context/cursor and retain this
public storage facade. The engine also retains an in-flight page across cancelled polls;
this small facade instead abandons it, charges the request and preserves the old cursor.
That is documented behavior, not evidence of a missing Concord capability. No pagination
crate was added merely to replace this already-tested small facade during this ticket.

The generic parts of typed error-code parsing/formatting and wrapper error forwarding are
still repetitive. Reusing `B2Policy`, bounded error decoding, the existing wire file model
and the account-bound scope avoided duplicating transport/auth handling. Before proposing a
core feature for pagination, use the existing engine; no shared change is required here.

Candidate improvements, recorded only:

1. Add a Concord recipe for a generated endpoint with optional fields using `maybe_...`,
   returning a bounded traversal whose checkpoint includes the full filter context. The
   `set_...` type diagnostic could point at `maybe_...` when given an `Option`.
2. Document an adapter from a scope-bound generated operation to
   `concord-pagination::sequential`, including metadata, cap/error resumes and public SDK
   naming. Prefer this over another pagination abstraction.
3. Preserve the earlier author's proposal for explicit runtime scope bases: it would make
   issued account/API/download URL binding clearer. The current `with_base_url` pattern
   already works and all new calls use it correctly.

Provider responsibilities remain in the SDK: account versus upload credentials and issued
URL trust; B2 JSON fields and capability/prefix restrictions; inclusive opaque file-name
continuations; folder nullability; name-list versus version-history semantics; typed
`file_not_present`; checksum verification; stream replay limits and upload/delete uncertainty.
These should not become provider-specific rules in Concord's shared crates.
