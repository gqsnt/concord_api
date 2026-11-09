# A1-error-document — A errors — Declare the provider error document and decode bounded failures centrally
Branch / commit: spike/A-error-document at 55b8d5cc (base ba10189b)
Targets (3 files): witness `examples/distribution-registry-sdk/src/error.rs`; counterexample `examples/amazon-s3-sdk/src/error.rs`; simple control `examples/nager-holidays-sdk/src/lib.rs` (byte-identical).
C1: yes — `error D in F;` states document and codec facts; bounds stay Rust.
C2: no — shared reading disappears, but conversions/generic propagation remain and Salesforce repeats the declared pairing for raw streams.
C3: yes — simple controls retain declarations and behavior.
C4: yes — explicit codec and nearest-owner inheritance are deterministic.
C5: no — whole-declaration spans, invalid generic advice and an internal codec path remain.
C6: yes — failed statuses carry a declared document; generic signatures remain a visible cost.
C7: yes — native/raw/unread exits survive; adopted decoded failures retain head and bounded bytes.
C8: yes — one bounded read; no new retry or actor; provider status remains authoritative.
C9: no — runtime coverage improved, but generated error-type documentation remains inaccurate and unguarded.
What disappears: total classifier glue −51 (error −38, result −7, forwarding −6); net handwritten source −15 over the measured cohort. Registry gains −44 source lines; S3 +3, Salesforce +16, ClickHouse +10 and Anthropic −2. GitHub never reads an error body and is not migrated.

Measurements: M1–M11, baseline → spike. Saved aggregate includes Registry, S3, Salesforce, ClickHouse, Anthropic, unchanged GitHub and simple controls; per-SDK results: [measurement](/home/canarit/projects/Concord_Next/work/wt/A1-error-document-measure.md); evidence: [report](/home/canarit/projects/Concord_Next/work/design/A1-error-document/REPORT.md).

| Measure | Baseline → spike |
|---|---|
| M1 glue | 6,248 → 6,197 (−51); error 3,286 → 3,248; result 1,164 → 1,157; forwarding 205 → 199. Reviewer excluding unchanged GitHub: 4,993 → 4,942. |
| M2 model / behavior | Model 3,556 → 3,558; behavior 7,051 → 7,077. |
| M3 concepts | Adds 1 statement, 2 public types (Failure, Document), defaulted D on 7 public generics and 13 public methods; hidden ErrorFormat/widen. Glue lines per concept: not measured. |
| M4 declaration | DSL lines/endpoint 6.41 → 6.49; declaration lines 474 → 480; tokens/endpoint not measured. |
| M5 negations | 0 → 0. |
| M6 controls | Nager 8 → 8; PokeAPI 7 → 7; Frankfurter 11 → 11 DSL lines; no added control concepts. |
| M7 caller docs | Final wording adds 3 words per adopter send(), exceeding the default “not higher” budget; supersedes the original +25 words. |
| M8 performance | Compile time, expanded lines and allocations: not measured. |
| M9 caller changes | Registry 5 test sites; S3 1 helper + bound; Salesforce 2 helpers (apps 0); Anthropic 8 test sites + README row; explicit compatibility inventory below. |
| M10 tests | Baseline not measured → final 926 passed, 0 failed, 4 ignored; no-default tests 302 passed; error-document tests 12/12 across 8 runs; touched canaries/clippy pass. SDK expectations migrate states/accessors; 3 policy UI label/note snapshots change. Full workspace gate not reported. |
| M11 diagnostics | Baseline not measured → parser errors precise and Error<D> conversion fails at authored ?; codec mismatch names `__private::ErrorFormat`; policy mismatch includes whole-declaration spans and incomplete/invalid D advice. |

Counterexample handling: S3 uses IgnoreContentType with its XML adapter; mixed/mislabeled formats and HEAD remain supported, with status/304 logic in Rust. Registry challenge behavior stays Rust. Anthropic SSE-opening failures use the declared document. Salesforce raw-stream reads still duplicate codec/document facts; Openable accepts only Received<T> with the default document type.
Breaking changes: adopter endpoints/policies require Endpoint<T,D>, SendWith<T,D> and SendOperation<T,O,D>; Prepared/Received/Decoder/Error carry defaulted D. Consumed failures make into_response() return Err and response()/meta() return None; use provider() for head/bytes. Registry fields become failure and variants become Read/Unrecognized(CodecError)/TooLong/Broken; S3 exposes document/codec types and changes prepare/parts/decoder signatures; Salesforce exposes ApiErrorBody and changes from_concord/QueryRecord signatures; ClickHouse adds a conversion; Anthropic document_error() becomes failure(), document type becomes public, unreadable-state/Debug/Display text changes. Owner-statement `error` becomes a keyword.

Review:
- Round 1: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-A1-error-document-review-1.md).
- Finding, blocking: the new JSON-dependent test did not respect feature-isolated builds.
- Finding, non-blocking: policy spans/advice and codec internals fail C5.
- Finding, non-blocking: generated/core docs promise unread responses and exceed the word budget.
- Finding, non-blocking: caller-visible compatibility changes were incompletely listed.
- Finding, non-blocking: prefix retention, decoder non-invocation, response context and bound propagation lacked discriminating tests.
- Fixes: require JSON for the test target; document-aware diagnostic labels; alternative consumed-response wording/core docs; full compatibility inventory; instrumented codec and prefix/context/completion-path tests.
- Round 2: [ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/spike-A1-error-document-review-2.md); no runtime regression found by inspection; C2/C5/C9 remain no.
- Recorded limits: D advice still invalid/incomplete, spans/internal path remain; policy summaries/send error sections still name Error<()> for Error<D>, without emitter regression coverage; +3 words remains over budget. SDK provider-controlled diagnostics retain pre-existing exposure; reviewers did not run builds/tests/rustdoc; M8 not measured.

Agent's assessment:
- Shared bounded reading and document states are sound, with request bytes/status authority and native exits preserved.
- Standalone savings are narrow: Registry wins; conversions and D propagation erase savings elsewhere, so the broad extrapolation does not hold.
- Combining conversion removal with this mechanism may help; alone it improves clarity while retaining duplication, diagnostics and documentation limits.
