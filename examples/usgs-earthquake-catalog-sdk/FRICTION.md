# SDK23 friction and validation ledger

Baseline `56a16ee34f8aa714bd51dca67f744e5870e813b6`; 2026-11-07.
**Example-assisted. Live provider calls: NOT_RUN.** No shared source changed.
The observable costs below distinguish provider decisions from reusable mechanics.
Proposed changes are recorded only; the frozen Concord baseline was preserved.

| ID | Observable evidence | Handwritten cost and ownership | Smallest change that would remove/reduce it |
|---|---|---|---|
| F1 — same query, two formats | `src/lib.rs`: `window` owns bounds/magnitude/limit/order/type; sibling `geojson` and `csv` endpoints differ in format/output. `exact_wire_shapes_share_filters_and_keep_event_lookup_separate` and the export workflow compare query pairs. | Two short endpoint blocks, with one extra `response CatalogCsv`. No runtime format union or duplicated filters. Provider formats/types belong in SDK; scope inheritance already handles generic sharing. | None required. Retain separate typed endpoints: a runtime format value would also require a sum of a finite GeoJSON value and an incremental reader, moving complexity to callers. |
| F2 — related input validation | Generated scope setters can independently change start/end. `input_guards_and_scope_setters_fail_before_io` makes an otherwise valid scope backward and proves both generated sends refuse without a request. | `Window` constructor plus `query(&Window)` mapping; policy reads the prepared query to revalidate the interval. Required dates cannot be one opaque input while independently projecting two query keys through current declaration syntax. Interval and 31-day restriction are SDK semantics; locating inputs before I/O is generic ceremony. | A narrow owner validation hook before `prepare()` returns, with borrowed owner values, would avoid parsing the built URL and would protect prepared execution too. No broad expression language is proposed. |
| F3 — CSV required columns on an empty export | Review found `id,place\n` was a clean zero-row result through the shared typed reader. The new `header_only_csv_requires_catalog_columns_but_allows_reordering_and_additions` regression rejects it while accepting reordered/extended headers. | `src/csv.rs` forwards `Incremental` head/context/open to shared CSV and wraps its reader to check required header presence even at EOF. It converts shared late errors into SDK `Error`. USGS's column list belongs here; presence checking, forwarding and terminal-state plumbing are generic. No parser was copied. | Add `CsvProfileOptions::required_columns(...)` with set-inclusion semantics, distinct from existing `columns(...)` (exact ordered equality). This would remove the wrapper and preserve the shared reader's standard stream adapter. |
| F4 — incremental rows across dependent pages | `Page<T,K>` owns `Vec<T>`; using `Pages<CsvRow,...>` would collect a complete CSV page. The delayed-suffix workflow receives row A while the server still withholds the remainder; budget/second-page failures preserve previously yielded rows. | `src/export.rs` owns one opening future/reader, budgets, counters, failure/completion and next-page transitions. Sequential offsets and short-page termination are provider semantics; bounded traversal of page readers is generic. | Add an external `concord-pagination` adapter for a factory returning an incremental reader plus a continuation computed at clean EOF. It must preserve partial rows, cancellation, budgets and late failure without buffering an entire page. Keep offset interpretation in the SDK. |
| F5 — heterogeneous no-data/status outcomes | Direct GeoJSON/CSV/event calls all classify 204/404; `pages` and `export` normalize them to exhaustion. Only 200 enters a decoder; tests cover all documented status classes and unknown statuses. | One common `SendWith<T>` policy plus two tiny no-data projections. `nodata=404` avoids inventing a generic empty value for unrelated finite/stream outputs. Status meaning is provider/SDK policy. | None required in core: retain typed SDK projection. A generic policy cannot manufacture arbitrary `T` without additional semantics. |
| F6 — redacted provider error text | FDSN documents echoing request URLs. `every_http_class_is_preserved_without_retries_or_secret_output` injects body/header/ID sentinels; malformed/oversized/interrupted error documents retain status. | `HttpFailure` stores `Result<String, concord::Error>` and writes Debug manually; Response Debug also withholds values. `document` and `decode_limited_with_meta` already remove raw collection/metadata mechanics. Status classes and text-access policy belong in SDK. | A small reusable diagnostic-opaque value wrapper could remove the manual formatting, if other SDKs need it. No new status DSL is justified here. |
| F7 — two independent limit paths | `decode_limited_with_meta` explicitly ignores streaming outputs; CSV tests exercise record/header/total limits while JSON tests exercise finite limits. | Finite/error limits in `Policy`; stream limits in `CatalogCsvProfile`. These are different lifecycles, not one universal byte cap. SDK selects numbers; shared codecs enforce them. | Keep the distinction. A focused send-policy example showing finite and incremental limits together would reduce author discovery cost without new core types. |
| F8 — page budget versus wire attempts | reqwest rustdoc documents native retry behavior. Tests supply `retry(never())`, `redirect(none())`, a timeout and loopback base; hang-up/status failures result in one request. | Explicit caller transport configuration and documentation. SDK sends once; no retry loop or global admission policy is introduced for public data. `concord-retry::http::retry_after` already supplies header parsing. | No core change: accepting arbitrary reqwest clients means logical page budgets cannot promise wire-attempt budgets. Document that boundary; an optional SDK transport constructor could package the recipe. |
| F9 — mutable offsets | `exact_multiple_needs_a_final_empty_request_and_updates_can_repeat_events` returns B twice after a mutation; continuation advances 1→3. A full page at the budget is an error, not completion. | Checked offset arithmetic, full/short-page interpretation, explicit incomplete states and snapshot caveats. Offset meaning and lack of snapshot consistency are provider semantics. | No Concord change can supply a provider snapshot. Applications own reconciliation; do not silently deduplicate or claim gap-free harvests. |

