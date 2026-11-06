# SDK16 maintenance: pending, partitions and cancellation race

Date: 2026-11-06. Role: independent maintenance; this role did not author the SDK.
Requested runner: Codex gpt-6.1-sol (xhigh). Starting HEAD: `19ef84e3`;
Concord baseline: `0983a349`. No library, lockfile, dependency or other-example edits.

## Result and compatibility

The existing SDK already handled extra 202 observations, arbitrary validated
partition counts and separate cancellation acknowledgement/observation outcomes.
The new wire test combines these perturbations: a pending poll exhausts one attempt,
completion arrives while the cancel response is held, then cancellation observation
sees another pending response before completion with two result partitions. Both a
200 acknowledgement and a typed 422 rejection preserve their own result and produce
the actually observed `PollOutcome::Complete`. Partition zero is reused; exactly one
later partition is fetched. Handle and original submission context remain B throughout.
Existing tests still cover observed terminal cancellation and unresolved cancellation.

The common contract exposed an additional defect: the SDK kept `statementStatusUrl`
and Link headers as evidence but discarded their targets when making requests. Status
and known partition links now use public `Prepared::follow`, keeping opaque query
values, repeated keys, percent-escape spelling, plus signs and server request IDs.
Relative references resolve against the actual request URL. The account, statement
path, partition index and relation are checked before a link can be followed. Query
clones share the latest correlated status target; plan clones share known partition
targets, including updates returned by later partitions. Unknown partition targets
still use the provider's documented indexed operation; partition independence remains
based on the validated descriptor array.

Public workflow signatures and the first-use consumer are unchanged. The deliberate
behavior adjustment is that invalid/foreign/wrong-statement returned links cause a
typed `ProtocolError::InvalidLink` instead of being silently ignored. An old negative
test now expects this refusal, with no request to the foreign origin. The exact-wire
four-operation test also needed a fixture correction: changing a cancel body's handle
from B to A now changes its status URL to A as well. No existing assertion about method,
request body, headers, result rows or cancellation outcome was weakened.

## Edits by kind

| Kind | Edits |
|---|---|
| Declaration | None: all four operations and the status-dependent JSON decoder in `src/api.rs` remain unchanged. |
| Adapter | `src/client.rs` captures the actual request URL and remembers correlated status targets from pending/complete/cancel/error responses. `src/partitions.rs` follows known targets and retains later updates. New `src/links.rs` parses/validates links and keeps their query bytes; `src/lib.rs` registers the private module. All execution remains in Concord. Debug omits URL values. |
| Model | Adds `ProtocolError::InvalidLink` in `src/error.rs`; updates `ServerMessage::statement_status_url` rustdoc in `src/model.rs`. No response field, pending/result union or partition schema changes. |
| Tests | Adds eight public-API tests in `tests/maintenance.rs`; adjusts two cases in `tests/protocol.rs`. Existing `tests/first_use.rs`, `tests/workflows.rs` and shared test helpers are unchanged. |
| Fixtures | Adds three pending JSON documents, one two-partition complete JSON document and one Link header fixture under `fixtures/`. Existing `.gitattributes` still applies `fixtures/** -text`; existing fixture bytes are unchanged. |
| Docs | README describes returned targets, pending/cancel observations and compatibility. PROVENANCE records official-page rechecks and the synthetic fixture transformations. This file records maintenance evidence. |

The tokenizer in `src/links.rs` is adapted from **the existing GitHub SDK's
`examples/github-sdk/src/link.rs`**. Its handling of quoted parameters and commas
supplied the solution; Snowflake-specific account/handle/index checks, target
retention and `next`-before-`last` selection are maintained here. The copied tokenizer
is private, has one typed SDK refusal category, and refuses unsupported anchor
contexts for known relations. No other provider SDK implementation supplied code.

## Independent descriptions and locality

