# First use of `concord-sdk-snowflake` (independent consumer)

Runner: requested "Claude Opus subagent" (capsule `tasks/wave4/SDK16-snowflake-consumer.md`);
observed: this session's system metadata names model ID `claude-opus-5-5` (not independently
verified). I did not write this SDK and did not read its `FRICTION.md`, `src/` or tests. Library
frozen at `0983a349`; started from `04f32a9a` (the author commit). 2026-11-06, about
06:44–06:57 UTC.

Result: `cargo test --locked -p concord-sdk-snowflake --test first_use` → **18 passed**
(12 `a_*` tests for outcome A and its failure/completion cases, 6 `b_*` tests for outcome B). Clippy `-D warnings`
clean. **No interventions**: no SDK source or test read, no native escape, no private API.

## Predictions (written before the corresponding test code)

### Outcome A — submit async, reuse first result, fan out remaining partitions

Written after reading README, rustdoc and the three Snowflake pages, before any test code.

- **Submit request.** `POST /api/v2/statements?requestId=<submission UUID>&async=true`
  (order of the two query parameters unknown), headers `Authorization: Bearer <token>`,
  `X-Snowflake-Authorization-Token-Type: OAUTH`, `Accept: application/json`,
  `Content-Type: application/json`, `User-Agent: <ClientOptions::user_agent>`. JSON body with
  `statement`, `timeout`, and only the *set* session fields at top level
  (`warehouse`, not a nested `session` object); `None` fields omitted.
- **Submit return.** A 202 QueryStatus (`333334`) gives `Submission { query, state:
  StatementState::Pending(Reply { value: ServerMessage{code: "333334", ..}, meta (202) }) }`
  with `query.handle()` equal to the body's `statementHandle` and `query.context()` the
  submission ID. A 200 ResultSet gives `StatementState::Complete`.
- **Polling.** `Submission::wait` issues `GET /api/v2/statements/<handle>` with no
  `requestId` and no `partition`; 202 continues; 200 gives `PollOutcome::Complete(FirstResult)`
  whose `response().value.data` are the partition-zero rows. If the submit was already 200,
  no GET is sent at all.
- **Fan-out.** `bounded_partitions(max, window)` yields partition 0 first with no I/O, then
  `GET /api/v2/statements/<handle>?partition=1`, `?partition=2` with `Accept-Encoding: gzip`;
  bodies are gzip JSON with `Content-Encoding: gzip`, no metadata (`{"data": [...]}` or a bare
  array). Each yields `Ok(Partition { index, rows, meta })`; then `None`.
- **Completion signal.** `None` alone is not success: `finish()` → `PartitionReport` with
  `is_complete() == true`, `missing()` empty, `remaining.outcome == Outcome::Exhausted`.
- **Failure signals.**
  - submit 422 QueryFailureStatus → `Err(Error::Server(ServerError))` from `submit` (the
    `StatementState` enum has no failure variant), with `document.code`/`sql_state`.
  - poll 422 → `Ok(PollOutcome::Terminal(ServerError))`, `is_cancelled() == false`.
  - poll 429/5xx → `Err(Error::Server)` (README: no automatic retry).
  - polling bound → `Ok(PollOutcome::Unresolved { reason: Attempts | Deadline | LocalStop, .. })`.
  - later partition HTTP 5xx → `Some(Err(PartitionFailure { index, error: Error::Server }))`,
    then `None` (stop-on-error); report `outcome == Failed`, `missing()` names the index.
  - body cut mid-stream (connection dropped before chunked end) → `PartitionFailure` with
    `Error::Concord` (transport) — or `Error::PartitionBody(Gzip)` if the truncation surfaces
    while inflating; I do not know which.
  - row count/width different from `partitionInfo` → `Error::Protocol { kind: InvalidRows }`.
  - a request budget smaller than the later partitions → `outcome == Bounded`, `missing()`
    names the unfetched index, `report.plan.fetch(i)` fetches it afterwards.
  - `plan.fetch(0)` → `Error::Protocol { kind: InvalidPartition }` without I/O.

#### Observed for A (after the tests ran)

Every prediction above held, including the exact submit query pairs (`async=true`,
`requestId`), the flat body with only the set session fields, status GETs with no query at
all, `Accept-Encoding: gzip` only on partition GETs, `missing() == [1, 2]` after a stop-on-error
failure at 1 (window 1), and `Outcome::Bounded` with `missing() == [2]` for a budget of one.
The one open question resolved as: a chunked body dropped mid-stream is `Error::Concord`
with the 200 head kept (`meta: Some`); a gzip stream truncated inside complete HTTP framing is
`Error::PartitionBody { source: PartitionBodyError::Gzip(_) }`.

### Outcome B — submit and explicitly cancel a separate long-running statement

Written before the B tests.

- **Submit** as in A with its own submission UUID → 202, handle B.
- **`query.cancel_and_wait(cancel_id, policy, stop)`** sends exactly one
  `POST /api/v2/statements/<B>/cancel?requestId=<cancel UUID>` (not the submission UUID) with the
  common headers. Body: unknown — the docs describe none; I predict an empty body.
