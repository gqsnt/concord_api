# SDK26 Postmark — independent maintenance: batch sending

Date: 2026-11-07. SDK baseline `75139f9517d0c6cadab34c4b56df955677b106e3` on the frozen library
baseline `8c27cd1a`. The maintainer did not write the SDK or its first consumer. Live provider
calls: **NOT_RUN**; every request goes to a loopback `TestServer`. This record lists observable
actions, diagnostics, files read and decisions; it is not a reasoning transcript. Nothing was
committed by the maintainer.

## Prior exposure (what was read before starting)

Before the ticket was attempted: the maintenance capsule; `git log`/`git status` of the worktree;
this package's `Cargo.toml`, `src/lib.rs`, `README.md`, `PROVENANCE.md`, the four fixtures,
`tests/protocol.rs`, `tests/workflows.rs` and `tests/first_use.rs`; Concord's
`docs/CHEATSHEET.md`, the section list of `docs/AUTHOR_GUIDE.md` and its §5.11–5.13 (response
metadata, `concord::Endpoint`, `send policy;`) and §9–10 (generated names, limits); doc comments of
`Endpoint`, `SendWith` and `Sends` in `crates/concord-core/src/prepared.rs`; the public function
lists of `concord-core`'s `response.rs` and `error.rs` (to learn that `concord::Error` has no
public constructor); the re-exports in `crates/concord/src/lib.rs`; the public helpers of
`examples/testserver/src/lib.rs`; the root `Cargo.toml` workspace lints. No other example SDK was
read. Harness-loaded instructions (user and project `CLAUDE.md`, memory index) were in context.

`FRICTION.md` and `FIRST_USE.md` were read only **after** the implementation, its tests and all
four verification commands had passed. Nothing in the implementation was changed after reading
them; they informed only this record (see "Relation to earlier records").

## Provider facts used (read 2026-11-07)

