# TERRA caller task — recorded before reviewing author conclusions

Task: add GitHub repository issue-event listing as an external SDK extension, traverse its Link pages and call it against loopback. The scratch implementation is `task/src/lib.rs`; `CARGO_TARGET_DIR=../target cargo test --offline` from `task` passes one test. It verifies two request targets, inherited auth, agent and version headers, response items and server cleanup. This is a small illustrative subset of the provider response, not a complete production SDK: its native error handling does not preserve Concord's redacted error semantics, it buffers JSON, the page budget is fixed, and it does not expose a continuation to the caller. No provider was contacted.

Material read before finishing: `docs/AUTHOR_GUIDE.md`, `docs/CALLER_GUIDE.md`, `docs/RECIPES.md` (terminal output truncated); public source/rustdoc in `examples/github-sdk/src/{lib,api,context,paging,pages,params,models,meta,link,error,ids,filters}.rs`; `examples/github-app/src/{lib,audit}.rs`, `examples/github-sdk/tests/paging.rs`, `crates/concord-pagination/README.md` and `src/sequential.rs`, `examples/testserver/src/lib.rs`, and relevant Cargo manifests. Did not read author conclusions or evidence files before the task was complete.

Touched two scratch files: manifest and source; within source, four conceptual places: event model, native request/page fetch, traversal/caller, and loopback test. The production extension needs its own operation and pagination adapter: the SDK's public `LinkPages::new` requires a generated operation plus `fn(Op) -> Result<Prepared<R>, concord::Error>`, while `RepoContext::request` returns a native request. The workaround uses `ResponseMeta` and the pagination crate directly, duplicating same-origin checking and Link extraction from the SDK adapter. It inherits credentials and configured headers without declaring them again. I learned seven concepts: native repo context; base-relative segments; request query construction; capturing metadata before consuming the response; Link resolution and origin validation; sequential `Page`/`Budget`/`Resume`; loopback replies and recorded requests. Existing generated listing declaration and page adapter repeat `per_page` and method-specific adapter blocks; my native extension does not duplicate their declarations but has no generated setters or decoder. The tests model only minimal events. `per_page` is a raw u8 here, unlike the SDK's validated `PerPage`.

Compiler output captured verbatim in `task-build.log` and `task-build-2.log` (full paths, diagnostics and help); notable errors:

```text
error[E0599]: no method named `json` found for struct `Response` in the current scope
  --> src/lib.rs:28:51
   |
28 |             let items: Vec<IssueEvent> = response.json().await?;
   |                                                   ^^^^ method not found in `Response`

For more information about this error, try `rustc --explain E0599`.
error: could not compile `terra-review-task` (lib) due to 1 previous error
```

The reqwest re-export did not have the `json` feature enabled; I added `serde_json` and used `response.bytes()` plus `from_slice`. Next, I omitted the status argument on the test server:

```text
error[E0061]: this function takes 2 arguments but 1 argument was supplied
   --> src/lib.rs:48:17
    |
 48 |                 Reply::json(r#"[{"id":1,"event":"closed"}]"#).header("link", "<?page=2>; rel=\"next\"")
    |                 ^^^^^^^^^^^ -------------------------------- argument #1 of type `u16` is missing
    |
note: associated function defined here
   --> /home/canarit/projects/concord-ext44/rv44-terra/examples/testserver/src/lib.rs:216:12
    |
216 |     pub fn json(status: u16, body: impl AsRef<[u8]>) -> Self {
    |            ^^^^
help: provide the argument
    |
 48 |                 Reply::json(/* u16 */, r#"[{"id":1,"event":"closed"}]"#).header("link", "<?page=2>; rel=\"next\"")
    |                             ++++++++++

error[E0061]: this function takes 2 arguments but 1 argument was supplied
   --> src/lib.rs:50:17
    |
 50 |                 Reply::json(r#"[{"id":2,"event":"reopened"}]"#)
    |                 ^^^^^^^^^^^ ---------------------------------- argument #1 of type `u16` is missing
    |
note: associated function defined here
   --> /home/canarit/projects/concord-ext44/rv44-terra/examples/testserver/src/lib.rs:216:12
    |
216 |     pub fn json(status: u16, body: impl AsRef<[u8]>) -> Self {
    |            ^^^^
help: provide the argument
    |
 50 |                 Reply::json(/* u16 */, r#"[{"id":2,"event":"reopened"}]"#)
    |                             ++++++++++

For more information about this error, try `rustc --explain E0061`.
error: could not compile `terra-review-task` (lib test) due to 2 previous errors
```

Fixed by `Reply::json(200, ...)`; final successful output is `task-build-3.log`. The first invocation was started from a non-existent working directory and never ran a shell; the scratch directory was then created from the repository root. No compile-fail experiment was performed for the `LinkPages` mismatch: that observation is from the public signatures.

## Post-task correction

After recording the first result and beginning the review, I noticed `Decoder::<Vec<IssueEvent>>::new::<Json>()` supports the native extension directly. I replaced raw `error_for_status`/`serde_json` with `Decoder::decode` and `finish(Err(error))`, removed `serde_json`, and reran the loopback test successfully (`task-build-4.log`). It now keeps Concord's redacted transport/status/head/decode semantics; native URLs remain origin-checked before following. The original compiler errors and workaround above describe the initial attempt, not the final code. The LinkPages incompatibility and separate traversal adapter remain. This still buffers each page and is a deliberately limited example, not a complete provider SDK.

A final task cleanup sends non-success responses directly to the decoder before interpreting Link, so a malformed Link cannot mask a status error. The successful rerun is `task-build-5.log`; the test exercises the successful two-page path only, not that error branch.
