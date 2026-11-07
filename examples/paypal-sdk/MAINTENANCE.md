# SDK32 PayPal maintenance — 2026-11-07

Added authorize-then-capture and authorization release within the existing client → scope →
endpoint hierarchy. Live provider calls: **NOT_RUN**. All executable workflows use loopback
`examples/testserver` fixtures; documentation research was read-only. No commit was made.

## Prior exposure and read order

This maintenance pass began with the ticket and no prior implementation work on this SDK.
Before the first implementation attempt, read the root/package manifests, package source
(`src/lib.rs`, `models.rs`, `policy.rs`, `auth.rs`, `error.rs`), README, PROVENANCE, existing
`tests/first_use.rs` and test support; inspected protocol/workflow test names and relevant
Concord codec/response/operation APIs. `rg` found no applicable `AGENTS.md` inside the worktree.
No campaign or parent-directory files were opened and no other SDK implementation was read.

The original 39-test suite passed before changes. Implemented the endpoints/models/policy and
five new consumer/regression tests, corrected a compiler diagnostic, and obtained a passing
44-test suite **before** opening `FRICTION.md` or `FIRST_USE.md`. Read those two historical
reports only afterwards, then extended failure coverage and the public documentation. Their
original exposure records describe earlier work; they are not this maintainer's read history.

Other source reads were confined to the worktree: Concord's `concord-core` response, codec,
prepared/error/name APIs, `concord-codecs` JSON/ignore implementations, and a setter search in
`concord-macros`. An attempted read of `concord-core/src/format.rs` and root
`.cargo/config.toml` found neither file. No shared source was edited.

## Provider evidence

Read **2026-11-07**:

- [Authentication](https://developer.paypal.com/api/rest/authentication),
  [API requests](https://developer.paypal.com/api/rest/requests/),
  [rate limiting](https://developer.paypal.com/api/rest/reference/rate-limiting/), and
  [Orders integration](https://developer.paypal.com/api/rest/integration/orders-api).
  Preserve client-credentials reacquisition/cache behavior, endpoint-specific request IDs,
  and caller handling of 429 without inventing a quota.
- The requested [Orders v2 reference](https://developer.paypal.com/docs/api/orders/v2/) and
  [Payments v2 reference](https://developer.paypal.com/docs/api/payments/v2/), which redirected
  to their `/api/` indexes.
- [Create order](https://developer.paypal.com/api/orders/v2/orders-create): `AUTHORIZE`
  permits only one purchase unit.
- [Authorize payment for order](https://developer.paypal.com/api/orders/v2/orders-authorize):
  the approved order's `/authorize` response contains nested authorizations with IDs/statuses;
  the request supports `PayPal-Request-Id`.
- [Capture authorized payment](https://developer.paypal.com/api/payments/v2/authorizations-capture):
  authorization capture supports request IDs, optional amount and final-capture flag.
- [Void authorized payment](https://developer.paypal.com/api/payments/v2/authorizations-void):
  the current page also documents request IDs, no request body, and success 200/204.

All three new writes therefore participate in the existing retry rule only with a supplied
ID. Repeated attempts clone the prepared method, target, body and ID. Unresolved timeouts
remain uncertain, including when the retry budget ends or a later attempt refuses the write.
No Payments v2 retention period is inferred from Orders v2's retention. More detailed
attribution and synthetic fixture derivations are in [PROVENANCE.md](PROVENANCE.md).

## Locations changed

| File | Locations and purpose |
|---|---|
| [Cargo.toml](Cargo.toml) | Package description includes authorization operations; dependencies unchanged. |
| [src/lib.rs](src/lib.rs) | Public model exports; `PayPal.order.authorize` and `get_authorized`; new `PayPal.authorization` scope with `capture` and `void`; client/module/Builder docs. |
| [src/models.rs](src/models.rs) | `OrderRequest::authorize`; `AuthorizeRequest`; `AuthorizationCaptureRequest`; `AuthorizationId`, `AuthorizationStatus`, `Authorization`; separate `AuthorizedOrder`, `AuthorizedPurchaseUnit`, `AuthorizedPayments` views and iterators; affected intent/request-ID/model docs. |
| [src/policy.rs](src/policy.rs) | Imports and `accepts_request_id` add exactly the three new generated endpoint identities; Session docs. Token/retry execution and classification algorithms unchanged. |
| [src/error.rs](src/error.rs) | `Uncertain`, `Uncertain::operation`, and `Error::Uncertain` docs now cover authorize and void. Error representation/classification unchanged. |
| [tests/authorizations.rs](tests/authorizations.rs) | New public consumer and focused wire/failure/input regressions; inline synthetic authorization responses and existing fixture reuse. |
| [README.md](README.md) | New call examples, retry/uncertainty contract, response handling and revised exclusions. Both code blocks compile as doctests. |
| [PROVENANCE.md](PROVENANCE.md) | Current supported-write/representation rules, exact new references/read date, fixture derivation and dependency/lock record. Original research entries remain historical. |
| [MAINTENANCE.md](MAINTENANCE.md) | This maintenance record. |

Root workspace membership and the SDK's lock entry already existed. Neither root `Cargo.toml`
nor `Cargo.lock` needed an edit. No new crate, version or license was introduced. No shared
crate, check, other example or script changed. `tests/first_use.rs`, all other existing tests,
and all their assertions remain unchanged.

Facts repeated across locations: endpoint request-ID support lives in both the declaration's
optional header and the policy's separate operation allowlist, plus README/rustdoc/provenance;
response nesting lives in models and synthetic test bodies; supported intent/unit cardinality
lives in the constructor, tests and docs; supported scope/exclusions and uncertain writes
appear in the manifest, module/error docs and README. Void's no-body acknowledgement must
agree across its no-`->` declaration, tests and documentation.

## Observable diagnostics and omission check

The first new-test compile failed with E0599: the test helper tried `.request_id(id)` on each
new generated endpoint. The diagnostic identified the existing `.set_request_id` setter.
Corrected the helper to apply that setter only for `Some(id)`; production endpoint declarations
and model exports compiled without changes for this diagnostic.

To exercise an omitted edit, temporarily removed only authorization capture from
`accepts_request_id`, then ran the exact timeout regression. Compilation succeeded (unused
imports warned), but the test failed: `PayPal.authorization.capture` returned uncertain after
one timeout despite its supplied key, where the test required the permitted replay to resolve
the outcome. Restored the original policy in a `finally` block before subsequent checks.
Thus the compiler does not ensure the declaration and replay permission agree; the regression
catches that omission. A later text search also found stale create/capture-only error docs;
those required manual updates in `src/error.rs`.

The API review found that adding a public authorization field to existing `Payments` would
break struct literals. Replaced that draft with a separate `AuthorizedOrder` view and added
`get_authorized()` for read-only reconciliation. A consumer compile assertion constructs the
original `Order`, `PurchaseUnit` and `Payments` shapes without any new fields.

## Verification

All cargo commands used `CARGO_BUILD_JOBS=4`, one at a time, with the default worktree `target/`.
`CARGO_TARGET_DIR` was never set.

- Initial required unlocked build/test: `cargo test -p concord-sdk-paypal --offline` —
  **39 passed**, lockfile unchanged.
- Final suite: `cargo test -p concord-sdk-paypal --offline --locked` — **48 passed**:
  3 unit, 8 new authorization tests, 2 unchanged first-use, 22 unchanged protocol,
  11 unchanged workflow, and 2 README compile doctests. No warnings.
- `cargo clippy -p concord-sdk-paypal --all-targets --offline --locked -- -D warnings` — passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc -p concord-sdk-paypal --no-deps --offline --locked` — passed.
- `rustfmt --edition 2024 --config skip_children=true --check` on all five changed Rust files,
  and `git diff --check` — passed.
- Frozen-baseline diff on `crates/`, `checks/`, `scripts/` — empty. Existing tests, historical
  first-use/friction reports, root manifest and lockfile — unchanged.

The new consumer creates an AUTHORIZE order, obtains APPROVED state, receives authorization
ID/status, captures and retrieves the capture; a second synthetic order is authorized and
voided. It checks exact paths, request bodies, authentication, representation preference and
distinct logical IDs. The failure matrix covers all three writes with no-ID timeouts,
resolved/exhausted keyed timeouts, 5xx exhaustion, later refusals, and non-retried 422/429;
typed writes also exercise unreadable successful JSON. Void accepts 200 and empty 204.
Unknown authorization statuses remain available; input bounds and encoded authorization paths
are checked. An uncertain authorize is reconciled with `get_authorized()` and no second write.
The original tests still exercise expiry/401 reacquisition without refresh tokens.

## Remaining responsibilities and Concord candidates

PayPal-specific responsibilities remain here: the operation allowlist and retention limits,
single-unit AUTHORIZE intent, independent order/authorization/capture statuses, amount/final
capture semantics, uncertainty and reconciliation, and 429 handling. Buyer approval, refunds,
payouts, reauthorization and authorization-detail retrieval remain outside this slice. A void
acknowledges success without returning or validating a JSON representation. A completed order
with an authorization is not proof of captured funds.

The CAPTURE constructors, request bytes, endpoints, response struct fields and `captures()`
behavior are unchanged. The separate authorization view keeps existing public struct literals
source-compatible. `get_authorized()` calls the same Show order details path as `get()` and
decodes that view without changing the existing read's return type.

Generic duplication remains: the new ID repeats CaptureId's validation/Display/Debug pattern;
the authorization order view repeats the order envelope and purchase-unit fields to preserve
the original public response shapes;
optional request-ID headers and representation preferences repeat across endpoints; policy
reads a typed ID back from a header; asynchronous token acquisition still needs manual
request cloning, attempt wrappers and cancellation/ambiguity bookkeeping. The separate OAuth
declaration/base and public generated construction surface also remain as recorded in FRICTION.

Candidate shared changes, recorded only:

1. Expose SDK-defined endpoint metadata through `Operation` so retry permission can live beside
   its declaration. Keep interpretation and provider permission in the SDK. The omission probe
   demonstrates the current split directly.
2. Add an asynchronous before-attempt hook to the retry adapter, carrying acquisition failure
   and token generation to classification, to remove repeated request-template plumbing.
3. Expose completed/cancelled attempt accounting in retry reports instead of requiring SDKs
   to infer an in-flight cancellation from separate counters.
4. Support initialized constant request bodies so callers need not supply a new empty request
   object for each authorize/order-capture call. The existing no-output declaration already
   handles void's acknowledgement without a custom codec.