- [Email API, "Send batch emails"](https://postmarkapp.com/developer/api/email-api):
  `POST /email/batch` with `Content-Type`/`Accept: application/json` and the server token; the
  body is a JSON array of single-send messages; up to 500 messages per call and 50 MB including
  attachments. The response is an array in the same order as the submitted messages; the endpoint
  returns a 200-level status even when validation of individual messages fails, so each element's
  `ErrorCode` and `Message` must be checked. The documented accepted element has `ErrorCode` 0,
  `Message` "OK", `MessageID`, `SubmittedAt`, `To`; the documented rejected element (406, inactive
  recipient, whose text names the recipient address) omits `MessageID`, `SubmittedAt` and `To`.
- [API overview](https://www.postmarkapp.com/developer/api/overview), "HTTP response codes" (413:
  50 MB total for the batch API; 422, 429, 500, 503) and "API error codes" (406 inactive
  recipient; 410 batch limit, at most 500 messages; both delivered with HTTP 422).
- Not stated by the pages, therefore **inferred, not observed**: that a rejected element was not
  sent (the code and example text suggest it); what Postmark does with an empty batch; whether a
  200 response can ever hold a different number of results than messages. The SDK treats the last
  case defensively (below) and tests it only with synthetic fixtures.

## What changed for callers

- New endpoint `api.outbound().send_batch(emails: Vec<Email>).send().await` →
  `Result<Vec<BatchResult>, Error>`, declared in `concord::api!` beside `send_email`
  (`POST email/batch`). Single sending, listing and details are unchanged on the wire and in type.
- New `BatchResult` enum, one per submitted email, in submission order:
  `Accepted(Acceptance)` (reuses the single-send type; its `message_id` feeds `details`) or
  `Rejected(ErrorDocument)` (reuses the error-document type, whose `Debug` already redacts
  `message`; the provider's code is `error_code`). Rule: `ErrorCode` 0 → accepted, any other code
  → rejected, even if acceptance fields are present.
- A whole-batch unknown outcome, never resent:
  - lost or unreadable response (timeout, stall, disconnect, unfinished body, malformed JSON,
    wrong media, oversized, a JSON object instead of an array, or an entry with `ErrorCode` 0
    lacking `MessageID`/`To`/`SubmittedAt`) → existing `Error::UnknownOutcome(concord::Error)`,
    because the batch is a POST and the existing policy already classifies POST receive/decode
    failures that way;
  - a 2xx with a result count different from the submitted count → new
    `Error::UnmatchedBatch { submitted, results }` (results kept for investigation, never for
    attribution). `Display` prints only the two counts; `Debug` relies on `ErrorDocument`'s
    redaction.
  - new `Error::is_unknown_outcome()` is true for both, so a caller writes one "do not resend"
    test. `Error` was already `#[non_exhaustive]`, so the variant is additive.
- The 500-message and 50 MB limits stay provider validation, like the existing count/offset
  bounds: the list is sent unchanged and a refusal (410/422, 413, 429 …) is `Error::Api` for the
  whole batch.
- README: new "Batch sending" section with a compiled (`no_run`) example; batch removed from the
  exclusions. Inbound, webhooks and delivery guarantees remain excluded.

## Every file and location edited

| File | Location | Edit |
| --- | --- | --- |
| `src/lib.rs` | crate doc | batch ordering and whole-batch unknown outcome |
| `src/lib.rs` | `ErrorDocument` doc | also a rejected batch entry; text can name recipients |
| `src/lib.rs` | after `ErrorDocument` | `BatchResult`, private wire `BatchEntry`, `TryFrom<BatchEntry>` |
| `src/lib.rs` | `enum Error` | `UnmatchedBatch` variant, `impl Error { is_unknown_outcome }` |
| `src/lib.rs` | `impl Display for Error` | arm for `UnmatchedBatch` |
| `src/lib.rs` | before `PostmarkPolicy` | private-module trait `Reply` (`expected`, `check`); impls for `Acceptance`, `MessagePage`, `Message`, `Vec<BatchResult>` |
| `src/lib.rs` | `impl SendWith<T> for PostmarkPolicy` | bound `T: Send + Reply`; count the request before `receive()`; `output.check(expected)` after decode; doc line |
| `src/lib.rs` | `concord::api!`, `scope outbound` | `POST send_batch(body emails: Vec<Email>) -> Vec<BatchResult> { path "email/batch"; }` |
| `fixtures/batch.json` | new | documented response example (rejected text shortened) |
| `tests/batch.rs` | new | focused regression tests (4) |
| `tests/first_use.rs` | import line; appended test | consumer test (1); no existing line or assertion removed |
| `README.md` | intro, new section, exclusions | caller contract and example |
| `PROVENANCE.md` | sources, fixture table, dependency note | citations and derivations |
| `MAINTENANCE.md` | new | this record |

`Cargo.toml` and `Cargo.lock` are unchanged; no crate was added (`serde::de::IgnoredAny` comes
from the existing `serde`).

### Facts that had to be written in several places

- **One result per email, in order**: the declaration's doc comment, `BatchResult` doc, crate doc,
  README, and as executable logic in `impl Reply for Vec<BatchResult>` (counting). Only the last is
  checked by anything (tests).
- **The output type `Vec<BatchResult>`**: the declaration's `->` and `impl Reply for
  Vec<BatchResult>`. The compiler ties them (below).
- **Every endpoint output** now also appears in an `impl Reply` line. This is new per-endpoint
  duplication introduced by this change: the next endpoint needs a declaration line *and* an
  `impl Reply for Output {}` line. The compiler enforces it.
- **Wire field names** (`ErrorCode`, `Message`, `MessageID`, `To`, `SubmittedAt`) now exist in
  two serde structs, `Acceptance` (derive) and `BatchEntry` (wire shape for the enum). Not checked
  by the compiler; a misspelling in `BatchEntry` is caught by the fixture tests (an accepted entry
  would lose its ID and the mixed-batch test fails).
- **The 500-message / 50 MB limits**: declaration doc, README, PROVENANCE, a test comment. Prose
  only; nothing would notice a provider change.
- **"POST means a send whose outcome can be unknown"**: unchanged single place (the policy's method
  check); batch relies on it implicitly.
- **The path `email/batch`**: declaration and the tests' expected targets (tests catch drift).

## Did the compiler or the tests catch an omitted edit?

Observed by temporarily removing one edit at a time and restoring the file (byte-compared):

| Omitted edit | Caught by | Observation |
| --- | --- | --- |
| `impl Reply for Vec<BatchResult>` | compiler | 4 errors `E0277: the trait bound Vec<BatchResult>: Reply is not satisfied`, spanned on the `concord::api!` invocation and on `send PostmarkPolicy;`, with "required for `PostmarkPolicy` to implement `SendWith<Vec<BatchResult>>`" and a list of the existing `Reply` impls. It names the output type but not the `send_batch` line; Concord's own `on_unimplemented` text for `SendWith` is not shown because rustc reports the SDK's leaf bound instead. Actionable. |
| `Display` arm for `UnmatchedBatch` | compiler | `E0004: non-exhaustive patterns: &Error::UnmatchedBatch { .. } not covered`. |
| the count check (condition forced false) | tests | `result_count_unlike_submitted_count_is_unknown_for_the_whole_batch` failed; other batch tests passed. |
| strict accepted entry (missing ID mapped to a value) | tests | `lost_or_unreadable_batch_response_is_unknown_and_never_resent` failed at the `accepted-without-id` case. |

Not caught by anything: stale prose (README exclusions, limit numbers, doc comments). The README's
code blocks are compiled as doctests, so a stale *example* would be caught.

## Decisions and alternatives rejected

- **Whole-batch attribution check in the policy.** Ordering is the only link between an email and
  its result. A short or long 2xx array would let a caller who zips results silently drop or shift
  outcomes and then resend "missing" emails, which is the duplicate-send failure the ticket
  forbids. The generic policy cannot see the endpoint's typed body, and an SDK cannot construct a
  `concord::Error`, so the SDK (a) counts its own buffered JSON request body before `receive()`
  (`Vec<IgnoredAny>`, no allocation per element, but a second parse of up to 50 MB) and (b) adds
  its own error variant. If the body cannot be counted (`Request::body()` not buffered), the check
  is skipped; with Concord's JSON encoder it is always buffered.
- **Rejected:** a hand-written `send_batch` wrapper beside the generated endpoint (two paths, the
  generated one bypassing the check); returning raw entries and leaving correlation to the caller;
  `#[serde(untagged)]` for the enum (an entry with a non-zero code and all acceptance fields would
  decode as accepted); client-side 500-message validation (inconsistent with the slice's
  "provider validates bounds" rule, and it would duplicate a provider limit that can change).
- **Reuse over new types**: `Acceptance` and `ErrorDocument` already had the right shapes and the
  right redaction; no `Rejection` type was added.

## Remaining generic duplication

- The `Reply` trait plus one impl per output is a generic mechanism written by hand: "a per-
  operation rule applied to a decoded 2xx output" with access to the request. Every SDK with one
  positional or self-validating operation behind a generic send policy would write the same thing.
- Two places map a Concord failure to `UnknownOutcome` vs `Request` by HTTP method (unchanged from
  the author, noted in `FRICTION.md`).
- `BatchEntry` re-declares wire names already declared on `Acceptance`.
- Counting a request body that Concord itself encoded from a `Vec` the caller handed over.

## Provider-specific responsibilities (stay in the SDK)

- The `ErrorCode == 0` acceptance rule and the meaning of non-zero codes in a 200 batch response.
- Which fields an accepted entry must carry to be attributable, and redaction of provider text
  (which can contain recipient addresses).
- That results are positional and must match the submitted count.
- That a lost or unattributable batch is unknown as a whole and must not be resent.
- Batch limits (500 messages, 50 MB) left to the provider and reported as `Error::Api`.

## Candidate Concord improvements (proposals only; shared crates untouched)

1. **Per-endpoint output check with the typed inputs.** Let a send policy, or the declaration,
   attach a post-decode rule that sees the endpoint's declared inputs (e.g. the body's length) and
   the decoded output, instead of re-parsing `Prepared::request()`'s bytes. A minimal form: an
   `Endpoint` accessor for its body value, or a generic `SendWith` hook that receives a borrowed
   endpoint before `prepare()` consumes it.
