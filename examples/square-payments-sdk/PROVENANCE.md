# Provenance

Author capsule: SDK33, frozen Concord baseline
`14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`. Implementation is left uncommitted
in the worktree. This is **example-assisted** implementation (documentation
excerpts and shared testserver). No existing
`examples/*-sdk` source file was consulted. Documentation excerpts from examples
and the shared testserver were consulted as listed below. No independent fresh
consumer or maintenance-role validation is claimed by this author capsule.

All external pages below were read **2026-11-07**, through read-only documentation
browsing. The reference pages displayed API version 2026-09-16. No Square service
request, sandbox or production, was sent. **Live provider calls: NOT_RUN.**

| Exact page | Facts used |
| --- | --- |
| https://developer.squareup.com/reference/square/payments-api | Provider slice, account credential context, endpoint inventory |
| https://developer.squareup.com/docs/build-basics/common-api-patterns/idempotency | Same key/body returns original payment; changed body/key reuse is an error; timeout ambiguity |
| https://developer.squareup.com/reference/square/payments-api/create-payment | POST route, required source/key, key limit, amount minor units, autocomplete, version/Bearer headers, response envelope, listed payment errors |
| https://developer.squareup.com/reference/square/payments-api/get-payment | GET path, payment envelope, permission |
| https://developer.squareup.com/reference/square/payments-api/list-payments | Account listing, main-location/time defaults, cursor, empty cursor completion, page limit and eventual consistency |
| https://developer.squareup.com/docs/build-basics/general-considerations/handling-errors | Error array and optional detail/field, authentication statuses/codes, 429 handling |
| https://developer.squareup.com/reference/square/enums/ErrorCode | `IDEMPOTENCY_KEY_REUSED` spelling and open error-code space |
| https://developer.squareup.com/reference/square/enums/ErrorCategory | Categories; subset relevant to payments and unknown-value preservation |

Fixtures are authored synthetic data grounded in these contracts, **not captured
provider responses** or expectations copied from this SDK's implementation.
Whitespace/field order are immaterial in the independent create-shape assertion;
the replay fixture separately compares exact transmitted bytes.

| Fixture | Derivation and deliberate changes |
| --- | --- |
| `create-request.json` | Synthetic minimal CreatePayment body following create reference fields; invented source/key; USD 1000; explicit autocomplete; excludes example's card-on-file and optional fields |
| `payment.json` | Synthetic create/get envelope following both endpoint references; invented ID; retained amount/status; extra source_type exercises ignored fields |
| `page-1.json`, `page-2.json` | Synthetic ListPayments envelopes; invented IDs and opaque punctuation cursor; second page has empty cursor and FAILED status |
| `empty-next.json`, `empty-final.json` | Synthetic listing edge cases from optional payments/cursor contract; empty intermediate and absent final cursor |
| `mismatch.json` | Synthetic error array from idempotency guide and ErrorCode; 400/category/detail/field are fixture choices, not an assertion that every mismatch has precisely that status/detail |
| `faults.json` | Authored adversarial bodies: invalid syntax, incomplete JSON, wrong shape, empty body, wrong/missing media and unsupported charset; paired with 200/503 in protocol tests |
| `error-cases.tsv` | All 36 status/code pairs in CreatePayment's displayed error descriptions, plus common authentication, not-found, idempotency, rate, 5xx and unknown-error cases; category assignments are synthetic choices unless supported by the common-error documentation; no exhaustive category inference from code |
| Runtime protocol recipes | Synthetic oversized full/chunked bodies (`limit + 1` spaces), incomplete chunked transport, stalled wrong-media body, duplicate media header, redirect, multiple secret-echoing errors and a page failing with 429; constructed with shared Reply/Body APIs |
| Runtime workflow ledger | Synthetic in-memory commit-before-timeout, exact-byte replay returns original payment; mismatch refuses and leaves committed body unchanged; first head is held by a Gate |

The shared server records actual HTTP/1.1 requests and supports response failures;
its presence establishes no live Square behavior. TLS, HTTP/2, real credentials,
provider latency, account permissions and Square's actual idempotency retention
period have not been tested. No retention period is assumed.

Local reading inventory (all at the frozen baseline, 2026-11-07):

- Parent `AGENTS.md`, `CLAUDE.md`, `START_HERE.md`, `DESIGN_CHARTER.md`,
  `PROMPT.md`, `AUTHORITY.md`, `contracts/PHASES.md`, `contracts/RESOURCES.md`:
  bounded ownership, frozen core, evidence and build discipline. No in-worktree
  AGENTS/CLAUDE file or narrower examples instruction was present.
- Root `README.md`; `docs/QUICKSTART.md`, `AUTHOR_GUIDE.md`, `CHEATSHEET.md`,
  `RECIPES.md`: declaration, generated send policy, limits, delivery, redaction,
  pagination, and reqwest boundary. These contain embedded example snippets;
  consulted excerpts and takeaways are detailed in FRICTION.md.
- `Cargo.toml`, `Cargo.lock`; `crates/{concord,concord-core,concord-extras}/Cargo.toml`:
  dependency versions/features and package policy.
