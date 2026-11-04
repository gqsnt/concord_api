# V4.3 candidate — review findings and resolutions

Reviews actually performed, and what each found. "Independent" here means a separate Claude
reviewer started with the code and the contract, not with the coordinator's conclusions. No other
reviewer (a configured Codex/Sol profile or a human) was available in this environment; none is
claimed. Agreement between reviewers is not used as evidence.

| Review | Scope | Reviewer |
|---|---|---|
| R1 | coordinator's own checks while integrating | coordinator (Claude Opus 5.5) |
| R2 | the generated-input prototype (`0a91ce5..214980d`) and its caller tasks (DX05) | independent Claude subagent |
| R3 | the supported branch's examples and guides (`5ed600b..`, excluding records) | independent Claude subagent |

## R1 — found while building and integrating

| # | Finding | Disposition | Change | Verified by |
|---|---|---|---|---|
| R1-1 | Prototype: the navigation method of a `#[deprecated]` endpoint called the equally deprecated `_with`, so callers with `#![deny(warnings)]` failed (`crates/concord/tests/attributes.rs`). | fixed (experiment) | navigation builds the request directly (`6f204ae`) | `cargo test --workspace --locked` on the experiment |
| R1-2 | Prototype: a value type's missing `Display`/`ToHeaderValue` was reported twice more (once per generated `Debug`), changing four UI expectations. | fixed (experiment) | one shared private `Debug` helper, emitted after the request's impls; all 162 UI expectations byte-identical (`6f204ae`) | `cargo test --locked -p concord --test ui` |
| R1-3 | Integration: the three helper branches each added workspace members on the same line of `Cargo.toml`. | resolved | union of members (`9aa570f`) | `cargo metadata --locked` |

## R2 — generated-input prototype

Pending.

## R3 — supported examples and guides

Pending.
