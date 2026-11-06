# Independent first use

Scope: an application consumer of `concord-sdk-gitlab`, at HEAD `debaefb7`, with
Concord frozen at `e04236ca`. All requests below target loopback testservers and
use `synthetic-first-use-token`. No provider calls or credentials are used.

## Predictions recorded before implementation

### A — page through project issues

Start at `GitLab::with_token(...).with_base_url(...)`, then
`project(ProjectRef::path("demo/platform/reader")?).issues()` with typed filters.
Expect `GET /api/v4/projects/demo%2Fplatform%2Freader/issues`, no initial `page`,
query values for the filters, `PRIVATE-TOKEN`, `Accept: application/json`, and no
request body. A returned `Link` with `rel="next"` supplies the exact next URL,
including its query order/encoding and page size. An empty page with a next link
continues. Missing totals do not imply completion.

`pages().next_page()` should return a vector of typed issues with response
metadata in `Ok(Some(page))`, then `Ok(None)` only after the last page. Repeated
reads after successful exhaustion should send nothing. `items().collect_complete()`
should return all issues only on completion; a failed later page should preserve
the collected prefix and a retry position. A page cap with more available should
yield `PagesError::Bounded`, resumable through the same listing. A later failure
must not turn into `Ok(None)` on another read. A truncated HTTP body should be a
request failure with the response metadata, not a successfully decoded prefix.
Malformed/missing/non-advancing/foreign next links should be explicit pagination
errors and should not cause an unsafe or fabricated request.

### B — retrieve issue and project context

Start at `issue_context(ProjectRef::path("demo/platform/reader")?, IssueIid::new(31)?)`.
Expect `GET /api/v4/projects/demo%2Fplatform%2Freader/issues/31`, using the
project-scoped IID rather than global issue ID `900031`, followed by
`GET /api/v4/projects/73`, using the issue's `project_id`. Both requests should
carry the token and JSON Accept header. Success should contain both typed values
and both response metadata records, including nullable fields.

An issue 404 should return `ContextError::Issue` and make no project request.
A project failure should return `ContextError::Project` with the already fetched
issue and the provider status/document/metadata. A project ID mismatch should
return `ContextError::Mismatch` with both responses. A mislabeled 2xx body should
be a Concord metadata failure. The documented redirect policy should refuse a
cross-origin redirect before sending the token to the other loopback server.

Additional single-call prediction, recorded before adding the comparison test:
the generated `.send()` should expose `concord::Error` with kind `Status` for a
404, while importing `Call` and using `.call()` should expose `GitLabError::Api`
with the parsed message and request ID. This follows the README's two documented
single-request entry points.

## Ordered action/evidence trace

1. First entry point: `examples/gitlab-sdk/README.md`, construction and workflows
   A/B. Read the SDK manifest and `.gitattributes`; neither SDK source nor SDK
   tests nor `FRICTION.md` was read. The new `tests/first_use.rs` starts empty.
2. Read campaign-root `AGENTS.md` and its required top-level handoff/authority
   documents for scope. The user-specific frozen-library/independent-consumer
   instructions govern this role; no author notes were consulted.
3. Public Concord docs used: repository `README.md`, `docs/CALLER_GUIDE.md`,
   `docs/CHEATSHEET.md`, `crates/concord-pagination/README.md`. Generated SDK
   rustdoc with `cargo doc --locked -p concord-sdk-gitlab --no-deps`, and facade
   rustdoc with `cargo doc --locked -p concord --no-deps --features csv,jsonl`.
   Both succeeded with the required build-job/cache prefixes. Used generated
   rustdoc for models, setters, error variants, metadata, `PageItems`, `Pages`,
   and continuations; did not follow rustdoc's Source links.
4. Read `examples/testserver/src/lib.rs` for its public loopback server, request
   recorder, reply script, truncated-body mode, and shutdown API. This is test
   infrastructure, not an SDK source intervention. No existing provider SDK
   supplied the solution.
