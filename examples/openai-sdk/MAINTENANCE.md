# SDK08 maintenance: split UTF-8/SSE chunks and reorder batch outputs

Ticket (chosen before the maintainer saw the SDK): split an SSE delimiter and a multi-byte UTF-8
code point across chunks, reorder JSONL output lines with a failed item, and assert event
decoding and correlation by ID rather than order.

- Baseline: branch `sdk20/w2-sdk08-openai` at `08f97864` (author `8a659204`, first-use consumer
  `08f97864`), Concord frozen at `e04236ca`. The maintainer did not write this SDK.
- Requested runner: Claude Opus subagent. Observed: the harness system prompt names the model
  `claude-opus-5-5`. Reasoning effort is not exposed to the session, so it is not recorded.
- Order: the capsule, then the SDK and its existing tests, then the change. `FRICTION.md` and
  `FIRST_USE.md` were read only after the first attempt passed (all 30 tests, clippy clean). After
  reading them, no source, test, fixture, README or PROVENANCE edit was made; only this file was
  written.

## Result

The SDK already satisfied the perturbation. No change was needed in the declaration, adapter or
model. `EventStream::next` (`src/lib.rs:500`) collects raw bytes until a line feed, strips one
trailing CR, and decodes an event's JSON only after its blank line. So a chunk boundary inside a
CRLF, inside the blank line or inside a code point cannot change what is decoded.
`BatchResults::add_file` (`src/lib.rs:211`) keys rows by `custom_id` in a `BTreeMap`, and
`batch_results` (`src/lib.rs:452`) merges both files. Row order and which file a row is in do not
matter. The change is therefore tests, fixtures and docs.

| Kind | Edits |
| --- | --- |
| Declaration (`concord::api!`) | none |
| Adapter (`EventStream`, `checked`/`send`, `batch_results`) | none |
| Model (`Event`, `BatchItem`, `BatchResults`, …) | none |
| Tests | new `tests/split_and_reorder.rs`: 4 tests |
| Fixtures | new `fixtures/error-event.sse`, `fixtures/reordered-output.jsonl`, `fixtures/reordered-error.jsonl` (covered by the existing `fixtures/** -text`) |
| Docs | `README.md`: 2 sentences; `PROVENANCE.md`: 1 row amended, 2 rows added; this file |

### Tests added (all on the wire through `examples/testserver`)

1. `the_test_server_delivers_every_cut_as_a_separate_client_chunk` uses a plain reqwest client.
   It checks that a body served one byte per HTTP chunk reaches the client as 356 one-byte
   `chunk()`s. Without that, the perturbation could be vacuous: TCP or hyper could merge the cuts.
2. `sse_split_at_every_boundary_decodes_the_same_events` serves `events.sse` and a CRLF copy made
   in the test. Each is served once per interior cut (one cut per stream) and once one byte per
   chunk: 721 streams in total. Every stream must give exactly `created resp_fixture`, the
   `café 🌍` delta and `completed`, then `None`. The test also asserts that the fixture still
   contains `\n\n`/`\r\n\r\n`, `é` and `🌍`, so the coverage cannot quietly disappear if the
   fixture is edited.
3. `split_error_event_and_split_eof_stay_typed_failures` cuts `error-event.sse` (LF and CRLF, 730
   streams) the same way. That fixture has a two-line `data` delta and an `error` event with 2-,
   3- and 4-byte characters. Every stream must yield the created event and the `partiel é` delta,
   then `Error::StreamEventError` with the exact message and `code`. A clean EOF two bytes into
   `🌍` (221 layouts) must give `Error::Stream`, never completion.
4. `batch_rows_correlate_by_custom_id_in_every_reordered_layout` uses inputs request-1…4. The
   fixture's output order is request-4, request-2 (HTTP 400), request-1, and the error file holds
   the expired request-3. The test runs ten layouts: all 6 orders of the output rows with the
   failure among them, plus 2 output orders × 2 error-file orders with the failed row moved to the
   error file, as the official guide describes. Each file is served one byte per chunk, so
   non-ASCII bytes are split. Every layout must give exactly the submitted IDs as keys, no
   problems and no file errors, and the same outcome for each ID. A final case drops the error
   file: request-3 is then absent from `items` and is not reported. The caller finds it by
   comparing keys with the submitted IDs. That behaviour is now stated in the README.

### Do the tests catch the bugs they target? (temporary mutations of `src/lib.rs`, reverted)

| Mutation | New tests | Existing `workflows.rs` |
| --- | --- | --- |
| M1: decode each chunk with `from_utf8_lossy` | 2 fail | 1 fails |
| M2: drop a partial line at each chunk boundary | 2 fail | 1 fails |
| M3: key results by row `id` instead of `custom_id` | 1 fails | 2 fail |
| M4: treat a CR at the end of a chunk as a line end | 1 fails | all pass |

M4 passed the first version of the new tests. A split CR|LF makes a spurious blank line, which
dispatches the event early, but that is harmless when each event has a single `data` line.
Making the delta in `error-event.sse` a two-line `data` event (framing the README already claims
to support) exposed it. `src/lib.rs` was byte-compared with a backup after each mutation.

