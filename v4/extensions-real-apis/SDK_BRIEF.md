# Brief for the provider SDK authors

Binding sources: `contracts/SDK_INTEGRATIONS.md` (your provider's workflows and §2 common
obligations) and `contracts/DATA_AND_VALIDATION.md` §4–5 and §11 in the handoff directory
`/home/canarit/projects/concord_extensions_implementation_handoff/`. Public interfaces you consume:
`v4/extensions-real-apis/API_PLAN.md` (pagination, CSV, JSON Lines, streaming). This brief fixes
layout and conventions so five independently written SDKs compose.

## Packages (stubs exist; replace their contents)

- `examples/<provider>-sdk` — package `concord-example-<provider>-sdk`. The SDK: `concord::api!`
  declarations of the real provider operations, typed models (resource ids as newtypes, typed
  envelopes and errors; keep genuinely dynamic metadata dynamic), context types (provider
  version, auth, scopes such as repository/account/shop/org/namespace), and small typed wrappers
  that construct generated operations. Provider-specific interpretation (Link headers, GraphQL
  envelopes, Stripe `has_more`, Salesforce locators, Kubernetes `continue`) lives here, exposed as
  reusable adapters for `concord-pagination` (page factories / projections). It must NOT depend
  on test fixtures.
- `examples/<provider>-app` — package `concord-example-<provider>-app`. The external caller: the
  two required workflows as public library functions taking a configured SDK client and explicit
  inputs (budgets, filters, sinks), returning typed reports/outcomes. It depends only on the SDK's
  public exports, `concord`, `concord-pagination` and ordinary crates. Its integration tests
  (`tests/<id>_*.rs`, e.g. `tests/gh_a_activity_audit.rs`) run each workflow end to end against
  `concord-example-testserver` (dev-dependency) and assert the wire requests and outcomes. A
  binary per workflow (`src/bin/<id>.rs`) may run the same function against a live provider when
  credentials are present in the environment; without them it prints `NOT_RUN` and exits 0. Never
  perform provider writes or job creation without the explicit opt-in described in the contract.

Allowed dependencies: `concord` (path `../../crates/concord`, features as needed: `json`, `form`,
`csv`, `jsonl`), `concord-pagination` (path `../../crates/concord-pagination`), `serde` (derive),
`serde_json`, `bytes`, `futures-core`/`futures-util` (workspace) if you need stream adapters;
dev-dependencies `tokio` (workspace; macros, rt, rt-multi-thread, time, sync), `concord-example-testserver`
(path `../testserver`). Ask the coordinator before adding anything else (no chrono, no http client
other than reqwest through Concord).

## Conventions

- Rust 1.99.1, edition 2024, `[lints] workspace = true` (unsafe forbidden, missing_docs warn),
  `cargo clippy -p <pkg> --all-targets -- -D warnings`, `cargo fmt`. Every package must run at least
  one test on its own (`cargo test --locked -p <pkg>` is how the maintained gate runs it).
- Test fixtures live in `examples/<provider>-app/tests/fixtures/` with a `MANIFEST.json` listing,
  per fixture: workflow, operation, SDK source path, provider family/version, schema commit or
  digest (or the documentation URL and retrieval date when no schema exists), authoritative URL,
  provenance class (`official_example`, `schema_derived_synthetic`, `fault_injection`; never claim
  a capture you did not make), sha256 of the fixture bytes, wire assertions, expected outcome and
  the test command. Check expectations against primary documentation, not against your own
  client. No real credentials, account ids or business data: synthetic ids that still let a test
  detect scope/header leaks.
- Use `examples/testserver` for HTTP: its `TestServer::shutdown().await` must end every test that
  starts a server (observable cleanup); use `Body::Chunks/Generate/FailAfter/Stall` for streaming
  and failure timing; route provider host names to the server with
  `reqwest::Client::builder().resolve(host, addr)` when the SDK needs real host names (redirect
  host checks, signed download hosts).
- Two contexts in tests (two repositories / accounts / shops / orgs / namespaces) so a scope or
  header leak is detectable.
- Errors: wrap `concord::Error`, `concord::stream::{OpenError, ReadError}` and pagination errors in
  your SDK/app error types without stringifying them; `Display`/`Debug` must not print tokens,
  cursors, signed URLs or record contents (assert this with synthetic secrets).
- Pin the provider version you implement (header/path/query as the provider requires) and record
  it with sources in `examples/<provider>-sdk/PROVIDER.md` (version, schema/doc URLs and retrieval
  date, supported vs deliberately unsupported operations, documented discrepancies and how you
  resolved them).
- Record friction honestly in `examples/<provider>-sdk/FRICTION.md`: every escape hatch,
  handwritten request, repeated workaround or awkward API, with the provider contract, the public
  Concord path you used, the extra code, and whether it is intentional flexibility or friction.
  Do not edit `crates/concord-core`, `crates/concord-macros`, `crates/concord-extras` or
  `crates/concord-pagination`; propose library changes in FRICTION.md (with a reproduction) and
  tell the coordinator.
- Documented command per workflow in the app README: `cargo test -p concord-example-<p>-app --test <id>_...`.

## Reporting

Report: final commit SHA; operations implemented (count and list by resource family); both
workflows' public functions and outcome types; commands run with results; acceptance items of
your contract section mapped to test names; FRICTION.md highlights and proposed library fixes;
anything incomplete with reasons.
