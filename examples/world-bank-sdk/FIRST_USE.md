# Independent first use: World Bank indicators

Consumer run: 2026-11-06. Requested runner: Codex gpt-6-astra, xhigh.
The session does not expose a verifiable model/effort identifier to the agent;
the requested settings are not asserted as observed settings.
Starting HEAD: `9b7f4e56`; frozen Concord baseline: `47a25fbf`.

## Predictions recorded before writing either outcome

### A — one country, one indicator, one year

First entry point: `Client::new(http).with_base_url(loopback_v2)?`, then
`client.year("br", "SP.POP.TOTL", 2006).await`.
Expect a single GET to `/v2/country/br/indicator/SP.POP.TOTL` with exactly
`date=2006`, `page=1`, `per_page=50`, `format=json`, JSON Accept, no auth and
no request body. Query order is immaterial. `year` returns `IndicatorPage`
with provider metadata, observations and the HTTP head/operation. A numeric
zero is `Some(0.0)`, null is `None`, and null data is an empty vector. Missing
`value` is a decode error. This method returns only the first page, even when
metadata announces more; it does not claim complete traversal.

Expect typed `Error::Provider` for the one-element message envelope even at
HTTP 200, `Error::Http` for a non-JSON error response, and `Error::Concord`
for invalid successful JSON, wrong media type or an interrupted body.
Received HTTP metadata must survive failures. A connection closed before
the head has no response metadata. No automatic retry is expected.

### B — inclusive time range across pages

First entry point: `client.range("br", "SP.POP.TOTL", 2004, 2006,
NonZeroU32::new(2).unwrap(), Resume::Start)?`.
Construction is lazy. Expect the same GET path with `date=2004:2006`,
`format=json`, `per_page=2` and numbered `page=1`, then `page=2`.
`next_page` returns `Page` values with items, next number and `TraversalMeta`;
only the last indicated page permits `Ok(None)`. `items().collect_complete()`
returns all observations in provider order with null values intact. An empty
intermediate page must not terminate traversal.

A failed second page must keep the first page/items in `Incomplete`, carry
the typed SDK error, and identify `Resume::At(2)` for the same query/page size.
A cap before the last page gives `PagesError::Bounded`; a cap exactly on the
last page is complete. Calls after a cap/failure give `Ended`, never success.
A truncated HTTP body must not expose part of its JSON page or hide the
failure behind clean item exhaustion. A resumed traversal starts directly at
the indicated page. Invalid/mismatched page metadata must fail. A reversed
range fails locally. Dropping an unpolled traversal sends nothing.

## Ordered trace

1. Confirmed a clean worktree and the baseline/author HEAD identities. Read
   the applicable ancestor `AGENTS.md` and its general handoff/authority
   documents. The narrower held-out consumer instruction governs this run.
   No campaign evidence under `v4/sdk20/` or any SDK friction/role reports
   was read.
2. Read the SDK README, package manifest and `.gitattributes`; then Concord's
   root README, caller guide and pagination README. Generated public SDK
   rustdoc with `cargo doc -p concord-sdk-world-bank --no-deps --locked
   --offline`, and facade rustdoc with `cargo doc -p concord --no-deps
   --features csv,jsonl --locked --offline`. All cargo commands use the
   required five-job/shared-target environment prefix.
3. Read generated rustdoc for the client alias/`WorldBank`, operation options,
   observations, metadata, wire envelope and typed errors. Generated/read
   pagination and testserver public rustdoc to learn the test harness and
   exact completion/error fields. No SDK or testserver source/tests were
   needed. No other provider SDK supplied the solution.
4. Opened the two official documentation pages below, without following live
   API example links. The basic-call page specifies year/range, JSON, page
   and page-size queries. The indicator-query page includes a metadata/data
   array for the indicator catalog, not a country observation capture.
5. Documentation extraction initially tried Python `bs4`, which is absent
   (`ModuleNotFoundError`). Switched to the standard-library HTML parser;
   no package installation or SDK intervention. Both documentation builds
   succeeded without Rust compiler diagnostics. Predictions above were
   recorded before creating the empty `tests/first_use.rs`.
6. Created five independent response fixtures and their provenance note in
   `fixtures/first-use/`, then implemented both outcomes in the empty test
   file. Used only the SDK's public client/types, its pagination re-export,
   facade response/transport types, and testserver public APIs. The test
   transport has a three-second timeout, disables proxies for loopback and
   refuses redirects. No SDK source or existing tests were opened.
