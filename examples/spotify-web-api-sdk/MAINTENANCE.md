# SDK36 Spotify Web API maintenance

Date: **2026-11-07**. Live provider calls: **NOT_RUN**. Every exercised workflow uses
`TestServer::start(|request: &Recorded| -> Reply)` on loopback. No commits were attempted.

## Prior exposure and order of work

No prior authorship of this SDK or exposure to its implementation is claimed. Before starting,
the maintainer had the maintenance capsule, ticket and workspace/tool instructions. Neither
`FRICTION.md` nor `FIRST_USE.md` was read before the first implementation attempt.

Before implementing, inspected the root/package manifests, package sources
(`lib.rs`, `session.rs`, `error.rs`, `models.rs`, `auth.rs`, `pages.rs`), existing test sources
(`first_use.rs`, `protocol.rs`, `workflows.rs`, `common/mod.rs`), relevant fixture bodies,
`README.md` and `PROVENANCE.md`. No applicable `AGENTS.md` was found in the root or SDK path.
Read the fixture server API in `examples/testserver/src/lib.rs`, relevant declaration/body
guidance in `docs/{CHEATSHEET,AUTHOR_GUIDE}.md`, and relevant shared source in
`crates/concord-core/src/{error,prepared}.rs`, `crates/concord-retry/src/lib.rs` and macro
declaration/encoding search results. These files were read only.

Read these exact official pages on **2026-11-07**:

