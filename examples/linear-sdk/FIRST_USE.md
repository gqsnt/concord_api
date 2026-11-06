# Independent first use: Linear

## Scope and entry point

- Consumer baseline: `ed548bd5d838b84f9a66e85c88eefa751016d0e2`; frozen Concord baseline: `6d6d66aa`. The initial worktree was clean and `git diff 6d6d66aa -- crates scripts docs checks` was empty.
- Requested runner: Codex gpt-6-astra, xhigh. No model-switch or runtime model attestation was available to this consumer; this records the request, not a verified runner identity.
- First entry point: `examples/linear-sdk/README.md`, followed by generated SDK rustdoc for `Linear`, response/error types, and pagination types. SDK source, existing SDK tests, and `FRICTION.md` have not been read. No other provider SDK supplied a solution.
- Only new consumer tests, new consumer fixtures, and this trace are authored. Existing `fixtures/** -text` in `.gitattributes` is retained.

## Prediction A — recorded before writing outcome A

Call `Linear::api_key(http, synthetic_key)?.with_base_url(loopback)?`, then `issues(2, Resume::Start)`. Expect POST `/graphql`, JSON request/response negotiation, a verbatim API-key authorization header, a fixed GraphQL query selecting `issues { nodes { id title description } pageInfo { hasNextPage endCursor } }`, and JSON variables `first` and `after`. The first cursor is absent or null; each next request must carry the preceding `endCursor` unchanged. Query operation names are not part of the documented consumer contract.

Expect `Page<Issue, String, PageMeta>` with typed fields, optional description, HTTP metadata, and GraphQL diagnostics. Empty pages with a cursor must continue. `hasNextPage: false` means `Ok(None)` on later polls without another request. A cap means `PagesError::Bounded` with a resume point, a failed page means `Provider` with its typed SDK error, and subsequent polls after either mean `Ended`. `collect_complete` should retain the successful prefix. Calling `finish` early should report `Stopped` and a resume point, never exhaustion.

HTTP 200 with partial data plus errors should return a page with diagnostics, requiring application acceptance policy. Missing usable connection or invalid continuation should produce `Error::Page` retaining the response (README prediction). A response body cut off before HTTP completion must fail, even if its JSON prefix is a complete document. No retries are expected. Missing selected fields should fail decoding instead of becoming nullable defaults.

## Prediction B — recorded before writing outcome B

Call `Linear::oauth(http, synthetic_token)?.with_base_url(loopback)?.issue(identifier).await`. Expect one POST `/graphql`, `Authorization: Bearer ...`, a fixed query selecting exactly `id title description`, and the identifier carried in JSON variables, never interpolated into the query. There is no documented arbitrary field-selection builder; the SDK's fixed selected fields satisfy this slice.

Expect `Response<IssueData>` retaining status/headers and the GraphQL envelope. `is_success()` requires data and no errors but can still be true for `data.issue: null`; the caller must check the nested option. Partial data and errors should coexist in an ordinary answer with `is_success() == false`. Errors-only and `data: null` envelopes should preserve diagnostics. Non-2xx responses should be `Error::Http` with raw bytes, parsed diagnostics, and headers. Invalid media type, missing selected fields, malformed JSON, and an interrupted response body should be `Error::Response` with HTTP metadata and a Concord error. No native request execution should be needed.

## Ordered trace

