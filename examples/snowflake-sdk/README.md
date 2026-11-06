# Snowflake SQL API SDK

`concord-sdk-snowflake` is an unpublished, bounded SQL API **v2** slice built with
`concord::api!`. It covers asynchronous submit, single status checks, bounded polling,
independently indexed result partitions, and explicit remote cancellation. All HTTP
execution stays in Concord. The library is frozen at `0983a349`.

It excludes SQL interpretation/type conversion, multiple statements, binding-variable
helpers, warehouse administration, ingestion, authentication token issuance/refresh,
automatic resubmission/retries, and result persistence. Pass a **single statement**;
the SDK does not parse SQL to detect semicolons inside strings or comments. A returned
multi-statement envelope is rejected.

These are self-authored fixtures and offline tests, **not vendor conformance or live
Snowflake verification**. Live use requires an account, warehouse, credentials and
budget. Source versions, synthetic cases and uncertainties are in [PROVENANCE.md](PROVENANCE.md).

## Construction and stable context

Depend on this directory by path. Rust 1.99.1 / edition 2024 is the workspace baseline.
`Client::new` accepts an HTTPS account **origin** (no path, query, fragment or embedded
credentials), a caller-issued token, its classification and `ClientOptions`. HTTP
loopback origins are allowed for fixtures. Supported token classifications are OAuth,
key-pair JWT and programmatic access token; no cryptography is implemented here.

```rust,no_run
use concord_sdk_snowflake::{Client, ClientOptions, TokenType};

let client = Client::new(
    "https://example-account.snowflakecomputing.com",
    "SYNTHETIC_NOT_A_CREDENTIAL", // replace via your application's credential configuration
    TokenType::OAuth,
    ClientOptions::default(),
)?;
# Ok::<(), concord_sdk_snowflake::Error>(())
```

`Statement::session` carries case-sensitive database/schema/warehouse/role fields in
the POST body. Omitted fields use server defaults. `Statement::timeout` is the server
execution timeout; it is independent of HTTP and polling deadlines.

Supply a distinct UUID `RequestContext::submission_id` for each new submission. The
returned `Query`, every `PartitionPlan`, and each partition retain this original
context and the server handle. Cloning preserves that identity. GETs retain any
server-returned `requestId` and opaque query values exactly; without a returned link,
the declared GET omits optional `requestId`. Cancel accepts a separate request UUID.
There is no implicit request-ID
generation, retry, resubmission, or promise of exactly-once SQL execution. An interrupted
submit may have reached the server without returning a handle.

## Workflow A: submit, poll, reuse partition zero, fan out

The normal entry point is `Client::submit` → `Submission::wait` →
`FirstResult::bounded_partitions`. Immediate completion bypasses status polling.
`Query::status()` and `Query::poll()` are also available for deliberate later observation.
Keep a cloned query if you may need to resume after a local polling bound.

```rust,no_run
use concord_sdk_snowflake::*;
use std::num::NonZeroUsize;

async fn query_rows(client: &Client) -> Result<(), Error> {
    let mut statement = Statement::new("select ID, LABEL from DEMO");
    statement.session.warehouse = Some("TEST_WH".into());
    statement.timeout = Some(60);
    let submission = client.submit(
        RequestContext {
            submission_id: RequestId::new("00000000-0000-4000-8000-000000000101")?,
        },
        statement,
    ).await?;
    let query = submission.query.clone();
    let stop = LocalStop::default();
    let first = match submission.wait(PollPolicy::default(), &stop).await? {
        PollOutcome::Complete(first) => first,
        PollOutcome::Terminal(error) => {
            // Inspect code / SQLSTATE deliberately; do not print raw SQL error text by default.
            eprintln!("terminal response: {}", error.meta.status());
            return Ok(());
        }
        PollOutcome::Unresolved { reason, .. } => {
            eprintln!("local bound: {reason:?}; query {} remains addressable", query.handle());
            return Ok(());
        }
    };
    let mut partitions = first.bounded_partitions(
        NonZeroUsize::new(64).unwrap(), // maximum LATER partition requests
        NonZeroUsize::new(3).unwrap(),  // maximum concurrent/retained partition slots
    );
    let mut failures = Vec::new();
    while let Some(partition) = partitions.next().await {
        match partition {
            Ok(partition) => {
                // Deliver/stage partition.rows here. Each cell is Option<String>.
                println!("partition {}: {} rows", partition.index, partition.rows.len());
                // A caller may break here, then call finish() to retain partial-work evidence.
            }
            Err(failure) => failures.push(failure),
        }
    }
    let report = partitions.finish();
    if report.is_complete() {
        // Every partition was successfully delivered; publish staged application output here.
    } else {
        let missing: Vec<_> = report.missing().collect();
        eprintln!("partial outcome: {:?}, missing {missing:?}", report.remaining.outcome);
        // Keep failures plus report.remaining.retained and .unemitted_errors.
        // report.plan.fetch(index).await deliberately retries a later partition.
    }
    Ok(())
}
```

