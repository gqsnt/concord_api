No real defects found in `14b9e1e0`. None of the earlier `with_output()` findings is reintroduced.

Claims found sound — **CONFIRMED by reading**:

- [emit.rs:1761](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1761): the locked syn visitor reaches type placeholders in qualified-self types, generic arguments, associated bindings, bounds, function types, parentheses and invisible type groups. Lifetimes and parameter names are untouched. Expressions—including type syntax inside expressions—are deliberately excluded. I found no missed placeholder or replaced non-type underscore within that contract.
- [emit.rs:1770](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1770): every macro invocation is opaque, regardless of delimiter or nesting. Its input tokens are cloned during printing. This removes the marker collision, changed parsing, parameter-name replacement and `&&` normalization defects. Brace-delimited macros now follow the documented exclusion.
- [w2_declared_send_result.rs:120](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/checks/core-concepts/tests/w2_declared_send_result.rs:120): the new forwarded `_` witness exercises an invisible type group. The retained forwarded-error and arithmetic-expression witnesses address the earlier forwarding scenarios; the relevant parsing and grouping machinery remains intact.
- [send_template_macro_placeholder.rs:21](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord/tests/ui/send_template_macro_placeholder.rs:21): the snapshot tests the stated behavior for this macro under every Concord feature combination. The endpoint drains to `()`, requiring no codec. Substitution would yield the matching `Result<(), concord::Error>`; preserving `_` produces the recorded E0121 failures.
- [AUTHOR_GUIDE.md:1726](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/docs/AUTHOR_GUIDE.md:1726): the added sentence accurately describes macro ownership. It does not claim every macro containing `_` must fail—a macro may consume or discard that token.

Areas not covered:

- No builds, tests, rustdoc generation or external services were used. Exact snapshot output across feature sets was not independently executed.
- Token-string assertions cannot establish hygiene, invisible-group identity or delimiter-span fidelity.
- Compiled fixtures do not exhaust the broader qualified-type/bound matrix, arbitrary forwarding combinations or unstable/verbatim type syntax.