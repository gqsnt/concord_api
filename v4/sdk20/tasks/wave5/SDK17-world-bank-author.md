# SDK17 World Bank Indicators — SDK author (small slice)

Requested runner: Codex gpt-6.1-terra (xhigh). You implement this SDK from scratch.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree whose Concord library is frozen at commit `47a25fbf`; its `HEAD` may
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
  `examples/world-bank-sdk/fixtures/` and keep `examples/world-bank-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.
- **Held-out wave:** this is first-pass validation of a frozen library. Do not read the campaign
  records under `v4/sdk20/`, or any other SDK's `FRICTION.md`, `FIRST_USE.md`, `MAINTENANCE.md` or
  `INTEGRATION.md`. Other SDKs' source, README and tests remain normal reading material.

## The locked provider brief

### 17. World Bank Indicators — small

Official: https://datahelpdesk.worldbank.org/knowledgebase/articles/898581-api-basic-call-structures and https://datahelpdesk.worldbank.org/knowledgebase/articles/898599-indicator-api-queries.

- Workflows: retrieve a country's indicator for a year in JSON; retrieve a time range and traverse the indicated result pages.
- Pressure and scope: JSON selected by query and a metadata/data response shape distinct from an ordinary object envelope; nullable indicator values, page metadata. Freeze examples against the documented Indicators API only.
- Perturbation: include an absent value and a multi-page metadata response; preserve null distinct from zero.
- Feasibility: local fixtures; optional unauthenticated read-only call. Exclude SDMX, XML and economic interpretation.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://datahelpdesk.worldbank.org/knowledgebase/articles/898581-api-basic-call-structures
- https://datahelpdesk.worldbank.org/knowledgebase/articles/898599-indicator-api-queries

Machine-readable scope: Indicators API JSON metadata/data; nullable values. Excluded: SDMX; XML; economic interpretation.
Workflows: A: Read country indicator for one year; B: Read a time range across pages.

## Deliverables (all under `examples/world-bank-sdk/`)

1. A crate named `concord-sdk-world-bank` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/world-bank-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
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

Commands: `cargo test --locked -p concord-sdk-world-bank`, `cargo clippy --locked -p
concord-sdk-world-bank --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-world-bank` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-world-bank` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
