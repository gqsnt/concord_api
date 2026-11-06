# Author friction and evidence

Ordered factual entries from the SDK author run. Paths are relative to this directory.
Concord remains frozen at `0983a349`; no library patches, private reach-through,
handwritten operation identities, native HTTP execution, or reconstructed execution
were needed. “Cause” below identifies the most likely layer, not an accepted upstream
change request.

1. **Declaration stayed small (easy; Concord).** Four operations share auth, media and
   path configuration in `src/api.rs:32`. Typed UUID values fit path/query placements;
   the async query switch and exact JSON bytes required no URL/body assembly. Reproduce
   with `tests/protocol.rs:9` (`all_four_operations_have_exact_targets_headers_and_bodies`).
   Result: method, target, shared headers and body bytes pass for submit/status/partition/cancel.

2. **One execution retains metadata and identity (easy; Concord).**
   `src/client.rs:180` uses `Prepared::receive`, copies `meta()`, then calls
   `decode_limited` or `bytes_limited`. No status is fabricated and no operation name is
   authored by SDK runtime code. The Salesforce transport pattern supplied this approach.
   Reproduce with the provider-error and truncated-body tests in `tests/protocol.rs`:
   the metadata still names `SqlApi.submit` after body consumption/failure. The operation
   string appears only in assertions, not in the adapter.

3. **Status-dependent success type needs a custom decoder (adapter work; provider +
   Concord declaration boundary).** Submit/status return 202 QueryStatus or 200
   ResultSet. A single declared output required `StatementJson` at `src/api.rs:12`:
   explicitly delegate JSON media checking, capture status in Context, and dispatch
   decoding to the appropriate type. The public extension works and compiles, but the
   adapter repeats JSON behavior for a routine response union. Reproduce with the
   pending/immediate paths in `tests/workflows.rs:31` and
   `immediate_one_empty_and_second_partition_variants_use_public_workflow`. A status
   union helper could reduce this code; Snowflake's outcome interpretation belongs here.

4. **Typed provider errors require a separate body branch (adapter work; Concord).**
   `src/client.rs:188` branches before declared success decoding, bounds bytes,
   deserializes the error document, preserves malformed bytes/read errors, and carries
   the response head into `Error`. The macro has no provider-error output declaration.
   Reproduce with `provider_error_keeps_code_sqlstate_headers_raw_body_and_generated_operation`,
   `malformed_and_non_json_server_errors_keep_status_and_evidence` and
   `body_limits_apply_after_gzip_and_keep_error_heads` in `tests/protocol.rs`.
   Result: useful metadata survives, but this reusable error-body plumbing is SDK code.
   `bytes_limited` made safe collection straightforward; no native escape was necessary.

5. **An already-obtained first result needs an independent-iterator wrapper (caller/
   adapter work; Concord-pagination).** Polling already owns partition zero, whereas
   `first_page_then` expects to fetch a first key. `src/partitions.rs:63` stores the
   first partition and starts `independent(1..count, ...)` for the rest. Naming the
   public traversal requires boxed closure/future aliases at `src/partitions.rs:204`.
   `PartitionReport::missing` at `src/partitions.rs:256` combines the first item,
   unfinished keys and remaining plan. This felt like reconstructing a seedable,
   ergonomic independent traversal facade, although all scheduling stays in Concord.
   Reproduce with `tests/workflows.rs:31`, the budget/caller-stop test and
   `tests/workflows.rs:577` (dropped next). Result: zero is never fetched, local stop
   preserves exact missing work, and cancellation-safe next resumes the same request.

6. **Provider metadata is the independence proof (necessary SDK work; provider).**
   `src/partitions.rs:15` validates descriptors, sums, supported format and first
   rows; `PartitionPlan::fetch` constructs a handle/index request and checks row counts
   and any echoed handle. Reproduce with `tests/protocol.rs:240` and
   `plan_can_fetch_index_two_before_one_using_original_descriptors` in workflows.
   Result: numRows alone never creates a plan; index 2 can be fetched before 1.
   These checks belong in the Snowflake adapter, not generic pagination.

7. **Later-partition body spelling is under-specified in the reviewed examples
   (docs/provider).** The response guide says gzipped JSON without metadata; the
   reference supplies a generic full GET ResultSet. Neither gave a complete compressed
   later-part specimen. `src/model.rs:218` accepts a data envelope and bare rows, and
   PROVENANCE marks both fixtures as synthetic. Reproduce with the independent-fetch
   workflow and `tests/protocol.rs:411` (gzip/limits). Result: no handwritten gzip;
   the original reqwest decoding dropped Content-Encoding and Content-Length. The
   coordinator rework in entry 11 replaces it with bounded local flate2 decoding,
   preserving those wire headers. The provider body-shape uncertainty is unchanged.

