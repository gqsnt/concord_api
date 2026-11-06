# First-use consumer trace

## Predictions, recorded before writing the tests

### A — Austria, 2026
`holidays("AT", 2026).fetch()` should send one unauthenticated GET to `/api/v4/Holidays/AT/2026`, accepting JSON and carrying no body. The response should contain the decoded holidays and HTTP metadata. A complete `[]` is successful; malformed or interrupted JSON returns an error with no partial holidays. A non-2xx response returns an error and retains its response head; 400 is `InvalidRequest`, with the unread body available through Concord.

### B — Germany, 2027
`holidays("DE", 2027).fetch()` should send one GET to `/api/v4/Holidays/DE/2027`. It should return a complete vector that ordinary local filters can select by `HolidayType::Public` and `Bank` without another request. A later failed read returns an error and does not invalidate a previously owned successful response. There are no pages, cursors, streaming completion markers, or partial results: one decoded finite array completes each fetch.

## Ordered trace

1. First entry point: SDK README, particularly construction, workflows A/B, errors and completion; then generated SDK rustdoc for `NagerHolidays`, `Holiday`, `HolidayType`, response metadata and errors. Concord README explains JSON Accept/media-type behavior and error response access. Generated both SDK and Concord rustdoc using the requested shared target. The SDK README explicitly identifies the two application workflows.
2. Consulted the official [API page](https://nagerholidays.com/api) for v4 path and JSON model, and the [Nager.Date README](https://github.com/nager/Nager.Date/blob/main/README.md) for types. No provider endpoint was called. Read `examples/testserver/src/lib.rs` for its public loopback scripting API; this is test infrastructure, not SDK source. No existing provider SDK supplied the solution.
3. Workspace HEAD is `38dbaa2959ad3b50dde23684ff33d47142fd8a9f`; requested frozen `94d13527` is an ancestor, not HEAD. Tests use the SDK available in this worktree.

## Fixture provenance

The first-use fixtures are hand-authored JSON using the field names and types in the official API page. Austria values are adapted from its displayed JSON preview; Germany entries and classifications are synthetic test data, not calendar claims. Empty, malformed, interrupted and error bodies are synthetic. Existing SDK fixtures were not read or reused. `.gitattributes` already contains `fixtures/** -text`.
4. Wrote the test from an empty `tests/first_use.rs` after recording the predictions. First compilation and run: seven passed, with no compiler diagnostics. Added media-type and pre-head transport checks; the final requested command passed all nine tests. The tests use a loopback-only, proxy-free, redirect-free reqwest client and explicitly shut down the server. No SDK `src/` or existing SDK test was read; no provider SDK example supplied the solution.
5. Working path: `NagerHolidays::new(http).with_base_url(server.url())`, then `holidays(country, year).fetch().await`. Ordinary iterator filters select typed holiday kinds after decoding. The fake checks exact request method and target, JSON Accept, absence of auth and body, and request count. It tests success, empty completion, HTTP 400 and 503, malformed JSON, interrupted body, refused media type, pre-head failure, and retention of an earlier completed read.

## Concrete friction and interventions

- First intervention: for a synthetic HTTP 400 body, the SDK's typed error exposes classification, status and metadata, but reading the body requires the documented native response escape. Reproduction: return 400 with `text/plain` from the loopback fake; `fetch().await.unwrap_err().into_concord().into_response().unwrap().bytes().await` recovers the unread bytes. The official pages reviewed provide no error-document schema, so the test does not invent one or treat the body as a typed provider error. This is not needed for either successful outcome.
- The JSON decoder buffers the complete response, and a malformed or interrupted body yields no partial vector. Reproduction: serve `first-use-truncated.json` as JSON or end a chunked response without its terminating chunk; `fetch()` errors even if the chunk contained a complete JSON array. Applications needing partial progress cannot obtain it through this finite fetch. For these country/year outcomes, a successful earlier fetch remains available when a later fetch fails.
- Environment discrepancy: `git rev-parse HEAD` reports `38dbaa29`, not the stated `94d13527`; `git merge-base --is-ancestor 94d13527 HEAD` succeeds. Verification therefore describes the checked-out SDK at HEAD, not a checkout of the named commit.

## Between waves 2-3 (coordinator integration)

The record above is unchanged and still describes the `fetch()` path it used. The SDK now names
a send policy in its declaration (`INTEGRATION.md`, "Between waves 2-3"): the generated `send()`
returns `HolidayResponse`/`Error` and `fetch()` was removed, so the coordinator changed the ten
`.fetch()` sites of `tests/first_use.rs` to `.send()`. No case or expectation changed; the suite
still passes (9 tests).
