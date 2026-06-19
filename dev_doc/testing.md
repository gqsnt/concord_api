# Testing

Concord uses several layers of tests because the project spans macro syntax, generated Rust, runtime behavior, and public docs.

## Macro tests

Trybuild pass/fail fixtures cover public DSL syntax and compile-time diagnostics. Parser diagnostics snapshots should be updated intentionally with:

```bash
TRYBUILD=overwrite cargo test -p concord_macros current_trybuild_fixtures_match_expected_results
```

Parser unit tests cover smaller syntax rules and span-sensitive diagnostics.

Sema unit tests cover name resolution, inheritance, policy merging, behavior expansion, and diagnostics that need semantic context.

Codegen tests inspect generated token output and public facade shape without relying on huge snapshots where a focused assertion is enough.

## Core tests

`concord_core` has runtime characterization tests for cache, inflight, rate-limit, auth rejection, retry, stale fallback, decode, pagination, codecs, and runtime configuration.

These tests protect runtime order and should be extended before runtime behavior is refactored.

Auth/redaction tests must cover arbitrary auth names, not only conventional names such as `Authorization` or `api_key`. When auth handling changes, verify that `BuiltRequest`, `BuiltResponse`, `DecodedResponse<T>`, debug sinks, errors, cache keys, and inflight keys do not contain raw auth material, while the materialized `TransportRequest` still carries real credentials at `Transport::send`.

Auth preparation boundary tests should also verify that no auth application hook receives `BuiltRequest`. `ClientContext::prepare_auth_requirement`, internal auth hooks, and generated auth preparation must receive the auth-only application request, and auth helpers must not be able to mutate logical request URL, headers, body, policy, timeout, or metadata.

Runtime strictness tests should reject invented policy values and silent saturation. Rate-limit `[host]` keys must fail explicitly when the logical URL has no host, and source guards should prevent `"<unknown-host>"` style fallbacks from returning. Cache TTL overflow belongs in macro semantic diagnostics, and request/auth attempt counters should return typed overflow errors instead of saturating.

## Examples and docs tests

`concord_examples` compile-checks public usage. It includes small examples, public docs fixtures, docs sync tests, release docs checks, and the Riot fixture.

The Riot fixture is the large real-world surface test. Do not change Riot semantics for unrelated macro or runtime work.

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
