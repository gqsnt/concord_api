# SDK19 author observations

Author pass on 2026-11-06 against frozen Concord **47a25fbf**. This is a factual
implementation record, not a library patch proposal or provider conformance claim.
Entries are ordered by the boundary encountered. Locations are relative to this
SDK directory. Reproductions run offline through `tests/protocol.rs` and
`tests/workflows.rs`:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-microsoft-graph
```

1. **F1 — Declaration handled request composition without a native escape.**
   Tried declaring three typed GET operations, optional `$select`, sensitive
   authorization, an encoded group segment and conditional advanced-query headers.
   Result: generated preparation supplied GET, JSON Accept, no Content-Type and
   zero body bytes. No handwritten request assembly or HTTP execution was needed.
   Reproduction: `src/api.rs:8`; `tests/protocol.rs:16`, `:50`, `:72` assert the
   actual wire requests, including `group/one` becoming one encoded path segment.
   Cause: **Concord, positive**. The supported field enums and forced selected id
   are SDK scope decisions in `src/model.rs:140` and `src/client.rs:299`.

2. **F2 — Opaque continuation execution was already public and preserved bytes.**
   Used `Prepared::follow` after validation. Result: the generated method,
   credentials, declared JSON handling and operation identity survived; query
   escapes, duplicate parameters and plus signs were retained without reappending
   selection. No `into_parts`, `__private`, reconstructed executor or handwritten
   operation identity appears in the adapter. Reproduction: `src/paging.rs:105`;
   `tests/protocol.rs:91`; the member continuation also keeps ConsistencyLevel in
   `tests/workflows.rs:130`. Cause: **Concord, positive**. GitHub's listing pattern
   and Salesforce's receive-before-decode pattern were useful reading material.

3. **F3 — Origin checks do not bind a Graph authorization/query context.**
   `follow` checks origin, but tenant, principal/permission-profile, selection,
   collection path and link kind still need SDK checks. It also leaves redirect
   policy to the supplied transport. Implemented versioned opaque-link wrappers,
   strict destination validation and a private generated client behind a transport
   with redirects disabled. Context is caller-asserted, not inferred from a token.
   Reproduction: `src/links.rs:48` (binding check), `:98` (destination rules),
   `src/client.rs:126` (transport); `tests/protocol.rs:410`, `:479`, and
   `tests/workflows.rs:240`, `:298`. Tests observe no extra request when a link is
   refused, including two tenants sharing one origin. Cause: **provider +
   application policy, implemented in SDK**; this is necessary policy above
   Concord, not evidence that its documented same-origin guard failed. Remaining
   limit: saved JSON is not authenticated and does not bind application staging
   to a particular run of the same query.

4. **F4 — Delta completion needs page metadata; item collection loses it.**
   The generic `Pages` adapter accepts items, next key and metadata, but its
   `.items()` projection does not retain metadata. Graph's final deltaLink is a
   completion checkpoint, not another page key. Implemented `PageMeta.delta_link`,
   validated exactly one next/delta link per delta page, and kept workflow A on
   `next_page`/page collection. The caller accumulates the final link and commits
   it with its staged changes. Reproduction: `src/paging.rs:16`, `:141`, `:170`;
   `tests/workflows.rs:31`, `:349`; complete caller code at `README.md:55`.
   Cause: **provider semantics + Concord facade shape**. No scheduler was
   rebuilt, but an item-only consumer cannot perform the full delta workflow
   without an additional SDK completion wrapper. The present public page path
   handles this explicitly and retains prefixes on failure.

5. **F5 — Typed provider errors still require an SDK response branch.**
   Tried `receive` → `meta` → `decode_limited` for success and
   `bytes_limited` → typed JSON envelope for non-success. Result: actual status,
   request IDs, Retry-After, nested provider codes and generated operation identity
   remain available after body consumption. Oversized, malformed and interrupted
   error bodies are distinct. The adapter still writes the status branch, JSON
   envelope conversion, metadata attachment and classification instead of a
   declarative typed error mapping. Reproduction: `src/paging.rs:109`,
   `src/error.rs:114`, `:203`; `tests/protocol.rs:118`, `:220`, `:257`, `:296`.
   Cause: **SDK provider mapping, supported by Concord's public response boundary**.
   No manually rebuilt transport/error URL redaction was necessary. A future
   reusable mapper would need to preserve these failure distinctions.

6. **F6 — Ordinary paging and partial outcomes needed no scheduler work.**
   Wrapped the page factory in `Pages::new` with a default 1000-page cap. Result:
   empty intermediate pages continue; exact-last-page caps exhaust; failed/capped
   traversals emit an error and never later report successful exhaustion;
   collected prefixes and resume keys survive. Standard item streams emit the
   error before EOF. Canceling `next_page` retains one in-flight request.
   Reproduction: `src/paging.rs:192`; `tests/workflows.rs:349`, `:384`, `:433`,
   `:460`, `:503`. Cause: **Concord pagination, positive**. A bounded traversal
   still needs application staging; no automatic publication was added.

7. **F7 — A common public listing needs a small adapter around generated types.**
   The three generated operations have different concrete types. Used one
   `Arc<dyn Fn() -> Result<Prepared<Envelope<T>>, _>>` factory, with a short closure
   per domain operation, to expose `Listing<T>`. Listing construction prepares an
   unsent request to obtain its correctly encoded collection path; fetch prepares
   it again. This duplicates preparation, not network execution. Reproduction:
   `src/paging.rs:28`, `:60`, `:94`; `src/client.rs:182`, `:240`, `:257`;
   `tests/protocol.rs:72` confirms the generated path encoding and one request.
   Cause: **SDK abstraction over Concord's per-operation types**. This is the
   main adapter boilerplate beyond Graph's own protocol checks. No generic
   scheduler/private API was copied to remove it.

8. **F8 — Sparse updates and member visibility belong in provider models.**
   `Option<String>` alone would collapse an omitted delta property and explicit
   null. Added a serde-backed `Field<T>` with Missing/Null/Value, and kept exact
   member types plus unknown properties. Type checks precede association with the
   application's known-user index. Reproduction: `src/model.rs:9`, `:93`, `:127`;
   `tests/workflows.rs:31`, `:130`. Result: the empty delta perturbation continues,
   limited users/devices and a synthetic future type survive, and a non-user with
   a matching synthetic id is not joined. Cause: **provider semantics + SDK**,
   not a Concord decode limitation. No claim of complete/transitive membership
   or a workaround for the documented v1.0 service-principal omission is made.

9. **F9 — Compile/lint corrections were local SDK errors.**
   First compilation reported E0433 for a guessed `reqwest::UrlHost` re-export.
   Corrected loopback validation to use the re-exported Url's `host_str` and
   `std::net::IpAddr` (`src/client.rs:95`). Initial test compilation reported E0283
   for `Vec<u8>::as_ref`; used `as_slice` (`tests/support/mod.rs:41`). First clippy
   pass reported `result_large_err` (provider variant at least 184 bytes); boxed
   the parsed ProviderError (`src/error.rs:116`) without dropping metadata.
   Result: tests and `clippy --all-targets -- -D warnings` pass. Cause: **SDK**;
   none required editing Concord. There were no observed failures requiring a
   frozen-library workaround.

## Verification record

- Generated Concord rustdoc with the requested csv/jsonl features; completed.
- One unlocked SDK test invocation added the package to Cargo.lock. Subsequent
  SDK test/clippy commands used `--locked`; no new registry package versions were
  introduced.
- Last full SDK test run observed **17 protocol tests + 9 workflow tests + 3
  README doctests passing** (0 unit tests, 0 failures).
- SDK formatting completed; clippy with `--all-targets -- -D warnings` passed
  after the boxing correction. Final clippy rerun and scope audit completed
  successfully before handoff.
- Only this SDK directory, the workspace-member insertion and Cargo.lock are
  changed. No library, script, check, other example or held-out campaign record
  was edited. No live Graph call, credential acquisition, commit or publication
  was performed by this author.
