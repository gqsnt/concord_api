# SDK34 QuickBooks Online maintenance

Date: **2026-11-07**. Live provider calls: **NOT_RUN**. All executed OAuth and Accounting
workflows use `examples/testserver::TestServer::start` on loopback. No commit was attempted.

## Prior exposure and observable sequence

No prior author participation or exposure is assumed beyond the supplied maintenance ticket.
The initial worktree was clean at `c89f1000939ae264da88eac2379b7ca9ee394be8`.

Before reading `FRICTION.md` or `FIRST_USE.md`, I inspected the root/package manifests, SDK
source (`lib`, `models`, `auth`, `policy`, `error`, `format`, `query`, `pages`), the existing
consumer/protocol/workflow tests and test support, the fixture-server source, README and local
PROVENANCE. I targeted the supplied official documentation URLs. An `AGENTS.md` filename
search found no applicable file. No campaign files, parent worktrees or other roles' briefs
were read; no agents or skills were used.

The first Cargo command was `CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-quickbooks-online
--offline`, without `--locked`: all **34** existing tests/doctests passed. The package was already
a workspace member and already present in the lockfile, so no workspace or lock edit was needed.
I implemented the payment models/endpoint and added the consumer and regression tests. The first
implementation build/test used `--offline --locked` and passed **40** tests/doctests. Only then
did I read `FRICTION.md` and `FIRST_USE.md`. Their request-ID and policy observations were used
for the improvement notes below, not to obtain the first implementation.

## Change and caller contract

The new generated call is `qbo.payments().create(NewPayment).send()`, under the existing
client → scope → endpoint hierarchy. `NewPayment::new(customer_id, invoice_id, amount)` builds
one positive amount allocated to one invoice. Partial payments retain exact decimal text.
The response exposes the recorded Payment. The caller reads `qbo.invoice(invoice_id).read()`
afterwards for the provider's balance; no client-side balance arithmetic or automatic second
operation was added. Existing customer and invoice declarations/models are unchanged.

Payment creation declares a default `requestid` and offers the generated `set_request_id`.
It uses the existing connection policy: one send; timeout/no response, 5xx or unreadable success
is uncertain with the operation, connection realm and sent request ID. A create 401 returns
authorization failure without refreshing/resending that create. Token acquisition failure
prevents any Accounting request. The same connection supplies both credential and realm path.

