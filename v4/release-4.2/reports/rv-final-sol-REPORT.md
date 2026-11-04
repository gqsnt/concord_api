# Final executable release review — rv-final-sol

**Verdict: REVISE** for frozen candidate `9c3229458bafb3f3441d5b76bb176961b2a118f9`.
Two reproduced correctness gaps remain: forwarded struct-path initializers regress from 4.1,
and the workflow guard accepts a gate disabled by Bash `noexec`. Neither passing maintained
tests nor the central gate covers these cases. Optional changelog polish is separate below.

Reviewed the complete `07dcdd03c61a1822341e92d3820e66117f1aef14..9c3229458bafb3f3441d5b76bb176961b2a118f9`
delta: 133 changed files, including compiler, tooling, benchmarks, examples, docs, packages and
release records. The independent reviewer authored none of those changes and spawned no agents.
The observed CLI startup banner reports **gpt-6.1-sol, xhigh**; see
[observed-runtime.txt](observed-runtime.txt). Review execution: 2026-11-04 UTC, Linux
`x86_64-unknown-linux-gnu`, rustc 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), Cargo 1.99.1.

## Blocking correctness findings

### R1 — P2: qualified path fragments become invalid struct initializers

Current source: `crates/concord-macros/src/syntax.rs::parse_initializer` (1260) and
`regroup_in` (1351–1361). The new initializer path discards the parsed expression and emits
regrouped written tokens. In expression context it parenthesizes every multi-token invisible
group. A forwarded path used as the head of a struct literal is such a group, but must remain a
path: `$p { n: 7 }` becomes effectively `(crate::Payload) { n: 7 }`.

Minimal trigger, with a Clone + Display `Payload { n: u64 }`:

```rust
macro_rules! declare {
    ($p:path) => {
        concord::api! {
            pub client Api {
                base "http://example.invalid";
                GET x(query p: Payload = $p { n: 7 });
            }
        }
    }
}
declare!(crate::Payload);
```

Full runnable fixture: [forward-path/candidate/src/main.rs](forward-path/candidate/src/main.rs).
Reproduction from `forward-path/candidate`:

```sh
RUSTUP_TOOLCHAIN=1.99.1 CARGO_NET_OFFLINE=true \
CARGO_TARGET_DIR=/home/canarit/projects/concord-v42/lanes/rv-final-sol/target-replay-candidate \
timeout 600 /home/canarit/projects/concord-v42/tools/heavy.sh \
cargo +1.99.1 run --offline --locked --bin review-forward-path
```

Expected: exit 0 and query `p=7`. Actual: exit 101, `struct literal body without path`, followed
by E0423/E0061; see [candidate log](logs/forward-path-candidate-locked.log). The same fixture on
the historical source exits 0 and prints `FORWARDED_STRUCT_LITERAL_PASSED`
([baseline log](logs/forward-path-baseline-locked.log)). Directly written `crate::Payload { n: 7 }`
passes on both sources ([candidate control](logs/forward-path-candidate-qualified-direct.log)).
The final comparison preserves the release lock's registry versions/checksums on both arms.

Impact: previously valid public DSL declarations emitted by ordinary `macro_rules!` wrappers
stop compiling. This is a compatibility regression introduced by the widened F03 correction,
not one of the documented intentional 6-versus-5 expression changes.

Strongest counterargument: this requires a forwarded qualified path; writing the path directly
or forwarding the whole initializer expression avoids it. That limits frequency but does not
make the valid 4.1 declaration unsupported.

Requested regression: retain this downstream runtime case and direct-path control in
`forwarded_types.rs`; cover struct paths in endpoint and owner defaults. Preserve the parsed
expression's syntactic roles when regrouping, while retaining the existing expression-precedence,
macro-opacity and type-position controls. Execute the widened compiler suite after repair.

### R2 — P2: `set -n` can satisfy M03 without executing the gate

Current source: `scripts/check-ci.py::disables_errexit` (591) and `Guard.job` (866–871).
The checker rejects `set +e` but accepts Bash's literal `noexec` option in the gate step.

Reproduction fixtures: [noexec.yml](noexec.yml), [noexec-option.yml](noexec-option.yml).
Both retain the Linux/Windows matrix and nightly lane from the maintained test workflow, with
the blocking gate changed to:

```yaml
run: |
  set -n
  scripts/verify.sh --package --bench
```

Run from the candidate root:

```sh
timeout 20 python3 -I scripts/check-ci.py --workflow-file \
  /home/canarit/projects/concord-v42/lanes/rv-final-sol/noexec.yml
```

