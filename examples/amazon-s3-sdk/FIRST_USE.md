# Independent first use

## Predictions, recorded before writing outcome A

- Entry point: the SDK README, Construction and Workflow A; then generated crate rustdoc
  (`S3Client`, `Bucket`, `ListObjectsV2`, `ObjectHead`, `ObjectBody`, `S3Error`). No SDK
  source, existing tests, author notes or `FRICTION.md` have been read.
- A prefix inventory sends virtual-hosted `GET /?list-type=2&prefix=...&max-keys=...`.
  Each request is SigV4 signed for the configured region/service and uses opaque continuation
  tokens unchanged. `pages().items().collect_complete()` yields object summaries only on
  exhaustion; a failed second page retains the first page and a typed failed cursor. A cap
  retains a resume cursor; truncated pages missing a token must fail rather than complete.
- `head_object(key).send()` sends HEAD with no request body and returns validators and metadata.
  `get_object(key).if_match(etag).range(...)` sends one Range and the opaque ETag; a matching
  206 yields a parsed partial head and an unread body. The caller must consume that body to
  establish completion. A conditional 304 is `NotModified`, a stale validator is a 412 API
  error, a missing object is a 404 API error with a parsed XML document (HEAD has no document).
- Bytes are complete only after validated EOF. Over-limit collection and early connection
  termination must produce `S3Error::Body`, retaining progress and response metadata. A
  response with the wrong Content-Range must fail rather than return unrelated bytes.

## Predictions, recorded before writing outcome B

- README Workflow B: `put_object(key, finite_bytes, ContentType)` sends one PUT with those exact
  bytes, encoded key segments, an explicit content type, metadata headers, and a SHA-256 signed
  payload. `.if_absent()` sends `If-None-Match: *`.
- Success yields an opaque ETag/version/request ids. GET then returns the same bytes and
  metadata. The ETag is compared for equality, never treated as a digest.
- A second conditional create fails with 412 and a typed XML error, leaving the existing
  object intact. A lost PUT response yields `Transport`; it does not establish whether the
  object was stored, so a later GET must reconcile it. No automatic retries are expected.

## Ordered observation trace

1. Initial tree was clean. Actual HEAD is `b30d9994061eb7f4270f2bc651284e38003011e2`
   (SDK04 addition), whose parent is the requested frozen core candidate `94d13527`.
   No checkout/reset or changes to frozen code were made. Toolchain file pins 1.99.1.
2. Read SDK README/Cargo.toml, repository README and caller guide, pagination README, and
   the public testserver implementation to learn its fixture API. No other provider SDK
   supplied the solution. Generated both requested rustdoc sets successfully with `--locked`
   and the specified build-job/cache prefix. Read public HTML only, not rustdoc source links.
   A local HTML extraction attempt lacked Python `bs4`; used stdlib `HTMLParser` instead.
3. Consulted the five official AWS URLs supplied in the task and the linked single-chunk
   SigV4 algorithm. Fixture documents and binary bytes will be independently hand-authored,
   not copied AWS responses or SDK test fixtures. All endpoints will resolve to loopback;
   all credential strings are conspicuously synthetic. Managed-environment status tool and
   network-policy file are unavailable; no credentials were inspected or needed.
4. Harness limitation discovered before implementation: `Reply::Full` always emits a length
   based on its body and writes that body even for HEAD. To produce a conforming HEAD without
   an entity body, use public `Body::Stall` plus `Connection: close`, omit Content-Length, and
   shut down owned tasks after the assertions. This is fixture plumbing, not an SDK source
   read or a native request/execution escape.

5. Wrote the independent test suite from the predictions. First requested test command failed
   to compile with one distinct diagnostic, emitted at two calls: E0599 for `ETag::new`.
   Rustc suggested `ETag::parse`; imported digest/key-init traits made the diagnostic include
   unrelated crypto trait bounds. Confirmed `parse` in public ETag rustdoc and corrected both
   calls. The compiler printed SDK source locations/signatures automatically; no SDK source
   file was opened. No other provider SDK or existing SDK test supplied code.

6. After the ETag correction, the requested command passed all 13 tests, with no runtime
   failures. Every request was checked by an independently written SigV4 verifier operating
   on `Recorded`, using the documented AWS algorithm and the existing hmac/sha2 dev-dependencies.
   It checks scope, clock, session token, payload hash, sorted canonical query/headers and the
   recomputed signature. Added the same bodyless harness helper to HEAD errors and 304 so the
   fake does not invent Content-Length: 0 for a nonempty representation.
