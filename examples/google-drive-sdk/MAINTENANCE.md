# SDK12 maintenance: Unicode, boundary collisions and interrupted upload

Date: 2026-11-06. Requested runner: Codex gpt-6-astra, xhigh.
This maintenance session did not author the SDK. Starting HEAD was `e8968c80`
(author and independent consumer commits above frozen Concord `6d6d66aa`).
All edits are under `examples/google-drive-sdk/`; no library, workspace manifest,
lockfile, other example, script, check or shared documentation was changed.

## First attempt and result

The baseline already implemented the ticket: Unicode JSON metadata, collision
scanning of both multipart parts, acknowledged resumable prefixes, checkpoint
restoration with mandatory probing, and an extra changes page. Its initial full
suite passed 34 integration tests and 3 compiled README examples. A behavior
change or caller migration was not necessary.

The first maintenance attempt added three independent wire regressions using
the existing public API and loopback testserver. All three compiled and passed
on their first run, without changing runtime logic:

1. A decomposed Unicode filename contains candidate boundary 0; the media contains
   candidate 1 and candidate 2 delimiter-like lines. The request matches a
   hand-authored multipart fixture byte for byte, using candidate 3.
2. Stop immediately after an acknowledged 256 KiB chunk, serialize the checkpoint
   and initial changes token, restore with a fresh client, probe, and finish.
3. Save that same checkpoint, attempt another chunk, have the server commit only
   17 bytes and drop the response, then restore the older checkpoint. Probe
   discovers offset 262161; send an aligned-length chunk from that unaligned
   offset and the final 20 bytes. The server's accepted bytes equal the source.

Both restart tests compare the raw returned session target on every PUT, including
escape case, query ordering, repeated parameters and an empty parameter. They
also consume a changes page and an empty continuation page, stop at a two-page
budget, retain the next token, encounter a typed 403 on the extra page, retain
that failed position, resume explicitly, and persist/reuse the terminal polling
token. Tokens contain reserved characters and Unicode and survive serialization.
Request counts verify that caps, blocked writes and failed traversals do not send
hidden requests; failure remains `Ended`, not successful exhaustion.

`FRICTION.md` and `FIRST_USE.md` were first opened only after this attempt passed.
They confirmed existing recovery and page-metadata obligations; they did not
supply the new implementation. No tests or expected outcomes were weakened.

## Edits by kind

| Kind | Edit |
| --- | --- |
| Declaration | None; `src/api.rs` already declares JSON metadata and runtime `EncodedBody` uploads. |
| Adapter | Runtime unchanged. `src/upload.rs` rustdoc clarifies checkpoint persistence, separately retained changes state, and exact unaligned acknowledged offsets. |
| Model | Layout/serde unchanged. `src/model.rs` documents that `NewFile.name` is serialized without Unicode normalization or MIME filename encoding. |
| Tests | New `tests/maintenance.rs`: three tests sharing a local fake server workflow. Three byte-sensitive fixtures added; existing `.gitattributes` retained. |
| Docs | README explains the edge cases and links this record; PROVENANCE lists the fixtures, official sources and precise simulation limits. This file records the maintenance evidence. |

## Independent descriptions and locality

**Zero independently maintained runtime descriptions had to change for the
ticket.** There is no semantic delta duplicated between declaration, model and
adapter. Boundary selection remains one implementation in `upload_multipart`;
acknowledgement acceptance remains one implementation in `UploadSession::accept`;
the changes projection and validation remain in `client.rs`. Their existing
contracts cover the new cases. Source edits are rustdoc only.

Test oracles intentionally repeat some facts: the filename has three authored
representations (Rust constant, metadata fixture, complete MIME fixture), and
the selected boundary has two expectations (header assertion and MIME fixture).
Changing that fixture scenario would require synchronizing those independent
descriptions. Source length, expected ranges and response sizes derive from the
source vector instead of adding independent numeric definitions. Each opaque
token and the returned session target have one fixture constant, used as server
input and compared against observed wire output. Source rustdoc and README both
describe recovery and Unicode preservation; those prose additions mirror the
existing behavior rather than introduce a new protocol rule.

The change was local: one new integration-test file, three fixtures and SDK
documentation. Existing workflows and first-use callers were preserved.

## Reading and friction

Read the SDK README and provenance, `src/api.rs`, `client.rs`, `model.rs`,
`upload.rs`, the protocol/workflow tests and shared support, and the first-use
test entry point. The root README, relevant author-guide/recipe sections,
extras and pagination READMEs, and generated Concord rustdoc for
`Prepared::follow`, `Prepared::receive` and `EncodedBody::with_content_type`
established the public transport boundary. Read the testserver request/reply API.
The facade has no separate `crates/concord/README.md`; its guide is the root
README. Official upload and changes references were checked as recorded in
PROVENANCE. No other provider SDK supplied this maintenance solution.

No generated-name surprise, compiler diagnostic or new Concord workaround was
observed. The existing domain client handles the generated endpoint types;
the new tests need no generated type names. Runtime-body binding, same-origin
`follow`, response inspection and bounded pagination already work through public
APIs. No private reach-through, reconstructed execution, handwritten operation
name, native request mutation or new dependency was introduced. There is no new
friction entry requiring a library intervention.

## Validation

All commands run from the workspace root with the required shared target and
five build jobs. Reproduction of the added tests:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-google-drive --test maintenance
```

Final validation passed:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-google-drive
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo clippy --locked -p concord-sdk-google-drive --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo fmt -p concord-sdk-google-drive -- --check
```

Results: **37 integration tests (5 first-use, 3 maintenance, 25 protocol,
4 workflow) and 3 README doctests passed**; no unit tests are defined. Clippy
reported no warnings; formatting and `git diff --check` passed. Concord rustdoc
generation with the requested `csv,jsonl` features also succeeded.
`git check-attr text` reports `unset` for all three new fixtures.

All SDK HTTP traffic is to the owned loopback server with synthetic bearer
values. Official documentation was read, but no live provider API calls,
credentials or vendor conformance verification were used.
