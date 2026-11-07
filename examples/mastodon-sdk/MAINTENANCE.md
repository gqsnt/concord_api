# SDK38 Mastodon maintenance — 2026-11-07

Added validated hashtag timelines and the federated/local hashtag SSE subscriptions. Existing
public timeline, status retrieval and public SSE calls retain their methods, setters, wire order,
response types and failure semantics. **Live provider calls: NOT_RUN.** All workflow traffic used
`examples/testserver` loopback fixtures; no commit was attempted.

## Prior exposure and reads

This maintenance session had no earlier work on this SDK. Before the first implementation
attempt, inspected root `Cargo.toml` and the Mastodon package manifest; `src/lib.rs`, `api.rs`,
`model.rs`, `streaming.rs`, `error.rs`; `README.md`, `PROVENANCE.md`; and the existing
`tests/first_use.rs`, `protocol.rs`, `workflows.rs`, `common/mod.rs`. Some initial bulk reads were
output-truncated; subsequent targeted reads covered the declarations, values and test helpers.
Neither `FRICTION.md` nor `FIRST_USE.md` was opened until the implementation built and the
first complete test run passed (54 tests, including the unchanged first consumer).

Read the two permitted official pages before implementation, both on **2026-11-07**:

- [timelines API methods](https://docs.joinmastodon.org/methods/timelines/), especially
  "View hashtag timeline": path, additional-tag arrays, flags, bounds, limits and access.
- [streaming API methods](https://docs.joinmastodon.org/methods/streaming/), discovery,
  categories, HTTP SSE framing, hashtag and local hashtag routes, authorization, and the
  WebSocket selector section.

Inside this worktree, searched `docs/`, `crates/concord-macros/src/` and
`crates/concord/src/` for query/list declarations, then read `docs/AUTHOR_GUIDE.md` §2.1
(parameter groups) and §4.1 (placements/`each`). Read `examples/testserver/src/lib.rs`'s public
fixture API and chunk/failure-writing branches. Repository file inventories and `git status` /
diffs were administrative reads. No `AGENTS.md` was found. No other example implementation,
campaign files, parent worktree or additional external page was consulted. After the initial
successful attempt, read the two historical logs to compare their reported frictions.

## Caller contract and implementation

- `Hashtag::new` is the common entry validation for the primary name, extra tags and stream
  tag. It rejects empty names, `#`, whitespace, controls and `.` / `..`. The field is private;
  all request constructors/setters take this validated type. Case and Unicode remain intact.
  These local constraints are SDK decisions; the pages do not give a complete tag grammar.
  Punctuation such as `/`, `?`, `&` and literal `%2F` is safely encoded, with final acceptance
  left to the instance.
- `hashtag_timeline(tag)` declares `GET /api/v1/timelines/tag/{hashtag}`. Concord's path hole
  encodes the value as one segment. `set_any`, `set_all`, `set_none` use `Vec<Hashtag>` and
  `query("any[]") each` etc.; the common timeline group supplies all flags, id bounds and limit.
  Unset/empty lists emit no pairs. No list truncation or automatic pagination was added.
- `hashtag_stream(&host, tag)` and `local_hashtag_stream(&host, tag)` use the two documented
  HTTP SSE paths, `/api/v1/streaming/hashtag` and `/api/v1/streaming/hashtag/local`, with `tag`.
  They represent `hashtag` and `hashtag:local`. The page's `stream=hashtag&tag=…` selector is
  documented under WebSocket; no query-selector SSE endpoint or WebSocket support was invented.
- All streams call one `streaming_api` helper, enforcing the existing instance/advertised-host
  relationship before rebasing the private generated client with its token. Discovery and
  HTTPS downgrade validation remain in their existing implementation.
- New streams use the existing `PublicStreamFormat` and `PublicStream` without runtime changes:
  same framing, payload mapping, content-type check, byte/event caps, cancellation behavior,
  terminal error followed by `None`, and no reconnect, replay or exactly-once guarantee.

## Every edited location

Paths are relative to `examples/mastodon-sdk/`; symbol names identify the edited locations.

| File / location | Edit |
| --- | --- |
| `Cargo.toml:3`, dependency comment | Describe public/hashtag timelines and streams; no dependency/version changes. |
| `src/model.rs:202`, `Hashtag`, `InvalidHashtag` | Validated name, constructor, accessor, `TryFrom<String>`, display and input-free error. |
| `src/api.rs:32`, `TimelineOptions`, `public_timeline`, `hashtag_timeline`; `:91`, both hashtag stream declarations | Share the seven timeline parameters; declare bracketed extra tags, encoded path and both SSE routes using the existing format. |
| `src/lib.rs:1`, operation table, exports at `:97`, `Mastodon::hashtag_timeline` at `:162`, stream methods/helper at `:198` | Public request/value exports, three new delegation methods, shared host binding, expanded crate docs. |
| `src/streaming.rs:1`, `StreamEvent`, `PublicStreamFormat`, `PublicStream` docs | Explain that the existing reader also serves hashtags. Documentation edits only. |
| `tests/hashtags.rs` (new), six regression tests | Local validation, path/query safety, every timeline option, omitted defaults, both stream routes, token/host binding, access refusals, UTF-8/field splits and terminal disconnect. |
| `tests/hashtag_use.rs` (new), consumer workflow | Public API usage: hashtag page → individual status → discovered streaming host → both hashtag streams, including edit/delete and terminal EOF. |
| `README.md`, coverage/exclusions, operation table, "Hashtags", host/failure contract, commands/test coverage | Document the caller API, a compiled example and provider route distinction. |
| `PROVENANCE.md:10`, maintenance research section | Exact sources/date, SDK decisions, synthetic fixtures and dependency/lock evidence; original research retained separately. |
| `MAINTENANCE.md` (new) | This observable maintenance record. |

Root workspace membership and the package lock entry already existed. Root `Cargo.toml` and
`Cargo.lock` are unchanged, as are all shared crates/checks/scripts and other examples.
`tests/first_use.rs`, all its assertions, and every pre-existing test/helper/fixture are unchanged.
`FRICTION.md` and `FIRST_USE.md` retain their historical contents.

## Repeated facts and omission evidence

Endpoint support had to be updated in the declaration, public wrapper/request aliases,
operation tables/examples, package description, provenance and tests. Hashtag validation rules
appear in the constructor/error text, caller docs and test cases. Route spellings and bracketed
query keys appear in declarations and independent wire expectations; docs name them too.
These repetitions remain maintenance locations, even though request generation itself is shared.

The seven existing timeline parameters were moved intact into `TimelineOptions` and included by
both timelines, so their types, defaults, setters and wire order have one declaration. Host
binding now has one helper. SSE framing, event mapping and disconnect policy have one existing
reader; no parallel hashtag parser was introduced.

Normal implementation/build and the first full test run succeeded without an omitted-edit
failure. Two temporary, intentional omission probes then checked diagnostic coverage:

1. Removing `each` from `query("any[]") each any?: Vec<Hashtag>` made the locked build fail
   with **E0277**, pointing at `Vec<Hashtag>` and saying that a list is placed with `each`.
   The compiler catches an omitted list-placement edit with an actionable diagnostic.
2. Dropping `[]` from the `all[]` wire key compiled, but
   `timeline_encodes_one_segment_and_repeats_bracketed_tag_filters` failed: decoded `all[]`
   values were `[]`, expected `["日本語", "sdk"]`. Query-key spelling is a provider fact the
   compiler cannot check; the independent fixture assertion catches it.

Both mutations were restored immediately. No assertion was removed or weakened. All final
verification below ran after restoration.

## Remaining duplication and responsibilities

Generic duplication remains in the public wrapper/delegation and request re-export aliases,
validated-string boilerplate, redacted provider error policy, documentation operation tables,
and the three stream declarations' format choice. The new reader still bears the legacy public
name `PublicStream`; keeping it avoids disturbing existing callers. A future neutral alias could
improve naming without changing its behavior.

Provider-specific responsibilities stay here: tag names and bracketed query semantics; which
statuses/streams are selected; accepted tag counts and feed/access configuration being instance
decisions; streaming-host advertisement/trust and scheme normalization; user-token scopes;
event payload mapping; and the policy that an endless feed's EOF is an error. No service-level
ordering, federation, replay or exactly-once semantics are established by these fixtures.

Candidate Concord changes (recorded only; frozen shared files were not edited):

- A validated runtime-base constructor and credentials bound to that base could reduce the
  private-client wrapper and placeholder-host documentation. The discovered-host trust decision
  would still belong to Mastodon.
- Generated rustdoc could identify public request aliases and the concrete incremental reader
  behind `Records<F, I>`; clearer stream naming would help this expanded caller surface. These
  also match the historical logs' reported documentation friction.
- Structured truncation/end-of-input diagnostics could remove the existing stream adapter's
  EOF-based distinction; the historical author log describes that limitation. This ticket
  reused the adapter and verified its externally observable behavior, without changing shared
  reader internals.
- Parameter groups and repeated-query `each` already solved the actual duplication introduced
  by this ticket. No new macro construct was needed for hashtag paths or extra tags.

## Verification

Every cargo command ran sequentially with `CARGO_BUILD_JOBS=4`, using the worktree's default
`target/` (no `CARGO_TARGET_DIR` override). The initial build allowed lock resolution offline;
subsequent resolving commands were offline and locked. No new crate or package version appeared.

| Command (with `CARGO_BUILD_JOBS=4`) | Final result |
| --- | --- |
| `cargo build --offline -p concord-sdk-mastodon` (initial) | Pass; lock unchanged. |
| `cargo fmt -p concord-sdk-mastodon -- --check` | Pass. |
| `cargo test --offline --locked -p concord-sdk-mastodon` | **55 passed, 0 failed**: 2 original first-consumer, 37 protocol, 4 existing workflow, 6 hashtag regression, 1 hashtag consumer, 5 doctests; no unit tests. |
| `cargo clippy --offline --locked -p concord-sdk-mastodon --all-targets -- -D warnings` | Pass. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-mastodon --no-deps` | Pass. |
| `git diff --check` | Pass. |

An explicit `git diff --exit-code` confirmed no changes to root manifests/lock, shared code,
or the original consumers/tests/helpers. Remaining limitations are the documented instance
dependencies and the absence of live-service verification, not failing fixture checks.
