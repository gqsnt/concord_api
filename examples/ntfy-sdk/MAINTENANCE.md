# Maintenance: split events and duplicate replay

The existing public contract already supports the ticket: SSE framing buffers partial input,
`Event` ignores unknown JSON members, keepalives are delivered without advancing the message
cursor, and replay returns every item without deduplicating it. The maintenance adds a combined
wire regression and corrects overconfident cursor documentation; existing callers need no API
changes. No live provider requests were made.

## Edits by kind

- **Declaration:** no operation or generated API change. `subscribe_sse` and `poll` already use
  the right formats and return the same `Events` type.
- **Adapter:** no behavior change. The existing SSE framer and `Events` reader already retain
  partial bytes, return keepalives, and expose completion and truncation.
- **Model:** no representation change. The `Since::Id` documentation now describes the ID as a
  replay selector without promising an exclusive boundary; `Event` already ignores unmodeled
  members.
- **Tests:** `split_live_event_and_duplicate_replay_leave_deduplication_to_the_caller` uses the
  loopback test server, a gated SSE fixture and a synthetic JSON replay fixture. It verifies
  consumed-byte positions while the stream pauses inside a multibyte character and between the
  two delimiter LFs, then checks the message, keepalive, unchanged cursor, clean completion,
  duplicate delivery, caller-owned filtering and exact request parameters. The fake explicitly
  returns a previously received item; this does not claim a real server uses that boundary.
- **Docs:** corrected the ID cursor wording in `src/events.rs` and the README, added the combined
  test to the README, and recorded the new fixture's synthetic provenance in `PROVENANCE.md`.
  The existing `.gitattributes` covers the new fixture.

## Locality and evidence

One replay-boundary fact appeared in **three independently maintained API descriptions** that
needed correction: `Since::Id` in `src/model.rs`, `ReplayCursor` in `src/events.rs`, and the
README workflow explanation. A fourth occurrence, the README example comment, also needed the
same correction. The fake server's explicitly chosen exclusive boundary remains documented as a
test choice, not a provider guarantee. Existing duplicate warnings and caller-side filtering
already described the intended behavior and did not need semantic changes.

To locate these, I read the SDK declaration, event adapter, framer, model, README, provenance,
protocol and workflow tests, the independent first-use test source, and the loopback server body
API. After the initial test compiled and passed, I read `FRICTION.md` and `FIRST_USE.md` as
requested; their existing findings did not require a new Concord workaround. No other provider
SDK supplied this solution. The first test compilation produced no diagnostics or generated-name
surprises. The test reused the SDK's existing handwritten SSE framer and `EventsError` adapter;
those Concord limitations and their reproductions were already recorded in `FRICTION.md`.

The change was local to the SDK, with no declaration or runtime implementation edit, but the
replay-boundary wording was repeated across the three descriptions and the example comment.
The first-use report separately notes a pre-existing ambiguity about newline-less complete JSON;
that issue is outside this ticket and was not changed here.

Validation: `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-ntfy` passed: 4 unit, 12 first-use, 21 protocol, 6 workflow and 5 doctests.
