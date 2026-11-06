# SDK04 Amazon S3 — SDK author (large slice)

Requested runner: Claude Opus subagent. You implement this SDK from scratch.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree of the frozen candidate commit `94d13527`. Prefix every cargo command
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
  `examples/amazon-s3-sdk/fixtures/` and keep `examples/amazon-s3-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.

## The locked provider brief

### 4. Amazon S3 — large

Official: https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html, https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html, https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html, https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html, and https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sigv4-auth-using-authorization-header.html.

- Workflows: (1) inventory keys by prefix over XML `ListObjectsV2` pages, HEAD a selected object and conditionally GET all or one byte range, checking 304/412 and preserving metadata; (2) put a finite byte body with explicit content type, then retrieve and compare bytes and metadata in the fake server.
- Professional bounded coverage: general-purpose buckets and the documented ListObjectsV2, HeadObject, GetObject and single-request PutObject family, including opaque continuation tokens, XML success/error responses, typed status/header/body, binary bodies, conditional/range requests, caller cancellation and SigV4 with region, service and per-request credentials context. Verify a fixed official-compatible signing vector independently; no assumption that ETag is a universal content checksum or that single GET supports multiple ranges.
- Perturbation: change a key to include spaces/Unicode and change a signed range or content header; verify canonical request, signature, both list pages and unrelated calls. Unknown XML elements must not prevent decoding expected fields.
- Feasibility: synthetic credentials, frozen clock and deterministic fixture server; an optional local S3 emulator supplements but does not prove AWS conformance. Live requires a bucket and IAM. Exclude multipart uploads, presigning, streaming signatures, directory buckets and other AWS services.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html
- https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html
- https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html
- https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html
- https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sigv4-auth-using-authorization-header.html

Machine-readable scope: ListObjectsV2 XML pages; HeadObject, GetObject, PutObject; SigV4, bytes and XML errors. Excluded: multipart uploads; presigning; directory buckets.
Workflows: A: List prefix, HEAD object and conditionally or partially download; B: Put finite bytes and retrieve them.

## Deliverables (all under `examples/amazon-s3-sdk/`)

1. A crate named `concord-sdk-amazon-s3` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/amazon-s3-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
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

Commands: `cargo test --locked -p concord-sdk-amazon-s3`, `cargo clippy --locked -p
concord-sdk-amazon-s3 --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-amazon-s3` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-amazon-s3` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