## Places that encode the same facts

**Fact 1: chunk boundaries do not affect SSE decoding.** Five places maintained independently:
- the parser (`src/lib.rs:500`);
- the README's Responses paragraph;
- the `events.sse` row of `PROVENANCE.md` ("Tests split the delimiter and a code point");
- `FRICTION.md` item 4 ("The loopback perturbation splits…");
- the hand-picked cut positions in `tests/workflows.rs::ordinary_and_fragmented_stream_through_completion`.

The consumer's `first_use.rs` streaming test makes a sixth. Changed: README (stated the guarantee)
and PROVENANCE (pointed at the new test). FRICTION.md is the author's log and is still accurate,
so it was left alone.

**Fact 2: batch rows correlate by `custom_id`, not position or file.** Five places:
- `BatchResults` and `batch_results` in `src/lib.rs`;
- the README's Batch paragraph;
- the `output.jsonl` row of `PROVENANCE.md` ("reverse input order");
- the row order inside `fixtures/output.jsonl`;
- the keyed assertions in `tests/workflows.rs`.

`first_use.rs` also checks IDs against its own submitted set. Changed: README (merge semantics,
iteration order, absent IDs) and PROVENANCE (new rows).

So one fact is described in four or five places, but the description lives in prose and fixtures,
not code. The single semantic fact (independence from chunking and order) touched only the two
prose descriptions, and only to make them more precise. No code had to change in step.

## What had to be read

- The capsule.
- All of `README.md`, `PROVENANCE.md`, `Cargo.toml` and `src/lib.rs` (560 lines; the relevant
  parts are `EventStream::next`, `parse_batch_jsonl`/`BatchResults` and `batch_results`).
- All of `tests/protocol.rs` and `tests/workflows.rs`.
- Only the imports and `results.*` uses in `tests/first_use.rs`, to keep its public-field use
  compatible.
- `examples/testserver/src/lib.rs`, the `Body` variants and `write_chunked`. This showed that
  `Body::Chunks` with zero delays writes one HTTP chunk frame per piece; test 1 confirms what the
  client then receives.
- Concord: `docs/CALLER_GUIDE.md` §6a, `docs/RECIPES.md` "Streaming a large or endless body" and
  the `docs/CUSTOM_FORMATS.md` §6 note on `concord_extras::jsonl`. I read these to see whether a
  Concord SSE or JSONL reader should replace the hand-written framing. No SSE reader exists, and
  the JSONL reader does not keep going past a malformed row, which `BatchResults` needs (author
  FRICTION item 5).
- Official Batch guide, re-read 2026-11-06 (a docs fetch, not an API call). It says output order
  may not match input order, to use `custom_id`, and that failed requests' error information is
  written to the error file. It shows an example line only for expired requests.

## Generated-name and compile surprises

- No generated names were involved, because the declaration was not touched.
- No compiler diagnostics. clippy on `--tests` was clean.
- rustfmt rewrapped five spots in the new test file only.
- `Event` and `BatchItem` derive neither `PartialEq` nor `Clone`, so the tests compare a
  `describe`/`outcome` string per value.
- `Batch` has all-public fields and can be built with a struct literal (and `..batch`) in tests.
- The only surprise was M4 above.

## Observations not changed (outside the ticket; owner decision)

- **Observed:** a leading UTF-8 BOM is not stripped. A throwaway probe test, deleted before
  commit, served a BOM, a `response.created` event and a `response.completed` event. The caller
  saw only `completed`: the BOM-prefixed `data:` line is not recognised, and that first event is
  silently dropped. Completion is still required, so this is not a false success. OpenAI frames
  start with `event:`, which a BOM would only make ignorable, and nothing documents a BOM from
  this provider. The WHATWG SSE parser ignores one leading BOM.
- **Inferred from code:** with a duplicate `custom_id`, the first row read is kept. Which one that
  is depends on row and file order. The duplicate is still reported in `problems`, as a string.
- **Observed by grep:** the `jsonl` feature on `concord` in `Cargo.toml` is unused by `src/`.
- **Not exercised:** `Event::OutputTextDelta` exposes only `delta`, so deltas cannot be attributed
  to an output item by ID. Adding fields would change a pattern used by the README and
  `first_use.rs`. The provider's field list was not re-verified in this session.

## Felt local?

Yes. Writing the perturbation needed no SDK source edit: one new test file, three fixtures and
three short doc edits, all in `examples/openai-sdk/`. The non-local part was reading: confirming
that the cuts really reach the client meant inspecting the shared test server. Keeping the facts
coherent meant finding the four or five prose and fixture places that restate them.

## Commands

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-openai
```

Result: lib 0, `first_use` 18, `protocol` 3, `split_and_reorder` 4, `workflows` 5. That is 30
passed, 0 failed, 0 ignored; 0 doctests. `cargo clippy --locked -p concord-sdk-openai --tests`
gave no warnings. These are offline loopback tests with synthetic data, not vendor conformance
evidence.
