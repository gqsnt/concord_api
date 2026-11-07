# SDK46 Anthropic Messages maintenance

Date: **2026-11-07**. Live provider calls: **NOT_RUN**. Every workflow used loopback
`TestServer::start(|request: &Recorded| -> Reply)` with synthetic credentials and scripted replies.
No commit was attempted. All changes are inside this package; shared crates, checks, scripts,
other examples, root workspace membership and `Cargo.lock` are unchanged.

## Prior exposure and reading boundary

There was no prior session exposure to this SDK beyond the maintenance capsule. This was
source-assisted maintenance. Before attempting the ticket, inspected the clean Git status,
worktree file lists (no `AGENTS.md` found), root/package `Cargo.toml`, this package's lock entry,
SDK `README.md`, `PROVENANCE.md`, `src/{lib,model,error,stream}.rs`, and
`tests/{first_use,protocol,workflows}.rs` plus `tests/support/mod.rs`. Some initial combined reads
were truncated; relevant model/error/test portions were reread separately. Read targeted
`concord-macros/src/emit.rs` sections and `concord-core/src/value.rs`; searched declaration/body
guidance in `docs/{AUTHOR_GUIDE,RECIPES,CHEATSHEET}.md` and macro sources.

**`FRICTION.md` and `FIRST_USE.md` were first read after the new implementation, five regression
tests and the new consumer had compiled and passed.** Their historical observations were not
used to construct the first attempt. No parent-directory campaign material or other worktree
was read. No subagents were used.

Official pages read with the web reader on **2026-11-07**:

