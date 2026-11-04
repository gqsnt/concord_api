# O02 — response metadata materialization

**Decision: DEFER_WITH_EVIDENCE.** The selective change preserves the observed outcomes in this bounded experiment and saves work on parameter-heavy synthetic metadata. The frequency and value of that case in a representative workload are unknown; the unchanged ordinary cases do not establish a general response speedup. Adoption also requires the coordinator's independent review, which this lane was explicitly instructed not to delegate. Next trigger: a representative workload or profile showing that non-charset response parameters materially contribute to decoding cost, followed by a repeat with registered equivalent decoder benchmarks and independent semantic review. This is neither an adoption nor a finding of a defect.

**Predeclared criteria:** [PLAN.md](PLAN.md) was written before timing: no behavioral differences, including diagnostics and ownership; meaningful repeatable improvement in parameter-heavy head and some plausible head+body workload; no reproducible >5% regression for bare or single-charset ordinary cases; independent review for adoption. Measured twelve-parameter small-body calls improved in this fixture and ordinary-case medians did not show a reproducible >5% regression, but a single synthetic body and an unverified header distribution do not settle the workload criterion. There was no independent review here.

**Source identity and files:** worktree `/home/canarit/projects/concord-v42/wt-o02`, branch `v42/o02-metadata`; HEAD and final commit remain `4874405bffa9f70716c4d5cb208852edec0fd05d`. No commit was made: the brief states this session's Git directory is read-only. The sole candidate worktree file is `crates/concord-extras/src/media.rs`, SHA-256 `7a84c4bf7530de93606b12de7108cd9572c1f643c6142537f8029bddad10fbb9`; [candidate.patch](candidate.patch) SHA-256 `d52bed8d5b4e77ca7c7fbe9d222eae5764f4202c4bf7a36e8e9c6c583529763a`. It keeps mime, scanning, ordering and charset value representations; it skips constructing non-charset strings but still reads and validates all their bytes. Experimental tests and measurement code are saved under [evidence/fixtures](evidence/fixtures), not left in the worktree. No dependency, lockfile, public API, or other tracked file changed. Cargo.lock SHA-256 `d490302056098b035d5769c63e27ce10549e48f0f5f8dbf49d5457a71a8971ce` throughout. No rejected alternative patch: only this candidate and a temporary intentionally incorrect negative-control mutant were built. Runtime model and effort were not observable to this worker.

## Semantic comparison and controls

All commands below ran from `/home/canarit/projects/concord-v42/wt-o02`; timeouts wrap the campaign lease. Test fixture sources were present in `crates/concord-extras/tests/` at execution and then copied intact to `evidence/fixtures/` and removed from the worktree. Reproduce by copying those files and `o02_support/` back to that directory. Baseline and candidate are different builds of the actual production crate, not two model implementations. Raw stdout including every observed result is in `evidence/{baseline,candidate}-{none,json,form,both}.log`.

| Acceptance / command | Expected | Actual and decisive witness |
|---|---|---|
| Before timings: `cargo +1.99.1 test --locked -p concord-extras --no-default-features [--features json\|form\|json,form] --test o02_differential -- --nocapture` for baseline then candidate, each prefixed `timeout 900 /home/canarit/projects/concord-v42/tools/heavy.sh` | All exit 0 and complete outcome rows identical per feature set | Each of eight exits 0, `test result: ok. 1 passed`; `python3 .../evidence/compare.py` exits 0 and reports `none: 9916`, `json: 19832`, `form: 19832`, `both: 29748 observations identical`. The 2,479 field combinations per set include absent, repeated equal/different fields, unreadable obs-text, 11 valid/invalid essences and 15×15 parameter combinations. |
| Outcome coverage in that command | Compare check result, static label, ordered numeric context, unredacted source text/presence, standard source boundary, Display/Debug, format Context and payload or body error | The rows capture all of these for Text, feature-enabled Json/Form, and each corresponding `IgnoreContentType`, on valid and invalid bodies. The concrete dynamic type of an attached source was not separately downcast; for standard formats the comparison includes its text and both source-access boundaries. This is bounded differential evidence, not an exhaustive grammar proof. |
| Full response / retention, baseline and candidate: `timeout 900 .../tools/heavy.sh cargo +1.99.1 test --locked -p concord --test metadata` | Exit 0, positive decoded payload and negative metadata refusal retain unread body | Both exit 0: `test result: ok. 10 passed`; `the_head_is_refused_before_the_body_exists` verifies refusal before server releases the body, the exact codec label and the complete recovered `{"id":7}` body; `charset_parameters_are_read_exactly` verifies quoted-charset success and conflicting duplicate refusal. Logs: `baseline-retention.log`, `candidate-retention.log`. |
| Candidate existing policy suite: `timeout 900 .../tools/heavy.sh cargo +1.99.1 test --locked -p concord-extras --no-default-features [--features json,form] --test media` | Exit 0 for both configurations | Exit 0; respectively `3 passed` and `13 passed`, in `candidate-media-none.log` and `candidate-media-both.log`. |
| Negative control: insert `return Some(found)` immediately after retaining a charset, then run `timeout 900 .../tools/heavy.sh cargo +1.99.1 test --locked -p concord-extras --no-default-features --features json --test o02_differential differential_corpus` | Nonzero because a foreign charset followed by malformed tail must be accepted by lenient Text, whose complete scan fails | Exit 101, assertion at `o02_differential.rs:62` that Text accepts the malformed tail failed. This is the intended witness, not a compile error. `early-exit-mutant.log`; the temporary edit was restored from `evidence/candidate-media.rs`. |
| Restored candidate: same corpus test with `--features json,form` | Exit 0 and positive assertions hold again | Exit 0, `test result: ok. 1 passed` in `restored-candidate.log`; source SHA equals the measured candidate. |
| `git diff --check` | Exit 0 | Exit 0. |