`partitionInfo` provides ordered descriptors, including each partition's actual row
count and server size fields. The [response guide](https://docs.snowflake.com/en/developer-guide/sql-api/handling-responses)
explicitly maps later entries to `GET /api/v2/statements/{handle}?partition=N`.
That contract, plus a validated complete first response, establishes the independent
plan. A total count alone never enables fanout. For example, `first.plan().fetch(2)`
may precede `fetch(1)` when both descriptors exist. Index zero is already owned by the
first result and is rejected by `fetch`; traversal delivers it without I/O.

When a response supplies partition `Link` targets, the plan follows those exact
targets, preserving query order, repeated values, escapes and server request IDs.
Known targets are shared by cloned plans and updated from later partition responses.
If `next` and `last` identify the same partition with distinct URLs, `next` is used.
Only partitions without a known returned target use the declared indexed GET.

Discovery rejects missing/empty descriptors, inconsistent total rows, arithmetic
overflow, unsupported format, a nonzero first-partition index, and mismatched row
count/width. Code `391908` (incomplete first result) is refused rather than silently
claiming completeness. String/null cells retain their provider values, including a
literal `"null"` distinct from SQL NULL. Unknown column/partition metadata is retained.

Use `FirstResult::partitions(pagination::Options)` for completion order, continue-on-error
or failure caps. The default is ordered, stop-on-error. The request budget counts only
later partitions. `Partitions::next()` is cancellation-safe: dropping an awaited next
future retains scheduler-owned requests for the following call. Dropping the traversal
or calling `finish()` drops local work, with no detached fetch tasks or remote cancel.

`None` alone does **not** prove success. `PartitionReport::is_complete()` requires
successful exhaustion. `missing()` names every index not delivered, including retained
results already available in the report. `remaining` keeps the Concord outcome,
progress, failed/cancelled keys, successful un-emitted results, un-emitted errors and
unstarted range. If stopped before the first call, `unconsumed_first` preserves zero.
Emitted results and errors belong to the caller and are not duplicated in the report.

## Workflow B: explicit remote cancellation and the completion race

`LocalStop::stop()` affects local polling only. It never invokes the server cancel
endpoint. `Query::cancel` sends that POST once and validates its documented
`000604` / `57014` acknowledgement. `cancel_and_wait` preserves that result, **including
rejection or transport error**, and then observes the query's actual status.
An inserted pending status remains pending; even an accepted cancel followed by
successful completion produces `PollOutcome::Complete`. Cancellation is terminal
only when a correlated status response actually reports it.

```rust,no_run
use concord_sdk_snowflake::*;

async fn cancel_long_query(client: &Client) -> Result<CancelObservation, Error> {
    let submission = client.submit(
        RequestContext {
            submission_id: RequestId::new("00000000-0000-4000-8000-000000000201")?,
        },
        Statement::new("select ID, LABEL from LARGE_DEMO"),
    ).await?;
    let observation = submission.query.cancel_and_wait(
        RequestId::new("00000000-0000-4000-8000-000000000202")?,
        PollPolicy::default(),
        &LocalStop::default(),
    ).await;
    match &observation.outcome {
        Ok(PollOutcome::Terminal(error)) if error.is_cancelled() => {
            println!("observed terminal cancellation");
        }
        Ok(PollOutcome::Complete(first)) => {
            println!("completion won; {} rows described", first.plan().metadata().num_rows);
        }
        Ok(PollOutcome::Terminal(_)) => println!("observed a different terminal failure"),
        Ok(PollOutcome::Unresolved { reason, .. }) => println!("still unresolved: {reason:?}"),
        Err(error) => eprintln!("observation failed: {error}"),
    }
    // Also inspect acknowledgement; a rejected cancel does not imply a cancelled query.
    Ok(observation)
}
```

Polling counts attempts and bounds elapsed time using Tokio's clock (virtual time in
tests). The deadline includes HTTP/body waiting and intervals, subject to Tokio timer
resolution and runtime scheduling. A bounded outcome retains the latest pending
response. Correlated 408/422 statement failures become `PollOutcome::Terminal`;
429/5xx and malformed failures remain errors, with no automatic retry. Only the
cancellation code/SQLSTATE pair marks terminal cancellation. A timeout-driven server
cancellation is not evidence that the caller sent a cancel POST.

