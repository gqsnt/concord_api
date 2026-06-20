# Testing

Concord uses several layers of tests because the project spans macro syntax, generated Rust, runtime behavior, and public docs.

## Macro tests

Trybuild pass/fail fixtures cover public DSL syntax and compile-time diagnostics. Parser diagnostics snapshots should be updated intentionally with:

```bash
TRYBUILD=overwrite cargo test -p concord_macros current_trybuild_fixtures_match_expected_results
```

Parser unit tests cover smaller syntax rules and span-sensitive diagnostics.

Sema unit tests cover name resolution, inheritance, policy merging, behavior expansion, and diagnostics that need semantic context.

Codegen tests should prefer generated API compile checks, type checks, trybuild fixtures, and focused generated-shape assertions that cannot be expressed through Rust type checking.

Macro strictness belongs primarily in semantic unit tests and trybuild pass/fail fixtures. Add trybuild fail fixtures when a rejected form needs a stable public diagnostic. Source-level keyword audits can be useful during review, but they should not be normal `cargo test` checks.

## Core tests

`concord_core` has runtime characterization tests for cache, concurrency, rate-limit, auth rejection, retry, stale fallback, decode, pagination, codecs, and runtime configuration.

These tests protect runtime order and should be extended before runtime behavior is refactored.

Auth/redaction tests must cover arbitrary auth names, not only conventional names such as `Authorization` or `api_key`. Basic auth usernames declared as `secret` are secret material too. When auth handling changes, verify that `BuiltRequest`, `BuiltResponse`, `DecodedResponse<T>`, debug sinks, errors, and cache keys do not contain raw auth material, while the materialized `TransportRequest` still carries real credentials at `Transport::send`.

Auth preparation boundary tests should verify behavior at the sealed auth boundary: raw material stays out of logical request/debug/error surfaces and reaches only `TransportRequest` at send time.

Runtime strictness tests should reject invented policy values and silent saturation through observable behavior. Rate-limit `[host]` keys must fail explicitly when the logical URL has no host. Cache TTL overflow belongs in macro semantic diagnostics, and request/auth attempt counters should return typed overflow errors instead of saturating.

Runtime lock/state tests should poison representative auth, cache, and rate-limit state where feasible and assert typed errors or explicit cache backend outcomes instead of panics.

Response body limit tests should cover `Content-Length` precheck, unknown-length/chunked enforcement, exactly-at-limit success, decode/cache bypass on oversized bodies, auth HTTP token response limits, and separation between endpoint response read limits and cache `max_body`.

## Examples and docs tests

`concord_examples` compile-checks public usage. It includes small examples, public docs fixtures, generated API usage tests, and the Riot fixture.

The Riot fixture is the large real-world surface test. Do not change Riot semantics for unrelated macro or runtime work.

Markdown prose is not validated by keyword tests in `cargo test`. Release review may include a manual source and documentation audit for stale wording, unsafe live calls, or validation-dependent codegen patterns, but that audit is not a substitute for behavior, compile, diagnostic, and runtime tests.

## New DSL feature checklist

1. Add or update parser AST.
2. Add parser success/fail fixtures.
3. Add semantic model fields.
4. Add sema resolution and diagnostics.
5. Add merge/inheritance behavior if applicable.
6. Add codegen.
7. Add core runtime support only if required.
8. Add public docs.
9. Add compiled example if public syntax.
10. Add dev docs if architecture changes.
11. Run full verification.
