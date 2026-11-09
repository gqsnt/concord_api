D2-secret-wiretext: complete SDK overlays and callers; DESIGN.md records the contract and test impact.
Hand-counted net glue removed: shopify 33 (38 Debug lines minus 5 WireText lines), amazon-s3 12, nager-holidays 0; S3's additional 25 declaration lines are excluded.
New concepts: Secret<T>, public WireText; conditional standard/serde forwarding; N4 has no helper calls to remove in these SDKs.
C1: #[sensitive] and header placements state provider facts; no generation switches or invented Bearer scheme.
C2: secrecy and placement are typed facts; validation, SigV4 signing, clocks, refresh and traversal remain Rust.
C3: Nager's entire supplied src tree is byte-identical; no new concepts enter its declaration or caller.
C4: explicit #[sensitive] remains mandatory for Secret placements, checked through aliases, optional values and each items.
C5: missing sensitivity or wire support fails at the declared value; Secret has no Display; diagnostics must not expose dispatch internals.
C6: store a Secret, mark its placement, use expose deliberately; headers accept native bytes or WireText.
C7: expose/into_inner, the caller's reqwest client, prepare/into_parts/send_raw and native headers remain available.
C8: identical request bytes and omission rules; no implicit retries, middleware, global state or hidden actors.
C9: retained wire/canary/signature oracles plus supplied future runtime/UI tests; intentionally unrun because this lane must not compile.
Simple control changed: no; Nager's copied declarations/modules are identical, with one ordinary caller example added.
Existing test changes: Shopify wire helper's return type and canary's deliberate token read only; assertions retained; S3/Nager unchanged.
Does the declaration read better? Yes: Shopify shows a typed secret instead of String, and S3's 17 headers name their placements directly; savings remain modest.
