# Fresh-use trial

Snapshot HEAD expected 2b09425c477aed1eb0640a322bf20407fd6a2c68; observed via git rev-parse: same. Original source SHA 389392fb5254c7d0093d86e07d60c729524ef072 was supplied, not independently verified. Requested model gpt-6.1-terra and effort xhigh; runtime does not expose effective model or reasoning effort to this process, so they cannot be independently verified. Toolchain file specifies 1.97.1.

## Navigation log
1. `docs/QUICKSTART.md`, sections 1–6 and Next steps; `examples/quickstart/` inventory. `examples/quickstart/README.md` does not exist.
2. Before `cargo doc -p concord --no-deps --locked --offline`: expect successful public rustdoc build with the pinned toolchain and cached dependencies. Outcome pending.
3. `examples/quickstart/src/lib.rs`, `examples/quickstart/tests/quickstart.rs` (declaration and signatures), then `examples/riot/tests/support/mod.rs` (loopback server). Copied server, removing Riot-specific helpers that require Riot types.

## Task 1 — before run
Author prediction: `Notes::read(&self, impl IntoValue<String>) -> NotesRead`, `Notes::create(&self, impl IntoValue<NewNotebook>) -> NotesCreate`; both send/prepare decode Notebook as JSON. Caller prediction: GET `/v1/notebooks/n%2F1` with no body, POST `/v1/notebooks` with JSON `{"title":"Field"}` and application/json; both carry Bearer secret and tenant t1. I expect the isolated crate to compile and the loopback assertions to pass with the copied lock and `--locked --offline`.
Outcome: first run failed before compilation, contrary to the copied-lock prediction:
```text
error: cannot update the lock file .../trial/Cargo.lock because --locked was passed to prevent this
help: to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
```
The standalone workspace needs its own root package recorded. Smallest improvement: document the offline lockfile refresh needed after copying, then use `--locked` for builds. Before refresh: expect `cargo generate-lockfile --offline` to succeed from the cache; then a locked test should compile and check the predictions above.
Outcome: offline lock regeneration succeeded and the locked test passed. The signatures compiled and the wire assertions matched. Regeneration resolves a lock for the separate workspace; version comparison is recorded below. `cargo doc -p concord --no-deps --locked --offline` succeeded; its actual output directory was `target/review-build/doc`, not `target/doc`, due to repository Cargo configuration. Navigation: rustdoc `concord/index.html`, Quick start, declaration and runtime/native request sections; then `docs/AUTHOR_GUIDE.md` §5, §5.1–5.2 and §5.4.

## Task 2 — before run
Author prediction: endpoint-level `response text`, `request text "text/markdown"`, and `response bytes` override client JSON independently. Caller prediction: raw markdown decodes to String, replacement sends exact UTF-8 with text/markdown and accepts 204, attachment returns unchanged bytes; expected test compiles and passes.
Outcome: passed; both raw formats and media type behaved as predicted.

## Task 3 — before run
Author prediction: `body patch?: Option<NotebookPatch>` creates a positional-free `update(notebook)` request with `set_patch(Option<NotebookPatch>)`, separately from `maybe_patch(Option<Option<NotebookPatch>>)`. Caller prediction: omitted -> no body or content type; `set_patch(None)` -> `null` plus application/json; `set_patch(Some(...))` -> object with only present fields. Expected test passes. The fictional API does not specify a PATCH response, so this trial assumes JSON Notebook; the wire assertions concern the request only.
Outcome: passed for all three request states.

## Task 4 — before run
Author prediction: grouping under `notebooks` changes navigation but preserves notebook JSON; `pages` inherits `format text`, its metadata exception returns JSON, and replace overrides only the request media type. Caller prediction: list emits optional query values, text remains raw, metadata decodes JSON, and previous assertions continue to pass. Expected `cargo test` and generated rustdoc build to succeed.
Outcome: all four tests passed. `metadata` is an invented test-only operation to exercise a JSON exception under the text page scope, not a claimed Field Notes interface. Navigation subsequently: `docs/CALLER_GUIDE.md` §§2–3, 5–7; `docs/AUTHOR_GUIDE.md` §4.8.

## Task 5 — before run
Author prediction: using standard `AUTHORIZATION` and custom `X_NOTES_TENANT: HeaderName` produces the same header names as before. Caller prediction: `set_token` and `set_tenant` consume a handle and yield a new snapshot; a scope captured earlier sends old values, later requests send new values, and Debug redacts token. Expected test passes.
Outcome: passed; observed old and new headers and redaction as predicted.

## Task 6 — before runs
Author prediction: `TENANT` and literal `"x-notes-tenant"` are indistinguishable when resolved but the macro cannot know the constant's value; compilation succeeds, and `prepare()` returns a Build error about the duplicate. Caller prediction: nothing sent. For `"bad\n{token}"`, I expect a compile-time diagnostic naming the invalid header template literal, not a runtime error.
Outcome: compilation and Build error matched. Verbatim runtime message:
```text
GET Collision.go: request not built: `x-notes-tenant` on `Collision` sets a header that an enclosing owner already sets (possibly with another spelling of the same name); use `override` or `append`, or `remove` it first
```
Surprise: both contributions are on the same owner, so "an enclosing owner" is inaccurate. Small public message change: say "another declaration" (or distinguish same-owner duplicates). No server was involved. Invalid template compilation produced:
```text
error: this header value contains a byte that `http::HeaderValue::from_bytes` rejects (at byte 3 of literal fragment 0; no value can repair it)
 --> examples/invalid_header.rs:4:30
```
As predicted. The reproduction is retained as `examples/invalid_header.rs.txt` so ordinary test builds do not intentionally fail. Navigation: `docs/AUTHOR_GUIDE.md` §5.7, `docs/RECIPES.md` custom NDJSON, `examples/native/src/repr.rs`.

