# Maintenance record: delayed capture

Role: independent maintainer of `concord-sdk-square-payments`, which I did not
write, working after its first consumer (`tests/first_use.rs`). Date: 2026-11-07.
Frozen Concord baseline `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`; no shared crate,
check, other example or script was edited. Nothing is committed (the coordinator
commits). **Live provider calls: NOT_RUN.**

## Prior exposure

Read **before** implementing, in order:

1. My role prompt (ticket, rules, report format). No other role, brief or record
   file and no other `work/w3-*` worktree was opened.
2. This package: `README.md`, `PROVENANCE.md`, `VERIFICATION.md`, `Cargo.toml`,
   `src/{lib,error,models}.rs`, `tests/{workflows,protocol,first_use}.rs`,
   `tests/support/mod.rs`, fixtures `payment.json`, `page-1.json`, `mismatch.json`,
   `create-request.json`. I grepped the root `Cargo.toml`/`Cargo.lock` for the member entry.
3. Concord docs and source: `docs/CHEATSHEET.md` (whole); `docs/AUTHOR_GUIDE.md`
   §5.4 (absent/empty bodies), §5.11b (`Error::delivery()`), §5.12 (`Endpoint`), §9
   (generated names); `concord-core/src/response.rs` (`Operation`, its `PartialEq`),
   `prepared.rs` (`Endpoint::operation()`), `error.rs` (`operation()` accessors);
   the re-export list in `crates/concord/src/lib.rs`; the public signatures in
   `examples/testserver/src/lib.rs` (grep only).
4. The official Square pages listed in PROVENANCE.md (maintenance section), read
   2026-11-07 through a fetch tool that summarises pages.

I read `FRICTION.md` and `FIRST_USE.md` only **after** the implementation and all
tests were green. They changed no design decision. FIRST_USE friction 8 (provider
`Display` is `Square HTTP <status>`, so log lines cannot tell codes apart) applies
to `VERSION_MISMATCH` too, and I left it unchanged. The user and project
`CLAUDE.md` instructions and the memory index were in my context from the harness.

## What changed for callers

The hierarchy is still `Square → payments() → endpoint`. Two endpoints and one
create constructor were added. Create, get and list are unchanged on the wire and
in the API, and so is every existing assertion.