| Page | Contract used |
|---|---|
| [Add Items to Playlist](https://developer.spotify.com/documentation/web-api/reference/add-tracks-to-playlist) | POST route, JSON URI array, 100-item maximum, insertion position, write permissions and snapshot response; route is marked deprecated |
| [API calls](https://developer.spotify.com/documentation/web-api/concepts/api-calls) | Authorization, JSON, HTTP status and error documents, offset pagination |
| [Refreshing tokens](https://developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens) | Basic-authenticated refresh form; keep the existing refresh token when its replacement is absent |
| [Quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes) | Development access differs; an unallowlisted user can get 403; quota-exceeded 429 differs from rate limiting |

Ran the baseline offline suite, implemented the endpoint and tests, corrected one new test
fixture, and obtained **7 passing regression tests plus the new passing consumer test** before
opening `FRICTION.md` and `FIRST_USE.md`. Those notes were used afterwards to compare findings.
Later inspected installed `reqwest` 0.13.4 registry request/client/retry sources to check which
transport policies the SDK can control. No parent campaign files or other worktrees were read;
no sub-agents were used. A file-name inventory is not an implementation-source consultation.

## Resulting behavior

The public call remains in Concord's fixed hierarchy:
`api.playlist(id).add_items(AddItems::new(uris)?.position(0)).send().await?` returns
`PlaylistSnapshot { snapshot_id }`. Omit `position()` to append.

`AddItems` has private fields and only a validating constructor. It refuses empty batches,
more than 100 items, malformed URIs, URLs, local-file URIs and unsupported item kinds before
any request or token refresh. It preserves order and duplicate entries, supports track and
episode catalog URIs, and does not split batches. Its strict 22-character ASCII base-62 ID
check is an SDK validation choice consistent with the add-reference examples; that page does
not itself specify a universal ID-length rule. Existing `SpotifyId` validation is unchanged.

The write acquires a token through the existing shared session/cache, preserving refresh
rotation and keep-on-absent rules. It sends once and returns explicit API refusals normally.
A 401 invalidates only the token generation used; it does not refresh or replay the addition.
The next separate caller-initiated call can refresh. Opt-in 429 retries apply only to reads.
403 remains a development-mode hint, never proof of a particular quota/permission problem.

A dispatched write that loses its result returns `Error::UncertainWrite(Box<Error>)`, including
HTTP timeouts, the session deadline, a dropped connection and unreadable success documents.
`error.concord()` and the boxed cause preserve diagnostics. The caller re-reads the playlist
and its item pages as necessary, then reconciles before choosing another addition. A deadline
during token acquisition before dispatch remains `Error::Deadline`. The write's deadline
includes success-body decoding, so a stalled body after a 201 head cannot hang past the budget.
Caller cancellation after dispatch also requires reconciliation, although dropping a future
cannot deliver an SDK error to the caller.

The existing GET loop, read declarations/models, pagination adapter, token state and all
existing consumer assertions are preserved. The transport remains caller supplied: use
`retry(reqwest::retry::never())` and `redirect(reqwest::redirect::Policy::none())` for a complete
single-send contract. The session cannot override an opaque client's internal replay policy.
This requirement is explicit in the README and exercised by the new consumers.

## Every edited location

All paths below are relative to `examples/spotify-web-api-sdk/`.

| File/location | Edit |
|---|---|
| `src/add_items.rs` — `AddItems`, `new`, `position`, `PlaylistSnapshot` | New validated JSON body and snapshot model |
| `src/lib.rs` — crate documentation, module/exports, `WebApi` and `playlist` scope | Export the new types and declare `POST add_items(body items: AddItems) -> PlaylistSnapshot` at `tracks`; describe write behavior |
| `src/session.rs` — module/options documentation and `SendWith::send` | Single-attempt branch for non-GET requests, generation-specific 401 invalidation, dispatch tracking and full write deadline; GET retry loop retained |
| `src/error.rs` — `ApiClass::Unauthorized`, `Error`, `concord`, `after_write`, Debug/Display/source | Expose uncertain writes and their safe recovery instruction, retain cause and existing error accessors |
| `fixtures/playlist-snapshot.json` | New synthetic opaque snapshot document |
| `tests/add_items.rs` | Seven focused regression tests, including wire accounting and both HTTP/session timeout stages |
| `tests/add_items_consumer.rs` | Independent public consumer: authorization permissions, list/select owned playlist, read/add/read and snapshot comparison |
| `README.md` — coverage table, adding-items section, failure table, exclusions and tests | Public calling example, scopes, validation, no-replay/recovery and transport requirements |
| `PROVENANCE.md` — maintenance section | Exact research pages/date, new fixture origins, validation-policy boundary and dependency/transport record |
| `MAINTENANCE.md` | This observable maintenance record |

The package was already in root workspace `members` and `Cargo.lock` on entry. Neither file
needed an edit. No dependencies, features, licenses or locked package versions changed. No
shared crate, check, script or other example was modified. `tests/first_use.rs` is unchanged;
every existing assertion is retained.

## Facts repeated across locations and diagnostics

| Fact | Places needing agreement |
|---|---|
| POST `tracks` route and snapshot output | Declaration, new response model, fixture, wire tests, consumer and public documentation |
| 100-item bound and supported URI syntax | Validating body, endpoint/type documentation, README/provenance and boundary/invalid-input tests |
| Optional zero-based position with append default | Body serialization, builder, README and wire/consumer expectations |
| Writes never replay; reads retain 401/429 policy | Session branch, error documentation, crate/endpoint docs, README and regression tests |
| Timeout after dispatch means re-read, including a stalled success body | Dispatch tracking/deadline, error mapping/display/accessor, README and timeout/reconciliation tests |
| Missing refresh replacement preserves the current token | Existing acquisition state, documentation and old/new token-form assertions; no production token change was needed |

The compiler accepted the first implementation; no omitted production edit produced a
compiler diagnostic. The first focused test run had **6 passes and 1 failure**: the new 401
test's code-exchange responder returned the refresh-only fixture, so the session correctly
had no stored refresh token. The token assertion caught this omitted fixture distinction.
Corrected the responder to distinguish `authorization_code` from `refresh_token`; retained
the assertion and all existing tests. No production behavior was weakened to make it pass.
No claim is made that tests proved every possible omission or every real provider behavior.

## Remaining duplication and provider responsibilities

Generic mechanics still live in this SDK: token/cache seed handling, refresh-state locking,
shared error ownership, request cloning for reads, token-header application, generation-specific
401 invalidation, per-cause retry counters, bounded error-body reading, response classification,
and deadline/dispatch accounting. The write branch repeats token application, execution and
401 invalidation from the read attempt. It deliberately has no retry owner and applies its
deadline through decoding. The new snapshot model also repeats the `snapshot_id: String`
field already present on full/simplified playlists; sharing an entire playlist model would
misrepresent the small add response.

Spotify responsibilities remain in the SDK/application: request route and body, URI kinds and
batch limit, insertion semantics, opaque snapshot versions, OAuth scopes and ownership checks,
refresh retention/rotation and invalid-grant handling, rate-versus-quota classification,
development-mode limitations and reconciliation of uncertain additions. Syntax validation
does not establish item existence, write permission or extended access. The `/tracks`
deprecation is recorded; migrating existing reads or the ticket's route is separate work.

## Candidate Concord improvements (recorded only)

- Add a documented `SendWith` recipe for write-once operations: token acquisition before the
  dispatch boundary, generation-specific invalidation without replay, `Delivery` classification,
  a deadline through result decoding, and a clear caller-owned transport configuration.
  Existing `Delivery::{NotSent, Unknown, Answered}` already supplies useful evidence; provider
  recovery decisions should stay outside core.
- Document that `concord-retry` ends at the final response head, while an operation needing a
  decoded write receipt must also bound decoding. An optional one-attempt helper could reduce
  the duplicated token/execute/invalidate sequence without changing GET retry semantics.
- Improve generated `prepare()`/`send()` documentation to make the unsigned prepared request,
  different error types and send-policy bypass explicit. Include a bare-origin fixture
  `with_base_url` example when declarations append `v1`.
- Consider token-cache seeding with the cache's own clock to remove the SDK's separate initial
  grant expiry bookkeeping, as also observed in the original friction report.
- Consider generated read access to explicitly set pagination parameters, or document that
  `pages_from(..., Resume::At(offset))` is required to validate the initial offset. The existing
  first-use test deliberately asserts the current limitation; it was preserved.

## Validation

Every Cargo command used `CARGO_BUILD_JOBS=4`, ran alone, and built into the default worktree
`target/`. `CARGO_TARGET_DIR` was unset. No network dependency resolution was needed.

| Check | Result |
|---|---|
| Baseline: `cargo test --offline -p concord-sdk-spotify-web-api` (first build, no `--locked`) | 28 integration tests and 2 doctests passed |
| Focused: `cargo test --offline --locked -p concord-sdk-spotify-web-api --test add_items --test add_items_consumer` after fixture correction | 7 regression tests and 1 consumer test passed |
| Full: `cargo test --offline --locked -p concord-sdk-spotify-web-api` after formatting | **36 integration tests and 2 doctests passed**, no failures |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-spotify-web-api --no-deps` | Passed, including new generated endpoint/type links |
| `rustfmt --check --edition 2024` for the six new/edited Rust files | Passed |
| `git diff --check`; scope/manifests/lock/first-use diff inspection | Passed; only owned package changes |

Timeout regressions cover both session and HTTP-client timeouts, before the response head and
during a stalled 201 body. They verify one addition, no timeout-triggered refresh, and a later
GET observing the applied item and new snapshot. Separate cases verify no 401 replay, no 403,
429 or 5xx retries, retention of a missing refresh replacement through two writes/refreshes,
and rejection of invalid input before even token traffic. These are fixture-backed contracts;
live provider calls remain **NOT_RUN**.
