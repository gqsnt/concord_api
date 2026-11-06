# Author observations

These entries record actions, diagnostics and outcomes for the frozen Concord
library at `6d6d66aa8ff408c61b718bea70c3fda9d2e1fb5e`. File locations are relative
to this SDK unless stated otherwise. No library, other example, script, docs or
checks were modified. No private API, native request mutation, handwritten
operation identity or replacement HTTP execution was needed.

1. **Response retention works, but the SDK still assembles an owned error boundary.**
   `src/wire.rs:53` uses `prepare()?.receive()` once, copies status/headers/operation,
   branches on status, then uses `bytes_limited` or `decode_limited`. `src/error.rs:222`
   preserves a failed body read beside the HTTP status rather than replacing the
   provider failure. This is repeated adapter work of the kind visible in Slack's
   example: metadata ownership, safe diagnostics, raw bytes and parsed errors need
   explicit SDK types. Concord made one execution, bounded reads, strict JSON and
   automatic operation labels easy; `send DropboxPolicy` centralizes the policy.
   Reproduce with `tests/protocol.rs:477` (429 with oversized/interrupted body) and
   `tests/protocol.rs:519` (success body size/transport failures). Both retain request
   IDs. **Cause: Concord/shared response-ownership mechanics plus provider-specific
   error schema.** A reusable owned response-head/error-body wrapper could reduce
   repetition; Dropbox status interpretation belongs in the SDK.

2. **The JSON-valued root header needs a typed-to-wire conversion before the declaration.**
   `src/context.rs:86` serializes the PathRoot enum with serde_json and builds a
   native HeaderValue; `src/wire.rs:33` declares that native value with the member
   string. The author guide documents ToHeaderValue as a closed set; a domain enum
   cannot be placed directly without conversion (or a Display/template spelling).
   No failed compile probe of a custom header trait is claimed. The resulting
   headers remain declarative, sensitive and part of each immutable scope snapshot.
   Reproduce all three modes in `tests/protocol.rs:16`, `:73`, `:112`; the gated
   concurrent perturbation is `tests/workflows.rs:67`. **Cause: provider JSON header
   representation and Concord's closed header-value conversion contract.** The
   serializer belongs in this SDK; no request escape or mutable shared defaults
   were required.

3. **Context-bound pagination needs SDK glue; completion mechanics do not.**
   `src/models.rs:141` carries the member/root with the opaque cursor.
   `src/lib.rs:95` constructs a different declared endpoint for continuation, and
   `src/lib.rs:114` clones owned state into the Send + 'static page factory and maps
   has_more to the next cursor. The initial request body and continuation body
   differ, so this is domain adaptation rather than a repeated pagination engine.
   The public Pages type supplies typed bounds, collected prefixes, standard
   streams, cancellation-safe polling and explicit termination. Reproduce with
   `tests/workflows.rs:203`, `:248`, `:274`, `:306`; caller examples are compiled from
   README.md. **Cause: provider context/cursor coupling and SDK ownership policy;
   Concord makes traversal/completion easier.** Item traversal drops page metadata,
   so callers needing every request ID must choose pages. No automatic context
   switch or reset retry occurs.

4. **An author annotation error produced a useful macro diagnostic.**
   Initial declaration used `#[sensitive] body options: ListFolderOptions` and the
   same attribute on the continuation body. Compilation rejected the first with
   “`#[sensitive]` applies to placed values, not bodies (Concord never renders bodies)”.
   Removed both attributes; header/token sensitivity remained. Reproduce by adding
   that attribute to the body parameter at `src/wire.rs:37` and running the package
   test command. The corrected declaration compiled. **Cause: SDK author assumption;
   Concord diagnostic was specific and sufficient.** This is not a request for
   a library change.

5. **Numeric guide examples were insufficient to validate namespace IDs.**
   The first local validator admitted only digits. A final check of Dropbox's
   official common.stone found `[-_0-9a-zA-Z:]+` at the pinned July 2026 revision.
   Updated `src/context.rs:34` and added `tests/protocol.rs:93`, which verifies the
   exact JSON header for `ns:Team_A-9`. Existing numeric fixture IDs still work.
   **Cause: SDK validation mistake caught by provider source verification.** The
   broader provider schema, rather than example IDs, is now the contract. See
   PROVENANCE.md source N.

6. **Pagination test spellings required checking the actual public API.**
   The first test compile used `Resume::Key`, `Outcome::Interrupted` and an
   exhaustive Provider pattern. Rust reported E0599 for the two nonexistent
   variants and E0638 for the non-exhaustive pattern. Inspection of the public
   types showed `Resume::At`, `Outcome::Stopped` and the required `..`. Corrected
   `tests/workflows.rs:203`, `:248`, `:306`; tests passed. **Cause: SDK test author
   spelling assumptions, not a Concord defect.** The README describes interruption
   in prose without inventing a public variant name.

