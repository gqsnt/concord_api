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
