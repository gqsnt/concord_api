# SDK31 maintenance — manual capture

Role: independent maintainer, running as an Agent-tool subagent; I did not write this SDK.
The harness system prompt names model ID `claude-opus-5-5`. The requested model and the
requested or observed effort were not visible to this run. Date: 2026-11-07. Baseline: author
commit `677bfd7f` plus consumer commit `5dc0247c` on frozen Concord `14b9e1e0`.
**Live provider calls: NOT_RUN.**

## Prior exposure (what I read before starting)

Read before and during implementation, in this order:

1. The role brief (`work/roles/w3-sdk31-maintainer-opus/prompt.md`). The harness also injected
   the user's and project's `CLAUDE.md` and the memory index. I did not open `START_HERE.md`,
   the other handoff files, other roles' briefs or records, or other `work/w3-*` worktrees.
2. This SDK: `README.md`, `Cargo.toml`, `PROVENANCE.md`, all of `src/`, all of `tests/`
   (including `first_use.rs`, read as code), the fixture list, and `authorised.json` and
   `error.json`.
3. Concord docs: `docs/CHEATSHEET.md`, and `docs/AUTHOR_GUIDE.md` §1, §3, §5.12–5.13 and §7–11.
   Excerpts of `examples/testserver/src/lib.rs` (Recorded, Reply, Script). Lines ~390–410 of
   `crates/concord-core/src/error.rs` (a reqwest timeout is a Transport error), and later the
   `Endpoint` and `Operation` definitions.
4. One stray read: a `grep` over `examples/stripe-sdk/src/*.rs` for path syntax. It matched
   one unrelated line in `pages.rs`. Nothing was taken from it.
5. Adyen pages, all read 2026-11-07 (PROVENANCE.md M1, M2, P2):
   - captures reference: <https://docs.adyen.com/api-explorer/Checkout/71/post/payments/(paymentPspReference)/captures>
   - capture guide: <https://docs.adyen.com/online-payments/capture>
   - idempotency, re-read: <https://docs.adyen.com/development-resources/api-idempotency/>

I opened `FRICTION.md` and `FIRST_USE.md` only after the ticket was implemented and its tests
passed. They did not change the design. Several first-use findings now also apply to captures:
see "Inherited behaviour" below.

## What changed

Callers can now request manual capture of an authorised payment:

```text
checkout.v71().payment(psp_reference).captures(key, CaptureRequest { merchant_account, amount, reference })
    .send().await -> Result<Response<CaptureReceived>, Error>
```

