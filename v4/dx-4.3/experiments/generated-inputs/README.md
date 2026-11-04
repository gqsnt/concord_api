# Experiment G1: generated input records

The prototype behind decision G1 in [`../../DECISIONS.md`](../../DECISIONS.md). It lives on branch
`concord-v4.3-dx-generated-inputs` only. On the supported branch this directory holds the records
(this file, `EVALUATION.md`, `evidence/`) and `generated-inputs.patch`, the prototype's two
commits as a patch series (`git am` onto `0a91ce5`, the ADR commit), not the macro change itself.

## What the macro change does

`concord-macros` (`emit.rs`, `model.rs`) emits, for every endpoint, a `#[non_exhaustive]`
`{Endpoint}Input` with the endpoint's own values and body as public fields and
`new(required..)`; the request type holds `__parent` plus that input and gains
`input()`/`input_mut()`/`into_input()`; the parent handle gains `{endpoint}_with(input)`.
Everything else (navigation, setters, `prepare`/`send`, `Debug` text, wire output, diagnostics) is
unchanged.

## How to rebuild and run each check

Check out `concord-v4.3-dx-generated-inputs` (or apply the patch onto `0a91ce5`), then from the
repository root:

| Check | Command |
|---|---|
| Existing workspace, unchanged | `cargo test --workspace --locked` |
| Prototype behaviour (I01–I08) | `cargo test --locked -p concord --test inputs` (also `--no-default-features`) |
| New collision diagnostics (C03) | `cargo test --locked -p concord --test ui` (case input_name_collisions) |
| Matched caller tasks (DX01, DX02, T3) | `cd v4/dx-4.3/experiments/generated-inputs/tasks && cargo test --features candidate` |
| Baseline tasks alone (any Concord) | `cd .../tasks && cargo test` |
| Downstream evolution (C02, DX03) | `v4/dx-4.3/experiments/generated-inputs/evolution/run.sh` |
| Editor observations (DX04) | `python3 -I v4/dx-4.3/experiments/generated-inputs/ra_probe.py` |
| Generated size (C05) | `cargo +nightly rustc --locked -p concord --test synthetic_sdk -- -Zunpretty=expanded \| wc -l` on both branches |

`tasks/` and `evolution/` are standalone Cargo workspaces (not members of the repository's
workspace, never packaged); `tasks` has an experiment-local `candidate` feature only so that the
baseline solutions also build against an unmodified Concord.

Recorded outputs are in `evidence/`; the evaluation and decision are in `EVALUATION.md`.