7. Formatted only the new test file and ran the exact requested test command
   with the required environment prefix. First compilation succeeded with
   no warnings or compiler diagnostics; all 25 tests passed. There were no
   failed assertions, corrective test edits, SDK interventions or native
   execution workarounds.
8. Checked formatting, whitespace and file scope. The worktree contains only
   the new consumer report, test suite and consumer fixture directory. The
   frozen `crates/`, `scripts/`, `docs/` and `checks/` still match `47a25fbf`.
   `git check-attr text` reports `unset` for the new JSON/truncated fixtures,
   confirming the existing byte-preservation rule applies.

## Fixture provenance

- [World Bank API basic call structures](https://datahelpdesk.worldbank.org/knowledgebase/articles/898581-api-basic-call-structures)
- [World Bank indicator API queries](https://datahelpdesk.worldbank.org/knowledgebase/articles/898599-indicator-api-queries)
- The SDK README and public rustdoc supply the observation, null, envelope,
  and error contracts used by this consumer.

All consumer response bytes are hand-authored synthetic examples or
deliberately malformed mutations of those examples. They are not downloaded
provider responses, real population estimates, or vendor conformance evidence.
Existing SDK fixtures were not read or reused. Byte-sensitive consumer
fixtures live in `fixtures/first-use/`, covered by the existing
`fixtures/** -text` attribute. All requests go to a loopback testserver;
there are no credentials and no provider API calls.

## Results and friction

**25 tests passed, 0 failed, 0 ignored**, on the first compilation/run:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-world-bank --test first_use
```

Outcome A works through `Client::year`; outcome B works through
`Client::range` followed by `next_page`, `collect_complete`, or
`items().collect_complete`. Wire checks cover method, loopback host, base
path, complete decoded query (including duplicate detection), Accept, absent
auth/cookies, empty request body and exact page/request counts. A separate
request checks path-segment encoding and a base URL with a path/trailing slash.

The suite covers observation identity, null versus zero versus missing value,
null data, provider/HTTP metadata and operation preservation, first-page-only
`year`, same-year ranges, multi-page exhaustion, empty intermediate pages,
failed/bounded prefixes, explicit resume, exact-final-page caps, sticky error
states, malformed JSON, wrong media type, invalid page metadata, pre-head
transport failure, reversed ranges and lazy/dropped traversals. Interrupted
HTTP responses fail both with partial JSON and with complete JSON lacking the
terminating HTTP chunk; no observations from that page leak into the prefix.
The item consumer's `next` interface is exercised, not the separate
`futures_core::Stream` adapter. Tests do not assert snapshot consistency or
cross-page totals, which the README explicitly does not promise.

**First SDK intervention: none.** No SDK source/test reads, other-provider
source reads, private reach-through, native execution escape, handwritten
operation labels, library edit, manifest/dependency change or credential use.
Rustdoc was sufficient to discover the required public signatures. The Python
documentation-extraction fallback in trace entry 5 was tooling only.

No blocking Concord or SDK defect was observed. The concrete caller friction
was limited to these documented ergonomics:

- **Resume requires preserving the original query outside the checkpoint.**
  `incomplete.error.resume()` yields only a page position. To continue, the
  caller supplies country, indicator, both years and page size again. The
  test's small `range` helper centralizes these values. Reproduce with
  `range_failure_keeps_prefix_and_can_resume_the_same_query`: the observed
  sequence is pages `[1, 2, 2]`, and combining the prefix and resumed result
  produces the complete three-observation result. No library workaround is
  required, but a persisted page number alone is insufficient application state.
- **Inspecting a traversal failure requires two error layers.** The outer
  `PagesError::Provider` means a page fetch failed; the inner SDK `Error`
  distinguishes provider messages, HTTP failures and Concord failures. The
  consumer uses nested matches to recover status and typed messages.
  Reproduce with `range_provider_error_is_typed_and_cannot_turn_into_clean_eof`
  (HTTP 200 provider message) or `range_failure_keeps_prefix_and_can_resume_the_same_query`
  (HTTP 503). The fields preserve all needed information; the extra matching
  is caller code, not a correctness failure.

To run either reproduction alone, append its test name to the command above.
No numeric scores or claims of live provider verification are attached to this run.