7. **Blocking inside a fixture generator made the concurrency test scheduler-dependent.**
   The coordinator reported the old test failing once in eight workflows-target
   runs and once during a full package run: the five-second condition-variable
   wait timed out, its assertion panicked while holding the mutex, and B then
   encountered a poisoned mutex. The old gate ran inside the synchronous
   `Body::Generate` callback on a Tokio worker. Blocking that worker could prevent
   queued work needed to send/handle B from progressing, despite the four-worker
   runtime. Increasing the timeout or worker count would retain this dependency.
   **Cause: SDK test design; the frozen test server has no asynchronous gate/body
   generator callback** (`../testserver/src/lib.rs:150` defines a synchronous
   `FnMut() -> Option<Bytes>`). No SDK or Concord production defect was identified.
   Reproduction: the coordinator's repeated whole-target command below on the
   original `tests/workflows.rs:67` test (previously line 64), with its blocking
   Condvar gate. That original failure was reported by the coordinator, not
   reproduced during this follow-up.

   Replaced the mutex/Condvar with an atomic receipt flag at
   `tests/workflows.rs:72`. Until B's request handler sets the flag, A's generator
   returns empty chunks; the server writes no bytes for those chunks and awaits
   its public inter-chunk delay, yielding execution instead of blocking a worker
   (`tests/workflows.rs:80`). Only receipt of B releases A's actual fixture bytes;
   the one-millisecond interval is a polling/yield interval, not an ordering
   assumption. Changed this test to the default single-thread Tokio runtime to
   verify that no spare worker is needed. Before changing context and sending B,
   `tests/workflows.rs:105` observes A's recorded request and active response body,
   then checks zero body bytes, a closed gate and a pending A task. Both complete
   listings still assert every request's method, target, headers and exact body,
   their separate result entries, cursor contexts and request IDs. No `src/` or
   shared test-server change was needed.

## Execution evidence

- Read the existing Slack response adapter/pagination implementation and GitHub
  overview; Slack supplied the reusable design pattern, as recorded in provenance.
- Initial `cargo test --locked` refused the expected new package lock update.
  One `cargo test --offline -p concord-sdk-dropbox` added only the new package's
  dependency entry; it reached the annotation diagnostic in entry 4.
- After removing that annotation, the crate and two README examples compiled.
  The first full test compile then exposed entry 6; after correction, 17 protocol,
  7 workflow and 2 documentation tests passed. Entry 5 subsequently added one
  protocol regression test, bringing the final verified count to 18 + 7 + 2.
- `cargo doc -p concord --no-deps --features csv,jsonl` completed successfully.
- Every Cargo invocation used `CARGO_BUILD_JOBS=5` and the supplied shared
  `CARGO_TARGET_DIR`. No new registry package/version was added, and no live
  provider verification is claimed. The requested runner was Codex gpt-6-astra
  with xhigh effort; the task's tools expose no independently verifiable resolved
  model/effort setting, so no runtime attestation is made here.

Final gate (2026-11-06), with the environment prefix above on every Cargo command:

- `cargo test --locked -p concord-sdk-dropbox`: **18 protocol + 7 workflow + 2
  documentation tests passed**, no failures or ignored tests. The documentation
  examples compile without provider calls.
- `cargo clippy --locked -p concord-sdk-dropbox --all-targets -- -D warnings`:
  passed, no warnings.
- `cargo fmt -p concord-sdk-dropbox`: completed; no other package was formatted.
- `git diff --check`: passed. Diff against the frozen commit for `crates/`, `docs/`,
  `scripts/` and `checks/` was empty; other examples were not edited. Root changes
  are the single workspace member addition and this package's lock entry.
- Fixture index check: 28 files, all mapped in PROVENANCE.md. No `tests/first_use.rs`
  was created. Changes remain uncommitted for the coordinator.

## Coordinator follow-up verification

All commands retained the required `CARGO_BUILD_JOBS=5` and shared
`CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a`
prefix. No test-thread serialization was added to the Cargo invocation.

- Ran `cargo test --locked -q -p concord-sdk-dropbox --test workflows` **20 times
  consecutively** against the same corrected test: **20/20 passed**, 7 tests each,
  **140 workflow test executions**, zero failures or ignored tests. Per-run output
  was retained in `/tmp/dropbox-workflows-run-1.log` through
  `/tmp/dropbox-workflows-run-20.log` for this session.
- Then ran `cargo test --locked -p concord-sdk-dropbox` once: **18 protocol + 7
  workflow + 2 documentation tests passed**, zero failures or ignored tests.
- Only `tests/workflows.rs` and this record changed during the follow-up;
  all SDK production source, fixture bytes, dependency manifests and lockfile
  remained unchanged from the author's delivered tree.
- `cargo fmt -p concord-sdk-dropbox` completed; strict all-target Clippy and
  `git diff --check` passed after the test correction.