Expected: reject this ineffective gate with exit 1. Actual: exit 0 and
`effective full gate on: Linux, Windows`; [log](logs/noexec-workflow-regression.log).
`set -o noexec` has the same result ([log](logs/noexec-option-workflow-regression.log)).
I executed both shell forms with GitHub's Bash options (`--noprofile --norc -eo pipefail`),
substituting an observable printf for the gate: both exit 0 with empty stdout. Removing noexec
prints the witness. See [shell receipt](logs/noexec-shell.json),
[long-option receipt](logs/noexec-option-shell.json) and
[execution control](logs/bash-execution-control.log).

Impact: the advertised executable inventory guard still certifies a statically disabled gate.
This does **not** invalidate the actual central run or historical CI; their workflow does not
contain this mutation.

Strongest counterargument: static inspection is deliberately bounded, and cannot evaluate all
shell state. This is a literal builtin in the very step being inspected, analogous to the already
rejected `set +e` and maintained `bash -n` negative. Rejecting unsupported option changes or
recognizing noexec requires no general shell interpreter.

Requested regression: add negative workflow cases for `set -n`, `set -o noexec` and combined
short options containing `n`, with the intended rejection witness; retain executable positive
and multiline/quoting controls. Reject unsupported shell-option changes conservatively.

## Independently executed release cases

Every heavy command used `/home/canarit/projects/concord-v42/tools/heavy.sh` with a finite
600-second timeout. Rust selection was explicit. Individual comparisons used `--offline --locked`.
The package scripts perform initial offline fixture-lock adaptation internally; I additionally
reran all twelve isolated/package fixture builds and both package-check feature variants with
`--locked`. Registry inputs stayed within each fixture's original locked graph.

The exact argv, cwd, timestamps, target directory, expected/actual exit, output witnesses and log
SHA-256 are in [receipts.jsonl](receipts.jsonl). Drivers are
[run_review.py](run_review.py) and [replay_review.py](replay_review.py).

| Case | Executed command / selected scope | Result |
|---|---|---|
| F01/F02 cross-crate | `cargo +1.99.1 test --offline --locked -p concord-clone-hygiene-check` | 4 hygiene tests, including all 4 trybuild negatives, and 1 localhost type-macro decoding test passed |
| F02–F04, lifecycle, D16, positives | `cargo +1.99.1 test --offline --locked -p concord --test output_types --test applied_advice --test forwarded_types --test lifecycle --test d16_followup --test accept --test bodies --test composition --test values` | 73 tests passed, including 3 D16 trybuild cases |
| UI | `cargo +1.99.1 test --offline --locked -p concord --test ui` | 2 harness tests, 83 UI cases passed; migration advice and output macro diagnostics included |
| Compiler units/rustdoc text | `cargo +1.99.1 test --offline --locked -p concord-macros` | 17 tests passed |
| M01 | `cargo +1.99.1 test --offline --locked -p concord-bench` | 10 tests passed: 92 registered cases, 45 groups, 47 comparisons, comparator negatives and positives |
| M02 | `cargo +1.99.1 test --offline --locked -p concord-example-renamed` alone | 3 tests passed |
| T01 + native positives | `cargo +1.99.1 test --offline --locked -p concord-example-native` | 30 tests passed, including 3 metadata-stage tests |
| M03/M04 | `python3 -I -B -m unittest discover -s scripts/tests -p 'test_*.py' -v` | 71 maintained tests passed; R2 is an additional missing negative |
| Fresh-output controls | `bash scripts/tests/output-dirs.sh` | 14 controls passed |
| Feature isolation | `bash scripts/consumers.sh` | All 6 separate workspaces passed: 18 tests + 1 doctest; no-extras graph excludes extras/json/form |
| Fresh packages | `bash scripts/package-check.sh <lane>/packages` | Fresh 4.2.0 archives/extractions; 2 JSON-enabled tests and 1 no-JSON test passed |
| Packaged isolation | `bash scripts/consumers.sh --packaged <lane>/packaged-consumers` | All 6 packaged workspaces passed: 18 tests + 1 doctest |

Logs are under [logs/](logs/). No maintained failure was hidden: an initial reviewer command
selected a nonexistent test target `defaults`, executing no tests. It was corrected to `values`
and the whole selected command passed (`compiler-runtime-corrected-selection.log`). The first
exploratory R1 lock generation selected newer registry inputs; those runs are excluded from the
finding's final evidence. I restored the release lock, adapted only the standalone root, checked
its dependency closure, and repeated both arms and controls with `--locked`.

## Historical replay and repaired expectations

Re-executed all **14 F01–F04 recipes** from the immutable `replay-v41/cases.json`, including
its duplicate F02 control. Every historical exit and required output witness matched. In
particular F01 still fails E0277, F02 rejects the parenthesized output macro, F03 emits the
four-byte bad advice and its application fails, and F04 emits the non-raw name and its
application fails. Expected 101 here reproduces defects; it is not repaired-contract success.

