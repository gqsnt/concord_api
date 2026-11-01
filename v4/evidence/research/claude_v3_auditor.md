<!-- Verbatim final report of a Claude subagent (Opus 5.5), extracted from its transcript. -->

I audited V3 at `/home/canarit/projects/concord_api_v3ref` (HEAD `a492d3678e83`, worktree clean); nothing was modified. The four guides were read first.

Evidence labels used below:
- **[RUN]**: I ran the test and it passed. That covers concord-core lib `request::program`, `redaction::`, `value::tests` (22 passed), concord-macros `semantic::tests` (26), the `generated` binary's `shaping::` (37, `--all-features`), `v3_grammar` (2) and the selected `v3_user_exercises` (4). All ran offline with `CARGO_TARGET_DIR` under `/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/auditor/target`.
- **[TEST]**: the test exists and I read it, but did not run it.
- **[SRC]**: read from source only.
- **[INF]**: my inference.

Paths are relative to the repo root: `core/` = `crates/concord-core/src`, `mac/` = `crates/concord-macros/src`, `tests/` = `crates/concord/tests`.

## 1. Ledger (keep / rethink / drop)

| Area | V3 behavior | Evidence | V4 | Reason | Cost |
|---|---|---|---|---|---|
| Value: `x: T` | Required. Root values go through a builder (`set_x`, `set_x_if_some`), and all missing ones are collected into one `ClientBuildError`. Scope/endpoint values are positional constructor args. | `mac/semantic/names.rs:188-226` (`controls_for`); [RUN] `v3_grammar.rs::builder_accumulates_every_missing_input` | keep | V01 | low |
| Value: `x?: T` | Outer `Option<Arc<T>>`, absent by default. Controls: set, replace(Option), set_if_some, omit; no reset. | `names.rs:210-216`; `mac/emit/owner.rs:132-155` | keep | V04 | low |
| Value: `x: T = V` / `#[default]` | The initializer runs once into a retained `Arc<T>`. `reset_x` clones that Arc and never re-runs it. Required-initialized values have no `replace`. | `emit/owner.rs:97-111,237-249`; [RUN] `shaping.rs::trait_default_evaluation_preserves_owner_and_reset_timing` | keep | V02, V03, V08 | low |
| `= V` grammar | Only literals, negative numerics or const paths. Calls, blocks and macros are rejected. `#[default]` together with `= V` is an error. | `mac/syntax/values.rs:538-596, 514-523`; UI `tests/ui/fail/syntax/default_expression_restrictions` | rethink | V03 assumes `= expression()`. V3 only allows side effects through `Default` [SRC]. | low |
| Evaluation point | Root: when the builder is created. Scope: on selection. Endpoint: when the request is constructed. Clone, bind, plan and reset never re-evaluate. | `docs/spec.md:271-277`; [RUN] same shaping test (counter assertions) | keep | V03, V09 | low |
| `x?: Option<T>` | Two layers. `set_x(None)` means present-inner-None, `replace_x(None)` means absent, `set_x_if_some(None)` means no change. | `docs/spec.md:298-316`; [RUN] `v3_user_exercises.rs::nested_option_controls_preserve_inner_none_and_the_retained_initializer` | keep semantics; rethink method count (5 per value) | V05–V07 | medium |
| `Option<T>` wire | No `RequestAtom` impl exists for `Option`, floats or containers. An inner `None` needs an explicit `via` adapter (the test uses one that prints `null`). | `core/atom.rs:286-358` (str, String, char, bool, ints, NonZero only) | keep | No inference from type spellings | low |
| IntoValue | Sealed. Only identity, `&str→String` and `&String→String`. | `core/value.rs:14-25,71-92`; [RUN] `value::tests::closed_conversions` | keep (or plain exact `T`) | V12 | low |
| Snapshots | Setters consume `self` and return a new snapshot. Sibling clones stay isolated. Body-bearing requests are not `Clone`. | [RUN] `shaping.rs::consuming_request_setters_keep_sibling_snapshots_isolated`, `facade_setters_leave_the_source_scope_snapshot_unchanged` | keep isolation; rethink "body ⇒ !Clone" (exists for replay/retry, spec §6.5) | V09, V10 | low |
| Name resolution | A bare name binds only the current owner. Ancestors must be named as `client.x` or `<scope>.x`. Diagnostics suggest the qualified form but never choose it. Redundant self-qualification is a C3-MIGRATE error. Shadowing across owners is allowed. | `mac/semantic/mod.rs:117-208`; [RUN] `semantic::tests::bare_names_bind_only_the_current_owner`, `redundant_qualification_of_the_current_owner_is_a_migration_error` | keep | C05 | low |
| Duplicate singletons | Each clause family once per owner, never last-writer-wins. Preamble must precede children. | `mac/syntax/declaration.rs:104-180, 671-771`; UI `names/duplicate_clause` | keep | L01 | low |
| Query/header program | Per owner: (1) fence `replace`/`off` clears, (2) direct placements in declaration order, (3) block statements in order. Ops are Introduce, Replace, WhenPresent, Append, Remove and Clear. Survivors are selected symbolically, then encoded in final order. | `mac/semantic/head.rs:458-551`; `core/request/program.rs:331-366, 387-421` | keep algorithm, simplify | C06–C08 | medium |
| Compile-time preconditions | Introducing a key that already has a potential writer is an error naming both writers. Replace, when_present and remove need an earlier writer. `when_present` needs an outer-optional source. Append has no precondition. | `semantic/head.rs:612-720`; [RUN] `introduction_conflicts_name_both_writers`, `replace_when_present_and_remove_require_an_earlier_writer` | keep | C07 | medium |
| Suppressed values not encoded | The encoder of a removed or replaced source never runs. | [RUN] `program.rs::tests::suppressed_emissions_are_never_encoded` | keep | C08 | low |
| Repeated keys | Only via `each` or `append`. Two plain introductions of the same key is a compile error. | [RUN] `program.rs::exact_replace_none_clears_and_append_keeps_existing`; `v3_user_exercises::append_each_preserves_the_declared_vector...` | keep | C06 | low |
| Header identity | Case-insensitive at compile time and runtime. Query keys are case-sensitive. 12 singleton headers cannot use `append`/`each`. | `program.rs:230-242`; `mac/syntax/diagnostics.rs:256-269`; [RUN] `header_identity_is_case_insensitive_and_query_is_not`, `header_names_compare_case_insensitively_and_singletons_cannot_repeat` | keep | C09 | low |
| Reserved public headers | Authorization, Cookie, Accept, Content-Type and 14 others are rejected as public targets. | `diagnostics.rs:233-252`; `core/request/head.rs:1516-1534`; UI `syntax/spec_rejections.stderr:109` | **drop the auth-driven part** (Authorization, Proxy-Authorization, Cookie). Rethink Accept/Content-Type. | Directly fails C10 because of the removed auth subsystem | low |
| Path templates | Relative only. A literal `/` separates segments outside holes. Errors on empty template, leading or trailing `/`, `//`, a literal-only `.`/`..`, `?`, `#`, `\`. `{{` escapes. `replace` and `off` drop inherited segments. | `mac/syntax/request.rs:79-111, 132-294`; UI `syntax/path_template_errors` | keep | C01, C03 | low |
| Dynamic path segment | Encoded once by `url` path-segment push. A value containing `/` or `\`, or equal to empty, `.` or `..`, is **rejected** at preparation (InvalidRoute), not encoded. `%2F` becomes `%252F`. | `head.rs:688-716, 1509-1514, 1447-1454`; [RUN] `shaping.rs::relative_templates_encode_each_segment_exactly_once` | rethink | C02 says `a/b` should be encoded, not rejected | low |
| Base URL | Must be http(s) with a host and no userinfo, query or fragment. `/root` and `/root/` both give the prefix `/root/`. | `diagnostics.rs:128-154`; `head.rs:1447-1454`; [RUN] `shaping.rs::slashless_base_normalization_is_visible_in_the_static_explanation` | keep | C13 | low |
| DNS labels | Prepend, replace or off. Static literals must be lowercase. Dynamic labels: 1–63 ASCII alphanumeric or `-`, no leading or trailing hyphen; uppercase is accepted and lowercased by `url`. Host ≤253. IP bases reject labels. | `request.rs:24-77`; `diagnostics.rs:211-231`; `head.rs:593-626, 1493-1502`; [RUN] `shaping.rs::dns_labels_are_single_validated_ascii_labels` | keep | C04 | low |
| Query encoding | `url::form_urlencoded` via `append_pair` (space becomes `+`). | `head.rs:797-815`; [RUN] same path test (`x&y=z+%` → `x%26y%3Dz%2B%25`) | keep, document | C02 | low |
| Sensitivity | `#[sensitive]` value: sent on the wire, rendered as `<redacted>` in the safe URL and headers. Sensitivity sticks to a key across replace, remove and fence. A sensitive host label becomes `x` and sets `host_withheld`. | `program.rs:372-379`; `head.rs:554-567, 660-670, 797-815`; [RUN] `sensitivity_is_sticky_across_replacement_and_fences` | keep | S01–S04 | low |
| Name-based redaction | Substring match on 15 categories, case-insensitive, `_` treated as `-` (headers and query keys). | `core/redaction/names.rs:1-49`; `redaction/url.rs:60-80`; [RUN] `redaction_matrix_*` | rethink (overbroad: `sessionless-mode` is redacted by test; [INF] `page_token` would be too) | S safety net | low |
| Response headers in errors | Only names are kept, values are always redacted. Status body is opaque and decoded explicitly. | `redaction/headers.rs:147-160`; `core/error/status.rs:51-160` | keep principle | R05 | medium |
| Body repr | `RequestRepresentation` produces `PreparedBody` (bytes or stream, replay semantics). Content-Type comes from the representation. | `core/body/spec.rs:287-445`; `crates/concord-extras/src/codec/json.rs:36-46` | rethink → reqwest body | R01 | medium |
| Response repr | Has a Driver (Buffered, Streaming, Discard), `SUPPORTS_ABSENCE` and `absent_output`. | `core/response/spec.rs:292-330, 398-557` | rethink → plain decode | R03 | medium |
| Content-type policy | Json: content type must be present and json or `+json`, otherwise InvalidMediaType. Binary: missing is OK, a mismatch is an error. Text: anything accepted. | `json.rs:56-70`; `spec.rs:538-557`; `codec/mod.rs:189-191, 211-222` | rethink (three different policies) | R03 | low |
| Empty / Discard | Empty buffers the body and errors if it is nonempty. Discard drops it unread. | `spec.rs:486-518` | keep Empty; drop Discard | R04 | low |
| Status | Default accepted set is 200–299. `status` replaces it. Other statuses give `ErrorCategory::HttpStatus`. | `docs/spec.md:667-670`; `declaration.rs:772-866` | keep, minimal | R05 | low |
| `absent on 404` | Explicit only. A plain `-> Option<T>` gets no status shortcut. The body is dropped unread. | `docs/spec.md:675-682`; [RUN] `shaping::bodies::head_only_absence_never_polls_the_missing_body` | keep explicit-only rule; rethink the mechanism | R06 | medium |
| Methods | GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS, `METHOD("X")`. CONNECT and TRACE rejected. | `declaration.rs:895-945` | keep | L01 | low |
| `effect`, `id`, `timeout`, `expose accepted_bytes`, `request`/`response` defaults and `explicit` fence | Effect exists for retry gating. `id` is for rate keys and operations. Timeout is a deadline policy. | `declaration.rs:550-653`; `docs/spec.md:631-684, 737-754, 821-829` | **drop** effect, id, timeout, expose. Rethink representation defaults (keep only if cheap). | Excluded subsystems | — |

## 2. Contamination (drop; do not port)

| V3 artifact | Coupling | Evidence |
|---|---|---|
| `OwnerProjection` | `push_auth`, `push_rate_limits`, `owner_timeout`, `core() -> &ClientCore` | `core/request/owner.rs:32-69` |
| `HeadBuilder` / `PreparedHead` | Carry AuthPlan, RateLimitPlan, cooldown origin, timeout, cancellation, call caps and a budget. `new()` takes `RequestOptions` and a runtime-endpoint timeout. | `head.rs:340-401, 1147-1200` |
| `prepare_operation` | Allocates `OperationId` and `OperationDeadline` before building the head. The head is built only on first poll. | `core/request/prepare.rs:56-183` (id at 71-74); `pending.rs:14` |
| `ClientCore` | `credential_context` + `ManagedHttp` + `RuntimeConfig`. Concord builds its own reqwest client (redirect none, `no_proxy`, its own `HttpConfigBuilder`). No external `reqwest::Client` or native Request is exposed [SRC: grep]. | `core/client.rs:21-34`; `core/http/build.rs:155-206` |
| `ApiError` categories | AuthenticationUnavailable/Provider/Rejected, RateLimit, Pagination, SendCapExhausted. FailureStage includes CredentialPreflight and RateAdmission. | `core/error/category.rs:4-84` |
| Reserved generated method names | `timeout`, `reset_timeout`, `cancel_with`, `with_options`, `control_summary`, `pages`, `collect_*`, `retry`, `configure_runtime`, `credentials`, `bind`, `plan_runtime`, `contract`, `accepted_bytes` | `mac/semantic/names.rs:277-315` |
| Liveness | A value counts as "used" through a rate dimension or pagination mapping | `docs/spec.md:334-340` |
| Auth DSL | `credentials`/`auth` blocks; auth targets collide with public headers | `docs/spec.md:686-735`; UI `authentication/header_ownership.stderr` |
| Rate, pagination, retry DSL | `rate_limits`, `rate_limit`, `paginate`; `retry` is already retired | `docs/spec.md:756-864`; `keyword.rs:40-66` |
| Header sensitivity storage | Per-value header sensitivity is held in `pagination_sensitive_headers` | `head.rs:1031-1033, 1057` |

**Concrete interactions** (all [SRC]/[TEST] unless marked):

1. **Auth vs public headers.** The public Authorization header is forbidden because auth owns it (C10 fails).
2. **Value names vs executor controls.** An endpoint value `timeout: u32 = 5` is rejected because `reset_timeout` already exists (UI `names/request_control_names_reserved.stderr`).
3. **Absence vs driver, pagination and `accepted_bytes`.** These are mutually exclusive ([RUN] `absence_is_disjoint_and_exclusive_with_pagination_and_accepted_bytes`).
4. **Body vs Clone.** Body-bearing requests are not `Clone` because of replay and retry authority (spec §6.5).
5. **Effect vs method vs retry.** `effect` defaults feed retry gating (`v3_user_exercises::effect_gates_observed_retries...`).
6. **Pagination vs sensitivity.** `push_pagination_value` redacts whole path segments (`head.rs:675-682`).
7. **Rate keys vs optionality and liveness.** Rate keys require required, non-page-live values (spec §13).
8. **Accept/Content-Type vs representation.** These are private to the representation and cannot be set publicly (`head.rs:1226-1300`).

**High-risk dependencies** (traps where a refactor would silently keep the V3 executor):

1. Reusing `OwnerProjection` or `HeadBuilder` brings ClientCore, AuthPlan, RateLimitPlan and deadlines with it.
2. Reusing `ResponseRepresentation`, Driver or `PreparedBody` brings lease, stream and replay machinery.
3. Reusing the `ApiError`/`FailureStage` taxonomy, or `prepare_operation`'s lazy first-poll preparation, brings OperationId and deadlines.

`PatchOp` also accepts runtime keys, which exist only for runtime endpoints [INF].

## 3. Grammar table

| Construct | Example (file:line) | Meaning | V4 |
|---|---|---|---|
| `client N { … }` | `docs/spec.md:42`; `declaration.rs:79-205` | Root | in |
| `base "…";` (required, once) | `declaration.rs:104-117, 182-191` | Fixed origin and prefix | in |
| `values { … }` (client) | `docs/spec.md:242-252` | Root values | in |
| `scope s(params) { … }` | `docs/spec.md:44-45` | Nested owner | in |
| `GET e(params) -> T (via R)? ;\|{…}` | `docs/spec.md:46-48` | Endpoint | in |
| `METHOD("X")` | `declaration.rs:895-925` | Custom method | in |
| `x: T`, `x?: T`, `= stable`, `#[default]` | `values.rs:452-596` | Presence and initializer | in |
| `query`/`header("K")` placement, `each`, `via A` | `docs/spec.md:63-66`; `values.rs:377-450` | Direct placement | in (rethink `via`) |
| `#[sensitive]`, `#[deprecated]`, docs | `values.rs:88-112` | Metadata | in |
| Refs `x`, `client.x`, `scope.x` | `request.rs:489-561` | Ownership | in |
| `path "a/{x}/{y via A}"`, `replace`, `off`, `{{` | `request.rs:79-294` | Route | in |
| `subdomain [a, "lit"]`, `replace`, `off` | `request.rs:24-77` | DNS labels | in |
| `concat [a, b]` (depth ≤64) | `request.rs:610-644` | Scalar concatenation | rethink |
| `query`/`headers { "k" = e; }`, `replace {}`, `off;` | `request.rs:369-413` | Program | in |
| Statements `replace`, `when_present`, `append`, `remove` | `request.rs:415-467` | Patches | in (consider dropping `when_present`) |
| `body b: T via R`, `body b?:` | `docs/spec.md:67` | Body | in, minimal |
| `request R;`, `response R;`, `explicit` | `declaration.rs:731-768` | Representation defaults | rethink |
| `status …;`, `absent on …;` | `declaration.rs:577-631` | Status | in, minimal |
| `effect`, `id`, `timeout`, `expose accepted_bytes` | `declaration.rs:550-653`; `request.rs:664-742` | Executor | out |
| `credentials{secret/credential}`, `auth{…}` | `docs/spec.md:101-118` | Auth | out |
| `rate_limits{windows/policy}`, `rate_limit …` | `docs/spec.md:120-139` | Rate | out |
| `paginate via … {init; cursor ->}` | `docs/spec.md:141-146` | Pagination | out |
| Retired forms (`vars`, `host`, `+=`, `fmt`, `= default`, `retry`, `#[redact]`) | `docs/spec.md:894-909` | Migration errors | out |

## 4. Generated surface (from `docs/generated_client.md:96-136, 177-184, 322-350`)

**Kept (neutral):**
- Client type, `builder()`, `new()` (only when nothing is required), `build() -> Result<_, ClientBuildError>`.
- Root value controls.
- Scope navigation methods returning owned handles.
- Endpoint constructors taking positional required args and returning `XRequest`.
- `.await`.
- `plan()` (sanitized URL and headers).
- Value controls: set, replace, set_if_some, omit, reset.

**Exist only for excluded subsystems or contract export:**
- Secret and provider setters, `credentials()`, `store_as_x()`.
- `configure_http` (a parallel reqwest config surface) and `configure_runtime` (limiter, page caps).
- `retry(policy)`.
- `timeout`/`reset_timeout`, `cancel_with`, `with_options`/`CallOptions`, `control_summary`, `RequestControls`.
- `pages`, `collect_prefix`, `collect_all`.
- `accepted_bytes`.
- `bind`/`plan_runtime` (runtime endpoints).
- `unbound()`, `XEndpoint` intents and `client.request(intent)` (cross-client binding).
- `contract()`/`description()`.
- Re-exports of `DispatchKnowledge`, `OperationId`, `ExecutionMeta`.

## 5. Oracle tests (guide-05 IDs)

| # | ID | Test |
|---|---|---|
| 1 | V01 | `tests/v3_grammar.rs::builder_accumulates_every_missing_input` [RUN] |
| 2 | V02/V08 | `tests/generated/shaping.rs::defaulted_option_client_omit_and_reset_are_distinct_override_states` [RUN] |
| 3 | V03/V09 | `shaping.rs::trait_default_evaluation_preserves_owner_and_reset_timing` [RUN] |
| 4 | V05–V07 | `tests/v3_user_exercises.rs::nested_option_controls_preserve_inner_none_and_the_retained_initializer` [RUN] |
| 5 | V06 | `tests/generated/current_language.rs::nested_optional_slot_operations_follow_the_exact_transitions` [TEST] |
| 6 | V10 | `shaping.rs::consuming_request_setters_keep_sibling_snapshots_isolated` [RUN] |
| 7 | V10/C12 | `shaping.rs::facade_setters_leave_the_source_scope_snapshot_unchanged`, `independently_built_clients_retain_independent_value_snapshots` [RUN] |
| 8 | V11 | `mac/semantic/tests.rs::generated_name_collisions_are_rejected` [RUN] |
| 9 | V12 | `shaping.rs::closed_conversions_accept_borrowed_strings_integers_and_exact_none` [RUN]; `core/value.rs::tests::closed_conversions` [RUN] |
| 10 | C01 | `v3_grammar.rs::routes_hosts_and_programs_compose_as_declared` [RUN] |
| 11 | C02/C03 | `shaping.rs::relative_templates_encode_each_segment_exactly_once` [RUN] |
| 12 | C04 | `shaping.rs::dns_labels_are_single_validated_ascii_labels` [RUN] |
| 13 | C05 | `semantic/tests.rs::bare_names_bind_only_the_current_owner` [RUN] |
| 14 | C06 | `v3_user_exercises.rs::append_each_preserves_the_declared_vector_and_adds_exact_wire_occurrences` [RUN] |
| 15 | C07 | `semantic/tests.rs::introduction_conflicts_name_both_writers` [RUN] |
| 16 | C08 | `core/request/program.rs::tests::when_present_none_preserves_order_some_replaces_at_new_position`, `suppressed_emissions_are_never_encoded` [RUN] |
| 17 | C08/C11 | `v3_user_exercises.rs::patches_distinguish_absence_empty_sequences_and_family_resets` [RUN] |
| 18 | C09 | `program.rs::header_identity_is_case_insensitive_and_query_is_not` [RUN] |
| 19 | C13 | `shaping.rs::slashless_base_normalization_is_visible_in_the_static_explanation` [RUN] |
| 20 | S01/S02 | `tests/runtime/runtime_endpoint.rs::runtime_sensitive_values_reach_the_request_but_not_the_plan` [TEST; runtime-endpoint mechanism, semantics only] |
| 21 | S04 | `program.rs::sensitivity_is_sticky_across_replacement_and_fences` [RUN] |
| 22 | S (names) | `core/redaction/tests/redaction_matrix.rs::redaction_matrix_retains_useful_public_near_misses` [RUN] |
| 23 | R05/R06 | `tests/generated/shaping/bodies.rs::head_only_absence_never_polls_the_missing_body` [RUN] |
| 24 | R01/R07 | `v3_user_exercises.rs::representations_are_independent_even_when_every_rust_payload_is_string` [RUN] |
| 25 | L01 | UI `tests/ui/fail/names/duplicate_clause`, `syntax/path_template_errors`, `syntax/patch_program_conflicts` [TEST] |

**No V3 oracle exists** for M02–M05 or C10. V3 has no external client or native request. M05 is executor-entangled, and V3 rejects Authorization for C10.

## 6. Contradictions and surprises

1. **Two reserved-header lists.** The macro has 18 entries (`diagnostics.rs:233-252`). The runtime has 14 (`head.rs:1516-1534`) and is missing `proxy-authenticate`, `accept-encoding`, `content-encoding` and `proxy-connection` [SRC]. Only runtime endpoints would hit the runtime list [INF].
2. **Label case asymmetry.** Static subdomain literals must be lowercase (`diagnostics.rs:211-231`), but dynamic `"EU"` is silently lowercased [RUN DNS test].
3. **`path off` adds a trailing slash.** It gives `/root/` even though templates forbid trailing slashes ([RUN] path test). A route of exactly `/root` cannot be expressed.
4. **Dynamic `/` is rejected, `%2F` is double-encoded.** `a/b` is rejected rather than encoded; `%2F` becomes `%252F`. This conflicts with C02's intent [RUN].
5. **Three content-type policies.** For a missing Content-Type, Json fails, Binary passes and Text accepts anything [SRC].
6. **Doc vs test on `timeout`.** The doc says a local `timeout` parameter "becomes `set_timeout`" (`generated_client.md:29-30`), but an initialized `timeout` is a compile error ([TEST] UI reserved).
7. **Present-empty `each` under `when_present`.** It deletes the parent occurrences and emits nothing ([RUN] `present_empty_each_when_present_clears_parent`). This is documented but surprising.
8. **Substring name redaction is overbroad.** `authorizationless` and `sessionless-mode` are redacted ([RUN]). [INF] `page_token` and `next_token` cursors would be too.
9. **Query `+` vs path `%20`.** Query uses form encoding, path uses percent encoding. This is unlabeled in the docs [SRC].
10. **`remove_header_values` clears the per-value marker.** It drops the header from the per-value sensitive set (`head.rs:1057`). This is safe only because the program applies stickiness separately [SRC/INF].
11. **`plan()` and `.await` prepare separately.** `.await` builds the head lazily at first poll and `plan()` builds it independently. [INF] Equivalence rests on the parity tests, not on one function.

## 7. Unresolved / could not verify

- **Json on a 204/205.** [INF] It fails as InvalidMediaType because the Content-Type is missing, not as "empty". No test was found.
- **Plain `-> Option<T>` with 404 and no `absent on`.** That it yields an HttpStatus error is stated in the spec only; I found no runtime test.
- **Present empty string → `k=`.** [INF] `append_pair` behavior. There is no V3 test for V04 specifically.
- **Subdomain prepend order across nested owners.** `prefixed_host` appends labels as pushed (`head.rs:1423-1445`). The generated push order (`owner.rs` emitter) was not traced. The grammar test shows `z9.eu.` [RUN], so this is consistent in practice.
- **Header name casing on the wire.** [INF] `http::HeaderName` lowercases.
- **Full test suite.** Not run: no UI/trybuild run, no Windows, no `--no-default-features`.
- **Native reqwest access.** The absence of any native Request/Response escape hatch comes from a grep-level search only.
- **Non-ASCII path values.** Exact encode set is assumed to be `url` crate defaults, not observed.
