# Brief for the staged SDK maintenance exercises (DX-01)

Binding source: `contracts/DATA_AND_VALIDATION.md` §6 and §12 in the handoff directory
`/home/canarit/projects/concord_extensions_implementation_handoff/`. These are deliberately staged
SDK evolution tasks, not claims that a provider changed. The person doing an exercise did not
write that SDK; your fresh experience is the evidence.

## The three stages, each its own commit

1. **Freeze the initial consumer.** In `checks/maintenance-<provider>/` (package
   `concord-check-maintenance-<provider>`, stub exists), write a small external consumer of the
   SDK's public exports only: `src/before.rs`, a realistic caller of the operations the exercise
   will touch (written as a downstream application would, using documented public API), with
   `tests/before.rs` running it against `concord-example-testserver` fixtures you write (wire
   assertions on the exact requests). Commit: "maintenance <provider>: frozen initial consumer".
   Record the commit SHA.
2. **Change the SDK.** Make the staged change in `examples/<provider>-sdk/` (and its app/tests if
   they need adapting — record why). Do not touch `src/before.rs` or `tests/before.rs`. The frozen
   consumer must still compile and pass, or the incompatibility is a finding you record with the
   exact compiler error. Commit: "maintenance <provider>: SDK change — <summary>".
3. **Update the consumer.** Add `src/after.rs` + `tests/after.rs` using the new capability (the
   old consumer stays unchanged beside it). Commit: "maintenance <provider>: updated consumer".

The executable before/after check is `cargo test --locked -p concord-check-maintenance-<provider>`
at the final commit (both consumers against the changed SDK) plus the frozen consumer at stage 1.

## What to record (in `checks/maintenance-<provider>/MAINTENANCE.md`)

The task statement; the three commit SHAs; `git diff --stat` and the SDK diff of stage 2
(declaration lines, wrapper lines, test lines); whether old call sites compiled unchanged;
concepts the caller had to learn; documentation you had to read (file names) and whether it
answered the question; compiler errors met (verbatim, with whether they pointed at the right
cause); casts/annotations/adapters needed; native escapes; duplicated context/encoding code;
hidden collection; metadata you could not preserve; wire bytes before/after where relevant
(omitted-field compatibility). Timings only as secondary observations with conditions. Counter
evidence (what was easy) is as important as friction. Do not edit `crates/` (library) code: if a
library change would remove the friction, describe it with before/after caller code.

## Conventions

Same as `SDK_BRIEF.md`: Rust 1.99.1, clippy `-D warnings`, fmt, `--locked`, `CARGO_BUILD_JOBS=6`,
no `scripts/verify.sh`, no push, commits end with
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Dependencies of the maintenance crate:
the SDK (path), `concord`, `concord-pagination` if needed, `serde`/`serde_json`; dev:
`tokio`, `concord-example-testserver`.
