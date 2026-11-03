**Recommendation: defer.** The test-only model demonstrates why variable-length paths need runtime redaction ranges, but it is not a shippable feature: it deliberately leaks metadata to satisfy today’s `'static` API and does not implement macro syntax or production ownership. The maintained native recipe is improved and tested.

The working tree contains the [test-only experiment](/home/canarit/projects/concord_v4_modular_work/snapshots/path-terra/crates/concord/tests/path_experiment.rs), the [compiled recipe](/home/canarit/projects/concord_v4_modular_work/snapshots/path-terra/examples/compat/src/lib.rs:119), its [loopback tests](/home/canarit/projects/concord_v4_modular_work/snapshots/path-terra/examples/compat/tests/resource_key.rs), and the [published recipe text](/home/canarit/projects/concord_v4_modular_work/snapshots/path-terra/docs/RECIPES.md). The experiment file is **untracked**, so `git diff` alone does not include it.

The experiment uses the explicit test-only spelling `Segments(&[&str])`; it adds no declaration syntax. I rejected an implicit string-splitting or `Display`-based interpretation because it would change ordinary `{value}` semantics. A possible `{path...}` declaration spelling was not implemented or reviewed.

For the table below, paths are the URL bytes following `http://127.0.0.1:12345`. Redacted paths are the URL portions observed in both normal and pretty `Prepared` Debug unless marked otherwise. The model has a public sequence, an ordinary sensitive value `ordinary-secret`, a sensitive sequence, and a literal trailing slash.

| Inputs | URL path bytes | Redacted Debug path |
|---|---|---|
| Ordinary `{value}` = `a/b` | `/base/root/items/a%2Fb` | `/base/root/items/a%2Fb` |
| Public `[]`, sensitive `[]` | `/base/root/items/ordinary-secret/tail/` | `/base/root/items/<redacted>/tail/` |
| Public `["public"]`, sensitive `["one-secret"]` | `/base/root/items/public/ordinary-secret/one-secret/tail/` | `/base/root/items/public/<redacted>/<redacted>/tail/` |
| Public `["a","b"]`, sensitive `["one-secret","two-secret"]` | `/base/root/items/a/b/ordinary-secret/one-secret/two-secret/tail/` | `/base/root/items/a/b/<redacted>/<redacted>/<redacted>/tail/` |
| Public `["a/b","%2F","%2e%2e","../x","\\","é","\n"]`, sensitive `[]` | `/base/root/items/a%2Fb/%252F/%252e%252e/..%2Fx/%5C/%C3%A9/%0A/ordinary-secret/tail/` | Not asserted for this case |
| An empty item, `.` or `..` | Build error; no URL | Error kind asserted; error output was not tested |

These results cover portions of P01–P05, **not the full adoption matrix**. In particular, the model does not test a public sequence after a sensitive ordinary value, sequence-related error output, local-wire behavior for the candidate, or generated syntax. Its empty sequence emits no segments; its empty items and dot segments fail. The separately maintained native recipe has a different policy: it rejects an empty sequence and control characters too.

I personally ran the normal experiment test successfully. With `CONCORD_PATH_STATIC_MUTANT=1`, the same redaction assertion failed: static one-element metadata incorrectly redacted part of the base path for the zero-element case. A separate control also confirmed that static indexing exposes `ordinary-secret` when both sequences emit two items. This is a mutation of the test model’s metadata, not a mutation of production code.

For P06’s ordinary-path control, I personally ran the existing allocation counter against an archived snapshot HEAD and the current tree. Its endpoint has one ordinary dynamic path segment: both runs measured **3 input allocations / 132 bytes** and **29 preparation allocations / 2,145 bytes** per call. Production path code is unchanged, so this does not predict the cost of a future implementation. Sequence construction was not benchmarked.

I personally ran `cargo test --workspace --locked --offline`, the focused experiment and recipe tests, `cargo fmt --all -- --check`, and warnings-denied Clippy for the changed test and example targets; they passed. The first workspace run caught an unchecked documentation excerpt, which I corrected before the passing rerun. The native URL library initially discarded a newline in a segment; the recipe now rejects control characters before mutation. No generated feature documentation exists because no feature was added; the maintained recipe documents its validation, native encoding, and redaction boundary.

The snapshot HEAD matches `3e320f11d96906b3e8e9f2c530ab546983c4ff75`; the supplied original SHA is absent from this checkout’s object database. I verified Rust and Cargo 1.99.1. The session does not expose a reliable effective model or reasoning-effort setting, so I cannot verify the requested model/effort. I did not perform the independent user review, verify the earlier mandatory integration gates, or claim feature acceptance.

The recipe text to publish is:

> Declare the final path position as an ordinary value. For a service that expects a key spanning several segments, pass an explicit list to an application helper, prepare with a placeholder, then use the native URL segment API to replace that final placeholder. Reject empty sequences, empty items, `.`/`..`, and control characters before editing. Each item is data: `a/b` becomes `a%2Fb`, and `%2F` becomes `%252F`. This example assumes no declared trailing slash; a runtime base path is retained. Native request Debug exposes the full URL, so do not log it when it contains secrets. Continue with `decoder.finish` to retain the endpoint’s response handling.

The complete, compiled helper and its exact loopback examples are linked above.