7. Working A path: construct S3Client/Bucket, list through three pages (including an empty
   middle page), select a decoded Unicode key, HEAD for ETag/metadata, GET with If-Match and
   Range, collect within a limit. Failed pages retain items and their failed token; manual
   resume fetches the rest. Caps, terminal errors, missing/repeated tokens, malformed XML,
   304, 412, GET/HEAD 404, mismatched ranges, full-200 range fallback, body limits and broken
   chunked transfer are checked. A valid chunked transfer reaches EOF with exact received bytes.
8. Working B path: put the binary fixture with explicit media type, metadata, cache control
   and if_absent; retrieve exact bytes and compare opaque ETag/version. A second create returns
   typed 412 and leaves the first bytes intact. A fake that stores bytes and drops the PUT
   response produces Transport with no status, exactly one PUT, and successful GET reconciliation.
9. No SDK source/existing-test reads or native operation escapes were needed. No private
   reach-through, handwritten operation identities, or rebuilt execution were used. All
   operation requests went through public SDK builders and send(); only transport construction
   uses the documented reqwest hooks. Frozen code and pre-existing SDK files remain unchanged.
10. Final rerun after the bodyless-fixture adjustment: 13 passed, 0 failed. Targeted rustfmt,
    whitespace, XML well-formedness, exact 24-byte payload and Git attribute checks passed.
    Git status contains only the new consumer, this trace, and the new first-use fixtures.

## Concrete friction and reproduction

- **Stopped body and verified completion look alike after an error.** Run the requested test
  command with filter `a_interrupted_body_requires_caller_to_remember_failure`. After the
  first `Err(S3Error::Body(..))`, `is_finished()` is true and a second `chunk()` is `Ok(None)`.
  The README's broad claim that None means verified completion needs the rustdoc qualification
  that calls after failure also return None. The working consumer retains the first error and
  does not publish the staged prefix. This is public, documented behavior, not a library patch.
- **Persisted validator construction required a lookup.** `ETag::new("\"stale\"")` produced
  E0599 (with crypto trait-bound noise in this consumer). `ETag::parse("\"stale\"")` compiles.
  Most input values in the README use `new`, but its workflow obtains ETags from HEAD and does
  not illustrate reconstructing a cached one. Public rustdoc was sufficient to resolve it.
- **Bodyless fake replies need fixture plumbing.** The public testserver's `Reply::new` always
  emits Content-Length for Full. `no_body_response` uses Stall/Connection: close for HEAD/304,
  and every test explicitly shuts down owned tasks. This is a harness limitation, not Concord
  or S3 SDK friction. A declared-length premature EOF could not be represented faithfully by
  these Reply modes without more harness work; the suite tests interrupted chunked transfer
  instead and does not claim coverage of that distinct wire framing case.

## Verification

The exact task command (with the required environment prefix) is:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-amazon-s3 --test first_use
```

Result: **13 passed, 0 failed**. The SDK root's existing `fixtures/** -text` attribute is kept.
This is offline loopback fixture evidence, not verification against AWS.

## Post-wave-1 integration (coordinator)

Disposition of "Stopped body and verified completion look alike after an error": the SDK added
`ObjectBody::completion() -> BodyCompletion` (`Reading`, `EndOfInput` only at the verified end,
`Failed(kind)` after an error), like the Concord readers' `completion()`. Later `chunk()` calls
still return `Ok(None)` after an error, so this suite's
`a_interrupted_body_requires_caller_to_remember_failure` is unchanged and still passes. The
bodyless-HEAD harness limitation was answered in the test server (`Body::HeadOnly`), which the
author's test support now uses; this suite's `no_body_response` is unchanged.

## Coordinator note: generated builders (between waves 2-3, after the value-form policy)

The SDK's builders are now the generated ones, and `tests/first_use.rs` was migrated
mechanically. Its predictions and its trace above describe the earlier API.

What changed in the suite:

- the setters were renamed (`if_match` → `set_if_match`, `range` → `set_range`, `if_absent()` →
  `set_if_none_match(AnyObject)`, `url_encoded_keys()` → `set_encoding_type(UrlEncoding)`, ...);
- bodies are now `Bytes`;
- the metadata put ends with `.metadata(m).send(&s3)`, which is why `bucket()` became
  `client()` + `bucket()`.

No assertion or expected value changed, and all 13 tests pass. Details: `INTEGRATION.md`,
"Between waves 2-3 (after the value-form policy)".
