# Independent agent Rustdoc review — Concord V3

**Review date:** 2026-10-17. **Reviewer:** a fresh independent AI agent, not a
human participant or the implementation reviewer. This record supplies agent
evidence only; it does not populate the human-results form.

The reviewer read the task sheet before the packet README or results, recorded
predictions before verification, and used public Markdown, maintained example
declarations, rendered Rustdoc, and inert compiled contract exports. No
macro/core implementation source was consulted, including during follow-up.
The original candidate was
`570eec460dee1d83828ae48e35396ae07652aabc`.

Preliminary rendered pages came from the retained Linux qualification artifact
at `579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31`. Eighteen relevant public pages
were then checked against fresh local candidate Rustdoc; the old pages are not
presented as final-candidate build evidence. The separate parent-edit exercise
used an isolated checkout at `570eec4`, with exactly the
[retained one-line patch](evidence/rustdoc/catalog_scope_default.patch).

## First pass

All ten tasks are now complete as an agent exercise. The first pass found the
requested semantics through public material, except that task 7 had no
prepared before/after snapshot pair or resulting diff. That gap was recorded
before generating a fixture. The native follow-up below completed the
exercise, exposed an inspector attribution defect, and verified its correction
without changing the original inputs.

These are measured agent wall-clock seconds, including navigation and tool
work. They are not human usability measurements. Confidence was recorded from
1–5 before feedback. Full predictions, navigation paths, search terms,
corrections, and implementation-reading flags are in
[observations.json](evidence/rustdoc/observations.json).

| Task | First-pass result | Seconds | Confidence | Main public path |
|---|---|---:|---:|---|
| 1. Construction | Found; corrected an initial guess that the caller supplies the base URL | 65.8 | 2 | README → quick start → generated client → runtime/rate limits |
| 2. Value controls | Found and checked against generated builder/request methods | 100.3 | 5 | Quick-start control matrix → generated client → getting-started Catalog Rustdoc |
| 3. Response provenance | Found through the public declaration and inspection getters; endpoint card omitted the source | 73.9 | 3 | Modeling guide → `FindItemRequest` → `EndpointContract` → `ResultContract` → `EffectiveRepresentation` |
| 4. Directional inheritance | Found | 21.7 | 5 | Public DSL specification §9 → request bodies/responses |
| 5. Explicit fence | Found, including the nearer-descendant-default exception | 38.6 | 5 | Specification §9 → modeling guide → `EffectiveRepresentation` |
| 6. Parent-change prediction | Recorded a concrete hypothetical from the public specification; no matching prepared export pair existed | 45.7 | 5 | Specification §9 → contract-tooling guide |
| 7. Semantic diff | Incomplete in the initial handoff; completed by the native follow-up | 21.2 | 3 | Contract-tooling guide; no supplied task-specific diff |
| 8. Pagination | Found, including exact completion-limit behavior and summary distinctions | 26.7 | 4 | Pagination guide → generated terminals → `CollectAll`, `Pages`, `TraversalSummary` |
| 9. Streaming lifecycle | Found, including cancel-safe individual reads versus whole-handle cancellation | 38.4 | 4 | Streams guide → `ByteStream` and `ItemStream` Rustdoc |
| 10. Explicit metadata authority | Found after checking actual header access and the limiter's explicit origin exception | 65.5 | 3 | Planning/responses/rate limits → public plan, response, observer and limiter types |

## Recorded answers

