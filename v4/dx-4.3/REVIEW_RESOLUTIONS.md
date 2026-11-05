# V4.3 candidate — review findings and resolutions

Reviews actually performed, and what each found. "Independent" means a reviewer that started
with the code and the contract, not with the coordinator's conclusions. Agreement between reviewers
is not used as evidence. R2 and R3 are Claude subagents (same provider as the coordinator); R4 is
the already-configured Codex CLI, a different provider. (An earlier revision of this file, in
`637c8d2`, said no Codex reviewer was available; that was wrong — it had not been checked yet.)

| Review | Scope | Reviewer |
|---|---|---|
| R1 | coordinator's own checks while integrating | coordinator (Claude Opus 5.5) |
| R2 | the generated-input prototype (`0a91ce5..214980d`) and its caller tasks (DX05) | independent Claude subagent |
| R3 | the supported branch's examples and guides (`5ed600b..`, excluding records) | independent Claude subagent |
| R4 | the whole supported branch at `637c8d2`, records and decisions included | Codex CLI 0.166.0, read-only sandbox; requested `-m gpt-6.1-sol -c model_reasoning_effort=xhigh`; the session header reported `model: gpt-6.1-sol`, `reasoning effort: xhigh`, `sandbox: read-only` (observed settings are what the CLI printed; nothing else was observable) |

## R1 — found while building and integrating

| # | Finding | Disposition | Change | Verified by |
|---|---|---|---|---|
| R1-1 | Prototype: the navigation method of a `#[deprecated]` endpoint called the equally deprecated `_with`, so callers with `#![deny(warnings)]` failed (`crates/concord/tests/attributes.rs`). | fixed (experiment) | navigation builds the request directly (`6f204ae`) | `cargo test --workspace --locked` on the experiment |
| R1-2 | Prototype: a value type's missing `Display`/`ToHeaderValue` was reported twice more (once per generated `Debug`), changing four UI expectations. | fixed (experiment) | one shared private `Debug` helper, emitted after the request's impls; all 162 UI expectations byte-identical (`6f204ae`) | `cargo test --locked -p concord --test ui` |
| R1-3 | Integration: the three helper branches each added workspace members on the same line of `Cargo.toml`. | resolved | union of members (`9aa570f`) | `cargo metadata --locked` |

## R2 — generated-input prototype (experiment branch)

| # | Finding (severity) | Disposition | Change / evidence |
|---|---|---|---|
| R2-1 | Not additive: generated `{Endpoint}Input`/`{endpoint}_with` names break valid V4.2 code (user types, imports, methods, sibling names) (high) | **confirmed** (coordinator reproduced `E0428`/`E0119`, `E0592` on 1.99.1); decisive for non-adoption | DECISIONS G1 outcome, reason 1 |
| R2-2 | Presence kind and exact type become public field types: new breaking edits (medium-high) | confirmed (evolution check v3, experiment branch); decisive | G1 outcome, reason 2 |
| R2-3a | Template `else inherit` documented as "not sent" (medium) | fixed | `9e4c0ef`; checked by expansion |
| R2-3b/c | "Declared by" not derived from the resolved composition; native constants unmatched (medium) | recorded as a limitation of the rejected prototype, not fixed | G1 outcome, reason 4 |
| R2-3d | `new` doc: initialized optional values start `Some` (low) | fixed | `9e4c0ef` |
| R2-3e | runs of spaces in the `_with` doc (low) | fixed | `9e4c0ef` |
| R2-4 | two collision checks can never fire (`Input` vs `Config`; `input*` built-ins) (info) | accepted as harmless in the prototype | — |
| R2-5 | caller footguns (`None` tags vs inherited; pushing onto `None`; stored initializer values repeat) (medium) | confirmed; part of the decision | G1 outcome, reason 3 |
| R2-6 | cost; cheaper `rebind` alternative (info) | recorded as possible later work, not implemented | G1 outcome |

