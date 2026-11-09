# E2-idempotency-key
Glue removed (hand-counted nonblank net): PayPal 20 lines; Adyen Checkout 0; Frankfurter 0; block arithmetic in DESIGN.md.
New concepts: `idempotency value;`, `Operation::idempotent()`, `Prepared::idempotency_key()`; no snapshot helper or E1 dependency.
C1: the statement names the provider's deduplication input, using E2's already-placed-value variant.
C2: facts move to endpoints; key validation, replay, retention, auth and uncertainty stay in Rust; PayPal's read/write match remains.
C3: Frankfurter's original source, manifest and README are byte-identical; its new caller uses only existing concepts.
C4: the named header role is explicit; no familiar header-name recognition or automatic method idempotency.
C5: proposed UI fixtures anchor invalid placement, read and duplicate-role diagnostics at the declaration.
C6: one fact per supported endpoint and one sentence of generated documentation.
C7: caller reqwest client, prepare(), into_parts() and send_raw() retain their existing meaning.
C8: no minted key or new runtime actor; request bytes and cloning are unchanged.
C9: proposed wire/carrier/sensitive-key tests and final-header identity regression accompany unchanged existing suites; none compiled or run.
Simple control changed: no original file changed; only an ordinary caller example was added.
Existing tests requiring changes: none; Adyen's strict single-header reader and complete native-request identity remain unchanged.
Does the declaration read better than before? Yes, narrowly: each keyed operation now states deduplication explicitly; Adyen gains clarity, not less policy.
