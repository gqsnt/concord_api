# SDK16 Snowflake SQL API — SDK author (large slice)

Requested runner: Codex gpt-6-astra (xhigh). You implement this SDK from scratch.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree whose Concord library is frozen at commit `0983a349`; its `HEAD` may
  also contain this SDK's own earlier role commits (author, consumer), which is expected. Prefix
  every cargo command
  with `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b` (a shared build cache; if cargo says it is
  waiting for a lock, wait).
- **Frozen library:** do not edit `crates/`, `scripts/`, `docs/`, `checks/` or other examples.
  If Concord forces a workaround (private reach-through, rebuilt execution, handwritten operation
  names, a native escape you think should not be needed), keep the working code and record it as
  a friction entry with a reproduction; do not patch the library.
- Use Concord's public API (`concord`, `concord-pagination`, `concord-extras` through the facade)
  and its docs: `README.md`, `docs/*.md`, crate rustdoc (`cargo doc -p concord --no-deps
  --features csv,jsonl`), `crates/*/README.md`. The five existing provider SDKs under `examples/`
  are normal reading material (say so if one supplied your solution).
- No live provider calls, no credentials, no secrets: all auth values are synthetic and must not
  look like real credentials. Tests run offline against the loopback server in
  `examples/testserver` (a dev-dependency by path).
- Fixture bytes are hand-authored from the official documentation unless you actually obtained a
  documented example; record provenance honestly. Put byte-sensitive fixtures under
  `examples/snowflake-sdk/fixtures/` and keep `examples/snowflake-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.

## The locked provider brief

### 16. Snowflake SQL API — large

Official: https://docs.snowflake.com/en/developer-guide/sql-api/submitting-requests, https://docs.snowflake.com/en/developer-guide/sql-api/handling-responses, and https://docs.snowflake.com/en/developer-guide/sql-api/cancelling-requests.

- Workflows: (1) submit an asynchronous statement, poll its handle until success, use its first result response for partition discovery and consume that first partition without refetching it, then fetch remaining independently addressable partitions with bounded concurrent fanout; (2) submit a second long-running statement, explicitly invoke the server cancel endpoint and observe a documented terminal outcome without conflating local cancellation and remote cancellation.
- Professional bounded coverage: submit, query status, retrieve partition and cancel operations, statement handle, stable request context, pagination and typed server errors, virtual-clock bounded polling and cancellation races. Support immediate completion and success with one or several partitions; preserve server-determined partition metadata and allow callers to stop. Prove from returned metadata and the documented indexed partition requests that each later partition is independently fetchable; do not infer a valid parallel plan merely from a count. Bound concurrency, correlate each partition and report missing or failed partitions.
- Perturbation: insert a pending response, add a second result partition, and race cancellation against completion; verify handle correlation and report actual resulting state, not assumed successful cancellation.
- Feasibility: deterministic fake state machine with a virtual clock. Live requires a warehouse, account, credentials and budget. Exclude SQL semantics, multi-statement execution, warehouse administration and ingestion.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://docs.snowflake.com/en/developer-guide/sql-api/submitting-requests
- https://docs.snowflake.com/en/developer-guide/sql-api/handling-responses
- https://docs.snowflake.com/en/developer-guide/sql-api/cancelling-requests

Machine-readable scope: submit/status/result partitions/cancel; bounded independent fanout proved by metadata. Excluded: SQL semantics; multi-statement; warehouse administration.
Workflows: A: Submit async statement, reuse first result and discover then fan out remaining partitions; B: Submit and explicitly cancel a separate long-running statement.

## Deliverables (all under `examples/snowflake-sdk/`)

1. A crate named `concord-sdk-snowflake` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/snowflake-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
   root edit allowed; `Cargo.lock` may change accordingly).
2. A normal public developer entry point: a client type with documented construction
   (auth/context configuration), typed domain operations for the whole declared scope, typed
   provider errors that keep useful response metadata, and the required pagination / streaming /
   body / completion behaviour. Built with `concord::api!` and Concord's public types. A mature
   third-party crate is appropriate for generic protocols (signing, XML parsing) — record why you
   chose it. Never delegate HTTP execution to a vendor SDK.
3. `README.md`: what the SDK covers and excludes, construction, both workflows as code, errors,
   completion/partial-outcome semantics, limitations, and that fixtures are not vendor
   conformance.
4. Tests: `tests/protocol.rs` (wire-level request assertions: method, target, headers, exact body
   bytes; documented success and failure shapes; meaningful negative cases from the brief) and
   `tests/workflows.rs` (workflow A and B end to end through the public API against fixtures).
   Do **not** create `tests/first_use.rs` (reserved for the independent consumer).
5. `PROVENANCE.md`: fixture file → official page/section + version + access date →
   transformations; which cases are synthetic.
6. `FRICTION.md`: ordered factual entries — what Concord made easy, what you had to work around,
   each with a file:line reproduction and whether you think it is an SDK, provider, application,
   docs or Concord cause. Include anything in the declaration, the adapter code, the caller path
   or error handling that felt like reconstructing library work.

Commands: `cargo test --locked -p concord-sdk-snowflake`, `cargo clippy --locked -p
concord-sdk-snowflake --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-snowflake` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-snowflake` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
