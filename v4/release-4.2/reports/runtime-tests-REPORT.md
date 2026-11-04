# runtime-tests lane report

- Branch: `v42/tests`; initial and current HEAD: `07dcdd03c61a1822341e92d3820e66117f1aef14`.
- Final commit: **NOT CREATED**. `git add ... && git commit ...` exited 128 before staging: `fatal: Unable to create '/home/canarit/projects/concord_api/.git/worktrees/wt-tests/index.lock': Read-only file system`. The environment mounts this worktree's Git administration directory read-only. The new files remain untracked in the worktree. The coordinator needs to stage and commit them on `v42/tests` using an environment with write access to that worktree's Git administration directory; suggested commit message `Add bounded runtime and composition follow-up tests` with `Lane: runtime-tests` in the final line.
- Changed files (all new): `examples/native/tests/metadata_stage.rs`, `crates/concord/tests/lifecycle.rs`, `crates/concord/tests/d16_followup.rs`, and the three `.rs`/`.stderr` pairs under `crates/concord/tests/ui_d16/` (`non_ancestor`, `removed_override`, `unqualified_ancestor`). No manifest, lockfile, production source, existing tests or documentation changes. The transient mutation of `prepared.rs` was restored, and `git diff -- crates/concord-core/src/prepared.rs` is empty.
- All commands below ran in `/home/canarit/projects/concord-v42/wt-tests`, under the checked-in Rust 1.99.1 toolchain, with the campaign lease and a 600-second outer timeout. Raw logs are in this report's directory. Expected successes require both exit 0 and the listed test/assertion witnesses.

## T01: mutation triple (actual commands and output witnesses, verbatim)

Clean, before mutation:

```text
timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord-example-native --test metadata_stage
```

Exit 0; decisive output from `t01-clean.log`:

```text
running 3 tests
test refused_metadata_does_not_wait_for_body_and_is_recoverable ... ok
test status_response_is_recoverable ... ok
test valid_head_and_body_decode_and_invalid_body_is_decode ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

Mutation: temporarily replace exactly `    F::check_content_type(head)?;` with `    // MUTANT M2: F::check_content_type(head)?;` in `crates/concord-core/src/prepared.rs` (the Z05 M2 mutation); an EXIT trap restores the saved original bytes. Expected exit 101 with the Metadata-before-body assertion, not a compiler error or outer command timeout.

```text
timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord-example-native --test metadata_stage refused_metadata_does_not_wait_for_body_and_is_recoverable
```

Actual exit 101; decisive output from `t01-mutant.log`:

```text
running 1 test
test refused_metadata_does_not_wait_for_body_and_is_recoverable ... FAILED

thread 'refused_metadata_does_not_wait_for_body_and_is_recoverable' (399) panicked at examples/native/tests/metadata_stage.rs:41:5:
expected Metadata before body; timed out waiting for withheld body

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 3.01s
```

Restored, after the trap:

```text
timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord-example-native --test metadata_stage
```

Exit 0; decisive output from `t01-restored.log`:

```text
running 3 tests
test refused_metadata_does_not_wait_for_body_and_is_recoverable ... ok
test status_response_is_recoverable ... ok
test valid_head_and_body_decode_and_invalid_body_is_decode ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

The server sends the head and awaits a channel before writing any body. The test receives Metadata within a three-second deadline, recovers `into_response()`, releases the server, and receives the entire body. The positive valid JSON and negative invalid JSON controls return the decoded item and `Decode`, respectively; the non-2xx control returns `Status` and recovers the entire body. Each owned server is joined within a deadline. This establishes unread at Concord's layer, not absence of transport prefetch. After formatting, the same target passed again (exit 0, 3 passed; `t01-final.log`).

## I20-A lifecycle follow-up

```text
timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord --test lifecycle
```

Expected exit 0 and all event-order/cancellation/control assertions reached; actual exit 0: `test stages_cancellation_and_controls ... ok`, `1 passed; 0 failed` (`lifecycle-final.log`). The fixture records `check`, context `construct`, `decode`, `drop`, server request arrival and joined cancellation; it checks `prepare` against a bounded no-arrival interval, cancellation before head and after context creation while the body is withheld, and success, truncated-body Transport, Metadata refusal and Status controls. A server listening after each single send counts accepted connections for a bounded 100 ms after completion/cancellation; the 503 control closes its connection, so a prompt retry would be observable. These are bounded observations, not proof against arbitrary delayed retries, transport task retention, or socket-prefetch claims. All server tasks are signaled and joined. I20-A/B already exercised cancellation variants; this maintains selected claims and adds explicit event order/refusal controls, not a claim that their remaining transport gaps are closed.

## D16-sol follow-up

```text
timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord --test d16_followup
```

Expected exit 0, literal prepared-request comparisons pass, and all three compile-negative fixtures match their checked-in exact stderr; actual exit 0: `test nested_owners_and_sibling_defaults ... ok`, `test tests/ui_d16/non_ancestor.rs ... ok`, `test tests/ui_d16/removed_override.rs ... ok`, `test tests/ui_d16/unqualified_ancestor.rs ... ok`, `test result: ok. 2 passed; 0 failed` (`d16-final.log`). The expected URLs and header sequences are hand-written, covering nested owners, sibling defaults, qualified references, and override/remove/reintroduction/append. Compiler diagnostics respectively identify a non-enclosing scope, override after ancestor removal, and an unqualified ancestor value. Initial stderr generation used `TRYBUILD=overwrite timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord --test d16_followup compiler_rejections` (exit 0, `d16-bless.log`); the generated messages were inspected against these intended errors, then the unmodified trybuild comparison above passed.

Test-strength control: temporarily replace the expected `k=inner&k=leaf&root=root` with `k=WRONG&k=leaf&root=root`, restore via EXIT trap, then run:

```text
timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord --test d16_followup nested_owners_and_sibling_defaults
```

Expected assertion failure, actual exit 101 (`d16-wrong.log`), with `assertion left == right failed`, actual URL containing `k=inner&k=leaf&root=root` and wrong expected URL containing `k=WRONG&k=leaf&root=root`. The restored test passed in `d16-final.log`. This qualifies the URL comparison against one deliberately wrong expected value; it is not a production resolver mutation or a broad oracle qualification.

## Required combined verification

- `timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord-example-native`: expected exit 0, actual 0 (`native-suite.log`); test targets report 4 + 4 + 3 + 5 + 10 + 4 passing tests, no failures; doc tests 0.
- `timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo test --locked -p concord --test lifecycle --test d16_followup`: expected exit 0, actual 0 (`combined.log`): D16 2 passed including all three trybuild cases; lifecycle 1 passed.
- `timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh cargo clippy --locked -p concord -p concord-example-native --all-targets -- -D warnings`: expected exit 0, actual 0 (`clippy.log`): `Finished dev profile`.

All new integration test targets are automatically discovered by Cargo; the UI fixtures are explicitly driven by `d16_followup.rs`, so no additional wiring is needed for the requested default-feature workspace gate. No dependency change is needed. `scripts/verify.sh`, native Windows, and transport-internal task/FD census were not run; the central gate belongs to the coordinator and no native Windows runner was requested in this lane. The inability to create the required commit is the sole handoff blocker. Open questions: none.
