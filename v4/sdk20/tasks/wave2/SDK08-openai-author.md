# SDK08 OpenAI — SDK author (large slice)

Requested runner: Codex gpt-6.1-terra (xhigh). You implement this SDK from scratch.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree whose Concord library is frozen at commit `e04236ca`; its `HEAD` may
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
  `examples/openai-sdk/fixtures/` and keep `examples/openai-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.

## The locked provider brief

### 8. OpenAI — large

Official: https://developers.openai.com/api/docs/guides/streaming-responses, https://developers.openai.com/api/reference/resources/files/methods/create and https://developers.openai.com/api/docs/guides/batch.

- Workflows: (1) create a response in both ordinary and SSE streaming modes, consume typed output events through completion and propagate a stream failure or cancellation; (2) prepare JSONL input, upload via Files with `purpose=batch`, create and poll a Batch, then retrieve and consume the output and applicable error files, correlating individual items by `custom_id`.
- Professional bounded coverage: Responses create, Files upload/retrieve metadata/content and the Batch create/retrieve lifecycle; normal, failed and in-progress statuses, typed request and response errors, caller-controlled cancellation and bounded polling, file bytes, multipart request, JSONL row parsing and SSE framing. Preserve partial results/errors instead of promising uniform batch success. The batch itself is not an SSE stream.
- Perturbation: split an SSE delimiter and multi-byte UTF-8 codepoint across chunks, and reorder JSONL output lines with a failed item; assert event decoding and correlation by ID rather than order.
- Feasibility: fake transport/SSE and synthetic local files, no model traffic; live requires quota, credentials and spending authorization. Exclude Realtime, model-quality evaluation, automatic retries and other endpoint families.

Official sources (re-check them now and pin the version/date you used in `PROVENANCE.md`):
- https://developers.openai.com/api/docs/guides/streaming-responses
- https://developers.openai.com/api/reference/resources/files/methods/create
- https://developers.openai.com/api/docs/guides/batch

Machine-readable scope: Responses, Files and Batch; SSE, JSONL and multipart. Excluded: Realtime; model quality; paid calls.
Workflows: A: Create ordinary and streaming Responses; B: Upload batch JSONL, poll batch and read output/error files.

## Deliverables (all under `examples/openai-sdk/`)

1. A crate named `concord-sdk-openai` (`publish = false`, `version = "0.0.0"`, `edition = "2024"`,
   `rust-version.workspace = true`, `license.workspace = true`, `[lints] workspace = true`), and
   its line `"examples/openai-sdk"` added to the root `Cargo.toml` `[workspace] members` (the only
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

Commands: `cargo test --locked -p concord-sdk-openai`, `cargo clippy --locked -p
concord-sdk-openai --all-targets -- -D warnings`, `cargo fmt -p concord-sdk-openai` (the first
`--locked` run may need one unlocked `cargo test -p concord-sdk-openai` to add your package and new
dependencies to the lock). Leave a clean, passing tree; the coordinator commits it.

Final message: files created, test counts you saw pass, dependencies added and why, and the three
most significant friction entries.
