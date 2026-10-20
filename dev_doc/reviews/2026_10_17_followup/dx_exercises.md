# V3 exercises: AI-agent execution and follow-up

**All twelve tasks were answered; eight new downstream behavioral tests pass.**
The local endpoint edit also produced a real compiled semantic diff with the
predicted endpoint and runtime classification. The exercise found two concrete
documentation problems: missing participant fixtures, and stale observer
authority/event wording. Both were corrected in this follow-up.

This record describes one AI agent working through the public exercise packet.
It is not a human session, independent-human validation, or a claim that any
human-study threshold passed. Initial predictions and mistakes are retained
below. The rubric was read after the answer record and the task-12 prediction
had been written; its scores are used only for candid agent self-assessment.

## Scope and evidence discipline

The [portable evidence bundle](evidence/dx/README.md) retains the initial
prediction and checked-answer records, exact isolated source patch, both native
contract exports, original CLI diff, and command/source/log digests. Its
manifest records actual dirty source states rather than claiming a clean
follow-up checkout.

The starting checkout was branch `concord-v3`, commit
`570eec460dee1d83828ae48e35396ae07652aabc`, initially clean. The first content
read was [the task packet](../../dx_study/tasks.md). Answers used public
Markdown, maintained example declarations, checked-in error fixtures and,
later, generated public rustdoc. No macro/core implementation was inspected
while solving the tasks. Existing facade tests were opened only after the
initial answer record, to implement the follow-up integration suite.

The packet originally omitted the three endpoints for task 5, concrete effect
examples for task 7 and the compiler error for task 10. The agent selected the
fixtures identified below; endpoint-specific answers for tasks 5 and 7 followed
that selection and public-documentation reading. They must not be represented
as independently predicted answers to previously supplied fixtures.

Per-task elapsed time was not instrumented. No reading times, medians, human
confidence distribution, recovery duration or human abandonment count is
invented. Native command durations below are measured process durations,
including compilation and Cargo lock waiting. Cargo/rustc were initially absent
from PATH; the task-local toolchain later enabled the actual commands.

## Predictions, corrections and self-assessment

Confidence is 1–5 before checking the relevant answer. A self-score of 2 means
the initial answer was materially correct; 1 means public-documentation or
compiler feedback supplied a correction or completed a materially incomplete
answer. These are agent self-scores, not qualifying participant rows.

| Task | Initial prediction and confidence | Checked result / correction | Self-score |
|---|---|---|---:|
| 1 | One outer `Option` for `?`; exact payload/slot types for all three controls. **4** | Correct layers; refined `set_x` to closed `IntoValue<T>` conversions, with exact `Option<T>` for replacement/conditional input. | 1 |
| 2 | All listed calls valid; reset restores initializer or absence without one. **4** | Wrong about uninitialized reset: that method does not exist. All other listed transitions were correct. | 1 |
| 3 | `each` adds 0/1/2 ordered occurrences without changing the vector type. **4** | Correct. An inherited occurrence remains under append, so total counts in the selected fixture are 1/2/3. | 2 |
| 4 | Plain assignment overwrites the inherited key; conditional/append/removal differ by absence; family replacement clears inheritance. **2** | Plain introduction over an earlier writer is a compile error. Explicit keyed `replace` owns the predicted replacement behavior. Present empty `each` also differs from absence. | 1 |
| 5 | Request and response inherit independently from their nearest owner. **3** | General model correct. The missing three-endpoint fixture was selected explicitly; exact owners are recorded below. | 1 |
| 6 | Overrides are local; an explicit fence cuts that direction's inheritance until a new choice. **3** | General model correct; public examples and stored diagnostics supplied exact inline syntax and focused errors. | 1 |
| 7 | Safe/idempotent replay requires body replayability; non-idempotent generally cannot retry. **2** | Added the settled-no-dispatch exception for non-idempotent policy retries and all independent gates. | 1 |
| 8 | Prefix succeeds incompletely; complete collection errors at an incomplete bound; lazy traversal stops. Uncertain about units/zero. **2** | Bounds count pages. Exact summaries distinguish bound, continuation and exhaustion. Zero performs no work and allocates no operation. | 1 |
| 9 | Acquisition precedes consumption; fallible byte/item polling; terminal failures do not reconnect; drop stops consumption. **3** | Added exact terminal states, one-error delivery, and the distinction between dropping a read future and dropping the handle. | 1 |
| 10 | No concrete error was supplied; generic expectation was an optional-control diagnosis. **1** | Selected the `clear_locale` fixture; `omit_locale()` expresses absence. Concrete diagnosis confidence was 5 after earlier control-table reading. A first fixture-note placeholder used `category`; the actual error and declaration name `locale`. | 1 |
| 11 | Observer/debug expose safe structural facts; limiter has narrow policy inputs; explicit plans/responses expose application data. **3** | Generated rustdoc supplied exact value-free observer fields and the limiter's protected-key/origin exception; it exposed stale prose. | 1 |
| 12 | An endpoint-local edit affects that endpoint only; precise selected effect edit should be `RuntimePolicy`, possibly provenance. **3** before export | Actual compiled diff: `catalog.item.update`, `effect.value`, `RuntimePolicy` only. The same owner means no provenance change. | 2 |