This sends `POST /v71/payments/{paymentPspReference}/captures` with `x-api-key`, the
`Idempotency-Key` and a JSON body. The body holds `merchantAccount` and `amount`, plus
`reference` when one is given (M1). The payment reference lives on a new scope `payment`, so it
sits apart from the idempotency key at the call site. A flat `captures(psp, key, request)` call
would take two `String`s in a row, and swapping them still compiles (AUTHOR_GUIDE §11: "silently
swaps the values"). The scope also gives refunds, cancels and amount updates a natural place
later. The reference is `#[sensitive]`: Concord withholds it from the generated `Debug` of the
scope, endpoint, prepared request and errors, and tests check this. It is percent-encoded as
one path segment, and `""`, `.` and `..` are Build errors before anything is sent.

`CaptureReceived` is the typed result. It holds `psp_reference` (the capture's own reference),
`payment_psp_reference`, `requested: Amount`, `merchant_account`, `reference` and the unknown
`fields`. It has no state, status or outcome field. Its rustdoc says it is not a capture
outcome: money is not shown to move, and the result arrives later by webhook, which is out of
scope. Two checks apply before a caller gets one:

1. **Decoding** (`serde(try_from)`). The document must say `status: "received"`; M1 says it
   "will always have the value received". It must also contain `pspReference`,
   `paymentPspReference`, `amount` and `merchantAccount`. If not, decoding fails, and the policy
   reports `Reconciliation { UncertainDelivery, cause: Decode }`, the same as any unreadable
   success document. The key is not revoked.
2. **Confirmation** (the new `Confirm` trait, run by the policy after decoding). The document
   must echo exactly what was sent: the payment reference in the request path, plus the merchant
   account, amount and reference from the sent body bytes. A reference must be absent when none
   was sent. If any differs, the policy revokes the key and reports `GuaranteeNotConfirmed`,
   keeping the `201` head. The counterexample behind this check: P2 does not say what Adyen
   returns when a key is reused with a different body. Keys are scoped to the whole company, and
   this SDK's ledger is process-local. So another process could have used the key for a
   different capture. Without this check, the SDK would label that other capture's amount as
   "the amount requested". The mutation tests below show the check is needed: with it disabled,
   these cases return a receipt.

Captures share the existing ledger and retry rules, with no change to them:

- a retry reuses the key with identical bytes;
- a changed amount (or any change) under a used key is `ChangedRequest` before sending. That
  includes reusing the payment's own key, because the route differs;
- a capture that misses the total deadline is `Deadline` and is not replayed automatically. A
  later identical replay by the caller is allowed while the guarantee holds;
- a transport failure before any head is replayed automatically with the same key and bytes,
  which is the existing guarantee rule;
- once the guarantee window expires, the key is refused.

Payment methods, payments and details are unchanged for existing callers. Their generated API
and models are the same. Their `Confirm` impls return `true`, so the only effect of the new
check is the trait bound. All 13 first-use, 13 protocol and 11 workflow tests pass unedited.

Manual capture itself is configured on the merchant account in the Customer Area. M2 says it
can also be set per payment, but the fetched page did not show the field name. Adding a field to
`PaymentRequest` would break exhaustive struct literals (AUTHOR_GUIDE §11), so the payment
request is unchanged and the README says so.

## Files and locations edited

| File | Change |
| --- | --- |
| `src/lib.rs` | Crate doc line 1. New `scope payment(#[sensitive] psp_reference)` with path `payments/{psp_reference}` and `POST captures` (path `captures`, output `CaptureReceived`) inside `scope v71`. `Confirm` re-exported. |
| `src/models.rs` | `CaptureRequest` (Serialize + Deserialize). `CaptureReceived` with private `CaptureDocument` and `TryFrom` (status check). Both added to `redacted_debug!`. |
| `src/policy.rs` | `Confirm` trait and three impls. `SendWith<T>` bound now `T: Send + Confirm`. After decoding: confirm, otherwise revoke and return `GuaranteeNotConfirmed`. |
| `src/error.rs` | `GuaranteeNotConfirmed` doc now also covers a document describing a different request. |
| `README.md` | Intro, public-shape line, new "Manual capture" section (with a compiled `no_run` doctest), excluded list. |
| `PROVENANCE.md` | Two fixture rows; maintenance sources M1/M2/P2 appended. The author's record is untouched. |
| `fixtures/capture-request.json`, `fixtures/capture-received.json` | New synthetic fixtures (schema M1). |
| `tests/capture.rs` | New: 5 regression tests and 1 consumer test. |
| `MAINTENANCE.md` | This file. |

Not edited: `Cargo.toml` and the root `Cargo.toml`/`Cargo.lock` (the package was already a
member and locked; no new crate), `tests/first_use.rs`, `tests/protocol.rs`,
`tests/workflows.rs`, `tests/common/mod.rs`, and every shared crate, check, script and other
example.

### Facts written in more than one place

- **Which echoed fields identify a capture.** The code is in one place
  (`Confirm for CaptureReceived`). The list is also written in prose in four places: the
  `CaptureReceived` rustdoc, the `Confirm` impl doc, the `GuaranteeNotConfirmed` doc and the
  README. Only the code is tested.
- **The route shape.** It is declared in `lib.rs` (`payments/{psp_reference}` + `captures`),
  and `Confirm` parses it again from the native URL (`rsplit('/')`: `captures`, then the
  reference). The policy sees only the built request: `Endpoint` exposes just `prepare()` and
  `operation()`, not declared values. If the path changed, the wire-contract test would fail;
  the compiler would not.
- **Wire field names of the capture body.** These are written once, in `CaptureRequest`'s serde
  attributes. `Confirm` reuses that type to parse the sent bytes, so the names are not spelled
  twice. The fixture spells them independently, on purpose.
- **Response field names.** These are in the private `CaptureDocument` only.
- **The slice description.** It appears in the `lib.rs` crate doc, the README intro, the
  public-shape line and the excluded list. These are prose only and nothing checks them.

### Did the compiler or the tests catch an omitted edit?

Observed by temporary edits, each restored byte-identical (`cmp`):

- **Omitting `impl Confirm for CaptureReceived`.** The compiler caught it: E0277 "the trait bound
  `models::CaptureReceived: Confirm` is not satisfied". The error points at the client's
  `send policy -> Result<Response<_>, Error>;` (lib.rs:37), not at the `captures` endpoint. A
  note gives the bound's origin (`impl<T: Send + Confirm> SendWith<T>`), and the error is
  "originates in the macro". So the compiler caught the omission, but it does not name the
  endpoint (log: `target/maint-logs/omit-confirm.log`).
- **Four behaviour mutations.** Each was caught by at least one test in `capture.rs`, and the
  rest of that file still passed:
  - `confirms` always true: 2 tests failed (mismatch, path-segment);
  - status unchecked: 1 failed (not-a-receipt);
  - no revoke on mismatch: 1 failed;
  - amount not compared: 1 failed.
- **A hypothetical `payment(..).methods` endpoint.** The macro refused it: "generated type name
  `CheckoutV71PaymentMethods` is used twice", naming both origins (the existing
  `payment_methods`). This is a concatenation hazard of the chosen scope name, and Concord
  reports it well.
- **Not exercised.** If `CaptureReceived` were left out of `redacted_debug!`, it would have no
  `Debug`, and only the tests that format it would fail to compile. A `#[derive(Debug)]`
  instead would compile and leak, and only the redaction assertions would catch it.

## Remaining generic duplication

- `#[sensitive] header("idempotency-key") key: String` now appears on four endpoints. Parameter
  groups (`params`) hold optional or initialized values, and this one is required and positional
  per request. That is acceptable at four endpoints.
- `Confirm` needs one impl per output type. Two of them are explicit `true`. I chose no default
  method, so each new output has to decide its echo rule; the compiler then forces the impl.
- The route is re-parsed from the native URL (see above).
- Test helpers are repeated across test files (`retry_policy` and the echoing reply builders in
  `workflows.rs`, `first_use.rs` and `capture.rs`).

## Provider-specific responsibilities (kept in the SDK)

- What `received` means, and refusing to claim a capture outcome in the type.
- Which echoed fields identify a capture, failing closed when they don't match, and revoking the
  key in that case.
- Applying Adyen's company-wide key scope: captures share the payment ledger.
- Partial capture, the authorised-amount ceiling, matching currency, authorisation expiry, and
  enabling multiple partial captures. Adyen validates all of these (M1, M2), and the SDK
  validates none of them locally.
- Redacting PSP references.
- The asynchronous outcome (CAPTURE / CAPTURE_FAILED webhooks) and accounting stay with the
  application.

## Inherited behaviour that now also applies to captures (from FIRST_USE.md)

- A 703 that persists through every retry revokes the key (first-use item 8).
- An exhausted 5xx is `Error::Provider`, not reconciliation (item 11). A capture that got a
  `500` may still have been accepted.
- `Display` says "payment requires reconciliation" (item 6).

I did not change these: the ticket requires the same rules as a payment.

## Candidate improvements (recorded, not implemented)

Concord (shared crates were not edited):

1. **Put the diagnostic at the endpoint.** When a send policy's `SendWith<T>` has a bound that
   one endpoint's output lacks, E0277 points at the client `send` template. A per-endpoint
   assertion generated with the endpoint's span would name the endpoint that needs the impl.
2. **A recipe for checking a response against its request.** A bound on the policy's `T` plus a
   per-output trait works today, needs no core change, and the compiler enforces it. AUTHOR_GUIDE
   §5.13 could show it beside the signing example.
3. **Let a policy read declared values.** `Endpoint` gives a policy only `prepare()` and
   `operation()`. An accessor for declared path values (redacted when `#[sensitive]`) would
   remove the URL re-parse in `Confirm`. Weigh it against keeping `Endpoint` minimal; the
   re-parse is small, and the tests cover it.

SDK, for its next breaking release:

4. A dedicated reason (for example `ResponseMismatch`) would be clearer than reusing
   `GuaranteeNotConfirmed`. `ReconciliationReason` is not `#[non_exhaustive]`, though, so adding
   a variant would break exhaustive matches in existing payment callers. I reused the variant
   (the caller's handling is the same: reconcile, never replay this key) and documented it.
   `Reconciliation` also has no place to keep the mismatched document, so the caller reconciles
   from its own records and the retained head.
5. A per-payment manual-capture field needs a builder or `#[non_exhaustive]` evolution rule for
   `PaymentRequest` first.

Not verified, because there were no live calls:

- whether every `201` actually carries `merchantAccount` and echoes `reference`. If not,
  captures fail closed as `UncertainDelivery` or `GuaranteeNotConfirmed`;
- what Adyen returns for a reused key with a different body;
- whether `transient-error` and 703/704 behave on captures as P2 describes for `/payments`. P2
  says only "POST requests", and M1 documents the header.

## Verification (2026-11-07, worktree `target/`, `CARGO_BUILD_JOBS=4`, sequential)

| Command | Observed |
| --- | --- |
| baseline `cargo test --locked --offline -p concord-sdk-adyen-checkout` (before edits) | PASS: first_use 13, protocol 13, workflows 11, doctest 1 |
| `cargo test --locked -p concord-sdk-adyen-checkout` (final) | PASS: capture 6, first_use 13, protocol 13, workflows 11, doctests 2 |
| `cargo test --locked -p concord-sdk-adyen-checkout --test capture`, repeated 8× | 8/8 PASS. The 300 ms consumer deadline did not flake |
| `cargo clippy --locked -p concord-sdk-adyen-checkout --all-targets -- -D warnings` | PASS |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-adyen-checkout --no-deps` | PASS. The generated `send()` reads `Result<Response<CaptureReceived>, Error>` |
| `cargo fmt -p concord-sdk-adyen-checkout -- --check`, `git diff --check` | PASS (after rustfmt on the new test file) |
| `Cargo.toml` / `Cargo.lock` diff | none |
