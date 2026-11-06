# First-pass SDK author observations

Frozen Concord baseline: `47a25fbf0252e11291ab81459add7a596ac5609c`.
This work changes no library, scripts, checks, docs or other examples. The only root
changes are the workspace member and lockfile package entry. No private reach-through,
manual HTTP execution, handwritten operation identity, or native transport escape was
needed. Entries below record observed implementation work and checks, not reasoning
traces. GitHub SDK api/meta/error/pages source helped locate public patterns.

For each named reproduction below, substitute its suite/test in this command:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-notion --test <suite> <test>
```

1. **Shared headers, path encoding and JSON bodies worked directly.**
   Cause: Concord capability; resolved without a workaround.
   `src/api.rs:11` declares sensitive bearer auth and a version parameter once;
   the query's cursor lives in its JSON body while children use an optional query
   parameter. Concord supplies encoding, Content-Type and Accept. Observed exact
   bytes matched the hand-authored request fixtures. Concurrent cloned clients sent
   one independent version/auth pair each, without duplicated or leaked headers.
   Reproduce: `tests/protocol.rs:41`, suite `protocol`,
   `query_posts_exact_body_bytes_and_retrieval_has_no_body`; also
   `tests/protocol.rs:114`, `concurrent_clients_keep_version_and_auth_context_independent`.

2. **Response metadata and bounded body reads were available, but typed provider
   failures still need a response adapter.**
   Cause: SDK/provider mapping; Concord's public boundary handles execution correctly.
   `src/client.rs:219` calls `Prepared::receive`, captures `ResponseMeta`, then selects
   `decode_limited` for success or `bytes_limited` and JSON error parsing for failure.
   The SDK checks the error object's status against the actual HTTP status, keeps raw
   malformed error bytes, and clones metadata into decoding/read errors. This is about
   27 lines of repeated response-policy plumbing around the three operations; it felt
   like the reusable boundary of a typed-error helper, but no transport was rebuilt.
   Concord retains the generated operation throughout. Failures after headers kept
   status and request metadata, including after a cut-off body and a limit failure.
   Reproduce: `tests/protocol.rs:166`, suite `protocol`,
   `documented_provider_failures_retain_code_status_and_headers`; additionally
   `tests/protocol.rs:359`, `wrong_success_media_type_and_truncated_body_keep_metadata`.

3. **Ordinary traversal needs small owned factories for each provider operation.**
   Cause: SDK adaptation to Concord-pagination's public factory API; no library defect
   observed. At `src/client.rs:138` and `src/client.rs:168`, the SDK owns a cloned
   client/parent/query, returns an async move future, and projects list results/cursor
   plus metadata at `src/client.rs:191`. Query and children factories repeat the
   clone/project/cycle-check scaffolding. The query cursor must be inserted into a body
   (`src/params.rs:89`); the children cursor uses the generated optional setter.
   The engine supplies caps, typed provider failure, retained prefixes, streams and
   truthful exhaustion without rebuilding a scheduler. Public `_from` methods were
   added so callers can use a page resume key without writing another factory.
   Reproduce: `tests/workflows.rs:193`, suite `workflows`,
   `cap_is_distinct_from_exhaustion_and_preserves_a_resume_key`; also
   `tests/workflows.rs:406`, `item_stream_surfaces_failure_before_ending`.

4. **Query exhaustion needs a provider-specific completeness check.**
   Cause: provider semantics. At `src/client.rs:100`, any supplied request_status other
   than complete returns `IncompleteQuery`, retaining this response and its results.
   The official query docs describe an incomplete capped result even when has_more is
   false. A generic cursor projection alone would miss it. The test's first response
   remains in the traversal prefix; the capped response remains inside the error.
   Unknown completeness statuses are also retained and refused. No workaround to
   Concord was needed, but the SDK must maintain this rule when versions change.
   Reproduce: `tests/workflows.rs:280`, suite `workflows`,
   `provider_query_cap_does_not_masquerade_as_exhaustion`; also
   `tests/workflows.rs:233`, `unfamiliar_query_status_is_retained_and_never_declared_complete`.

5. **Unknown nested variants require explicit JSON discrimination.**
   Cause: SDK/schema evolution; independent of Concord's HTTP declaration.
   `src/models.rs:168`, `src/models.rs:227`, and `src/models.rs:295` deserialize
   properties, rich text and blocks by type. Known fields are parsed strictly;
   unfamiliar types retain their entire Value, and rendering emits a marker at
   `src/render.rs:7`. This custom code is substantial relative to the 32-line API
   declaration. It avoids a catch-all accepting malformed known payloads. It also
   retains a relation's has_more flag and children under an unknown block. Known
   annotation/style fields remain deliberately unmodeled, as documented.
   Reproduce: `tests/protocol.rs:246`, suite `protocol`,
   `supported_properties_and_unknown_payloads_survive_together`; also
   `tests/protocol.rs:330`, `malformed_supported_variants_fail_decoding_instead_of_becoming_unknown`.

6. **Tree expansion and property completeness remain explicit caller work.**
   Cause: provider scope and application policy. The children endpoint returns one
   level, and retrieve_page cannot stand in for block content or property-item
   pagination. `tests/workflows.rs:93` retrieves a second children page, then separately
   expands known and unknown parents. README Workflow B provides compiled application
   code with depth/block/page limits and repeated-parent detection. The SDK does not
   claim a complete property export or hide unknown content. No generic library
   recursion or provider mutation was attempted.
   Reproduce: suite `workflows`,
   `workflow_b_reads_second_children_page_and_expands_known_and_unknown_parents`.

7. **Two initial test API spelling mistakes were SDK-author errors.**
   Cause: SDK test implementation, corrected. The first locked compile reported E0599
   for `TestServer::only_request` and `Resume::Key`. Inspection showed the public names
   are `single` and `Resume::At`; those were used instead. Missing integration-test
   crate documentation was also added before the warnings-as-errors gate. No library
   change was made. Final corrected call sites are `tests/protocol.rs:97` and
   `tests/workflows.rs:182` / `tests/workflows.rs:205`.
   Reproduce the corrected paths with suite `protocol`,
   `children_cursor_is_query_encoded_and_never_a_post_body`, and suite `workflows`,
   `a_failed_second_page_keeps_the_prefix_and_typed_provider_error`. Replacing these
   names with the initial spellings reproduces the compiler diagnostics.

Validation observed: one unlocked test invocation registered the new package, then
locked tests passed. Final suite totals are 11 protocol tests, 10 workflow tests and
5 doctests (README examples included). All are offline, not vendor conformance.
Formatting completed and Clippy with `--all-targets -- -D warnings` passed after the
final source changes. The exact commands are in README. All 15 fixture files parsed
as JSON and had a provenance entry; the frozen-path and root-member checks passed.
No new registry packages were added.
