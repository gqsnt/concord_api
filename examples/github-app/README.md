# concord-example-github-app

An external caller of [`concord-example-github-sdk`](../github-sdk): the two 4.4 GitHub workflows
as library functions, using only the SDK's public exports, `concord` and `concord-pagination`.
Both are serial (GitHub asks for serial requests) and never retry.

| Workflow | Function | Input | Output |
|---|---|---|---|
| GH-A repository activity audit | `audit::activity_audit(&RepoContext, AuditInput) -> ActivityReport` | `IssueFilter`, `AuditBudget { issue_pages, child_pages }`, optional repository `ETag`, optional `AuditContinuation` | repository state (`Fetched`/`NotModified`), `ActivityEntry::{Issue, PullRequest}` with comments or details + reviews, request log, `Completion::{Complete, Bounded, Interrupted}` with continuation |
| GH-B workflow artifact retrieval | `artifacts::retrieve_artifact(&RepoContext, &RetrievalInput, &mut impl ArtifactSink) -> Retrieval<E>` | `RunFilter`, `RunSelector`, artifact name, `RetrievalBudget`, `DownloadLimits`, a bounded sink (`BoundedBuffer`, `FileSink`, both with `CancelFlag`) | request log and `RetrievalOutcome::{Retrieved, NoMatchingRun, NoMatchingArtifact, Expired, Incomplete}` |

## Commands

```sh
cargo test -p concord-example-github-app --test gh_a_activity_audit      # GH-A, loopback
cargo test -p concord-example-github-app --test gh_b_artifact_retrieval  # GH-B, loopback
cargo test -p concord-example-github-app --test manifest                 # fixture manifest
cargo run  -p concord-example-github-app --bin gh_a   # live, read-only; NOT_RUN without GITHUB_TOKEN, GH_REPO
cargo run  -p concord-example-github-app --bin gh_b   # live, read-only; NOT_RUN without GITHUB_TOKEN, GH_REPO, GH_ARTIFACT, GH_ARTIFACT_OUT
```

The binaries call the same functions as the tests; only the transport target, credentials and
budgets differ.

## Acceptance evidence (contract `SDK_INTEGRATIONS.md`, GH-A and GH-B)

| Requirement | Test |
|---|---|
| GH-A multiple pages; empty page with `next` continues; absent `last`; no `next` ends | `gh_a_activity_audit::complete_audit_over_multiple_pages` |
| GH-A optional filters on the wire; issue vs pull-request representations; nested child collections | `complete_audit_over_multiple_pages` (`state`, `labels`, `since`, `per_page`; `ActivityEntry::PullRequest` with Pulls API id 7012 vs issue id 5012) |
| GH-A request/rate metadata | `complete_audit_over_multiple_pages` (request ids, `last_rate`) |
| GH-A ETag conditional refresh, `304` | `conditional_refresh_with_etag` |
| GH-A child-page failure after earlier successes, resumption | `child_failure_after_successes_then_resume` |
| GH-A bounded traversal with continuation | `parent_budget_then_resume`, `child_budget_bounds_each_collection` |
| GH-A parent-page failure | `parent_page_failure` |
| GH-A context isolation (two repositories, two tokens) | `context_isolation_between_repositories` |
| GH-A changed listing on resume | `stale_continuation_is_refused` |
| GH-A no secrets/contents in `Debug`/`Display` | `report_rendering_is_redacted` |
| GH-B run selection over pages, jobs, artifact envelope, `302` Location, file sink | `gh_b_artifact_retrieval::retrieves_into_a_file_without_forwarding_the_token` |
| GH-B token not forwarded to the download host (second server via `resolve`) | same test, and `context_isolation_between_repositories` |
| GH-B expired artifact | `expired_artifact_by_metadata` (`expired: true`), `expired_artifact_by_410` |
| GH-B transport failure mid-body | `transport_failure_mid_body` |
| GH-B sink cancellation | `sink_cancellation`, `dropped_workflow_stops_the_transfer` |
| GH-B bounded sink and byte limit | `bounded_sink_and_size_limit` |
| GH-B signed URL refused by the download host | `expired_signed_url` |
| GH-B no matching run / artifact | `no_matching_run_or_artifact` |

SDK-level evidence (every operation on the wire, Link parsing, page factory, download boundary,
official-example decoding) is in `examples/github-sdk/tests/`.

## Evidence labels

| Workflow | Label | Scope |
|---|---|---|
| GH-A | `OFFLINE_VERIFIED` | loopback GitHub stand-in serving `tests/fixtures` (schema-derived synthetic data, fault injection) |
| GH-A | `NOT_RUN` (live) | no credentials were provided |
| GH-B | `OFFLINE_VERIFIED` | loopback API and a separate loopback download host; generated archive bytes |
| GH-B | `NOT_RUN` (live) | no credentials were provided |

## Policies

- **GH-A** stops at the first failure and keeps everything collected; the continuation restarts at
  the parent item whose child failed (that item is re-emitted, pull-request details refetched,
  the child resumed at its failed page). The report is a best-effort enumeration of a live listing,
  never a snapshot; a resumed page whose item moved is refused (`StaleContinuation`).
- **GH-B** treats `expired: true` and a `410` from the download endpoint as `Expired` (nothing
  downloaded). Bytes stream into the sink as they arrive; any transfer failure, limit or
  cancellation aborts the sink (a `FileSink` removes its `.partial` file) and reports the bytes
  that had been written. A digest GitHub reports is passed through, not verified.

Fixtures and their provenance: [`tests/fixtures/MANIFEST.json`](tests/fixtures/MANIFEST.json).