The corpus compares actual head check followed by context and body decode; the localhost test separately exercises the production `Decoder` path, including retention. The two focused assertions distinguish a well-formed foreign charset from the same charset with a malformed tail and a duplicate charset from a duplicate with malformed tail. They are not a proof of all possible malformed inputs.

## Measurements

Built baseline and candidate separately, before timing, with `RUSTFLAGS='--cap-lints allow' timeout 900 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1 test --locked --release -p concord-extras --no-default-features --features json --test o02_timing --test o02_alloc --no-run`. Both exit 0. The cap was needed because the workspace forbids unsafe code, while the experimental allocator implements `GlobalAlloc`; the first uncapped build failed with `-F unsafe-code` and is preserved as `baseline-build.log`. The same cap and flags were used on both versions; neither timing executable installs that allocator. Executable snapshots and build logs are retained in `evidence/`.

After semantic parity: `timeout 300 /home/canarit/projects/concord-v42/tools/heavy.sh --exclusive /home/canarit/projects/concord-v42/lanes/o02-metadata/evidence/run-measurements.sh` from the worktree exited 0. It first runs allocation executables and then uninstrumented timing executables in baseline/candidate/candidate/baseline order. Every run exits 0 and reports `test result: ok. 1 passed`. Allocation counting and timing are separate programs; neither counts allocations during timed sampling. Each timing case warms up 1,000 calls, then takes seven raw elapsed-time samples of 200,000 calls (small) or 30,000 calls (10 KiB). Header maps and JSON byte bodies are built before sampling; the full call checks the head, makes Context, clones a Bytes handle, decodes the JSON string, and drops the result within the measured iteration. Head-only drops the check result within the iteration. The unchanged Binary control performs the same Context/body path without the scanner. This measures in-memory codec calls, not networking, response construction, or the complete async `Decoder` machinery. The JSON body is a synthetic repeated-character string, not a representative structured document.

Raw results, environment snapshots and reproduction script are in [evidence/](evidence/); [comparison.md](evidence/comparison.md) contains all four medians for all cases and [compare.py](evidence/compare.py) verifies semantic row equality and derives the medians. Selected median ns/call (seven samples per run):

| Case | Baseline 1 | Candidate 1 | Candidate 2 | Baseline 2 |
|---|---:|---:|---:|---:|
| bare head | 65.02 | 62.97 | 63.46 | 67.23 |
| charset head | 108.18 | 109.48 | 106.36 | 110.51 |
| twelve parameters head | 310.56 | 223.02 | 214.31 | 312.22 |
| bare head+decode 16 B | 91.06 | 91.50 | 91.64 | 92.97 |
| charset head+decode 16 B | 141.35 | 137.81 | 135.35 | 141.08 |
| twelve parameters head+decode 16 B | 346.47 | 250.88 | 242.50 | 345.59 |
| twelve parameters head+decode 10 KiB | 1236.88 | 1194.03 | 1199.16 | 1233.85 |
| unchanged Binary control, 16 B | 14.85–14.94 | 14.88–14.98 | 14.86–14.97 | 14.79–14.88 |

Allocation calls / requested bytes per operation (1,000 operations, after setup and warm-up; output is dropped per call):

| Header and stage | Baseline | Candidate |
|---|---:|---:|
| bare head | 1 / 16 | 1 / 16 |
| charset head | 3 / 117 | 3 / 117 |
| twelve parameters head | 13 / 71 | 1 / 16 |
| bare full, 16 B | 2 / 30 | 2 / 30 |
| charset full, 16 B | 4 / 131 | 4 / 131 |
| twelve parameters full, 16 B | 14 / 85 | 2 / 30 |
| twelve parameters full, 10 KiB | 14 / 10309 | 2 / 10254 |
| Binary control, 16 B | 0 / 0 | 0 / 0 |

These are requested bytes, not net live memory. `baseline-alloc.log` and `candidate-alloc.log` contain all rows, including the 10 KiB ordinary cases and controls.

Host: Linux x86_64, Intel i5-13600K, 20 logical CPUs, `powersave` governor; rustc 1.99.1 (58cf8f9c3 2026-10-13). During the exclusive lease (2026-11-04 04:48:51–04:49:00 UTC), load average was about 7.84–8.73 at one minute; CPU pressure `some avg10` fell from 3.36% to 1.51%. The lease excludes other campaign builds but does not certify an idle machine; no CPU affinity or frequency controls were applied. Four short runs and unchanged controls do not provide a confidence interval or control all host noise. The timing harness is experiment-local rather than registered in the existing Criterion registry, so its benchmark equivalence test did not certify these cases. The two large-body runs show small differences, below the predeclared 5% noise threshold; no claim is made for them. No Form timing, end-to-end network timing, real workload distribution, independent review, exhaustive grammar proof, or Windows run was performed.

## Handoff / open questions

- Coordinator: obtain the required independent review if a representative profile justifies revisiting this patch; register equivalent head and complete-decoder workload cases through the Criterion registry before relying on an adoption decision. The coordinator owns any integration and final decision.
- Open question: do real responses in the target workload commonly carry enough unrelated parameters for the measured small-body savings to matter? No production workload data was available in this lane.
- No `scripts/verify.sh`, replay consumer, new dependency, lockfile update, push, merge, publication, or commit was attempted. No shared-file change is required for this deferred result.