- `crates/{concord-core,concord-auth,concord-retry,concord-ratelimit,concord-pagination,concord-codecs}/README.md`:
  evaluated every requested shared capability before selecting dependencies.
- Rustdoc/source: `crates/concord/src/lib.rs`; `concord-core/src/{lib,prepared,error,response,codec}.rs`;
  `concord-auth/src/{lib,credentials}.rs`; `concord-pagination/src/{lib,pages,types}.rs`;
  `concord-codecs/src/json.rs`: Endpoint/SendWith, bounded document/decoding,
  response metadata, delivery and error disclosure, Bearer, Pages/Resume, strict JSON.
- `examples/testserver/Cargo.toml`, `examples/testserver/src/lib.rs`:
  `TestServer::start(|request: &Recorded| -> Reply)`, wire recording, Script,
  Gate, full/chunked/truncated/stalled bodies and explicit shutdown.
- Environment runtime skill read at `/home/canarit/.codex/plugins/cache/openai-curated-remote/cloud-environment/0.2.0/skills/runtime/SKILL.md`;
  no environment-status tool was available; no environment changes made.

Dependencies: no new registry crate, version, checksum or transitive resolution.
All dependencies were already in Cargo.lock. Existing workspace path crates are
MIT OR Apache-2.0. The sole new lock package is this SDK, version 0.0.0, also
MIT OR Apache-2.0. Initial `cargo build --offline` wrote that entry; later gates
use `--locked`. Only its workspace membership and lock entry change outside
this directory.

## Maintenance: delayed capture (independent maintainer, 2026-11-07)

Added by the maintainer role, not the author. The pages below were read
**2026-11-07** through a read-only fetch tool that summarises pages; every page
displayed API version 2026-09-16. No Square request was sent. **Live provider
calls: NOT_RUN.**

| Exact page | Facts used |
| --- | --- |
| https://developer.squareup.com/reference/square/payments-api/complete-payment | `POST /v2/payments/{payment_id}/complete`; completes an `APPROVED` payment; optional Beta `version_token` ("Used for optimistic concurrency"); a different server version fails with a `VERSION_MISMATCH` error; no status stated; example body `{}`; `PAYMENTS_WRITE` |
| https://developer.squareup.com/reference/square/payments-api/cancel-payment | `POST /v2/payments/{payment_id}/cancel`; voids an `APPROVED` payment; no request body fields; example status `CANCELED`; `PAYMENTS_WRITE` |
| https://developer.squareup.com/docs/payments-api/take-payments/card-payments/delayed-capture | `autocomplete: false` → `APPROVED`; complete → `COMPLETED`; cancel → `CANCELED`; complete "succeeds only if the payment hasn't been updated since that `version_token` was generated"; default window 36 h card-present / 7 days card-not-present; default `delay_action` CANCEL; no timeout or idempotency guidance for complete/cancel |
| https://developer.squareup.com/reference/square/payments-api/create-payment | `autocomplete` default true, false holds the payment "in an approved state"; `delay_duration`/`delay_action` only with `autocomplete=false` (not exposed) |
| https://developer.squareup.com/reference/square/objects/Payment | `version_token` string; status values APPROVED, PENDING, COMPLETED, CANCELED, FAILED; delay fields |
| https://developer.squareup.com/reference/square/payments-api | Endpoint inventory, including CompletePayment, CancelPayment, UpdatePayment and CancelPaymentByIdempotencyKey (the last two excluded) |
| https://developer.squareup.com/docs/build-basics/common-api-patterns/idempotency | Re-read: names no endpoint that lacks a key and does not mention complete/cancel; behaviour "might vary depending on the API" |
| https://developer.squareup.com/reference/square/enums/ErrorCode | `DELAYED_TRANSACTION_{EXPIRED,CANCELED,CAPTURED,FAILED}` present; `VERSION_MISMATCH` **absent** from the enum page; no statuses |
| https://developer.squareup.com/docs/build-basics/common-api-patterns/optimistic-concurrency | A stale update "fails with a `VERSION_MISMATCH` error"; caller re-retrieves and updates; no status |
| https://developer.squareup.com/docs/build-basics/general-considerations/handling-errors | Re-read: no status for VERSION_MISMATCH/CONFLICT, no timeout guidance |

| New fixture | Derivation and deliberate choices |
| --- | --- |
| `authorization-request.json` | `create-request.json` with `autocomplete: false`; same invented key/source/amount |
| `authorized-payment.json` | Synthetic envelope from the Payment object and delayed-capture guide: `APPROVED`, invented `version_token`, `delay_action`/`delay_duration` (ignored by the model), extra `source_type` |
| `version-mismatch.json` | Synthetic error document using the documented `VERSION_MISMATCH` code. Category, `field`, detail and the 400 status used in tests are fixture choices; Square states no status, so a 409 variant is also exercised |
| Runtime stand-in in `tests/delayed_capture.rs` | Authored in-memory model: new token per change, `DELAYED_TRANSACTION_CAPTURED`/`_CANCELED` 400 answers for finished payments, and status checked before token. These orderings and statuses are fixture choices, not observed provider behaviour |

Dependencies: none added; root `Cargo.toml` and `Cargo.lock` unchanged by
maintenance (the package was already a member).