- [Token counting](https://platform.claude.com/docs/en/build-with-claude/token-counting):
  endpoint, shared inputs, system/tool shapes, `input_tokens`, estimates and input restrictions.
- [Claude API errors](https://platform.claude.com/docs/en/api/errors): error documents, status,
  request IDs and errors after successful streaming setup.
- [Streaming messages](https://platform.claude.com/docs/en/build-with-claude/streaming): named
  JSON events, final `message_stop` and midstream errors.

Exact sections, fixture derivations and dependency status are in `PROVENANCE.md`'s maintenance
section. The earlier provenance retrieval hashes describe the initial author's reading.

## Result and caller contract

```rust,ignore
let counted = api.messages().count_tokens(request.count_tokens()).send().await?;
let estimate = counted.value.input_tokens;
// `request` is still available for an explicit create or streaming call.
```

`CountTokensRequest` is an owned snapshot of the existing model/messages/system/tools; it
contains no generation settings. `MessageRequest` flattens that same input structure alongside
`max_tokens`; `StreamRequest` still flattens the message request alongside `stream: true`.
This avoids a second projection field list that could silently omit future shared inputs.
Optional system/tools setters accept JSON without interpreting it; absent fields stay absent,
preserving existing callers' request JSON fields and defaults.

The generated client → messages scope → count_tokens endpoint posts only to
`/v1/messages/count_tokens`, returns `Response<TokenCount>` and inherits authentication,
version overrides, error documents, request IDs and the existing one-attempt `SendPolicy`.
There is no generation fallback. Unsupported counting, rejected tools, bad successful bodies
and provider errors remain failures. Counting does not promise identical later generation usage.
The 8 MiB successful JSON bound and 64 KiB error-document bound apply as before.

An initial proposal to borrow inputs was revised after inspecting the generated endpoint
structs: they store a concrete owned body and have no lifetime parameter. No borrowed-body
compile experiment or shared-crate edit was needed. The snapshot clones the inputs; callers do
not retype them. An already built stream request exposes them through `request().count_tokens()`.

## Every edited location

| File | Locations and change |
|---|---|
| `Cargo.toml` (this package) | Package description adds counting; dependencies and version unchanged. |
| `src/model.rs` | `MessageRequest` representation, constructor and existing accessors share `CountTokensRequest`; added `with_system`, `with_tools`, `system`, `tools`, `count_tokens`; added counting input/output types and a compile-fail example. `StreamRequest` serialization and all response models are unchanged. |
| `src/lib.rs` | Crate example, exports, client docs, `messages.count_tokens` declaration with path/JSON bound, and response/send-policy/bound documentation. Send policy, transport, create and stream implementations unchanged. |
| `src/error.rs` | Documentation only: a successful JSON decode failure can concern a count as well as a message. |
| `tests/token_counting.rs` (new) | Five regressions: exact count wire/default headers/zero; snapshot/system forms/empty tools; provider error/ID/retry-delay parity and no fallback; unreadable error documents; malformed/wrong-media/oversized successful counts and no fallback. |
| `tests/count_tokens_consumer.rs` (new) | Counts a multi-turn request with system blocks/tools, then explicitly creates and streams it through `MessageStop`; compares all shared wire inputs and configured key/version across the three sends. |
| `README.md` | Scope, compiled-example counterpart, caller instructions, count semantics, error table, exclusions and validation instructions. |
| `PROVENANCE.md` | Dated maintenance sources, inline fixture derivations and unchanged dependencies/lockfile. |
| `MAINTENANCE.md` (new) | This report. |

All existing test files, including `tests/first_use.rs`, and every existing assertion are
unchanged. Existing fixture bytes and `src/stream.rs` are unchanged.

Facts updated in several places: counting's name/path/result in the declaration, public models,
examples, README and new independent wire expectations; system/tools availability in model
setters, README/exclusions and tests; JSON count failure semantics in error/response docs and
README; source dates in provenance and this report. The README and crate example deliberately
retain matching snippets. Auth/version/error decoding have no new copies. Shared input fields
have one production serialization definition; test expectations repeat provider fields on
purpose so an omitted input would be observable.

## Observable validation

Cargo commands ran sequentially with `CARGO_BUILD_JOBS=4`, offline, in the default worktree
`target/`; no target-directory override was set. Initial package membership and its lock entry
already existed, so no manifest registration or dependency resolution change was necessary.

1. `cargo test --offline -p concord-sdk-anthropic-messages` without `--locked`: baseline
   **33 integration tests + 2 doctests passed**, lockfile unchanged.
2. Implemented counting and both new test files; formatted the package with `cargo fmt -p
   concord-sdk-anthropic-messages`.
3. `cargo test --offline --locked -p concord-sdk-anthropic-messages`: **39 integration tests +
   3 doctests passed** on the first changed build/run. This includes the untouched first-use
   consumer, incremental streaming, midstream error and EOF-before-`message_stop` failures.
4. Read the deferred handoff notes; completed public documentation and package-description edits.
5. `cargo clippy --offline --locked -p concord-sdk-anthropic-messages --all-targets -- -D
   warnings`: passed. `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p
   concord-sdk-anthropic-messages --no-deps`: passed.
6. Targeted `rustfmt --edition 2024 --check` and `git diff --check`: passed. Git status lists
   only the nine package files above; explicit diff checks confirm the root manifest, lockfile,
   all existing integration test files and stream implementation are unchanged.

No omitted edit was caught as a compiler diagnostic or failing test: the first changed test run
passed. The compile-fail doctest confirms a counting body is rejected by `create`, rather than
recording an accidental implementation failure. Runtime tests check both optional inputs survive
all three sends and that counting has neither generation field. Prose/exclusion changes still
required manual review; the compiler cannot check those statements.

## Remaining duplication and responsibilities

Generic mechanics remaining in this SDK include `Response<T>`, the bounded error-document send
policy, request-ID accessors, transport builder, and repeated successful JSON bound in the create
and counting declarations. The new endpoint needed no copied error logic. Test clients, example
snippets and wire expectations have intentional repetition. No new crate was needed.

Provider responsibilities remain the input/output schemas and validation, which inputs counting
accepts, estimate semantics, API version and key header, error taxonomy and ID precedence, plus
the SSE state machine: JSON event names/types, block ordering, cumulative usage and completion
only at `message_stop`. Incomplete results remain partial and cannot trigger automatic restart.
Tools are definitions only; execution, batches and extended-thinking interpretation stay excluded.

Candidate Concord improvements, recorded only:

- Support lifetime-parameterized generated endpoint bodies, allowing a borrowed shared-input
  request without cloning; the inspected emission stores its declared body type directly in a
  non-generic endpoint struct.
- Keep the existing explicit input conversion rules documented prominently: `IntoValue` is
  sealed and does not accept an SDK's ordinary `From<&MessageRequest>` conversion. This capsule
  therefore uses the explicit `request.count_tokens()` adapter.
- Offer reusable bounded provider-error/response metadata scaffolding, while leaving Anthropic
  document schemas and retry decisions in this SDK.
- As corroborated by the deferred notes, improve generated streaming-reader documentation and
  typed recovery of Concord transport failures from SSE `ReadError`; neither required a change
  for counting. Keep provider completion sentinels in provider code.

No shared-crate changes are proposed for automatic counting fallback, JSON body mutation or
stream resumption. The distinct endpoint body types already express the required boundary.