Copied consumers 1–3 to [repaired-replay/](repaired-replay/), redirected their Concord dependency
to the exact candidate, and changed only the four local package versions in locks to 4.2.0.
All **18 candidate recipes** matched repaired expectations: the 14 historical selections,
an actual same-forward run, all three F02 delimiters together, and two regenerated-advice tests.
F03/F04 retain their historical bad applied files as negative controls. The new files apply
snippets extracted from the candidate diagnostics and verify `Payload<6>`/body bytes and the
raw-name setter's body bytes. No unfiltered test command ran in a historical/repaired replay
consumer containing deliberate failing binaries.

The original portable manifest still matches **all 1,769 files**
([historical-integrity.json](historical-integrity.json)). Candidate consumer registry closures
are identical to their historical counterparts; consumer 1's historical ICU versions differ
from the workspace lock and were deliberately preserved. Inputs and local path dependencies
are hashed in [replay-inputs.json](replay-inputs.json), with step results in
[replay-summary.jsonl](replay-summary.jsonl).

## Source, packages, cache and broad gate

The reviewed snapshot remained clean at the requested detached HEAD. All reviewer fixtures and
reports are outside it. [source-receipt.json](source-receipt.json) records the compiler, HEAD,
workspace lock, complete-diff hash and the 1,831-file tracked-source manifest;
[candidate-files.sha256.json](candidate-files.sha256.json) includes all four production crates
and the test/helper/benchmark inputs. [dependency-closure.json](dependency-closure.json) binds
external consumer locks to their appropriate reference graph, including the separately locked
gzip consumer. No registry drift remains in decisive comparisons.

Review-owned target roots were `target-candidate`, `target-replay-baseline` and
`target-replay-candidate` under this lane; the package scripts also created fresh internal target
directories. Cargo build witnesses identify the actual candidate/baseline paths. Package-check
and packaged-consumers extracted four new **4.2.0** packages and byte-compared `src/` plus original
manifests to this snapshot. The generated consumers point to these extractions, including the
deep Unicode path, and patch internal dependencies to extracted packages. Package-check's
fresh build explicitly confirms Concord compiled from that extraction, not checkout source.

All four production versions are inherited as 4.2.0; the facade's three internal requirements
and extras-to-core are exact `=4.2.0`. Both tracked locks have the coordinated version change,
with no registry graph change in the release diff. Migration notes document the intended
forwarded-expression semantic correction. The changelog has the separate editorial defect below.

The completed `scripts/verify.sh --package --bench` result was **inspected from the central log**,
not independently executed as a full gate: 37 PASS steps, exit 0, HEAD `9c32294`, initial dirty
count 0, rustc 1.99.1. Its workspace result is 676 passed; all listed feature lanes, clippy,
formatting, graph, records, generation checks, isolated examples, docs, bench compilation and
packaging passed. The inspected bytes are retained in
[central-gate-inspected.log](logs/central-gate-inspected.log); their SHA-256 is in the source receipt.

The full diff adds no header-planner or media-scanner runtime optimization: O01/O02 remain
`DEFER_WITH_EVIDENCE`, with patches retained only as records. I inspected those dispositions and
their source boundaries; I did not independently repeat performance measurements or certify the
experimental differential claims. Existing UI snapshots were not weakened or rewritten by this
release; new snapshots add the relevant negatives. The benchmark test replacement connects the
actual registered closures to stronger stage comparisons rather than omitting registered pairs.
The compiler's new regrouping implementation is the substantive scope extension implicated in R1.

## Optional polish

**R3 — P3, `CHANGELOG.md:71`: restore the `## 4.1.0` heading.** The diff replaces the old heading
with the new 4.2 section but never reintroduces it before the retained 4.1 prose. Reproduce by
reading lines 69–86: the current 4.2 section now includes the old instruction to require
`=4.1.0`, and historical crate/DSL changes appear to belong to 4.2. Expected: separate versioned
entries; actual: one merged entry. Impact is release-note ambiguity, with no runtime consequence.
Strongest counterargument: all historical text is preserved and the top 4.2 paragraph correctly
states `=4.2.0`. Requested check: restore the heading and review the rendered version boundaries;
the migration guide's old “first two sections” reference can also be made version-specific.

## Unavailable environments and acceptance boundary

I cannot run native Windows here and did not do so. The frozen status record still reserves its
final native-platform receipt for the coordinator; the supplied central gate is Linux evidence.
The coordinator's separately recorded, source-bound Windows result must be assessed before
native Linux-and-Windows release acceptance. Linux deep/Unicode package builds are not Windows
execution. I did not perform visual rustdoc QA, timing qualification or dedicated security work.

Requested next scope: repair R1 and R2 with the stated regressions, execute their neighboring
compiler/tooling controls, rerun the affected release checks on the resulting source, and attach
the corresponding native Windows receipt. This REVISE verdict concerns the identified release
delta and bounded executions; it is not a universal correctness or security certification.
