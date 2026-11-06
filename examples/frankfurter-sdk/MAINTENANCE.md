# SDK05 maintenance: unknown response attribution

Date: 2026-11-06. Ticket: **Add unknown field or optional filter**, preserving latest
and historical rate callers. Concord remains frozen at `e04236ca`; starting worktree
HEAD was `3aefaf5c` (author and independent consumer commits already present).
Requested runner: Codex `gpt-6.1-sol`, `xhigh`. Actual model/reasoning settings:
**UNVERIFIABLE** from available runtime metadata; the request is not execution evidence.

## Scope and first attempt

The baseline already declared the optional `providers` query filter on both operations
and ignored unknown response fields through `Rate`'s Serde derive. The official
[OpenAPI 2.1.1](https://api.frankfurter.dev/v2/openapi.json), rechecked on 2026-11-06,
lists only `date` and `providers` as query parameters for `/rate/{base}/{quote}`.
The shared `Rate` schema also describes optional nested `providers` attribution, while
the pair operation does not list `expand`. The selected perturbation therefore adds
that response field to a synthetic wire fixture; no new query parameter is invented.

The fixture's top-level rate/date differ from its nested observations. Existing callers
still receive the top-level `Rate`, including through generated `.send()`; attribution
is discarded. No production behavior change was needed because the existing decoder
already supports this extension. In particular, adding a public field to `Rate` was
unnecessary: the new test constructs the original four-field struct exhaustively.
This is a compatibility regression change, not a newly exposed attribution API.

`FRICTION.md` and `FIRST_USE.md` were not read, including before the completed first
attempt. No author coaching or other provider SDK supplied this solution. The existing
Frankfurter tests and their shared loopback helpers supplied the local test conventions.

## Edits by kind

| Kind | Edits |
| --- | --- |
| Declaration | None. Both operation signatures, optional filters and generated names remain unchanged. |
| Adapter | None. The existing public `Endpoint<Rate>` / `receive` / `decode` path handles the new fixture. Typed provider errors and error-body limits are unchanged. |
| Model | `src/model.rs`: rustdoc names the ignored nested attribution case, its independence from the query filter, and continued validation. No field, decoder or derive changes. |
| Tests | `tests/workflows.rs`: one test checks both workflows, filtered/unfiltered latest calls, `.fetch()`, `.send()`, request cloning, exhaustive `Rate` construction and all five exact HTTP targets/headers/empty bodies. `tests/protocol.rs`: one test rejects a quoted known rate and invalid JSON in the unknown field while retaining status and operation identity. |
| Fixture | `fixtures/provider-attribution.json`: hand-authored nested attribution with invented values and a long decimal; top-level values follow the documented latest example. Existing `.gitattributes` already covers it with `fixtures/** -text`. |
| Docs | `README.md`: caller contract and scope of ignored attribution. `PROVENANCE.md`: official schema source, synthetic scenario and invalid transformations. This file records maintenance evidence. |

## Locality and independent descriptions

**Zero independently maintained production descriptions required a semantic edit.**
One existing model policy, Serde's unknown-field handling on `Rate`, serves both
operations and both execution entry points. Neither endpoint declaration, either
`.fetch()` method, nor the shared adapter needed a corresponding change.

Five manually maintained sites now describe or exercise this concrete scenario:
the fixture, the positive workflow test, the negative protocol test, model rustdoc,
and the README paragraph. Provenance and this maintenance record are two additional
evidence documents. Those updates duplicate test/documentation facts, not runtime
field mappings. The response extension felt local: seven changed/added SDK files,
no caller migration, dependencies, generated registry, library workaround or FX abstraction.

## Reading and diagnostics

Read the SDK README, source modules, Cargo manifest, fixture provenance, workflow,
protocol and independent first-use test source, and shared test helpers. Read Concord's
root README, author guide (query placement, generated names and declaration evolution),
caller guide, quick start, cheat sheet, relevant custom-format/response recipes, crate
READMEs and generated facade rustdoc (`Json` and `Decode`). Consulted the official v2
website and linked OpenAPI, not live rate/resource endpoints. Campaign root instructions
and maintenance method were also read.

The first compile/test attempt passed without a generated-name or compiler surprise.
The first format check requested wrapping in the added workflow test; applied rustfmt.
Two patch applications were rejected for a mismatched prose context and made no changes;
using an exact existing heading resolved the documentation insertion. No Concord friction
or private/native escape was required, so no friction entry was added.

## Verification

Every Cargo command used:

```text
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a
```

- `cargo doc -p concord --no-deps --features csv,jsonl`: passed.
- First `cargo test --locked -p concord-sdk-frankfurter`: passed, 47 integration
  tests (24 unchanged first-use, 18 protocol, 5 workflow), plus 2 doctests.
- Final `cargo test --locked -p concord-sdk-frankfurter`: passed with the same
  47 integration tests and 2 doctests, zero failures.
- `cargo fmt -p concord-sdk-frankfurter -- --check` and `git diff --check`: passed.
- `git diff --exit-code e04236ca -- crates scripts docs checks`: passed; frozen
  library paths remain unchanged. All working-tree edits are inside this SDK.
- New fixture is covered by `text: unset` in `git check-attr text`.
- No live provider calls, credentials or secrets. This is offline compatibility
  evidence, not provider conformance or rate-accuracy verification.
