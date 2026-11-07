# Verification record

Baseline: `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`, uncommitted author tree.
Date: 2026-11-07. All Cargo commands set `CARGO_BUILD_JOBS=4`, run sequentially,
and use the worktree's default `target/`; CARGO_TARGET_DIR is unset.
No shared crate/check/other example/script edits; no commits and no live provider
calls. Source and fixtures remain in this package, with root member/lock addition.

Initial resource observation: `/dev/nvme0n1p2`, 437 GiB available of 915 GiB;
26 GiB RAM available of 30 GiB, 5.6 GiB free swap of 8 GiB. No other Cargo/rustc
process appeared in the process inspection. Only the owned default target is
created; no cache cleanup was necessary. First offline build completed in 14.46s.

- `cargo build --offline -p concord-sdk-square-payments`: PASS; initial unlocked
  build added only this SDK lock entry, no registry/package updates.
- First `cargo test --locked -p concord-sdk-square-payments`: compile failure
  E0599 (`Resume::Next`) and missing integration-crate documentation warning;
  corrected to documented `Resume::At` and added crate docs.
- `rustfmt --edition 2024` on package Rust files: applied package formatting.

Final gates:

| Command (all with `CARGO_BUILD_JOBS=4`) | Result |
| --- | --- |
| `cargo test --locked -p concord-sdk-square-payments` | PASS: 11 protocol tests, 4 workflow tests, 1 compiled no-run README doctest; no ignored tests |
| `cargo clippy --locked -p concord-sdk-square-payments --all-targets -- -D warnings` | PASS: no warnings; 11.07s |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-square-payments --no-deps` | PASS: no rustdoc warnings |
| `rustfmt --edition 2024 --check` on all package Rust files | PASS |
| `git diff --check` | PASS |
| Python/tomllib baseline comparison of root member/lock packages | PASS: all prior package objects identical, precisely one SDK lock package and one appended member |

The final test matrix runs 48 synthetic provider error status/category/code
cases, including every listed CreatePayment error description and all common
authentication error codes. Body-fault recipes are run for both success and
HTTP failure; limits are exercised for full and chunked responses. Every local
server is explicitly shut down and asserts no connections remain. Protocol
tests assert application dispatch counts for non-retry and redirect behavior.

Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`,
`cargo 1.99.1 (5f94df478 2026-08-27)`. Target measured after build/test/clippy:
1.1 GiB; available filesystem space 434 GiB. Outputs are reproducible, isolated
to this worktree's target and retained for review/reuse. No other cache was
changed. These are spot measurements, not a claimed sampled peak-memory result.

Line counts (physical lines, including blank/comment lines): source 354 across
3 files; tests 657 across 3 files (including support); fixtures 104 across 9
files. Documentation and Cargo manifests are excluded from those groups.

No shared fixes, live-call results, fresh-role results, commit or publication is
claimed. The first compile failure above is preserved as author evidence; the
corrected run is the final result.
