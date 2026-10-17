# V3 participant exercise fixtures

These inputs make [the twelve tasks](tasks.md) reproducible. Use the exact
candidate selected for the session and record its commit and any local changes.
Do not treat a stored compiler output as a new compiler run. A facilitator may
copy the declarations below into a small downstream crate with the indicated
features; support types remain ordinary public Rust code.

## Tasks 1–2: payload, presence and reset

Use `T = i32`. Compare uninitialized `x?: i32`, uninitialized
`x?: Option<i32>`, and initialized `x?: Option<i32> = INITIAL_X`, where
`INITIAL_X: Option<i32> = Some(3)`. Also consider the initializer `None`.
The names denote distinct exercises, not three same-named values in one owner.

For wire inspection, use an explicit adapter for the `Option<i32>` payload:
inner `None` renders the literal `null`, and `Some(n)` renders its decimal
number. The adapter does not decide whether an occurrence exists. Task 1 asks
for both the stored type and the complete accepted argument types/conversions.

Public references are [values](../../docs/requests_and_values.md#values-and-presence)
and [generated controls](../../docs/generated_client.md#value-controls).

## Tasks 3–4: repeated values and inherited patches

Start with the maintained `PatchApi` declaration in
[the query/header task page](../../docs/requests_and_values.md#query-and-header-programs),
authored as `docs_spec::patch_program` in
[the examples crate](../../crates/concord-examples/src/docs_spec.rs).

For task 3, add an inherited `"tag" = "root"` query occurrence and a local
required `tags: Vec<String>` parameter. Use the requested
`append "tag" = each tags;` statement. Test empty, one-element and two-element
vectors and distinguish total occurrences from newly appended occurrences.

For task 4, use inherited `locale=fr` and a local `language?: String`. Repeat
the experiment for inherited header `X-Locale: fr`, targeting `x-locale` locally.
Evaluate each keyed operation separately, including a plain introduction over
an already-owned key. Compare optional absence with a present empty
`Vec<String>` under `when_present ... = each ...`. For family `replace` and
`off`, check whether a deeper owner can introduce a new key.

## Tasks 5–6: the three representation endpoints

Use exactly these three endpoints from the public
[representation-defaults example](../../docs/spec.md#9-representation-defaults):

| Endpoint | Authored source |
|---|---|
| `RepresentationInheritanceApi::inherited` | root endpoint |
| `RepresentationInheritanceApi::overrides::local` | endpoint in `overrides` |
| `RepresentationInheritanceApi::fenced::explicit` | endpoint in `fenced` |

The complete declaration and ordinary `ParentBuffered` support type are in the
`representation_inheritance_contract` anchor of
[representation_inheritance.rs](../../crates/concord/tests/generated/representation_inheritance.rs).
For the first prediction, read its authored declaration and support-type
documentation; do not use the assertions below the anchor as an answer key.
The fixture requires no JSON feature.

For task 6, add a new `POST added` returning `String` with a `String` body under
`overrides`, then a separate `POST added` under `fenced`. Make both compile.
Separately remove each direction's required representation beneath a fence
and predict the authored location and recovery suggestion in the error.
After recording the prediction, compare the maintained
[request-fence diagnostic](../../crates/concord/tests/ui/fail/syntax/request_fence_requires_inline_representation.stderr)
and [response-fence diagnostic](../../crates/concord/tests/ui/fail/syntax/response_fence_requires_inline_representation.stderr),
or run the corresponding `.rs` fixture with the candidate toolchain.

## Task 7: status, effect and policy retries

Use `OutcomeApi::properties`, `OutcomeApi::upsert`, and `OutcomeApi::create`
from the `outcomes` module in
[docs_spec.rs](../../crates/concord-examples/src/docs_spec.rs), also reproduced in
[specification §10](../../docs/spec.md#10-methods-status-effect-and-absence).
This fixture requires `json`.

Evaluate a policy that asks to retry a rejected 503 with one retry available,
remaining send/replay/load budgets, sufficient time, and a new rate admission.
Also evaluate a connection failure settled as `NotDispatched`, an ambiguous
transport failure, and accepted-head/body conversion failure. Finally substitute
a one-shot streaming request body while keeping the effect unchanged. Report
which gate changes each answer; a retry classifier expresses appetite only.

## Task 8: preview, synchronization and lazy traversal

Use the maintained `PaginationApi::offset` endpoint in
[the pagination example](../../crates/concord-examples/src/pagination/mod.rs),
with initial offset 0 and page size 2. This fixture requires `json`.
Use a two-page bound. Compare provider pages `[1, 2]`, `[3, 4]`, `[5]` with a
separate run whose second page is `[3]`. Test all three terminals, then zero.
For lazy traversal, also stop explicitly after the first page.

## Task 9: bytes and records

Use `BodyApi::download` from
[bodies.rs](../../crates/concord-examples/src/request_response/bodies.rs) and
`RecordsApi::nd_json_stream` from
[records.rs](../../crates/concord-examples/src/request_response/records.rs).
The record endpoint requires `ndjson` (which enables `records`).

For bytes, return `1\nnot-json\n` with `application/octet-stream`; for records,
return the same bytes with `application/x-ndjson`. Separately compare clean
EOF, truncated transport, malformed record, external cancellation, explicit
`finish()`, a dropped stream handle, and a dropped pending `next()` future.
Record acquisition and consumption outcomes separately.

## Task 10: a generated method error

Supply
[clear_optional_control_absent.stderr](../../crates/concord/tests/ui/fail/capabilities/clear_optional_control_absent.stderr)
as the initial diagnostic. The authored client is `ClearControlApi`; its root
value is `locale?: String`, used by `query { "locale" = locale; }`, and the
failing call is `ClearControlApi::builder().clear_locale()`.

After the first diagnosis and confidence are recorded, allow navigation to
[the authored fixture](../../crates/concord/tests/ui/fail/capabilities/clear_optional_control_absent.rs)
and generated public documentation. The `.rs` includes a positive control and
an explanatory comment, so exposing it before the first prediction would reveal
the intended repair. Distinguish the compiler's similar-name suggestion from
a method that expresses the intended state transition.

## Task 11: narrow authority and explicit metadata access

Locate the public rustdoc for `OperationObserver`, `OperationEvent`,
`AttemptStarted`, `ResponseHeadObserved`, `RequestTarget`, `RateLimitRequest`,
`RequestPlan`, and `ApiResponse`. Start at
[planning and observation](../../docs/planning_and_observation.md) and
[limiter admission](../../docs/rate_limits.md#admission). Include built-in debug
levels and the origin-dimension exception in the comparison. Do not substitute
private source inspection for public API navigation.

## Task 12: one endpoint effect edit and its actual semantic diff

Use the canonical extended
[Catalog declaration](../../crates/concord-examples/src/contracts/catalog_v3.rs).
In an isolated checkout of the selected candidate, change only
`CatalogApi::regional::update_item` from `effect idempotent;` to
`effect non_idempotent;`. This is an exercise variant, not a proposed change to
the provider's real contract. Record the expected endpoint set and all expected
classifications before exporting or comparing.

Run `cargo run -p concord-xtask --features examples -- contract export --target catalog --output BEFORE.json`
before the source edit; repeat with `AFTER.json` after the edit. Then run
`cargo run -p concord-xtask --features examples -- contract diff BEFORE.json AFTER.json --format json`.
Both documents must come from compiled declarations; manually editing a JSON
export does not perform this task. Keep exit statuses and actual output, and
restore/remove the isolated variant afterwards according to the session's
normal workspace policy. See the [contract tooling guide](../../docs/planning_and_observation.md#contract-tooling).

## Automated exercise follow-up

After answers are recorded, the public downstream
[v3_user_exercises.rs](../../crates/concord/tests/v3_user_exercises.rs) supplies
behavioral checks for the same distinctions. Run
`cargo test -p concord --features json,ndjson --test v3_user_exercises`.
Passing automated exercises is correctness evidence; it does not create human
participation, timing, confidence, or usability evidence.