No native HTTP escape was necessary. Ordinary generated send remains the terminal
operation for all three endpoints; custom CSV representation is a normal
`Incremental` implementation using shared framing/limits. `concord-pagination`
handles the finite GeoJSON traversal. `concord-retry` is used for Retry-After
projection only. No auth flow, automatic backoff or rate limiter is warranted by
this bounded public-data slice.

Examples consulted (full per-file/access/adoption table in `PROVENANCE.md`):

- Direct: `examples/testserver/src/lib.rs` and its `Cargo.toml`.
- Embedded excerpts only: `examples/quickstart/src/{lib,main}.rs`,
  `examples/shapes/tests/wire.rs`, `examples/frankfurter-sdk/src/lib.rs`,
  `examples/openai-sdk/src/lib.rs`, `examples/google-drive-sdk/src/upload.rs`,
  `examples/native/src/native_edit.rs`, `examples/compat/src/lib.rs`.
- Adopted declaration/local-base and bounded policy patterns from quickstart and
  Frankfurter snippets. Multipart, subdomain and native-edit snippets were read
  with the required guides but supplied no implementation here. No existing SDK
  file was directly opened. This is not an unassisted or fresh-user measurement.

Observable diagnostics and corrections:

1. Initial offline unlocked `cargo test ... --no-run` compiled successfully in
   10.19 s; Cargo.lock added only this new package.
2. First full locked test run passed 12 protocol tests, 9 workflow tests and one
   compile-only doctest. Two missing crate-level documentation warnings on test
   files were corrected with module documentation.
3. `cargo fmt --all --check` reported formatting diffs in the new package (exit 1,
   read-only). `cargo fmt -p concord-sdk-usgs-earthquake-catalog` corrected only
   owned source. No shared formatter changes were made.
4. First clippy run passed after formatting/docs fixes. Enabled reqwest defaults
   for the public client's TLS capability; all workflow calls still used HTTP
   loopback. Existing lock entries sufficed.
5. Independent source review identified F3 and an ambiguous UTF-8 test that also
   had the wrong row width. Added a required-header wrapper and two fixtures;
   mutated a complete third row's place byte and asserted its UTF-8 diagnostic.
   The next locked run passed 13 protocol tests, 9 workflow tests and one doctest.
6. Focused follow-up review inspected the CSV wrapper and regressions and found
   both findings addressed. Final gates and the scope/lock audit passed below.

Resource ledger (observations, not a claim of exact peak RSS):

- Before the first build: `df -h . /tmp`, `free -h`, `nproc`, `du -sh target`;
  worktree filesystem `/dev/nvme0n1p2`, 368 GiB free of 915 GiB; 30 GiB RAM,
  23 GiB available, 4.9 GiB swap in use, 20 logical CPUs. Target did not exist.
- The offline no-run canary created 452 MiB in this worktree's `target/`; filesystem
  free space was 364 GiB. Subsequent TLS/check/test builds used the same target.
- Before final documentation gate: target 1.4 GiB, 359 GiB free, 23 GiB memory
  available; swap use 4.6 GiB. No pressure or cleanup was observed/required. Other
  workspace activity means whole-filesystem deltas are not attributed to this SDK.
- Build budget for this worker: one cargo process, four build jobs, default local
  target, 3 GiB expected output with more than 20 GiB filesystem reserve. Rustc
  `1.99.1 (58cf8f9c3 2026-10-13)`. Target is disposable build output; source,
  fixtures and these evidence files are preserved. No shared cache was deleted.


Final validation (all exit 0, 2026-11-07, one cargo process at a time):

| Command | Result |
|---|---|
| `CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-usgs-earthquake-catalog` | 13 protocol + 9 workflow tests passed; final README compile-only doctest passed; 0 ignored |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-usgs-earthquake-catalog --all-targets -- -D warnings` | Passed, no warnings |
| `CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-usgs-earthquake-catalog --no-deps` | Passed; generated `target/doc/concord_sdk_usgs_earthquake_catalog/index.html` |
| `CARGO_BUILD_JOBS=4 cargo fmt -p concord-sdk-usgs-earthquake-catalog --check` | Passed |
| `git diff --check` | Passed |
| Python TOML comparison against `git show HEAD:Cargo.lock` and root manifest | Every existing lock entry unchanged; only SDK entry/member added |
| Git tracked/untracked path audit | Only root Cargo.toml/Cargo.lock and the owned SDK directory changed; no commits |

Final package inventory: 7 source files, 3 test/support files, 18 fixtures,
README/PROVENANCE/FRICTION and Cargo.toml (32 files). `wc -l` totals: **src 921,
tests 1,033, fixtures 256**. Fixture logical-line count is 262 because six fixtures
intentionally lack final newlines (including truncated JSON). Counts exclude
expanded macro output, generated target artifacts and documentation. The final
source changes preserve all shared crates/checks/scripts/other examples.
