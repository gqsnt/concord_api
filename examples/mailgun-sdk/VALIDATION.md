# SDK27 validation record

Date: 2026-11-07 UTC. Frozen HEAD:
`8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. All changes are uncommitted.
Live provider calls: **NOT_RUN**. This is author verification, not independent
consumer or maintainer evidence. No shared source was modified.

Toolchain observed: `rustc 1.99.1 (58cf8f9c3 2026-10-13)` and
`cargo 1.99.1 (5f94df478 2026-08-27)`. All Cargo invocations had
`CARGO_BUILD_JOBS=4`, ran one at a time and used the worktree's default `target/`.
No CARGO_TARGET_DIR override, commits, live service calls or package publication.

## Commands and outcomes

| Command | Observed outcome |
|---|---|
| `git status --short`; `git rev-parse HEAD` | Initially clean; exactly the frozen baseline. |
| `CARGO_BUILD_JOBS=4 cargo build --offline -p concord-sdk-mailgun` | PASS, first build without `--locked`, 15.18 s; only new package entry added to Cargo.lock. |
| `CARGO_BUILD_JOBS=4 cargo test --locked --offline -p concord-sdk-mailgun` (initial) | Compile failed: E0609 `Progress.requested` should be `admitted`; E0382 formatting an operation while consuming it. Fixed only SDK tests. |
| Same test command (after corrections) | PASS, 10 protocol + 9 workflow tests. |
| `rustfmt --edition 2024 examples/mailgun-sdk/src/*.rs examples/mailgun-sdk/tests/*.rs examples/mailgun-sdk/tests/support/mod.rs` | PASS; only owned Rust files formatted. |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked --offline -p concord-sdk-mailgun --all-targets -- -D warnings` | PASS, 11.98 s, no warnings. Includes final test-mode handling and tests. |
| `CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-mailgun --no-deps` | PASS, 3.86 s. Output: `target/doc/concord_sdk_mailgun/index.html`. |
| `CARGO_BUILD_JOBS=4 cargo test --locked --offline -p concord-sdk-mailgun` (final) | PASS: 10 protocol tests, 9 workflow tests, one compiled caller doctest and one compile-fail ownership doctest. No ignored tests. |
| Parsed before/after Cargo.lock comparison using Python `tomllib` | Only `concord-sdk-mailgun` 0.0.0 added; zero removed/changed existing package entries, including dependency lists. |
| `git diff --check` | PASS. |

## Coverage witnesses

- `buffered_send_uses_basic_multipart_and_preserves_acknowledgement`: exact auth,
  multipart names/repeated recipients/file bytes, media and length, message ID.
- `event_filters_and_regional_paths_are_declarative`: finite query parameters,
  URL encoding, US/EU prepared paths, Basic, no GET body.
- `send_file_retain_id_search_page_and_inspect_each_recipient`: actual local file
  read, acknowledgement retained, opaque next URL byte preservation, accepted /
  temporary / delivered / permanent results, empty terminal page.
- `post_upload_timeout_is_ambiguous_and_never_replays_either_source`: buffered and
  one-shot bodies, before-head and after-head timeouts, exactly one recorded POST
  and one connection per invocation.
- `one_shot_attachment_prepares_without_polling_and_sends_once`: zero prepare
  polls, non-cloneable native body, one source read and one chunked multipart POST.
- Remaining workflow cases: non-rewindable 429/500, source/size failures, bounded
  prefix and context-bound resume, failed second page, URL cycles, unknown and
  test-mode delivery events, equal-time contradictions.
- Protocol matrix: all documented HTTP classes plus fallback, strict body/media
  handling on both endpoints and both success/error statuses, transport-truncated
  bodies versus truncated JSON, secret markers in payload/headers/credentials,
  rate hints, redirects, malformed schemas and hostile continuation URLs.

## Resource and scope observations

Before building: `df -h . /tmp`, `free -h`, `nproc`, `du -sh target`.
Workspace filesystem `/dev/nvme0n1p2`: 915G total, 207G available; no preexisting
worktree target. RAM 30Gi total / 24Gi available, 20 visible CPUs. `/tmp` is a
separate 16G tmpfs with 15G available. Four Cargo jobs were retained conservatively.

After the first test compilation: target 776M, workspace 204G available.
After final checks: target 1.3G, workspace 203G available, RAM 22Gi available.
These are samples, not peak-RSS or continuous disk measurements; the filesystem
also contains other work. No cleanup was needed. The only owned disposable cache
is this worktree's `target/`; documentation and fixture evidence live in the
package, outside that cache. No other target or registry was deleted.

Physical line counts (`wc -l`, including comments and blanks):

| Files | Lines |
|---|---:|
| `src/lib.rs`, `message.rs`, `events.rs`, `error.rs`, `policy.rs` | 1,041 |
| `tests/protocol.rs`, `workflows.rs`, `support/mod.rs` | 969 |
| Eight files under `fixtures/` | 64 |

The package also contains Cargo.toml, README.md, PROVENANCE.md, FRICTION.md and
this record. Root changes are exactly one workspace member plus the Cargo.lock
package entry. Documentation lookups used only documentation.mailgun.com;
regional API origins were inspected in prepared URLs and never contacted.