8. **Polling and remote cancellation are separate policies (necessary application/
   provider work).** `src/polling.rs:114` composes Tokio's clock, finite attempts and
   LocalStop with single status calls. `src/polling.rs:183` preserves cancel result and
   later observation separately. Reproduce with `tests/workflows.rs:416` and `:465`:
   one observes terminal cancellation, the other a rejected cancel followed by success.
   No generic Concord retry/cancel scheduler was rebuilt; only Snowflake's workflow
   decisions are implemented. An acknowledged cancel with only pending observations
   remains unresolved.

9. **Paused-clock tests initially missed rounded timer boundaries (test/application).**
   Initial run: 11 protocol tests and 8 workflow tests passed; 4 paused-clock cases
   timed out in the test spin helper, with no second status request at the exact
   virtual boundary. Advancing by one extra millisecond resolved Tokio timer rounding.
   `tests/common/mod.rs:79` keeps the paused runtime runnable while loopback I/O arrives;
   `tests/workflows.rs:333` asserts no request before the interval and then advances
   beyond the timer tick. Result: all clock cases pass without wall-clock sleeps.
   This is fixture infrastructure, not a provider timing claim or library defect.

10. **Gzip dependency resolution hit the read-only Cargo cache (environment).**
    The first unlocked test selected async-compression 0.4.52 /
    compression-codecs 0.4.47 / crc32fast 1.5.4; downloading crc32fast failed with
    `Read-only file system (os error 30)`. The original dependency at `Cargo.toml:12`
    enabled reqwest gzip; entry 11 supersedes that decision with flate2.
    Three offline `cargo update --precise` calls selected the already-cached compatible
    versions recorded in PROVENANCE. Result: locked builds work with no cache copying,
    credential access or sandbox escalation. Clippy later requested one collapsible-if
    cleanup and boxing PollOutcome's large complete variant; both were applied.

Initial author validation: 13 protocol tests, 13 workflow tests and 3 README doctests
pass; `cargo clippy --locked -p concord-sdk-snowflake --all-targets -- -D warnings`
passes and `cargo fmt -p concord-sdk-snowflake` completes. All cargo invocations use
`CARGO_BUILD_JOBS=5` and the requested shared target-b cache. Tests are offline loopback
fixtures; no live provider or external integration verification is claimed.


11. **SDK-selected reqwest decompression leaked through unified features (SDK cause;
    coordinator follow-up).** The coordinator reported that the original
    `reqwest = { workspace = true, features = ["gzip"] }` made
    `crates/concord/tests/metadata.rs:360` fail under `cargo test --locked --workspace`:
    an unrelated client's Binary response was decompressed instead of retaining the
    wire bytes. `scripts/verify.sh:53` assigns reqwest decompression features to
    applications; `checks/consumers/facade-gzip/Cargo.toml:11` exercises that choice
    in an isolated workspace. The original targeted SDK tests did not catch this
    workspace integration error.

    Intervention: `Cargo.toml:12` now uses flate2 1.1.10 with only its Rust backend,
    with no direct reqwest dependency or decompression feature. The SDK client uses
    reqwest's public no-decompression switches (`src/client.rs:126`) so a downstream
    application's feature choice cannot double-decode its partition responses.
    `src/api.rs:50` advertises gzip only for the indexed partition operation.
    `src/compression.rs:17` checks media/encoding through a declared Decode;
    `src/compression.rs:52` uses mature MultiGzDecoder with a limit-plus-one read,
    validates complete gzip members/trailers, and parses the bounded JSON. Concord
    still executes once and bounds wire bytes; the original metadata survives.

    Runtime inflation limits are client configuration, unavailable in the static
    Decode::context signature. The declared finite output therefore carries bounded
    encoded bytes and an encoding flag to the SDK's runtime decoder. This is adapter
    work around that configuration boundary, not a reimplementation of transport or
    gzip. `src/error.rs:93` carries typed body errors plus the original ResponseMeta.

    Reproduction: `tests/protocol.rs:468` asserts that an ordinary reqwest client
    linked alongside the SDK still receives compressed bytes and does not advertise
    gzip. `tests/protocol.rs:411` keeps the former overflow/corruption checks, changing
    only their expected error ownership from Concord Limit/Transport to SDK
    PartitionBodyError::Limit/Gzip; both still retain the generated operation name.
    `tests/protocol.rs:482`, `:514`, `:550` and `:590` check wire bounds, gzip
    checksum/trailer/trailing-data integrity, exact inflated bounds and multiple
    members, and early unsupported-encoding rejection. Existing fixture bytes,
    workflow assertions, row/handle correlation and request-body assertions remain.

    Verification: three requested locked SDK runs each passed 18 protocol tests,
    13 workflow tests and 3 README doctests. Clippy with all targets and -D warnings
    passed. The workspace reverse-feature tree contains no reqwest gzip, brotli,
    zstd or deflate feature. `cargo test --locked --workspace` completed with exit 0;
    `content_encoding_is_not_a_media_type` passed. Formatting completed and
    `git diff --check` was clean. All commands used the required five-job/shared-target
    prefix. The frozen library, other examples and existing fixture bytes were unchanged.
