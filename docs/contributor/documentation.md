# Documentation ownership

The manual follows the product transformation:

1. represent a remote contract;
2. understand generated Rust;
3. construct and execute requests;
4. add lifecycle capabilities;
5. consult exact reference.

Guides teach a path and must link to an importable module under
`crates/concord-examples/src` plus a proving test under
`crates/concord-examples/tests`. Recipes compose those capabilities for a
concrete use case. Concepts explain why. Reference pages own exact syntax,
features, limits, signatures, error categories, compatibility, dangerous
capabilities, and unsupported forms.

Do not copy a complete example into docs. Keep declarations canonical in the
example crate and use small illustrative fragments only. Do not expose the
private generated bridge in user-facing pages.

Feature and limit tables are checked:

```text
cargo run --locked -p concord-xtask -- docs --write --locked # regenerate
cargo run --locked -p concord-xtask -- docs --locked # verify
```

The task reads feature metadata with Cargo's typed metadata format, reads
normative limits by linking core and extras, parses Markdown with a Markdown
parser, and checks local links and anchors. External URLs are counted but not
fetched, so a network URL check must be reported separately. Compiler
diagnostics, command ownership, workflow structure, and user vocabulary belong
to their compiler, Justfile, workflow, and documentation owners; the
documentation task does not parse those source formats.

`just docs` additionally runs all-feature workspace doctests and
warning-denied Rustdoc for core, extras, and façade no-default surfaces; the
selected façade `csv`, `json`, and `dangerous-raw-response` surfaces; and the
all-feature product surface. This is intentionally a maintained feature sample
rather than a combinatorial matrix. Recipe bodies remain portable command
lines without shell-specific parsing.

When a fact changes, update its canonical owner:

| Fact | Owner |
|---|---|
| declaration grammar | `reference/dsl.md` |
| generated shape | `reference/generated-api.md` |
| features | façade manifest + generated feature table |
| numeric defaults and ceilings | linked core/extras constants + generated limits table |
| errors | `reference/errors.md` and Rustdoc |
| package graph | `contributor/architecture.md` |
| commands/CI | `justfile`, xtask, `contributor/testing.md` |