2. **A public way for SDKs to build a `concord::Error`** of kind `Decode` with a static label
   (e.g. "result count differs"). The SDK could then keep a single `UnknownOutcome(concord::Error)`
   variant instead of adding a second "unknown" variant and a helper to group them.
3. **Diagnostic span**: when an endpoint output fails the policy's bound, point at the endpoint's
   `->` type in addition to the `api!` invocation and `send` statement.
4. **Operation semantics metadata** (already proposed in `FRICTION.md`): declaring "this operation
   submits" would replace the POST check that batch silently depends on.

## Relation to earlier records (read after the work)

- `FRICTION.md` already identified the generic-policy limitation for send classification; batch
  hit the same limitation a second time, for an output rule rather than an error rule.
- `FIRST_USE.md` records that a single-send 2xx with a non-zero `ErrorCode` is returned as
  `Ok(Acceptance)` with the code unchanged and suggests the SDK document or validate the
  `error_code == 0` invariant. Batch now applies that invariant per entry (`Accepted` always has
  code 0). Single sending was deliberately **not** changed (ticket: keep it unchanged), so the two
  paths now differ on this adversarial case; a follow-up decision for the owner.
- `FIRST_USE.md` friction 1 (generated `send()` docs show `<PostmarkPolicy as SendWith<T>>::Output`)
  now has a small addition: the policy's rustdoc impl header shows a `Reply` bound that callers
  cannot name.

## Verification (this worktree, `CARGO_TARGET_DIR` absolute, `CARGO_BUILD_JOBS=4`, one at a time)

- Baseline before edits: `cargo test --locked -p concord-sdk-postmark` — 10 integration tests and
  1 compile doctest passed.
- `cargo test --locked -p concord-sdk-postmark` — passed: `batch` 4, `first_use` 3 (2 existing + 1
  new), `protocol` 4, `workflows` 4, doctests 2 (README examples, compile-only). 15 integration
  tests, 0 failures.
- `cargo clippy --locked -p concord-sdk-postmark --all-targets -- -D warnings` — passed.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-postmark --no-deps` — passed.
- `cargo fmt --all -- --check` — passed after `cargo fmt -p concord-sdk-postmark`, which reflowed
  two newly written assertion lines only.
- `Cargo.lock` unchanged (`git diff --stat -- Cargo.lock` empty). No file outside
  `examples/postmark-sdk/` changed.
