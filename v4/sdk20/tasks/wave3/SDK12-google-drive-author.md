# SDK12 Google Drive — SDK author (large slice)

Requested runner: Codex gpt-6.1-sol (xhigh). You implement this SDK from scratch.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree whose Concord library is frozen at commit `6d6d66aa`; its `HEAD` may
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
  `examples/google-drive-sdk/fixtures/` and keep `examples/google-drive-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.

## The locked provider brief

### 12. Google Drive — large

Official: https://developers.google.com/workspace/drive/api/guides/manage-uploads, https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list, and https://developers.google.com/workspace/drive/api/guides/manage-changes.

- Workflows: (1) list files across pages, inspect metadata, upload a small binary with metadata as multipart, and retrieve the resulting metadata; (2) initiate a resumable upload, send bounded byte chunks, recover the session's acknowledged position after an interruption, finish, then consume the changes feed from a start token through its next-page/new-start-token boundary.
- Professional bounded coverage: Drive v3 files list/get/create, multipart/related, resumable initiation and status/resumption, and changes start/list. Verify MIME parts and bytes, returned session Location, Content-Range, documented intermediate 308 behavior, partial fields and pagination. URLs and credentials are isolated by origin policy; uploaded bytes are never replayed blindly after uncertain progress. OAuth bearer is injected rather than minting tokens.
- Perturbation: filename includes Unicode and bytes contain a boundary-like sequence; a resumable upload is interrupted after an acknowledged chunk, followed by another changes page. Verify exact reconstruction and retained tokens.
- Feasibility: deterministic fake upload and changes state machine; optional live requires a scoped test Drive and authorization. Exclude sharing, export, all Drive metadata fields, background sync and OAuth token acquisition.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://developers.google.com/workspace/drive/api/guides/manage-uploads
- https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list
- https://developers.google.com/workspace/drive/api/guides/manage-changes

Machine-readable scope: v3 files and changes; multipart and resumable upload. Excluded: sharing; export; OAuth token acquisition.
Workflows: A: List and retrieve metadata, multipart upload; B: Resume interrupted upload and consume changes feed.

## Deliverables (all under `examples/google-drive-sdk/`)

1. A crate named `concord-sdk-google-drive` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/google-drive-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
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

Commands: `cargo test --locked -p concord-sdk-google-drive`, `cargo clippy --locked -p
concord-sdk-google-drive --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-google-drive` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-google-drive` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
