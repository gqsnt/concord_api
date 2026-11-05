# ASTRA caller-first record (4.4 review)

Checkout: `0dd655018892a8cae21479f1cd249d2c875f6c83`, detached; release version intentionally not changed. All task files, build artifacts, and logs are beneath `review/astra/`. No network except loopback; no provider calls. No agents delegated.

## Reading before task completion

Repository/package discovery: root `Cargo.toml`, `crates/concord/Cargo.toml`, `examples/github-sdk/Cargo.toml`; searched for `AGENTS.md` at repository and ancestor levels (none found), checked clean status and commit identity.

Public caller material read: `docs/CALLER_GUIDE.md` (particularly preparing, errors, native requests), `docs/CUSTOM_FORMATS.md` (Encode/Decode, metadata context, Decoder), `docs/RECIPES.md` (metadata and streaming), `crates/concord-extras/README.md`, `examples/github-app/README.md`. The initial combined read was output-truncated; followed the relevant sections separately.

Public source docs / API declarations: `crates/concord-extras/src/csv/mod.rs`, `csv/read.rs`, `csv/write.rs`, `csv/options.rs`; `stream/mod.rs`, `stream/source.rs`, `stream/error.rs`; `examples/github-sdk/src/lib.rs`, `api.rs`, `models.rs` (Label), `ids.rs` (Owner/RepoName). Used the public documentation, signatures, declarations and adjacent simple API implementation text shown by those reads; did not investigate parser/driver/writer internals. Public GitHub docs overstate the generated endpoint API slightly by listing `send_raw`/`into_parts` without saying to prepare first; the caller guide clearly gives the actual route.

Environment instructions required reading the cloud runtime skill and its networking reference. Its status tool was not available; `/etc/codex/network-policy.json` was absent (`cat: /etc/codex/network-policy.json: No such file or directory`). No credentials read and no external networking attempted.

No author conclusions (`v4/extensions-real-apis/*.md`, FRICTION, MAINTENANCE, NOTES, PROVENANCE, DECISIONS, NEXT_REVIEW) were read before the caller task finished.

## Implementation and commands

`task/` is an independent crate with its own `[workspace]`, path dependencies on Concord and the GitHub SDK, and an offline lockfile. `task/src/lib.rs::with_metadata` retains status and `x-github-request-id` alongside a typed result, preserving the declared decoder and using `decoder.finish(Err(error))` to redact transport errors. It deliberately does not borrow an SDK-specific metadata helper.

`task/tests/caller.rs` implements all three requested cases:

1. GitHub declared `repo(...).labels()` returning `Vec<Label>` plus status and one header.
2. Declared POST with `request Csv` and `StreamRows<Source>`; instrumented one-shot source is deliberately `Send + !Sync + !Unpin`. Zero polls/rows at preparation, native request is not cloneable, send is aborted after body delivery begins, source drop and subsequent poll stability checked, slow server drains to EOF/reset and checks missing final HTTP chunk.
3. Declared CSV response fetched with `prepare()?.send_raw()`, runtime `HeaderMode::Absent` against `header=present`; server withholds the body until after synchronous `open` refuses the head. Status/header remain inspectable and taking the response back yields the entire body.

Command inside `review/astra/task`: `CARGO_TARGET_DIR=../target cargo test --offline -- --nocapture`. Full initial output, including any compiler errors verbatim, is retained in `../logs/caller-first.log`.

## Task results

All three tasks passed before opening review evidence. See `logs/caller-pass.log` (3 passed).

There were **no compiler errors**. First execution had one fixture error: I omitted required `Label.default`. Verbatim diagnostic:

```text
thread 'provider_typed_value_keeps_metadata' (3745) panicked at tests/caller.rs:68:89:
called `Result::unwrap()` on an `Err` value: Error { kind: Decode, endpoint: Some("GitHub.repo.labels"), message: GET GitHub.repo.labels: response not decoded: the JSON does not match the expected type (line 1, column 58) }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Corrected only the local fixture with `"default":false`, per the public model. This was my fixture mistake, not an SDK defect. The redacted decode error correctly withheld the body and kept endpoint context.

Observed successful metadata: status `200 OK`, `x-github-request-id=astra-7`, typed `Label.name=bug`.

Observed upload: preparation polls **0**; cancellation after **4** rows / **16** source polls; source dropped **once**, polls stayed stable; slow receiver saw **32,826** wire-body bytes then **EOF**, without the final chunk. These are one run's counts, not fixed protocol guarantees; the tests assert bounds/events rather than scheduling-dependent exact counts. Cancellation does not roll back rows already delivered. Non-Sync/non-Unpin source compiled and sent without an adapter.

Observed runtime contradiction: synchronous `OpenErrorKind::Metadata`, `200 OK`, request-id accessible while the server still withheld all body bytes. Diagnostic: `the response head was refused: the CSV response declares header=present, but this format reads no header (response kept unread)`. After releasing the server, `into_response().text()` returned every original byte.

Friction/surprises: metadata preservation requires a short generic wrapper, notably an awkward `decoder.finish(Err(error)).await.err().unwrap()` branch to preserve Concord's transport conversion. The docs described it accurately and no core change was necessary to finish the task. Streaming behavior and unread-response refusal met the documented contracts on first execution. SDK module docs list methods as if directly on endpoints, whereas the usable native methods are on `Prepared`; the caller guide resolves this without guessing.

**Phase boundary:** task complete, 3/3 passing; only after this record was written did I read implementation internals or author/independent-review conclusions.


## Post-task review work (after the phase boundary)

Read the implementations throughout `crates/concord-extras/src/{stream,csv,jsonl}/`, shared `media.rs`, and `crates/concord-core/src/{prepared,codec,error}.rs`; relevant CSV/JSONL framing, mapping, limits, HTTP, upload, and scale tests; core/facade metadata and error-inspection tests; SDK metadata/transport, error-body, download, watch and Salesforce CSV/spool paths. Read relevant sections of all five SDK `FRICTION.md` files, all five `checks/maintenance-*/MAINTENANCE.md` records, both `checks/dx03-*/NOTES.md` records, and CSV/JSONL/core-related implementer decisions. Inspected each app fixture `MANIFEST.json` for evidence/provenance scope, the supplied stress JSON, and the stress harness source. No links in provider provenance were fetched.

The full initial decision/evidence reads exceeded tool-output budgets; followed the relevant CSV/JSONL and native/metadata sections with focused reads/searches. Full library/extras implementation review included writer schema capture, finite/raw codecs, parser buffers, media validation, shared driver/source and upload lifecycle. No source change was made as a proposed fix.

Additional task-crate files: `tests/probes.rs` records four current defects (escape round-trip, empty-source polling, large JSON unfinished records, oversized JSONL scan); `src/bin/writer_memory.rs` isolates finite and streaming writer allocation observations. Added the already-used `http-body` dependency to inspect a lazy body directly without network buffering. Review-source formatting used `cargo fmt --manifest-path review/astra/task/Cargo.toml` only.

One post-task probe-writing compiler failure occurred: I typed one too many closing angle brackets in a temporary erased-future helper. Replaced it with a generic `fn poll_once<F: Future>` helper; no library workaround. This is distinct from the earlier caller task, which had no compiler errors. Verbatim output (also retained in `logs/probes-budget-compile-error.log`):

```text
   Compiling astra-review-task v0.0.0 (/home/canarit/projects/concord-ext44/rv44-astra/review/astra/task)
error: expected one of `!`, `,`, `::`, or `|`, found `>`
  --> tests/probes.rs:78:126
   |
78 |     let poll_once = |future: &mut std::pin::Pin<&mut dyn Future<Output = Option<Result<String, concord::stream::ReadError>>>>>| {
   |                                                                                                                              ^ expected one of `!`, `,`, `::`, or `|`

error: expected one of `!`, `#`, `&`, `(`, `,`, `-`, `...`, `..=`, `..`, `::`, `[`, `_`, `box`, `mut`, `ref`, `|`, identifier, or path, found `>`
  --> tests/probes.rs:78:126
   |
78 |     let poll_once = |future: &mut std::pin::Pin<&mut dyn Future<Output = Option<Result<String, concord::stream::ReadError>>>>>| {
   |                                                                                                                              ^ expected one of 18 possible tokens

error: could not compile `astra-review-task` (test "probes") due to 2 previous errors
```

Final independent command: `CARGO_TARGET_DIR=../target TMPDIR="$PWD/../tmp" cargo test --offline -- --nocapture` inside `task/`: **7 tests passed** (3 caller, 4 probes); `logs/task-final.log`. The probes assert the current incorrect behavior explicitly; passing means the finding is reproduced. The isolated memory commands and observations are in FINDINGS and their two logs.

Repository-root verification used these commands, each with `CARGO_TARGET_DIR="$PWD/review/astra/target" TMPDIR="$PWD/review/astra/tmp"`:

```sh
cargo test --offline --locked -p concord-core -p concord-extras -p concord-example-native --all-features
cargo test --offline --locked -p concord-example-github-sdk -p concord-example-stripe-sdk -p concord-example-shopify-sdk -p concord-example-salesforce-sdk -p concord-example-kubernetes-sdk -p concord-check-dx03-csv-runtime -p concord-check-dx03-fanout-sink
cargo test --offline --locked -p concord-check-stream-stress -p concord-check-maintenance-github -p concord-check-maintenance-stripe -p concord-check-maintenance-shopify -p concord-check-maintenance-salesforce -p concord-check-maintenance-kubernetes
cargo test --offline --locked -p concord --all-features --test metadata --test error_inspection --test bodies
```

Totals: 444 existing tests/doc-tests passed, 0 failed, 1 opt-in throughput measurement ignored. Logs: `core-extras-native.log`, `providers-callers.log`, `stress-maintenance.log`, `facade-boundaries.log`. The full 256 MiB release stress measurement was inspected, not rerun. `git diff 54f96ab 0dd65501 -- crates/concord-core crates/concord-extras checks/stream-stress` was empty, establishing unchanged reviewed implementation for the supplied report.

Final review conclusions are in `FINDINGS.md`, separate from this chronological caller record. Final source-tree check showed no tracked diff; only the review directory is untracked. No network beyond loopback, no commits/pushes, and no other worktree operations.
