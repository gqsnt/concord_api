# SDK15 ClickHouse HTTP — SDK author (medium slice)

Requested runner: Claude Opus subagent. You implement this SDK from scratch.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree whose Concord library is frozen at commit `0983a349`; its `HEAD` may
  also contain this SDK's own earlier role commits (author, consumer), which is expected. Prefix
  every cargo command
  with `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a` (a shared build cache; if cargo says it is
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
  `examples/clickhouse-sdk/fixtures/` and keep `examples/clickhouse-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.

## The locked provider brief

### 15. ClickHouse HTTP — medium

Official: https://clickhouse.com/docs/interfaces/http, https://clickhouse.com/docs/interfaces/formats/JSONEachRow, https://clickhouse.com/docs/interfaces/formats/CSVWithNames, and https://clickhouse.com/blog/clickhouse-release-24-11.

- Workflows: (1) issue a typed parameterized SELECT over HTTP and consume a runtime-selected CSVWithNames or JSONEachRow response; (2) stream a finite JSONEachRow or CSV insert to a fixture table and query it back with a distinct query ID and caller context.
- Professional bounded coverage: a constrained SQL/query+parameter request, Basic-auth or documented headers, request/query ID, output format negotiation, exact streaming body bytes, CSV headers and quoted fields, line-oriented JSON input/output, server diagnostics and cancellation. Query IDs and progress/summary headers are metadata and must be surfaced when present, without promising their presence on every response. Do not interpolate untrusted SQL values as string fragments.
- Perturbation: split a quoted CSV field and a multi-byte JSONL row across transport chunks, then have a fixture emit HTTP 200, rows and a late query exception. Status 200 is not sufficient to claim a complete result; late-error behavior varies by format, settings and server version. Document selected detection contract and tradeoff of `wait_end_of_query=1` buffering, which does not universally guarantee that no partial result escapes.
- Feasibility: deterministic fake server with documented HTTP semantics; an optional local ClickHouse server can add compatibility evidence. Exclude general SQL engine support, database management, transaction promises, compression and every output format.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://clickhouse.com/docs/interfaces/http
- https://clickhouse.com/docs/interfaces/formats/JSONEachRow
- https://clickhouse.com/docs/interfaces/formats/CSVWithNames
- https://clickhouse.com/blog/clickhouse-release-24-11

Machine-readable scope: HTTP query parameters and ID; CSV and line-delimited JSON streams; late errors. Excluded: general SQL engine; database management; compression.
Workflows: A: Parameterized SELECT returning runtime CSVWithNames or JSONEachRow; B: Stream JSONEachRow or CSV insert and query back.

## Deliverables (all under `examples/clickhouse-sdk/`)

1. A crate named `concord-sdk-clickhouse` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/clickhouse-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
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

Commands: `cargo test --locked -p concord-sdk-clickhouse`, `cargo clippy --locked -p
concord-sdk-clickhouse --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-clickhouse` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-clickhouse` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