| Caller surface | Wire / behaviour | Documentation basis |
| --- | --- | --- |
| `CreatePayment::authorization(key, source, amount)` | Same validation and body as `new`, with `autocomplete: false`. The payment comes back `APPROVED`. The same key/body replay rule applies | create-payment reference, delayed-capture guide |
| `payments().complete(id, CompletePayment::expecting(token))` | `POST /v2/payments/{id}/complete` (ID is one encoded segment), body `{"version_token": token}` | complete-payment reference |
| `payments().complete(id, CompletePayment::unconditional())` | Same route, body `{}` (Square's own example), not an absent body | complete-payment reference |
| `payments().cancel(id)` | `POST /v2/payments/{id}/cancel`, no body, no `content-type` | cancel-payment reference |
| `Payment::version_token: Option<String>` | Decoded from every payment response, including listings; `None` when omitted | Payment object |
| `ProviderError::is_version_mismatch()` | True when the document holds the code `VERSION_MISMATCH`. Square states no status, so the status is ignored (tested at 400 and 409) | complete-payment reference, optimistic-concurrency guide |
| `Error::outcome_unknown()` | The former `create_outcome_unknown` rule applied to every write (POST): transport failure after sending, unreadable 2xx, 5xx or redirect | — (SDK rule; Square gives no timeout guidance for complete/cancel) |
| `Error::create_outcome_unknown()` | Now `outcome_unknown()` **and** operation == create. Unchanged for every existing create error; false for complete/cancel | — |

A complete or cancel carries no idempotency key, so the SDK never resends one. When
`outcome_unknown()` is true, the caller re-reads with `payments().get(id)` and acts
on `status` (`APPROVED`, `COMPLETED`, `CANCELED`). On a version mismatch nothing was
captured. The caller re-reads, decides again, and completes with the fresh token
if it still wants to. `DELAYED_TRANSACTION_CAPTURED`, `_CANCELED`, `_EXPIRED` and
`_FAILED` stay ordinary provider codes in `document`, with no predicate (see below).

Not added: `delay_duration`/`delay_action` (Square's defaults apply, which cancel
after 36 h card-present or 7 days card-not-present), partial authorization,
UpdatePayment and CancelPaymentByIdempotencyKey.

## Every file and location edited

| File | Location | Edit |
| --- | --- | --- |
| `src/lib.rs` | module doc (l. 1–9) | Scope now includes delayed capture; re-read rule |
| `src/lib.rs` | `pub use models::{..}` (l. 25–27) | Re-export `CompletePayment` |
| `src/lib.rs` | `create` doc in `api!` | No longer says "immediate capture" only |
| `src/lib.rs` | `api!` scope `payments` (l. 76–93) | `POST complete(#[sensitive] payment_id, body completion: CompletePayment)` with `path "{payment_id}/complete"`; `POST cancel(#[sensitive] payment_id)` with `path "{payment_id}/cancel"` |
| `src/models.rs` | `CreatePayment` doc (l. 12–16) and impl (l. 25–91): `new`, new `authorization` (l. 47), private `validated` (l. 55) | Validation moved unchanged into `validated`; the `autocomplete` literal became a parameter |
| `src/models.rs` | new `CompletePayment` (l. 125–161) | `expecting`/`unconditional`, `skip_serializing_if`, presence-only `Debug` |
| `src/models.rs` | `Payment` status doc and `version_token` (l. 171–176), `PaymentResponse` docs | `version_token` field; status doc |
| `src/error.rs` | `Error` impl (l. 18–54) | `outcome_unknown` (old body), restricted `create_outcome_unknown`, private `operation()` |
| `src/error.rs` | `ProviderError` impl (l. 84–105) | `is_version_mismatch`; shared private `has_code` (`is_idempotency_mismatch` uses it, same behaviour) |
| `tests/protocol.rs` | import lines only, plus new section from l. 409 | 3 regression tests (below) |
| `tests/delayed_capture.rs` | new | Consumer test |
| `fixtures/` | new `authorization-request.json`, `authorized-payment.json`, `version-mismatch.json` | — |
| `README.md` | intro, hierarchy line, permissions line, new `## Delayed capture` with a compiled `no_run` example, `## Listing` / `## Diagnostics and scope` headings, redaction sentence, Excluded list (removed "delayed capture"), links | — |
| `PROVENANCE.md`, `VERIFICATION.md` | appended maintenance sections | — |

Root `Cargo.toml`/`Cargo.lock`: unchanged (the package was already a member). No dependency was added.

### Facts that had to change in several places

- **Which writes are uncertain and how each is resolved**: the method test in
  `Error::outcome_unknown` (`== Some("POST")`), the operation test in
  `create_outcome_unknown`, the `complete`/`cancel` doc comments in `api!`, the
  `CompletePayment` doc, the module doc and the README. That is six statements of
  one rule, and the code holds it in two forms (method string and operation identity).
- **`autocomplete`**: `CreatePayment::{new, authorization}`, `authorization-request.json`,
  the README and both test files.
- **Routes `/complete` and `/cancel`**: the declaration, the README, the regression
  test targets and the consumer stand-in's routing.
- **`VERSION_MISMATCH`**: `error.rs`, `version-mismatch.json`, the consumer stand-in
  and the README.
- **Square's default authorization window** (36 h / 7 days): the
  `CreatePayment::authorization` doc and the README.
- **`version_token` redaction choice**: hidden in `CompletePayment`'s `Debug`, shown in
  `Payment`'s derived `Debug` (model fields were already unredacted). Both are stated
  in the README.

## Did the compiler or the tests catch an omitted edit?

**Observed** with a mutation run: six one-line source mutations, each applied in
isolation, the full package test suite run, then the source restored (verified
byte-identical):

| Mutation | New tests failing | Pre-existing tests failing |
| --- | --- | --- |
| M1 `create_outcome_unknown` back to the old POST-only rule | consumer, `delayed_capture_errors_are_typed_and_never_resent`, `timed_out_complete_and_cancel_..` | **none** |
| M2 `authorization` sends `autocomplete: true` | consumer, `delayed_capture_wire_shapes` | none |
| M3 `is_version_mismatch` matches another code | consumer, `delayed_capture_errors_..` | none |
| M4 `skip_serializing_if` dropped (`{"version_token":null}`) | consumer, `delayed_capture_wire_shapes` | none |
| M5 `outcome_unknown` limited to create | consumer, `timed_out_complete_and_cancel_..` | none |
| M6 complete path `"{payment_id}/capture"` | consumer, `delayed_capture_wire_shapes`, `timed_out_..` | none |

Every mutation compiled. **The compiler caught nothing**, and no pre-existing test
caught anything. The important case is M1. The original
`create_outcome_unknown` keyed on the HTTP method. Once I declared two more POST
endpoints, it silently started answering true for timed-out completes and cancels,
inviting a caller's "replay the retained create" logic. With the old body, all 19
pre-existing tests and the compiler pass. Only a test written for the new endpoints
exposes it.

The first build of the change compiled, and the existing suite passed unchanged.
The new tests passed on their first run, which is why I ran the mutation check.
Adding `Payment::version_token` breaks struct-literal construction of `Payment`
and exhaustive destructuring of it outside the crate (inferred; nothing in the
repository does either). The consumer test uses struct-update syntax. Stale prose
(the README's old "Excluded: delayed capture") is caught by nothing. The README
doctest catches only code drift in its examples. `cargo doc -D warnings` would
catch a renamed method behind an intra-doc link (inferred, not exercised).

## Tests added

- `protocol.rs::delayed_capture_wire_shapes` (regression). Checks the authorization
  body against an independent fixture and against the immediate body with only
  `autocomplete` changed. Also checks the complete target, `content-type` and token
  body; the exact `{}` bytes; the bodiless cancel; no idempotency header; and
  `APPROVED`/`COMPLETED`/`CANCELED` decoding with `version_token`.
- `protocol.rs::delayed_capture_errors_are_typed_and_never_resent` (regression).
  `VERSION_MISMATCH` at 400 and 409 is a mismatch. `DELAYED_TRANSACTION_*` is not.
  A 5xx on complete or cancel is `outcome_unknown` and never `create_outcome_unknown`.
  A read's 5xx is not uncertain. Build refusals (`..`, empty ID) are not uncertain
  and send nothing. Seven requests for seven sends. Token, ID and detail never
  appear in `Debug`/`Display`.
- `protocol.rs::timed_out_complete_and_cancel_are_unknown_but_not_create_replays`
  (regression). Committed then held responses give `Delivery::Unknown` and
  `outcome_unknown`. Complete and cancel are not create replays. A timed-out create
  is still both. Three requests, so nothing was resent.
- `delayed_capture.rs::authorize_capture_void_with_stale_and_uncertain_outcomes`
  (consumer). An order application with `authorize`, `capture` and `void` routines
  runs against a stateful stand-in. It covers: capture with the checkout token; a
  stale token after an out-of-band change (typed, nothing captured, re-read, the
  app captures the fresh version); an uncertain complete (committed, unanswered,
  then re-read: `COMPLETED`, exactly one complete sent); a void and an uncertain
  void resolved by re-read; capturing a voided payment as a typed refusal that is
  not a mismatch and not uncertain; an unconditional `{}` complete; and immediate
  capture still sending `autocomplete: true`.

Results are in VERIFICATION.md (maintenance section). Totals: 24 tests and 2 doctests pass. Clippy, rustdoc and rustfmt are clean.

## Remaining generic duplication

- **Write classification by method string.** `outcome_unknown` re-derives "is a
  write" from `op.method() == Some("POST")`, which the declaration already knows.
  AUTHOR_GUIDE §5.11b shows the same pattern, so every SDK writes it, and M1 shows
  it silently absorbs new endpoints.
- **Operation identity needs the output type.**
  `<SquarePaymentsCreate as Endpoint<PaymentResponse>>::operation()` names the
  response type only to reach a static identity that does not depend on it.
- **The two-variant split** (`Error::Concord(e).operation()` versus
  `Error::Provider(p).meta.operation()`) is repeated in each predicate and was
  folded into a private `operation()` helper. Same shape as F1 in FRICTION.md.
- **Version duplication, not touched:** `API_VERSION` and the
  `headers { "square-version" = "2026-09-16"; }` literal are separate facts. I did not
  check whether the header block can reference a Rust constant.

## Provider-specific responsibilities (stay in the SDK)

- `autocomplete: false` semantics and the statuses as open strings.
- `version_token` is opaque. The `{}` versus token body shape.
- `VERSION_MISMATCH` is decided by its code: the status is undocumented and the
  code is absent from Square's ErrorCode enum page, though named on the
  CompletePayment reference.
- Complete and cancel have no idempotency key, so uncertainty is resolved by
  re-reading. The SDK documents this and never resends. Reconciliation (what to
  do with an `APPROVED` re-read) belongs to the application.
- No predicates for `DELAYED_TRANSACTION_*`. They describe a payment that is already
  finished, and the re-read status already says that. They remain readable in
  `document`. Adding predicates would be a separate decision.
- The authorization window and its defaults are Square's. Exposing
  `delay_duration`/`delay_action` is future work.
- CancelPaymentByIdempotencyKey (`POST /v2/payments/cancel`) is the documented way
  to void a payment located by its create key. It would let a caller void an
  authorization whose create outcome is unknown. Excluded here, and a natural next
  ticket.

## Candidate Concord improvements (recorded, not implemented)

1. **Output-independent operation identity.** A generated inherent constant (for
   example `SquarePaymentsCreate::OPERATION`) or `Operation::of::<E>()` would let
   an SDK key per-operation rules on identity without naming `T`. Evidence:
   `error.rs` `create_outcome_unknown`.
2. **AUTHOR_GUIDE §5.11b note.** The method-based `uncertain` example should say
   that any later endpoint with the same method joins the rule, and suggest keying
   operation-specific rules (such as "replay with the same key") on operation
   identity. Evidence: M1. This is a documentation change; no new declaration
   syntax is justified by two endpoints.
3. **No core change needed for the endpoints themselves.** A suffix after a
   sensitive segment (`"{payment_id}/complete"`), a bodiless POST and a required JSON
   body all worked with the existing grammar. `SquarePolicy`, limits, redaction and
   error typing applied unchanged. The declaration grew by 18 lines including docs; the rest of the
   cost was SDK error classification, models and documentation.