The confidence-4 reset error is a real calibration miss. The exercise recovered
through the public control matrix; it must not disappear behind the passing
automated checks.

## Checked answers

### Tasks 1–2: Rust payload and Concord presence

| Declaration | Stored slot | `set_x` accepts | `replace_x` accepts | `set_x_if_some` accepts |
|---|---|---|---|---|
| `x?: T` | `Option<T>` | `impl IntoValue<T>` | exact `Option<T>` | exact `Option<T>` |
| `x?: Option<T>` | `Option<Option<T>>` | `impl IntoValue<Option<T>>` | exact `Option<Option<T>>` | exact `Option<Option<T>>` |

`IntoValue` provides identity, `&str -> String` and `&String -> String`, with no
recursive container conversion. For `x?: Option<i32>`, the checked transitions
are:

| Call | Selected slot / validity |
|---|---|
| `set_x(None)` | `Some(None)`: present inner Rust `None` |
| `set_x(Some(7))` | `Some(Some(7))` |
| `replace_x(None)` | outer absence |
| `replace_x(Some(None))` | `Some(None)` |
| `set_x_if_some(None)` | preserve the entire selected slot |
| `omit_x()` | outer absence |
| `reset_x()` | available only with a retained initializer; no reevaluation |

With retained initializer `Some(3)`, reset selects `Some(Some(3))`; an inner
`None` initializer instead restores `Some(None)`. An explicit adapter owns
inner-null wire encoding; only Concord's outer absence means omission.
Sources: [generated controls](../../../docs/generated_client.md#value-controls)
and [specification §6](../../../docs/spec.md#6-values).

### Tasks 3–4: occurrence programs

`append "tag" = each tags;` keeps `Vec<String>` and contributes one occurrence
per element. The wire test sends empty, one-element and two-element vectors;
the inherited `tag=root` survives, yielding `root`, `root,one` and
`root,one,two` in order.

For inherited `locale=fr`, plain `"locale" = language` is rejected before
runtime presence is known. Explicit `replace` suppresses inherited values even
when the replacement is absent; `when_present` preserves inheritance on absence
but replaces it on presence; `append` preserves it and adds present values;
`remove` deletes the key. Plain introduction is valid for a key without an
earlier writer. A present empty `each` sequence under `when_present` suppresses
the parent and emits zero occurrences; absent optional data keeps the parent.

Family `replace` clears inherited occurrences before local direct placements
and block statements. `off` clears that family at its owner; a deeper owner can
introduce new entries. A direct placement in the same family cannot coexist
with `off` at the same owner; a replacement block is the valid choice there.
Header keys compare case-insensitively. Suppressed
writers are never encoded. Sources: [query/header programs](../../../docs/requests_and_values.md#query-and-header-programs)
and the maintained `docs_spec::patch_program` example.

### Tasks 5–6: representation direction and owner

The selected public [representation fixture](../../../crates/concord/tests/generated/representation_inheritance.rs)
has these exact answers:

| Endpoint | Request representation / owner | Response representation / owner |
|---|---|---|
| `inherited` | `Text` / client | `ParentBuffered` / client |
| `overrides::local` | `Text` / named scope `overrides` | `Text` / named scope `overrides` |
| `fenced::explicit` | `Text` / endpoint body inline `via` | `Text` / endpoint result inline `via` |

Every result needs a response representation; a bodyless endpoint needs no
request representation. A result's Rust spelling does not select its codec.
An endpoint inline choice changes only that direction on that endpoint. A scope
default affects descendants until a closer choice or a fence.

The new compiled exercise adds `POST added(body input: String) -> String;`
below a Text/Text override, and
`POST added(body input: String via Text) -> String via Text;` below both explicit
fences. It also sends a Text request with an inherited Json response and a Json
request with an inline Text response, despite all payloads being `String`.
Recorded bodies/media types and decoded results prove that the directions are
independent.

A missing choice beneath a fence yields `[C3-REP]`, naming the authored body or
endpoint, the blocking `request explicit` / `response explicit` scope, and
the correct owner-default or inline repair. The stored
[request diagnostic](../../../crates/concord/tests/ui/fail/syntax/request_fence_requires_inline_representation.stderr)
and [response diagnostic](../../../crates/concord/tests/ui/fail/syntax/response_fence_requires_inline_representation.stderr)
were inspected as supplied compiler feedback; those stored errors are not
claimed as fresh compiler runs. Source: [specification §9](../../../docs/spec.md#9-representation-defaults).

### Task 7: acceptance, effect and replay gates

The selected maintained `docs_spec::outcomes::OutcomeApi` endpoints are:

| Endpoint | Accepted statuses | Effect | Body replayability |
|---|---|---|---|
| `properties` | default 200–299 | explicit safe on `METHOD("PROPFIND")` | empty body, replayable |
| `upsert` | 200, 201, 204 | explicit idempotent | buffered Json, replayable |
| `create` | 201 | POST's non-idempotent default | buffered Json, replayable |

Safe/idempotent operations can policy-retry an eligible failure if every gate
passes. A non-idempotent operation can policy-retry only while aggregate
dispatch is settled as `NotDispatched`; ambiguous or observed dispatch forbids
resubmission. Mechanical replay alone grants no effect safety. A one-shot body
blocks replay even on a safe operation. Accepted-head validation and accepted
body conversion failures are terminal.

Other gates remain independent: eligible failure, classifier `Retry`, installed
policy, retry/send/replay/load budgets, body capability, deadline and server
minimum/backoff, and new rate admission. The new runtime test gives all three
effect classes a rejected 503 and an eager classifier: safe/idempotent send
twice, non-idempotent sends once; idempotent replay preserves the body bytes.
Sources: [retry](../../../docs/retry.md), [responses](../../../docs/responses.md#accepted-statuses)
and [prepared bodies](../../../docs/request_bodies.md#preparedbody).

### Task 8: bounded traversal outcomes

All bounds count pages. An incomplete `collect_prefix(n)` returns `Prefix<T>`
with `PageBound` and `NextRequestKnown`; `collect_all(NonZeroU32)` instead returns
`CompletionLimitExceeded`, never a partial success `Vec`. `pages(n)` gives
application-controlled pages and then `Ok(None)` with its stop reason retained.
A `Done` on the last allowed page reports exhaustion without an extra fetch.
Explicit early `finish()` records `Application`.

`collect_prefix(0)` and `pages(0)` do no shaping, controller start, body,
credential, quota or transport work and allocate no operation ID. Their summary
is `PageBound`/`Unknown`. Zero cannot inhabit `NonZeroU32`. The new zero test
uses a path value that fails ordinary planning: both zero terminals still
succeed without a request, proving the short circuit before shaping.
Source: [pagination](../../../docs/pagination.md#terminals).

### Task 9: stream acquisition and consumption

Successful `.await` opens a stream after accepted-head validation; it does not
prove EOF. Byte polling yields `Option<Result<Bytes, ApiError>>`; item polling
yields `Option<Result<T, ApiError>>` after framing/decoding complete records.
The same `1\nnot-json\n` payload is valid opaque bytes and a failing u32 NDJSON
record stream after its first item. The new test proves that distinction.

Transport failure affects both; framing/decode failure additionally affects
item streams. A terminal failure yields one error, then `None`, retains
`Failed` and `ResponseObserved`, and causes no retry after handoff. External
cancellation records `Cancelled`; explicit `finish()` records `Application`
without draining; dropping a live handle is cancellation, never EOF. Dropping
only a pending `next()` future preserves the stream's state and budgets for a
later read. This differs from dropping a started page fetch, which terminates
its traversal. Source: [streams](../../../docs/streams.md).

### Task 10: repair the authored call

The supplied error reports missing `ClearControlApiBuilder::clear_locale`.
Its authored input is root `locale?: String`. `omit_locale()` or
`replace_locale(None)` selects absence without editing generated code. The
generic compiler suggestion points to similarly named `set_locale`, whose
required payload expresses a different transition; the public control matrix
provides the semantic repair. The selected
[fixture](../../../crates/concord/tests/ui/fail/capabilities/clear_optional_control_absent.rs)
contains a positive control, so the revised fixture sheet withholds that source
until the initial diagnosis is recorded.

### Task 11: observer, limiter and application-data authority

Generated public rustdoc confirms that `OperationObserver` is synchronous,
read-only and value-free. `RequestTarget` supplies only path-segment,
query-occurrence and public-header-occurrence counts. Attempt/head events add
identities, method/page/attempt facts, body kind/known length, status, content
length and parsed server delay as appropriate. They expose no exact URL,
header names or values, body, credential material, raw rate key or native object.
Built-in debug levels likewise expose no header names or values.

`RateLimitRequest` exposes `cooldown_origin`, `endpoint`, `method`,
`logical_origin`, `page_index`, `attempt`, `plan` and `capability`. It is a trusted
consumer of the complete conjunctive plan, protected canonical keys and schema
descriptors. Logical origin is present only for a declared `origin` dimension
and can contain operational identifiers that require protection. The separate
cooldown identity is opaque and equality-only. This narrow admission role has
no arbitrary headers, body, credentials or executor mutation authority.

Explicit `RequestPlan` accessors expose sanitized URLs and ordered public
headers; ordinary public data can still be present. `ApiResponse::url()` is
credential-sanitized and `headers()` exposes actual headers. These are deliberate
application-data access, not safe automatic telemetry.

The public task page had incorrectly said observer events carry sanitized
targets and headers, and its event list omitted `ProviderAcquisitionFinished`.
Both are corrected in [planning and observation](../../../docs/planning_and_observation.md#observing-operations).
The actual public observer API already enforced the narrower boundary.

### Task 12: actual source edit and compiled diff

An isolated worktree of the exact starting commit changed only
`CatalogApi::regional::update_item` from `effect idempotent;` to
`effect non_idempotent;`. Both snapshots came from compiled Catalog declarations;
no JSON was manually edited. The shared Catalog declaration was not changed.

| Actual semantic diff field | Result |
|---|---|
| Contract format | `concord.contract/3` |
| Diff format | `concord.contract-diff/3` |
| Changed endpoint | `catalog.item.update` |
| Authored path | `CatalogApi::regional::update_item` |
| Changed field | `effect.value`: `idempotent` → `non_idempotent` |
| Classification | `RuntimePolicy` only |
| Caller / wire / auth / provenance changes | none reported |
| Unresolved dependencies | none |

This narrows automatic retry/recovery safety after an observed or ambiguous
dispatch while preserving the declared call and wire shape. The effect's owner
is unchanged, so the tentative extra provenance prediction was unnecessary.

## Navigation and real execution record

The navigation route was the repository README and public index, then
`requests_and_values.md`, `generated_client.md`, `request_bodies.md`,
`responses.md`, `retry.md`, `pagination.md`, `streams.md`,
`planning_and_observation.md`, `rate_limits.md`, `extensions.md` and the relevant
§6/§9/§10 specification sections. Searches included representation/fence
spellings, effect spellings, `RateLimitRequest`, and fixture filename searches
for optional controls and fence diagnostics. One wide combined output was
truncated, so the affected public pages were reread in smaller calls.

The maintained Catalog and `docs_spec` declarations supplied ordinary examples.
Task-11 rustdoc navigation followed hooks-path redirects to the public
`concord_core::contracts` pages for `OperationObserver`, `OperationEvent`,
`AttemptStarted`, `ResponseHeadObserved`, `RequestTarget` and `StreamFinished`,
and opened the public `rate_limit::RateLimitRequest` page. An attempted HTML
reader found BeautifulSoup unavailable; Python's standard `HTMLParser`
completed the read. No implementation-source fallback was needed.

| Real native command | Result | Measured duration |
|---|---|---:|
| `cargo test -p concord --test v3_user_exercises --no-default-features --features json,ndjson --locked` | exit 0; 8 passed, 0 failed | 7.363 s including compile; test body 0.17 s |
| `cargo run -p concord-xtask --features examples -- contract export --target catalog --output BEFORE.json` | exit 0; compiled baseline export | 29.285 s |
| same export after the isolated source edit, to `AFTER.json` | exit 0; compiled changed export | 33.038 s |
| `cargo run -p concord-xtask --features examples -- contract diff BEFORE.json AFTER.json --format json` | exit 0; one runtime-policy change | 19.599 s |

The integration test passed on its first native compilation/run. It ran against
the follow-up working tree; the receipt records its actual source state. The
contract commands ran in the separate original-commit worktree. These Linux
receipts do not claim another platform's execution or any measured human task
time. Stored `.stderr` inspection adds no new compiler cycle.

Receipt labels are `user-exercises`, `dx-contract-before`, `dx-contract-after`
and `dx-contract-diff`. [manifest.json](evidence/dx/manifest.json) records each
original command/receipt/log digest, source revision and actual before/after
status. The [before](evidence/dx/catalog-before.json) and
[after](evidence/dx/catalog-after.json) exports and the
[original semantic diff](evidence/dx/catalog-effect-diff.json) are retained
alongside the [exact source patch](evidence/dx/catalog-effect.patch).

## Follow-up changes and limits

[fixtures.md](../../dx_study/fixtures.md) now supplies reproducible inputs for
every task and separates first predictions from answer-revealing fixture
assertions/comments. [tasks.md](../../dx_study/tasks.md) links to it.
[v3_user_exercises.rs](../../../crates/concord/tests/v3_user_exercises.rs) adds
eight discriminating downstream tests; its Cargo test entry requires exactly
the JSON/NDJSON features used by the exercise. The observer task page now
matches its value-free public fields and complete event list.

The reset, plain-patch, retry and lifecycle corrections are retained as actual
agent learning. Passing code checks establishes the tested semantics and makes
future changes reviewable; it does not establish how quickly an unfamiliar
human will discover them. No new runtime defect was demonstrated by this
exercise suite, and no human result is inferred from it.
