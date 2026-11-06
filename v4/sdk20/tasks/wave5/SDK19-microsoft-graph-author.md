# SDK19 Microsoft Graph — SDK author (large slice)

Requested runner: Codex gpt-6-astra (xhigh). You implement this SDK from scratch.

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
  `examples/microsoft-graph-sdk/fixtures/` and keep `examples/microsoft-graph-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.
- **Held-out wave:** this is first-pass validation of a frozen library. Do not read the campaign
  records under `v4/sdk20/`, or any other SDK's `FRICTION.md`, `FIRST_USE.md`, `MAINTENANCE.md` or
  `INTEGRATION.md`. Other SDKs' source, README and tests remain normal reading material.

## The locked provider brief

### 19. Microsoft Graph — large

Official: https://learn.microsoft.com/graph/delta-query-overview, https://learn.microsoft.com/en-us/graph/delta-query-users, https://learn.microsoft.com/en-us/graph/api/group-list?view=graph-rest-1.0, and https://learn.microsoft.com/en-us/graph/api/group-list-members?view=graph-rest-1.0.

- Workflows: (1) enumerate user delta pages, save the final delta link and perform a later incremental sync; (2) enumerate groups and their direct members, handling member pages and a mix of directory object shapes, then associate accessible users with those groups.
- Professional bounded coverage: v1.0 users delta, groups list and direct-members listing, `@odata.nextLink` versus `@odata.deltaLink`, selected fields, opaque returned URLs, common errors and partially visible member objects. Verify destination and credential policy; member visibility depends on permissions and the v1.0 group-member endpoint has a documented service-principal omission. Do not claim transitive or complete directory membership.
- Perturbation: insert an empty intermediate delta page with a next link, and a group member of another directory object type or limited visibility; preserve type information and continue paging.
- Feasibility: offline fake sequence with distinct tenant contexts; optional live needs a permitted test tenant and token. Exclude directory mutations, transitive-members traversal, group delta and unsupported reset semantics.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://learn.microsoft.com/graph/delta-query-overview
- https://learn.microsoft.com/en-us/graph/delta-query-users
- https://learn.microsoft.com/en-us/graph/api/group-list?view=graph-rest-1.0
- https://learn.microsoft.com/en-us/graph/api/group-list-members?view=graph-rest-1.0

Machine-readable scope: user delta; groups and direct members; opaque links and object types. Excluded: mutations; transitive membership; group delta.
Workflows: A: Sync users through delta and reuse final link; B: Enumerate groups and direct members.

## Deliverables (all under `examples/microsoft-graph-sdk/`)

1. A crate named `concord-sdk-microsoft-graph` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/microsoft-graph-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
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

Commands: `cargo test --locked -p concord-sdk-microsoft-graph`, `cargo clippy --locked -p
concord-sdk-microsoft-graph --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-microsoft-graph` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-microsoft-graph` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
