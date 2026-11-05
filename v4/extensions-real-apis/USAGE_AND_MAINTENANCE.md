# Usage and maintenance evidence (DX-01, DX-03)

Every exercise below was done by someone who did not write the code being maintained or used,
through public exports only, with executable before/after consumers kept in the tree. Full records
are next to the code; this index summarizes them. Commit SHAs are those of the published history
(see HISTORY_REWRITE.md for the Stripe/Shopify/Salesforce exercises).

## Staged SDK maintenance exercises (DATA_AND_VALIDATION §6)

Each `checks/maintenance-<provider>/` package holds the frozen initial consumer (`src/before.rs`,
`tests/before.rs`, byte-identical from stage 1 on), the updated consumer (`src/after.rs`,
`tests/after.rs`) and `MAINTENANCE.md`. Check: `cargo test --locked -p concord-check-maintenance-<provider>`.

| SDK | Staged change | Stages (frozen / SDK change / updated) | Old call sites | Main findings | What was easy |
|---|---|---|---|---|---|
| GitHub ([record](../../checks/maintenance-github/MAINTENANCE.md)) | `type` filter on issues list; `issues/list-events` added from an outside crate via `RepoContext::request` | `c21e130` / `7afc379` / `9c42c1a` | compiled unchanged; exhaustive struct literals/patterns of `IssueFilter` break (E0063/E0027) | filters repeated in three SDK places; ~80 extension lines duplicate SDK send/metadata/continuation logic; an outside `Decoder::new` has no endpoint label; ~25 lines declared in the SDK vs ~145 lines outside | keyword rename answered by AUTHOR_GUIDE at first lookup; rustdoc shows the wire name; context gave the outside operation exactly the declared headers |
| Stripe ([record](../../checks/maintenance-stripe/MAINTENANCE.md)) | `invoice_settings[default_payment_method]` + typed expansion `Expandable<PaymentMethod>` | `5a72590` / `fb519c7` / `9e9da9d` | compiled unchanged; byte-identical form body when the field is omitted; struct-literal callers break (E0063) | Stripe error `param` comes back in wire spelling with no mapping to Rust fields; expansion paths are unchecked text (the `data.` prefix must be known) | zero lines changed in the custom bracket-form encoder, `api!` declarations, expansion machinery, context and `send()` |
| Shopify ([record](../../checks/maintenance-shopify/MAINTENANCE.md)) | `onlineStoreUrl @include(if:)` with a tri-state `Included<T>`; metafields as a third bulk JSONL record kind | `d72e469` / `ee7145a` / `183959f` | compiled and passed unchanged (simpler designs broke: always-selected field fails every stored response at run time; a new enum variant is E0004) | a decode error withholds the missing field's name (only reachable through `source_unredacted`); bulk selection and decode type are linked only by docs | no declaration change; one turbofish switches the reader's record type; framing, limits and positions unchanged |
| Salesforce ([record](../../checks/maintenance-salesforce/MAINTENANCE.md)) | `HasOptedOutOfEmail` on the Contact export; export switched from COMMA/LF to TAB/CRLF | `6b55420` / `0190425` / `3433930` | compiled unchanged | row struct and column list coupled with no compile check (runtime decode error naming no field); consumers deriving SOQL from the list silently select the new column | dialect switch needed zero SDK lines (job → `SalesforceCsv::options()` → reader/writer); errors named header field 7 without the value |
| Kubernetes ([record](../../checks/maintenance-kubernetes/MAINTENANCE.md)) | `networking.k8s.io/v1` Ingress family (list/get/watch); set-based label selector | `b2c7d0b` / `f998bcf` / `af786e3` | compiled unchanged; no break | 13 copies of the list-parameter block, 6 of the watch block; a forgotten `list_operation!` registration compiles in the SDK and fails only in a consumer; `KubeContext` is closed to other crates | list/watch machinery reused unchanged; dotted group path needed nothing; selector change needed no SDK change |

## Independent caller tasks (DATA_AND_VALIDATION §12, DX-03)

Written by other providers' models from public docs only, before reading any author conclusion
(PROVENANCE.md in each package records requested and observed settings and the rounds).

| Task | Caller | Result | Caller's assessment (from its NOTES.md) |
|---|---|---|---|
| (b) static `Csv<HasHeader, Semicolon>` declaration + runtime dialect bridge (incremental read, metadata, typed errors, matching upload Content-Type) — [`checks/dx03-csv-runtime`](../../checks/dx03-csv-runtime/NOTES.md) | Codex `gpt-6.1-terra` xhigh | 2 unit + 6 loopback tests pass; no compiler errors | ownership boundary explicit but more steps than `.send()` and two error types (`OpenError` before reading, `ReadError` after); copied status/headers before opening; a late upload failure surfaces as a Concord transport error, not a typed `UploadError`; explicit writer columns validate order rather than reorder; "the CSV overview is much more directly useful than the general streaming recipe" |
| (a) Salesforce export: sequential collecting consumer (stage 1) → discovered bounded fanout + staged incremental sink + resume of unfinished parts (stage 2) — [`checks/dx03-fanout-sink`](../../checks/dx03-fanout-sink/NOTES.md) | Codex `gpt-6-astra` xhigh | stage 1: 7 tests; stage 2: 5 tests (stalled part holds its slot; mid-body failure publishes nothing; resume re-downloads only unfinished work); stage 1 unchanged | "the public API fits this requirement well; the whole-job future is the correct concurrency unit"; cost: several report categories and an application-level resume token; `finish()`/`into_rerun()` are not a whole export-resume operation (discovery cursor, receipts and budgets must be merged by the caller); stage 2 is 486 lines vs 154 for stage 1, mostly application state |

## Limits

- The maintenance authors and SDK authors are the same model family (separate sessions); the two
  DX-03 callers are a different provider.
- All exercises are offline (loopback fixtures derived from provider documentation); no provider
  actually changed.
- Line counts and timings are supporting observations; the compiled consumers, diffs and quoted
  diagnostics are the evidence.
