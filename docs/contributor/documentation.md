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
cargo run --locked -p concord-xtask -- docs --write  # regenerate
cargo run --locked -p concord-xtask -- docs          # verify
```

The same task validates local Markdown files and Markdown anchors, including
historical evidence; required example/test links; parsed recipe command
ownership; immutable workflow action revisions and required CI jobs; and
retired/implementation vocabulary in user docs. External URLs are counted but
not fetched, so a network URL check must be reported separately. `just docs`
additionally runs workspace doctests and warning-denied Rustdoc.

Historical phase evidence lives under `docs/contributor/evidence`. It records
commands and architecture decisions but is not normative user documentation.
Its local links and anchors are checked, while its historical vocabulary and
observed command text remain faithful to the state recorded in each phase.

`just` may select PowerShell as its native Windows shell. Recipe bodies must
therefore remain portable command lines rather than shell-specific scripts.

When a fact changes, update its canonical owner:

| Fact | Owner |
|---|---|
| declaration grammar | `reference/dsl.md` |
| generated shape | `reference/generated-api.md` |
| features | façade manifest + generated feature table |
| numeric defaults | core defaults source + generated limits table |
| errors | `reference/errors.md` and Rustdoc |
| package graph | `contributor/architecture.md` |
| commands/CI | `justfile`, xtask, `contributor/testing.md` |