The [Payment reference](https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/payment)
was read through indexed official-page text on **2026-11-07**. The exact requested
[REST API features / requestid guidance](https://developer.intuit.com/app/developer/qbo/docs/develop/rest-api-features)
was attempted that day but only returned an application shell. Browser access was unavailable.
Its guidance could not be independently read; the mandatory request-ID and no-replay behavior
comes from the explicit ticket and the existing SDK contract. No replay retention period is
claimed. See PROVENANCE M1–M5 for all exact supplied URLs, access methods and evidence limits.

## Every location edited

All paths below are relative to `examples/quickbooks-online-sdk/`.

| File/location | Change |
|---|---|
| `src/payment.rs` (new) | Payment request constructor/serialization, response entity/envelope, payment line and linked transaction types. |
| `src/lib.rs`: module/exports, crate docs, entity limit docs, `QuickBooks::payments` declaration | Publish the models, declare `POST /payment` with mandatory default request ID, document generated usage. |
| `src/policy.rs`: module documentation only | State that the existing write policy serves invoices and payments. No policy code changed. |
| `tests/first_use.rs`: `record_a_partial_payment_then_read_the_invoice_balance` (new test) | Public consumer creates an invoice, records a partial payment and retrieves its reduced balance with the same realm/token. All original text/assertions remain. |
| `tests/payments.rs` (new) | Five focused regression tests: timeout/no replay, failure classifications, exact decimal wire text/fresh IDs, positive input restriction, rejected refresh isolation/recovery. |
| `fixtures/payment-created.json` (new) | Synthetic Payment response with one invoice allocation and decimal trailing zeros. |
| `fixtures/invoice-after-payment.json` (new) | Synthetic reduced balance, served by the consumer fixture only after it accepts the payment. |
| `Cargo.toml`: package description | Include the new supported payment slice; no dependency edit. |
| `README.md`: scope table, compiled caller example, payment contract, failures and exclusions | Show payment creation followed by existing invoice read; remove payments from the blanket exclusion, retain unsupported payment operations. |
| `PROVENANCE.md`: maintenance section | Dated exact citations, documentation-access limits, synthetic fixture derivation and unchanged dependencies/lock. Original research record retained. |
| `MAINTENANCE.md` (new) | This maintenance and verification record. |

Neither root `Cargo.toml` nor `Cargo.lock` changed. No shared crate, check, script or other example
was edited. `FRICTION.md`, `FIRST_USE.md`, the old protocol/workflow tests and support stayed intact.

## Facts repeated and omission detection

- Adding Payment required a module declaration and public exports as well as the endpoint;
  the public consumer and compiled README exercise those names. Package/crate descriptions,
  README scope/exclusions and provenance also needed the new capability named explicitly.
- Provider field spellings occur independently in request serialization, response deserialization,
  fixture JSON and expected request assertions. `TotalAmt` and line `Amount` are tied to one
  stored decimal in implementation; the consumer checks their wire text and the resulting balance.
- The request-ID default now appears on both invoice and payment create declarations. The
  single-send/failure classification is reused centrally, while its documented coverage needed
  updates in the policy comment and README.
- Synthetic IDs and amounts occur in the two new fixture files and consumer wire/result
  expectations. They are deliberately independent expectations rather than generated test data.

**No omitted edit produced a compiler or test failure.** Both the first implementation run and
the final checks passed. Thus this change provides no observed diagnostic for an accidentally
omitted export, request-ID declaration or renamed provider field. The tests do directly assert
that a timed-out payment has its sent request ID, a single invoice link and one POST; they also
exercise the generated public names. Documentation completeness remains a manual review duty.

## Remaining duplication and provider responsibilities

Generic mechanics still repeated locally: tiny reference/envelope wire types, request-ID
defaults per write, and token/fixture routing across test binaries. Payment has a distinct line
type because its allocation has `LinkedTxn` without the invoice line's required `DetailType`.
No new HTTP execution, retry, authentication, decimal codec or error-classification loop was
copied. The existing policy still parses `requestid` back out of the prepared URL and represents
it as optional on `UncertainWrite`, even though both exposed creates declare it.

QuickBooks-specific responsibilities remain in the SDK/application: entity field names and
envelopes, same-realm connections, OAuth callback validation and rotating/rejected refresh
tokens, request-ID persistence and deliberate reconciliation, and selection of customer/invoice
IDs. QuickBooks validates customer/invoice ownership, currency, invoice balance and accounting
rules. The application supplies the intended amount and reads the provider balance afterwards.
This bounded API excludes multicurrency, credits/refunds, multiple-invoice allocations, payment
read/update and automatic replay. A positive amount is an SDK restriction, not a statement that
the provider forbids all zero/other arrangements. The existing transport restrictions still
apply to application-supplied HTTP clients.

## Candidate Concord improvements (recorded only)

1. Expose typed declared request metadata to a send policy, including the replay/reconciliation
   key. That would remove parsing a generated query value back out of the URL and could make
   required write IDs explicit rather than optional in SDK uncertainty handling.
2. Provide a declaration recipe or validation for a required per-create request ID, so adding
   another write cannot silently omit it. Keep this opt-in: the provider's parameter name and
   replay policy belong to its SDK. Preserve client → scope → endpoint.
3. Improve generated endpoint documentation to label the shown production URL as the declared
   default; `connect` and fixture overrides determine the actual base. This agrees with the
   earlier consumer record; no wrong-environment request occurred here.

The existing rotation/cache and typed-Fault-boundary observations in `FRICTION.md` remain
candidate recipes/core work. This ticket needed no additional auth or decoder workaround.
None of these proposals was applied to the frozen shared baseline.

## Verification

All Cargo commands used `CARGO_BUILD_JOBS=4`, one at a time, and the worktree's default
`target/` with no target override. After the initial unlocked offline build, checks were locked.

- `cargo test -p concord-sdk-quickbooks-online --offline --locked`: **40 passed, 0 failed**
  (1 unit, 3 consumer, 5 payment regression, 21 protocol, 8 workflow, 2 doctests).
  The original 34 tests/doctests and all their assertions remain.
- Consumer evidence: invoice `250.0`, payment `25.1300`, invoice read balance `224.8700`;
  correct customer/invoice references, realm path, bearer credential, media and request ID.
- Regression evidence: timeout returns `QuickBooks.payments.create` uncertainty with the exact
  generated ID and one POST; 400/401/503/malformed-success creates each send once; a number
  beyond binary64 integer precision and its trailing zeros remain unquoted JSON number text;
  zero/negative amounts fail locally. A rejected A refresh sends no Accounting request, B
  continues with its own token/path, and replacing A's refresh token recovers A's existing client.
- Every new server is explicitly shut down and asserts `open_after == 0`.
- `cargo clippy -p concord-sdk-quickbooks-online --all-targets --offline --locked -- -D warnings`:
  passed with no warnings. Package formatting and `git diff --check` passed.
- Ownership checks: shared `crates/`, `checks/`, `scripts/` match frozen
  `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`; working changes are confined to this SDK.
  Root manifest and lockfile have no diff; no dependency/package version changed.

These fixtures verify the SDK contract, not live provider behavior. The unavailable requested
REST-features page remains a research limitation, explicitly retained in the provenance.
