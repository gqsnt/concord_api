# Findings and placement ledger

One row per shared decision or finding. Dispositions: IMPLEMENT, SDK_ONLY, DOCS_ONLY,
DEFER_WITH_REASON, REJECT_WITH_REASON. Source review findings F1–F7 are in the handoff
(`reference/Developer_Interface_Review.md`).

## Phase A

### D-A1 — response boundary: owned `Received<T>` (IMPLEMENT, core)

- **Reproduction:** F1 — GitHub `send_edited`/`send_conditional`, Stripe `send` (fabricated
  `status: 0` in an impossible branch), Salesforce `send_with_usage`, Shopify `ShopContext::send`
  each re-implement `into_parts` → `execute` → `Decoder::finish(Err(e))`/`decode`. F2 — the
  Kubernetes watch and Salesforce result readers need identity passed as strings (`open_for`).
- **Compared** (`v4/sdk20/a1-boundary/`, compiled against the candidate's public API): the owned
  state, and the smallest head-guard alternative `send_observed(prepared, |head| -> Result<M, R>)`
  built over `into_parts`. The guard covers metadata pairing and a refusal of a 2xx head, but
  (a) every caller handles a `Refused` arm even for an infallible observer, (b) a provider document
  for a non-2xx goes through a second error path, and (c) it decodes with the finite decoder, so
  the incremental path needs a second helper, keeps the bodyless declaration (whose `send()`
  drains) and still names the reader with a duplicated string, because the guard never exposes the
  identity. It does not satisfy A1's identity-transfer clause; the owned state does with one public
  type and three steps (`receive`, `head`, consume once).
- **Selected:** `Prepared::receive() -> Received<T>`; `send()` is `receive()` + `decode()` (one
  code path). Natively edited requests: `Decoder::execute`; synchronous `Decoder::transport_error`.
  Identity: public `Operation` (declared name + method, or a static label); extras sees only
  `Operation::name()`, never `EndpointMeta`.
- **Ownership:** each consumer takes `self` (compile-fail doctest in `response.rs`); dropping
  releases the response; no implicit drain or retry; `Debug` prints operation, status, version and
  header names only.
- **Metadata:** SDK-owned values parsed from `head()` before the consumer; Concord copies no header
  (no header snapshot on the ordinary path).
- **Cost:** allocation counts unchanged on all 94 rows of `concord-bench --bin alloc-count`
  (base `a95fb2fb` vs `d83b4782`).
- **Compatibility:** additive; `send`, `send_raw`, `into_parts`, `Decoder::{new, drain, decode,
  finish}` unchanged in signature and behaviour. `Error`'s internal identity became `Operation`
  (Display unchanged for declared endpoints).

### D-A2 — runtime media: defaulted `Encode::encode_body` + `EncodedBody` (IMPLEMENT, core + extras)

- **Reproduction:** F3 — `checks/dx03-csv-runtime` declared the upload bodyless and applied
  `Upload` to the native request; `examples/multipart` likewise for a per-body boundary.
- **Competing option:** a separate `EncodeRuntime<T>` trait. Rejected: the macro would have to pick
  a trait per body (it knows no format), and a blanket bridge between the two traits conflicts with
  custom formats that implement both.
- **Selected:** a defaulted hook on `Encode` (existing implementations unchanged; static
  `CONTENT_TYPE` still compile-time validated), and a pass-through `Encoded` format in extras for
  `EncodedBody` / `Upload`. Precedence table in `EncodedBody` docs and `v4/SEMANTICS.md` §7:
  strict byte equality, contradictions are `Build` errors before I/O, `remove content-type` keeps
  the runtime value, absent optional bodies send none.
- **Second consumer:** a generated-boundary `multipart/form-data` codec
  (`checks/phase-a/tests/bodies.rs`, known length → `Content-Length`).
- **Found while implementing:** `IgnoreContentType<F>` forwarded only `encode`, which would have
  dropped `F`'s runtime media; it now forwards `encode_body`. Custom wrapper formats in SDKs must
  do the same (checked during migration).

### D-A3 — truthful incremental outputs: `-> stream T` / `-> stream` (IMPLEMENT, core + macros + extras)

- **Reproduction:** F2 — Kubernetes watch and Salesforce result endpoints were declared without
  output; their generated `send()` drained the body. F4 — readers had no standard `Stream`.
- **Runtime path first:** `Incoming` + readers opening `Openable` (`Incoming`, `Received<T>`,
  `reqwest::Response`) with automatic identity; `RecordStream` (one boxed future per record read,
  reader moved in and out, `Unpin`, cancellation-safe, no read-ahead); `completion()`.
- **Declaration opt-in:** `stream` keyword after `->` (not when it begins a path or a generic
  type). `-> stream T` needs `F: Incremental<T>` (new core trait, the streaming counterpart of
  `Decode`, not a `Decode<Stream<..>>`); `send()` returns `Records<F, T>`. `-> stream` returns the
  unread `Incoming` for runtime-configured readers. No generated method was added (no new
  collision surface); `send`/`prepare` keep their names.
- **Known tradeoff:** a finite format used for `-> stream T` is reported once per signature that
  names the reader type (six identical-message E0277 errors in `ui_json/stream_outputs.stderr`)
  where a finite mismatch is reported once.

### D-A4 — ordinary pagination facade (IMPLEMENT, concord-pagination)

- **Reproduction:** F5/F6 — Stripe returns a factory because `Sequential` cannot be named;
  `Sequential::next` returns `None` on exhaustion and on a budget stop alike; dx03 stage 2 clones
  `discovery.resume` and `mem::take`s errors before `into_rerun`.
- **Selected:** `Pages`/`PageItems`/`ItemStream` (engine erased behind a boxed trait object, no
  per-page allocation beyond the engine's own; `Send + 'static` factories), typed `PagesError`
  (`Provider`, `Bounded`, `Ended`), `Incomplete` for `collect_complete`; lossless `into_resume` on
  `JobsReport` and `IndependentReport`. No hidden cap; exhaustion is checked before a cap reached on
  the last page. Engine semantics unchanged; `into_rerun` documented as lossy.

## Gate A independent consumer (`checks/consumers/phase-a-dx`, Codex `gpt-6.1-terra` xhigh)

Unassisted first use from public docs and rustdoc; no implementation source read (its
`REPORT.md`). All four outcomes completed (9 tests). Findings and dispositions:

| ID | Observation | Attribution | Disposition |
|---|---|---|---|
| FA-1 | RECIPES' metadata recipe led with `into_parts`; rustdoc of `receive` showed the shorter path | docs | DOCS_ONLY, `8bcd1015`: recipe starts from `receive()`; native recipe uses `transport_error` |
| FA-2 | `csv`/`jsonl` do not enable `json`; the consumer wrote its own JSON decoder | docs (feature selection was the brief's) | DOCS_ONLY, `8bcd1015`: feature list says so |
| FA-3 | moving a `RecordStream` into an upload hid its completion and terminal error; consumer wrote a shared-state `unfold` adapter and stringified the error | library (A3 composition) | IMPLEMENT, `8bcd1015`: `RecordStream::watch() -> StreamWatch` (completion, position, failure kind; the `ReadError` itself is the upload error's source); test `streams.rs::a_watch_reports_the_download_outcome_after_the_upload` |
| FA-4 | `CsvWriter` docs still said to apply `Upload` to a native request | docs | DOCS_ONLY, `8bcd1015` |
| FA-5 | the page projection closure and resume wrapper are written per SDK; `Incomplete` easier to find in the pagination README than in the guides | provider adapter (expected) + docs | SDK examples (Stripe/GitHub facades) and CALLER_GUIDE §6a link; re-check after migration |
| FA-6 | reading a provider's JSON error document from a non-2xx needed the native response | library candidate | DEFER to the SDK migration evidence: bounded error-document reads already repeat in Stripe/Kubernetes/Salesforce; decide with those cases |

### D-A5 — repeated parameter placement (F7): DEFER_WITH_REASON (measured)

Kubernetes repeats its list selector block in 19 places (13 list + 6 watch endpoints across 4
groups × cluster/namespaced scopes); adding one list parameter today is ~13 identical edits.
Scope values do not help: they would also reach the single-object endpoints of the same scope.
Stripe's account override and Salesforce's per-type query declarations show the same pattern in
smaller form. A parameter-group construct needs collision, docs, override and cfg semantics; the
contract allows it only as a targeted experiment. Decision: collect the Phase B cases (GitLab,
Microsoft Graph, Notion, Airtable list endpoints) before designing anything; revisit between
waves with at least two new distinct providers showing the same repetition.

## Phase A migration findings (existing five SDKs)

| ID | Source | Observation | Disposition |
|---|---|---|---|
| MG-1 | GitHub migration (`meta.rs`, `download.rs`) | relative `Link`/`Location` resolve against the final response URL, which `Received` did not expose; the SDK took `into_parts()` to read `response.url()` and re-wrapped with `Decoder::receive` | IMPLEMENT: `Received::url()` (unredacted, documented, not in `Debug`); GitHub `capture` now uses `head()` + `url()` |
| MG-2 | Shopify bulk result download | a labelled native response could reach a reader only via `Incoming::new`, which turns a non-2xx into a `concord::Error` instead of the reader's `OpenError` | IMPLEMENT: `Openable for (reqwest::Response, Operation)` |
| MG-3 | Shopify bulk result download | no public conversion of a native `reqwest::Error` to a labelled Concord error (URL removed) | IMPLEMENT: `Operation::transport_error` |
| MG-4 | GitHub `download::locate` | no way to build a `Status` error from an unexpected status without a branch that cannot succeed | DEFER: one call site; revisit if Phase B shows it again |

Test: `checks/phase-a/tests/received.rs::the_final_url_and_labelled_native_inputs`.