1. Read the SDK README and manifest, repository README, `docs/CALLER_GUIDE.md`, and `crates/concord-pagination/README.md`. Checked the pinned toolchain and available disk/RAM. No frozen-library changes were present.
2. Read the official [GraphQL guide](https://linear.app/developers/graphql) and [pagination guide](https://linear.app/developers/pagination) on 2026-11-06. They document POST JSON, personal-key/OAuth headers, issue selection, GraphQL partial errors, and forward cursors. All response fixture bytes below will be newly hand-authored from those shapes, not captured provider responses or copied SDK fixtures. Error extension values, cursors, IDs, messages, and request IDs are synthetic.
3. Generated rustdoc successfully with the required build environment: `cargo doc --locked -p concord-sdk-linear --no-deps`, `cargo doc --locked -p concord --no-deps --features csv,jsonl`, and `cargo doc --locked -p concord-pagination --no-deps`. Read rendered public documentation only, including `PagesError`, `Pages`, `Incomplete`, `Page`, `Progress`, `SequentialReport`, and Concord error categories. A local HTML extraction attempt lacked Python `bs4`; switched to the standard-library HTML parser without installing anything. This was tooling setup, not an SDK intervention.
4. Read the shared `examples/testserver/src/lib.rs` for its documented `Recorded`, `Reply`, `Body::FailAfter`, `TestServer::start`, request recording, and shutdown API. This is test infrastructure, not SDK source. The fake will be independent scripted replies with wire assertions. Wrote both predictions above before creating outcome code.
5. Created an empty `tests/first_use.rs`, then authored ten outcome-A tests and new `fixtures/first-use-*` documents. First test command compiled without Rust errors or warnings: **9 passed, 1 failed**. `null_connection_is_a_page_error_with_provider_diagnostics` expected the README's `Error::Page`, but observed `Error::Response` with `ErrorKind::Decode`, HTTP 200, operation identity, and a JSON type mismatch at line 1, column 22. The fixture was `data.issues: null` plus a GraphQL error. No SDK source read was needed to understand the public enum result.
6. Kept the frozen SDK intact and changed that consumer test to characterize the observed decode failure; added the missing `issues` member as a second malformed-connection case. The README prediction above is deliberately retained. This is an expectation correction after a failed run, not a claim that diagnostic retention passed. Outcome B proceeds from its already-recorded prediction.
7. Added eight outcome-B tests and formatted only the new test file with `rustfmt --edition 2024`. The combined command compiled without Rust errors or warnings: **17 passed, 1 failed**. The selected-issue success fixture contained an extra closing brace; the SDK correctly returned a JSON syntax failure at line 1, column 133. A Python JSON check identified that consumer-authored fixture error (and the deliberately malformed fixture). Corrected only `first-use-issue.json`; no SDK intervention was involved.
8. Reran the required test command after the fixture correction: **18 passed, 0 failed**, with no Rust compiler errors or warnings. Both workflows use `Linear` directly; no SDK source/existing-test read, private reach-through, native escape, manually supplied operation identity, retry, or reconstructed execution was needed. The test fake ignores the query operation's name while asserting the complete variable declaration, arguments, selection set, HTTP headers/path, JSON variables, and request count.

## Results and friction

**18 tests pass.** First SDK intervention: **none**. The two failed test runs and their corrections are recorded above; there were no distinct Rust compiler diagnostics. All requests went to the shared testserver on `127.0.0.1`, with synthetic credentials, proxy use disabled, redirects disabled, and a five-second transport timeout. This is offline consumer evidence, not live provider verification.

Working paths:

- A: `Linear::api_key` → `with_base_url` → `issues` → `next_page` / `collect_complete` / `finish`; direct `issue_page` also covers input validation and malformed connections. Tests check cursor bytes in JSON, empty intermediate pages, exact-cap exhaustion, bounded resume, failed-page prefixes, HTTP and GraphQL errors, partial-page policy, invalid continuations, explicit application stop, and interrupted HTTP framing.
- B: `Linear::oauth` or `api_key` → `with_base_url` → `issue` → `Response<IssueData>`. Tests check the fixed selected fields, variable encoding, nested nullable issue, partial and errors-only envelopes, raw HTTP rejections, content type, decoding, required nullable-field presence, interrupted HTTP framing, and both documented body limits.

- **Missing-connection diagnostics do not match the README:** `first-use-connection-null.json` yields `Error::Response { source: Decode, .. }`, not `Error::Page`. HTTP metadata survives, but this error variant offers no typed GraphQL envelope or raw body to recover the provider diagnostics. Reproduce with `cargo test --locked -p concord-sdk-linear --test first_use missing_connections_are_decode_failures_without_the_graphql_envelope` (using the build environment below). This is an adversarial missing-connection shape; no claim is made that Linear's current schema emits this exact response. No fallback execution or SDK patch was added.
- **Completion is separate from data acceptance:** a partial page is an ordinary `Ok(Some(page))`, and the chain can subsequently report `Exhausted`. Applications must inspect `page.meta.errors` before accepting items; flattening would lose those diagnostics, as the README warns. Reproduction: `partial_page_keeps_diagnostics_for_application_acceptance_policy`. The test keeps the documented page-level path and demonstrates a reject-partial policy. This is documented caller responsibility, not an unexpected SDK failure.
- **Envelope success is separate from issue existence:** `data: { issue: null }` yields `is_success() == true`. Applications still need `answer.envelope.data.and_then(|data| data.issue)`. Reproduction: `successful_envelope_does_not_mean_a_nullable_issue_exists`. The README explains this; it remains a condition the caller must remember.

All cargo commands in this session used this environment prefix. Final verification command:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-linear --test first_use
```

## Consumer fixture provenance

These files were newly authored for this consumer; no existing SDK fixture was read or reused. The official guides linked in trace entry 2 supplied the GraphQL envelope/query shapes and forward pagination rules, not these response bytes. All IDs, error codes, headers, cursors, titles, descriptions, and extension fields are invented. Paths/locations model GraphQL diagnostics; location numbers are synthetic, not validated against an actual Linear query compiler.

| New fixture | Purpose |
| --- | --- |
| `first-use-page-one.json`, `first-use-page-empty.json`, `first-use-page-last.json` | Forward traversal with an opaque cursor, an empty intermediate page, nullable description, and a terminal page. |
| `first-use-page-partial.json` | Partial connection data and a field diagnostic. |
| `first-use-errors.json`, `first-use-issue-errors.json` | Errors-only documents used for HTTP 200 and non-2xx cases. |
| `first-use-connection-null.json` | Adversarial missing connection alongside a diagnostic, testing the README's retention claim. |
| `first-use-issue.json`, `first-use-issue-partial.json` | Selected issue fields, plus nullable description and diagnostics in the partial case. |
| `first-use-issue-missing-description.json`, `first-use-malformed.json` | Deliberately invalid response shape and deliberately truncated JSON syntax. |

Tests also derive JSON variants in memory (missing connection member, invalid cursors, null data, null issue), use a synthetic plain-text gateway rejection, and pad valid JSON with whitespace to one byte above each documented body limit. Interrupted HTTP cases send a complete fixture as a chunk and deliberately omit the terminating HTTP chunk using `Body::FailAfter`; the success fixture's JSON is valid after the correction in trace entry 7. This checks HTTP completion separately from JSON completeness. All stored fixture bytes remain under the existing `fixtures/** -text` rule.
