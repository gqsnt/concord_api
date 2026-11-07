# SDK29 maintenance: exact-email contact search

Date: **2026-11-07**. Baseline and unchanged HEAD:
`8a8befa6ebed33904bde7d28bbfbcc9acbcc4919`.
Live provider calls: **NOT_RUN**. All requests exercised by tests went to local
`TestServer::start(|request: &Recorded| -> Reply)` fixtures. No commit was attempted.

## Change and caller contract

Added `contacts().search(EmailSearch::new(email))`, a generated endpoint under
the existing client → contacts scope → endpoint hierarchy. It supports both
one-page `send()` and the same `pages(Resume<String>, NonZeroUsize)` traversal
as listing. `EmailSearch` fixes an equality filter and owns its nested body
pagination; the caller can select a validated page size or a single-call cursor.

Search uses the existing response/error policy and `ContactList` model. The
shared SDK-local `contact_pages` factory now handles list and search response
projection, cursor validation, metadata and mandatory caps. Listing's request
factory still puts its cursor in the URL query. Search's factory replaces only
the JSON-body cursor, preserving the email and page size. Neither operation
automatically retries. Retrieval and creation declarations/policies are unchanged.

A failed search retains the collected prefix and failed cursor. The headerless
429 regression observes all rate values as `Missing`, Retry-After as `None`,
and exactly two requests, under a 500 ms timeout. No clock-derived reset or
retry scheduling was added. Resume context now explicitly includes the email,
in addition to the existing credentials, base, version and page size.

## Provider sources

These exact official pages were read on **2026-11-07** (documentation only):

- [Search contacts](https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/searchcontacts):
  POST route, email equality filter, search default 50 and maximum 150, and the
  contact-list result. Email is passed through without normalization.
