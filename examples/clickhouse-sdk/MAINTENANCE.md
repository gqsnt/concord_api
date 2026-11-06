# Maintenance: split rows and late exceptions

Concord baseline `0983a349`; SDK author `66f3cf00`, first-use consumer `699239c2`.
The first attempt was completed and the full test suite passed before reading `FRICTION.md`
or `FIRST_USE.md`. No live provider was contacted.

## Edits by kind

- **Declaration:** no change. `select` already declares a streaming response and the optional
  `wait_end_of_query` setting in `src/wire.rs`.
- **Adapter:** the existing `Guarded` byte source already recognizes a tagged block across
  arbitrary splits; CSV and JSONL readers already handle split records. `open_rows` now treats an
  empty exception-tag header as unusable, matching the guard's existing behavior. Previously an
  empty header could produce `Outcome::Complete` even though the guard passed the body through.
- **Model:** no public variants or signatures changed. `Outcome` documentation now describes
  `Complete` as conditional on the tagged-block contract and includes empty tags in the
  unmarked outcome.
- **Tests:** the existing perturbation tests already split quoted CSV, UTF-8 JSONL, and exception
  blocks before a deliberately broken HTTP body. Added delayed chunked responses that terminate
  cleanly after the same fixtures and still yield the three rows followed by a typed late error;
  added both-format wire tests for empty tags and metadata preservation.
- **Docs and fixtures:** README and crate rustdoc qualify completion and explain clean framing;
  `PROVENANCE.md` identifies the new synthetic response cases. Existing perturbation fixture bytes
  and `fixtures/** -text` are unchanged. The existing README describes buffering and its caveat.

## Maintenance surface

The empty-tag fact was independently encoded in two production decisions: the guard's nonempty
tag check and `open_rows`' completion flag. One had to change; the guard was already correct.
Four independently maintained explanatory locations changed: the `Outcome` rustdoc, crate
rustdoc, README detection table and README completion explanation. The new protocol test separately
asserts the same classification. The tagged late-error behavior was already implemented, so the
new clean-framing test extends coverage rather than changing its declaration or adapter.

To locate these, read the SDK README, declaration, client, guard, row model, workflow and protocol
tests, testserver body API, and fixture provenance. The existing provider SDKs were not needed for
this change. The earlier author friction and first-use notes were read only after the first attempt;
they record the existing Concord reader/source adaptation and first-use limitations. No new
Concord workaround was introduced. The public signatures and generated names needed no change;
the first implementation compiled without a generated-name or other compiler diagnostic.

The repair was local to the SDK, but one semantic condition was duplicated between the body
guard and completion classification, with separate descriptions in the API docs and README.
The tagged exception contract does not cover output-format exceptions or arbitrary server behavior;
`wait_end_of_query=1` reduces late errors by buffering at server memory/disk and delaying rows,
but does not guarantee that partial results cannot escape.

Validation: `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-clickhouse`.
Offline result: 16 unit, 17 first-use, 26 protocol, 5 workflow and 5 doctests passed.