- 200 CancelStatus `000604`/`57014` → `acknowledgement: Ok(Reply { value.code: "000604" })`; then
  `GET /api/v2/statements/<B>` (no `requestId`) → my fake answers 422 QueryFailureStatus with
  the same pair (synthetic: the docs do not show the post-cancel status body) →
  `outcome: Ok(PollOutcome::Terminal(e))` with `e.is_cancelled()`.
- Cancel rejected (422 `000709`/`02000`, "not found") → `acknowledgement: Err(Error::Server)`,
  polling still runs; if the status is 200 the race is `outcome: Ok(Complete)`.
- The reference's *CancelStatus object* section shows a different example (`code "0"`,
  "successfully canceled") from the operation example (`000604`/`57014`). The README says only
  the pair is accepted, so I predict a 200 with `code "0"` gives
  `acknowledgement: Err(Error::Protocol { kind: UnexpectedCancellation })`.
- A pre-stopped `LocalStop`: the cancel POST is still sent, then
  `Unresolved { reason: LocalStop, attempts: 0 }` with no GET. `LocalStop` alone never sends a
  cancel POST (already checked in A: `poll` with a stopped stop sends nothing).
- 408 on the submit POST (reference: "exceeded the timeout period, cancelled") →
  `Err(Error::Server)` from `submit` with status 408; `is_cancelled()` only if the document has
  the cancellation pair.
- Cancelling B never touches A's handle or A's partitions.

#### Observed for B (after the tests ran)

Every B prediction held: one cancel POST with `requestId=<cancel UUID>` only, the common
headers and an **empty body**; acknowledgement and observation kept separately; a rejected
cancel (`000709`) kept as `Err(Error::Server)` while completion won the race; a `code "0"`
acknowledgement refused as `UnexpectedCancellation`; a pre-stopped `LocalStop` sent the cancel
and nothing else; the submit 408 became `Err(Error::Server)` with `is_cancelled() == true` and
no cancel POST. Not predicted but observed: after a bounded `cancel_and_wait`, a later
`Query::status()` on the cancelled statement returns `Err(Error::Server)` (with
`is_cancelled()`), where `poll` would have returned `Ok(Terminal)` for the same response.

#### The one prediction that failed

A well-formed 200 ResultSet **without** `partitionInfo` is `Error::Concord { meta: Some(200),
source.kind() == Decode }`, not `Error::Protocol { kind: InvalidManifest }`. I predicted the
latter from the rustdoc of `InvalidManifest` ("Metadata is absent, inconsistent, …") and the
README ("Discovery rejects missing/empty descriptors"). An **empty** `partitionInfo: []` is
`InvalidManifest`. The test pins the observed behaviour (both are refusals; nothing is
accepted as a result) and the difference is listed as friction F1.

## Trace

1. Read the capsule. Entry point: `examples/snowflake-sdk/README.md` (Workflow A/B code).
2. Read `PROVENANCE.md` (linked from README). The README does not point to specific fixture
   files for consumers, so I author my own fake responses from the Snowflake docs instead of
   reusing `fixtures/*`.
3. `cargo doc -p concord-sdk-snowflake -p concord-example-testserver --no-deps`; rustdoc read as
   text through a scratch HTML-to-text script (no plain-text rustdoc output exists).
4. Snowflake docs fetched: submitting-requests, handling-responses, cancelling-requests. The
   cancelling page gives only `POST /api/v2/statements/{handle}/cancel`; its body/response
   contract is on the SQL API reference page, which I also fetched (cancel 200 `000604`/`57014`,
   cancel 422 `000709`/`02000`, 408 on POST = timeout-cancelled).
5. Concord docs: `crates/concord-pagination/README.md`, `docs/CALLER_GUIDE.md` §1–2, rustdoc for
   `concord::ResponseMeta`, `concord::Operation`, `concord_pagination::{IndependentReport,
   Options, Outcome, Progress, Unfinished, Keyed, Budget}`; later (step 9) `concord::Error` and
   `concord::ErrorKind`.
6. Wrote the outcome A prediction above, then `tests/first_use.rs` with my own fake provider: a
   `TestServer` responder routing `METHOD path[?partition=N]` to one `Script` per route (an
   unscripted request gets a 418 so it cannot pass silently). Response documents are inline
   `serde_json::json!` values hand-authored from the pages in step 4; later partitions are
   gzip-compressed at test time with `flate2` (already a dependency of the SDK package), so no
   byte-sensitive fixture file was added and the SDK's own `fixtures/` were not used.
7. First compile of the 11 A tests: **no compiler errors**. Only diagnostic: 5 `dead_code`
   warnings for the B constants/helpers not yet used. All 11 passed on the first run.
8. Because a first-run pass proves little, I mutated three assertions (submit body warehouse,
   literal `"null"` vs SQL NULL, budget indexes) and confirmed each failed, then restored the
   file byte-for-byte (`cmp`).
9. Added `concord::ErrorKind` checks (transport vs decode) to two A tests. This compiled only
   because `concord` is a dependency of the SDK package itself; see F2.