- [Pagination, search section](https://developers.intercom.com/docs/build-an-integration/learn-more/rest-apis/pagination#pagination-for-search-apis):
  both pagination fields belong inside the JSON `pagination` object; the next
  cursor comes from the response. A search traversal is not a snapshot.
- [List contacts](https://developers.intercom.com/docs/references/rest-api/api.intercom.io/contacts/listcontacts):
  listing retains URL pagination and its default size of 10.
- [Rate limiting](https://developers.intercom.com/docs/references/rest-api/errors/rate-limiting):
  a 429 may omit rate headers. Absence supplies no reset timestamp.

The search reference's default of 50 takes precedence over the pagination
overview's illustrative conversation defaults. Search indexing delay remains
provider behavior; create-and-retrieve continues to use the returned ID.

## Every edited location

Paths in this table are relative to `examples/intercom-sdk/`.

| File/location | Edit and responsibility |
| --- | --- |
| `src/lib.rs`: crate synopsis, model reexports, `scope contacts` | Export `EmailSearch`; declare `POST search(body search: EmailSearch) -> ContactList` at `search`. Inherits Bearer/version headers, JSON, bounds and `ResponsePolicy` |
| `src/models.rs`: `PageSize`, `EmailSearch`, private `EmailQuery`/`SearchPagination`, `CursorNext`, `redacted_debug!` | Add numeric transparent serialization to the existing validated size; construct immutable email equality semantics, search default and body pagination; redact the new input's Debug; document both cursor placements |
| `src/pages.rs`: imports, `IntercomContactsList::pages`, new `IntercomContactsSearch::pages`, private `contact_pages` | Extract the existing response projection/validation once; add body cursor replacement; retain listing behavior and the existing public `ContactPages` type |
| `tests/search.rs` (new) | Four regression tests: independent wire oracle, headerless 429, empty intermediate/terminal pages, and invalid/missing cursor metadata. Reuses existing synthetic response fixture files |
| `tests/search_consumer.rs` (new) | Public API consumer: same-email matches, one-page cap, saved resume key, template-cursor replacement for Start and At, metadata, terminal completion despite total_count=99, and get by returned ID |
| `README.md`: summary, list default wording, new exact-email section, failure/exclusion wording, verification link | Public API and compiled example for incomplete results; body placement, defaults, resume context and indexing caveat |
| `PROVENANCE.md`: appended maintenance section | Exact pages/read date, synthetic fixture derivation, maintenance baseline and unchanged dependencies |
| `MAINTENANCE.md` (new) | This change, exposure, diagnostics, verification and ownership record |

`tests/first_use.rs`, every pre-existing test/assertion, existing fixtures,
`src/error.rs`, the package manifest, and historical `FRICTION.md`, `FIRST_USE.md`
and `VALIDATION.md` remain byte-for-byte unchanged. The root workspace member
and SDK lock entry already existed; neither `Cargo.toml` nor `Cargo.lock` changed.
No shared crate, check, script or other example was edited.

## Facts maintained in several places

- Search is a new public operation: declaration, public input reexport, named
  generated-operation extension, README, provenance and caller tests all had
  corresponding edits. `IntercomContactsSearch` is named by hand in `pages.rs`.
- Exact email and search default 50 are implemented once in `EmailSearch::new`,
  then independently asserted in the fixture oracle and described in endpoint
  rustdoc/README/provenance. The 1..=150 range reuses `PageSize::new`; it was not
  copied into a second validator, and `PageSize::default()` still means 10.
- Body pagination has two related responsibilities: serde declares its shape,
  and the traversal updates the body cursor on every request. The compiler
  checks types but cannot prove that the cursor was applied to the next request.
- Cursor response extraction, empty/nonadvancing validation and metadata
  projection now live once in `contact_pages`. Both endpoint adapters and their
  wire assertions remain separate because their request placement differs.
- Resume context requirements appear in API docs and README; the plain cursor
  type does not enforce them. The consumer proves same-query resumption only.

## Observable attempts and omitted edits

1. Wrote the regression and consumer tests before adding the API. The initial
   unlocked offline `cargo test --no-run` failed with E0432 (no exported
   `EmailSearch`) and E0599 (no `IntercomContacts::search`). This was the expected
   test-first absence, not a defect in the baseline's promised scope.
2. Added the implementation. The first full locked offline run passed all
   26 integration tests and the then-existing doctest. It warned that `FIRST`
   and `CREATED` were unused because the new test imported all of `support`.
   The search test now includes just the fixtures it needs and uses a small
   local client constructor; no warning suppression or old-test edit was used.
3. Only after that successful implementation/test attempt, read `FRICTION.md`
   and `FIRST_USE.md`. The latter's incomplete-result discovery cost informed
   the new README example. No historical note was rewritten.
4. Deliberately removed only the search cursor-injection statement in a
   temporary local mutation. It compiled, but
   `headerless_search_429_keeps_prefix_and_failed_cursor_without_retry_or_reset`
   failed with `expected a provider failure` (exit 101): the repeated first
   page reached the nonadvancing-cursor guard instead of the fixture's 429.
   Restored the original source in a `finally` block. This establishes that
   tests, rather than the compiler, catch this omitted body update.
5. Final tests, Clippy, rustdoc, formatting and scope checks passed. No actual
   omitted implementation edit was found beyond the deliberate mutation; no
   independent reviewer or sub-agent was used.

## Verification receipt

All Cargo commands ran serially with `CARGO_BUILD_JOBS=4`, no target override,
and the worktree's default `target/`. Every Cargo command after the initial
unlocked attempt used `--locked`; all used `--offline`.

| Command/check | Observed result |
| --- | --- |
| `cargo test --offline -p concord-sdk-intercom --no-run` | Expected initial E0432/E0599 above; lock unchanged |
| `cargo test --locked --offline -p concord-sdk-intercom --test search headerless_search_429_keeps_prefix_and_failed_cursor_without_retry_or_reset -- --exact` with deliberately omitted cursor injection | Compiled, then failed as intended; source restored |
| `cargo test --locked --offline -p concord-sdk-intercom` on final implementation | Pass: 2 first-use + 12 protocol + 7 workflows + 4 search regressions + 1 search consumer = **26 integration tests**, plus **2 compile-only doctests**, no warnings |
| `cargo clippy --locked --offline -p concord-sdk-intercom --all-targets -- -D warnings` | Pass |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --offline -p concord-sdk-intercom --no-deps` | Pass |
| `rustfmt --check --edition 2024 --config skip_children=true` on the three changed source files and two new test files | Pass |
| `git diff --check` and baseline byte/path audit | Pass; only the eight files listed above changed; all existing tests and lock entries unchanged |

The existing tests still execute both required workflows (list→get and
create→get), headerless listing 429 and ambiguous-create no-replay behavior.
The new consumer adds search→resume→get. Tests establish loopback behavior,
not live provider compatibility. No credential, live workspace data or remote
provider endpoint was used.

Resource observations before/after checks: `df -h .` reported about 201 GiB
available on `/dev/nvme0n1p2` both times; `du -sh target` grew from 1.3 to 1.5 GiB.
`free -h` reported 22 then 21 GiB available RAM, and `nproc` reported 20 CPUs.
These are point observations, not measured process peaks. Existing artifacts
were reused; no full workspace gate, cleanup or alternate build cache was needed.

## Remaining responsibilities and Concord candidates

Provider-specific responsibilities remain in this SDK: method/route, the
email/equals JSON schema, body-vs-query pagination, endpoint defaults, header
names and API version, response discriminators/envelope, cursor validity,
error documents, unknown reset semantics and create ambiguity. The application
owns durable prefix/resume storage and the decision to try again. Search
filters beyond exact email, conversations and webhooks remain excluded.

Generic mechanics still present: clone-and-send factories around generated
requests; response-policy status branching and value/metadata wrapping; strict
singleton numeric-header parsing; repeated opaque Debug implementations;
plain-string resume context documentation; and fixture client/cap setup in
multiple integration-test crates. The local shared page factory removes the
second copy of the projection, not all adapter work. Longer cursor cycles are
bounded by the mandatory cap rather than detected; contexts are not bound to
their cursor type.

Candidates to evaluate outside this frozen capsule:

- **Macros/generated usage:** a documented request-body transformation API
  could avoid `op.search = op.search.with_starting_after(cursor)` depending on
  a private generated field. The existing `set_search` replaces the whole
  body; preserving and editing a nested input required inspecting the emitter.
  Compare this with a handwritten SDK adapter before adding syntax.
- **External pagination:** context-bearing continuations and an optional
  cycle guard could remove caller-only context checks. The provider still
  selects and places the cursor; no Intercom schema belongs in Concord.
- **External response/rate helpers:** evaluate bounded success/error projection
  and singleton unsigned-header parsing while retaining Missing/Invalid/Value.
  Unknown reset must never become a zero-duration retry or assumed window.
- **Core/codecs documentation/diagnostics:** the prior consumer's missing
  `type` discriminator was hard to discover. Improve schema guidance or safe
  structural decode diagnostics without exposing response values. This is a
  historical finding, not a newly reproduced failure in this maintenance run.

No shared implementation change was necessary to add this endpoint. These are
candidate improvements with observed SDK costs, not claims that unimplemented
alternatives have been validated.

## Prior exposure and files consulted

This is source-assisted maintenance of an existing SDK, not an unaided fresh
consumer exercise. Before the first edit, read the ticket; parent handoff
`AGENTS.md`, `START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`, `AUTHORITY.md`,
`contracts/PHASES.md` and `contracts/RESOURCES.md`; root `Cargo.toml`; SDK
`Cargo.toml`, `README.md`, `PROVENANCE.md`, `VALIDATION.md`, all four `src/*.rs`,
`tests/first_use.rs`, `tests/workflows.rs`, `tests/support/mod.rs`, and relevant
wire/redaction portions and test names in `tests/protocol.rs`.

Read Intercom's four pages listed above before writing tests/implementation.
Read `crates/concord-pagination/src/lib.rs` rustdoc and the opening portion of
`src/pages.rs`; searched `crates/concord-macros/src/` and read emitter sections
for body fields, setters and generated send/prepare methods. These were
read-only inspections. After the first successful implementation run, read
`FRICTION.md` and `FIRST_USE.md`, and searched the testserver source for harness
signatures. Existing response fixtures were consumed by the local tests;
new expected request JSON and consumer responses were handwritten from the
provider contracts. No other provider SDK implementation was consulted.

Initial path probes for root-level `PHASES.md`/`RESOURCES.md` and a macro
`src/codegen/` directory found no files; subsequent reads used `contracts/`
and `src/emit.rs`. No prior authorship, external coaching, model-settings
attestation or independent review is claimed. This record contains actions,
diagnostics and decisions, not private reasoning transcripts.