For the requested pending/partition/race behavior, **zero duplicated protocol-constant
edits** were needed: polling, metadata-driven fanout and `CancelObservation` were
already generic enough. `src/polling.rs` did not change.

For the single semantic correction **follow returned links rather than discard
them**, **seven pre-existing locations** needed coordinated edits:

1. `Query::status` request selection.
2. `PartitionPlan::fetch` request selection.
3. `ServerMessage::statement_status_url` field documentation.
4. `FirstResult::plan` documentation saying no server URL was followed.
5. README's stable-context claim that GETs omit `requestId`.
6. README's transport paragraph saying returned links are never followed.
7. The protocol test specifying that a foreign advisory URL was ignored in favor
   of a reconstructed status path.

This count covers existing descriptions/call sites, not every new assertion, fixture
or provenance paragraph. Status ingestion branches share `remember_status`; account
and statement-path validation is shared in `links::validate`. The generated declaration
remains the source of the statement path; no handwritten operation identity or second
transport was added. One separate fixture inconsistency (cancel handle A / URL B) also
needed correction, as described above.

The perturbation itself felt local to fixtures and tests. Fixing the common link
contract crossed both request adapters, model documentation and independently written
README/test claims, so the complete maintenance change was not confined to one place.

## Reading and observed interventions

- Read the SDK README, PROVENANCE, manifest, source modules, workflow/protocol tests,
  common helpers and first-use test code. Did not open `FRICTION.md` or `FIRST_USE.md`
  until after the first implementation and its maintenance test run.
- Read the workspace README, relevant response/SDK sections of Concord's author,
  caller and recipe guides, and core/extras/pagination crate READMEs. Generated
  facade rustdoc with the required `cargo doc -p concord --no-deps --features csv,jsonl`
  command and inspected `Prepared::follow`; also inspected its public implementation.
  The campaign ancestor AGENTS guidance and its three required root documents were read.
- Reopened Snowflake's official handling-responses, cancelling-requests and SQL API
  reference pages. Those sources and fixture assumptions are linked in PROVENANCE.
  No live Snowflake request, credential, secret or captured response was used.
- Baseline: 49 integration tests and 3 README doctests passed.
- New regression before implementation failed with the bare handle path on the wire
  where the returned URL contained `requestId` and opaque query values. Reproduction:
  run the main maintenance race test against starting HEAD's SDK sources.
- First implementation compiled. The next maintenance run failed only an operation-name
  assertion: actual `SqlApi.statement.cancel`, expected `SqlApiStatement.cancel`.
  Corrected the assertion to the observed generated identity. No generated type-name
  compile workaround or runtime operation-name string was needed.
- The first complete post-change run exposed the old handle-A/status-URL-B fixture
  inconsistency. Corrected both correlated fields. The foreign-URL negative case was
  intentionally updated for the common contract; ordinary workflows and all independent
  first-use tests pass unchanged.
- No private reach-through, native HTTP escape, rebuilt execution, implicit retry or
  Concord workaround was required. Link validation/tokenization is SDK adapter work.

## Verification

Every cargo invocation used
`CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b`.

- `cargo test --locked -p concord-sdk-snowflake`: **57 integration tests and 3
  doctests passed** (18 first-use, 8 maintenance, 18 protocol, 13 workflow).
- `cargo clippy --locked -p concord-sdk-snowflake --all-targets -- -D warnings`: passed.
- `cargo fmt -p concord-sdk-snowflake`: completed; `git diff --check`: clean.
- Frozen library paths match `0983a349`; all changed/new files are under this SDK.

New checks cover both cancel reply outcomes under a gate-controlled race, inserted
pending status, second-partition row delivery, mismatched pending/complete/error handles,
mismatched cancel acknowledgement, exact returned URL targets, absolute/query-relative
status URLs, typed provider-error preservation, cloned-plan target updates, malformed
and foreign links, and keyed later-partition failure with explicit missing work.
Fixture tests establish these offline behaviors, not live provider conformance.
