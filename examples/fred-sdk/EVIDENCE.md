# Observable execution evidence

Date: 2026-11-07 Europe/Paris. Baseline HEAD:
`56a16ee34f8aa714bd51dca67f744e5870e813b6`. Initial `git status --short` was empty.
Implementation is uncommitted in the working tree; no commit or external message
was attempted. No shared crate, check, script or other example was modified.

## Resource and toolchain record

- Before building, `df -h . /tmp`, `du -sh target /home/canarit/.cargo/registry`,
  `free -h`, `nproc` and `printenv CARGO_TARGET_DIR` were inspected at
  2026-11-06 23:24:14 UTC. Worktree filesystem `/dev/nvme0n1p2`: 915G total,
  366G available. `/tmp` was a 16G tmpfs. Registry 1.5G; this worktree had no target
  directory. RAM 30Gi total, 23Gi available; 20 CPUs; swap 4.9Gi used of 8Gi.
- All Cargo build/test/doc/clippy commands used `CARGO_BUILD_JOBS=4`, one Cargo
  command at a time, no CARGO_TARGET_DIR override. Compilation used the worktree's
  default `target/`. No global cache or other worktree was modified or cleaned.
- Mid-run: target 684M / disk 361G available; after clippy target 1.4G;
  after docs/tests target 1.5G / disk 359G available. These are observed sizes,
  not a process peak-RSS measurement. No resource cancellation or cleanup was
  needed; the 20GiB free-space floor remained well clear. Outputs are rebuildable;
  source/fixtures and these records are the retained evidence.
- `rustc --version`: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
  `cargo --version`: `cargo 1.99.1 (5f94df478 2026-08-27)`.

## Commands and diagnostic sequence

All commands run from the workspace root unless a path is specified.

| Command (CARGO_BUILD_JOBS=4 on Cargo commands) | Result |
|---|---|
| `cargo test --offline -p concord-sdk-fred` (initial) | Exit 101: zip absent from cached index; no provider request |
| `cargo test -p concord-sdk-fred` (initial unlocked resolution) | Added only the SDK, zip and three conditional transitives; build exit 101, zip E0432/E0433 because deflate-flate2 alone did not enable its optional flate2 dependency |
| `cargo test --locked --offline -p concord-sdk-fred` (after feature correction) | Exit 101: lock needed to add ZIP's flate2 dependency edge |
| `cargo test --offline -p concord-sdk-fred` | Wrote corrected dependency edge; exit 101, SDK E0277 on MIME parameter reference comparison; fixed by dereferencing the name |
| `cargo fmt -p concord-sdk-fred` | Exit 0; formatting limited to owned package |
| `cargo test --locked -p concord-sdk-fred` (first protocol run) | Exit 101: 10 protocol tests passed; test wrongly required every scope Debug to print a redaction marker; fixed assertion to check absence of the secret. Also fixed missing test-crate doc warnings |
| `cargo test --offline -p concord-sdk-fred` (after adopting shared Retry-After parser) | Wrote concord-retry edge for the SDK; exit 0, 11 protocol + 6 workflow tests |
| `python3 examples/fred-sdk/fixtures/generate.py` | Exit 0, generated synthetic fixtures without SDK imports or network |
| `cargo fmt -p concord-sdk-fred` | Exit 0 after added archive/header scenarios |
| `cargo test --locked -p concord-sdk-fred` | Exit 0, 12 protocol + 6 workflow tests |
| `cargo clippy --locked -p concord-sdk-fred --all-targets -- -D warnings` | Exit 0, warning-free |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-fred --no-deps` | Exit 0, generated target/doc/concord_sdk_fred/index.html |
| `cargo test --locked -p concord-sdk-fred` (README included as doctests) | Exit 0, 12 protocol + 6 workflow tests and 2 compile-only README doctests; no ignored tests |
| `cargo clippy --locked -p concord-sdk-fred --all-targets -- -D warnings` (final source) | Exit 0, warning-free |
| `git diff --check` | Exit 0 |
| `cargo fmt -p concord-sdk-fred -- --check` | Exit 0 |
| Fixture regeneration with SHA256 comparison | Byte-identical, including every ZIP |
| Parsed old/new Cargo.lock entry comparison | Every pre-existing package entry unchanged, including dependency lists |

The extra unlocked invocations were dependency-setup corrections, recorded above;
all final required gates use `--locked`. No pre-existing lock package changed
version or disappeared. Parsed lock comparison added precisely:
`concord-sdk-fred 0.0.0`, `zip 2.4.2`, `crossbeam-utils 0.8.23`,
`arbitrary 1.5.0`, `derive_arbitrary 1.5.0`.

Lock SHA256:
`44c60624448953f0c51fd136d87c5660e0cd8752451ba254be02685f420cb51d`.

## Files and coverage

- `src/lib.rs`: ordinary `api!` declaration, sensitive query auth, bounded send
  policy, safe transport, validated limits, facade reexports, README doctests.
- `src/models.rs`: provider metadata/observations and exact-byte CSV export.
- `src/error.rs`: typed HTTP failures, secret-safe problem/error formatting.
- `src/codecs.rs`: XML/dual problem/ZIP Decode implementations.
- `src/pagination.rs`: FRED envelope checks over shared Pages/Sends.
- `tests/protocol.rs`: seven endpoint wire shapes; encoding; all documented HTTP
  classes; malformed/truncated/wrong-shape/wrong-media/missing/repeated bodies and
  heads; known-length/chunked limits; interrupted transport; ZIP count, aggregate
  sizes, malformed headers, CRC, forged lengths, encryption, unsafe members;
  Retry-After classes; redaction and redirect isolation.
- `tests/workflows.rs`: search→select→metadata, selected-series XML+JSON reads→CSV
  extraction→application-selected file, required export failures, bounded/resumed
  pagination, nonprogress/overflow and a failed second page preserving prior data.
- `tests/support/mod.rs`: local client, safe-error assertions and an independently
  authored valid oversized stored ZIP fixture builder.
- `fixtures/`: all synthetic; binary ZIPs are separate from text line counts.
- `README.md`, `PROVENANCE.md`, `FRICTION.md`, this record, owned Cargo.toml;
  root Cargo.toml has only the member addition and Cargo.lock is Cargo-written.

Final line accounting (text lines, including comments/blank lines and final
unterminated fragments): **src 666; tests 804; fixtures 221 text lines across
18 text files, plus 16 ZIP files totalling 12698 bytes.** Fixture text includes
generate.py; it contains 218 newline characters plus three unterminated final
lines. Archives are classified by `.zip` suffix even when deliberately malformed
and ASCII-readable; they have no meaningful line count. Markdown and manifests
are outside those three totals. No live provider behavior, independent consumer
review or shared Concord improvement is claimed.
