# Maintenance: change selected user/root during an in-flight request

Starting SDK: `373362dc` (author plus independent consumer commits). Concord remains
frozen at `6d6d66aa8ff408c61b718bea70c3fda9d2e1fb5e`. All work is inside
`examples/dropbox-sdk/`; no live provider calls or credentials were used.

## Result and edits by kind

The existing SDK already passed an owned `RequestContext` into each request and
bound returned cursors to it. No transport correction or caller migration was
necessary. This change adds consuming methods for deriving a different local
selection and verifies isolation while an initial or continuation response is held
open. Previously created requests, traversals and cursors retain their selection.

| Kind | Edits |
|---|---|
| Declaration | None. `wire.rs` already places both sensitive headers on the shared `selected` scope for both operations. |
| Adapter | None. `Client::list_folder`, `list_folder_continue`, `list_folder_pages` and the common response/error policy retain their existing behavior. |
| Model | `src/context.rs`: add `RequestContext::with_member(self, member) -> Result<Self, InputError>` and `with_path_root(self, root) -> Self`. Member changes delegate to `new` for validation; root changes accept the existing validated `PathRoot` type. No cursor or provider response/error model changes. |
| Tests | Add `tests/maintenance.rs`: five tests, including seven gated wire scenarios. Reuse the existing testserver, support helpers and fixture bytes. Existing first-use, protocol and workflow tests are unchanged. |
| Docs | Update the README's concurrent workflow and explain the consuming methods, in-flight isolation and cursor behavior. Add public rustdoc alongside the methods and this maintenance record. |

The wire scenarios change both member and root while A's initial response or
continuation response is pending; change only the member; change only the root
to namespace or home mode; and return a typed HTTP 422/`InvalidRoot` for B while
either kind of A response is pending. Each scenario observes A's recorded request
and active response body before changing the local selection. B must finish
before the test releases A's body. The tests run on Tokio's default single-thread
test runtime, with bounded waits and a nonblocking generator.

Assertions cover exact selected-user/path-root headers, bearer/header cardinality,
method, target and JSON request bytes; response status, request ID, operation,
entry names, completion indicator, raw cursor value and cursor context; A's later
continuation after B completes; exact request counts with no retry/fallback; and
server cleanup. The local-input test rejects invalid replacement member IDs
without echoing them in diagnostics. Empty B pages still continue to the terminal
page. Existing tests continue to cover partial progress, typed failures and caps.

## Locality and independently maintained descriptions

**One runtime implementation location changed:** `RequestContext` in
`src/context.rs`. The existing constructor remains the single member-validation
implementation; the new method delegates to it. The root JSON spelling remains
defined by `PathRoot` and its existing serializer. The mapping to the two HTTP
headers remains in one Concord scope. The two existing adapter call sites that
select this scope did not require edits.

The caller API addition required synchronizing **three independently maintained
descriptions** of its contract: public rustdoc in `context.rs`, the README example
and explanation, and regression assertions in `tests/maintenance.rs`. This record
is a fourth narrative account. There was no provider wire-semantic change to
propagate through multiple declarations or schemas. The implementation felt
local; the larger part of the change is evidence for response/cursor isolation
across request stages, root modes and failure outcomes.

## Reading and discovery

Before implementation, read the SDK README, provenance, `lib.rs`, `context.rs`,
`models.rs`, `wire.rs`, test support, protocol/workflow tests, and consumer test
code. The declaration
identified header placement; `Client` showed initial/continuation selection and
lazy traversal capture; `Cursor` showed context binding; `RequestContext` held
the existing validation and root serialization.

Read the workspace README, the extras/pagination crate READMEs, the caller guide's
navigation/snapshot sections, and author-guide material on value ownership and
header placement. Built Concord rustdoc using `cargo doc -p concord --no-deps
--features csv,jsonl` and inspected `Endpoint`/`SendWith`. Read the testserver's
public body/generator definitions and existing SDK gates to design the held
responses. No existing provider SDK supplied this maintenance solution; the
pattern came from Dropbox's existing code/tests and Concord's public docs.

`FRICTION.md` and `FIRST_USE.md` were first read **after** the first complete
implementation passed the full test suite. Their earlier observations were not
used to design the first attempt. The subsequent review retained the existing
public transport and error boundaries. No private reach-through, native request
mutation, replacement HTTP executor or constructed operation identity was needed.
Operation-name strings in the new tests are expectations for generated metadata.

No fixture bytes or fixture provenance changed. The new scenarios reuse the
hand-authored files documented in `PROVENANCE.md`; they do not represent recorded
provider timing or conformance. `fixtures/** -text` remains unchanged.

## Diagnostics and interventions

- Baseline package suite passed: 18 first-use, 18 protocol, 7 workflow and 2 README
  doc tests.
- First new-test compile failed with E0433, `cannot find poll in futures_util`.
  The existing dependency feature set does not expose `futures_util::poll!`.
  Replaced that assertion with `std::future::poll_fn` and `Future::poll`; no
  dependency or feature change. Reproduction: replace the pending-state assertion
  block in `tests/maintenance.rs` with
  `assert!(futures_util::poll!(&mut pending).is_pending());`, then run the package
  test command below.
- The first passing build warned that shared test helper `context_b` was unused
  in the new test target. The scenario helper now accepts an independently built
  expected context, reusing `context_b` and eliminating the warning.
- Review adjusted gate ownership so the test alone owns the release-on-drop
  guard and the server captures only its atomic flag. This makes the cleanup
  comment accurate on assertion failures. There was no observed gate timeout.
- Final review kept the `InvalidRoot` failure scenario in `root` mode, matching
  the existing fixture's documented basis; successful scenarios cover namespace
  and home modes separately.
- No generated-name surprise or new Concord workaround occurred. Strict
  all-target Clippy passed. Cargo briefly waited for the shared build-directory
  lock, then completed normally.

## Verification

All Cargo commands use this prefix, including formatting and documentation:

```text
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a
```

- `cargo test --locked -p concord-sdk-dropbox`: **48 integration tests and 2
  README doc tests passed** (18 first-use + 5 maintenance + 18 protocol + 7 workflow).
- `cargo clippy --locked -p concord-sdk-dropbox --all-targets -- -D warnings`:
  passed without warnings.
- `cargo fmt -p concord-sdk-dropbox`: completed for this package only.
- `cargo doc -p concord --no-deps --features csv,jsonl`: passed.
- `git diff --check`: passed; the working changes are confined to this SDK.

These are offline loopback and compilation checks, not live Dropbox verification.