10. Wrote the outcome B prediction, then 6 B tests and one more A test (malformed/missing
    manifest). Compile: no errors, no warnings. Run: 17/18; the failure was the
    `partitionInfo`-missing prediction (see above). I changed that one expectation to the
    observed `Concord`/`Decode` behaviour with a comment and added the empty-`partitionInfo`
    case, which does give `InvalidManifest`. 18/18.
11. Mutated two B assertions (cancel `requestId`, `is_cancelled()` on the later status) and
    confirmed both failed; restored. `rustfmt --edition 2024` on my file only (the package-wide
    `cargo fmt` would also touch the SDK's `src/`); `cargo clippy --locked -p
    concord-sdk-snowflake --all-targets -- -D warnings` clean; the SDK's existing
    `protocol`/`workflows` suites still pass (18 and 13).

Working path, as an application would write it: `Client::new(origin, token, TokenType::OAuth,
ClientOptions{user_agent, ..})` → `client.submit(RequestContext{submission_id}, statement)` →
`submission.wait(policy, &LocalStop)` → `PollOutcome::Complete(first)` →
`first.bounded_partitions(max_requests, window)` → `while let Some(p) = partitions.next()` →
`partitions.finish()` → `report.is_complete()` / `report.missing()` → `report.plan.fetch(i)`
to retry. Cancel: `submission.query.cancel_and_wait(cancel_id, policy, &LocalStop)` → inspect
`acknowledgement` and `outcome` separately. The README's two workflow examples supplied this
path directly; no other provider SDK under `examples/` was read.

## Friction

- **F1. Missing `partitionInfo` is a decode error, not `ProtocolError::InvalidManifest`.**
  README "Discovery rejects missing/empty descriptors" and the variant doc "Metadata is absent"
  suggest `Error::Protocol`. Observed: `Error::Concord { meta: Some(200), source.kind() ==
  Decode }`; only an empty `[]` is `InvalidManifest`. An application that treats
  `Error::Protocol` as "provider result unusable" and `Error::Concord` as "transport/codec
  trouble, maybe retry" would misclassify a manifest-less result. Reproduction:
  `a_malformed_result_document_is_a_decode_error_not_a_result` (second scripted reply).
- **F2. Concord types are in the public API but `concord` is not re-exported.** `Reply::meta`,
  `ServerError::meta`, `Partition::meta` are `concord::ResponseMeta` and `Error::Concord`
  carries a `concord::Error`; the SDK re-exports only `pagination`. Calling `.status()` works
  without naming the type, but telling a dropped body (`ErrorKind::Transport`) from a bad
  document (`ErrorKind::Decode`) needs `concord::ErrorKind`, so an application must add its own
  dependency on the same Concord crate and keep it in lock-step. Observed here only as "it
  compiles because the test lives inside the SDK package"; the failure in a separate
  application crate is **inferred** from the re-export list, not compiled.
- **F3. Poll defaults are not documented.** The README's examples use `PollPolicy::default()`,
  but neither README nor rustdoc states its attempts, interval or elapsed limit (the
  `Default` impl has no doc). `ClientOptions` documents its defaults except `user_agent`. I
  sidestepped by constructing every `PollPolicy` explicitly.
- **F4. `Query::status()` vs `poll()` classify the same 422 differently.** A cancelled or failed
  statement is `Ok(PollOutcome::Terminal)` from `poll`/`wait`/`cancel_and_wait`, but
  `Err(Error::Server)` from `status()`, because `StatementState` has no failure variant. The
  `status()` rustdoc does not say this; I predicted it only from the enum's shape. Reproduction:
  the last check in `b_cancel_observation_is_bounded_and_resumable`.
- **F5. 429 while polling ends `wait` with an error.** The handling-responses page says a 429
  on a status GET means the statement has not finished (QueryStatus body); the SDK, as its
  README states, returns `Err(Error::Server)` with no retry. That is documented, but the
  application owns the resume loop (keep a cloned `Query`, check `meta.status() == 429`, call
  `poll` again) and there is no typed "throttled" signal. Reproduction:
  `a_rate_limited_status_check_is_an_error_without_retry`.
- **F6. Snowflake's own CancelStatus examples disagree.** The reference shows `000604`/`57014`
  in the cancel operation example and `code "0"`, "successfully canceled" in the CancelStatus
  object section. The SDK accepts only the pair and refuses `"0"` as `UnexpectedCancellation`.
  If a live account answered with the object-section shape, `Query::cancel` would report an
  error for a cancel that may have succeeded; `cancel_and_wait` still observes the real status.
  Which shape live accounts send is unknown here (no live call). Reproduction:
  `b_cancel_acknowledgement_without_the_documented_pair_is_refused`.
- **F7. Tooling, not SDK:** rustdoc is HTML only; reading it from a terminal needed a scratch
  HTML-to-text script.

Things that worked without friction: the README's two workflow examples (compiled as doctests
in the package run) used exactly the API my tests needed; every documented wire detail I checked (query pairs, flat body, header set, gzip only
on partitions, no `requestId` on GETs, separate cancel ID, empty cancel body) held; `None` vs
`finish()` completion and `missing()` made partial outcomes explicit; Display/Debug of a 422
did not leak the SQL-bearing message.
