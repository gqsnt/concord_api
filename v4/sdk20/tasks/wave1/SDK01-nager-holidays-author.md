# SDK01 Nager.Holidays — SDK author (small slice)

Requested runner: Codex gpt-6.1-sol (xhigh). You implement this SDK from scratch.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree of the frozen candidate commit `94d13527`. Prefix every cargo command
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
  `examples/nager-holidays-sdk/fixtures/` and keep `examples/nager-holidays-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.

## The locked provider brief

### 1. Nager.Holidays (Nager.Date) — small; negative control

Official: https://nagerholidays.com/api and https://github.com/nager/Nager.Date/blob/main/README.md. The researched v4 example uses `GET /api/v4/Holidays/{CountryCode}/{Year}`. Recheck and pin the current version at kickoff; the former Nager.Date v3 path is not this v4 contract.

- Workflows: retrieve the holidays for a country and year; retrieve a different country/year and filter by national status or holiday type locally.
- Pressure and scope: path order and encoding, a JSON array, dates, nullable subdivision codes and holiday types. Ordinary SDK declarations are the negative control: do not add a generic abstraction simply for this API.
- Perturbation: add an unknown optional field and a null subdivision list to a fixture; callers retain known fields.
- Feasibility: deterministic local JSON and synthetic invalid-request failure; no live writes or paid calls. Exclude worldwide coverage guarantees, date calculations and calendar semantics.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://nagerholidays.com/api
- https://github.com/nager/Nager.Date/blob/main/README.md

Machine-readable scope: v4 holiday JSON array and country/year path; nullable subdivisions. Excluded: calendar calculations; worldwide coverage guarantees.
Workflows: A: Read holidays for one country and year; B: Read another country/year and filter known types.

## Deliverables (all under `examples/nager-holidays-sdk/`)

1. A crate named `concord-sdk-nager-holidays` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/nager-holidays-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
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

Commands: `cargo test --locked -p concord-sdk-nager-holidays`, `cargo clippy --locked -p
concord-sdk-nager-holidays --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-nager-holidays` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-nager-holidays` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