1. **Construction.** The canonical builder selects the required tenant,
   bootstrap secret, and explicit limiter, plus trusted HTTP/runtime settings,
   before `build()`. The base belongs to the declaration. `new()` exists only
   when no required input or limiter choice remains. Build validation collects
   independent errors and performs no provider acquisition or network request.
   See [generated client](../../../docs/generated_client.md#building-a-client)
   and [runtime configuration](../../../docs/runtime.md).
2. **Controls.** A required root `tenant` has builder `set_tenant` and
   `set_tenant_if_some`; a required scope argument is positional and has no
   later controls. Optional `category` has payload set, exact
   `replace_category(Option<String>)`, conditional set, and omit. Initialized
   `offset` has set, conditional set, and retained reset, with no omit or exact
   optional replacement. Conditional `None` preserves the current selection;
   exact replacement with `None` removes it. Reset never reevaluates the
   initializer. See the [complete matrix](../../../docs/generated_client.md#value-controls).
3. **Provenance.** Canonical `regional::find_item` uses response `Json` from
   `CatalogApi`; its 404 branch is head-only absence. The public getter path is
   `description().contract()` → `result()` → `representation()` → `source()`.
   The endpoint card's `via Json` alone did not identify the owner. The
   [before snapshot](evidence/rustdoc/catalog_before.contract.json) also records
   the compiled source selection.
4. **Independent directions.** A body encoder and a response acquisition/decoder
   resolve independently. A local `via`, default, or fence in one direction
   does not select the other. Bodyless endpoints need no request
   representation; every result needs an effective response representation.
   See [specification §9](../../../docs/spec.md#9-representation-defaults).
5. **Fences.** `explicit` removes an inherited default in one direction. An
   endpoint needs a local `via` when no nearer descendant default supplies
   that direction. A deeper owner may establish a new default. The fence is
   recorded provenance and changes the effect of future parent edits; it does
   not mean raw, disabled, empty, or unbounded behavior. See
   [representation modeling](../../../docs/modeling_apis.md#representations).
6. **Parent prediction.** The original hypothetical selected the complete
   `RepresentationInheritanceApi` declaration in specification §9 and changed
   root response `ParentBuffered` to `Text`: the inherited endpoint changes,
   the overridden and fenced endpoints retain their selections, and request
   choices stay independent. This was a source-based prediction, not a
   claimed native diff. The executed fixture below uses the maintained Catalog
   target specified by the packet's preparation workflow.
7. **Diff interpretation.** Classify the report's caller, wire, authority,
   runtime, provenance, dependency, and other axes separately. An unchanged
   result type does not by itself prove identical behavior, and structural
   exports do not execute the provider. The actual native result below is
   classified rather than substituted with a hypothetical output. See
   [contract tooling](../../../docs/planning_and_observation.md#contract-tooling).
8. **Pagination.** `collect_prefix(u32)` returns a useful prefix with its
   summary; `collect_all(NonZeroU32)` returns a `Vec` only after controller-proven
   exhaustion; `pages(u32)` supports lazy processing. Continued traversal at
   the bound is `PageBound`/`NextRequestKnown`; complete collection fails with
   `CompletionLimitExceeded`. `Done` on the last permitted page is exhaustion
   without an extra fetch. Summaries distinguish received from delivered
   work, stop reason, continuation, budgets, attempts and dispatch knowledge.
   Zero prefix/pages performs no operation. See [pagination](../../../docs/pagination.md).
9. **Streams.** Successful acquisition reports `StreamOpened`, not EOF.
   Subsequent reading can complete, fail, or be cancelled. `finish()` stops
   without draining and preserves an existing terminal state; early handle
   drop records cancellation. A pending individual `next()` future can be
   dropped and resumed without resending or resetting idle accounting. One
   terminal error is followed by `None`, with the failure retained in the
   stream summary. Acquisition metadata stays a snapshot. See
   [streams](../../../docs/streams.md).
10. **Metadata authority.** Plan URL/public-header access is explicit caller
    authority over credential-sanitized data that may still contain sensitive
    public values. `ApiResponse::headers()` exposes the actual accepted
    headers. Automatic observation and wrapper `Debug` omit the full URL,
    exact path/query data, and arbitrary header names/values; observers expose
    target counts and typed response facts. A limiter gets no raw headers,
    body or credentials. Its textual `logical_origin` is present only when
    the plan explicitly grants the `origin` dimension; `cooldown_origin`
    remains opaque. See [planning and observation](../../../docs/planning_and_observation.md),
    [responses](../../../docs/responses.md), and [rate limits](../../../docs/rate_limits.md#admission).

## Native parent-edit follow-up

The [fixture](../../rustdoc_review/fixtures.md) inserts `response Json;` as the
first clause of canonical `CatalogApi::regional`. The reviewer recorded a new
prediction at confidence **4/5**, before exporting either snapshot or seeing
the diff: six inherited regional responses would change owner, the inline
Binary export would change its shadowed-default owner, and the root
`authenticate` endpoint record would remain equal.

The unchanged export, edit, changed export, and semantic diff each exited
**0**. Their native labels are `rustdoc-contract-before`,
`rustdoc-contract-edit`, `rustdoc-contract-after`, and
`rustdoc-contract-diff`. The exact commands, source-state digests, timestamps,
exit statuses and raw native-log hashes are in the
[evidence index](evidence/rustdoc/index.json). The measured follow-up took
191.6 seconds including review and recording.

The [original native diff](evidence/rustdoc/catalog_scope_default.original.diff.json)
reported `different`, **10 changes**, **Provenance only**, and an empty
`unresolved` list. Direct comparison of the public snapshots confirmed seven
changed regional endpoint records and an identical root `authenticate`
record. No caller, wire, authentication, runtime, rate-schema or capability
change was reported.

The report nevertheless listed all eight endpoint IDs in `changed_endpoints`:
the changed `Json` dependency-use-site row associated the unchanged root
endpoint too. The reviewer did not predict that broader association and
recorded the correction without redefining the original prediction. This
observation was handed to a separate inspection reviewer; the original output
is retained alongside the corrected output.

That reviewer confirmed an attribution defect and supplied a corrected CLI
from an isolated build target. This Rustdoc reviewer then independently
executed that exact binary through the native runner, recording its SHA-256
and the unchanged input hashes. `rustdoc-contract-diff-corrected` exited **0**.
The [corrected diff](evidence/rustdoc/catalog_scope_default.corrected.diff.json)
names exactly the seven regional endpoints, including the export's
shadowed-default provenance, and excludes the unchanged root endpoint. It
retains the same ten changes, `Provenance` axis, empty `unresolved` list, and
every per-field before/after value. Only the endpoint associations narrowed.
The targeted replay and recording took 27.6 measured seconds.

## Findings and correction review

| Finding | Severity | Disposition |
|---|---|---|
| No concrete paired fixture/diff was included in the first handoff | Major preparation gap | Fixed by the explicit fixture, reveal order, and native executed follow-up |
| Generated endpoint card omitted request/response representation provenance | Minor navigation friction | Fixed; independent rendered reread sees separate directional origins and endpoint `via` provenance |
| Observation overview described sanitized headers and omitted `ProviderAcquisitionFinished` | Minor documentation inconsistency | Fixed; reread sees value-free events and all six variants |
| `SanitizedHeaderMap` described its text as safe without its caller-data qualification | Minor wording friction | Fixed; current facade Rustdoc qualifies application-sensitive public data and explicit caller authority |
| Dependency-use-site diff associated an unchanged endpoint | Major semantic-diff accuracy finding | Fixed by the inspection reviewer; independent native replay verifies precise associations on unchanged inputs |

The final [rendered correction record](evidence/rustdoc/rendered_corrections.json)
retains the actual public text and page hashes. `FindItemRequest` now names
the client response source; `UpdateItemRequest` separately names request and
response sources; `ExportRequest` names its endpoint-level `via`. The first
post-fix header-map reread found the core page refreshed while its facade
re-export was stale. The subsequent `facade-rustdoc` build refreshed that
page, and the final reread observes the corrected wording. The native
`doc-audit-followup`, `redaction-rustdoc`, and `facade-rustdoc` builds all exited
0, with their source and log digests in the evidence index.

No unresolved Critical misunderstanding or release-relevant Major finding from
this agent exercise remains. The first-pass gap, initial predictions,
corrections, original defective report, and successful targeted rereviews all
remain in the record. The evidence covers public-documentation understanding
and inert structural inspection; the separate
[runtime review](runtime_review.md) records implementation and runtime tests.