## Task 7 — before run
Author prediction: a separate workspace using only the dependency name `notes_http` can implement Encode/Decode and refer to its own `Ndjson` format, without a crate named `concord`. Caller prediction: two records send as newline-delimited JSON with application/x-ndjson and response decodes to rows. The import endpoint is fictional trial scaffolding. Expect offline lock generation and locked test to pass.
Outcome: renamed downstream codec compiled and its loopback request and response passed as predicted. Cargo doc on trial also succeeded; opened generated `field_notes_trial/struct.NotesNotebooksPages.html`, methods text, replace and metadata: it reports the expected format and origin. 

## Task 8 — before run
Caller prediction: 503 makes `send()` return a Status error holding the unread native response; `into_response()` exposes 503, both headers and exact HTML bytes without JSON decoding. Expected loopback test passes.
Outcome: passed; status, headers and HTML bytes were available unchanged.

## Task 9 — before run
Caller prediction: `prepare()?.into_parts()` permits a reqwest timeout and extra header; `decoder.finish(client.execute(request).await)` retains the declared JSON decoding. Expected test passes and loopback observes x-trace. Timeout is set on the native request; this test does not prove timeout expiration.
Outcome: passed, including the extra native header and typed decode.
Navigation: `examples/riot/README.md`; `examples/riot/tests/workflows.rs`; `examples/riot/src/lib.rs` (workflow), `fixtures/README.md`, and relevant sections of `examples/riot/src/api.rs`, `src/types.rs`, `tests/wire.rs`. These are example code/tests, not library source/tests.

## Task 10 — before run
Caller prediction: loopback DNS mapping from the existing support helper keeps routed Riot names while sending only to 127.0.0.1; player summary resolves account, summoner, mastery, IDs and two matches (six GETs) and sees fixture player and Ahri. Then create_codes sends POST to Americas with tournamentId=9&count=2, application/json, documented camelCase body, and returns two codes from the fixture. Expected one new integration test passes, seven requests total; no Riot network traffic.
Outcome: passed: six workflow requests and one stub POST, values and JSON body as predicted. The compiler emitted `warning: missing documentation for the crate`; I added a crate doc comment. Before rerun: predict the same test passes without that warning.
Rerun outcome: passed with no warning. The server's `Drop` aborts its listener task after each test; no external Riot request was made.

## Triage and evidence boundaries
- **Author-side diagnostic issue:** the same-owner native/literal collision is correctly rejected at preparation but the "enclosing owner" wording is misleading. Suggested smallest public change: identify a conflicting declaration without asserting that it belongs to an enclosing owner. This is not a finding that duplicate headers were sent.
- **Trial setup friction:** copied lock alone fails `--locked` for a new workspace. A short setup instruction to refresh its lock offline would avoid the wrong prediction. Both refreshed lockfiles were compared by package name, version and source against the original: the only package absent from the original is the respective new trial root; no dependency version/source changed.
- **Ambiguity:** PATCH response is unspecified; I chose JSON Notebook for the mock, so the request states are tested but the real service's response is not established. The metadata and import endpoints are test scaffolding for format behavior, not Field Notes claims. The fictional list example shows `archived=true`; the optional bool declaration also permits false, which this trial did not separately test.
- **Caller-side:** the old handle retains its snapshot after rotation. Status errors retain unread native responses; native edits can retain typed decoding. These observations are limited to the tested calls. Riot results prove synthetic fixtures and loopback routing only, not live Riot authorization or behavior.
- **Personal execution:** the commands and tests described here, the loopback assertions, lock comparison, compiler diagnostic and generated docs were actually run/read during this trial. Example tests and example code were inspected for interfaces; their prior assertions are not presented as my test results. I did not run the full Concord suite.
- **Source boundary:** no library source, library tests, macro expansions or `v4/` design records were inspected; there were no transitions into them. Public examples and their tests were read, as allowed. No tracked file was modified. The only files outside `trial/` created for this work are `examples/riot/tests/trial_fresh_use.rs` and build artifacts in ignored target directories.
- **Settings:** `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1 (c980f4866 2026-06-30)` and active toolchain `1.97.1-x86_64-unknown-linux-gnu` were personally queried. Model `gpt-6.1-terra`, effort `xhigh`, and execute mode were requested in the supplied prompt; effective model/effort are not exposed for independent verification. No global settings were changed.
- **Navigation order, reconciled:** quickstart, quickstart example code/test and copied Riot server; then generated Concord rustdoc index; author guide §5/§5.1–5.2/§5.4; caller guide §§2–3 and 5–7 and author guide §4.8; author guide §5.7, recipes NDJSON and native example codec; generated trial rustdoc page; Riot README, workflow test, example workflow code, fixtures README and relevant API/types/wire example sections. The initial `rg` also inspected section headings in the author, caller, recipes and diagnostics pages.