After the fixes: `cargo test --locked -p concord --test inputs --test ui --test attributes` passes
on `9e4c0ef`; the full maintained gate on the experiment branch is in
`experiments/generated-inputs/evidence/`.

## R3 — supported examples and guides

No high-severity finding. Every package passed alone; clippy clean.

| # | Finding (severity) | Disposition | Change |
|---|---|---|---|
| R3-1 | `examples/sdk/src/context.rs` said the contexts' `Debug` never contains path data, yet shows the tenant (as the generated handle does) (low-medium) | fixed wording | `a42ef99` |
| R3-2 | `Meta::capture` duplicated values when `keep` names a header twice (low) | fixed: a repeated name is kept once | `a42ef99` |
| R3-3 | the extension's `compile_fail` doc test had no compiling control (rustdoc does not check error codes on stable) (low) | fixed: control doc test added | `a42ef99` |
| R3-4 | the empty-source upload test did not assert chunked framing (low) | fixed | `a42ef99` |
| R3-5 | the fixture's RFC 3339 oracle accepted impossible dates (low) | fixed: days-in-month check, century leap anchors | `a42ef99` |
| R3-6 | a test pinned reqwest's internal `is_decode()` classification (low) | fixed: `is_decode() || is_body()` | `a42ef99` |
| R3-7 | doc slips (traversal comment, RECIPES setter example, `Prepared` `Clone` wording) (low) | fixed | `a42ef99` |
| R3-8 | the feature-isolation step does not refuse reqwest `stream` for Concord (info) | not changed: true today (`cargo tree -i reqwest -p concord`, `--all-features`, `-p concord-core` show no `stream`); the maintained inventory is changed only where a new supported surface requires it, and none does | — |
| R3-9 | derived `Debug` of application types (`Interrupted`, `Traversal`, `Outcome`) prints records and cursors (info) | accepted: application data, matching the generated `Debug` (the cursor is not declared sensitive); the metadata recipe's stricter stance concerns response headers and URLs | — |
| R3-10 | example tests use `reqwest::Client::new()` (system proxy) against loopback (info) | accepted: a proxy environment makes them fail, not pass falsely; same as the existing examples | — |

After the fixes: each affected package passes alone (fixture 4, SDK 12, extension 17, native 47,
app 21 tests) and `cargo clippy --locked --workspace --all-targets -- -D warnings` is clean; the
final gate is in VERIFICATION.md.


## R4 — Codex review of the supported branch at `637c8d2`

No high- or medium-severity finding; five low ones, all confirmed by reading the code.

| # | Finding (low) | Disposition | Change |
|---|---|---|---|
| R4-1 | the fixture added a caller-supplied cursor offset to the page size unchecked (overflow panic in debug) | fixed: saturating end | this commit |
| R4-2 | the fixture's RFC 3339 oracle parsed signed numbers (`T-1:00:00Z` accepted) | fixed: digits only, with tests | this commit |
| R4-3 | `MetaError::meta()` promised `None` ⇔ "no head arrived", but `From<concord::Error>` gives `None` for any converted error | fixed wording: the stage guarantee holds for errors the recipe produces; a converted error has nothing captured | this commit |
| R4-4 | `VERIFICATION.md` said the requirement tests ran "on FINAL" while the final rows were still pending | fixed: rows name the exact commit each result ran on | this commit and the records-only successor |
| R4-5 | `docs/RECIPES.md` said rewriting a seed request's URL "only reaches suffixes"; `into_parts` gives an unrestricted native URL | fixed: the suffix limit is attributed to an append-only helper; the seed's real problem is its base, headers, `Accept`, decoder and name | this commit |

Codex also reported as checked and correct: path-encoding agreement of the SDK context with
Concord's, the metadata recipe's single execution and stage handling, wrapper formatting,
traversal positions, the conversions, the guides apart from R4-5, that G1/M1/X1 are supported by
their evidence, and that `crates/` and `scripts/verify.sh` are unchanged. It inspected but did not
run the tests. After the fixes: fixture 4, native 47, SDK 12, app 21, extension 17 tests pass,
each package alone; clippy clean.
