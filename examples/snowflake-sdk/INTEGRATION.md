# Integration record

## Final migration (open issue W4-10)

Open issue W4-10 in `v4/sdk20/STATUS.md`, found by the independent first-use consumer
(`FIRST_USE.md`, "The one prediction that failed" and friction F1). Base `47c6c5b0`. Role:
implementer, a Claude agent session spawned by the coordinator (model and effort not re-derived
from harness metadata here). Not an independent review.

**Defect.** The README says discovery rejects missing or empty descriptors, and the
`ProtocolError::InvalidManifest` rustdoc says "Metadata is absent". A 200 result whose
`resultSetMetaData` has no `partitionInfo` member was nonetheless an `Error::Concord` decode
error, because serde required the field before discovery ran. Only an empty `partitionInfo: []`
reached discovery and became `InvalidManifest`. An application that treats `Error::Protocol` as
"result unusable" and `Error::Concord` as "transport or codec trouble" would misclassify it.

**Fix.** `ResultMetadata::partition_info` gets `#[serde(default)]`, so an absent member reads as
empty and the existing discovery check (`partition_info.is_empty()`) returns `InvalidManifest`
with the response head. No new type, variant or check. The rustdoc of `partition_info` and
`InvalidManifest`, and the README's discovery paragraph, now say that an absent or empty
`partitionInfo` is `InvalidManifest`, and that a 200 body that does not decode as a result
document stays a decode error.

| HTTP 200 result document | Before | After |
|---|---|---|
| `resultSetMetaData` without `partitionInfo` | `Error::Concord { meta: Some(200), Decode }` | `Error::Protocol { kind: InvalidManifest, meta: Some(200) }` |
| `partitionInfo: []` | `InvalidManifest` | unchanged |
| invalid JSON; `resultSetMetaData` itself missing | `Error::Concord`, `Decode` | unchanged (not a result document) |

Caller-visible: only the error variant for the first row. `ResultMetadata` is public and
`Deserialize`. Code that deserializes it directly now gets an empty `partition_info` for an
absent member instead of an error. Inside the SDK, a `ResultMetadata` with no descriptors never
reaches a `FirstResult` or `PartitionPlan`.

### Lines (non-blank, non-comment, from `git diff`; this file excluded)

| | removed | added |
|---|---|---|
| `src/` | 0 | 1 |
| `tests/` | 15 | 76 |
| `README.md` | 2 | 5 |

The `src/` edit is one attribute; the rest of `src/` changes are rustdoc.

### Test edits, by kind

1. **Consumer expectation that pinned the defect, changed to the corrected behaviour**
   (`tests/first_use.rs::a_malformed_result_document_is_a_decode_error_not_a_result`). Its
   second scripted reply (no `partitionInfo`) was asserted as `Error::Concord`/`Decode` in a loop
   shared with the invalid-JSON reply. The loop is split. Invalid JSON keeps its decode
   assertions. The missing case now asserts `Error::Protocol { kind: InvalidManifest }` with the
   200 head, sharing a loop with the existing empty case, whose assertions are unchanged. This is
   the consumer's original prediction. The scripted replies are unchanged. The test name still
   holds for the invalid-JSON reply and is kept, so the reproduction named in `FIRST_USE.md`
   (not edited) still runs this test.
2. **Added wire test**
   `tests/protocol.rs::absent_and_empty_partition_info_are_invalid_manifest_with_the_head`. Over
   the loopback server it checks the following:
   - A submit that completes immediately without `partitionInfo` is `InvalidManifest`.
   - A status check after a pending submit gives the same for the absent and the empty case,
     with status 200 and the `x-snowflake-request-id` header kept in each case.
   - No partition request is sent: exactly four requests.
   - A 200 without `resultSetMetaData` is still `Error::Concord` with `ErrorKind::Decode` and the
     head.

No other assertion or expected value changed. The existing
`tests/protocol.rs::metadata_must_prove_partition_plan_and_first_rows_must_be_complete` asserts
only `is_err()` for both cases, which holds before and after. `FRICTION.md`, `FIRST_USE.md` and
`MAINTENANCE.md` are not edited.

### Verification (observed)

With the attribute removed (the pre-fix model) and the edited tests in place, both changed
tests failed with the old `Error::Concord`/`Decode` result: `first_use` 17 passed, 1 failed;
the new `protocol` test failed. With the fix, `CARGO_BUILD_JOBS=6 cargo test --locked -p
concord-sdk-snowflake`, run twice: `first_use` 18, `maintenance` 8, `protocol` 19 (was 18),
`workflows` 13, doctests 3 passed, 0 failed.
`cargo clippy --locked -p concord-sdk-snowflake --all-targets -- -D warnings`, `cargo fmt --check`
and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`: clean. Offline only.
