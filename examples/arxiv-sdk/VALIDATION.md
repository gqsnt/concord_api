# Validation — 2026-11-07

Live provider calls: **NOT_RUN**. All network tests use loopback TestServer fixtures.
No commit was made. Build output is the worktree's default `target/`; no
`CARGO_TARGET_DIR` was set. Cargo commands ran serially with four build jobs.

| Command | Observed result |
| --- | --- |
| `CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-arxiv --no-run` | PASS; initial unlocked offline build, 14.69 s. Added only the SDK's package entry. |
| `CARGO_BUILD_JOBS=4 cargo fmt -p concord-sdk-arxiv --check` | PASS. |
| `CARGO_BUILD_JOBS=4 cargo test --locked --offline -p concord-sdk-arxiv` | PASS; 16 protocol tests, 5 workflow tests, 1 compiled README doctest. Protocol suite 3.03 s; workflows 0.02 s; doctest compilation 0.14 s. |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked --offline -p concord-sdk-arxiv --all-targets -- -D warnings` | PASS; final run 0.34 s, no warnings. |
| `CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-arxiv --no-deps` | PASS; final run 0.23 s, no warnings. Generated `target/doc/concord_sdk_arxiv/index.html`. |
| `git diff --check` | PASS. |
| Lockfile package-set comparison against the pre-build copy | PASS; zero removals or version/source changes, only `concord-sdk-arxiv` 0.1.0 added. The textual diff also contains only its new record. |

The first test run passed but warned about missing documentation on the two test
crates. Crate doc comments fixed that diagnostic before the final warning-denying
checks. A final build-error redaction check and compiled README example increased
the final totals to 21 integration tests and one doctest.

Physical lines, including comments and blank lines:

| Group / file | Lines |
| --- | ---: |
| `src/atom.rs` | 163 |
| `src/error.rs` | 60 |
| `src/input.rs` | 86 |
| `src/lib.rs` | 104 |
| `src/model.rs` | 84 |
| `src/policy.rs` | 91 |
| **src total — 6 files** | **588** |
| `tests/protocol.rs` | 522 |
| `tests/workflows.rs` | 179 |
| `tests/common/mod.rs` | 47 |
| **tests total — 3 files** | **748** |
| **fixtures total — 13 XML files** | **132** |

Adversarial variants generated in protocol tests are counted in tests, rather than
in the fixture XML total. Other package files are `Cargo.toml`, `README.md`,
`PROVENANCE.md`, `FRICTION.md` and this validation record. Outside the package,
only the root workspace member list and Cargo's lockfile package entry changed.

Coverage includes both required workflows, unknown extensions and missing optional
fields, literal query/ID-list wire encodings, every manual error class, 2xx Atom
errors, HTTP 302/400/404/429/503, bad/missing/repeated media, malformed/truncated/XML
and interrupted HTTP bodies, fixed/chunked size limits, exact byte boundary, required
fields/counts/node limits, redaction, local refusal, pacing through clones, empty
results, bounded continuation/resume and failure with retained prefix.

These fixtures establish SDK behavior against the authored contract cases. They do
not establish successful live service interoperability, HTTP/2 behavior, TLS service
configuration, or cross-process rate coordination.