The cancellation POST has the HTTP timeout; its subsequent polling has a separate
elapsed limit. A pre-stopped `LocalStop` still permits the explicitly requested cancel
POST, then stops observation. If the original submit already completed, the example
still invokes cancel and observes the resulting race; normal application policy can
consume that original `Submission::state` instead.

## Errors, limits and remaining tradeoffs

`Error::Server` retains status, headers, generated operation identity, typed code,
SQLSTATE, handle, returned status URL, unknown fields and bounded raw bytes. Invalid JSON
keeps its decode diagnostic; body truncation/limit failure keeps the Concord error
and response metadata. Successful-body read/decode/limit failures also keep the head.
`Error::Protocol` identifies correlation/manifest/row violations. `Error::Config`
means local validation failed. `Error::PartitionBody` retains the wire response head
with a typed `PartitionBodyError::Limit`, `Gzip` or `Json` diagnostic from SDK decoding.
Error Debug/Display omit provider message and raw body;
explicit document/body access can contain SQL and should be logged deliberately.

The default successful-body cap is **16 MiB of wire bytes per response**, and the same
cap independently bounds inflated partition bytes. The error cap is **64 KiB** and the
HTTP deadline is **30 seconds**. Each partition is fully decoded before delivery. This
is partition-level iteration, not an incremental row-byte stream. Memory includes
bounded encoded and inflated buffers, decoded rows, the first metadata, and the bounded scheduler
window; retaining delivered rows is application-owned. Server byte counts are
preserved, not used as trustworthy local allocation or byte-limit instructions.

Snowflake documents gzip delivery for later partitions. The SDK advertises gzip only
on partition requests and decodes those bodies with **flate2's Rust backend**. It
enables **no reqwest decompression features**; application clients retain their own
choice of reqwest features. The SDK's own client explicitly disables automatic
decompression even if an application enables it, keeping original `Content-Encoding`,
`Content-Length` and operation metadata available and preventing double decoding.
No additional application feature is needed to fetch Snowflake's gzipped partitions.

The partition decoder accepts JSON with absent/identity encoding or a single gzip
encoding. Other/stacked encodings are refused before body reads. Gzip decoding
validates every member and trailer, rejects corrupt/truncated streams and trailing
junk, and reads at most one byte beyond the inflated cap to detect overflow. It never
uses the gzip size trailer as an allocation instruction. Decoding is synchronous,
after bounded HTTP reception; HTTP deadlines do not preempt decompression or JSON
parsing. Non-partition success documents use Concord's ordinary JSON handling;
compressed error documents remain bounded raw evidence with a JSON decode error.

Metadata-free later partitions are correlated by their requested handle and
index; an echoed handle is checked when present. The documentation does not provide
a complete compressed later-partition specimen: both a `data` envelope and a bare row
array are accepted; fixtures for those shapes are synthetic (see provenance).

The SDK follows the latest correlated `statementStatusUrl` for subsequent status
checks, including links returned by cancel acknowledgements and correlated provider
errors. Query clones share that target. Relative links resolve against the response's
request URL; absolute links must stay on the same account origin and statement path.
Partition links must also match their descriptor index and relation. Invalid,
ambiguous, foreign-account or wrong-handle links produce `ProtocolError::InvalidLink`
with the response head, without a follow-up request. Unknown Link relations are ignored;
links with an `anchor` context are unsupported and refused for known relations.
Redirects/retries remain disabled. It does not expose
custom HTTP middleware or persist/reconstitute a Query across processes. Token
rotation requires a new client. Incomplete first results, expired server results and
account-specific SQL behavior are not repaired automatically.

## Offline validation

Run from the workspace root (the prefix is mandatory for this campaign):

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-snowflake
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo clippy --locked -p concord-sdk-snowflake --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo fmt -p concord-sdk-snowflake
```

`tests/protocol.rs` asserts wire bytes and errors. `tests/workflows.rs` covers both
workflows, extra pending responses, added partitions, independent out-of-order
retrieval, concurrency gates, budgets, caller stops, cancellation races and virtual
deadlines. Testserver owns and shuts down loopback tasks. No test uses credentials or
calls Snowflake. `tests/first_use.rs` is reserved for the independent consumer.
`tests/maintenance.rs` combines an inserted pending poll, two result partitions and
a gated cancel/completion race with both accepted and rejected cancel replies. It
also checks opaque link bytes, cloned-plan updates and handle/link failures. See
[MAINTENANCE.md](MAINTENANCE.md) for the change evidence and compatibility adjustment.
