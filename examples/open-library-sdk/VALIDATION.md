# SDK21 validation receipt

Date: 2026-11-07 Europe/Paris. Baseline: `56a16ee34f8aa714bd51dca67f744e5870e813b6`.
Changes remain uncommitted in the working tree. Live provider calls: **NOT_RUN**.

Every build/check used the worktree's default `target/`, with `CARGO_BUILD_JOBS=4` and one
cargo command at a time. No relative or absolute CARGO_TARGET_DIR override was set.

| Command | Observed result |
|---|---|
| `CARGO_BUILD_JOBS=4 cargo build --offline -p concord-sdk-open-library` | PASS, 15.23s; initial unlocked dependency resolution added the package entry. |
| `cargo fmt --all --check` | Initial check reported formatting differences in the new package; no files changed. |
| `CARGO_BUILD_JOBS=4 cargo fmt -p concord-sdk-open-library` | PASS; only the owned package was formatted. Repeated after adding a protocol control. |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-open-library` | Initial PASS: 12 protocol + 3 workflows. Final PASS after README/control addition: 13 protocol + 3 workflows + 1 compiled no-run README doctest; zero failures/ignored tests. |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-open-library --all-targets -- -D warnings` | PASS initially and on final source/tests; no warnings. |
| `CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-open-library --no-deps` | PASS, no warnings; generated `target/doc/concord_sdk_open_library/index.html`. |

Protocol coverage includes literal request targets/query encoding, all four routes, SDK defaults
and omitted options, base-path retention, no credentials/body, generated/prepared redaction,
safe error Display/Debug, ID normalization/rejection, keyless hits, Build and Transport errors,
Concord Status at the lower-level bypass, all non-2xx SDK HTTP handling, Metadata/Decode/Limit,
strict structured JSON media, absent/repeated/malformed/unsupported-charset media, malformed and
truncated JSON, wrong shape and missing envelope, fixed-length/chunked oversized bodies,
exact-cap acceptance, stalled-body head refusal, transfer loss after complete JSON, and original
HTTP status preservation across every error-document failure class.

Workflow coverage connects work search page 2 to sparse selected-work retrieval, author search
to its work batch, and both empty-page selections to a successful no-follow-up result. Every
fixture server is explicitly shut down and reports zero remaining open connections.

Machine-readable TOML comparison against `git show HEAD:Cargo.lock` confirmed exactly one
added package and byte-equivalent parsed records for every preexisting package (no version,
checksum, source or dependency changes). `git diff --name-only` showed only Cargo.toml and
Cargo.lock among tracked files; `git status --short` additionally showed the owned new package.

Final content counts (physical text lines, including comments/blank lines):

| Category | Files | Lines |
|---|---:|---:|
| `src/` | 4 | 350 |
| `tests/` (including support) | 3 | 582 |
| `fixtures/` | 14 | 104 |

The source files are `lib.rs` (declaration/reexports), `models.rs`, `error.rs` and `policy.rs`.
Tests are `protocol.rs`, `workflows.rs` and `support/mod.rs`. The package also contains
Cargo.toml, README.md, PROVENANCE.md, FRICTION.md and this receipt.

Concord capabilities: `api!` and generated scopes/setters/send/prepare; `#[sensitive]` search
values; shared `concord-codecs` JSON through facade feature `json`; `reqwest-defaults` for TLS;
`Endpoint`/`SendWith` for one SDK policy; `Received::document` and `decode_limited` for bounds;
`ResponseMeta` and error heads for safe metadata retention. No custom format, native escape,
pagination traversal or retry adapter was necessary.

Final observed target size was 1.2 GiB; the filesystem had 361 GiB free. No cleanup was needed.
These results cover this local focused package, not live-provider, Windows or full-workspace
verification.
