# SDK20 Docker Distribution Registry v2 — SDK author (large slice)

Requested runner: Claude Opus subagent. You implement this SDK from scratch.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree whose Concord library is frozen at commit `47a25fbf`; its `HEAD` may
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
  `examples/distribution-registry-sdk/fixtures/` and keep `examples/distribution-registry-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.
- **Held-out wave:** this is first-pass validation of a frozen library. Do not read the campaign
  records under `v4/sdk20/`, or any other SDK's `FRICTION.md`, `FIRST_USE.md`, `MAINTENANCE.md` or
  `INTEGRATION.md`. Other SDKs' source, README and tests remain normal reading material.

## The locked provider brief

### 20. Docker Distribution Registry API v2 — large

Official: https://distribution.github.io/distribution/spec/api/ and https://distribution.github.io/distribution/spec/auth/token/. This is the CNCF Distribution registry protocol, not a claim that every registry deployment implements every optional feature identically.

- Workflows: (1) discover repositories and paginated tags, retrieve a manifest with an explicit supported `Accept` media type, then HEAD and GET its referenced blobs with digest and byte validation; (2) initiate a blob upload, PATCH multiple ordered byte ranges, query upload status after interruption, complete with its digest and publish a manifest referring to the blob, all against a deterministic fake registry.
- Professional bounded coverage: `/v2/`, catalog, tags, manifest GET/PUT, blob HEAD/GET and upload POST/PATCH/status GET/final PUT, documented error bodies, response digest and relevant `Link`, `Location` and `Range` headers. Handle 401 Bearer challenge, token request including service and repository/action scopes, and retry with a synthetic token. Preserve the **latest server-provided Location**, query state and inclusive byte ranges; never construct upload URLs from UUIDs. Scope catalog and tag behavior to the documented deployment, recognizing catalog may be restricted or incomplete.
- Perturbation: change Location and opaque query state after a PATCH, interrupt and resume using the server-reported range, and require a distinct push vs pull token scope. Include a digest mismatch and assert failure without exposing tokens or crossing unauthorized origins.
- Feasibility: fake registry and auth service only; no public registry access or real push. Exclude foreign-layer redirects, credential helper discovery, manifest schema breadth, registry deletion/garbage collection and vendor certification. A running Distribution server is optional supplemental evidence only.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://distribution.github.io/distribution/spec/api/
- https://distribution.github.io/distribution/spec/auth/token/

Machine-readable scope: v2 catalog/tags/manifests/blobs; chunk upload and token challenge. Excluded: public registry traffic; deletion; foreign-layer redirects.
Workflows: A: Discover catalog and tags, retrieve manifest and verify referenced blobs; B: Upload chunked blob, resume and publish manifest in fake registry.

## Deliverables (all under `examples/distribution-registry-sdk/`)

1. A crate named `concord-sdk-distribution-registry` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/distribution-registry-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
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

Commands: `cargo test --locked -p concord-sdk-distribution-registry`, `cargo clippy --locked -p
concord-sdk-distribution-registry --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-distribution-registry` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-distribution-registry` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