5. Consulted official [REST](https://docs.gitlab.com/api/rest/),
   [Issues](https://docs.gitlab.com/api/issues/),
   [Projects](https://docs.gitlab.com/api/projects/),
   [authentication](https://docs.gitlab.com/api/rest/authentication/), and
   [troubleshooting](https://docs.gitlab.com/api/rest/troubleshooting/) docs.
   Consumer fixture provenance is recorded alongside the new fixture bytes.
6. Recorded both predictions above before writing outcome test code.
7. First targeted compilation failed with one distinct compiler diagnostic,
   repeated at three call sites: `E0599`, no `TestServer::spawn` associated
   function. Re-read the public testserver constructor (`start`/`start_with`)
   and corrected the consumer's three constructor calls to `TestServer::start`.
   This was a consumer/test-harness spelling mistake, not an SDK diagnostic.
8. Re-ran the requested targeted command: all initial 17 tests passed. Predictions
   held, including exact linked request targets, absent totals, empty intermediate
   pages, explicit exhaustion, cap/resume, failed-page prefix/retry, and truncated
   HTTP bodies. Bad links stopped with typed errors; cross-origin link and
   redirect destinations received no requests.
9. Added the separately predicted `.send()`/`.call()` comparison to make the
   single-call error-surface choice concrete. Applied rustfmt only to the new
   test file. Final targeted command: **18 passed, 0 failed, 0 ignored**. No
   further compiler diagnostics or SDK behavior corrections were needed.
10. Checked the fixture attributes: `text: unset` for the new JSON/HTML bytes.
    The existing `fixtures/** -text` rule already covers the new directory.

## Diagnostics, interventions, and results

Only compiler diagnostic: the testserver `E0599` above. First corrective read:
the testserver's public constructor. **SDK source/test interventions: none.**
Native request escapes, private reach-through, manual operation names, rebuilt
execution: none. SDK source, README, existing tests, author fixtures, and the
frozen Concord library were left unchanged; `FRICTION.md` was not read.

Working application paths:

- A: typed listing setters → `pages().next_page()` or
  `items().collect_complete()` → `PagesError::Provider`/`Bounded` → the same
  listing's `pages_from(resume)`. A failed reader subsequently returns `Ended`,
  while successful exhaustion returns `Ok(None)` repeatedly without I/O.
- B: `issue_context(project, iid)` → issue, then its numeric project ID → both
  typed values and metadata, or the documented partial-context variant.
  `Call::call()` provides the same SDK error/document handling for single calls.

Final reproduction command (offline provider fake; no live-provider verification):

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-gitlab --test first_use
```

## Concrete friction

No blocking SDK defect or undocumented workaround was encountered. The following
are concrete, documented adoption costs rather than failed SDK promises:

- **Two single-call error surfaces.** Generated `.send()` returns a Concord
  status error; the SDK's parsed provider message and `ResponseMeta` require
  importing `Call` and using `.call()`. Reproduction:
  `b_single_calls_require_call_for_the_sdk_error_document_and_metadata` sends the
  same operation against two synthetic 404 documents and checks both surfaces.
  The public README was sufficient to choose `.call()`; no source read was needed.
- **Redirect protection is caller setup.** The application must include
  `.redirect(concord_sdk_gitlab::redirect_policy())` in the transport builder.
  Reproduction: `b_documented_redirect_policy_does_not_forward_token_to_another_origin`
  returns a 302 pointing to another loopback port; the configured client returns
  an issue-stage API error and the destination receives nothing. This verifies
  the documented policy path; this suite does not test the default policy.
- **Listing failures have two error layers.** The page position/prefix lives in
  `PagesError`/`Incomplete`, while provider status, document and request ID live
  in `GitLabError::Api`. Reproduction:
  `a_failed_page_preserves_prefix_document_metadata_and_retry_position` collects
  one issue, receives a 429, reads both layers, and explicitly retries only the
  returned continuation. No manual page URL or operation identity is constructed
  in the application traversal.

## Between waves 2-3 (coordinator integration)

`Call::call()` is now the generated `send()` (client statement `send SendPolicy;`). The tests
in `tests/first_use.rs` call `.send()`, and the Concord-only comparison calls
`prepare()?.send()`; expected values are unchanged. See [INTEGRATION.md](INTEGRATION.md).
